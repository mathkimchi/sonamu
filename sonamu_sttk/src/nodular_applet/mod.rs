//! This is where the hierarchy stuff is implemented.

use calloop::{EventLoop, LoopHandle, LoopSignal, channel::Sender};
use sonamu_common::{
    command::{self, CommandResult, NodeCall, NodeUid, Signature, Value, World},
    sap::{
        client_handle::ClientHandle,
        detached::{DetachedClient, DetachedTree},
        packets::{
            AwaitedWl, NewWlSurface, NodeAction, OpenApplet, SpawnPosition, StandardEvent,
            StandardRequest, StateKind, Treeview, WlSurfaceEvent,
        },
        raw_client_initializer::RawClientInitializer,
        session::{SavedApplet, SavedTree, SavedWl},
        shared_state::{SharedState, StatePublisher},
    },
    settings::{self, Settings},
    utils::tree::world_tree::{WorldTree, WorldTreePath},
};
use sonamu_ui::{display_units::DisplayContainerSize, ui_element::UITree, ui_event::UIEvent};
use std::{
    cell::Cell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, RwLock},
};

pub mod node_holder;
pub mod node_tree;
pub mod project_applet;
pub mod root_node_applet;
mod status_bar;
pub mod swipe;
mod traversal;
pub(crate) mod treeview;

pub use treeview::with_opacity;

// pub struct RootApplet<Applet: BasicApplet> {
//     inner: Applet,
//     children: NodeApplet<Box<dyn BasicApplet>>,
// }
// impl<Applet: BasicApplet> BasicApplet for RootApplet<Applet> {
//     type InitializingData = Applet::InitializingData;

//     fn initialize(
//         initializing_data: Self::InitializingData,
//         hook: Box<dyn sonamu_sar::applet::RunnerHook>,
//     ) -> Self {
//         todo!()
//     }

//     fn handle_ui_event(&mut self, ui_event: sonamu_ui::ui_event::UIEvent) {
//         todo!()
//     }
// }

/// An applet; runs on its own thread as a client of the node holding it.
pub trait StandardApplet {
    fn handle_standard_event(&mut self, standard_event: StandardEvent);

    /// Called when the surface is damaged (`AppletContext::damage_window`) or resized
    /// (or told of the screens, `StandardEvent::ScreenViews`).
    /// With `screen_windows`, a container of one window per screen.
    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree;

    /// Whether its window is one per screen (`Treeview::screen_windows`, #60)
    fn screen_windows(&self) -> bool {
        false
    }

    /// With `screen_windows`, whether each screen's window frames itself (`Treeview::screen_frames_itself`)
    fn screen_frames_itself(&self) -> Vec<bool> {
        Vec::new()
    }

    /// With `screen_windows`, where focus is on each screen (`Treeview::screen_focus`)
    fn screen_focus(&self) -> Vec<WorldTreePath> {
        Vec::new()
    }

    /// Paths of its inner tree whose children are all shown at once (`Treeview::shown_at_once`)
    fn shown_at_once(&self) -> Vec<WorldTreePath> {
        Vec::new()
    }

    /// Where going in (4 fingers down) goes from its focus, in its inner tree (`Treeview::zoom_focus`, #145)
    fn zoom_focus(&self) -> Option<WorldTreePath> {
        None
    }

    /// The applet's inner tree; its node shows it as is.
    fn get_treeview(&self) -> WorldTree<String>;

    /// Where focus is in the inner tree (see `Treeview::focus`); published with the treeview,
    /// so call `AppletContext::damage_treeview` when it changes
    fn get_focus_path(&self) -> WorldTreePath {
        WorldTreePath::root()
    }

    /// Whether it is selecting an element of its inner tree (its own treeview selector, so the root shows the treeview),
    /// rather than taking input; published with the treeview
    fn is_traversing(&self) -> bool {
        false
    }

    /// Whether it frames what it shows itself (eg its own tree's panes), so its node isn't framed;
    /// published with the treeview
    fn frames_itself(&self) -> bool {
        false
    }

