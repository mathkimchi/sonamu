use crate::{
    creatable_applet::CreatableNodularApplet,
    nodular_applet::{
        NodeRequest, NodularAppletInitializer, swipe, traversal::Traversal, treeview::BTN_LEFT,
    },
    session,
    standard_keybinds::standard_keybind_request,
    ui,
    wl_surface_applet::WlSurfaceApplet,
};
use slotmap::SlotMap;
use sonamu_common::{
    command::{self, NodeUid, Value, World},
    sap::{
        client_handle::ClientHandle,
        detached::{DetachedClient, DetachedTree, EMPTY_LABEL},
        packets::{AwaitedWl, NewWlSurface, SpawnPosition, StandardEvent, SwipeView, WlSurfaceId},
        session::{SavedApplet, SavedTree, SavedWl},
    },
    settings::{self, ZoomOutView},
    utils::tree::{
        recursive_tree::RecursiveTreeNode,
        tree_node_path::TreeNodePath,
        world_tree::{Step, WorldTree, WorldTreePath},
    },
};
use sonamu_ui::{
    display_units::{DisplayContainerSize, ToPx},
    ui_element::{Style, UIElement, UITree, font_size},
    ui_event::{KeySymbol, MouseEvent, MouseEventKind, PinchPhase, UIEvent},
};
use std::{cell::Cell, collections::HashMap, time::Instant};

#[derive(Debug, Clone, Copy)]
enum FocusIndex {
    /// Focus is at this node: inside its main if it has one (at its root unless the main's own focus is deeper),
    /// else the node itself (eg a container: the selector)
    Here,
    Child(usize),
}

slotmap::new_key_type! {
    /// Identifies a node, so requests from its applet still find it after it moves
    pub struct NodeId;
}

/// Starts the label of an inactive node's row in the treeview (after any `▾`/`▸`)
pub(crate) const INACTIVE_MARK: &str = "⏾ ";

/// Shows its main applet or the focused child
struct Node {
    /// For commands (`command::register`); kept when it moves to another world
    uid: NodeUid,
    /// Dropping it ends the applet. `None` for containers.
    main: Option<ClientHandle>,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    focus: FocusIndex,
    /// Last size sent to `main`
    size: Option<DisplayContainerSize>,
    /// The wl surface focus is on inside its main's inner tree, as last reported by the main
    inner_wl_focus: Option<WlSurfaceId>,
    /// Its main's inner tree is hidden in the treeview
    collapsed: bool,
    /// The child focus was last on; shown by a container (no main) while it is focused itself
    last_child: usize,
    /// A restored wl client's tile (its main stands in) waiting for the client's toplevel
    awaiting: Option<AwaitedWl>,
    /// Hibernating: shown grey in the treeview
    inactive: bool,
    /// A restored wl client not started yet, or a deactivated applet (its main stands in); started when focused
    hibernated: Option<SavedApplet>,
    /// The last `StandardEvent::ScreenViews` sent to its main: its size on each screen, and the active one
    screen_views: Option<(Vec<Option<DisplayContainerSize>>, usize)>,
    /// Half width (`Super+F`, #144)
    half: bool,
    /// How far its children's strip is scrolled, in px (#144)
    scroll: Cell<i32>,
    /// How far the last layout scrolled it, kept or not (for a swipe's ends)
    laid_scroll: Cell<i32>,
}
impl Node {
    fn new(main: Option<ClientHandle>, parent: Option<NodeId>) -> Self {
        Self {
            uid: NodeUid::new(),
            main,
            parent,
            children: Vec::new(),
            focus: FocusIndex::Here,
            size: None,
            inner_wl_focus: None,
            collapsed: false,
            last_child: 0,
            awaiting: None,
            inactive: false,
            hibernated: None,
            screen_views: None,
            half: false,
            scroll: Cell::new(0),
            laid_scroll: Cell::new(0),
        }
    }
}

/// Runs an applet as a client whose requests are tagged with the given node
pub type SpawnClient = Box<dyn Fn(NodeId, NodularAppletInitializer) -> ClientHandle>;
/// Like `SpawnClient`, for a placed client that is already running
pub type AttachClient = Box<dyn Fn(NodeId, DetachedClient) -> ClientHandle>;

/// A tree of nodes held by a `NodeHolder` (eg the root's): every node's main is a client of the holder.
///
/// Only touched by the holder's thread; applets talk to it through `NodeRequest`s.
pub struct NodeTree {
    root: NodeId,
    nodes: SlotMap<NodeId, Node>,
    /// Which node shows each wl surface (its main is a `WlSurfaceApplet`)
    wl_surfaces: HashMap<WlSurfaceId, NodeId>,

    /// The whole tree is displayed at this size
    latest_size: Option<DisplayContainerSize>,
    /// What is on screen, from the last `update_layout`
    panes: Vec<Pane>,
    /// The pane a mouse button was pressed in; it gets the pointer until the release
    pointer_grab: Option<NodeId>,
    spawn_client: SpawnClient,
    attach_client: AttachClient,
    /// Where commands for its nodes go (`command::register`): the holder's thread
    world: World,
    /// Set when the tree changed in a way that needs a redraw
    damaged: bool,
    /// A standard keybind pressed on the selected node (in the treeview selector), for the holder to do as if from it
    keybind_request: Option<(NodeId, NodeRequest)>,

    /// Only its subtree is shown (panes and treeview), as if it were the root.
    /// Focus always stays inside it: when focus leaves, it widens to the ancestor that has the focus.
    view_root: NodeId,

    /// Every screen showing the tree, each with its own focus and view root (#60); only the root's has several.
    /// The active one's are the fields above (`latest_size`, `panes`, `view_root` and the nodes' focus),
    /// so its entry is stale.
    screens: Vec<Screen>,
    active_screen: usize,

    /// A swipe drawn between two of its nodes instead of the focus (#141)
    swipe: Option<TreeSwipe>,
    /// The node a swipe was passed into (its main's inner world has both ends), to stop it there
    swipe_sent_to: Option<NodeId>,

    /// Where focus last was below the root: the zoom-out view's center (`zoom_center`, #145)
    last_focus: NodeId,
    /// How far a scroll in the zoom-out view moved, until it steps (`zoom_scroll`)
    zoom_scrolled: [f64; 2],
    /// A press in the zoom-out view, until its release (`zoom_button`)
    zoom_press: Option<ZoomPress>,
    /// How far the free zoom-out view (`ZoomOutView::Free`) was moved: px (before scaling) and rows down, like
    /// `zoom_pan`
    zoom_free: ([f32; 2], f32),
    /// How much a pinch scales the zoom-out view (`zoom_pinch`), and what it was when the pinch began
    zoom_pinched: (f32, Option<f32>),
    /// The zoom-out view growing out of, or back into, a node's view
    zoom_anim: Option<ZoomAnim>,
    /// Whether focus was at a node without a main (zoomed out) at the last `update_layout`; `None` before it
    was_zoomed: Option<bool>,
    /// The zoom-out view springing to its moved center: since when, and from how far off: px (before scaling) and
    /// rows down (rows are further apart than their views when zoomed out, by their titles); lateral (to a node
    /// in the same row): the rows above stay
    zoom_pan: Option<(Instant, [f32; 2], f32, bool)>,
    /// Panes sliding to where a drop or swap put them in the zoom-out view: since when, and from how far off (px,
    /// before scaling) per node and its occurrence (rows show a node more than once)
    zoom_moves: Option<(Instant, ZoomOccurrences<[f32; 2]>)>,
}

impl Drop for NodeTree {
    fn drop(&mut self) {
        for node in self.nodes.values() {
            command::unregister(node.uid);
        }
    }
}

/// A press in the zoom-out view (`NodeTree::zoom_button`)
#[derive(Clone, Copy)]
struct ZoomPress {
    /// The node it was on
    node: NodeId,
    from: [f64; 2],
    /// Where the pointer is now
    at: [f64; 2],
    /// Of the view
    size: DisplayContainerSize,
    /// How far dragging at its edges scrolled it (px, before scaling); the center stays
    scroll: [f32; 2],
    /// When `scroll` last moved
    tick: Instant,
}

/// Where a drag in the zoom-out view drops a node: beside another (`Before`/`After`), or as its child (`Below`)
#[derive(Clone, Copy, PartialEq, Eq)]
enum ZoomSide {
    Before,
    After,
    Below,
}

/// The zoom-out view springing between a node's view (its center) and the whole view (#145)
#[derive(Clone, Copy)]
struct ZoomAnim {
    started: Instant,
    /// Zooming out, else in
    out: bool,
    center: NodeId,
}

/// A swipe between two nodes of the tree (`StandardEvent::Swipe`), drawn on the active screen
struct TreeSwipe {
    vertical: bool,
    from: NodeId,
    to: Option<NodeId>,
    progress: f32,
    /// What each end shows, from the last `update_layout`
    panes: [Vec<Pane>; 2],
    /// Siblings' parent, and how far each end scrolls its strip: the strip slides between them (#144)
    strip: Option<(NodeId, [i32; 2])>,
}

/// A screen showing the tree, as it was left when another became active
struct Screen {
    size: Option<DisplayContainerSize>,
    panes: Vec<Pane>,
    /// The focused node, and whether focus is inside its main
    focus: (NodeId, bool),
    view_root: NodeId,
}

/// A node shown on screen, at `rect` (`[x, y, width, height]`, border included) of the tree's area,
/// cut to `clip` (the strip it is in, #144)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pane {
    node: NodeId,
    rect: [i32; 4],
    clip: [i32; 4],
    /// From the settings; 0 when unframed
    border: u32,
    /// Has a border and rounded corners: not the only pane (unless `always_frame`), and not a project
    framed: bool,
}
impl Pane {
    /// Inside the border
    const fn inner_size(&self) -> DisplayContainerSize {
        DisplayContainerSize::new(
            self.rect[2].cast_unsigned().saturating_sub(2 * self.border),
            self.rect[3].cast_unsigned().saturating_sub(2 * self.border),
        )
    }

    fn contains(&self, [x, y]: [f64; 2]) -> bool {
        let inside = |[left, top, width, height]: [i32; 4]| {
            let [left, top, width, height] = [left, top, width, height].map(f64::from);
            (left..left + width).contains(&x) && (top..top + height).contains(&y)
        };
        inside(self.rect) && inside(self.clip)
    }
}

/// Something per pane of the zoom-out view, by its node and occurrence (rows show a node more than once)
type ZoomOccurrences<T> = HashMap<(NodeId, usize), T>;

/// The zoom-out view's rows (`NodeTree::zoom_rows`): the center (`zoom_center`, else the focused node), its row,
/// and the rows' spacing (px)
struct ZoomRows {
    /// Each with the node focus is at for it
    rows: Vec<(NodeId, Vec<Pane>)>,
    center: NodeId,
    center_row: usize,
    step: i32,
}

/// The zoom-out view (`NodeTree::zoom_layout`), before its `scale`: its panes, center, and the box around the
/// center's column (`[x, y, width, height]`)
struct ZoomLayout {
    panes: Vec<Pane>,
    /// For each pane, the node focus is at for its row
    row_focus: Vec<NodeId>,
    center: NodeId,
    scale: f32,
    column: [f32; 4],
}

/// How `push_panes` lays out strips
#[derive(Clone, Copy)]
struct Strips {
    /// Shown whole, scrolled to
    focus: NodeId,
    gap: i32,
    /// Keeps how far each strip is scrolled
    remember: bool,
    /// This strip scrolled this far instead, showing every child in sight (a swipe between siblings)
    fixed: Option<(NodeId, i32)>,
    /// Every child of the last strip, in sight or not, unclipped (the zoom-out view, #145)
    whole: bool,
}

impl NodeTree {
    pub fn new(
        root: NodularAppletInitializer,
        spawn_client: SpawnClient,
        attach_client: AttachClient,
        world: World,
    ) -> Self {
        Self::with_root(spawn_client, attach_client, world, |tree| {
            tree.insert_node(None, root)
        })
    }

    /// A tree of placed clients (eg a project's contents)
    pub fn from_detached(
        root: DetachedTree,
        spawn_client: SpawnClient,
        attach_client: AttachClient,
        world: World,
    ) -> Self {
        Self::with_root(spawn_client, attach_client, world, |tree| {
            tree.attach_subtree(None, root)
        })
    }

    /// A saved tree, made again (`session::restore`)
    pub fn restore(
        root: SavedTree,
        spawn_client: SpawnClient,
        attach_client: AttachClient,
        world: World,
    ) -> Self {
        Self::with_root(spawn_client, attach_client, world, |tree| {
            tree.restore_subtree(None, root)
        })
    }

    fn with_root(
        spawn_client: SpawnClient,
        attach_client: AttachClient,
        world: World,
        insert_root: impl FnOnce(&mut Self) -> NodeId,
    ) -> Self {
        let mut tree = Self {
            root: NodeId::default(),
            nodes: SlotMap::with_key(),
            wl_surfaces: HashMap::new(),
            latest_size: None,
            panes: Vec::new(),
            pointer_grab: None,
            spawn_client,
            attach_client,
            world,
            damaged: true,
            keybind_request: None,
            view_root: NodeId::default(),
            screens: Vec::new(),
            active_screen: 0,
            swipe: None,
            swipe_sent_to: None,
            last_focus: NodeId::default(),
            zoom_scrolled: [0.0; 2],
            zoom_press: None,
            zoom_pinched: (1.0, None),
            zoom_free: ([0.0; 2], 0.0),
            zoom_anim: None,
            was_zoomed: None,
            zoom_pan: None,
            zoom_moves: None,
        };
        tree.root = insert_root(&mut tree);
        tree.view_root = tree.root;
        tree.screens.push(Screen {
            size: None,
            panes: Vec::new(),
            focus: (tree.root, false),
            view_root: tree.root,
        });
        tree
    }

    pub const fn root(&self) -> NodeId {
        self.root
    }

    /// Its commands come to this world; its main hears its uid
    fn register(&self, id: NodeId) {
        let node = &self.nodes[id];
        command::register(node.uid, self.world.clone());
        if let Some(main) = &node.main {
            main.send_event(StandardEvent::NodeUid(node.uid));
        }
    }

    pub fn uid(&self, id: NodeId) -> NodeUid {
        self.nodes[id].uid
    }

    pub fn node_by_uid(&self, uid: NodeUid) -> Option<NodeId> {
        self.nodes
            .iter()
            .find_map(|(id, node)| (node.uid == uid).then_some(id))
    }

    /// Whether the tree changed since the last call
    pub const fn take_damage(&mut self) -> bool {
        std::mem::replace(&mut self.damaged, false)
    }

    pub const fn damage(&mut self) {
        self.damaged = true;
    }

    /// A standard keybind pressed on a node with no main, since the last call
    pub const fn take_keybind_request(&mut self) -> Option<(NodeId, NodeRequest)> {
        self.keybind_request.take()
    }

