//! What the root and projects share: holding a `NodeTree` of clients, and pluck and place, node requests
//! and wl surfaces on top of it.

use crate::{
    command_hub::CommandHubApplet,
    nodular_applet::{
        AppletClient, AppletSpawnerRegistry, NodeRequest, NodularAppletInitializer,
        WrapInitializer,
        node_tree::{AttachClient, NodeId, NodeTree, SpawnClient},
        project_applet::ProjectApplet,
        split_applet::SplitApplet,
    },
    standard_keybinds::{BROWSER, TERMINAL},
};
use calloop::LoopHandle;
use singularity_common::{
    sap::{
        client_handle::ClientHandle,
        detached::{DetachedClient, DetachedTree},
        packets::{
            NewApplet, NewWlSurface, NodeAction, OpenApplet, SpawnPosition, StandardEvent,
            StandardRequest, WlSurfaceId,
        },
    },
    settings,
    utils::tree::world_tree::WorldTreePath,
};

/// A pluck or place the holder is waiting on a node's applet for
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    Pluck(NodeId),
    Place(NodeId),
}

/// Runs and attaches applets on `event_loop`; their requests go to `on_request`, tagged with their node
pub fn client_spawners<Data: 'static>(
    event_loop: &LoopHandle<'static, Data>,
    applet_spawners: AppletSpawnerRegistry,
    on_request: fn(&mut Data, NodeId, NodeRequest),
) -> (SpawnClient, AttachClient) {
    let spawn_client: SpawnClient = {
        let event_loop = event_loop.clone();
        Box::new(move |id, initializer| {
            let client = Box::new(AppletClient {
                initializer,
                applet_spawners: applet_spawners.clone(),
            });
            ClientHandle::spawn_new_client(client, &event_loop, move |request, data: &mut Data| {
                on_request(data, id, request);
            })
        })
    };
    let attach_client: AttachClient = {
        let event_loop = event_loop.clone();
        Box::new(move |id, client: DetachedClient| {
            client.attach(&event_loop, move |request, data: &mut Data| {
                on_request(data, id, request);
            })
        })
    };
    (spawn_client, attach_client)
}

/// Holds a tree of nodes, for the root (whose node is the runner) or an applet like a project (whose node is in another tree).
/// Its node doesn't know: it only sees the holder's published state and requests.
///
/// Paths it is given and sends are of its tree.
pub struct NodeHolder {
    pub tree: NodeTree,
    /// Sends to the holder's node
    send: Box<dyn Fn(StandardRequest)>,
    /// Its tree is a project's contents (or a split's panes), whose root stands for the applet itself
    project: bool,
    /// The working directory of programs started in this tree (`NodeRequest::Run`); the runner's if `None`.
    /// Set with `set_directory`, so the tree's applets hear of it
    pub directory: Option<std::path::PathBuf>,
    /// For `NodeAction::Spawn` (eg a file manager)
    applet_spawners: AppletSpawnerRegistry,
    /// Where the next new wl surface goes, for a program `NodeAction::Spawn` started
    wl_target: Option<(NodeId, SpawnPosition)>,

    /// At most one at a time, since the holder's node sends one at a time
    pending: Option<Pending>,
    /// Plucked after the event batch: detaching a client inside its own request callback panics
    to_pluck: Option<NodeId>,
    /// Wrapped after the event batch, for the same reason; then focused inside if true (a split's new pane)
    to_wrap: Option<(NodeId, WrapInitializer, bool)>,
    /// Last sent with `StandardRequest::WlKeyboardFocus`
    focused_wl_surface: Option<WlSurfaceId>,
    /// A project's root was closed, so the project should give its contents back
    pub dissolve: bool,
    /// An applet asked to quit everything; the holder passes it on after the batch
    pub quit: bool,
}
impl NodeHolder {
    pub fn new(
        tree: NodeTree,
        send: Box<dyn Fn(StandardRequest)>,
        project: bool,
        applet_spawners: AppletSpawnerRegistry,
    ) -> Self {
        Self {
            tree,
            send,
            project,
            directory: None,
            applet_spawners,
            wl_target: None,
            pending: None,
            to_pluck: None,
            to_wrap: None,
            focused_wl_surface: None,
            dissolve: false,
            quit: false,
        }
    }

    /// Tells every applet in the tree (`StandardEvent::Directory`)
    pub fn set_directory(&mut self, directory: Option<std::path::PathBuf>) {
        if directory == self.directory {
            return;
        }
        self.directory = directory;
        for id in self.tree.with_main() {
            self.tree
                .send_event(id, StandardEvent::Directory(self.directory.clone()));
        }
    }