    /// Puts focus at the element at `path` (in the inner tree), eg it was clicked in the root's treeview
    /// or `e` went into this applet (`[[]]`, its inner root)
    fn focus(&mut self, _path: &WorldTreePath) {}

    /// Collapses or expands the element at `path` (in the inner tree) in the treeview, eg its `▾` was clicked
    fn toggle_collapsed(&mut self, _path: &WorldTreePath) {}

    /// Does the action to the element at `path` (in the inner tree), eg its close button was clicked
    fn node_action(&mut self, _path: &WorldTreePath, _action: NodeAction) {}

    /// Takes the closest pluckable element at `path` (in the inner tree) out of the applet.
    /// `Now(None)` makes its node pluck itself, with the applet and its children.
    /// `Later`: the applet answers itself with `StandardRequest::Plucked` (eg after asking an applet inside it).
    fn pluck(&mut self, _path: &WorldTreePath) -> Answer<Option<DetachedTree>> {
        Answer::Now(None)
    }

    /// Takes ownership of `tree` at `path` (in the inner tree).
    /// `position` is among the children of the node at `path`.
    /// `Now(Err)` gives it back, and the node adds it as its last child.
    /// `Later`: the applet answers itself with `StandardRequest::Placed`.
    fn place(
        &mut self,
        _path: &WorldTreePath,
        _position: SpawnPosition,
        tree: DetachedTree,
    ) -> Answer<Result<(), DetachedTree>> {
        Answer::Now(Err(tree))
    }

    /// Its node was asked to close it (eg Ctrl+W on its node in the treeview).
    /// Returns whether it closes now; it may refuse, or close later.
    fn close_requested(&mut self) -> bool {
        true
    }

    /// Its node is about to drop it inactive (`NodeAction::Deactivate`, restored from its `save` when focused);
    /// stop what it holds that would outlive it, eg the wl clients in its inner tree
    fn deactivate(&mut self) {}

    /// A new wl surface while focus is inside this applet, or one it awaits (`awaiting_wl`).
    /// `Err` gives it back, and its node gets it as a child.
    fn take_wl_surface(&mut self, surface: NewWlSurface) -> Result<(), Box<NewWlSurface>> {
        Err(Box::new(surface))
    }

    /// An applet to open while focus is inside this applet (`StandardEvent::Open`), like `take_wl_surface`
    fn take_open(&mut self, open: OpenApplet) -> Result<(), OpenApplet> {
        Err(open)
    }

    /// How to make it again on the next start (see `.claude/SESSIONS.md`); `None` isn't saved.
    /// Published with the treeview, so call `AppletContext::damage_treeview` when it changes
    fn save(&self) -> Option<SavedApplet> {
        None
    }

    /// Restored wl clients inside it waiting for their toplevel; published with the treeview
    fn awaiting_wl(&self) -> Vec<AwaitedWl> {
        Vec::new()
    }

    /// A request from an applet this applet embedded (`AppletContext::embed`)
    fn handle_embedded_request(&mut self, _id: EmbeddedId, _request: NodeRequest) {}

    /// From the compositor, for a surface the applet listens to (`AppletContext::listen_wl_surface`)
    fn handle_wl_surface_event(&mut self, _event: WlSurfaceEvent) {}

    /// Called on the applet's thread after an `AppletWaker` woke it (eg a worker thread finished)
    fn wake(&mut self) {}

    /// The settings, once before the first draw and whenever they change (`StandardEvent::SettingsChanged`);
    /// the surface is damaged afterwards
    fn settings_changed(&mut self, _settings: &Settings) {}

    /// Called after each batch of events, before the applet's state is published
    fn update(&mut self) {}

    /// Runs one of its commands (`StandardEvent::Command`, see `.claude/COMMANDS.md`)
    fn command(&mut self, name: &str, _args: Vec<Value>) -> CommandResult {
        Err(command::unknown(name))
    }

    /// A command for a node of the world it holds (`AppletContext::world`), eg a project's
    fn handle_node_call(&mut self, _call: NodeCall) {}
}

/// An answer to a request, now or sent later by the applet itself
pub enum Answer<T> {
    Now(T),
    Later,
}

