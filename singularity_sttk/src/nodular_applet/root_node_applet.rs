use crate::{
    help::HelpApplet,
    nodular_applet::{
        AppletSpawner, NodeRequest, NodularAppletInitializer,
        node_holder::{NodeHolder, client_spawners},
        node_tree::{NodeId, NodeTree},
        treeview::{TreeviewAction, TreeviewPointer, TreeviewRow, inner_layer_rows, treeview_rows},
    },
    session,
    standard_keybinds::standard_keybind_request,
};
use calloop::{
    EventLoop, LoopHandle,
    channel::{Channel, Sender},
    timer::{TimeoutAction, Timer},
};
use singularity_common::{
    sap::{
        packets::{SpawnPosition, StandardEvent, StandardRequest, StateKind, Treeview},
        raw_client_initializer::RawClientInitializer,
        session::Session,
        shared_state::{SharedState, StatePublisher},
    },
    settings::{self, Settings, TreeviewPosition},
    utils::tree::{
        tree_node_path::TreeNodePath,
        world_tree::{WorldTree, WorldTreePath},
    },
};
use sonamu_ui::{
    display_units::DisplayContainerSize,
    ui_element::{Length, Style, UIElement, UITree},
    ui_event::{KeyModifiers, KeySymbol, MouseEvent, UIEvent},
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, RwLock},
};

/// Holds the tree of nodes, is held by a Basic Applet runner (applet runner).
///
/// Every applet in the tree is a client of this, on its own thread.
pub struct RootNodeApplet {
    holder: NodeHolder,
    surface: StatePublisher<UITree>,
    request_sender: Sender<StandardRequest>,

    latest_size: DisplayContainerSize,

    treeview_pointer: TreeviewPointer,
    /// The last treeview drop and the node it was on.
    /// A drop can come before its drag's pluck finishes, which can shift the paths.
    last_drop: Option<(WorldTreePath, NodeId)>,
    /// The help Super+? opened, to close it on the next one
    help: Option<OpenHelp>,
    /// Pinned in the settings, or traversing (see `update_treeview_visibility`)
    treeview_shown: bool,
    /// The node whose inner tree the treeview shows (right click › Set as view layer, Alt+R);
    /// `None` for the outermost tree. Moves out with focus (see `update_view_layer`).
    view_layer: Option<WorldTreePath>,
    /// The applets' area's left edge and width, as last sent to them
    applet_area: [u32; 2],
    /// Where the session is saved, if anywhere
    session_path: Option<PathBuf>,
    /// The last one written, so unchanged ones aren't written again
    saved_session: Option<Session>,
}

/// A help node, and where it was opened
struct OpenHelp {
    node: NodeId,
    parent: NodeId,
    /// Focus was inside the parent's applet, so it goes back there when the help closes
    parent_inner: bool,
}
impl RootNodeApplet {
    /// Of the width, beside the panes
    const TREEVIEW_PERCENT: u32 = 20;
    /// Of the width and height, floating
    const FLOATING_PERCENT: [u32; 2] = [40, 70];
    /// How often the session is saved (if it changed), besides before quitting
    const SAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

    /// Where the treeview is drawn (`[x, y, width, height]`, frame included), and how;
    /// `None` while hidden
    fn treeview_area(&self, settings: &Settings) -> Option<([u32; 4], TreeviewPosition)> {
        if !self.treeview_shown {
            return None;
        }
        let DisplayContainerSize { width, height } = self.latest_size;
        let position = settings.treeview.position;
        let column = width * Self::TREEVIEW_PERCENT / 100;
        let area = match position {
            TreeviewPosition::Left => [0, 0, column, height],
            TreeviewPosition::Right => [width - column, 0, column, height],
            TreeviewPosition::Floating => {
                let [w, h] = [
                    width * Self::FLOATING_PERCENT[0] / 100,
                    height * Self::FLOATING_PERCENT[1] / 100,
                ];
                [(width - w) / 2, (height - h) / 2, w, h]
            }
        };
        Some((area, position))
    }