    fn send(&self, request: StandardRequest) {
        (self.send)(request);
    }

    /// A path inside the node's applet, as a path of the holder's treeview
    pub fn full_path(&self, id: NodeId, inner: WorldTreePath) -> Option<WorldTreePath> {
        let mut path = vec![self.tree.path_of(id)?];
        path.extend(inner.0);
        Some(WorldTreePath(path.into_boxed_slice()))
    }

    pub fn focus(&mut self, path: &WorldTreePath) {
        self.tree.focus_path(path);
        self.tree.wake_focused();
    }

    /// Collapses or expands what `path` points at: the node, or something inside its applet
    pub fn toggle_collapsed(&mut self, path: &WorldTreePath) {
        self.tree.toggle_collapsed_path(path);
    }

    /// Plucks at `path` (`None`: at the focus) and answers with `Plucked`.
    /// `path[0]` is the node, the rest is inside its applet.
    pub fn start_pluck(&mut self, path: Option<&WorldTreePath>) {
        let path = path.cloned().unwrap_or_else(|| self.tree.get_focus_path());
        let id = match path.0.first() {
            None => Some(self.tree.root()),
            Some(node_path) => self.tree.node_at(node_path),
        };
        let Some(id) = id else {
            self.send(StandardRequest::Plucked(None));
            return;
        };

        // the applet may have something closer; it answers with `Plucked`.
        // Without a running applet (eg a split) it would never answer.
        if path.0.len() > 1
            && self
                .tree
                .try_send_event(id, StandardEvent::Pluck(Some(inner_path(&path))))
                .is_ok()
        {
            self.pending = Some(Pending::Pluck(id));
        } else {
            self.to_pluck = Some(id);
        }
    }

    /// Run after each batch of events
    pub fn finish(&mut self) {
        if let Some(id) = self.to_pluck.take() {
            // `None` for the root
            let plucked = self.tree.pluck(id);
            self.send(StandardRequest::Plucked(plucked));
        }
        if let Some((id, wrapper, inside)) = self.to_wrap.take() {
            let directory = self.directory.clone();
            let new = self.tree.wrap(id, |subtree| wrapper(subtree, directory));
            // a split focuses its new pane
            if inside && let Some(new) = new {
                self.tree.focus(new, true);
            }
        }
    }

    /// A UI event for the tree; a standard keybind on the selected node is done as if its applet asked,
    /// except that its applet is asked to close rather than closed (eg a wl client may refuse).
    /// Returns what the holder's own node should do instead, like `handle_node_request`.
    pub fn handle_ui_event(&mut self, event: StandardEvent) -> Option<NodeRequest> {
        self.tree.handle_event(event);
        self.tree.wake_focused();
        let (id, request) = self.tree.take_keybind_request()?;
        if matches!(request, NodeRequest::Close) {
            self.request_close(id);
            return None;
        }
        self.handle_node_request(id, request)
    }

    /// Asks the node's applet to close (eg a wl client may refuse); a node without one closes now
    fn request_close(&mut self, id: NodeId) {
        if self.tree.has_main(id) {
            self.tree.send_event(id, StandardEvent::CloseRequest);
        } else {
            self.handle_node_request(id, NodeRequest::Close);
        }
    }

    /// Does the action to the node at `path[0]`, or passes it on to its applet if the path goes into it
    pub fn node_action(&mut self, path: &WorldTreePath, action: NodeAction) {
        let Some(id) = path
            .0
            .first()
            .and_then(|node_path| self.tree.node_at(node_path))
        else {
            return;
        };
        if path.0.len() > 1 {
            // so a spawned program's window is passed in there
            if matches!(action, NodeAction::Spawn { .. }) {
                self.tree.focus(id, true);
            }
            self.tree
                .send_event(id, StandardEvent::NodeAction(inner_path(path), action));
            return;
        }
        match action {
            NodeAction::Close => self.request_close(id),
            NodeAction::CloseWithChildren => {
                for id in self.tree.subtree(id) {
                    // a node without a main closes right away, so a later one may be gone
                    if self.tree.contains_node(id) {
                        self.request_close(id);
                    }
                }
            }
            NodeAction::Deactivate => self.tree.deactivate(id),
            NodeAction::MakeProject => {
                self.handle_node_request(id, NodeRequest::Wrap(ProjectApplet::wrapper()));
            }
            NodeAction::Spawn { applet, sibling } => self.spawn(id, applet, sibling),
        }
    }