    fn insert_node(
        &mut self,
        parent: Option<NodeId>,
        initializer: NodularAppletInitializer,
    ) -> NodeId {
        let spawn_client = &self.spawn_client;
        let id = self
            .nodes
            .insert_with_key(|id| Node::new(Some(spawn_client(id, initializer)), parent));
        self.register(id);
        self.send_initial_size(id);
        sonamu_script::emit("open", vec![Value::Node(self.nodes[id].uid)]);
        id
    }

    /// Attaches the tree's clients under `parent` (not added to its children), with new ids
    fn attach_subtree(&mut self, parent: Option<NodeId>, tree: DetachedTree) -> NodeId {
        let attach_client = &self.attach_client;
        let id = self.nodes.insert_with_key(|id| Node {
            inactive: tree.hibernated.is_some(),
            hibernated: tree.hibernated,
            awaiting: tree.awaiting,
            uid: tree.uid.unwrap_or_default(),
            ..Node::new(tree.main.map(|main| attach_client(id, main)), parent)
        });
        self.register(id);
        self.send_initial_size(id);
        if let Some(surface_id) = tree.wl_surface {
            self.wl_surfaces.insert(surface_id, id);
        }
        for child in tree.children {
            let child = self.attach_subtree(Some(id), child);
            self.nodes[id].children.push(child);
        }
        id
    }

    /// Makes the saved node and its descendants under `parent` (not added to its children),
    /// all hibernating until focused
    fn restore_subtree(&mut self, parent: Option<NodeId>, mut saved: SavedTree) -> NodeId {
        // a split, saved before they were removed: a container of its panes
        if let Some(SavedApplet::Spawner {
            name,
            inner: Some(inner),
            ..
        }) = &mut saved.main
            && name == "split"
        {
            let inner = std::mem::take(&mut **inner);
            let panes = if inner.main.is_some() {
                vec![inner]
            } else {
                inner.children
            };
            saved.main = None;
            saved.children.splice(0..0, panes);
        }
        let hibernated = saved.main.clone();
        let main = saved.main.map(session::hibernate);
        let spawn_client = &self.spawn_client;
        let id = self.nodes.insert_with_key(|id| Node {
            inactive: hibernated.is_some(),
            hibernated,
            half: saved.half,
            ..Node::new(
                main.map(|initializer| spawn_client(id, initializer)),
                parent,
            )
        });
        self.register(id);
        self.send_initial_size(id);
        for child in saved.children {
            let child = self.restore_subtree(Some(id), child);
            self.nodes[id].children.push(child);
        }
        id
    }

    /// What to restore on the next start.
    /// Nodes whose main isn't saved (eg the help) are left out, their children taking their place.
    pub fn save(&self) -> SavedTree {
        let mut saved = self.save_subtree(self.root);
        match saved.len() {
            1 => saved.pop().unwrap(),
            _ => SavedTree {
                main: None,
                children: saved,
                ..SavedTree::default()
            },
        }
    }

    /// The node, or its children if it is left out
    fn save_subtree(&self, id: NodeId) -> Vec<SavedTree> {
        let node = &self.nodes[id];
        let children: Vec<_> = node
            .children
            .iter()
            .flat_map(|&child| self.save_subtree(child))
            .collect();
        let main = match &node.main {
            Some(main) => match main.get_treeview().saved {
                Some(saved) => Some(saved),
                None => return children,
            },
            None => None,
        };
        vec![SavedTree {
            main,
            children,
            half: node.half,
            label: None,
            directory: None,
            icon: None,
        }]
    }

    /// Restored wl clients waiting for their toplevel, in this tree and inside its mains (eg projects)
    pub fn awaiting_wl(&self) -> Vec<AwaitedWl> {
        self.nodes
            .values()
            .flat_map(|node| {
                let inner = node
                    .main
                    .as_ref()
                    .map(|main| main.get_treeview().awaiting_wl)
                    .unwrap_or_default();
                node.awaiting.clone().into_iter().chain(inner)
            })
            .collect()
    }

    /// Gives the surface to the restored tile awaiting it, by its pid, else its app id
    /// (eg a browser's window comes from its running instance, not the process the tile started).
    /// A tile inside a main (eg a project) is given it by the main. `Err` if none awaits it.
    pub fn claim_wl_surface(&mut self, surface: NewWlSurface) -> Result<(), Box<NewWlSurface>> {
        let matches = |awaited: &AwaitedWl, by_pid: bool| {
            if by_pid {
                surface.pid == Some(awaited.pid)
            } else {
                awaited.app_id.is_some() && awaited.app_id == surface.app_id
            }
        };
        for by_pid in [true, false] {
            let own = self.nodes.iter().find_map(|(id, node)| {
                node.awaiting
                    .as_ref()
                    .is_some_and(|awaited| matches(awaited, by_pid))
                    .then_some(id)
            });
            if let Some(id) = own {
                self.fill_awaiting(id, surface);
                return Ok(());
            }
            let inside = self.nodes.iter().find_map(|(id, node)| {
                node.main
                    .as_ref()
                    .is_some_and(|main| {
                        main.get_treeview()
                            .awaiting_wl
                            .iter()
                            .any(|awaited| matches(awaited, by_pid))
                    })
                    .then_some(id)
            });
            if let Some(id) = inside {
                self.send_event(id, StandardEvent::WlSurfaceRegistered(surface));
                return Ok(());
            }
        }
        Err(Box::new(surface))
    }

    /// The tile shows the surface in place of the applet standing in for it
    fn fill_awaiting(&mut self, id: NodeId, surface: NewWlSurface) {
        self.wl_surfaces.insert(surface.surface_id, id);
        let main = (self.spawn_client)(id, WlSurfaceApplet::get_boxed_initiator(surface));
        let node = &mut self.nodes[id];
        node.awaiting = None;
        main.send_event(StandardEvent::NodeUid(node.uid));
        node.main = Some(main);
        // so the new applet gets its size
        node.size = None;
        node.screen_views = None;
        self.send_initial_size(id);
        self.damaged = true;
    }

    /// Detaches every node; the tree can't be used after
    pub fn detach_all(&mut self) -> DetachedTree {
        self.detach_subtree(self.root)
    }

    /// Detaches the node and its descendants (already removed from its parent's children)
    fn detach_subtree(&mut self, id: NodeId) -> DetachedTree {
        let node = self.nodes.remove(id).unwrap();
        let wl_surface = self
            .wl_surfaces
            .iter()
            .find_map(|(&surface_id, &node_id)| (node_id == id).then_some(surface_id));
        if let Some(surface_id) = wl_surface {
            self.wl_surfaces.remove(&surface_id);
        }
        DetachedTree {
            main: node.main.map(ClientHandle::detach),
            wl_surface,
            hibernated: node.hibernated,
            awaiting: node.awaiting,
            uid: Some(node.uid),
            children: node
                .children
                .into_iter()
                .map(|child| self.detach_subtree(child))
                .collect(),
        }
    }

    /// A guess until `update_layout` shows it: the whole tree's size
    fn send_initial_size(&mut self, id: NodeId) {
        if let Some(size) = self.latest_size {
            self.resize(id, size);
        }
    }

    fn resize(&mut self, id: NodeId, size: DisplayContainerSize) {
        let node = &mut self.nodes[id];
        if node.size != Some(size) {
            node.size = Some(size);
            if let Some(main) = &node.main {
                main.send_event(StandardEvent::UIEvent(UIEvent::WindowResized(size)));
            }
        }
    }

    pub fn send_event(&self, id: NodeId, event: StandardEvent) {
        if let Some(main) = self.nodes.get(id).and_then(|node| node.main.as_ref()) {
            main.send_event(event);
        }
    }

    /// Gives the event back if the node has no running applet to send it to
    /// (containers have none, and an applet may have ended, eg by panicking)
    // the error is just the event, as big as what was passed in
    #[allow(clippy::result_large_err)]
    pub fn try_send_event(&self, id: NodeId, event: StandardEvent) -> Result<(), StandardEvent> {
        match self.nodes.get(id).and_then(|node| node.main.as_ref()) {
            Some(main) => main.try_send_event(event),
            None => Err(event),
        }
    }

    pub fn contains_node(&self, id: NodeId) -> bool {
        self.nodes.contains_key(id)
    }

    /// The node at `path` of the node tree
    pub fn node_at(&self, path: &TreeNodePath) -> Option<NodeId> {
        let mut id = self.root;
        for &index in &path.0 {
            id = *self.nodes[id].children.get(index)?;
        }
        Some(id)
    }

    /// The deepest node along `path`, and whether it is the node at `path`
    pub fn closest_node_at(&self, path: &TreeNodePath) -> (NodeId, bool) {
        let mut id = self.root;
        for &index in &path.0 {
            let Some(&child) = self.nodes[id].children.get(index) else {
                return (id, false);
            };
            id = child;
        }
        (id, true)
    }

    /// The node's path in the node tree, `None` if it isn't in the tree
    pub fn path_of(&self, id: NodeId) -> Option<TreeNodePath> {
        let mut indices = Vec::new();
        let mut child = id;
        while let Some(parent) = self.nodes.get(child)?.parent {
            indices.push(self.child_index(parent, child));
            child = parent;
        }
        indices.reverse();
        Some(TreeNodePath(indices))
    }

    fn child_index(&self, parent: NodeId, child: NodeId) -> usize {
        self.nodes[parent]
            .children
            .iter()
            .position(|&id| id == child)
            .unwrap()
    }

    /// Takes the node out with its children.
    /// If it was its parent's focused child, the parent becomes focused.
    ///
    /// `None` for the root or a node not in the tree.
    ///
    /// # Panics
    ///
    /// If called while the loop is dispatching a request from one of the plucked applets.
    pub fn pluck(&mut self, id: NodeId) -> Option<DetachedTree> {
        let parent_id = self.nodes.get(id)?.parent?;
        let index = self.child_index(parent_id, id);
        let parent = &mut self.nodes[parent_id];
        parent.children.remove(index);
        parent.focus = match parent.focus {
            FocusIndex::Child(focused) if focused == index => FocusIndex::Here,
            FocusIndex::Child(focused) if focused > index => FocusIndex::Child(focused - 1),
            focus => focus,
        };
        if matches!(parent.focus, FocusIndex::Here) {
            self.selected(parent_id);
        }
        self.damaged = true;
        if self.contains(id, self.view_root) {
            self.view_root = parent_id;
        }
        Some(self.detach_subtree(id))
    }

    /// Whether `descendant` is `ancestor` or below it
    fn contains(&self, ancestor: NodeId, descendant: NodeId) -> bool {
        let mut id = Some(descendant);
        while let Some(current) = id {
            if current == ancestor {
                return true;
            }
            id = self.nodes.get(current).and_then(|node| node.parent);
        }
        false
    }

