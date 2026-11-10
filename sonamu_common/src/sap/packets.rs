use crate::{
    command::{CommandResult, NodeUid, Value},
    sap::{detached::DetachedTree, session::SavedApplet},
    settings::Settings,
    utils::tree::world_tree::{WorldTree, WorldTreePath},
};
use slotmap::KeyData;
use sonamu_ui::{
    display_units::DisplayContainerSize,
    ui_event::{MouseEvent, UIEvent},
};

/// A piece of client state the server reads through a `SharedState`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateKind {
    /// Currently the whole surface
    Surface,
    Treeview,
}

/// Where a placed tree goes among the children of the node at the place path
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpawnPosition {
    #[default]
    AppendChild,
    /// Before the child at this index (appended if it's past the end)
    InsertChild { child_index: usize },
    /// Before the child at this index of the node's inner root (`Step::Inner`), so it goes to the node's applet.
    /// A node without a running applet appends it as a child.
    InsertInner { child_index: usize },
}

/// Something done to a node from the treeview (eg its close button, or its right-click menu)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeAction {
    /// Asks its applet to close, like Ctrl+W on it; its children take its place
    Close,
    /// Asks it and everything under it to close
    CloseWithChildren,
    /// Stops its applet (a wl client, or a native applet and everything inside it), leaving the node inactive
    /// until it is focused again (like a restored wl tile)
    Deactivate,
    /// Wraps it and everything under it in a project, like Super+P on it
    MakeProject,
    /// Adds an applet as its last child, or as its next sibling if `sibling`
    Spawn { applet: NewApplet, sibling: bool },
}

/// What `NodeAction::Spawn` can add
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewApplet {
    FileManager,
    /// An empty one
    Project,
    Terminal,
    Browser,
}

/// A client's inner tree and where focus is in it, published as `StateKind::Treeview`
#[derive(Clone)]
pub struct Treeview {
    pub tree: WorldTree<String>,
    /// A path of `tree`, from its root (the client itself)
    pub focus: WorldTreePath,
    /// The client is selecting a node in its tree (eg a project's own treeview selector),
    /// rather than taking input itself
    pub traversing: bool,
    /// It frames what it shows itself (eg a nested tree framing its own panes), so its holder doesn't frame it
    pub frames_itself: bool,
    /// New children of its node go inside it (eg a project's panes), so its holder passes them in (#83)
    pub holds_children: bool,
    /// How to make the client again on the next start; `None` if it isn't saved (eg the help)
    pub saved: Option<SavedApplet>,
    /// Restored wl clients inside the client whose tiles wait for their toplevel (eg in a project)
    pub awaiting_wl: Vec<AwaitedWl>,
    /// Its surface is one window per screen (a container of them, in `ScreenViews` order), each shown on its screen
    pub screen_windows: bool,
    /// With `screen_windows`, `frames_itself` for each screen's window (eg a project's overview isn't framed)
    pub screen_frames_itself: Vec<bool>,
    /// With `screen_windows`, `focus` on each screen (eg a project's per-screen focus)
    pub screen_focus: Vec<WorldTreePath>,
    /// Paths of `tree` whose children are all shown at once: a swipe between them moves the
    /// focus with the fingers rather than the content (#141)
    pub shown_at_once: Vec<WorldTreePath>,
    /// Where going in (4 fingers down) goes from its focus, in its inner tree; `None`: nowhere (#145)
    pub zoom_focus: Option<WorldTreePath>,
}
impl Treeview {
    /// Focus inside the root, not traversing, not saved
    pub fn new(tree: WorldTree<String>) -> Self {
        Self {
            tree,
            focus: WorldTreePath::root(),
            traversing: false,
            frames_itself: false,
            holds_children: false,
            saved: None,
            awaiting_wl: Vec::new(),
            screen_windows: false,
            screen_frames_itself: Vec::new(),
            screen_focus: Vec::new(),
            shown_at_once: Vec::new(),
            zoom_focus: None,
        }
    }
}

/// A restored wl client's tile, waiting for the client's toplevel
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AwaitedWl {
    /// Of the process it started
    pub pid: u32,
    /// For clients that hand the window to a running instance (eg a browser), so it comes from another pid
    pub app_id: Option<String>,
}

/// A new wl toplevel, registered on its first commit (once it has set its app id)
#[derive(Debug)]
pub struct NewWlSurface {
    pub surface_id: WlSurfaceId,
    pub wl_request_queue: calloop::channel::Sender<(WlSurfaceId, WlSurfaceRequest)>,
    /// From the compositor to the surface's applet, wherever it is in the tree
    pub wl_event_queue: calloop::channel::Channel<WlSurfaceEvent>,
    /// Of the client, if known
    pub pid: Option<u32>,
    pub app_id: Option<String>,
}

