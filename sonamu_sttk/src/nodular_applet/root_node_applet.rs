use crate::{
    man_page::ManPageApplet,
    nodular_applet::{
        AppletSpawner, NodeRequest, NodularAppletInitializer,
        node_holder::{NodeHolder, client_spawners},
        node_tree::{NodeId, NodeTree},
        project_applet::ProjectApplet,
        split_applet::SplitApplet,
        swipe::{Settle, Strip, Swipe},
        traversal::{self, Traversal},
        treeview::{
            BTN_LEFT, TreeviewAction, TreeviewPointer, TreeviewRow, close_width, focused_row,
            treeview_rows, with_opacity,
        },
    },
    session,
    standard_keybinds::standard_keybind_request,
    ui::{self, Rows},
};
use calloop::{
    EventLoop, LoopHandle,
    channel::{Channel, Sender},
    timer::{TimeoutAction, Timer},
};
use sonamu_common::{
    sap::{
        packets::{OpenApplet, SpawnPosition, StandardEvent, StandardRequest, StateKind, Treeview},
        raw_client_initializer::RawClientInitializer,
        session::Session,
        shared_state::{SharedState, StatePublisher},
    },
    settings::{self, Settings, TreeviewFloating, TreeviewSide},
    utils::tree::{
        tree_node_path::TreeNodePath,
        world_tree::{Step, WorldTree, WorldTreePath},
    },
};
use sonamu_ui::{
    display_units::DisplayContainerSize,
    ui_element::{Style, UIElement, UITree, font_size},
    ui_event::{KeyModifiers, KeySymbol, MouseEvent, MouseEventKind, SwipePhase, UIEvent},
};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
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
    /// Pinned, or traversing by keys (see `update_treeview_visibility`)
    treeview_shown: bool,
    /// The selector was last moved by keys, so it shows the treeview; gestures leave it hidden, the selected holder
    /// drawing its zoomed-out view instead (#145)
    traversing_by_keys: bool,
    /// Always shown, beside the panes; toggled by its pin button and saved in the session
    treeview_pinned: bool,
    /// The view root when it is a row inside an applet (double click, right click › Set as view root, Alt+R);
    /// `None`: the `NodeTree`'s view root. Moves out with focus (see `update_view_path`).
    view_path: Option<WorldTreePath>,
    /// The focus path `update_view_path` last saw: it only moves the view out when focus moves, since right
    /// after switching screens an applet's published focus (eg a project's) is still the other screen's
    view_path_focus: Option<WorldTreePath>,
    /// The applets' area's left edge and width, as last sent to them
    applet_area: [u32; 2],
    /// Where the session is saved, if anywhere
    session_path: Option<PathBuf>,
    /// The last one written, so unchanged ones aren't written again
    saved_session: Option<Session>,
    /// Every screen (#60), each with its own focus (see `NodeTree::activate_screen`).
    /// The active one's view is the fields above (`latest_size`, `view_path`, `treeview_pointer`).
    screens: Vec<Screen>,

    /// The 3 finger swipe while the fingers are down (#141)
    swipe: Option<Swipe>,
    /// Easing onto a node after a swipe, a frame at a time (`tick_settle`)
    settle: Option<Settle>,
    /// Whether `tick_settle`'s timer is running
    settling: bool,
    /// How far a 4 finger swipe moved, until it acts (once a swipe)
    select_swipe: Option<[f64; 2]>,
    /// The child row each row last had focus under, for swipes into children (#142); not saved
    remembered: HashMap<WorldTreePath, WorldTreePath>,
    /// When Ctrl+Shift+Q was pressed, while it asks to press it again (#139)
    quit_asked: Option<Instant>,
    loop_handle: LoopHandle<'static, Self>,
}

/// A screen, in the root's area. While inactive it keeps its view, and shows the treeview only if pinned.
#[derive(Default)]
struct Screen {
    /// `[x, y, width, height]`
    rect: [u32; 4],
    view_path: Option<WorldTreePath>,
    treeview_pointer: TreeviewPointer,
}

/// Where the treeview is
#[derive(Debug, Clone, Copy)]
enum TreeviewPlace {
    /// Pinned beside the panes
    Column(TreeviewSide),
    /// Over the panes
    Floating,
}