    /// Toggles the node between half and full width (`Super+F`, #144)
    pub fn toggle_half(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.half = !node.half;
            self.damaged = true;
        }
    }

    /// Swaps the node with the sibling `shift` places over; does nothing if there is none
    pub fn swap_sibling(&mut self, id: NodeId, shift: isize) {
        let Some(parent_id) = self.nodes.get(id).and_then(|node| node.parent) else {
            return;
        };
        let index = self.child_index(parent_id, id);
        let Some(other) = index
            .checked_add_signed(shift)
            .filter(|&other| other < self.nodes[parent_id].children.len())
        else {
            return;
        };
        let parent = &mut self.nodes[parent_id];
        parent.children.swap(index, other);
        // focus and last child follow their nodes
        for slot in [&mut parent.last_child]
            .into_iter()
            .chain(match &mut parent.focus {
                FocusIndex::Child(focused) => Some(focused),
                _ => None,
            })
        {
            if *slot == index {
                *slot = other;
            } else if *slot == other {
                *slot = index;
            }
        }
        self.damaged = true;
    }

    /// Moves the node, with its children, beside `target` or after `target`'s remembered child (`Below`); focus and
    /// remembered children stay on their nodes. Neither may be the root, nor `target` inside the node.
    fn move_node(&mut self, id: NodeId, target: NodeId, side: ZoomSide) {
        let old_parent = self.nodes[id].parent.expect("not the root");
        let old_index = self.child_index(old_parent, id);
        let old = &mut self.nodes[old_parent];
        old.children.remove(old_index);
        old.focus = match old.focus {
            FocusIndex::Child(focused) if focused == old_index => FocusIndex::Here,
            FocusIndex::Child(focused) if focused > old_index => FocusIndex::Child(focused - 1),
            focus => focus,
        };
        if old.last_child > old_index {
            old.last_child -= 1;
        }
        let (parent, index) = if side == ZoomSide::Below {
            let index = self
                .remembered_child(target)
                .map_or(0, |child| self.child_index(target, child) + 1);
            (target, index)
        } else {
            let parent = self.nodes[target].parent.expect("not the root");
            let index = self.child_index(parent, target);
            (parent, index + usize::from(side == ZoomSide::After))
        };
        let new = &mut self.nodes[parent];
        new.children.insert(index, id);
        if let FocusIndex::Child(focused) = &mut new.focus
            && *focused >= index
        {
            *focused += 1;
        }
        if new.children.len() > 1 && new.last_child >= index {
            new.last_child += 1;
        }
        self.nodes[id].parent = Some(parent);
        self.damaged = true;
    }

    /// Puts `new` where `old` is among the parent's children (or makes it the root)
    fn replace_child(&mut self, parent: Option<NodeId>, old: NodeId, new: NodeId) {
        match parent {
            Some(parent) => {
                let index = self.child_index(parent, old);
                self.nodes[parent].children[index] = new;
            }
            None => self.root = new,
        }
    }

    /// Replaces the node and its descendants with a node whose applet is made from them (eg a project).
    /// The new node takes its place, focused itself. Returns it.
    pub fn wrap(
        &mut self,
        id: NodeId,
        wrapper: impl FnOnce(DetachedTree) -> NodularAppletInitializer,
    ) -> Option<NodeId> {
        let node = self.nodes.get(id)?;
        let parent = node.parent;
        let had_view_root = self.contains(id, self.view_root);
        let subtree = self.detach_subtree(id);
        let new = self.insert_node(parent, wrapper(subtree));
        self.replace_child(parent, id, new);
        if had_view_root {
            self.view_root = new;
        }
        self.damaged = true;
        Some(new)
    }

    /// Replaces the node with `tree` (eg a project's contents); the node's children move under the tree's root.
    /// The tree goes under the root if the node is gone.
    ///
    /// A container root (no main, like a project's) is dropped, its children taking its place,
    /// unless it would leave the tree without a root.
    pub fn unwrap(&mut self, id: NodeId, tree: DetachedTree) {
        let Some(node) = self.nodes.get(id) else {
            self.place(self.root, SpawnPosition::AppendChild, tree);
            return;
        };
        let parent = node.parent;
        let was_focused = self.focused_node() == id;
        let new = self.attach_subtree(parent, tree);
        self.replace_child(parent, id, new);
        if self.view_root == id {
            self.view_root = new;
        }
        let old = self.nodes.remove(id).unwrap();
        command::unregister(old.uid);
        for &child in &old.children {
            self.nodes[child].parent = Some(new);
        }
        self.nodes[new].children.extend(old.children);
        self.wl_surfaces.retain(|_, node_id| *node_id != id);
        self.damaged = true;

        let node = &self.nodes[new];
        if node.main.is_some() {
            return;
        }
        let children = node.children.clone();
        match (node.parent, children.as_slice()) {
            (Some(_), _) => {
                self.close(new);
            }
            (None, &[only]) => {
                if let Some(removed) = self.nodes.remove(new) {
                    command::unregister(removed.uid);
                }
                self.nodes[only].parent = None;
                self.root = only;
                if self.view_root == new {
                    self.view_root = only;
                }
            }
            // the tree needs a root
            (None, _) => {}
        }
        if was_focused && let Some(&first) = children.first() {
            self.focus(first, false);
        }
    }

    /// Adds the tree as the parent's child at `position` (the root's if the parent is gone) and focuses it
    pub fn place(&mut self, parent: NodeId, position: SpawnPosition, tree: DetachedTree) -> NodeId {
        // eg it closed while its applet was being asked to place
        let parent = if self.nodes.contains_key(parent) {
            parent
        } else {
            self.root
        };
        let (parent, position) = self.child_target(parent, position);
        let id = self.attach_subtree(Some(parent), tree);
        let children = &mut self.nodes[parent].children;
        match position {
            SpawnPosition::InsertChild { child_index } if child_index < children.len() => {
                children.insert(child_index, id);
            }
            _ => children.push(id),
        }
        self.focus_node(id);
        self.nodes[id].focus = FocusIndex::Here;
        self.damaged = true;
        id
    }

    /// Points every ancestor's focus down to the node, without changing the node's own focus
    fn focus_node(&mut self, id: NodeId) {
        if id != self.root {
            self.last_focus = id;
        }
        let mut child = id;
        while let Some(parent) = self.nodes[child].parent {
            let index = self.child_index(parent, child);
            let parent_node = &mut self.nodes[parent];
            parent_node.focus = FocusIndex::Child(index);
            parent_node.last_child = index;
            child = parent;
        }
        self.damaged = true;
    }

    /// Focuses what `path` of the treeview points at: the node itself if the path ends at it,
    /// otherwise inside its main, which is asked to focus the rest of the path
    pub fn focus_path(&mut self, path: &WorldTreePath) {
        let (node_path, inner) = path.split_world();
        let Some(id) = self.node_at(&node_path) else {
            return;
        };
        self.focus_node(id);
        self.nodes[id].focus = FocusIndex::Here;
        // so the path inside goes to the woken applet
        self.wake_focused();
        match inner {
            Some(inner) => self.send_event(id, StandardEvent::Focus(inner)),
            None => self.selected(id),
        }
    }

    /// Focus ends at the node itself: its main goes to its root (eg a project shows its overview)
    fn selected(&self, id: NodeId) {
        if let Some(main) = &self.nodes[id].main {
            main.send_event(StandardEvent::Focus(WorldTreePath::root()));
        }
    }

    /// Returns `None` if `parent` isn't in the tree (eg it closed since)
    pub fn add_child(
        &mut self,
        parent: NodeId,
        initializer: NodularAppletInitializer,
    ) -> Option<NodeId> {
        self.add_child_at(parent, SpawnPosition::AppendChild, initializer)
    }

    /// Like `add_child`, at `position` among the parent's children
    pub fn add_child_at(
        &mut self,
        parent: NodeId,
        position: SpawnPosition,
        initializer: NodularAppletInitializer,
    ) -> Option<NodeId> {
        if !self.nodes.contains_key(parent) {
            return None;
        }
        let (parent, position) = self.child_target(parent, position);
        let id = self.insert_node(Some(parent), initializer);
        let children = &mut self.nodes[parent].children;
        match position {
            SpawnPosition::InsertChild { child_index } if child_index < children.len() => {
                children.insert(child_index, id);
                // focus and last child stay on their nodes
                let parent = &mut self.nodes[parent];
                if let FocusIndex::Child(focused) = &mut parent.focus
                    && *focused >= child_index
                {
                    *focused += 1;
                }
                if parent.last_child >= child_index {
                    parent.last_child += 1;
                }
            }
            _ => children.push(id),
        }
        self.damaged = true;
        Some(id)
    }

    /// Where a child of `parent` at `position` goes. A node holding its children never gets one:
    /// it goes right after it (eg one its hibernating or dissolving project gave back).
    fn child_target(&self, parent: NodeId, position: SpawnPosition) -> (NodeId, SpawnPosition) {
        if !self.holds_children(parent) {
            return (parent, position);
        }
        if self.nodes[parent].parent.is_none() {
            log::warn!("A child of the root, which holds its children");
        }
        self.spawn_target(parent, true)
    }

    /// Where something added at the node goes: as its last child, or if `sibling`, right after it
    /// (as the root has no siblings, it gets a child instead)
    pub fn spawn_target(&self, id: NodeId, sibling: bool) -> (NodeId, SpawnPosition) {
        match self.nodes[id].parent.filter(|_| sibling) {
            Some(parent) => (
                parent,
                SpawnPosition::InsertChild {
                    child_index: self.child_index(parent, id) + 1,
                },
            ),
            None => (id, SpawnPosition::AppendChild),
        }
    }

    /// The node and its descendants, parents first
    pub fn subtree(&self, id: NodeId) -> Vec<NodeId> {
        let mut ids = vec![id];
        let mut index = 0;
        while let Some(&next) = ids.get(index) {
            ids.extend(&self.nodes[next].children);
            index += 1;
        }
        ids
    }

    /// Stops the node's applet, leaving the node inactive like a restored wl tile, until it is focused:
    /// a wl client is asked to close its toplevel, a native applet stops what it holds (eg its inner tree's wl
    /// clients, `StandardEvent::Deactivate`) and is dropped. Does nothing if it has no saved form, or is still
    /// starting. Focus on it moves to its parent, or it would wake right away.
    pub fn deactivate(&mut self, id: NodeId) {
        let node = &self.nodes[id];
        if node.hibernated.is_some() {
            return;
        }
        let Some(treeview) = node.main.as_ref().map(ClientHandle::get_treeview) else {
            return;
        };
        let stand_in = match treeview.saved {
            Some(SavedApplet::Wl(_))
                if !self.wl_surfaces.values().any(|&node_id| node_id == id) =>
            {
                return;
            }
            Some(saved @ SavedApplet::Wl(_)) => {
                // its toplevel's destruction no longer closes the node
                self.wl_surfaces.retain(|_, node_id| *node_id != id);
                // a wl tile asks its client, without asking its node to close
                self.send_event(id, StandardEvent::CloseRequest);
                saved
            }
            Some(saved) => {
                self.send_event(id, StandardEvent::Deactivate);
                saved
            }
            None => return,
        };
        let main = (self.spawn_client)(id, session::hibernate(stand_in.clone()));
        let node = &mut self.nodes[id];
        main.send_event(StandardEvent::NodeUid(node.uid));
        node.main = Some(main);
        node.hibernated = Some(stand_in);
        node.inactive = true;
        node.inner_wl_focus = None;
        // so the new applet gets its size
        node.size = None;
        node.screen_views = None;
        self.send_initial_size(id);
        if self.focused_node() == id
            && let Some(parent) = self.nodes[id].parent
        {
            self.focus(parent, false);
        }
        self.damaged = true;
    }

    /// Stops what every node's applet holds, before the tree is dropped (its holder is deactivated)
    pub fn deactivate_all(&mut self) {
        for (id, node) in &self.nodes {
            // a stand-in (hibernated) has nothing to stop
            if let Some(main) = &node.main
                && node.hibernated.is_none()
            {
                main.send_event(if self.wl_surfaces.values().any(|&node_id| node_id == id) {
                    StandardEvent::CloseRequest
                } else {
                    StandardEvent::Deactivate
                });
            }
        }
    }

    /// Adds a tile for the surface as the parent's last child, and focuses inside it (so it's shown and gets keys)
    pub fn add_wl_surface(&mut self, parent: NodeId, surface: NewWlSurface) {
        self.add_wl_surface_at(parent, SpawnPosition::AppendChild, surface);
    }

    /// Like `add_wl_surface`, at `position` among the parent's children
    pub fn add_wl_surface_at(
        &mut self,
        parent: NodeId,
        position: SpawnPosition,
        surface: NewWlSurface,
    ) {
        let surface_id = surface.surface_id;
        if let Some(id) = self.add_child_at(
            parent,
            position,
            WlSurfaceApplet::get_boxed_initiator(surface),
        ) {
            self.wl_surfaces.insert(surface_id, id);
            self.focus(id, true);
        }
    }

    /// Focuses the node: where its applet's focus is if `inner`, else at the applet's root
    pub fn focus(&mut self, id: NodeId, inner: bool) {
        self.focus_node(id);
        self.nodes[id].focus = FocusIndex::Here;
        if !inner {
            self.selected(id);
        }
    }

    /// Nodes with a main, which may hold more of the tree inside
    pub fn with_main(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes
            .iter()
            .filter(|(_, node)| node.main.is_some())
            .map(|(id, _)| id)
    }

    pub fn has_main(&self, id: NodeId) -> bool {
        self.nodes.get(id).is_some_and(|node| node.main.is_some())
    }

    /// The wl surface focus is on inside the node's main, as reported by it
    pub fn set_inner_wl_focus(&mut self, id: NodeId, surface_id: Option<WlSurfaceId>) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.inner_wl_focus = surface_id;
        }
    }

    /// Views only the focused node's subtree, or everything if it's already the view root
    pub fn toggle_view_root(&mut self) {
        let focused = self.focused_node();
        self.view_root = if focused == self.view_root {
            self.root
        } else {
            focused
        };
        self.damaged = true;
    }

    /// The node at `path` (or the deepest one along it) as the view root
    pub fn set_view_root(&mut self, path: &TreeNodePath) {
        self.view_root = self.closest_node_at(path).0;
        self.damaged = true;
    }

    /// Widens the view root if focus left it. Run after each batch of changes.
    pub fn update_view_root(&mut self) {
        let old = self.view_root;
        // pluck and close move it themselves; just in case
        if !self.nodes.contains_key(self.view_root) {
            self.view_root = self.root;
        }
        let focused = self.focused_node();
        // eg traversing to the parent past it
        while !self.contains(self.view_root, focused)
            && let Some(parent) = self.nodes[self.view_root].parent
        {
            self.view_root = parent;
        }
        if self.view_root != old {
            self.damaged = true;
        }
    }

    /// The screen's view root (as of the last `update_layout` for an inactive one)
    fn screen_view_root(&self, screen: usize) -> NodeId {
        if screen == self.active_screen {
            return self.view_root;
        }
        self.screens
            .get(screen)
            .map(|screen| screen.view_root)
            .filter(|&id| self.nodes.contains_key(id))
            .unwrap_or(self.root)
    }

    /// The screen's view root's path in the node tree
    pub fn view_root_path(&self, screen: usize) -> TreeNodePath {
        self.path_of(self.screen_view_root(screen))
            .unwrap_or_else(TreeNodePath::new_root)
    }

    /// The screen's view root's parent: its path and label
    pub fn view_parent(&self, screen: usize) -> Option<(TreeNodePath, String)> {
        let parent = self.nodes[self.screen_view_root(screen)].parent?;
        Some((self.path_of(parent)?, self.label(parent)))
    }

    /// Where the screen's focus is: `get_focus_path` for the active one,
    /// else the node it was left on, and inside it the applet's focus on that screen if it has one per screen
    pub fn screen_focus_path(&self, screen: usize) -> WorldTreePath {
        if screen == self.active_screen {
            return self.get_focus_path();
        }
        let Some(&Screen {
            focus: (id, inner), ..
        }) = self.screens.get(screen)
        else {
            return WorldTreePath::root();
        };
        let mut path =
            WorldTreePath::from_tree_path(&self.path_of(id).unwrap_or_else(TreeNodePath::new_root));
        if inner && let Some(main) = self.nodes.get(id).and_then(|node| node.main.as_ref()) {
            let treeview = main.get_treeview();
            if treeview.screen_windows
                && let Some(focus) = treeview.screen_focus.get(screen)
            {
                path.push_inner(focus);
            }
        }
        path
    }

    /// Whether keys on the screen go into an applet, rather than to a selector
    pub fn screen_focus_inside(&self, screen: usize) -> bool {
        if screen == self.active_screen {
            return !self.is_traversing();
        }
        self.screens.get(screen).is_some_and(|shown| shown.focus.1)
    }

    pub const fn active_screen(&self) -> usize {
        self.active_screen
    }

    /// Adds or removes screens; new ones start with the active one's focus and view root
    pub fn set_screen_count(&mut self, count: usize) {
        let count = count.max(1);
        if self.active_screen >= count {
            self.activate_screen(count - 1);
        }
        let focused = self.focused_node();
        let focus = (focused, self.is_inner(focused));
        let view_root = self.view_root;
        self.screens.resize_with(count, || Screen {
            size: None,
            panes: Vec::new(),
            focus,
            view_root,
        });
        self.damaged = true;
    }

    /// The screen's size (`None`: not shown), `WindowResized` for the active one
    pub fn resize_screen(&mut self, screen: usize, size: Option<DisplayContainerSize>) {
        if screen == self.active_screen {
            if let Some(size) = size {
                self.handle_event(StandardEvent::UIEvent(UIEvent::WindowResized(size)));
            }
        } else if let Some(screen) = self.screens.get_mut(screen)
            && screen.size != size
        {
            screen.size = size;
            if size.is_none() {
                screen.panes.clear();
            }
            self.damaged = true;
        }
    }

    /// Keys and the treeview go to the screen from now on: focus goes back where it was left on it
    /// (inside an applet, where the applet's own focus is now)
    pub fn activate_screen(&mut self, screen: usize) {
        if screen == self.active_screen || screen >= self.screens.len() {
            return;
        }
        let focused = self.focused_node();
        let left = Screen {
            size: self.latest_size,
            panes: std::mem::take(&mut self.panes),
            focus: (focused, self.is_inner(focused)),
            view_root: self.view_root,
        };
        let Screen {
            size,
            panes,
            focus: (id, inner),
            view_root,
        } = std::mem::replace(&mut self.screens[screen], left);
        self.screens.swap(screen, self.active_screen);
        self.active_screen = screen;
        self.latest_size = size;
        self.panes = panes;
        self.pointer_grab = None;
        self.view_root = Some(view_root)
            .filter(|&id| self.nodes.contains_key(id))
            .unwrap_or(self.root);
        if self.contains(self.view_root, id) {
            self.focus(id, inner && self.has_main(id));
            self.wake_focused();
        }
        self.damaged = true;
        // now, so the applets get the events that follow on the screen too (eg the click that activated it)
        self.send_screen_views();
    }

    pub fn screen_count(&self) -> usize {
        self.screens.len()
    }

    /// The node the screen's focus is on (as of the last `update_layout` for an inactive one)
    pub fn screen_focused_node(&self, screen: usize) -> NodeId {
        if screen == self.active_screen {
            return self.focused_node();
        }
        self.screens
            .get(screen)
            .map(|shown| shown.focus.0)
            .filter(|&id| self.nodes.contains_key(id))
            .unwrap_or(self.root)
    }

    pub fn screen_size(&self, screen: usize) -> Option<DisplayContainerSize> {
        if screen == self.active_screen {
            return self.latest_size;
        }
        self.screens.get(screen)?.size
    }

    /// Whether a pane has the pointer until a button is released
    pub const fn has_pointer_grab(&self) -> bool {
        self.pointer_grab.is_some()
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.nodes[id].children
    }

    /// New children of the node go inside its main (see `NodularAppletInitializer::holds_children`)
    pub fn holds_children(&self, id: NodeId) -> bool {
        self.nodes
            .get(id)
            .and_then(|node| node.main.as_ref())
            .is_some_and(|main| main.get_treeview().holds_children)
    }

    /// Its main frames what it shows itself (see `StandardApplet::frames_itself`)
    /// Its window on `screen` frames itself
    fn frames_itself(&self, id: NodeId, screen: usize) -> bool {
        self.nodes[id].main.as_ref().is_some_and(|main| {
            let treeview = main.get_treeview();
            treeview
                .screen_frames_itself
                .get(screen)
                .copied()
                .filter(|_| treeview.screen_windows)
                .unwrap_or(treeview.frames_itself)
        })
    }

    /// Its applet's treeview label (or `EMPTY_LABEL`)
    pub fn label(&self, id: NodeId) -> String {
        self.applet_treeview(id).get_root_value().clone()
    }

    /// Its main's inner tree (or `EMPTY_LABEL`), marked `▾` if it has inner elements or children,
    /// or `▸` and hidden if collapsed (its children too, see `get_treeview_of`)
    fn applet_treeview(&self, id: NodeId) -> WorldTree<String> {
        let node = &self.nodes[id];
        let mut treeview = match &node.main {
            None => WorldTree::Base(EMPTY_LABEL.to_string()),
            Some(main) => {
                let published = main.get_treeview();
                let mut treeview = published.tree;
                if let Some(icon) = icon(published.saved.as_ref()) {
                    root_label_mut(&mut treeview).insert_str(0, &format!("{icon} "));
                }
                if node.inactive {
                    root_label_mut(&mut treeview).insert_str(0, INACTIVE_MARK);
                }
                treeview
            }
        };
        if node.collapsed {
            return WorldTree::Base(format!("▸ {}", treeview.get_root_value()));
        }
        if has_inner_elements(&treeview) || !node.children.is_empty() {
            root_label_mut(&mut treeview).insert_str(0, "▾ ");
        }
        treeview
    }

    /// Its applet's own label (`EMPTY_LABEL` without one), for commands
    pub fn applet_title(&self, id: NodeId) -> String {
        self.nodes[id].main.as_ref().map_or_else(
            || EMPTY_LABEL.to_string(),
            |main| main.get_treeview().tree.get_root_value().clone(),
        )
    }

    /// The registered spawner its applet is from, or `wl_surface`, `container` (no main) or `applet` (unsaved,
    /// eg the help), for commands
    pub fn applet_type(&self, id: NodeId) -> String {
        let node = &self.nodes[id];
        if self.wl_surfaces.values().any(|&node_id| node_id == id) {
            return "wl_surface".to_string();
        }
        let Some(main) = &node.main else {
            return "container".to_string();
        };
        let saved = node.hibernated.clone().or(main.get_treeview().saved);
        match saved {
            Some(SavedApplet::Spawner { name, .. }) => name,
            Some(SavedApplet::Wl(_)) => "wl_surface".to_string(),
            None => "applet".to_string(),
        }
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes[id].parent
    }

    /// Its icon and label, without the treeview's marks (eg its title in the zoom-out view)
    fn title(&self, id: NodeId) -> String {
        let Some(main) = &self.nodes[id].main else {
            return EMPTY_LABEL.to_string();
        };
        let published = main.get_treeview();
        let label = published.tree.get_root_value();
        icon(published.saved.as_ref())
            .map_or_else(|| label.clone(), |icon| format!("{icon} {label}"))
    }

    /// Starts the focused node's wl client if it is hibernating
    pub fn wake_focused(&mut self) {
        let id = self.focused_node();
        let Some(saved) = self.nodes[id].hibernated.take() else {
            return;
        };
        let (initializer, awaiting) = match saved {
            SavedApplet::Wl(wl) => session::wake(&wl),
            saved => (session::restore(saved), None),
        };
        self.replace_main(id, initializer, awaiting);
    }

    /// Starts the saved wl client in the node, in place of its main (eg a prompt to restore it)
    pub fn start_wl(&mut self, id: NodeId, wl: &SavedWl) {
        if !self.nodes.contains_key(id) {
            return;
        }
        let (initializer, awaiting) = session::start(wl);
        self.replace_main(id, initializer, awaiting);
    }

    /// Runs `initializer` in the node in place of its main
    pub fn replace_applet(&mut self, id: NodeId, initializer: NodularAppletInitializer) {
        if self.nodes.contains_key(id) {
            self.replace_main(id, initializer, None);
        }
    }

    fn replace_main(
        &mut self,
        id: NodeId,
        initializer: NodularAppletInitializer,
        awaiting: Option<AwaitedWl>,
    ) {
        let main = (self.spawn_client)(id, initializer);
        let node = &mut self.nodes[id];
        main.send_event(StandardEvent::NodeUid(node.uid));
        node.main = Some(main);
        node.awaiting = awaiting;
        node.inactive = false;
        // so the new applet gets its size
        node.size = None;
        node.screen_views = None;
        self.send_initial_size(id);
        self.damaged = true;
    }

    /// Marks the node active or inactive (grey in the treeview)
    pub fn set_inactive(&mut self, id: NodeId, inactive: bool) {
        if let Some(node) = self.nodes.get_mut(id)
            && std::mem::replace(&mut node.inactive, inactive) != inactive
        {
            self.damaged = true;
        }
    }

    /// Hides or shows the inner tree of the node's main in the treeview, if it has one
    pub fn toggle_collapsed(&mut self, id: NodeId) {
        let Some(node) = self.nodes.get(id) else {
            return;
        };
        let collapsible = node.collapsed
            || !node.children.is_empty()
            || node
                .main
                .as_ref()
                .is_some_and(|main| has_inner_elements(&main.get_treeview().tree));
        if collapsible {
            self.nodes[id].collapsed ^= true;
            self.damaged = true;
        }
    }

    /// Collapses or expands what `path` of the treeview points at: the node itself if the path ends at it,
    /// otherwise something inside its main, which is asked to do it
    pub fn toggle_collapsed_path(&mut self, path: &WorldTreePath) {
        let (node_path, inner) = path.split_world();
        let Some(id) = self.node_at(&node_path) else {
            return;
        };
        match inner {
            None => self.toggle_collapsed(id),
            Some(inner) => self.send_event(id, StandardEvent::ToggleCollapsed(inner)),
        }
    }

    /// Whether focus is inside the node's applet
    pub fn is_inner(&self, id: NodeId) -> bool {
        self.nodes
            .get(id)
            .is_some_and(|node| node.main.is_some() && matches!(node.focus, FocusIndex::Here))
    }

    /// Closes the node; its parent adopts its children in its place.
    /// If the focused child closed, the parent becomes focused.
    ///
    /// Returns whether something closed.
    pub fn close(&mut self, id: NodeId) -> bool {
        let Some(node) = self.nodes.get(id) else {
            return false;
        };
        let Some(parent_id) = node.parent else {
            log::warn!("Tried to close root");
            return false;
        };

        if self.view_root == id {
            self.view_root = parent_id;
        }
        let closed = self.nodes.remove(id).unwrap();
        command::unregister(closed.uid);
        sonamu_script::emit("close", vec![Value::Node(closed.uid)]);
        for &orphan in &closed.children {
            self.nodes[orphan].parent = Some(parent_id);
        }
        let parent = &mut self.nodes[parent_id];
        let index = parent
            .children
            .iter()
            .position(|&child| child == id)
            .unwrap();
        parent
            .children
            .splice(index..=index, closed.children.iter().copied());
        parent.focus = match parent.focus {
            FocusIndex::Child(focused) if focused == index => FocusIndex::Here,
            FocusIndex::Child(focused) if focused > index => {
                FocusIndex::Child(focused + closed.children.len() - 1)
            }
            focus => focus,
        };
        if matches!(parent.focus, FocusIndex::Here) {
            self.selected(parent_id);
        }

        self.wl_surfaces.retain(|_, node_id| *node_id != id);
        self.damaged = true;
        true
    }

    /// Follows focused children down to the node that is focusing or focused on its main
    pub fn focused_node(&self) -> NodeId {
        let mut id = self.root;
        while let FocusIndex::Child(child_index) = self.nodes[id].focus {
            id = self.nodes[id].children[child_index];
        }
        id
    }

    pub fn focused_wl_surface(&self) -> Option<WlSurfaceId> {
        let id = self.focused_node();
        self.wl_surfaces
            .iter()
            .find_map(|(&surface_id, &node_id)| (node_id == id).then_some(surface_id))
            .or(self.nodes[id].inner_wl_focus)
    }

    pub fn handle_event(&mut self, event: StandardEvent) {
        match event {
            // shown panes get their sizes in `update_layout`
            StandardEvent::UIEvent(UIEvent::WindowResized(size)) => {
                self.latest_size = Some(size);
                self.damaged = true;
                return;
            }
            StandardEvent::UIEvent(UIEvent::Mouse(mouse_event)) => {
                self.handle_mouse(mouse_event);
                return;
            }
            StandardEvent::UIEvent(UIEvent::Pinch(phase)) if self.zoomed_out() => {
                self.zoom_pinch(phase);
                return;
            }
            _ => {}
        }

        let id = self.focused_node();
        let node = &mut self.nodes[id];
        if let Some(main) = &node.main {
            main.send_event(event);
            return;
        }

        // a node without a main (eg a container) is focused itself, so this is the treeview selector
        let StandardEvent::UIEvent(ui_event) = event else {
            return;
        };
        if let Some(request) = standard_keybind_request(&ui_event) {
            // on the node itself, eg Super+P wraps it
            self.keybind_request = Some((id, request));
            return;
        }
        // releases, and modifiers on their own, stay in the selector
        let UIEvent::Key {
            symbol: Some(key),
            modifiers,
            pressed: true,
            ..
        } = ui_event
        else {
            return;
        };
        // traversal keys are the root's (`traversal`)
        if modifiers.shift
            && let Some(shift) = match key {
                KeySymbol::ArrowKeyLeft => Some(-1),
                KeySymbol::ArrowKeyRight => Some(1),
                _ => None,
            }
        {
            self.swap_zoom_center(shift);
        } else if let Some(zoom_move) = match key {
            KeySymbol::ArrowKeyLeft => Some(Traversal::Sibling(-1)),
            KeySymbol::ArrowKeyRight => Some(Traversal::Sibling(1)),
            KeySymbol::ArrowKeyUp => Some(Traversal::Parent),
            KeySymbol::ArrowKeyDown => Some(Traversal::Child(0)),
            _ => None,
        } {
            self.move_zoom(zoom_move);
        } else if matches!(key, KeySymbol::Char(' ')) {
            self.toggle_collapsed(id);
        } else if matches!(key, KeySymbol::Enter)
            && let Some(center) = self.zoom_center()
        {
            // into the zoom-out view's center
            self.focus(center, false);
        }
    }

    /// Whether a node without a main is focused at the end of the focus path (the treeview selector),
    /// rather than an applet. Asks the main if focus is in one (eg a project's own selector).
    pub fn is_traversing(&self) -> bool {
        self.nodes[self.focused_node()]
            .main
            .as_ref()
            .is_none_or(|main| main.get_treeview().traversing)
    }

    /// Sends the pane under the pointer (or the one holding the pointer grab) the event, relative to it.
    /// A press focuses the pane's applet.
    fn handle_mouse(&mut self, mut mouse_event: MouseEvent) {
        if self.zoomed_out() {
            match mouse_event.kind {
                MouseEventKind::Scroll { delta, stop, .. } => {
                    if let Some(size) = self.latest_size {
                        self.zoom_scroll(size, delta, stop);
                    }
                }
                MouseEventKind::Button {
                    button: BTN_LEFT,
                    pressed,
                } => {
                    if let Some(size) = self.latest_size {
                        self.zoom_button(size, mouse_event.position, pressed);
                    }
                }
                MouseEventKind::Motion => self.zoom_motion(mouse_event.position),
                _ => {}
            }
            return;
        }
        let pane = match self.pointer_grab {
            Some(id) => self.panes.iter().find(|pane| pane.node == id),
            None => self
                .panes
                .iter()
                .find(|pane| pane.contains(mouse_event.position)),
        };
        let Some(&pane) = pane else {
            return;
        };

        if let MouseEventKind::Button { pressed, .. } = mouse_event.kind {
            self.pointer_grab = pressed.then_some(pane.node);
            // panes are from the last `update_layout`, so the node may have closed since
            if pressed
                && self
                    .nodes
                    .get(pane.node)
                    .is_some_and(|node| node.main.is_some())
            {
                self.focus_node(pane.node);
                self.nodes[pane.node].focus = FocusIndex::Here;
            }
        }
        mouse_event.position[0] -= f64::from(pane.rect[0]) + f64::from(pane.border);
        mouse_event.position[1] -= f64::from(pane.rect[1]) + f64::from(pane.border);
        self.send_event(
            pane.node,
            StandardEvent::UIEvent(UIEvent::Mouse(mouse_event)),
        );
    }

    /// The node `id` shows, following the children `focus` is under
    /// (eg another screen's focus, which the nodes' own focus doesn't point to)
    fn displayed_node_toward(&self, mut id: NodeId, focus: NodeId) -> NodeId {
        while let Some(child) = self.shown_child(id, focus) {
            id = child;
        }
        id
    }

    /// The child the node shows instead of itself, following the children `focus` is under
    fn shown_child(&self, id: NodeId, focus: NodeId) -> Option<NodeId> {
        let node = &self.nodes[id];
        if let Some(&child) = node
            .children
            .iter()
            .find(|&&child| self.contains(child, focus))
        {
            return Some(child);
        }
        match node.focus {
            // the focus is on this node itself (its own `focus` may point to another screen's)
            FocusIndex::Child(index) if id != focus => Some(node.children[index]),
            // a container has nothing of its own to show
            _ if node.main.is_none() && !node.children.is_empty() => {
                Some(node.children[node.last_child.min(node.children.len() - 1)])
            }
            _ => None,
        }
    }

    /// Works out what is on screen, and resizes the shown applets to their panes.
    /// An applet on several screens gets the active one's pane, else the first one's.
    pub fn update_layout(&mut self) {
        self.update_zoom_anim();
        let mut shown = Vec::new();
        if let Some(size) = self.latest_size {
            let panes = self.layout(
                self.view_root,
                self.focused_node(),
                size,
                self.active_screen,
                true,
                None,
            );
            for pane in &panes {
                self.resize(pane.node, pane.inner_size());
            }
            if self
                .pointer_grab
                .is_some_and(|id| !panes.iter().any(|pane| pane.node == id))
            {
                self.pointer_grab = None;
            }
            shown.extend(panes.iter().map(|pane| pane.node));
            self.panes = panes;
            self.layout_swipe(size);
        }
        for index in 0..self.screens.len() {
            if index == self.active_screen {
                continue;
            }
            // its nodes may have gone meanwhile
            let screen = &self.screens[index];
            let view_root = Some(screen.view_root)
                .filter(|&id| self.nodes.contains_key(id))
                .unwrap_or(self.root);
            let focus = Some(screen.focus.0)
                .filter(|&id| self.contains(view_root, id))
                .unwrap_or(view_root);
            let Some(size) = screen.size else {
                continue;
            };
            let panes = self.layout(view_root, focus, size, index, false, None);
            for pane in &panes {
                if !shown.contains(&pane.node) {
                    self.resize(pane.node, pane.inner_size());
                    shown.push(pane.node);
                }
            }
            let screen = &mut self.screens[index];
            screen.view_root = view_root;
            if screen.focus.0 != focus {
                screen.focus = (focus, false);
            }
            screen.panes = panes;
        }
        self.send_screen_views();
    }

    /// Tells every main its size on each screen and the active screen, when that changed (#60).
    /// Not with a single screen, unless they were told of several before.
    fn send_screen_views(&mut self) {
        let count = self.screens.len();
        let mut sizes: HashMap<NodeId, Vec<Option<DisplayContainerSize>>> = HashMap::new();
        for (index, screen) in self.screens.iter().enumerate() {
            let panes = if index == self.active_screen {
                &self.panes
            } else {
                &screen.panes
            };
            for pane in panes {
                sizes.entry(pane.node).or_insert_with(|| vec![None; count])[index] =
                    Some(pane.inner_size());
            }
        }
        let active = self.active_screen;
        for (id, node) in &mut self.nodes {
            let Some(main) = &node.main else {
                continue;
            };
            if count == 1 && node.screen_views.is_none() {
                continue;
            }
            let views = (
                sizes.remove(&id).unwrap_or_else(|| vec![None; count]),
                active,
            );
            if node.screen_views.as_ref() != Some(&views) {
                main.send_event(StandardEvent::ScreenViews {
                    sizes: views.0.clone(),
                    active,
                });
                node.screen_views = Some(views);
            }
        }
    }

    /// The panes showing `view_root` at `size`, with focus on `focus`
    /// (on `screen`); `remember` keeps how far the strips are scrolled, `fixed` scrolls one strip this far
    fn layout(
        &self,
        view_root: NodeId,
        focus: NodeId,
        size: DisplayContainerSize,
        screen: usize,
        remember: bool,
        fixed: Option<(NodeId, i32)>,
    ) -> Vec<Pane> {
        let settings = settings::get();
        let (gap, border) = settings.gap_and_border_px();
        let gap = gap.cast_signed();
        let full = [0, 0, size.width.cast_signed(), size.height.cast_signed()];
        let area = grow(full, -gap);
        let mut panes = Vec::new();
        let strips = Strips {
            focus,
            gap,
            remember,
            fixed,
            whole: false,
        };
        self.push_panes(view_root, area, full, &strips, &mut panes);
        for pane in &mut panes {
            pane.border = border;
        }
        if let (false, [pane]) = (settings.always_frame, panes.as_mut_slice())
            && pane.rect == area
        {
            pane.rect = full;
            pane.border = 0;
            pane.framed = false;
        }
        for pane in panes.iter_mut().filter(|pane| pane.framed) {
            if self.frames_itself(pane.node, screen) {
                // it frames its own content, with its own gap around it, so it takes this gap back
                pane.rect = grow(pane.rect, gap);
                pane.border = 0;
                pane.framed = false;
            }
        }
        panes
    }

    /// The panes showing `id` in `rect`, cut to `clip`. A node's children are a strip (#144): side by side at their
    /// widths (half ones `Super+F`), scrolled as little as can be from where it was to show the one it shows whole.
    /// Only the last strip toward what it shows is on screen: a child showing one of its own takes the whole `rect`.
    fn push_panes(
        &self,
        mut id: NodeId,
        mut rect: [i32; 4],
        mut clip: [i32; 4],
        strips: &Strips,
        panes: &mut Vec<Pane>,
    ) {
        let Strips {
            focus,
            gap,
            remember,
            fixed,
            whole,
        } = *strips;
        while let Some(child) = self.shown_child(id, focus) {
            let fixed = fixed.filter(|&(strip, _)| strip == id);
            if fixed.is_none() && self.shown_child(child, focus).is_some() {
                id = child;
                continue;
            }
            let node = &self.nodes[id];
            let [x, y, width, height] = rect;
            let widths: Vec<i32> = node
                .children
                .iter()
                .map(|&child| {
                    if self.nodes[child].half {
                        (width - gap) / 2
                    } else {
                        width
                    }
                })
                .collect();
            let starts: Vec<i32> = widths
                .iter()
                .scan(0, |start, &width| {
                    let this = *start;
                    *start += width + gap;
                    Some(this)
                })
                .collect();
            let index = self.child_index(id, child);
            let end = starts
                .last()
                .zip(widths.last())
                .map_or(0, |(start, width)| start + width);
            let scroll = fixed.map_or_else(
                || {
                    node.scroll
                        .get()
                        .min(end - width)
                        .max(0)
                        .min(starts[index])
                        .max(starts[index] + widths[index] - width)
                },
                |(_, scroll)| scroll,
            );
            node.laid_scroll.set(scroll);
            if remember {
                node.scroll.set(scroll);
            }
            if !whole {
                clip = intersect(clip, rect);
            }
            for (sibling, (&start, &column)) in node.children.iter().zip(starts.iter().zip(&widths))
            {
                let left = x + start - scroll;
                // scrolled to a fixed place, the shown child may be partly in sight too
                if (*sibling != child || fixed.is_some())
                    && (whole || (left < x + width && left + column > x))
                {
                    panes.push(Pane {
                        node: self.displayed_node_toward(*sibling, focus),
                        rect: [left, y, column, height],
                        clip,
                        border: 0,
                        framed: true,
                    });
                }
            }
            if fixed.is_some() {
                return;
            }
            rect = [x + starts[index] - scroll, y, widths[index], height];
            id = child;
        }
        panes.push(Pane {
            node: id,
            rect,
            clip,
            border: 0,
            framed: true,
        });
    }

    /// Draws the swipe if both its ends are nodes of this world, else passes it into the node whose inner world
    /// has both (a node and its inner root are one), or stops it (`None`, or ends in different worlds)
    pub fn set_swipe(&mut self, view: Option<SwipeView>) {
        if view.is_none() && self.swipe.is_none() && self.swipe_sent_to.is_none() {
            return;
        }
        /// Where the swipe's `to` is: a node of this world, or (with `from`) inside `from`'s node
        enum To {
            Here(Option<NodeId>),
            Inside(Option<WorldTreePath>),
        }
        let mut draw = None;
        let mut pass = None;
        if let Some(view) = view {
            let (from, from_inner) = view.from.split_world();
            let to = match view.to.as_ref().map(WorldTreePath::split_world) {
                None if from_inner.is_none() => Some(To::Here(None)),
                None => Some(To::Inside(None)),
                Some((to, None)) if from_inner.is_none() => {
                    self.node_at(&to).map(|to| To::Here(Some(to)))
                }
                Some((to, to_inner)) if to == from => {
                    Some(To::Inside(Some(to_inner.unwrap_or_default())))
                }
                Some(_) => None,
            };
            match (self.node_at(&from), to) {
                (Some(from), Some(To::Here(to))) => {
                    draw = Some((from, to, view.vertical, view.progress))
                }
                (Some(node), Some(To::Inside(to))) => {
                    let view = SwipeView {
                        from: from_inner.unwrap_or_default(),
                        to,
                        ..view
                    };
                    pass = Some((node, view));
                }
                _ => {}
            }
        }
        let pass_to = pass.as_ref().map(|(node, _)| *node);
        if self.swipe_sent_to != pass_to
            && let Some(old) = self.swipe_sent_to.take()
        {
            self.send_event(old, StandardEvent::Swipe(None));
        }
        if let Some((node, view)) = pass {
            self.send_event(node, StandardEvent::Swipe(Some(view)));
            self.swipe_sent_to = Some(node);
        }
        self.swipe = draw.map(|(from, to, vertical, progress)| TreeSwipe {
            vertical,
            from,
            to,
            progress,
            panes: Default::default(),
            strip: None,
        });
        if let Some(size) = self.latest_size {
            self.layout_swipe(size);
        }
        self.damaged = true;
    }

    /// What the swipe's ends show (each as if focused), resizing their applets
    fn layout_swipe(&mut self, size: DisplayContainerSize) {
        let Some(swipe) = &self.swipe else {
            return;
        };
        let ends = [Some(swipe.from), swipe.to];
        // siblings: the strip they are in
        let parent = self.nodes[swipe.from].parent.filter(|&parent| {
            swipe
                .to
                .is_some_and(|to| self.nodes[to].parent == Some(parent))
        });
        let mut scrolls = [0; 2];
        let panes = ends.map(|end| {
            end.filter(|&id| self.nodes.contains_key(id))
                .map(|id| {
                    let panes = self.layout(
                        self.view_root_for(id),
                        id,
                        size,
                        self.active_screen,
                        false,
                        None,
                    );
                    if let Some(parent) = parent {
                        let index = usize::from(Some(id) == swipe.to);
                        scrolls[index] = self.nodes[parent].laid_scroll.get();
                    }
                    panes
                })
                .unwrap_or_default()
        });
        for pane in panes.iter().flatten() {
            self.resize(pane.node, pane.inner_size());
        }
        if let Some(swipe) = &mut self.swipe {
            swipe.panes = panes;
            swipe.strip = parent.map(|parent| (parent, scrolls));
        }
    }

    /// The view root to lay out `id` focused: the view root if it holds it, else `id` itself
    fn view_root_for(&self, id: NodeId) -> NodeId {
        if self.contains(self.view_root, id) {
            self.view_root
        } else {
            id
        }
    }

    /// Every pane, bordered unless it is the only one; the focused one's border shows whether focus is inside it.
    /// Focus ending at a node without a main (eg the root) shows the zoom-out view instead (#145).
    pub fn get_window(&self) -> UITree {
        if (self.zoomed_out() || self.zoom_progress().is_some())
            && let Some(size) = self.latest_size
        {
            return self.zoom_window(size);
        }
        self.get_window_with(None)
    }

    /// Whether the zoom-out view is shown, so panes don't take the pointer
    fn zoomed_out(&self) -> bool {
        self.swipe.is_none() && self.nodes[self.focused_node()].main.is_none()
    }

    /// Starts the zoom-out view springing when focus went to or from a node without a main, and ends it once there
    fn update_zoom_anim(&mut self) {
        let zoomed = self.nodes[self.focused_node()].main.is_none();
        if self.was_zoomed.is_some_and(|was| was != zoomed) {
            let center = if zoomed {
                self.zoom_center()
            } else {
                Some(self.focused_node())
            };
            self.zoom_anim = center.map(|center| ZoomAnim {
                started: Instant::now(),
                out: zoomed,
                center,
            });
        }
        self.was_zoomed = Some(zoomed);
        if self
            .zoom_anim
            .is_some_and(|anim| swipe::spring_progress(anim.started).is_none())
        {
            self.zoom_anim = None;
        }
        if self
            .zoom_pan
            .is_some_and(|(started, ..)| swipe::spring_progress(started).is_none())
        {
            self.zoom_pan = None;
        }
        if self
            .zoom_moves
            .as_ref()
            .is_some_and(|(started, _)| swipe::spring_progress(*started).is_none())
        {
            self.zoom_moves = None;
        }
        self.zoom_edge_scroll();
    }

    /// While a drag is at the zoom-out view's edges: how fast it scrolls there (px of `size` a second; toward
    /// left and up is positive)
    fn zoom_edge_speed(&self) -> Option<[f32; 2]> {
        // px from the edge where it starts, and its speed at the edge
        const MARGIN: f64 = 48.0;
        const SPEED: f64 = 600.0;
        let press = self
            .zoom_press
            .filter(|press| Self::zoom_dragged(press.from, press.at))?;
        let size = [press.size.width, press.size.height].map(f64::from);
        let depth = |distance: f64| (1.0 - distance / MARGIN).clamp(0.0, 1.0);
        let depths = [0, 1].map(|axis| depth(press.at[axis]) - depth(size[axis] - press.at[axis]));
        depths
            .iter()
            .any(|&depth| depth != 0.0)
            .then(|| depths.map(|depth| (depth * SPEED) as f32))
    }

    /// Scrolls the zoom-out view while a drag is at its edges
    fn zoom_edge_scroll(&mut self) {
        let speed = self.zoom_edge_speed();
        let scale = self
            .zoom_press
            .and_then(|press| self.zoom_full_scale(press.size));
        let Some(press) = &mut self.zoom_press else {
            return;
        };
        let now = Instant::now();
        let elapsed = now.duration_since(press.tick).as_secs_f32().min(0.1);
        press.tick = now;
        if let (Some(scale), Some(speed)) = (scale, speed) {
            for (scroll, speed) in press.scroll.iter_mut().zip(speed) {
                *scroll += speed * elapsed / scale;
            }
            self.damaged = true;
        }
    }

    /// A pinch in the zoom-out view scales it: apart, the views get bigger
    pub fn zoom_pinch(&mut self, phase: PinchPhase) {
        let (pinched, began) = &mut self.zoom_pinched;
        match phase {
            PinchPhase::Begin { .. } => *began = Some(*pinched),
            PinchPhase::Update { scale, .. } => {
                if let Some(began) = began {
                    *pinched = *began * scale as f32;
                }
            }
            PinchPhase::End { cancelled } => {
                if let Some(began) = began.take()
                    && cancelled
                {
                    *pinched = began;
                }
            }
        }
        self.damaged = true;
    }

    /// Whether the zoom-out view is springing (zooming, panning or moving panes), so holders redraw
    pub fn zoom_animating(&self) -> bool {
        self.zoom_anim.is_some()
            || self.zoom_pan.is_some()
            || self.zoom_moves.is_some()
            || self.zoom_edge_speed().is_some()
    }

    /// How far off its center the zoom-out view still is while panning to it (like `zoom_pan`)
    fn zoom_pan_offset(&self) -> ([f32; 2], f32, bool) {
        self.zoom_pan.map_or(
            ([0.0; 2], 0.0, false),
            |(started, offset, rows, lateral)| {
                let left = 1.0 - swipe::spring_progress(started).unwrap_or(1.0);
                (offset.map(|value| value * left), rows * left, lateral)
            },
        )
    }

    /// While the zoom-out view springs: how zoomed out it is, 0 (its center's view) to 1; holders redraw meanwhile
    pub fn zoom_progress(&self) -> Option<f32> {
        let anim = self.zoom_anim?;
        let progress = swipe::spring_progress(anim.started).unwrap_or(1.0);
        Some(if anim.out { progress } else { 1.0 - progress })
    }

    /// The child focus was last under
    fn remembered_child(&self, id: NodeId) -> Option<NodeId> {
        let node = &self.nodes[id];
        node.children
            .get(node.last_child.min(node.children.len().saturating_sub(1)))
            .copied()
    }

    /// The zoom-out view's center: where focus last was below the focused node, else its remembered child
    fn zoom_center(&self) -> Option<NodeId> {
        let focused = self.focused_node();
        if self.nodes.contains_key(self.last_focus)
            && self.last_focus != focused
            && self.contains(focused, self.last_focus)
        {
            return Some(self.last_focus);
        }
        self.remembered_child(focused)
    }

    /// Moves the zoom-out view's center like 3 fingers (siblings, parent, remembered child), staying below the
    /// focused node; focus stays
    pub fn move_zoom(&mut self, traversal: Traversal) {
        let focused = self.focused_node();
        let Some(center) = self.zoom_center() else {
            return;
        };
        let target = match traversal {
            Traversal::Sibling(shift) => self.nodes[center].parent.and_then(|parent| {
                let index = self.child_index(parent, center).checked_add_signed(shift)?;
                self.nodes[parent].children.get(index).copied()
            }),
            Traversal::Parent => self.nodes[center]
                .parent
                .filter(|&parent| parent != focused),
            Traversal::Child(_) => self.remembered_child(center),
            _ => None,
        };
        if let Some(target) = target {
            self.set_zoom_center(target);
        }
    }

    /// Swaps the zoom-out view's center with a sibling; it stays the center
    fn swap_zoom_center(&mut self, shift: isize) {
        if let Some(center) = self.zoom_center() {
            self.move_zoom_panes(|tree| {
                tree.swap_sibling(center, shift);
                tree.remember_zoom_center(center);
            });
        }
    }

    /// Makes the node (below the focused one) the zoom-out view's center; its ancestors remember it, so the rows
    /// below follow it
    fn set_zoom_center(&mut self, id: NodeId) {
        self.pan_zoom(|tree| tree.remember_zoom_center(id));
    }

    /// Like `set_zoom_center`, without panning
    fn remember_zoom_center(&mut self, id: NodeId) {
        let focused = self.focused_node();
        self.last_focus = id;
        let mut child = id;
        while child != focused
            && let Some(parent) = self.nodes[child].parent
        {
            self.nodes[parent].last_child = self.child_index(parent, child);
            child = parent;
        }
        self.damaged = true;
    }

    /// Changes the zoom-out view, panning from where the old center was, carrying on a pan underway. Lateral if
    /// the center stays among the same siblings.
    fn pan_zoom(&mut self, change: impl FnOnce(&mut Self)) {
        let old = self.zoom_center();
        let before = old.and_then(|old| Some((self.nodes[old].parent, self.zoom_position(old)?)));
        let (carried, carried_rows, carried_lateral) = self.zoom_pan_offset();
        change(self);
        if let (Some(old), Some((old_parent, (before, before_row)))) = (old, before)
            && let Some((after, after_row)) = self.zoom_position(old)
        {
            let offset = [0, 1].map(|axis| (before[axis] - after[axis]) as f32 + carried[axis]);
            let rows = (before_row - after_row) as f32 + carried_rows;
            let siblings =
                self.zoom_center().map(|center| self.nodes[center].parent) == Some(old_parent);
            let lateral =
                siblings && before_row == after_row && (carried_lateral || self.zoom_pan.is_none());
            if !Self::zoom_free_view() {
                self.zoom_pan = Some((Instant::now(), offset, rows, lateral));
                return;
            }
            // free: it stays put, but for a lateral move's row sliding under its parent
            let (free, free_rows) = &mut self.zoom_free;
            let x = if lateral {
                self.zoom_pan = Some((Instant::now(), [offset[0], 0.0], 0.0, true));
                0.0
            } else {
                self.zoom_pan = None;
                offset[0]
            };
            free[0] += x;
            free[1] += offset[1];
            *free_rows += rows;
        }
    }

    /// Whether the zoom-out view floats freely (`ZoomOutView::Free`)
    fn zoom_free_view() -> bool {
        settings::get().zoom_out.view == ZoomOutView::Free
    }

    /// Changes the zoom-out view (eg moves a node), each pane sliding from where it was to its new place
    fn move_zoom_panes(&mut self, change: impl FnOnce(&mut Self)) {
        let before = self.zoom_occurrences();
        // a free view stays put
        self.pan_zoom(change);
        // `before` has them in
        self.zoom_pan = None;
        self.zoom_moves = None;
        let moves = self
            .zoom_occurrences()
            .into_iter()
            .filter_map(|(key, after)| {
                let before = before.get(&key)?;
                let offset = [0, 1].map(|axis| before[axis] - after[axis]);
                (offset != [0; 2]).then_some((key, offset.map(|value| value as f32)))
            })
            .collect();
        self.zoom_moves = Some((Instant::now(), moves));
        self.damaged = true;
    }

    /// Where each node's panes are in the zoom-out view (before scaling), by occurrence
    fn zoom_occurrences(&self) -> ZoomOccurrences<[i32; 2]> {
        let Some(ZoomLayout { panes, .. }) = self
            .latest_size
            .and_then(|size| self.zoom_layout(size, 1.0))
        else {
            return HashMap::new();
        };
        let mut counts = HashMap::new();
        panes
            .iter()
            .map(|pane| {
                let count = counts.entry(pane.node).or_insert(0);
                *count += 1;
                ((pane.node, *count - 1), [pane.rect[0], pane.rect[1]])
            })
            .collect()
    }

    /// A scroll in the zoom-out view drawn at `size`: past a step, moves its center one node (sideways: siblings,
    /// down: child); a free one (`ZoomOutView::Free`) just moves
    pub fn zoom_scroll(&mut self, size: DisplayContainerSize, [x, y]: [f64; 2], stop: [bool; 2]) {
        const STEP: f64 = 50.0;
        if Self::zoom_free_view() {
            if let Some(scale) = self.zoom_full_scale(size) {
                let (free, _) = &mut self.zoom_free;
                free[0] -= x as f32 / scale;
                free[1] -= y as f32 / scale;
                self.damaged = true;
            }
            return;
        }
        let [moved_x, moved_y] = &mut self.zoom_scrolled;
        *moved_x += x;
        *moved_y += y;
        let [moved_x, moved_y] = self.zoom_scrolled;
        let zoom_move = if moved_x.abs() >= STEP {
            Some(Traversal::Sibling(if moved_x > 0.0 { 1 } else { -1 }))
        } else if moved_y.abs() >= STEP {
            Some(if moved_y > 0.0 {
                Traversal::Child(0)
            } else {
                Traversal::Parent
            })
        } else {
            None
        };
        if let Some(zoom_move) = zoom_move {
            self.move_zoom(zoom_move);
        }
        if zoom_move.is_some() || stop.contains(&true) {
            self.zoom_scrolled = [0.0; 2];
        }
    }

    /// Where 4 fingers down (Alt+Enter) go from the focus, as a path of this tree: the zoom-out view's center
    /// if focus is at a node without a main, else as far into the focused main as it says (its `zoom_focus`)
    pub fn zoom_target(&self) -> Option<WorldTreePath> {
        let world_path = |id| {
            let path = self.path_of(id)?;
            Some(WorldTreePath(path.0.into_iter().map(Step::Child).collect()))
        };
        let focused = self.focused_node();
        let Some(main) = &self.nodes[focused].main else {
            return world_path(self.zoom_center()?);
        };
        // from the node itself: `zoom_focus` is from its main's inner root
        let mut path = world_path(focused)?;
        path.push_inner(&main.get_treeview().zoom_focus.clone()?);
        Some(path)
    }

    /// The zoom-out view's rows, each in its view's own place
    fn zoom_rows(&self) -> Option<ZoomRows> {
        let view = self.latest_size?;
        let (gap, border) = settings::get().gap_and_border_px();
        let gap = gap.cast_signed();
        let area = grow(
            [0, 0, view.width.cast_signed(), view.height.cast_signed()],
            -gap,
        );
        let unclipped = [i32::MIN / 2, i32::MIN / 2, i32::MAX, i32::MAX];
        // zooming in, on the node going in
        let center = match self.zoom_anim {
            Some(anim) if !anim.out && self.nodes.contains_key(anim.center) => anim.center,
            _ => self.zoom_center().unwrap_or_else(|| self.focused_node()),
        };
        let mut chain = Vec::new();
        let mut id = center;
        while id != self.view_root
            && let Some(parent) = self.nodes[id].parent
        {
            chain.push(id);
            id = parent;
        }
        chain.reverse();
        let mut id = center;
        while let Some(child) = self.remembered_child(id) {
            chain.push(child);
            id = child;
        }

        // a container shows the same row as the child it shows
        let mut rows: Vec<(NodeId, Vec<Pane>, bool)> = Vec::new();
        for id in chain {
            let strips = Strips {
                focus: id,
                gap,
                remember: false,
                fixed: None,
                whole: true,
            };
            let mut panes = Vec::new();
            self.push_panes(self.view_root, area, unclipped, &strips, &mut panes);
            for pane in &mut panes {
                pane.border = border;
            }
            let same = |(_, row, _): &(NodeId, Vec<Pane>, bool)| {
                row.iter()
                    .map(|pane| pane.node)
                    .eq(panes.iter().map(|pane| pane.node))
            };
            match rows.last_mut() {
                // a container's row is its child's
                Some(last) if same(last) => {
                    last.0 = id;
                    last.2 |= id == center;
                }
                _ => rows.push((id, panes, id == center)),
            }
        }
        Some(ZoomRows {
            center_row: rows.iter().position(|(_, _, center)| *center).unwrap_or(0),
            rows: rows
                .into_iter()
                .map(|(focus, panes, _)| (focus, panes))
                .collect(),
            center,
            step: view.height.cast_signed() + gap,
        })
    }

    /// Where the node's pane is in the zoom-out view: in its row's view (px, before scaling), and that row's offset from
    /// the center's. A pane shows the node it displays, eg a container's child.
    fn zoom_position(&self, id: NodeId) -> Option<([i32; 2], i32)> {
        let ZoomRows {
            rows, center_row, ..
        } = self.zoom_rows()?;
        rows.iter().enumerate().find_map(|(index, (_, panes))| {
            let pane = panes.iter().find(|pane| self.contains(id, pane.node))?;
            Some((
                [pane.rect[0], pane.rect[1]],
                index as i32 - center_row as i32,
            ))
        })
    }

    /// The zoom-out view (#145) fitted in `size`: the swipe space drawn small. A row per node down the focused node's
    /// remembered chain (ancestors above), each the view focus there gives with all its strip's siblings, centered
    /// on that view; the focused node's remembered child is in the middle, a thin box around the views' column.
    pub fn zoom_window(&self, size: DisplayContainerSize) -> UITree {
        let Some(ZoomLayout {
            panes,
            row_focus,
            center,
            scale,
            column,
        }) = self.zoom_layout(size, self.zoom_progress().unwrap_or(1.0))
        else {
            return UITree::leaf(UIElement::Nothing);
        };
        let settings = settings::get();
        let border = settings.border.unfocused;
        // above each pane once zoomed out, unscaled, apart from their neighbours'
        const INSET: f32 = 4.0;
        const DROP_LINE: f32 = 3.0;
        let titles = self
            .zoom_progress()
            .is_none()
            .then(|| {
                panes.iter().map(|pane| {
                    let [x, y, width, _] = pane.rect.map(|value| value as f32 * scale);
                    UITree::leaf_styled(
                        ui::text(self.title(pane.node), settings.palette.fg),
                        Style::absolute(
                            [x + INSET, y - f32::midpoint(ui::row_height(), font_size())],
                            [(width - 2.0 * INSET).max(0.0), font_size()],
                        ),
                    )
                })
            })
            .into_iter()
            .flatten();
        // a drag's drop: a line on the side of the pane it goes to
        let drop_line = self.zoom_drop(size).map(|(_, _, side, rect)| {
            let [x, y, width, height] = rect.map(|value| value as f32 * scale);
            let (position, size) = match side {
                ZoomSide::Before => ([x - 1.0 - DROP_LINE, y], [DROP_LINE, height]),
                ZoomSide::After => ([x + width + 1.0, y], [DROP_LINE, height]),
                ZoomSide::Below => ([x, y + height + 1.0], [width, DROP_LINE]),
            };
            ui::rect(settings.palette.accent, position, size)
        });
        let marks = self.zoom_marks(&panes, &row_focus, center, scale);
        let size_style = Style::absolute([0.0, 0.0], [size.width as f32, size.height as f32]);
        // a box: nothing is drawn outside `size`
        let scaled = UITree::new_styled(
            UIElement::Scaled(scale),
            size_style.clone(),
            vec![
                UITree::new_container_styled(
                    Style::absolute(
                        [0.0, 0.0],
                        [size.width as f32 / scale, size.height as f32 / scale],
                    ),
                    std::iter::once(self.window(&panes, None, self.active_screen))
                        .chain(self.zoom_previews(&panes, &row_focus))
                        .collect(),
                ),
                ui::frame(
                    border,
                    0.0,
                    1.0 / scale,
                    Style::absolute(
                        [column[0] - 2.0 / scale, column[1] - 2.0 / scale],
                        [column[2] + 4.0 / scale, column[3] + 4.0 / scale],
                    ),
                    Vec::new(),
                ),
            ],
        );
        UITree::new_container_styled(
            size_style,
            std::iter::once(scaled)
                .chain(titles)
                .chain(drop_line)
                .chain(marks)
                .collect(),
        )
    }

    /// Frames over the zoom-out view's panes (scaled by `scale`): the center's in the focused border's color, and while
    /// dragging, the node's in the accent, what is shown of its children greyed out (rows focused inside it)
    fn zoom_marks(
        &self,
        panes: &[Pane],
        row_focus: &[NodeId],
        center: NodeId,
        scale: f32,
    ) -> Vec<UITree> {
        const WIDTH: f32 = 3.0;
        let settings = settings::get();
        let outline = |color, [x, y, width, height]: [f32; 4]| {
            ui::frame(
                color,
                settings.radius * scale,
                WIDTH,
                Style::absolute([x, y], [width, height]),
                Vec::new(),
            )
        };
        let scaled_rect = |pane: &Pane| pane.rect.map(|value| value as f32 * scale);
        let focus = panes
            .iter()
            .filter(|pane| pane.node == center)
            .map(|pane| outline(settings.border.focused, scaled_rect(pane)));
        let dragged = self
            .zoom_press
            .filter(|press| {
                self.nodes.contains_key(press.node) && Self::zoom_dragged(press.from, press.at)
            })
            .into_iter()
            .flat_map(|ZoomPress { node: id, .. }| {
                panes
                    .iter()
                    .zip(row_focus)
                    .filter(move |(pane, _)| self.contains(id, pane.node))
                    .map(move |(pane, &focus)| {
                        let [x, y, width, height] = scaled_rect(pane);
                        if focus != id && self.contains(id, focus) {
                            let mut grey = settings.palette.bg;
                            grey.0[3] = 0xB0;
                            ui::rect(grey, [x, y], [width, height])
                        } else {
                            outline(settings.palette.accent, [x, y, width, height])
                        }
                    })
            });
        focus.chain(dragged).collect()
    }

    /// Of a child preview, to a view
    const ZOOM_PREVIEW: f32 = 0.25;

    /// Room below each row of the zoom-out view for child previews (px, before scaling; none if they're off)
    fn zoom_preview_room(view_height: f32) -> f32 {
        let settings = settings::get();
        if !settings.zoom_out.child_previews {
            return 0.0;
        }
        settings.gap + view_height * Self::ZOOM_PREVIEW
    }

    /// The node beside its row's own (a sibling of the row's focus) whose place the pane is in, if it has
    /// children to preview
    fn zoom_previewed(&self, pane: &Pane, focus: NodeId) -> Option<NodeId> {
        if self.contains(focus, pane.node) || self.contains(pane.node, focus) {
            return None;
        }
        let mut slot = pane.node;
        while let Some(parent) = self.nodes[slot].parent
            && !self.contains(parent, focus)
        {
            slot = parent;
        }
        let parent = self.nodes[slot].parent?;
        (self.nodes[focus].parent == Some(parent) && !self.nodes[slot].children.is_empty())
            .then_some(slot)
    }

    /// Tiny views of the children of the nodes beside each row's own, under their panes (before scaling), as many as
    /// fit, at most 4
    fn zoom_previews(&self, panes: &[Pane], row_focus: &[NodeId]) -> Vec<UITree> {
        const LIMIT: usize = 4;
        let settings = settings::get();
        let Some(view) = self
            .latest_size
            .filter(|_| settings.zoom_out.child_previews)
        else {
            return Vec::new();
        };
        let (gap, border) = settings.gap_and_border_px();
        let gap = gap as f32;
        let [width, height] = [view.width, view.height].map(|length| length as f32);
        let mini = [width, height].map(|length| length * Self::ZOOM_PREVIEW);
        panes
            .iter()
            .zip(row_focus)
            .filter_map(|(pane, &focus)| Some((pane, self.zoom_previewed(pane, focus)?)))
            .flat_map(|(pane, slot)| {
                let [x, y, pane_width, pane_height] = pane.rect.map(|value| value as f32);
                let fit = ((pane_width + gap) / (mini[0] + gap))
                    .floor()
                    .to_px()
                    .max(1) as usize;
                self.nodes[slot]
                    .children
                    .iter()
                    .take(fit.min(LIMIT))
                    .enumerate()
                    .map(move |(index, &child)| {
                        let shown = Pane {
                            node: self.displayed_node_toward(child, child),
                            rect: [0, 0, view.width.cast_signed(), view.height.cast_signed()],
                            clip: [0, 0, view.width.cast_signed(), view.height.cast_signed()],
                            border,
                            framed: true,
                        };
                        let position = [x + index as f32 * (mini[0] + gap), y + pane_height + gap];
                        UITree::new_styled(
                            UIElement::Scaled(Self::ZOOM_PREVIEW),
                            Style::absolute(position, mini),
                            vec![UITree::new_container_styled(
                                Style::absolute([0.0, 0.0], [width, height]),
                                vec![self.window(&[shown], None, self.active_screen)],
                            )],
                        )
                    })
            })
            .collect()
    }

    /// The zoom-out view's scale in `size`, fully zoomed out: 4 × 3 views fit, times a pinch
    fn zoom_full_scale(&self, size: DisplayContainerSize) -> Option<f32> {
        // pinched no further than a view filling `size`, or a quarter
        const LEAST: f32 = 0.25;
        let view = self.latest_size?;
        // across and down (the smaller scale of the two); the center view is in the middle, so an even count shows
        // halves at the edges
        let [across, down] = [4.0, 3.0];
        let [width, height] = [view.width, view.height].map(|length| length.max(1) as f32);
        let fit = (size.width as f32 / (across * width)).min(size.height as f32 / (down * height));
        let most = (size.width as f32 / width).min(size.height as f32 / height);
        Some((fit * self.zoom_pinched.0.max(LEAST)).min(most))
    }

    /// Where `zoom_window(size)` draws everything, before scaling, `zoomed` of the way (0 to 1) from the center's
    /// view filling `size` to all of it
    fn zoom_layout(&self, size: DisplayContainerSize, zoomed: f32) -> Option<ZoomLayout> {
        let view = self.latest_size?;
        let ZoomRows {
            rows,
            center,
            center_row,
            step,
        } = self.zoom_rows()?;

        let [width, height] = [view.width, view.height].map(|length| length.max(1) as f32);
        let full_scale = self.zoom_full_scale(size)?;
        let scale = 1.0 + (full_scale - 1.0) * zoomed;
        // rows have room for their titles above them, which aren't scaled, and for child previews below them
        let step = step as f32
            + ui::row_height() * zoomed / scale
            + Self::zoom_preview_room(height) * zoomed;
        // the center view's middle at the middle of `size`, from its top left; off by the pan left
        let (pan, pan_rows, lateral) = self.zoom_pan_offset();
        let pan = [pan[0], pan[1] + pan_rows * step];
        // dragged at the edges, and floated (gone once zoomed in)
        let (free, free_rows) = self.zoom_free;
        let free = if Self::zoom_free_view() {
            [free[0], free[1] + free_rows * step].map(|value| value * zoomed)
        } else {
            [0.0; 2]
        };
        let dragged = self.zoom_press.map_or([0.0; 2], |press| press.scroll);
        let scroll = [0, 1].map(|axis| dragged[axis] + free[axis]);
        let [[x, x_still], [y, _]] = [
            [size.width as f32, width, pan[0], scroll[0]],
            [size.height as f32, height, pan[1], scroll[1]],
        ]
        .map(|[size, length, pan, scroll]| {
            let still =
                (size / full_scale / 2.0 - length / 2.0) * full_scale * zoomed / scale + scroll;
            [(still + pan).round() as i32, still.round() as i32]
        });
        // a lateral pan leaves the rows above the center's and the box alone
        let x_above = if lateral { x_still } else { x };
        // moved panes are still this far off
        let moves = self.zoom_moves.as_ref().map(|(started, moves)| {
            let left = 1.0 - swipe::spring_progress(*started).unwrap_or(1.0);
            (moves, left)
        });
        let mut counts = HashMap::new();
        let (panes, row_focus) = rows
            .iter()
            .enumerate()
            .flat_map(|(index, (focus, panes))| {
                let y = y + ((index as f32 - center_row as f32) * step).round() as i32;
                let x = if index < center_row { x_above } else { x };
                panes.iter().map(move |pane| (*focus, pane, [x, y]))
            })
            .map(|(focus, pane, [x, y])| {
                let count = counts.entry(pane.node).or_insert(0);
                *count += 1;
                let [moved_x, moved_y] = moves
                    .and_then(|(moves, left)| {
                        let offset = moves.get(&(pane.node, *count - 1))?;
                        Some(offset.map(|value| (value * left).round() as i32))
                    })
                    .unwrap_or_default();
                let rect = [
                    pane.rect[0] + x + moved_x,
                    pane.rect[1] + y + moved_y,
                    pane.rect[2],
                    pane.rect[3],
                ];
                let pane = Pane {
                    rect,
                    clip: rect,
                    ..*pane
                };
                (pane, focus)
            })
            .unzip();
        let top = y as f32 - center_row as f32 * step;
        let column_height = (rows.len() as f32 - 1.0) * step + height;
        Some(ZoomLayout {
            panes,
            row_focus,
            center,
            scale,
            column: [x_above as f32, top, width, column_height],
        })
    }

    /// The pane under `position` of the zoom-out view drawn at `size`, and `position` before scaling
    fn zoom_pane_at(
        &self,
        size: DisplayContainerSize,
        [x, y]: [f64; 2],
    ) -> Option<(Pane, [f64; 2])> {
        let ZoomLayout { panes, scale, .. } = self.zoom_layout(size, 1.0)?;
        let scale = f64::from(scale);
        let at = [x / scale, y / scale];
        panes
            .into_iter()
            .find(|pane| pane.contains(at))
            .map(|pane| (pane, at))
    }

    /// Whether the pointer moved far enough from `from` for a drag
    fn zoom_dragged(from: [f64; 2], at: [f64; 2]) -> bool {
        const DRAG_THRESHOLD: f64 = 12.0;
        (at[0] - from[0]).hypot(at[1] - from[1]) > DRAG_THRESHOLD
    }

    /// During a drag in the zoom-out view drawn at `size`: the dragged node, the node whose pane it is over, which
    /// edge (left, right or bottom; the rest drops nothing), and that pane's rect (before scaling). Both stay
    /// below the focused node.
    fn zoom_drop(
        &self,
        size: DisplayContainerSize,
    ) -> Option<(NodeId, NodeId, ZoomSide, [i32; 4])> {
        // of the pane's width or height
        const EDGE: f64 = 0.25;
        let ZoomPress {
            node: id, from, at, ..
        } = self.zoom_press?;
        let focused = self.focused_node();
        if !self.nodes.contains_key(id)
            || !Self::zoom_dragged(from, at)
            || id == focused
            || !self.contains(focused, id)
        {
            return None;
        }
        let (pane, [x, y]) = self.zoom_pane_at(size, at)?;
        let target = pane.node;
        let [left, top, width, height] = pane.rect.map(f64::from);
        let [across, down] = [(x - left) / width, (y - top) / height];
        let (_, side) = [
            (across, ZoomSide::Before),
            (1.0 - across, ZoomSide::After),
            (1.0 - down, ZoomSide::Below),
        ]
        .into_iter()
        .filter(|(distance, _)| *distance < EDGE)
        .min_by(|a, b| a.0.total_cmp(&b.0))?;
        let parent = match side {
            ZoomSide::Below => target,
            _ => self.nodes[target].parent?,
        };
        (!self.contains(id, target)
            && self.contains(focused, parent)
            && !self.holds_children(parent))
        .then_some((id, target, side, pane.rect))
    }

    /// The pointer moved in the zoom-out view; a drag there shows where it drops
    pub fn zoom_motion(&mut self, position: [f64; 2]) {
        if let Some(press) = &mut self.zoom_press {
            press.at = position;
            self.damaged = true;
        }
    }

    /// A left press or release at `position` of the zoom-out view drawn at `size`. A click on the center focuses it
    /// (going in), on another node makes that the center. A drag to a pane's edge (`zoom_drop`) moves the node there, as
    /// the center. Anything else springs back from where dragging at the edges scrolled it (a free view stays).
    pub fn zoom_button(&mut self, size: DisplayContainerSize, position: [f64; 2], pressed: bool) {
        if pressed {
            self.zoom_press = self
                .zoom_pane_at(size, position)
                .map(|(pane, _)| ZoomPress {
                    node: pane.node,
                    from: position,
                    at: position,
                    size,
                    scroll: [0.0; 2],
                    tick: Instant::now(),
                });
            return;
        }
        self.zoom_motion(position);
        let drop = self.zoom_drop(size);
        let Some(ZoomPress {
            node: id,
            from,
            scroll,
            ..
        }) = self.zoom_press
        else {
            return;
        };
        if let Some((id, target, side, _)) = drop {
            // from where it scrolled to
            self.move_zoom_panes(|tree| {
                tree.zoom_press = None;
                // a free view keeps where it scrolled to
                if Self::zoom_free_view() {
                    for (free, scroll) in tree.zoom_free.0.iter_mut().zip(scroll) {
                        *free += scroll;
                    }
                }
                tree.move_node(id, target, side);
                tree.remember_zoom_center(id);
            });
            return;
        }
        self.zoom_press = None;
        if Self::zoom_free_view() {
            // floats where it scrolled to
            for (free, scroll) in self.zoom_free.0.iter_mut().zip(scroll) {
                *free += scroll;
            }
        } else if scroll.iter().any(|value| value.abs() > f32::EPSILON) {
            let (carried, carried_rows, _) = self.zoom_pan_offset();
            let offset = [0, 1].map(|axis| carried[axis] + scroll[axis]);
            self.zoom_pan = Some((Instant::now(), offset, carried_rows, false));
        }
        if self.nodes.contains_key(id) && !Self::zoom_dragged(from, position) {
            // a click
            if Some(id) == self.zoom_center() {
                self.focus(id, false);
            } else {
                self.set_zoom_center(id);
            }
        }
    }

    /// Like `get_window`, but with `at_root` drawn while its root is focused (eg a project's overview).
    /// During a swipe, its ends slide past each other.
    pub fn get_window_with(&self, at_root: Option<&dyn Fn() -> UITree>) -> UITree {
        let focused = self.focused_node();
        let view = |id: NodeId, panes: &[Pane]| match at_root {
            Some(at_root) if id == self.root => at_root(),
            _ => self.window(panes, Some(focused), self.active_screen),
        };
        let (Some(swipe), Some(size)) = (&self.swipe, self.latest_size) else {
            return view(focused, &self.panes);
        };
        // an end closed during the swipe
        if [Some(swipe.from), swipe.to]
            .into_iter()
            .flatten()
            .any(|id| !self.nodes.contains_key(id))
        {
            return view(focused, &self.panes);
        }
        let [from_panes, to_panes] = &swipe.panes;
        // siblings scrolled differently: the strip slides from one's scroll to the other's (#144)
        if let Some((strip, [from_scroll, to_scroll])) = swipe.strip
            && from_scroll != to_scroll
        {
            let scroll = from_scroll as f32 + (to_scroll - from_scroll) as f32 * swipe.progress;
            let panes = self.layout(
                self.view_root_for(swipe.from),
                swipe.from,
                size,
                self.active_screen,
                false,
                Some((strip, scroll.round() as i32)),
            );
            return self.window(&panes, Some(focused), self.active_screen);
        }
        // ends that show the same panes don't move, the focus ring does
        let nodes = |panes: &[Pane]| {
            let mut nodes: Vec<_> = panes.iter().map(|pane| pane.node).collect();
            nodes.sort_unstable();
            nodes
        };
        if let Some(to) = swipe.to
            && (at_root.is_none() || (swipe.from != self.root && to != self.root))
            && nodes(from_panes) == nodes(to_panes)
        {
            return self.ring_window(from_panes, swipe.from, to, swipe.progress);
        }
        swipe::slide(
            size,
            swipe.vertical,
            swipe.progress,
            view(swipe.from, from_panes),
            swipe.to.map(|to| view(to, to_panes)),
        )
    }

    /// `panes` without focus, and a focus ring partway from `from`'s pane to `to`'s
    fn ring_window(&self, panes: &[Pane], from: NodeId, to: NodeId, progress: f32) -> UITree {
        let rect = |id: NodeId| {
            let shown = self.displayed_node_toward(id, id);
            panes
                .iter()
                .find(|pane| pane.node == shown)
                .map(|pane| (pane.rect.map(|value| value as f32), pane))
        };
        let window = self.window(panes, None, self.active_screen);
        let Some((from_rect, pane)) = rect(from) else {
            return window;
        };
        let Some((to_rect, _)) = rect(to) else {
            return window;
        };
        let settings = settings::get();
        let color = if self.is_inner(self.focused_node()) {
            settings.border.focused
        } else {
            settings.palette.accent
        };
        let radius = if pane.framed { settings.radius } else { 0.0 };
        // at least the usual width, so an unframed pane's focus still shows
        let border = (pane.border as f32).max(settings.border.width).max(2.0);
        UITree::new_container(vec![
            window,
            swipe::ring(from_rect, to_rect, progress, color, border, radius),
        ])
    }

    /// Paths whose children are all shown at once (`Treeview::shown_at_once`): its applets' own
    pub fn shown_at_once(&self) -> Vec<WorldTreePath> {
        let mut paths = Vec::new();
        for (id, node) in &self.nodes {
            let Some(path) = self.path_of(id) else {
                continue;
            };
            if let Some(main) = &node.main {
                paths.extend(
                    main.get_treeview()
                        .shown_at_once
                        .iter()
                        .map(|inner| WorldTreePath::join(&path, inner)),
                );
            }
        }
        paths
    }

    /// Like `get_window`, for the screen (an inactive one shows no focus)
    pub fn get_screen_window(&self, screen: usize) -> UITree {
        if screen == self.active_screen {
            return self.get_window();
        }
        self.screens.get(screen).map_or_else(
            || UITree::leaf(UIElement::Nothing),
            |shown| self.window(&shown.panes, None, screen),
        )
    }

    /// The panes on `screen`
    fn window(&self, panes: &[Pane], focused: Option<NodeId>, screen: usize) -> UITree {
        let settings = settings::get();
        let panes = panes
            .iter()
            .filter(|pane| self.nodes.contains_key(pane.node))
            .map(|pane| {
                let node = &self.nodes[pane.node];
                let border = match node.focus {
                    _ if Some(pane.node) != focused => settings.border.unfocused,
                    FocusIndex::Here if node.main.is_some() => settings.border.focused,
                    _ => settings.palette.accent,
                };
                let surface = node.main.as_ref().map_or_else(
                    || UITree::leaf(UIElement::Nothing),
                    |main| screen_surface(main, screen),
                );
                let frame = |[x, y]: [i32; 2]| {
                    let [x, y, width, height] =
                        [x, y, pane.rect[2], pane.rect[3]].map(|value| value as f32);
                    ui::frame(
                        border,
                        if pane.framed { settings.radius } else { 0.0 },
                        pane.border as f32,
                        Style::absolute([x, y], [width, height]),
                        vec![surface],
                    )
                };
                if intersect(pane.rect, pane.clip) == pane.rect {
                    return frame([pane.rect[0], pane.rect[1]]);
                }
                // partly scrolled out of its strip: a box, so it isn't drawn outside it
                let [x, y, width, height] = pane.clip.map(|value| value as f32);
                UITree::new_container_styled(
                    Style::absolute([x, y], [width, height]),
                    vec![frame([
                        pane.rect[0] - pane.clip[0],
                        pane.rect[1] - pane.clip[1],
                    ])],
                )
            })
            .collect();
        UITree::new_container(panes)
    }

    /// The screen's view root's subtree; its root is at `view_root_path`
    pub fn get_view_treeview(&self, screen: usize) -> WorldTree<String> {
        self.get_treeview_of(self.screen_view_root(screen))
    }

    pub fn get_treeview(&self) -> WorldTree<String> {
        self.get_treeview_of(self.root)
    }

    fn get_treeview_of(&self, id: NodeId) -> WorldTree<String> {
        let node = &self.nodes[id];
        let mut raw_treeview = RecursiveTreeNode::from_value(self.applet_treeview(id));
        if node.collapsed {
            return WorldTree::World(Box::new(raw_treeview));
        }

        for &child in &node.children {
            match self.get_treeview_of(child) {
                WorldTree::World(recursive_tree_node) => {
                    raw_treeview.push_child_node(*recursive_tree_node);
                }
                child_treeview => {
                    // REVIEW: should this ever even happen?
                    raw_treeview.push_child_node(RecursiveTreeNode::from_value(child_treeview));
                }
            }
        }

        WorldTree::World(Box::new(raw_treeview))
    }

    pub fn get_focus_path(&self) -> WorldTreePath {
        self.get_focus_path_of(self.root)
    }

    fn get_focus_path_of(&self, id: NodeId) -> WorldTreePath {
        let node = &self.nodes[id];
        match node.focus {
            // this node, then the focus inside its main's inner tree
            FocusIndex::Here => {
                let mut path = WorldTreePath::root();
                if let Some(main) = &node.main {
                    path.push_inner(&main.get_treeview().focus);
                }
                path
            }
            FocusIndex::Child(child_index) => {
                let mut path = self.get_focus_path_of(node.children[child_index]);
                path.0.insert(0, Step::Child(child_index));
                path
            }
        }
    }
}