    /// The applets' area's left edge and width: beside a treeview column, else everything
    fn applet_area(&self, settings: &Settings) -> [u32; 2] {
        match self.treeview_area(settings) {
            Some(([x, _, width, _], TreeviewPosition::Left)) => {
                [x + width, self.latest_size.width - width]
            }
            Some(([_, _, width, _], TreeviewPosition::Right)) => {
                [0, self.latest_size.width - width]
            }
            Some((_, TreeviewPosition::Floating)) | None => [0, self.latest_size.width],
        }
    }

    fn inner_applet_size(&self) -> DisplayContainerSize {
        DisplayContainerSize::new(
            self.applet_area(&settings::get())[1],
            self.latest_size.height,
        )
    }

    /// Shows the treeview if pinned or traversing, else hides it, resizing the applets if their area changed
    /// (also when the treeview settings did)
    fn update_treeview_visibility(&mut self) {
        let settings = settings::get();
        let shown = settings.treeview.is_pinned()
            || self.holder.tree.is_traversing()
            // eg a drag or menu that started there
            || self.treeview_pointer.is_pressed();
        if shown != self.treeview_shown {
            self.treeview_shown = shown;
            self.surface.damage();
        }
        let applet_area = self.applet_area(&settings);
        if applet_area != self.applet_area {
            self.applet_area = applet_area;
            self.surface.damage();
            self.holder
                .tree
                .handle_event(StandardEvent::UIEvent(UIEvent::WindowResized(
                    self.inner_applet_size(),
                )));
        }
    }

    /// Publishes the window if damaged and the runner read the last one
    fn flush_surface(&mut self) {
        let settings = settings::get();
        let treeview_area = self.treeview_area(&settings);
        let applet_area = self.applet_area(&settings);
        let height = self.latest_size.height;
        let Self {
            holder,
            treeview_pointer,
            view_layer,
            ..
        } = self;
        self.surface.flush(
            || {
                let [left, width] = applet_area.map(|length| length as f32);
                let tree = UITree::new_container_styled(
                    Style::absolute([left, 0.0], [width, height as f32]),
                    vec![holder.tree.get_window()],
                );
                let mut window = vec![tree];
                if let Some(area) = treeview_area {
                    window.push(Self::get_treeview(
                        &holder.tree,
                        view_layer.as_ref(),
                        treeview_pointer,
                        area,
                    ));
                }
                UITree::new_container(window)
            },
            &self.request_sender,
        );
    }

    fn send_request(&self, request: StandardRequest) {
        self.request_sender.send(request).unwrap();
    }

    /// Writes the session if it changed since the last write
    fn save_session(&mut self) {
        let Some(path) = &self.session_path else {
            return;
        };
        let session = Session {
            tree: self.holder.tree.save(),
            focus: self.holder.tree.get_focus_path().0.into_vec(),
        };
        if self.saved_session.as_ref() == Some(&session) {
            return;
        }
        match session::write(path, &session) {
            Ok(()) => self.saved_session = Some(session),
            Err(err) => log::warn!("Couldn't save the session to {}: {err}", path.display()),
        }
    }

    /// Saves first, since the runner may end the process right after
    fn quit(&mut self) {
        self.save_session();
        self.send_request(StandardRequest::Quit);
    }

    /// The saved session if there is one, else `root_initializer` at the root
    #[must_use]
    fn new(
        root_initializer: NodularAppletInitializer,
        applet_spawner_registry: BTreeMap<String, AppletSpawner>,
        session_path: Option<PathBuf>,
        surface: SharedState<UITree>,
        request_sender: Sender<StandardRequest>,
        event_loop: &LoopHandle<'static, Self>,
    ) -> Self {
        let applet_spawners = Arc::new(RwLock::new(applet_spawner_registry));
        let (spawn_client, attach_client) = client_spawners(
            event_loop,
            Arc::clone(&applet_spawners),
            |root: &mut Self, id, request| {
                root.handle_node_request(id, request);
            },
        );
        let saved_session = session_path.as_deref().and_then(session::load);
        let tree = match &saved_session {
            Some(saved) => {
                let mut tree = NodeTree::restore(saved.tree.clone(), spawn_client, attach_client);
                tree.focus_path(&WorldTreePath(saved.focus.clone().into_boxed_slice()));
                tree
            }
            None => NodeTree::new(root_initializer, spawn_client, attach_client),
        };
        let send = {
            let request_sender = request_sender.clone();
            // the runner may already be gone while shutting down
            Box::new(move |request| {
                let _ = request_sender.send(request);
            })
        };

        Self {
            holder: NodeHolder::new(tree, send, false, applet_spawners),
            surface: StatePublisher::new(surface),
            request_sender,
            // Just making stuff up
            latest_size: DisplayContainerSize::new(800, 600),
            treeview_pointer: TreeviewPointer::default(),
            last_drop: None,
            help: None,
            // the root node starts focused
            treeview_shown: true,
            view_layer: None,
            applet_area: [0, 800],
            session_path,
            saved_session,
        }
    }