/// Wakes an applet from another thread (`StandardApplet::wake`); from `AppletContext::waker`
#[derive(Clone)]
pub struct AppletWaker(Sender<()>);
impl AppletWaker {
    /// `false` if the applet has ended
    pub fn wake(&self) -> bool {
        self.0.send(()).is_ok()
    }
}

/// An applet embedded in another applet's inner tree (`AppletContext::embed`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmbeddedId(u64);

/// Makes the applet, on the applet's own thread
pub struct NodularAppletInitializer {
    init: Box<dyn FnOnce(AppletContext) -> Box<dyn StandardApplet> + Send>,
    /// New children of its node go inside it instead (eg a project's contents): a keybind, menu, drop,
    /// opened applet or wl window that would add one to its node is passed in, at its root (#83).
    /// Known before it runs, so its holder never adds a child to its node while it starts.
    pub holds_children: bool,
}
impl NodularAppletInitializer {
    pub fn new(
        init: impl FnOnce(AppletContext) -> Box<dyn StandardApplet> + Send + 'static,
    ) -> Self {
        Self {
            init: Box::new(init),
            holds_children: false,
        }
    }

    /// Like `new`, with `holds_children`
    pub fn holding_children(
        init: impl FnOnce(AppletContext) -> Box<dyn StandardApplet> + Send + 'static,
    ) -> Self {
        Self {
            holds_children: true,
            ..Self::new(init)
        }
    }

    pub fn init(self, context: AppletContext) -> Box<dyn StandardApplet> {
        (self.init)(context)
    }
}

pub trait AppletSpawnerTrait: Send + Sync {
    fn create_initializer(&self, args: &[&str]) -> Option<NodularAppletInitializer>;

    /// Makes a saved applet again that saved its inner tree too (`session::saved_with_inner`)
    fn restore_initializer(
        &self,
        args: &[&str],
        _inner: SavedTree,
    ) -> Option<NodularAppletInitializer> {
        self.create_initializer(args)
    }

    /// Its applets' commands (`StandardApplet::command`), for completion and help
    fn commands(&self) -> &'static [Signature] {
        &[]
    }

    /// Clone on its own is not dyn compatible because it outputs -> Self.
    /// Using the AppletSpawnerTrait, AppletSpawner workaround for that.
    fn duplicate(&self) -> AppletSpawner;
}
pub type AppletSpawner = Box<dyn AppletSpawnerTrait>;

impl Clone for AppletSpawner {
    fn clone(&self) -> Self {
        self.duplicate()
    }
}

pub type AppletSpawnerRegistry = Arc<RwLock<BTreeMap<String, AppletSpawner>>>;

/// Applet to its node
pub enum NodeRequest {
    Standard(StandardRequest),
    /// The child gets its own node under the applet's node
    AddChild(NodularAppletInitializer),
    /// Focuses inside the applet's node, and its holder's node and so on up to the root
    FocusSelf,
    /// Closes the applet's node; its children move to the parent
    Close,
    /// Swaps the applet's node (with its subtree) with the sibling this many places over (-1: previous)
    SwapSibling(isize),
    /// Toggles the applet's node between half and full width (#144)
    ToggleHalf,
    /// Replaces the applet's node and everything under it with a node whose applet is made from them
    /// (eg a project holding them as its inner tree)
    Wrap(WrapInitializer),
    /// Replaces the applet's node with `tree`, ending the applet (eg a project's contents, when it dissolves)
    Unwrap(DetachedTree),
    /// Starts a program without waiting for it (eg a wl client)
    Run(String),
    /// Like `Run`, but its window goes right after the applet's node
    RunSibling(String),
    /// A wl surface the applet didn't take (`StandardApplet::take_wl_surface`), as a child of its node
    AddWlSurface(NewWlSurface),
    /// An applet to open the applet didn't take (`StandardApplet::take_open`), as a child of its node
    Open(OpenApplet),
    /// Asks for the working directory of the tree the applet is in, answered with `StandardEvent::Directory`
    GetDirectory,
    /// Starts the saved wl client in the applet's node, in place of the applet (eg a prompt to restore it)
    StartWl(SavedWl),
    /// Runs this applet in the applet's node in its place, keeping the node's children (eg the command hub's `open`)
    Replace(NodularAppletInitializer),
}