pub fn root_label_mut(treeview: &mut WorldTree<String>) -> &mut String {
    match treeview {
        WorldTree::Base(label) => label,
        WorldTree::World(node) => root_label_mut(node.get_value_mut()),
    }
}

/// Icons of wl clients whose app id contains one of the names
const WL_ICONS: [(&[&str], char); 4] = [
    (
        &[
            "foot",
            "alacritty",
            "kitty",
            "wezterm",
            "ghostty",
            "konsole",
            "term",
        ],
        '\u{f120}',
    ),
    (
        &[
            "firefox",
            "chrom",
            "qutebrowser",
            "brave",
            "librewolf",
            "zen",
        ],
        '\u{f0ac}',
    ),
    (&["dolphin", "nautilus", "thunar", "nemo"], '\u{f07b}'),
    (&["okular", "zathura", "evince"], '\u{f1c1}'),
];

/// A project's icon until one is picked
pub(crate) const PROJECT_ICON: char = '\u{f0b1}';

/// A Nerd Font icon for the applet, by how it is saved (`None`: no icon, eg the help).
/// A project's is the one it picked, if any.
fn icon(saved: Option<&SavedApplet>) -> Option<String> {
    let icon = match saved? {
        SavedApplet::Spawner {
            name,
            inner: Some(contents),
            ..
        } if name == "project" => {
            return contents.icon.clone().or(Some(PROJECT_ICON.to_string()));
        }
        SavedApplet::Spawner { name, .. } => match name.as_str() {
            "command_hub" => '\u{f054}',
            "file_manager" => '\u{f07b}',
            "code_editor" => '\u{f121}',
            "image_viewer" => '\u{f03e}',
            _ => '\u{f2d0}',
        },
        SavedApplet::Wl(wl) => {
            let app_id = wl.app_id.as_deref().unwrap_or_default().to_lowercase();
            WL_ICONS
                .iter()
                .find(|(names, _)| names.iter().any(|name| app_id.contains(name)))
                .map_or('\u{f2d0}', |&(_, icon)| icon)
        }
    };
    Some(icon.to_string())
}

