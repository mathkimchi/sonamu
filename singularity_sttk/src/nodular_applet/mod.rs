//! This is where the hierarchy stuff is implemented.

use crate::standard_keybinds::{is_dfs_swipe, standard_keybind_request};
use calloop::{EventLoop, LoopHandle, LoopSignal, channel::Sender};
use singularity_common::{
    sap::{
        client_handle::ClientHandle,
        detached::{Axis, DetachedClient, DetachedTree},
        packets::{
            AwaitedWl, NewWlSurface, NodeAction, OpenApplet, SpawnPosition, StandardEvent,
            StandardRequest, StateKind, Treeview, WlSurfaceEvent,
        },
        raw_client_initializer::RawClientInitializer,
        session::{SavedApplet, SavedTree},
        shared_state::{SharedState, StatePublisher},
    },
    settings::{self, Settings},
    utils::tree::world_tree::{
        WorldTree, WorldTreePath, world_tree_traversal::WorldTreeTraversalOperation,
    },
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
pub mod split_applet;
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
//         hook: Box<dyn singularity_sar::applet::RunnerHook>,
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
    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree;

    /// The applet's inner tree; its node shows it as is.
    fn get_treeview(&self) -> WorldTree<String>;

    /// Where focus is in the inner tree (see `Treeview::focus`); published with the treeview,
    /// so call `AppletContext::damage_treeview` when it changes
    fn get_focus_path(&self) -> WorldTreePath {
        WorldTreePath::new_into()
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

    /// Whether it gets 3 finger swipes (`is_dfs_swipe`), eg a project, to move through its own tree.
    /// Otherwise they go straight to its node, so they work in any applet.
    fn handles_dfs_swipes(&self) -> bool {
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

    /// A new wl surface while focus is inside this applet, or one it awaits (`awaiting_wl`).
    /// `Err` gives it back, and its node gets it as a child.
    fn take_wl_surface(&mut self, surface: NewWlSurface) -> Result<(), NewWlSurface> {
        Err(surface)
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
    pub fn wake(&self) {
        // the applet may have ended
        let _ = self.0.send(());
    }
}

/// An applet embedded in another applet's inner tree (`AppletContext::embed`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmbeddedId(u64);

/// Makes the applet, on the applet's own thread
pub type NodularAppletInitializer =
    Box<dyn FnOnce(AppletContext) -> Box<dyn StandardApplet> + Send>;

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
    /// From the perspective of the applet's node
    ChangeFocus(WorldTreeTraversalOperation),
    /// Focuses inside the applet's node, and its holder's node and so on up to the root
    FocusSelf,
    /// Closes the applet's node; its children move to the parent
    Close,
    /// Swaps the applet's node (with its subtree) with the sibling this many places over (-1: previous)
    SwapSibling(isize),
    /// Replaces the applet's node with a split showing it and a new applet next to it
    Split {
        axis: Axis,
        new_pane: NodularAppletInitializer,
    },
    /// Replaces the applet's node and everything under it with a node whose applet is made from them
    /// (eg a project holding them as its inner tree)
    Wrap(WrapInitializer),
    /// Replaces the applet's node with `tree`, ending the applet (eg a project's contents, when it dissolves)
    Unwrap(DetachedTree),
    /// Starts a program without waiting for it (eg a wl client)
    Run(&'static str),
    /// A wl surface the applet didn't take (`StandardApplet::take_wl_surface`), as a child of its node
    AddWlSurface(NewWlSurface),
    /// An applet to open the applet didn't take (`StandardApplet::take_open`), as a child of its node
    Open(OpenApplet),
    /// Asks for the working directory of the tree the applet is in, answered with `StandardEvent::Directory`
    GetDirectory,
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

    fn new_embedded_id(&self) -> EmbeddedId {
        let id = self.next_embedded_id.get();
        self.next_embedded_id.set(id + 1);
        EmbeddedId(id)
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

    pub fn change_focus(&self, operation: WorldTreeTraversalOperation) {
        self.request(NodeRequest::ChangeFocus(operation));
    }

    /// Quits everything
    pub fn quit(&self) {
        self.request(StandardRequest::Quit.into());
    }

    /// NOTE: look at 2026-03-08 for more detail on future things
    pub fn register_applet_spawner(&self, name: String, applet_spawner: AppletSpawner) {
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
}

impl AppletState {
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
                calloop::channel::Event::Msg(StandardEvent::DamageAck(StateKind::Surface)) => {
                    state.surface.ack();
                }
                calloop::channel::Event::Msg(StandardEvent::DamageAck(StateKind::Treeview)) => {
                    state.treeview.ack();
                }
                // `None`: at the focus
                calloop::channel::Event::Msg(StandardEvent::Pluck(path)) => {
                    let path = path.unwrap_or_else(|| state.applet.get_focus_path());
                    if let Answer::Now(plucked) = state.applet.pluck(&path) {
                        let _ = state
                            .request_sender
                            .send(StandardRequest::Plucked(plucked).into());
                    }
                }
                calloop::channel::Event::Msg(StandardEvent::Place(path, position, tree)) => {
                    let path = path.unwrap_or_else(|| state.applet.get_focus_path());
                    if let Answer::Now(placed) = state.applet.place(&path, position, tree) {
                        let _ = state
                            .request_sender
                            .send(StandardRequest::Placed(placed.err()).into());
                    }
                }
                calloop::channel::Event::Msg(StandardEvent::Focus(path)) => {
                    state.applet.focus(&path);
                }
                calloop::channel::Event::Msg(StandardEvent::ToggleCollapsed(path)) => {
                    state.applet.toggle_collapsed(&path);
                }
                calloop::channel::Event::Msg(StandardEvent::NodeAction(path, action)) => {
                    state.applet.node_action(&path, action);
                }
                calloop::channel::Event::Msg(StandardEvent::SettingsChanged(settings)) => {
                    state.settings_changed(&settings);
                }
                calloop::channel::Event::Msg(StandardEvent::CloseRequest) => {
                    if state.applet.close_requested() {
                        let _ = state.request_sender.send(NodeRequest::Close);
                    }
                }
                calloop::channel::Event::Msg(StandardEvent::WlSurfaceRegistered(surface)) => {
                    if let Err(surface) = state.applet.take_wl_surface(surface) {
                        let _ = state
                            .request_sender
                            .send(NodeRequest::AddWlSurface(surface));
                    }
                }
                calloop::channel::Event::Msg(StandardEvent::Open(open)) => {
                    if let Err(open) = state.applet.take_open(open) {
                        let _ = state.request_sender.send(NodeRequest::Open(open));
                    }
                }
                calloop::channel::Event::Msg(StandardEvent::UIEvent(ui_event))
                    if is_dfs_swipe(&ui_event) && !state.applet.handles_dfs_swipes() =>
                {
                    if let Some(request) = standard_keybind_request(&ui_event) {
                        let _ = state.request_sender.send(request);
                    }
                }
                calloop::channel::Event::Msg(event) => {
                    if let StandardEvent::UIEvent(UIEvent::WindowResized(size)) = event {
                        state.size = size;
                        state.surface.damage();
                    }
                    state.applet.handle_standard_event(event);
                }
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

        let context = AppletContext {
            request_sender: request_sender.clone(),
            damage_sender: damage_tx,
            wake_sender: wake_tx,
            applet_spawners: self.applet_spawners,
            event_loop: event_loop.handle(),
            next_embedded_id: Rc::default(),
        };
        let mut state = AppletState {
            applet: (self.initializer)(context),
            // until the node sends a size
            size: DisplayContainerSize::new(800, 600),
            surface: StatePublisher::new(surface),
            treeview: StatePublisher::new(treeview),
            request_sender,
            loop_signal: event_loop.get_signal(),
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
                        saved: state.applet.save(),
                        awaiting_wl: state.applet.awaiting_wl(),
                    },
                    &state.request_sender,
                );
            })
            .unwrap();
    }
}