    /// Adds the applet under the node or after it, focused inside.
    /// A program's window goes there when it arrives.
    fn spawn(&mut self, id: NodeId, applet: NewApplet, sibling: bool) {
        let (parent, position) = self.tree.spawn_target(id, sibling);
        let initializer = match applet {
            NewApplet::Terminal | NewApplet::Browser => {
                let program = if applet == NewApplet::Terminal {
                    TERMINAL
                } else {
                    BROWSER
                };
                if self.run(program) {
                    self.wl_target = Some((parent, position));
                }
                return;
            }
            NewApplet::CommandHub => CommandHubApplet::get_boxed_initiator(),
            NewApplet::Project => ProjectApplet::empty(self.directory.clone()),
            NewApplet::FileManager => {
                let directory = self
                    .directory
                    .as_ref()
                    .map(|directory| directory.to_string_lossy().into_owned());
                let args: Vec<&str> = directory.as_deref().into_iter().collect();
                let Some(initializer) = self.initializer("file_manager", &args) else {
                    return;
                };
                initializer
            }
        };
        if let Some(new) = self.tree.add_child_at(parent, position, initializer) {
            self.tree.focus(new, true);
        }
    }

    /// Like `add_wl_surface`: into the focused node's main if focus is inside it
    /// (it may take it or give it back with `NodeRequest::Open`), otherwise under the focused node
    pub fn open(&mut self, open: OpenApplet) {
        let id = self.tree.focused_node();
        if self.tree.is_inner(id) {
            self.tree.send_event(id, StandardEvent::Open(open));
        } else {
            self.open_under(id, &open);
        }
    }

    /// As the node's last child, focused inside. It's in this tree's directory unless `args` say otherwise.
    fn open_under(&mut self, parent: NodeId, OpenApplet { applet, args }: &OpenApplet) {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let Some(initializer) = self.initializer(applet, &args) else {
            return;
        };
        if let Some(new) = self.tree.add_child(parent, initializer) {
            self.tree.focus(new, true);
        }
    }

    /// From the applet registered under `applet`
    fn initializer(&self, applet: &str, args: &[&str]) -> Option<NodularAppletInitializer> {
        let spawner = self.applet_spawners.read().unwrap().get(applet).cloned();
        let initializer = spawner.and_then(|spawner| spawner.create_initializer(args));
        if initializer.is_none() {
            log::warn!("No {applet} to spawn with {args:?}");
        }
        initializer
    }

    /// Starts the program in the tree's directory without waiting for it; whether it started
    fn run(&self, program: &str) -> bool {
        let mut command = crate::command_hub::program_command(program, &[]);
        if let Some(directory) = &self.directory {
            command.current_dir(directory);
        }
        command
            .spawn()
            .inspect_err(|err| log::warn!("Failed to start {program}: {err}"))
            .is_ok()
    }

    /// Places at the deepest node along `path[0]` (`None`: at the focus),
    /// asking its applet first if the path goes into it
    pub fn start_place(
        &mut self,
        path: Option<&WorldTreePath>,
        position: SpawnPosition,
        mut tree: DetachedTree,
    ) {
        let path = path.cloned().unwrap_or_else(|| self.tree.get_focus_path());
        let (id, exact) = path
            .0
            .first()
            .map_or((self.tree.root(), true), |node_path| {
                self.tree.closest_node_at(node_path)
            });

        if exact && path.0.len() > 1 {
            match self.tree.try_send_event(
                id,
                StandardEvent::Place(Some(inner_path(&path)), position, tree),
            ) {
                Ok(()) => {
                    self.pending = Some(Pending::Place(id));
                    return;
                }
                // no running applet (eg a split), so the node takes it
                Err(StandardEvent::Place(_, _, given_back)) => tree = given_back,
                Err(_) => unreachable!("the event is given back as is"),
            }
        }
        self.tree.place(id, position, tree);
        self.send(StandardRequest::Placed(None));
    }

    /// To the restored tile awaiting it, if any. Otherwise into the focused node's main if focus is inside it
    /// (it may take it or give it back with `AddWlSurface`), otherwise under the focused node.
    pub fn add_wl_surface(&mut self, surface: NewWlSurface) {
        let Err(surface) = self.tree.claim_wl_surface(surface) else {
            return;
        };
        if let Some((parent, position)) = self
            .wl_target
            .take()
            .filter(|&(parent, _)| self.tree.contains_node(parent))
        {
            self.tree.add_wl_surface_at(parent, position, surface);
            return;
        }
        let id = self.tree.focused_node();
        if self.tree.is_inner(id) {
            self.tree
                .send_event(id, StandardEvent::WlSurfaceRegistered(surface));
        } else {
            self.tree.add_wl_surface(id, surface);
        }
    }