    /// Restores the session at `session_path` if there is one (else starts with `root_initializer` at the root),
    /// and keeps it saved there
    pub fn get_initializer(
        root_initializer: NodularAppletInitializer,
        applet_spawner_registry: BTreeMap<String, AppletSpawner>,
        session_path: Option<PathBuf>,
    ) -> Box<dyn RawClientInitializer> {
        struct Initializer {
            root_initializer: NodularAppletInitializer,
            applet_spawner_registry: BTreeMap<String, AppletSpawner>,
            session_path: Option<PathBuf>,
        }
        impl RawClientInitializer for Initializer {
            fn init(
                // smth smth box needs to know size
                self: Box<Self>,
                surface: SharedState<UITree>,
                // the treeview is drawn into the surface
                _treeview: SharedState<Treeview>,
                event_queue: Channel<StandardEvent>,
                // yeah, ik the naming is inconsistent bc I'm not saying "event_receiver" or "event_rx", but it's calm
                // (I am really trying to convince myself this is fine, I am the strawman)
                request_sender: Sender<StandardRequest>,
            ) {
                let mut event_loop = EventLoop::try_new().unwrap();

                event_loop
                    .handle()
                    .insert_source(event_queue, |event, &mut (), root: &mut RootNodeApplet| {
                        let calloop::channel::Event::Msg(event) = event else {
                            return;
                        };
                        root.handle_event(event);
                    })
                    .unwrap();

                event_loop
                    .handle()
                    .insert_source(
                        Timer::from_duration(RootNodeApplet::SAVE_INTERVAL),
                        |_, &mut (), root: &mut RootNodeApplet| {
                            root.save_session();
                            TimeoutAction::ToDuration(RootNodeApplet::SAVE_INTERVAL)
                        },
                    )
                    .unwrap();

                let (settings_tx, settings_rx) = calloop::channel::channel();
                settings::subscribe(settings_tx);
                event_loop
                    .handle()
                    .insert_source(settings_rx, |event, &mut (), root: &mut RootNodeApplet| {
                        if let calloop::channel::Event::Msg(()) = event {
                            root.surface.damage();
                        }
                    })
                    .unwrap();

                let mut root = RootNodeApplet::new(
                    self.root_initializer,
                    self.applet_spawner_registry,
                    self.session_path,
                    surface,
                    request_sender,
                    &event_loop.handle(),
                );

                // after each batch of events, since any of them may move focus or damage
                event_loop
                    .run(None, &mut root, |root| {
                        root.holder.finish();
                        if std::mem::take(&mut root.holder.quit) {
                            root.quit();
                        }
                        root.holder.tree.update_view_root();
                        root.update_view_layer();
                        root.damage_if_tree_changed();
                        root.update_treeview_visibility();
                        root.holder.tree.update_layout();
                        root.holder.update_wl_focus();
                        root.flush_surface();
                    })
                    .unwrap();
            }
        }
        Box::new(Initializer {
            root_initializer,
            applet_spawner_registry,
            session_path,
        })
    }

