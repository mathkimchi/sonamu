use crate::{
    creatable_applet::CreatableNodularApplet,
    nodular_applet::{NodeRequest, NodularAppletInitializer, swipe},
    session,
    standard_keybinds::standard_keybind_request,
    ui,
    wl_surface_applet::WlSurfaceApplet,
};
use slotmap::SlotMap;
use sonamu_common::{
    sap::{
        client_handle::ClientHandle,
        detached::{Axis, DetachedClient, DetachedTree, Layout},
        packets::{AwaitedWl, NewWlSurface, SpawnPosition, StandardEvent, SwipeView, WlSurfaceId},
        session::{SavedApplet, SavedTree, SavedWl},
    },
    settings,
    utils::tree::{
        recursive_tree::RecursiveTreeNode,
        tree_node_path::TreeNodePath,
        world_tree::{Step, WorldTree, WorldTreePath},
    },
};
use sonamu_ui::{
    display_units::DisplayContainerSize,
    ui_element::{Style, UIElement, UITree},
    ui_event::{KeySymbol, MouseEvent, MouseEventKind, UIEvent},
};
use std::{cell::Cell, collections::HashMap};

#[derive(Debug, Clone, Copy)]
enum FocusIndex {
    Focusing,
    Inner,
    Child(usize),
}

slotmap::new_key_type! {
    /// Identifies a node, so requests from its applet still find it after it moves
    pub struct NodeId;
}

/// Shows its main applet or the focused child, or all its children at once (a split).
/// Starts the label of an inactive node's row in the treeview (after any `▾`/`▸`)
pub(crate) const INACTIVE_MARK: &str = "⏾ ";