    /// Tells the holder's node if focus moved onto or off a wl surface. Run after each batch of events.
    pub fn update_wl_focus(&mut self) {
        // projects only get events while focused, and wake in those
        if !self.project {
            self.tree.wake_focused();
        }
        let focused_wl_surface = self.tree.focused_wl_surface();
        if focused_wl_surface != self.focused_wl_surface {
            self.focused_wl_surface = focused_wl_surface;
            self.send(StandardRequest::WlKeyboardFocus(focused_wl_surface));
        }
    }

    /// From the node's main. Returns what the holder's own node should do instead.
    pub fn handle_node_request(&mut self, id: NodeId, request: NodeRequest) -> Option<NodeRequest> {
        match request {
            NodeRequest::Standard(StandardRequest::Damage(_)) => self.tree.damage(),
            // eg Ctrl+Shift+Q; quits everything, after the batch (so the root can save first)
            NodeRequest::Standard(StandardRequest::Quit) => self.quit = true,
            // the main is a holder too
            NodeRequest::Standard(StandardRequest::WlKeyboardFocus(surface_id)) => {
                self.tree.set_inner_wl_focus(id, surface_id);
            }
            NodeRequest::Standard(StandardRequest::Plucked(plucked)) => {
                if self.pending != Some(Pending::Pluck(id)) {
                    log::warn!("Unexpected `Plucked` from {id:?}; dropping it");
                    return None;
                }
                self.pending = None;
                match plucked {
                    Some(plucked) => self.send(StandardRequest::Plucked(Some(plucked))),
                    // nothing inside, so the node itself
                    None => self.to_pluck = Some(id),
                }
            }
            NodeRequest::Standard(StandardRequest::Placed(given_back)) => {
                if self.pending != Some(Pending::Place(id)) {
                    log::warn!("Unexpected `Placed` from {id:?}; dropping it");
                    return None;
                }
                self.pending = None;
                // the applet couldn't, so it becomes the node's child
                if let Some(tree) = given_back {
                    self.tree.place(id, SpawnPosition::AppendChild, tree);
                }
                self.send(StandardRequest::Placed(None));
            }
            NodeRequest::Standard(StandardRequest::GetSettings) => self
                .tree
                .send_event(id, StandardEvent::SettingsChanged(settings::get())),
            // the applet's path is inside its inner tree
            NodeRequest::Standard(StandardRequest::RequestPluck(path)) => {
                if let Some(path) = self.full_path(id, path) {
                    self.send(StandardRequest::RequestPluck(path));
                }
            }
            NodeRequest::Standard(StandardRequest::RequestPlace(path, position)) => {
                if let Some(path) = self.full_path(id, path) {
                    self.send(StandardRequest::RequestPlace(path, position));
                }
            }
            NodeRequest::AddChild(initializer) => {
                self.tree.add_child(id, initializer);
            }
            NodeRequest::ChangeFocus(operation) => self.tree.change_focus(id, operation),
            // and the holder's node likewise
            NodeRequest::FocusSelf => {
                self.tree.focus(id, true);
                return Some(NodeRequest::FocusSelf);
            }
            // the root of a project's inner tree stands for the project, which closes into its contents
            NodeRequest::Close if self.project && id == self.tree.root() => self.dissolve = true,
            NodeRequest::Close => {
                self.tree.close(id);
            }
            // the project's node splits, likewise
            request @ NodeRequest::Split { .. } if self.project && id == self.tree.root() => {
                return Some(request);
            }
            NodeRequest::SwapSibling(shift) => self.tree.swap_sibling(id, shift),
            NodeRequest::Split { axis, new_pane } => {
                self.to_wrap = Some((id, SplitApplet::wrapper(axis, new_pane), true));
            }
            // it is a project already
            NodeRequest::Wrap(_) if self.project && id == self.tree.root() => {}
            NodeRequest::Wrap(wrapper) => self.to_wrap = Some((id, wrapper, false)),
            NodeRequest::Unwrap(tree) => self.tree.unwrap(id, tree),
            NodeRequest::Run(program) => {
                self.run(program);
            }
            NodeRequest::AddWlSurface(surface) => self.tree.add_wl_surface(id, surface),
            NodeRequest::Open(open) => self.open_under(id, &open),
            NodeRequest::GetDirectory => self
                .tree
                .send_event(id, StandardEvent::Directory(self.directory.clone())),
        }
        None
    }
}

/// The part of the path inside the node's applet
fn inner_path(path: &WorldTreePath) -> WorldTreePath {
    WorldTreePath(path.0.get(1..).unwrap_or_default().into())
}