    /// From the runner
    fn handle_event(&mut self, event: StandardEvent) {
        if let StandardEvent::UIEvent(ui_event) = &event
            && let Some(keybind) = RootKeybind::of(ui_event)
        {
            match keybind {
                RootKeybind::Help => self.toggle_help(),
                RootKeybind::ViewRoot => self.holder.tree.toggle_view_root(),
                RootKeybind::ViewLayer => self.set_view_layer(&self.holder.tree.get_focus_path()),
            }
            self.damage_if_tree_changed();
            return;
        }
        match event {
            StandardEvent::UIEvent(UIEvent::WindowResized(size)) => {
                self.latest_size = size;
                self.surface.damage();
                self.holder
                    .tree
                    .handle_event(StandardEvent::UIEvent(UIEvent::WindowResized(
                        self.inner_applet_size(),
                    )));
            }
            StandardEvent::UIEvent(UIEvent::Mouse(mouse_event)) => self.handle_mouse(mouse_event),
            StandardEvent::DamageAck(StateKind::Surface) => self.surface.ack(),
            // the treeview is drawn into the surface
            StandardEvent::DamageAck(StateKind::Treeview) => {}
            // the root can't close without ending everything
            StandardEvent::CloseRequest => self.quit(),
            StandardEvent::WlSurfaceRegistered(surface) => {
                // a restored tile, else where the user is working (eg under the command hub that ran it)
                self.holder.add_wl_surface(surface);
            }
            StandardEvent::Pluck(path) => self.holder.start_pluck(path.as_ref()),
            StandardEvent::Place(path, position, tree) => {
                let path = path.map(|path| self.redirect_drop(path));
                self.holder.start_place(path.as_ref(), position, tree);
            }
            StandardEvent::Focus(path) => self.holder.focus(&path),
            StandardEvent::ToggleCollapsed(path) => self.holder.toggle_collapsed(&path),
            StandardEvent::NodeAction(path, action) => self.holder.node_action(&path, action),
            StandardEvent::Capturing(_) => {}
            // it reads the settings when it draws
            StandardEvent::SettingsChanged(_) => self.surface.damage(),
            // its tree is in the runner's directory
            StandardEvent::Directory(_) => {}
            StandardEvent::Open { applet, args } => {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                self.holder.open(&applet, &args);
            }
            StandardEvent::UIEvent(_)
            | StandardEvent::FocusChanged(..)
            | StandardEvent::Highlighted(..) => {
                // the root is never a project, so nothing is passed up
                let _ = self.holder.handle_ui_event(event);
            }
        }

        self.damage_if_tree_changed();
    }

    /// Closes the help, or opens it as a child of the focused node (so it shows there), focused
    fn toggle_help(&mut self) {
        let tree = &mut self.holder.tree;
        if let Some(help) = self.help.take()
            && tree.close(help.node)
        {
            self.help_closed(&help);
            return;
        }
        let parent = tree.focused_node();
        let parent_inner = tree.is_inner(parent);
        if let Some(node) = tree.add_child(parent, HelpApplet::get_boxed_initiator()) {
            tree.focus(node, true);
            self.help = Some(OpenHelp {
                node,
                parent,
                parent_inner,
            });
        }
    }

    /// Focus goes back inside the applet it was opened from, if it was there and the help had focus
    fn help_closed(&mut self, help: &OpenHelp) {
        let tree = &mut self.holder.tree;
        if help.parent_inner && tree.focused_node() == help.parent {
            tree.focus(help.parent, true);
        }
    }