fn has_inner_elements(treeview: &WorldTree<String>) -> bool {
    matches!(treeview, WorldTree::World(node) if node.safe_get(&TreeNodePath(vec![0])).is_some())
}

/*
// /// Has a list of inner applets and displays them in vertical or horizontal division.
// pub struct DividedApplet {
//     shared_resource: Arc<MultiAppletHolder>,
// }
// impl DividedApplet {
//     fn new(
//         inner_initiator: impl FnOnce(Box<dyn NodularRunnerHook>) -> Box<dyn NodularApplet>,
//         // hook: Box<dyn NodularRunnerHook>,
//         hook: Box<dyn NodularRunnerHook>,
//     ) -> Self {
//         let s = Self {
//             shared_resource: Arc::new(MultiAppletHolder::new(hook)),
//         };

//         MultiAppletHolder::add_child(Arc::downgrade(&s.shared_resource), inner_initiator);

//         s
//     }

//     /// Partial application
//     pub fn get_initializer(
//         inner_initializer: impl FnOnce(Box<dyn NodularRunnerHook>) -> Box<dyn NodularApplet>,
//     ) -> impl FnOnce(Box<dyn NodularRunnerHook>) -> Self {
//         move |hook: Box<dyn NodularRunnerHook>| Self::new(inner_initializer, hook)
//     }

//     /// Takes in a list of full-size elements and returns a combined ui element where they are equally spaced
//     /// across the horizontal axis and take full height.
//     fn combine_displays(subdisplays: Vec<UIElement>) -> UIElement {
//         // proportional units so widths out of 1
//         let widths = 1. / subdisplays.len() as f32;
//         UIElement::Container(
//             subdisplays
//                 .into_iter()
//                 .enumerate()
//                 .map(|(i, subdisplay)| {
//                     subdisplay
//                         .bordered(Color::LIGHT_GREEN)
//                         .contain(DisplayArea::new(
//                             (widths * (i as f32), 0.),
//                             (DisplayUnits::from_mixed(-1, widths * ((i + 1) as f32)), 1.),
//                         ))
//                 })
//                 .collect(),
//         )
//     }

//     fn get_display(&self) -> UIElement {
//         let mut applet_displays = Vec::new();

//         for applet in self.shared_resource.applets.read().unwrap().iter() {
//             applet_displays.push(applet.get_window());
//         }

//         Self::combine_displays(applet_displays)
//     }
// }
// impl BasicApplet for DividedApplet {
//     fn handle_ui_event(&mut self, ui_event: UIEvent) {
//         if let UIEvent::KeyPress(
//             key,
//             KeyModifiers {
//                 ctrl: true,
//                 alt: false,
//                 shift: false,
//                 caps_lock: false,
//                 logo: false,
//             },
//         ) = &ui_event
//             && key.to_char() == Some('\t')
//         {
//             self.shared_resource.applets.read().unwrap()[self
//                 .shared_resource
//                 .focus_index
//                 .load(std::sync::atomic::Ordering::Relaxed)]
//             .immut_handle_nodular_event(NodularEvent::Focused(false));

//             self.shared_resource
//                 .focus_index
//                 .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
//             self.shared_resource.focus_index.fetch_min(
//                 self.shared_resource.applets.read().unwrap().len() - 1,
//                 std::sync::atomic::Ordering::Relaxed,
//             );

//             self.shared_resource.applets.read().unwrap()[self
//                 .shared_resource
//                 .focus_index
//                 .load(std::sync::atomic::Ordering::Relaxed)]
//             .immut_handle_nodular_event(NodularEvent::Focused(true));

//             return;
//         }

//         if let UIEvent::KeyPress(
//             key,
//             KeyModifiers {
//                 ctrl: true,
//                 alt: false,
//                 shift: true,
//                 caps_lock: false,
//                 logo: false,
//             },
//         ) = &ui_event
//             && key.raw_code == 15
//         {
//             // Ctrl+Shift+Tab
//             self.shared_resource.applets.read().unwrap()[self
//                 .shared_resource
//                 .focus_index
//                 .load(std::sync::atomic::Ordering::Relaxed)]
//             .immut_handle_nodular_event(NodularEvent::Focused(false));

//             self.shared_resource
//                 .focus_index
//                 .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
//             self.shared_resource.focus_index.fetch_min(
//                 self.shared_resource.applets.read().unwrap().len() - 1,
//                 std::sync::atomic::Ordering::Relaxed,
//             );

//             self.shared_resource.applets.read().unwrap()[self
//                 .shared_resource
//                 .focus_index
//                 .load(std::sync::atomic::Ordering::Relaxed)]
//             .immut_handle_nodular_event(NodularEvent::Focused(true));
//             return;
//         }

//         let applet_holder = self.shared_resource.applets.read().unwrap()[self
//             .shared_resource
//             .focus_index
//             .load(std::sync::atomic::Ordering::Relaxed)]
//         .clone();
//         applet_holder.immut_handle_ui_event(ui_event);
//     }

//     fn get_window(&self) -> UIElement {
//         // TODO: return cached if damaged is already false?
//         self.shared_resource
//             .window_damaged
//             .store(false, std::sync::atomic::Ordering::Relaxed);

//         self.get_display()
//     }
// }
// impl NodularApplet for DividedApplet {
//     fn handle_nodular_event(&mut self, nodular_event: NodularEvent) {
//         match nodular_event {
//             NodularEvent::Highlighted(_) => todo!(),
//             NodularEvent::Focused(state) => {
//                 self.shared_resource.applets.read().unwrap()[self
//                     .shared_resource
//                     .focus_index
//                     .load(std::sync::atomic::Ordering::Relaxed)]
//                 .immut_handle_nodular_event(NodularEvent::Focused(state));
//             }
//         }
//     }

//     fn get_treeview(&self) -> RootedTree<String> {
//         self.shared_resource
//             .treeview_damaged
//             .store(false, std::sync::atomic::Ordering::Relaxed);

//         // RootedTree::
//         todo!()
//     }
// }

*/