/// Client to server
#[derive(Debug)]
pub enum StandardRequest {
    /// The client published a new value of this state
    Damage(StateKind),
    Quit,
    /// The wl surface Sonamu focus is on (`None` if it's on something else), sent when it changes.
    /// Keyboard (and so clipboard and text input) focus follows it.
    WlKeyboardFocus(Option<WlSurfaceId>),
    /// Answers `StandardEvent::Pluck`; `None` if nothing there can be plucked
    Plucked(Option<DetachedTree>),
    /// Answers `StandardEvent::Place`; gives the tree back if it wasn't placed
    Placed(Option<DetachedTree>),
    /// Asks the server to pluck at this path of the client's treeview (eg a drag started there)
    RequestPluck(WorldTreePath),
    /// Asks the server to place what it plucked at this path of the client's treeview
    RequestPlace(WorldTreePath, SpawnPosition),
    /// Asks for the settings, answered with `StandardEvent::SettingsChanged`
    GetSettings,
}

slotmap::new_key_type! {
    pub struct WlSurfaceId;
}
impl WlSurfaceId {
    pub fn as_u64(self) -> u64 {
        self.0.as_ffi()
    }

    /// Not doing `impl From ...` bc I'm lazy and it lowkey looks ugly
    pub fn from_u64(data: u64) -> Self {
        Self(KeyData::from_ffi(data))
    }
}

/// Server to client
#[derive(Debug)]
pub enum StandardEvent {
    UIEvent(UIEvent),
    FocusChanged(bool),
    Highlighted(bool),
    // // Is a UI event already
    // Resize(DisplayContainerSize),
    CloseRequest,
    /// Its holder is about to drop it inactive: stop what it holds (eg the wl clients in its inner tree)
    Deactivate,
    /// The server read this state, so the client may publish a new value
    DamageAck(StateKind),

    /// NOTE: ts is kinda niche; dk if it belongs here
    /// Currently doing by surface instead of by wl client
    WlSurfaceRegistered(NewWlSurface),
    /// Take out the closest pluckable element at this path of the client's treeview
    /// (`None`: at its focus), and answer with `StandardRequest::Plucked`
    Pluck(Option<WorldTreePath>),
    /// Take ownership of the tree at this path (`None`: at its focus),
    /// and answer with `StandardRequest::Placed`
    Place(Option<WorldTreePath>, SpawnPosition, DetachedTree),
    /// Focus the element at this path of the client's treeview (eg it was clicked in the root's treeview)
    Focus(WorldTreePath),
    /// Collapse or expand the element at this path of the client's treeview (eg its `▾` was clicked in the root's treeview)
    ToggleCollapsed(WorldTreePath),
    /// Do this to the element at this path of the client's treeview (eg its close button was clicked in the root's treeview)
    NodeAction(WorldTreePath, NodeAction),
    /// A wl client started (`true`) or stopped capturing the screen (eg recording). Only the root gets these.
    Capturing(bool),
    /// The settings, when they change or were asked for (`StandardRequest::GetSettings`)
    SettingsChanged(Settings),
    /// The working directory of the tree the client is in (`None`: the runner's),
    /// when it changes or was asked for (eg a project's directory)
    Directory(Option<std::path::PathBuf>),
    /// Open an applet where focus is, like a new wl surface (eg from `sonamu-msg`)
    Open(OpenApplet),
    /// The screens' areas (`[x, y, width, height]`) in the client's window, each showing the tree
    /// with its own focus (#60). Only the root gets these, after each `WindowResized`.
    Screens(Vec<[u32; 4]>),
    /// The size the client is shown at on each screen (`None` where it isn't), and which screen is active (#60),
    /// when either changes. A client can show each screen its own window (`Treeview::screen_windows`).
    ScreenViews {
        sizes: Vec<Option<DisplayContainerSize>>,
        active: usize,
    },
    /// Show this swipe between elements of the client's treeview instead of its focus (#141), or stop (`None`).
    /// Only the holder of both elements' world draws it; others pass it in or ignore it.
    Swipe(Option<SwipeView>),
    /// Its node's uid (`AppletContext::uid`), when the node is made or moves into a world
    NodeUid(NodeUid),
    /// Run the applet's command (`StandardApplet::command`) and answer on `reply`
    Command {
        name: String,
        args: Vec<Value>,
        reply: std::sync::mpsc::Sender<CommandResult>,
    },
}

/// A swipe partway between two elements (#141): `from` is drawn moved `progress` of the way toward `to`,
/// which follows it in (below for `vertical`, else to the right)
#[derive(Clone, Debug, PartialEq)]
pub struct SwipeView {
    pub vertical: bool,
    pub from: WorldTreePath,
    /// `None`: past the first / last element, `from` alone is pulled `progress` away (negative toward the first)
    pub to: Option<WorldTreePath>,
    pub progress: f32,
}

/// The applet registered under `applet`, made with `args`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenApplet {
    pub applet: String,
    pub args: Vec<String>,
}

/// Compositor to wl surface applet, through `NewWlSurface::wl_event_queue`
#[derive(Clone, Debug)]
pub enum WlSurfaceEvent {
    /// Focus the applet (eg for xdg-activation)
    Focus,
    /// The toplevel is gone (eg the client disconnected); the applet should close
    Destroyed,
    /// The toplevel's title changed
    Title(String),
}

/// Wl surface applet to compositor
#[derive(Clone, Copy, Debug)]
pub enum WlSurfaceRequest {
    Resize(DisplayContainerSize),
    Mouse(MouseEvent),
    /// Asks the client to close the toplevel; its tile closes once the toplevel is destroyed
    Close,
}