struct Node {
    /// Dropping it ends the applet. `None` for splits.
    main: Option<ClientHandle>,
    layout: Layout,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    focus: FocusIndex,
    /// Last size sent to `main`
    size: Option<DisplayContainerSize>,
    /// The wl surface focus is on inside its main's inner tree, as last reported by the main
    inner_wl_focus: Option<WlSurfaceId>,
    /// Its main's inner tree is hidden in the treeview
    collapsed: bool,
    /// The child focus was last on; shown by a container (no main, not a split) while it is focused itself
    last_child: usize,
    /// A restored wl client's tile (its main stands in) waiting for the client's toplevel
    awaiting: Option<AwaitedWl>,
    /// Hibernating: shown grey in the treeview
    inactive: bool,
    /// A restored wl client not started yet, or a deactivated applet (its main stands in); started when focused
    hibernated: Option<SavedApplet>,
    /// The last `StandardEvent::ScreenViews` sent to its main: its size on each screen, and the active one
    screen_views: Option<(Vec<Option<DisplayContainerSize>>, usize)>,
    /// Half width in its parent's strip (#144)
    half: bool,
    /// Of its children shown as a pair of half width ones, the first (#144)
    pair: Cell<usize>,
}
impl Node {
    const fn new(main: Option<ClientHandle>, layout: Layout, parent: Option<NodeId>) -> Self {
        Self {
            main,
            layout,
            parent,
            children: Vec::new(),
            focus: FocusIndex::Focusing,
            size: None,
            inner_wl_focus: None,
            collapsed: false,
            last_child: 0,
            awaiting: None,
            inactive: false,
            hibernated: None,
            screen_views: None,
            half: false,
            pair: Cell::new(0),
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
}

/// A swipe between two nodes of the tree (`StandardEvent::Swipe`), drawn on the active screen
struct TreeSwipe {
    vertical: bool,
    from: NodeId,
    to: Option<NodeId>,
    progress: f32,
    /// What each end shows, from the last `update_layout`
    panes: [Vec<Pane>; 2],
}

/// A screen showing the tree, as it was left when another became active
struct Screen {
    size: Option<DisplayContainerSize>,
    panes: Vec<Pane>,
    /// The focused node, and whether focus is inside its main
    focus: (NodeId, bool),
    view_root: NodeId,
}

/// A node shown on screen, at `rect` (`[x, y, width, height]`, border included) of the tree's area
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pane {
    node: NodeId,
    rect: [u32; 4],
    /// From the settings; 0 when unframed
    border: u32,
    /// Has a border and rounded corners: not the only pane (unless `always_frame`), and not a project
    framed: bool,
}
impl Pane {
    /// Inside the border
    const fn inner_size(&self) -> DisplayContainerSize {
        DisplayContainerSize::new(
            self.rect[2].saturating_sub(2 * self.border),
            self.rect[3].saturating_sub(2 * self.border),
        )
    }

    fn contains(&self, [x, y]: [f64; 2]) -> bool {
        let [left, top, width, height] = self.rect.map(f64::from);
        (left..left + width).contains(&x) && (top..top + height).contains(&y)
    }
}

impl NodeTree {
    pub fn new(
        root: NodularAppletInitializer,
        spawn_client: SpawnClient,
        attach_client: AttachClient,
    ) -> Self {
        Self::with_root(spawn_client, attach_client, |tree| {
            tree.insert_node(None, root)
        })
    }

    /// A tree of placed clients (eg a project's contents)
    pub fn from_detached(
        root: DetachedTree,
        spawn_client: SpawnClient,
        attach_client: AttachClient,
    ) -> Self {
        Self::with_root(spawn_client, attach_client, |tree| {
            tree.attach_subtree(None, root)
        })
    }

    /// A saved tree, made again (`session::restore`)
    pub fn restore(
        root: SavedTree,
        spawn_client: SpawnClient,
        attach_client: AttachClient,
    ) -> Self {
        Self::with_root(spawn_client, attach_client, |tree| {
            tree.restore_subtree(None, root)
        })
    }

    fn with_root(
        spawn_client: SpawnClient,
        attach_client: AttachClient,
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
            damaged: true,
            keybind_request: None,
            view_root: NodeId::default(),
            screens: Vec::new(),
            active_screen: 0,
            swipe: None,
            swipe_sent_to: None,
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
        let id = self.nodes.insert_with_key(|id| {
            Node::new(Some(spawn_client(id, initializer)), Layout::Scroll, parent)
        });
        self.send_initial_size(id);
        id
    }

    /// Attaches the tree's clients under `parent` (not added to its children), with new ids
    fn attach_subtree(&mut self, parent: Option<NodeId>, tree: DetachedTree) -> NodeId {
        let attach_client = &self.attach_client;
        let id = self.nodes.insert_with_key(|id| Node {
            inactive: tree.hibernated.is_some(),
            hibernated: tree.hibernated,
            awaiting: tree.awaiting,
            ..Node::new(
                tree.main.map(|main| attach_client(id, main)),
                tree.layout,
                parent,
            )
        });
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
    fn restore_subtree(&mut self, parent: Option<NodeId>, saved: SavedTree) -> NodeId {
        let hibernated = saved.main.clone();
        let main = saved.main.map(session::hibernate);
        let spawn_client = &self.spawn_client;
        let id = self.nodes.insert_with_key(|id| Node {
            inactive: hibernated.is_some(),
            hibernated,
            half: saved.half,
            ..Node::new(
                main.map(|initializer| spawn_client(id, initializer)),
                saved.layout,
                parent,
            )
        });
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
                layout: Layout::Scroll,
                children: saved,
                ..SavedTree::default()
            },
        }
    }

    /// The node, or its children if it is left out (a split left with one pane is too)
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
            None if matches!(node.layout, Layout::Split(_)) && children.len() <= 1 => {
                return children;
            }
            None => None,
        };
        vec![SavedTree {
            main,
            layout: node.layout,
            children,
            half: node.half,
            ..SavedTree::default()
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
            layout: node.layout,
            wl_surface,
            hibernated: node.hibernated,
            awaiting: node.awaiting,
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
    /// (splits have none, and an applet may have ended, eg by panicking)
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
            FocusIndex::Child(focused) if focused == index => FocusIndex::Focusing,
            FocusIndex::Child(focused) if focused > index => FocusIndex::Child(focused - 1),
            focus => focus,
        };
        if matches!(parent.focus, FocusIndex::Focusing) {
            self.selected(parent_id);
        }
        self.damaged = true;
        if self.contains(id, self.view_root) {
            self.view_root = parent_id;
        }
        let plucked = self.detach_subtree(id);
        self.collapse_split(parent_id);
        Some(plucked)
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

    /// Toggles the node between half and full width in its parent's strip (`Super+F`, #144)
    pub fn toggle_half(&mut self, id: NodeId) {
        let node = &mut self.nodes[id];
        node.half = !node.half;
        self.damaged = true;
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

    /// A split left with one pane is replaced by that pane
    fn collapse_split(&mut self, id: NodeId) {
        let node = &self.nodes[id];
        if !matches!(node.layout, Layout::Split(_)) || node.children.len() != 1 {
            return;
        }
        let child = node.children[0];
        let parent = node.parent;
        self.replace_child(parent, id, child);
        if self.view_root == id {
            self.view_root = child;
        }
        self.nodes.remove(id);
        self.nodes[child].parent = parent;
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
    /// A container root (no main, not a split, like a project's) is dropped, its children taking its place,
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
        for &child in &old.children {
            self.nodes[child].parent = Some(new);
        }
        self.nodes[new].children.extend(old.children);
        self.wl_surfaces.retain(|_, node_id| *node_id != id);
        self.damaged = true;

        let node = &self.nodes[new];
        if node.main.is_some() || node.layout != Layout::Scroll {
            return;
        }
        let children = node.children.clone();
        match (node.parent, children.as_slice()) {
            (Some(_), _) => {
                self.close(new);
            }
            (None, &[only]) => {
                self.nodes.remove(new);
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
        let id = self.attach_subtree(Some(parent), tree);
        let children = &mut self.nodes[parent].children;
        match position {
            SpawnPosition::InsertChild { child_index } if child_index < children.len() => {
                children.insert(child_index, id);
            }
            _ => children.push(id),
        }
        self.focus_node(id);
        self.nodes[id].focus = FocusIndex::Focusing;
        self.damaged = true;
        id
    }

    /// Points every ancestor's focus down to the node, without changing the node's own focus
    fn focus_node(&mut self, id: NodeId) {
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
        self.nodes[id].focus = if inner.is_none() {
            FocusIndex::Focusing
        } else {
            FocusIndex::Inner
        };
        // so the path inside goes to the woken applet
        self.wake_focused();
        match inner {
            Some(inner) => self.send_event(id, StandardEvent::Focus(inner)),
            None => self.selected(id),
        }
    }

    /// The node itself was selected: its main goes to its root if that's what its row is
    /// (`Treeview::selected_at_root`, eg a project shows its overview)
    fn selected(&self, id: NodeId) {
        if let Some(main) = &self.nodes[id].main
            && main.get_treeview().selected_at_root
        {
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

    /// Focuses the node itself, or inside its applet if `inner`
    pub fn focus(&mut self, id: NodeId, inner: bool) {
        self.focus_node(id);
        self.nodes[id].focus = if inner {
            FocusIndex::Inner
        } else {
            self.selected(id);
            FocusIndex::Focusing
        };
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
        // pluck, close and split move it themselves; just in case
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

    /// New children of the node go inside its main (see `StandardApplet::holds_children`)
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

    /// Whether the node is a split (eg a `SplitApplet`'s inner root)
    pub fn is_split(&self, id: NodeId) -> bool {
        self.nodes
            .get(id)
            .is_some_and(|node| matches!(node.layout, Layout::Split(_)))
    }

    /// Its applet's treeview label (or its layout's)
    pub fn label(&self, id: NodeId) -> String {
        self.applet_treeview(id).get_root_value().clone()
    }

    /// Its main's inner tree (or its layout's label), marked `▾` if it has inner elements or children,
    /// or `▸` and hidden if collapsed (its children too, see `get_treeview_of`)
    fn applet_treeview(&self, id: NodeId) -> WorldTree<String> {
        let node = &self.nodes[id];
        let mut treeview = match &node.main {
            None => WorldTree::Base(node.layout.label().to_string()),
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
            .is_some_and(|node| matches!(node.focus, FocusIndex::Inner))
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
            FocusIndex::Child(focused) if focused == index => FocusIndex::Focusing,
            FocusIndex::Child(focused) if focused > index => {
                FocusIndex::Child(focused + closed.children.len() - 1)
            }
            focus => focus,
        };
        if matches!(parent.focus, FocusIndex::Focusing) {
            self.selected(parent_id);
        }

        self.wl_surfaces.retain(|_, node_id| *node_id != id);
        self.damaged = true;
        self.collapse_split(parent_id);
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

    /// `None` while a node itself is focused (`Focusing`), since it takes the keys then
    pub fn focused_wl_surface(&self) -> Option<WlSurfaceId> {
        let id = self.focused_node();
        if matches!(self.nodes[id].focus, FocusIndex::Focusing) {
            return None;
        }
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
            _ => {}
        }

        let id = self.focused_node();
        let node = &mut self.nodes[id];
        if let Some(main) = &node.main
            && matches!(node.focus, FocusIndex::Inner)
        {
            main.send_event(event);
            return;
        }

        // the node itself is focused (or it has no main), so this is the treeview selector
        let StandardEvent::UIEvent(ui_event) = event else {
            if let Some(main) = &node.main {
                main.send_event(event);
            }
            return;
        };
        if let Some(request) = standard_keybind_request(&ui_event) {
            // on the node itself, eg Ctrl+Shift++ adds a child to it
            self.keybind_request = Some((id, request));
            return;
        }
        // releases, and modifiers on their own, stay in the selector
        let UIEvent::Key {
            symbol: Some(key),
            pressed: true,
            ..
        } = ui_event
        else {
            return;
        };
        // traversal keys are the root's (`traversal`)
        if matches!(key, KeySymbol::Char(' ')) {
            self.toggle_collapsed(id);
        } else if matches!(key, KeySymbol::Enter) {
            // confirms the selection: into its main, or a split's first pane
            if node.main.is_some() {
                node.focus = FocusIndex::Inner;
                self.damaged = true;
            } else if let Some(&first) = node.children.first() {
                self.focus_node(first);
                self.nodes[first].focus = FocusIndex::Focusing;
            }
        } else if let Some(main) = &node.main {
            // any other key goes into the main
            node.focus = FocusIndex::Inner;
            main.send_event(StandardEvent::UIEvent(ui_event));
            self.damaged = true;
        }
    }

    /// Whether the node itself is focused at the end of the focus path (the treeview selector),
    /// rather than inside an applet. Asks the main if focus is inside it (eg a project's own selector).
    pub fn is_traversing(&self) -> bool {
        let node = &self.nodes[self.focused_node()];
        match (&node.main, node.focus) {
            (Some(main), FocusIndex::Inner) => main.get_treeview().traversing,
            _ => true,
        }
    }

    /// Sends the pane under the pointer (or the one holding the pointer grab) the event, relative to it.
    /// A press focuses the pane's applet.
    fn handle_mouse(&mut self, mut mouse_event: MouseEvent) {
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
                self.nodes[pane.node].focus = FocusIndex::Inner;
            }
        }
        mouse_event.position[0] -= f64::from(pane.rect[0] + pane.border);
        mouse_event.position[1] -= f64::from(pane.rect[1] + pane.border);
        self.send_event(
            pane.node,
            StandardEvent::UIEvent(UIEvent::Mouse(mouse_event)),
        );
    }

    /// The node `id` shows: following focused children, but stopping at splits, which show every pane
    fn displayed_node(&self, id: NodeId) -> NodeId {
        self.displayed_node_toward(id, self.focused_node())
    }

    /// Like `displayed_node`, but following the children `focus` is under
    /// (another screen's focus, which the nodes' own focus doesn't point to)
    fn displayed_node_toward(&self, mut id: NodeId, focus: NodeId) -> NodeId {
        while let Some(child) = self.shown_child(id, focus) {
            id = child;
        }
        id
    }

    /// The child a scrolling node shows instead of itself, following the children `focus` is under
    fn shown_child(&self, id: NodeId, focus: NodeId) -> Option<NodeId> {
        let node = &self.nodes[id];
        if !matches!(node.layout, Layout::Scroll) {
            return None;
        }
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

    /// The half width children shown side by side with `child` (one of `id`'s, half width itself), as `[left, right]`
    /// (`None`: empty), scrolled as little as can be from the last pair shown (#144). `remember` keeps the pair.
    fn half_pair(&self, id: NodeId, child: NodeId, remember: bool) -> Option<[Option<NodeId>; 2]> {
        let node = &self.nodes[id];
        if !self.nodes[child].half {
            return None;
        }
        let index = self.child_index(id, child);
        let half = |index: usize| {
            node.children
                .get(index)
                .is_some_and(|&child| self.nodes[child].half)
        };
        let last = node.pair.get();
        let first = if (index == last || index == last + 1) && half(last) && half(last + 1) {
            last
        } else if index > last && index > 0 && half(index - 1) {
            // moved right: it comes in on the right
            index - 1
        } else if half(index + 1) || index == 0 || !half(index - 1) {
            index
        } else {
            index - 1
        };
        if remember {
            node.pair.set(first);
        }
        let at = |index: usize| node.children.get(index).copied().filter(|_| half(index));
        Some([at(first), at(first + 1)])
    }

    /// What the node shows on its own (for a split, its first pane) and the size it is drawn at, eg for a preview.
    /// `None` if that has no running applet or no size yet.
    pub fn preview(&self, id: NodeId) -> Option<(UITree, DisplayContainerSize)> {
        let mut id = self.displayed_node(id);
        while matches!(self.nodes[id].layout, Layout::Split(_)) {
            id = self.displayed_node(*self.nodes[id].children.first()?);
        }
        let node = &self.nodes[id];
        Some((
            screen_surface(node.main.as_ref()?, self.active_screen),
            node.size?,
        ))
    }

    /// Works out what is on screen, and resizes the shown applets to their panes.
    /// An applet on several screens gets the active one's pane, else the first one's.
    pub fn update_layout(&mut self) {
        let mut shown = Vec::new();
        if let Some(size) = self.latest_size {
            let panes = self.layout(
                self.view_root,
                self.focused_node(),
                size,
                self.active_screen,
                true,
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
            let panes = self.layout(view_root, focus, size, index, false);
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
    /// (on `screen`); `remember` keeps which half width children were shown (`half_pair`)
    fn layout(
        &self,
        view_root: NodeId,
        focus: NodeId,
        size: DisplayContainerSize,
        screen: usize,
        remember: bool,
    ) -> Vec<Pane> {
        let settings = settings::get();
        let (gap, border) = settings.gap_and_border_px();
        let mut panes = Vec::new();
        let area = [
            gap,
            gap,
            size.width.saturating_sub(2 * gap),
            size.height.saturating_sub(2 * gap),
        ];
        self.push_panes(view_root, focus, area, [gap, border], remember, &mut panes);
        // a lone half width pane keeps its frame
        if let (false, [pane]) = (settings.always_frame, panes.as_mut_slice())
            && pane.rect == area
        {
            pane.rect = [0, 0, size.width, size.height];
            pane.border = 0;
            pane.framed = false;
        }
        for pane in panes.iter_mut().filter(|pane| pane.framed) {
            if self.frames_itself(pane.node, screen) {
                // it frames its own content, with its own gap around it, so it takes this gap back
                let [x, y, width, height] = pane.rect;
                pane.rect = [
                    x.saturating_sub(gap),
                    y.saturating_sub(gap),
                    width + 2 * gap,
                    height + 2 * gap,
                ];
                pane.border = 0;
                pane.framed = false;
            }
        }
        panes
    }

    /// Splits and half width children side by side leave `gap` between them
    fn push_panes(
        &self,
        mut id: NodeId,
        focus: NodeId,
        mut rect: [u32; 4],
        [gap, border]: [u32; 2],
        remember: bool,
        panes: &mut Vec<Pane>,
    ) {
        while let Some(child) = self.shown_child(id, focus) {
            if let Some(pair) = self.half_pair(id, child, remember) {
                let [x, y, width, height] = rect;
                let left = width.saturating_sub(gap) / 2;
                let halves = [
                    [x, y, left, height],
                    [x + left + gap, y, width.saturating_sub(left + gap), height],
                ];
                for (side, half) in pair.into_iter().zip(halves) {
                    match side {
                        Some(side) if side == child => rect = half,
                        Some(side) => {
                            self.push_panes(side, focus, half, [gap, border], remember, panes);
                        }
                        None => {}
                    }
                }
            }
            id = child;
        }
        let node = &self.nodes[id];
        let Layout::Split(axis) = node.layout else {
            panes.push(Pane {
                node: id,
                rect,
                border,
                framed: true,
            });
            return;
        };

        let [x, y, width, height] = rect;
        let count = node.children.len() as u32;
        let length = match axis {
            Axis::Horizontal => width,
            Axis::Vertical => height,
        };
        let length = length.saturating_sub(gap * count.saturating_sub(1));
        for (index, &child) in (0..).zip(&node.children) {
            // the last pane takes the rounding leftovers
            let start = length * index / count + gap * index;
            let end = length * (index + 1) / count + gap * index;
            let child_rect = match axis {
                Axis::Horizontal => [x + start, y, end - start, height],
                Axis::Vertical => [x, y + start, width, end - start],
            };
            self.push_panes(child, focus, child_rect, [gap, border], remember, panes);
        }
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
        let panes = ends.map(|end| {
            end.filter(|&id| self.nodes.contains_key(id))
                .map(|id| {
                    let view_root = if self.contains(self.view_root, id) {
                        self.view_root
                    } else {
                        id
                    };
                    self.layout(view_root, id, size, self.active_screen, false)
                })
                .unwrap_or_default()
        });
        for pane in panes.iter().flatten() {
            self.resize(pane.node, pane.inner_size());
        }
        if let Some(swipe) = &mut self.swipe {
            swipe.panes = panes;
        }
    }

    /// Every pane, bordered unless it is the only one; the focused one's border shows whether focus is inside it
    pub fn get_window(&self) -> UITree {
        self.get_window_with(None)
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
        // ends that show the same panes (eg both in a split) don't move, the focus ring does
        let nodes = |panes: &[Pane]| panes.iter().map(|pane| pane.node).collect::<Vec<_>>();
        if let Some(to) = swipe.to
            && (at_root.is_none() || (swipe.from != self.root && to != self.root))
            && nodes(from_panes) == nodes(to_panes)
        {
            return self.ring_window(from_panes, swipe.from, Some(to), swipe.progress);
        }
        // past a split's first / last pane: the ring pushes against its edge rather than the panes sliding
        if swipe.to.is_none()
            && let Some(parent) = self.nodes[swipe.from].parent
            && matches!(self.nodes[parent].layout, Layout::Split(_))
        {
            return self.ring_window(from_panes, swipe.from, None, swipe.progress);
        }
        swipe::slide(
            size,
            swipe.vertical,
            swipe.progress,
            view(swipe.from, from_panes),
            swipe.to.map(|to| view(to, to_panes)),
        )
    }

    /// `panes` without focus, and a focus ring partway from `from`'s pane to `to`'s.
    /// With no `to` (past a split's first / last pane, `progress` rubber-banded, negative before the first), the ring
    /// squashes against the split's edge: its leading edge stays and its trailing edge follows.
    fn ring_window(
        &self,
        panes: &[Pane],
        from: NodeId,
        to: Option<NodeId>,
        progress: f32,
    ) -> UITree {
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
        let (to_rect, progress) = if let Some(to) = to {
            let Some((to_rect, _)) = rect(to) else {
                return window;
            };
            (to_rect, progress)
        } else {
            let axis = match self.nodes[from]
                .parent
                .map(|parent| self.nodes[parent].layout)
            {
                Some(Layout::Split(Axis::Vertical)) => 1,
                _ => 0,
            };
            let mut squashed = from_rect;
            let pushed = progress.abs() * from_rect[axis + 2];
            squashed[axis + 2] -= pushed;
            if progress > 0.0 {
                // toward the end: the start edge follows
                squashed[axis] += pushed;
            }
            (squashed, 1.0)
        };
        let settings = settings::get();
        let color = match self.nodes[self.focused_node()].focus {
            FocusIndex::Inner => settings.border.focused,
            _ => settings.palette.accent,
        };
        let radius = if pane.framed { settings.radius } else { 0.0 };
        // at least the usual width, so an unframed pane's focus still shows
        let border = (pane.border as f32).max(settings.border.width).max(2.0);
        UITree::new_container(vec![
            window,
            swipe::ring(from_rect, to_rect, progress, color, border, radius),
        ])
    }

    /// Paths whose children are all shown at once (`Treeview::shown_at_once`): its splits, and its applets' own
    pub fn shown_at_once(&self) -> Vec<WorldTreePath> {
        let mut paths = Vec::new();
        for (id, node) in &self.nodes {
            let Some(path) = self.path_of(id) else {
                continue;
            };
            if matches!(node.layout, Layout::Split(_)) {
                paths.push(WorldTreePath::from_tree_path(&path));
            }
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
                    FocusIndex::Inner => settings.border.focused,
                    _ => settings.palette.accent,
                };
                let surface = node.main.as_ref().map_or_else(
                    || UITree::leaf(UIElement::Nothing),
                    |main| screen_surface(main, screen),
                );
                let [x, y, width, height] = pane.rect.map(|value| value as f32);
                ui::frame(
                    border,
                    if pane.framed { settings.radius } else { 0.0 },
                    pane.border as f32,
                    Style::absolute([x, y], [width, height]),
                    vec![surface],
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
            FocusIndex::Focusing => WorldTreePath::root(),
            // this node, then the focus inside its main's inner tree
            FocusIndex::Inner => {
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
        SavedApplet::Spawner { name, .. } if name == "split" => return None,
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