/// Makes an applet from a detached subtree (`NodeRequest::Wrap`) and the directory of the tree it is in
/// (`NodeHolder::directory`)
pub type WrapInitializer =
    Box<dyn FnOnce(DetachedTree, Option<std::path::PathBuf>) -> NodularAppletInitializer + Send>;
impl From<StandardRequest> for NodeRequest {
    fn from(request: StandardRequest) -> Self {
        Self::Standard(request)
    }
}

/// What an applet uses to talk to its node (replaces the old runner hooks).
///
/// Can be cloned, but stays on the applet's thread (it holds the applet's loop, for `embed`).
#[derive(Clone)]
pub struct AppletContext {
    request_sender: Sender<NodeRequest>,
    /// To the applet's own loop, which publishes the damaged state
    damage_sender: Sender<StateKind>,
    /// To the applet's own loop, which calls `StandardApplet::wake`
    wake_sender: Sender<()>,
    applet_spawners: AppletSpawnerRegistry,
    /// The applet's own loop, which embedded applets' requests are handled on
    event_loop: LoopHandle<'static, AppletState>,
    next_embedded_id: Rc<Cell<u64>>,
    /// Its node's, once told (`StandardEvent::NodeUid`)
    uid: Rc<Cell<Option<NodeUid>>>,
}
impl AppletContext {
    /// Runs an applet that this applet holds itself, in its inner tree.
    /// Its requests go to `StandardApplet::handle_embedded_request`; dropping the handle ends it.
    pub fn embed(&self, initializer: NodularAppletInitializer) -> (EmbeddedId, ClientHandle) {
        let id = self.new_embedded_id();
        let handle = ClientHandle::spawn_new_client(
            Box::new(AppletClient {
                initializer,
                applet_spawners: self.applet_spawners.clone(),
            }),
            &self.event_loop,
            move |request, state: &mut AppletState| {
                state.applet.handle_embedded_request(id, request);
            },
        );
        (id, handle)
    }

    /// Like `embed`, for a placed client that is already running
    pub fn embed_detached(&self, client: DetachedClient) -> (EmbeddedId, ClientHandle) {
        let id = self.new_embedded_id();
        let handle = client.attach(&self.event_loop, move |request, state: &mut AppletState| {
            state.applet.handle_embedded_request(id, request);
        });
        (id, handle)
    }

    /// Its events go to `StandardApplet::handle_wl_surface_event`
    pub fn listen_wl_surface(&self, wl_event_queue: calloop::channel::Channel<WlSurfaceEvent>) {
        self.event_loop
            .insert_source(wl_event_queue, |event, &mut (), state| {
                if let calloop::channel::Event::Msg(event) = event {
                    state.applet.handle_wl_surface_event(event);
                }
            })
            .unwrap();
    }

    /// For a world the applet holds (eg a project's tree): its commands go to `StandardApplet::handle_node_call`
    pub fn world(&self) -> World {
        let (world, calls) = calloop::channel::channel();
        self.event_loop
            .insert_source(calls, |event, &mut (), state| {
                if let calloop::channel::Event::Msg(call) = event {
                    state.applet.handle_node_call(call);
                }
            })
            .unwrap();
        world
    }

    fn new_embedded_id(&self) -> EmbeddedId {
        let id = self.next_embedded_id.get();
        self.next_embedded_id.set(id + 1);
        EmbeddedId(id)
    }

    /// Its node's uid (for commands), once its holder told it
    pub fn uid(&self) -> Option<NodeUid> {
        self.uid.get()
    }

    pub fn request(&self, request: NodeRequest) {
        // the node may already be gone while closing
        let _ = self.request_sender.send(request);
    }

    pub fn damage_window(&self) {
        let _ = self.damage_sender.send(StateKind::Surface);
    }

    pub fn damage_treeview(&self) {
        let _ = self.damage_sender.send(StateKind::Treeview);
    }