    fn handle_mouse(&mut self, mut mouse_event: MouseEvent) {
        let settings = settings::get();
        let treeview_area = self.treeview_area(&settings);
        // a hidden treeview has no rows to hit
        let (rows, general) = if treeview_area.is_some() {
            view_rows(&self.holder.tree, self.view_layer.as_ref())
        } else {
            (Vec::new(), WorldTreePath::new_into())
        };
        // in the treeview's coords, inside its frame
        let ([x, y, width, height], border) = treeview_area
            .map_or(([0; 4], 0), |(area, position)| {
                treeview_frame(&settings, area, position)
            });
        let [x, y, width, height] = [x, y, width, height].map(f64::from);
        let [pointer_x, pointer_y] = mouse_event.position;
        let in_treeview = treeview_area.is_some()
            && (x..x + width).contains(&pointer_x)
            && (y..y + height).contains(&pointer_y);
        let mut treeview_event = mouse_event;
        treeview_event.position = [
            pointer_x - x - f64::from(border),
            pointer_y - y - f64::from(border),
        ];
        let response = self.treeview_pointer.handle_mouse(
            &treeview_event,
            in_treeview,
            &rows,
            &general,
            [width, height].map(|length| (length - f64::from(2 * border)).max(0.0) as f32),
        );

        if response.damaged {
            self.surface.damage();
        }
        match response.action {
            Some(TreeviewAction::Focus(path)) => self.holder.focus(&path),
            Some(TreeviewAction::ToggleCollapsed(path)) => self.holder.toggle_collapsed(&path),
            Some(TreeviewAction::Node(path, action)) => self.holder.node_action(&path, action),
            Some(TreeviewAction::SetViewLayer(path)) => self.set_view_layer(&path),
            Some(TreeviewAction::StartDrag(path)) => {
                self.send_request(StandardRequest::RequestPluck(path));
            }
            Some(TreeviewAction::Drop(target)) => {
                let (path, position) = target.unwrap_or_else(|| {
                    (
                        self.holder.tree.get_focus_path(),
                        SpawnPosition::AppendChild,
                    )
                });
                self.last_drop = path
                    .0
                    .first()
                    .and_then(|node_path| self.holder.tree.node_at(node_path))
                    .map(|id| (path.clone(), id));
                self.send_request(StandardRequest::RequestPlace(path, position));
            }
            None => {}
        }
        if response.consumed {
            return;
        }

        // remap window coords -> tree coords
        mouse_event.position[0] -= f64::from(self.applet_area(&settings)[0]);
        self.holder
            .tree
            .handle_event(StandardEvent::UIEvent(UIEvent::Mouse(mouse_event)));
    }

    /// If this is the last drop, the path to the node it was on now.
    /// That node was plucked too if it was dropped onto the dragged part, so it goes to the focus instead.
    fn redirect_drop(&mut self, path: WorldTreePath) -> WorldTreePath {
        match self.last_drop.take() {
            Some((dropped, id)) if dropped == path => self
                .holder
                .full_path(id, inner_path(&path))
                .unwrap_or_else(|| self.holder.tree.get_focus_path()),
            _ => path,
        }
    }

    /// From an applet in the tree
    fn handle_node_request(&mut self, id: NodeId, request: NodeRequest) {
        let closes_help = matches!(request, NodeRequest::Close)
            && self.help.as_ref().is_some_and(|help| help.node == id);
        // nothing is above the root
        match self.holder.handle_node_request(id, request) {
            None | Some(NodeRequest::FocusSelf) => {}
            Some(_) => log::warn!("The root holder has no node to pass a request to"),
        }
        if closes_help && let Some(help) = self.help.take() {
            self.help_closed(&help);
        }
        self.damage_if_tree_changed();
    }

    /// Shows the tree that `path` is in
    fn set_view_layer(&mut self, path: &WorldTreePath) {
        self.view_layer = path
            .0
            .split_last()
            .map(|(_, holder)| holder)
            .filter(|holder| !holder.is_empty())
            .map(|holder| WorldTreePath(holder.into()));
        self.surface.damage();
    }

    /// Moves the view layer out until focus is inside it (eg focus went out of it), and out of gone nodes
    fn update_view_layer(&mut self) {
        let Some(mut holder) = self.view_layer.clone() else {
            return;
        };
        let focus = self.holder.tree.get_focus_path();
        let treeview = self.holder.tree.get_treeview();
        while !(focus.0.len() > holder.0.len()
            && focus.0.starts_with(&holder.0)
            && matches!(treeview.safe_get(&holder), Some(WorldTree::World(_))))
        {
            let mut outer = holder.0.into_vec();
            outer.pop();
            if outer.is_empty() {
                self.view_layer = None;
                self.surface.damage();
                return;
            }
            holder = WorldTreePath(outer.into_boxed_slice());
        }
        if self.view_layer.as_ref() != Some(&holder) {
            self.view_layer = Some(holder);
            self.surface.damage();
        }
    }

    fn damage_if_tree_changed(&mut self) {
        if self.holder.tree.take_damage() {
            self.surface.damage();
        }
    }