impl RootNodeApplet {
    /// How often the session is saved (if it changed), besides before quitting
    const SAVE_INTERVAL: Duration = Duration::from_secs(2);
    /// Between frames of a swipe settling
    const SETTLE_FRAME: Duration = Duration::from_millis(8);
    /// How far (px) a 4 finger swipe moves to act
    const SELECT_DISTANCE: f64 = 50.0;
    /// How long Ctrl+Shift+Q waits to be pressed again
    const QUIT_CONFIRM: Duration = Duration::from_secs(3);
    const QUIT_NOTICE: &str = "Press Ctrl+Shift+Q again to quit";

    /// Where the treeview is drawn (`[x, y, width, height]`, frame included), and how;
    /// `None` while hidden
    fn treeview_area(&self, settings: &Settings) -> Option<([u32; 4], TreeviewPlace)> {
        treeview_area(
            settings,
            self.latest_size,
            self.treeview_shown,
            self.treeview_pinned,
        )
    }

    /// The applets' area's left edge and width: beside a treeview column, else everything
    fn applet_area(&self, settings: &Settings) -> [u32; 2] {
        applet_area(self.treeview_area(settings), self.latest_size)
    }

    fn inner_applet_size(&self) -> DisplayContainerSize {
        DisplayContainerSize::new(
            self.applet_area(&settings::get())[1],
            self.latest_size.height,
        )
    }

    /// Shows the treeview if pinned or traversing by keys, else hides it, resizing the applets if their area changed
    /// (also when the treeview settings or pinning did)
    fn update_treeview_visibility(&mut self) {
        let settings = settings::get();
        let shown = self.treeview_pinned
            || (self.traversing_by_keys && self.holder.tree.is_traversing())
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
        self.resize_inactive_screens();
    }

    /// Publishes the window (every screen) if damaged and the runner read the last one
    fn flush_surface(&mut self) {
        let settings = settings::get();
        let shown = self.treeview_shown;
        let pinned = self.treeview_pinned;
        let quit_asked = self.quit_asked.is_some();
        let Self {
            holder,
            treeview_pointer,
            view_path,
            screens,
            ..
        } = self;
        let active = holder.tree.active_screen();
        self.surface.flush(
            || {
                let screens = screens.iter_mut().enumerate().map(|(index, screen)| {
                    let [x, y, width, height] = screen.rect;
                    let size = DisplayContainerSize::new(width, height);
                    // the active one's view is the root's own
                    let (shown, view_path, treeview_pointer) = if index == active {
                        (shown, &*view_path, &mut *treeview_pointer)
                    } else {
                        (pinned, &screen.view_path, &mut screen.treeview_pointer)
                    };
                    let treeview_area = treeview_area(&settings, size, shown, pinned);
                    let [left, width] =
                        applet_area(treeview_area, size).map(|length| length as f32);
                    let mut window = vec![UITree::new_container_styled(
                        Style::absolute([left, 0.0], [width, height as f32]),
                        vec![holder.tree.get_screen_window(index)],
                    )];
                    if let Some(area) = treeview_area {
                        window.push(Self::get_treeview(
                            &holder.tree,
                            index,
                            view_path.as_ref(),
                            treeview_pointer,
                            area,
                            pinned,
                        ));
                    }
                    if quit_asked && index == active {
                        window.push(notice(&settings, Self::QUIT_NOTICE, size));
                    }
                    UITree::new_container_styled(
                        Style::absolute([x as f32, y as f32], [size.width as f32, height as f32]),
                        window,
                    )
                });
                UITree::new_container(screens.collect())
            },
            &self.request_sender,
        );
    }

    /// The screens' areas from the runner
    fn set_screens(&mut self, rects: Vec<[u32; 4]>) {
        if rects.is_empty() {
            return;
        }
        // the active one may be gone
        let active = self.holder.tree.active_screen().min(rects.len() - 1);
        self.activate_screen(active);
        self.holder.tree.set_screen_count(rects.len());
        self.screens.resize_with(rects.len(), Screen::default);
        for (screen, rect) in self.screens.iter_mut().zip(rects) {
            screen.rect = rect;
        }
        let [_, _, width, height] = self.screens[active].rect;
        self.latest_size = DisplayContainerSize::new(width, height);
        self.surface.damage();
        self.holder
            .tree
            .handle_event(StandardEvent::UIEvent(UIEvent::WindowResized(
                self.inner_applet_size(),
            )));
        self.resize_inactive_screens();
    }