// /// Ideally, this would've been a function,
// /// but I couldn't get the types working.
// macro_rules! get_focused_applet {
//     ($s:expr, $f:literal) => {
//         match $s.shared_resource.focus_index.get() {
//             FocusIndex::Focusing | FocusIndex::Inner => ($f)($s.main_applet),
//             FocusIndex::Child(child_index) => {
//                 ($f)($s.shared_resource.children.read().unwrap()[child_index])
//             }
//         }
//     };
// }

/// What the client shows on `screen`: its window for it, if it has one per screen (`Treeview::screen_windows`)
fn screen_surface(main: &ClientHandle, screen: usize) -> UITree {
    let surface = main.get_surface();
    if !main.get_treeview().screen_windows {
        return surface;
    }
    surface
        .into_children()
        .into_iter()
        .nth(screen)
        .unwrap_or_else(|| UITree::leaf(UIElement::Nothing))
}

/// `rect` (`[x, y, width, height]`) with `by` more on every side
const fn grow([x, y, width, height]: [i32; 4], by: i32) -> [i32; 4] {
    [x - by, y - by, width + 2 * by, height + 2 * by]
}

/// Where two rects (`[x, y, width, height]`) overlap (empty if they don't)
fn intersect(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    let [left, top] = [a[0].max(b[0]), a[1].max(b[1])];
    let [right, bottom] = [
        (a[0] + a[2]).min(b[0] + b[2]),
        (a[1] + a[3]).min(b[1] + b[3]),
    ];
    [left, top, (right - left).max(0), (bottom - top).max(0)]
}