    /// The treeview at `area`, framed like a pane with `always_frame` (a floating one always is)
    fn get_treeview(
        tree: &NodeTree,
        view_layer: Option<&WorldTreePath>,
        treeview_pointer: &mut TreeviewPointer,
        (area, position): ([u32; 4], TreeviewPosition),
    ) -> UITree {
        let (rows, _) = view_rows(tree, view_layer);
        let settings = settings::get();
        let ([x, y, width, height], border) = treeview_frame(&settings, area, position);
        let treeview = treeview_pointer.render(
            &rows,
            [
                width.saturating_sub(2 * border) as f32,
                height.saturating_sub(2 * border) as f32,
            ],
            &tree.get_focus_path(),
        );
        UITree::new_styled(
            UIElement::Bordered {
                color: settings.border.treeview,
                radius: if border > 0 { settings.radius } else { 0.0 },
            },
            Style {
                border: Length::uniform_rect(Length::Px(border as f32)),
                ..Style::absolute([x as f32, y as f32], [width as f32, height as f32])
            },
            vec![treeview],
        )
    }
}

/// The treeview's frame (`[x, y, width, height]`, border included) in its `area`, and its border width.
/// Framed like a pane with `always_frame`: in a column the panes' gap is on the side facing them, so it gets the
/// other gaps. A floating one is always framed, as it is over the panes.
fn treeview_frame(
    settings: &Settings,
    [x, y, width, height]: [u32; 4],
    position: TreeviewPosition,
) -> ([u32; 4], u32) {
    let gap = settings.gap.round() as u32;
    let border = settings.border.width.round() as u32;
    match position {
        TreeviewPosition::Floating => ([x, y, width, height], border.max(1)),
        _ if !settings.always_frame => ([x, y, width, height], 0),
        TreeviewPosition::Left => (
            [
                x + gap,
                y + gap,
                width.saturating_sub(gap),
                height.saturating_sub(2 * gap),
            ],
            border,
        ),
        TreeviewPosition::Right => (
            [
                x,
                y + gap,
                width.saturating_sub(gap),
                height.saturating_sub(2 * gap),
            ],
            border,
        ),
    }
}

/// The treeview's rows: the view layer's inner tree, else the view root's subtree.
/// Also where its menu adds things when opened on no row: that inner tree's root, else the view root.
fn view_rows(
    tree: &NodeTree,
    view_layer: Option<&WorldTreePath>,
) -> (Vec<TreeviewRow>, WorldTreePath) {
    if let Some(rows) = view_layer.and_then(|holder| inner_layer_rows(&tree.get_treeview(), holder))
    {
        // the first row is the node holding it
        let mut general = rows[0].path.0.to_vec();
        general.push(TreeNodePath::new_root());
        return (rows, WorldTreePath(general.into_boxed_slice()));
    }
    let base = tree.view_root_path();
    let rows = treeview_rows(&tree.get_view_treeview(), &base, tree.view_parent());
    (rows, WorldTreePath(Box::new([base])))
}

/// The part of the path inside the node's applet
fn inner_path(path: &WorldTreePath) -> WorldTreePath {
    WorldTreePath(path.0.get(1..).unwrap_or_default().into())
}

/// The root's own keybinds
enum RootKeybind {
    /// Super+?
    Help,
    /// Super+V
    ViewRoot,
    /// Alt+r
    ViewLayer,
}
impl RootKeybind {
    fn of(ui_event: &UIEvent) -> Option<Self> {
        let UIEvent::Key {
            symbol: Some(KeySymbol::Char(c)),
            modifiers,
            pressed: true,
            ..
        } = *ui_event
        else {
            return None;
        };
        match c {
            '?' if modifiers.logo => Some(Self::Help),
            'v' | 'V' if modifiers.logo => Some(Self::ViewRoot),
            'r' if modifiers == KeyModifiers::ALT => Some(Self::ViewLayer),
            _ => None,
        }
    }
}

/// Whether the root or a node acts on this key, rather than a focused wl client.
/// The runner sends other keys straight to the client (#56).
pub fn is_keybind(ui_event: &UIEvent) -> bool {
    RootKeybind::of(ui_event).is_some() || standard_keybind_request(ui_event).is_some()
}