    /// Keys and the treeview go to the screen from now on (eg the pointer moved onto it)
    fn activate_screen(&mut self, index: usize) {
        let active = self.holder.tree.active_screen();
        if index == active || index >= self.screens.len() {
            return;
        }
        let left = &mut self.screens[active];
        left.view_path = self.view_path.take();
        left.treeview_pointer = std::mem::take(&mut self.treeview_pointer);
        let screen = &mut self.screens[index];
        self.view_path = screen.view_path.take();
        self.treeview_pointer = std::mem::take(&mut screen.treeview_pointer);
        let [_, _, width, height] = screen.rect;
        self.latest_size = DisplayContainerSize::new(width, height);
        self.holder.tree.activate_screen(index);
        self.surface.damage();
    }

    /// Sends the inactive screens' applets' areas' sizes to the tree (the treeview shows on them only if pinned)
    fn resize_inactive_screens(&mut self) {
        let settings = settings::get();
        let active = self.holder.tree.active_screen();
        for (index, screen) in self.screens.iter().enumerate() {
            if index == active {
                continue;
            }
            let [_, _, width, height] = screen.rect;
            let size = DisplayContainerSize::new(width, height);
            let treeview_area =
                treeview_area(&settings, size, self.treeview_pinned, self.treeview_pinned);
            self.holder.tree.resize_screen(
                index,
                Some(DisplayContainerSize::new(
                    applet_area(treeview_area, size)[1],
                    height,
                )),
            );
        }
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
            focus: self.holder.tree.get_focus_path(),
            treeview_pinned: self.treeview_pinned,
        };
        if self.saved_session.as_ref() == Some(&session) {
            return;
        }
        match session::write(path, &session) {
            Ok(()) => self.saved_session = Some(session),
            Err(err) => log::warn!("Couldn't save the session to {}: {err}", path.display()),
        }
    }

    /// Ctrl+Shift+Q: quits if pressed again within `QUIT_CONFIRM`, else shows a notice asking for that (#139)
    fn ask_quit(&mut self) {
        if self
            .quit_asked
            .is_some_and(|asked| asked.elapsed() < Self::QUIT_CONFIRM)
        {
            self.quit();
            return;
        }
        self.quit_asked = Some(Instant::now());
        self.surface.damage();
        self.loop_handle
            .insert_source(
                Timer::from_duration(Self::QUIT_CONFIRM),
                |_, &mut (), root: &mut Self| {
                    // not asked again meanwhile
                    if root
                        .quit_asked
                        .is_some_and(|asked| asked.elapsed() >= Self::QUIT_CONFIRM)
                    {
                        root.quit_asked = None;
                        root.surface.damage();
                    }
                    TimeoutAction::Drop
                },
            )
            .unwrap();
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
        mut applet_spawner_registry: BTreeMap<String, AppletSpawner>,
        session_path: Option<PathBuf>,
        surface: SharedState<UITree>,
        request_sender: Sender<StandardRequest>,
        event_loop: &LoopHandle<'static, Self>,
    ) -> Self {
        // saved projects and splits are made again by these
        applet_spawner_registry.insert("project".to_string(), ProjectApplet::get_applet_spawner());
        applet_spawner_registry.insert("split".to_string(), SplitApplet::get_applet_spawner());
        // Super+? opens it
        applet_spawner_registry.insert("help".to_string(), ManPageApplet::get_applet_spawner());
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
                tree.focus_path(&saved.focus);
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

        let mut root = Self {
            holder: NodeHolder::new(tree, send, false, applet_spawners),
            surface: StatePublisher::new(surface),
            request_sender,
            // Just making stuff up
            latest_size: DisplayContainerSize::new(800, 600),
            treeview_pointer: TreeviewPointer::default(),
            last_drop: None,
            // the root node starts focused
            treeview_shown: true,
            traversing_by_keys: false,
            // a clean session starts pinned
            treeview_pinned: saved_session
                .as_ref()
                .is_none_or(|saved| saved.treeview_pinned),
            view_path: None,
            view_path_focus: None,
            applet_area: [0, 800],
            session_path,
            saved_session,
            screens: vec![Screen {
                rect: [0, 0, 800, 600],
                ..Screen::default()
            }],
            swipe: None,
            settle: None,
            settling: false,
            select_swipe: None,
            remembered: HashMap::new(),
            quit_asked: None,
            loop_handle: event_loop.clone(),
        };
        // a clean session starts with the help shown
        if root.saved_session.is_none() {
            root.open_help();
        }
        root
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
                            root.ask_quit();
                        }
                        root.holder.tree.update_view_root();
                        root.update_view_path();
                        root.remember_focus();
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
        if let StandardEvent::UIEvent(UIEvent::Swipe { fingers, phase }) = event {
            self.traversing_by_keys = false;
            match fingers {
                3 => self.handle_swipe(phase),
                4 => self.handle_select_swipe(phase),
                _ => {}
            }
            self.damage_if_tree_changed();
            return;
        }
        if let StandardEvent::UIEvent(ui_event) = &event
            && let Some(keybind) = RootKeybind::of(ui_event)
        {
            match keybind {
                RootKeybind::Help => self.open_help(),
                RootKeybind::ViewRoot => self.toggle_view_root(),
                RootKeybind::ViewParent => {
                    let mut parent = self.holder.tree.get_focus_path();
                    parent.0.pop();
                    self.set_view_root(&parent);
                }
            }
            self.damage_if_tree_changed();
            return;
        }
        if let StandardEvent::UIEvent(ui_event) = &event
            && let Some(traversal) = Traversal::of(ui_event, self.holder.tree.is_traversing())
        {
            self.traversing_by_keys = true;
            self.traverse(traversal);
            self.damage_if_tree_changed();
            return;
        }
        match event {
            // the screens' areas follow
            StandardEvent::UIEvent(UIEvent::WindowResized(_)) => {}
            StandardEvent::UIEvent(UIEvent::Mouse(mouse_event)) => self.handle_mouse(mouse_event),
            StandardEvent::DamageAck(StateKind::Surface) => self.surface.ack(),
            // the treeview is drawn into the surface
            StandardEvent::DamageAck(StateKind::Treeview) => {}
            // the root can't close without ending everything
            StandardEvent::CloseRequest => self.quit(),
            // the root is never deactivated
            StandardEvent::Deactivate => {}
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
            StandardEvent::Screens(rects) => self.set_screens(rects),
            // it has the screens itself
            StandardEvent::ScreenViews { .. } => {}
            // it reads the settings when it draws
            StandardEvent::SettingsChanged(_) => self.surface.damage(),
            // its tree is in the runner's directory
            StandardEvent::Directory(_) => {}
            StandardEvent::Open(open) => self.holder.open(open),
            // it starts them
            StandardEvent::Swipe(_) => {}
            StandardEvent::UIEvent(_)
            | StandardEvent::FocusChanged(..)
            | StandardEvent::Highlighted(..) => {
                // the root is never a project, so nothing is passed up
                let _ = self.holder.handle_ui_event(event);
            }
        }

        self.damage_if_tree_changed();
    }

    /// Where a new wl surface would go, focused
    fn open_help(&mut self) {
        self.holder.open(OpenApplet {
            applet: "help".to_string(),
            args: Vec::new(),
        });
    }

    fn handle_mouse(&mut self, mut mouse_event: MouseEvent) {
        // another screen becomes active when clicked (not during a drag); until then it ignores the pointer
        let [pointer_x, pointer_y] = mouse_event.position;
        if !self.treeview_pointer.is_pressed()
            && !self.holder.tree.has_pointer_grab()
            && let Some(index) = self.screens.iter().position(|screen| {
                let [x, y, width, height] = screen.rect.map(f64::from);
                (x..x + width).contains(&pointer_x) && (y..y + height).contains(&pointer_y)
            })
            && index != self.holder.tree.active_screen()
        {
            if !matches!(
                mouse_event.kind,
                MouseEventKind::Button { pressed: true, .. }
            ) {
                return;
            }
            self.activate_screen(index);
        }
        // in the active screen's coords
        let [x, y, ..] = self.screens[self.holder.tree.active_screen()].rect;
        mouse_event.position = [pointer_x - f64::from(x), pointer_y - f64::from(y)];

        let settings = settings::get();
        let treeview_area = self.treeview_area(&settings);
        // a hidden treeview has no rows to hit
        let (rows, general) = if treeview_area.is_some() {
            view_rows(
                &self.holder.tree,
                self.view_path.as_ref(),
                self.holder.tree.active_screen(),
            )
        } else {
            (Vec::new(), WorldTreePath::root())
        };
        // in the treeview's coords, inside its frame
        let ([x, y, width, height], border) = treeview_area.map_or(([0; 4], 0), |(area, place)| {
            treeview_frame(&settings, area, place)
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
        let inner_width = (width - f64::from(2 * border)).max(0.0);
        if in_treeview
            && !self.treeview_pointer.is_pressed()
            && on_pin_button(treeview_event.position, inner_width)
            && matches!(
                mouse_event.kind,
                MouseEventKind::Button {
                    button: BTN_LEFT,
                    pressed: true
                }
            )
        {
            self.treeview_pinned = !self.treeview_pinned;
            self.surface.damage();
            self.update_treeview_visibility();
            return;
        }
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
        if let Some(action) = response.action {
            self.apply_treeview_action(action);
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

    fn apply_treeview_action(&mut self, action: TreeviewAction) {
        match action {
            TreeviewAction::Focus(path) => self.holder.focus(&path),
            TreeviewAction::ToggleCollapsed(path) => self.holder.toggle_collapsed(&path),
            TreeviewAction::Node(path, action) => self.holder.node_action(&path, action),
            TreeviewAction::SetViewRoot(path) => self.set_view_root(&path),
            TreeviewAction::StartDrag(path) => {
                self.send_request(StandardRequest::RequestPluck(path));
            }
            TreeviewAction::Drop(target) => {
                let (path, position) = target.unwrap_or_else(|| {
                    (
                        self.holder.tree.get_focus_path(),
                        SpawnPosition::AppendChild,
                    )
                });
                self.last_drop = self
                    .holder
                    .tree
                    .node_at(&path.split_world().0)
                    .map(|id| (path.clone(), id));
                self.send_request(StandardRequest::RequestPlace(path, position));
            }
        }
    }

    /// If this is the last drop, the path to the node it was on now.
    /// That node was plucked too if it was dropped onto the dragged part, so it goes to the focus instead.
    fn redirect_drop(&mut self, path: WorldTreePath) -> WorldTreePath {
        match self.last_drop.take() {
            Some((dropped, id)) if dropped == path => self
                .holder
                .full_path(id, &path.split_world().1.unwrap_or_default())
                .unwrap_or_else(|| self.holder.tree.get_focus_path()),
            _ => path,
        }
    }

    /// From an applet in the tree
    fn handle_node_request(&mut self, id: NodeId, request: NodeRequest) {
        // nothing is above the root
        match self.holder.handle_node_request(id, request) {
            None | Some(NodeRequest::FocusSelf) => {}
            Some(_) => log::warn!("The root holder has no node to pass a request to"),
        }
        self.damage_if_tree_changed();
    }

    /// Moves focus by the shown rows (#83), across applets' inner trees
    fn traverse(&mut self, traversal: Traversal) {
        if matches!(traversal, Traversal::Enter) {
            // like Enter in the selector, wherever it is
            let _ = self
                .holder
                .handle_ui_event(StandardEvent::UIEvent(UIEvent::Key {
                    symbol: Some(KeySymbol::Enter),
                    modifiers: KeyModifiers::NONE,
                    raw_keycode: 0,
                    pressed: true,
                }));
            return;
        }
        let focus = self.holder.tree.get_focus_path();
        let rows = view_tree_rows(
            &self.holder.tree,
            self.view_path.as_ref(),
            self.holder.tree.active_screen(),
            false,
        );
        let from = rows
            .iter()
            .position(|row| row.path == focused_row(&rows, &focus));
        if let Some(target) = traversal::target(&rows, from, traversal) {
            self.holder.focus(&target);
            self.surface.damage();
        }
    }

    /// 3 fingers: the view follows them between nodes, and settles on one when they lift (`swipe`, #141)
    fn handle_swipe(&mut self, phase: SwipePhase) {
        let settings = settings::get().swipe;
        match phase {
            SwipePhase::Begin => {
                // it carries on from where a settle it interrupts was
                let carried = self
                    .settle
                    .take()
                    .map(|settle| settle.carried(Instant::now()));
                self.swipe = Some(Swipe::new(carried));
            }
            SwipePhase::Update(delta) => {
                let Self {
                    swipe: Some(swipe),
                    holder,
                    view_path,
                    remembered,
                    ..
                } = self
                else {
                    return;
                };
                let view = swipe.update(delta, &settings, |vertical| {
                    let tree = &holder.tree;
                    let rows =
                        view_tree_rows(tree, view_path.as_ref(), tree.active_screen(), false);
                    let focus = focused_row(&rows, &tree.get_focus_path());
                    let from = rows.iter().position(|row| row.path == focus)?;
                    Some(if vertical {
                        Strip::lineage(&rows, from, remembered)
                    } else {
                        let mut strip = Strip::siblings(&rows, from);
                        let mut parent = focus;
                        parent.0.pop();
                        strip.reversed = tree.shown_at_once().contains(&parent);
                        strip
                    })
                });
                if let Some(view) = view {
                    self.holder.tree.set_swipe(Some(view));
                }
            }
            SwipePhase::End { cancelled } => {
                let Some(settle) = self
                    .swipe
                    .take()
                    .and_then(|swipe| swipe.end(cancelled, &settings))
                else {
                    // it never moved far enough (but may have stopped a settle)
                    self.holder.tree.set_swipe(None);
                    return;
                };
                if settle.target != settle.strip.start {
                    self.holder.focus(&settle.strip.paths[settle.target]);
                    self.surface.damage();
                }
                self.settle = Some(settle);
                if !std::mem::replace(&mut self.settling, true) {
                    self.loop_handle
                        .insert_source(Timer::immediate(), |_, &mut (), root: &mut Self| {
                            root.tick_settle()
                        })
                        .unwrap();
                }
            }
        }
    }

    /// A frame of the settle, until it arrives (or a swipe interrupts it)
    fn tick_settle(&mut self) -> TimeoutAction {
        let Some(settle) = &self.settle else {
            self.settling = false;
            return TimeoutAction::Drop;
        };
        let view = settle.view(Instant::now());
        let arrived = view.is_none();
        self.holder.tree.set_swipe(view);
        self.damage_if_tree_changed();
        if arrived {
            self.settle = None;
            self.settling = false;
            return TimeoutAction::Drop;
        }
        TimeoutAction::ToDuration(Self::SETTLE_FRAME)
    }

    /// 4 fingers up zoom out to the holder around the focus, down zoom back in (#145); once a swipe
    fn handle_select_swipe(&mut self, phase: SwipePhase) {
        match phase {
            SwipePhase::Begin => self.select_swipe = Some([0.0; 2]),
            SwipePhase::Update([x, y]) => {
                let Some(moved) = &mut self.select_swipe else {
                    return;
                };
                *moved = [moved[0] + x, moved[1] + y];
                let [x, y] = *moved;
                if x.hypot(y) < Self::SELECT_DISTANCE {
                    return;
                }
                self.select_swipe = None;
                if x.abs() > y.abs() {
                    return;
                }
                if y < 0.0 {
                    self.zoom_out();
                } else {
                    self.zoom_in();
                }
            }
            SwipePhase::End { .. } => self.select_swipe = None,
        }
    }

    /// Selects the nearest holder around the focus: the node whose inner tree it is in, else the root (#145).
    /// Selected, a holder draws its zoomed-out view (eg a project's overview, the root's map).
    fn zoom_out(&mut self) {
        let mut path = self.holder.tree.get_focus_path();
        match path.0.iter().rposition(|step| matches!(step, Step::Inner(_))) {
            Some(index) => path.0.truncate(index),
            None if path.0.is_empty() => return,
            None => path.0.clear(),
        }
        self.holder.focus(&path);
        self.surface.damage();
    }

    /// Back down the rows focus was last under (`remembered`) to the next holder, selected, or else to the last
    /// row, inside it (#145)
    fn zoom_in(&mut self) {
        let start = self.holder.tree.get_focus_path();
        let rows = view_tree_rows(
            &self.holder.tree,
            self.view_path.as_ref(),
            self.holder.tree.active_screen(),
            false,
        );
        let mut path = start.clone();
        while let Some(child) = self
            .remembered
            .get(&path)
            .filter(|child| rows.iter().any(|row| &row.path == *child))
        {
            // into another holder's inner tree: it is selected first
            if path != start && matches!(child.0.last(), Some(Step::Inner(_))) {
                self.holder.focus(&path);
                self.surface.damage();
                return;
            }
            path = child.clone();
        }
        if path != start {
            self.holder.focus(&path);
        }
        if self.holder.tree.is_traversing() {
            self.traverse(Traversal::Enter);
        }
        self.surface.damage();
    }

    /// Remembers the child each row on the focus path has focus under (#142)
    fn remember_focus(&mut self) {
        let focus = self.holder.tree.get_focus_path();
        for end in 1..=focus.0.len() {
            self.remembered.insert(
                WorldTreePath(focus.0[..end - 1].to_vec()),
                WorldTreePath(focus.0[..end].to_vec()),
            );
        }
    }

    /// Super+V: the focused row as the view root, or back to the whole tree
    fn toggle_view_root(&mut self) {
        let tree = &self.holder.tree;
        let path = if self.view_path.is_some()
            || tree.view_root_path(tree.active_screen()) != TreeNodePath::new_root()
        {
            WorldTreePath::root()
        } else {
            tree.get_focus_path()
        };
        self.set_view_root(&path);
    }

    /// Shows the row's subtree: the `NodeTree`'s view root for a node of it, else `view_path`
    fn set_view_root(&mut self, path: &WorldTreePath) {
        let (node_path, inner) = path.split_world();
        self.holder.tree.set_view_root(&node_path);
        self.view_path = inner.map(|_| path.clone());
        self.surface.damage();
    }

    /// Moves the view out until focus is inside it (eg focus went out of it), and out of gone rows
    fn update_view_path(&mut self) {
        let Some(mut view) = self.view_path.clone() else {
            return;
        };
        let focus = self.holder.tree.get_focus_path();
        let focus_moved = self.view_path_focus.as_ref() != Some(&focus);
        self.view_path_focus = Some(focus.clone());
        let treeview = self.holder.tree.get_treeview();
        while !((!focus_moved || focus.starts_with(&view)) && treeview.safe_get(&view).is_some()) {
            view.0.pop();
            if !view.is_inner() {
                self.view_path = None;
                self.holder.tree.set_view_root(&view.split_world().0);
                self.surface.damage();
                return;
            }
        }
        if self.view_path.as_ref() != Some(&view) {
            self.view_path = Some(view);
            self.surface.damage();
        }
    }

    fn damage_if_tree_changed(&mut self) {
        if self.holder.tree.take_damage() {
            self.surface.damage();
        }
    }

    /// The treeview at `area` with its pin button, framed like a pane with `always_frame` (a floating one always is)
    fn get_treeview(
        tree: &NodeTree,
        screen: usize,
        view_path: Option<&WorldTreePath>,
        treeview_pointer: &mut TreeviewPointer,
        (area, place): ([u32; 4], TreeviewPlace),
        pinned: bool,
    ) -> UITree {
        let (rows, _) = view_rows(tree, view_path, screen);
        let settings = settings::get();
        let ([x, y, width, height], border) = treeview_frame(&settings, area, place);
        let inner_width = width.saturating_sub(2 * border) as f32;
        let treeview = treeview_pointer.render(
            &rows,
            [inner_width, height.saturating_sub(2 * border) as f32],
            &tree.screen_focus_path(screen),
            tree.screen_focus_inside(screen),
        );
        // the top row's close button's place
        let size = close_width();
        let left = inner_width - size + (size - font_size()) / 2.0;
        let pin_button = Rows::new(0.0)
            .text(
                ui::text(
                    if pinned { "●" } else { "○" },
                    with_opacity(settings.palette.fg),
                ),
                left,
                left + size,
            )
            .with_pointer();
        ui::frame(
            settings.border.treeview,
            if border > 0 { settings.radius } else { 0.0 },
            border as f32,
            Style::absolute([x as f32, y as f32], [width as f32, height as f32]),
            vec![treeview, pin_button],
        )
    }
}

/// Where the treeview is drawn on a screen of `size` (`[x, y, width, height]`, frame included), and how;
/// `None` while hidden
fn treeview_area(
    settings: &Settings,
    DisplayContainerSize { width, height }: DisplayContainerSize,
    shown: bool,
    pinned: bool,
) -> Option<([u32; 4], TreeviewPlace)> {
    if !shown {
        return None;
    }
    let side = settings.treeview.side;
    let column = width * settings.treeview.width.min(100) / 100;
    let column_x = match side {
        TreeviewSide::Left => 0,
        TreeviewSide::Right => width - column,
    };
    if pinned {
        return Some(([column_x, 0, column, height], TreeviewPlace::Column(side)));
    }
    let area = match settings.treeview.floating {
        TreeviewFloating::Center => {
            let [w, h] = settings.treeview.floating_size;
            let [w, h] = [width * w.min(100) / 100, height * h.min(100) / 100];
            [(width - w) / 2, (height - h) / 2, w, h]
        }
        // in the column's place, with the gap all around
        TreeviewFloating::Side => {
            let (gap, _) = settings.gap_and_border_px();
            [
                column_x + gap,
                gap,
                column.saturating_sub(2 * gap),
                height.saturating_sub(2 * gap),
            ]
        }
    };
    Some((area, TreeviewPlace::Floating))
}

/// The applets' area's left edge and width on a screen of `size`: beside a treeview column, else everything
fn applet_area(
    treeview_area: Option<([u32; 4], TreeviewPlace)>,
    size: DisplayContainerSize,
) -> [u32; 2] {
    match treeview_area {
        Some(([_, _, width, _], TreeviewPlace::Column(TreeviewSide::Left))) => {
            [width, size.width - width]
        }
        Some(([_, _, width, _], TreeviewPlace::Column(TreeviewSide::Right))) => {
            [0, size.width - width]
        }
        Some((_, TreeviewPlace::Floating)) | None => [0, size.width],
    }
}

/// Whether `[x, y]` (in the treeview's coords, inside its frame) is on its pin button, in its top right corner
fn on_pin_button([x, y]: [f64; 2], inner_width: f64) -> bool {
    let size = f64::from(close_width());
    (inner_width - size..inner_width).contains(&x) && (0.0..size).contains(&y)
}

/// The treeview's frame (`[x, y, width, height]`, border included) in its `area`, and its border width.
/// Framed like a pane with `always_frame`: in a column the panes' gap is on the side facing them, so it gets the
/// other gaps. A floating one is always framed, as it is over the panes.
fn treeview_frame(
    settings: &Settings,
    [x, y, width, height]: [u32; 4],
    place: TreeviewPlace,
) -> ([u32; 4], u32) {
    let (gap, border) = settings.gap_and_border_px();
    match place {
        TreeviewPlace::Floating => ([x, y, width, height], border.max(1)),
        _ if !settings.always_frame => ([x, y, width, height], 0),
        TreeviewPlace::Column(TreeviewSide::Left) => (
            [
                x + gap,
                y + gap,
                width.saturating_sub(gap),
                height.saturating_sub(2 * gap),
            ],
            border,
        ),
        TreeviewPlace::Column(TreeviewSide::Right) => (
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

/// The treeview's rows: the view root's subtree (`view_path`, else the `NodeTree`'s),
/// under a `‹` row for its parent with `up`.
/// A line of text framed in the middle of `size` (eg asking to confirm quitting)
fn notice(settings: &Settings, text: &str, size: DisplayContainerSize) -> UITree {
    let pad = font_size();
    // monospace: about this wide a character
    let width = text.chars().count() as f32 * font_size() * 0.6 + 2.0 * pad;
    // room for descenders
    let line = font_size() * 1.4;
    let height = line + 2.0 * pad;
    let top = (size.height as f32 - height) / 2.0;
    let left = (size.width as f32 - width) / 2.0;
    ui::frame(
        settings.palette.accent,
        settings.radius,
        settings.border.width,
        Style::absolute([left, top], [width, height]),
        vec![UITree::new_styled(
            UIElement::Backgrounded(with_opacity(settings.palette.bg)),
            Style::full_size(),
            vec![UITree::leaf_styled(
                ui::text(text, settings.palette.fg),
                Style::absolute([pad, pad], [width - 2.0 * pad, line]),
            )],
        )],
    )
}

fn view_tree_rows(
    tree: &NodeTree,
    view_path: Option<&WorldTreePath>,
    screen: usize,
    up: bool,
) -> Vec<TreeviewRow> {
    if let Some(view) = view_path {
        let treeview = tree.get_treeview();
        if let Some(node) = treeview.safe_get(view) {
            let mut parent = view.clone();
            parent.0.pop();
            let up = up
                .then(|| treeview.safe_get(&parent))
                .flatten()
                .map(|parent_node| (parent, parent_node.get_value().get_root_value().clone()));
            return treeview_rows(&WorldTree::World(Box::new(node.clone())), view, up);
        }
    }
    let base = WorldTreePath::from_tree_path(&tree.view_root_path(screen));
    let up = up
        .then(|| tree.view_parent(screen))
        .flatten()
        .map(|(path, label)| (WorldTreePath::from_tree_path(&path), label));
    treeview_rows(&tree.get_view_treeview(screen), &base, up)
}

/// The treeview's rows, and where its menu adds things when opened on no row (the view root)
fn view_rows(
    tree: &NodeTree,
    view_path: Option<&WorldTreePath>,
    screen: usize,
) -> (Vec<TreeviewRow>, WorldTreePath) {
    let general = view_path
        .cloned()
        .unwrap_or_else(|| WorldTreePath::from_tree_path(&tree.view_root_path(screen)));
    (view_tree_rows(tree, view_path, screen, true), general)
}

/// The root's own keybinds
enum RootKeybind {
    /// Super+?
    Help,
    /// Super+V
    ViewRoot,
    /// Alt+r
    ViewParent,
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
            'r' if modifiers == KeyModifiers::ALT => Some(Self::ViewParent),
            _ => None,
        }
    }
}

/// Whether the root or a node acts on this key, rather than a focused wl client.
/// The runner sends other keys straight to the client (#56).
pub fn is_keybind(ui_event: &UIEvent) -> bool {
    RootKeybind::of(ui_event).is_some()
        || standard_keybind_request(ui_event).is_some()
        || Traversal::of(ui_event, false).is_some()
}