    /// For other threads, since the context stays on the applet's thread
    pub fn waker(&self) -> AppletWaker {
        AppletWaker(self.wake_sender.clone())
    }

    /// Answered with `StandardEvent::Directory`
    pub fn get_directory(&self) {
        self.request(NodeRequest::GetDirectory);
    }

    pub fn close(&self) {
        self.request(NodeRequest::Close);
    }

    pub fn add_child(&self, initializer: NodularAppletInitializer) {
        self.request(NodeRequest::AddChild(initializer));
    }

    /// Quits everything
    pub fn quit(&self) {
        self.request(StandardRequest::Quit.into());
    }

    /// NOTE: look at 2026-03-08 for more detail on future things
    pub fn register_applet_spawner(&self, name: String, applet_spawner: AppletSpawner) {
        command::declare(&name, applet_spawner.commands());
        self.applet_spawners
            .write()
            .unwrap()
            .insert(name, applet_spawner);
    }
    pub fn get_applet_spawners(&self) -> BTreeMap<String, AppletSpawner> {
        self.applet_spawners.read().unwrap().clone()
    }
    pub fn find_applet_spawner(&self, name: &str) -> Option<AppletSpawner> {
        self.applet_spawners.read().unwrap().get(name).cloned()
    }
}

/// The applet's event loop data
pub struct AppletState {
    applet: Box<dyn StandardApplet>,
    size: DisplayContainerSize,
    surface: StatePublisher<UITree>,
    treeview: StatePublisher<Treeview>,
    request_sender: Sender<NodeRequest>,
    loop_signal: LoopSignal,
    /// Shared with its `AppletContext`
    uid: Rc<Cell<Option<NodeUid>>>,
}

impl AppletState {
    fn handle_event(&mut self, event: StandardEvent) {
        match event {
            StandardEvent::DamageAck(StateKind::Surface) => {
                self.surface.ack();
            }
            StandardEvent::DamageAck(StateKind::Treeview) => {
                self.treeview.ack();
            }
            // `None`: at the focus
            StandardEvent::Pluck(path) => {
                let path = path.unwrap_or_else(|| self.applet.get_focus_path());
                if let Answer::Now(plucked) = self.applet.pluck(&path) {
                    let _ = self
                        .request_sender
                        .send(StandardRequest::Plucked(plucked).into());
                }
            }
            StandardEvent::Place(path, position, tree) => {
                let path = path.unwrap_or_else(|| self.applet.get_focus_path());
                if let Answer::Now(placed) = self.applet.place(&path, position, tree) {
                    let _ = self
                        .request_sender
                        .send(StandardRequest::Placed(placed.err()).into());
                }
            }
            StandardEvent::Focus(path) => {
                self.applet.focus(&path);
            }
            StandardEvent::ToggleCollapsed(path) => {
                self.applet.toggle_collapsed(&path);
            }
            StandardEvent::NodeAction(path, action) => {
                self.applet.node_action(&path, action);
            }
            StandardEvent::SettingsChanged(settings) => {
                self.settings_changed(&settings);
            }
            StandardEvent::CloseRequest => {
                if self.applet.close_requested() {
                    let _ = self.request_sender.send(NodeRequest::Close);
                }
            }
            StandardEvent::Deactivate => self.applet.deactivate(),
            StandardEvent::NodeUid(uid) => self.uid.set(Some(uid)),
            StandardEvent::Command { name, args, reply } => {
                let _ = reply.send(self.applet.command(&name, args));
            }
            StandardEvent::WlSurfaceRegistered(surface) => {
                if let Err(surface) = self.applet.take_wl_surface(surface) {
                    let _ = self
                        .request_sender
                        .send(NodeRequest::AddWlSurface(*surface));
                }
            }
            StandardEvent::Open(open) => {
                if let Err(open) = self.applet.take_open(open) {
                    let _ = self.request_sender.send(NodeRequest::Open(open));
                }
            }
            event => {
                if let StandardEvent::UIEvent(UIEvent::WindowResized(size)) = event {
                    self.size = size;
                    self.surface.damage();
                }
                if let StandardEvent::ScreenViews { .. } = event {
                    self.surface.damage();
                    self.treeview.damage();
                }
                self.applet.handle_standard_event(event);
            }
        }
    }

    fn settings_changed(&mut self, settings: &Settings) {
        self.applet.settings_changed(settings);
        self.surface.damage();
    }
}

/// Runs a `StandardApplet` as a client of a node
pub(crate) struct AppletClient {
    pub initializer: NodularAppletInitializer,
    pub applet_spawners: AppletSpawnerRegistry,
}
impl RawClientInitializer<NodeRequest> for AppletClient {
    fn holds_children(&self) -> bool {
        self.initializer.holds_children
    }

    fn init(
        self: Box<Self>,
        surface: SharedState<UITree>,
        treeview: SharedState<Treeview>,
        event_queue: calloop::channel::Channel<StandardEvent>,
        request_sender: Sender<NodeRequest>,
    ) {
        let mut event_loop: EventLoop<AppletState> = EventLoop::try_new().unwrap();
        let (damage_tx, damage_rx) = calloop::channel::channel();

        event_loop
            .handle()
            .insert_source(event_queue, |event, &mut (), state| match event {
                calloop::channel::Event::Msg(event) => state.handle_event(event),
                // the node dropped this applet's `ClientHandle`
                calloop::channel::Event::Closed => state.loop_signal.stop(),
            })
            .unwrap();
        event_loop
            .handle()
            .insert_source(damage_rx, |event, &mut (), state| {
                if let calloop::channel::Event::Msg(kind) = event {
                    match kind {
                        StateKind::Surface => state.surface.damage(),
                        StateKind::Treeview => state.treeview.damage(),
                    }
                }
            })
            .unwrap();
        let (settings_tx, settings_rx) = calloop::channel::channel();
        settings::subscribe(settings_tx);
        event_loop
            .handle()
            .insert_source(settings_rx, |event, &mut (), state| {
                if let calloop::channel::Event::Msg(()) = event {
                    state.settings_changed(&settings::get());
                }
            })
            .unwrap();
        let (wake_tx, wake_rx) = calloop::channel::channel();
        event_loop
            .handle()
            .insert_source(wake_rx, |event, &mut (), state| {
                if let calloop::channel::Event::Msg(()) = event {
                    state.applet.wake();
                }
            })
            .unwrap();

        let uid = Rc::default();
        let context = AppletContext {
            request_sender: request_sender.clone(),
            damage_sender: damage_tx,
            wake_sender: wake_tx,
            applet_spawners: self.applet_spawners,
            event_loop: event_loop.handle(),
            next_embedded_id: Rc::default(),
            uid: Rc::clone(&uid),
        };
        let holds_children = self.initializer.holds_children;
        let mut state = AppletState {
            applet: self.initializer.init(context),
            // until the node sends a size
            size: DisplayContainerSize::new(800, 600),
            surface: StatePublisher::new(surface),
            treeview: StatePublisher::new(treeview),
            request_sender,
            loop_signal: event_loop.get_signal(),
            uid,
        };
        state.settings_changed(&settings::get());

        // after each batch of events, so multiple damages become one publish
        event_loop
            .run(None, &mut state, |state| {
                state.applet.update();
                state.surface.flush(
                    || state.applet.get_window_standard_applet(state.size),
                    &state.request_sender,
                );
                state.treeview.flush(
                    || Treeview {
                        tree: state.applet.get_treeview(),
                        focus: state.applet.get_focus_path(),
                        traversing: state.applet.is_traversing(),
                        frames_itself: state.applet.frames_itself(),
                        holds_children,
                        saved: state.applet.save(),
                        awaiting_wl: state.applet.awaiting_wl(),
                        screen_windows: state.applet.screen_windows(),
                        screen_frames_itself: state.applet.screen_frames_itself(),
                        screen_focus: state.applet.screen_focus(),
                        shown_at_once: state.applet.shown_at_once(),
                        zoom_focus: state.applet.zoom_focus(),
                    },
                    &state.request_sender,
                );
            })
            .unwrap();
    }
}
