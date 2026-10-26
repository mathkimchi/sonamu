//! A split: an applet holding its panes as its inner tree, like a project holds its contents.
//! Its node doesn't know it is a split; it only sees an applet with an inner tree.

use crate::{
    nodular_applet::{
        Answer, AppletContext, AppletSpawner, AppletSpawnerTrait, EmbeddedId, NodeRequest,
        NodularAppletInitializer, StandardApplet, WrapInitializer,
        node_holder::NodeHolder,
        node_tree::{AttachClient, NodeId, NodeTree, SpawnClient},
    },
    session,
};
use singularity_common::sap::{
    detached::{Axis, DetachedClient, DetachedTree, Layout},
    packets::{
        AwaitedWl, NewWlSurface, NodeAction, OpenApplet, SpawnPosition, StandardEvent,
        StandardRequest,
    },
    session::{SavedApplet, SavedTree},
};
use singularity_common::utils::tree::world_tree::{WorldTree, WorldTreePath};
use sonamu_ui::{
    display_units::DisplayContainerSize,
    ui_element::{UIElement, UITree},
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// Shows its panes side by side (or stacked); the root's treeview shows them as its node's inner tree.
/// Its inner tree's root is the split (no main), its children the panes.
///
/// Closing it (`CloseRequest`, eg Ctrl+W on its node) gives its panes back to its node, which they replace.
/// Left with one pane, it is replaced by that pane.
pub struct SplitApplet {
    holder: NodeHolder,
    hook: AppletContext,
    /// The node each embedded applet (its panes' mains) is the main of
    nodes: Rc<RefCell<HashMap<EmbeddedId, NodeId>>>,
    /// Its panes were given back, so its tree is empty
    dissolved: bool,
}
impl SplitApplet {
    /// Makes a split of a subtree (`NodeRequest::Split`) and a new pane after it, which gets focus
    pub fn wrapper(axis: Axis, new_pane: NodularAppletInitializer) -> WrapInitializer {
        Box::new(move |subtree, directory| {
            Box::new(move |hook| {
                let contents = DetachedTree {
                    main: None,
                    layout: Layout::Split(axis),
                    wl_surface: None,
                    children: vec![subtree],
                };
                let mut split = Self::with_tree(hook, |spawn_client, attach_client| {
                    NodeTree::from_detached(contents, spawn_client, attach_client)
                });
                split.holder.directory = directory;
                let tree = &mut split.holder.tree;
                if let Some(new) = tree.add_child(tree.root(), new_pane) {
                    tree.focus(new, true);
                }
                Box::new(split) as Box<dyn StandardApplet>
            })
        })
    }

    /// Makes a saved split again, holding its saved panes, in the directory of the tree it is in
    pub fn restorer(contents: SavedTree) -> NodularAppletInitializer {
        Box::new(move |hook| {
            hook.get_directory();
            Box::new(Self::with_tree(hook, |spawn_client, attach_client| {
                NodeTree::restore(contents, spawn_client, attach_client)
            })) as Box<dyn StandardApplet>
        })
    }

    /// Registered as `split` by the root, to restore saved splits (`save`); it can't be made new from args
    pub fn get_applet_spawner() -> AppletSpawner {
        struct Spawner;
        impl AppletSpawnerTrait for Spawner {
            fn create_initializer(&self, _args: &[&str]) -> Option<NodularAppletInitializer> {
                None
            }

            fn restore_initializer(
                &self,
                _args: &[&str],
                inner: SavedTree,
            ) -> Option<NodularAppletInitializer> {
                Some(SplitApplet::restorer(inner))
            }

            fn duplicate(&self) -> AppletSpawner {
                Box::new(Self)
            }
        }
        Box::new(Spawner)
    }

    /// Holds the tree made with clients embedded in it
    fn with_tree(
        hook: AppletContext,
        make_tree: impl FnOnce(SpawnClient, AttachClient) -> NodeTree,
    ) -> Self {
        let nodes: Rc<RefCell<HashMap<EmbeddedId, NodeId>>> = Rc::default();
        let spawn_client: SpawnClient = {
            let hook = hook.clone();
            let nodes = Rc::clone(&nodes);
            Box::new(move |id, initializer| {
                let (embedded, handle) = hook.embed(initializer);
                nodes.borrow_mut().insert(embedded, id);
                handle
            })
        };
        let attach_client: AttachClient = {
            let hook = hook.clone();
            let nodes = Rc::clone(&nodes);
            Box::new(move |id, client: DetachedClient| {
                let (embedded, handle) = hook.embed_detached(client);
                nodes.borrow_mut().insert(embedded, id);
                handle
            })
        };
        let tree = make_tree(spawn_client, attach_client);
        let send = {
            let hook = hook.clone();
            Box::new(move |request: StandardRequest| hook.request(request.into()))
        };
        Self {
            holder: NodeHolder::new(tree, send, true, hook.applet_spawners.clone()),
            hook,
            nodes,
            dissolved: false,
        }
    }

    /// What replaces it: its only pane, else its panes (a container, whose children take its place)
    fn contents(&mut self) -> DetachedTree {
        let tree = self.holder.tree.detach_all();
        match (tree.layout, tree.children.len()) {
            (Layout::Split(_), 1) => tree.children.into_iter().next().unwrap(),
            (Layout::Split(_), _) => DetachedTree {
                main: None,
                layout: Layout::Single,
                wl_surface: None,
                children: tree.children,
            },
            // already collapsed into its pane
            (Layout::Single, _) => tree,
        }
    }
}
impl StandardApplet for SplitApplet {
    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        if self.dissolved {
            return;
        }
        match standard_event {
            StandardEvent::UIEvent(_)
            | StandardEvent::FocusChanged(..)
            | StandardEvent::Highlighted(..) => {
                if let Some(request) = self.holder.handle_ui_event(standard_event) {
                    self.hook.request(request);
                }
            }
            // its panes are in its node's tree's directory
            StandardEvent::Directory(directory) => self.holder.set_directory(directory),
            // handled by the applet's client
            StandardEvent::CloseRequest
            | StandardEvent::SettingsChanged(_)
            | StandardEvent::DamageAck(_)
            | StandardEvent::WlSurfaceRegistered(_)
            | StandardEvent::Open(_)
            | StandardEvent::Pluck(_)
            | StandardEvent::Place(..)
            | StandardEvent::Focus(_)
            | StandardEvent::ToggleCollapsed(_)
            | StandardEvent::NodeAction(..) => {}
            // only the root gets these
            StandardEvent::Capturing(_) => {}
        }
    }

    fn get_window_standard_applet(&self, _container_size: DisplayContainerSize) -> UITree {
        if self.dissolved {
            return UITree::leaf(UIElement::Nothing);
        }
        self.holder.tree.get_window()
    }

    /// Its tree, whose root is labeled like a split (`Split ⇆`)
    fn get_treeview(&self) -> WorldTree<String> {
        if self.dissolved {
            return WorldTree::Base(String::new());
        }
        self.holder.tree.get_treeview()
    }

    fn get_focus_path(&self) -> WorldTreePath {
        if self.dissolved {
            return WorldTreePath::new_into();
        }
        self.holder.tree.get_focus_path()
    }

    fn is_traversing(&self) -> bool {
        !self.dissolved && self.holder.tree.is_traversing()
    }

    fn frames_itself(&self) -> bool {
        !self.dissolved
    }

    /// Through its own tree, while focus is in it
    fn handles_dfs_swipes(&self) -> bool {
        !self.dissolved
    }

    fn focus(&mut self, path: &WorldTreePath) {
        if !self.dissolved {
            self.holder.focus(path);
        }
    }

    fn toggle_collapsed(&mut self, path: &WorldTreePath) {
        if !self.dissolved {
            self.holder.toggle_collapsed(path);
        }
    }

    fn node_action(&mut self, path: &WorldTreePath, action: NodeAction) {
        if !self.dissolved {
            self.holder.node_action(path, action);
        }
    }

    /// Answered by its holder, maybe after asking an applet inside
    fn pluck(&mut self, path: &WorldTreePath) -> Answer<Option<DetachedTree>> {
        if self.dissolved {
            return Answer::Now(None);
        }
        self.holder.start_pluck(Some(path));
        Answer::Later
    }

    fn place(
        &mut self,
        path: &WorldTreePath,
        position: SpawnPosition,
        tree: DetachedTree,
    ) -> Answer<Result<(), DetachedTree>> {
        if self.dissolved {
            return Answer::Now(Err(tree));
        }
        self.holder.start_place(Some(path), position, tree);
        Answer::Later
    }

    /// It closes into its panes
    fn close_requested(&mut self) -> bool {
        self.holder.dissolve = true;
        false
    }

    fn take_wl_surface(&mut self, surface: NewWlSurface) -> Result<(), NewWlSurface> {
        if self.dissolved {
            return Err(surface);
        }
        self.holder.add_wl_surface(surface);
        Ok(())
    }

    fn take_open(&mut self, open: OpenApplet) -> Result<(), OpenApplet> {
        if self.dissolved {
            return Err(open);
        }
        self.holder.open(open);
        Ok(())
    }

    fn save(&self) -> Option<SavedApplet> {
        if self.dissolved {
            return None;
        }
        Some(session::saved_with_inner(
            "split",
            Vec::new(),
            self.holder.tree.save(),
        ))
    }

    fn awaiting_wl(&self) -> Vec<AwaitedWl> {
        if self.dissolved {
            return Vec::new();
        }
        self.holder.tree.awaiting_wl()
    }

    /// From a main of its panes
    fn handle_embedded_request(&mut self, id: EmbeddedId, request: NodeRequest) {
        let Some(&id) = self.nodes.borrow().get(&id) else {
            return;
        };
        if let Some(request) = self.holder.handle_node_request(id, request) {
            self.hook.request(request);
        }
    }

    fn update(&mut self) {
        if self.dissolved {
            return;
        }
        self.holder.finish();
        let root = self.holder.tree.root();
        // closed, or left with one pane (its tree replaced the split with it)
        if self.holder.dissolve
            || !self.holder.tree.is_split(root)
            || self.holder.tree.children(root).len() <= 1
        {
            let contents = self.contents();
            self.hook.request(NodeRequest::Unwrap(contents));
            self.dissolved = true;
            return;
        }
        if std::mem::take(&mut self.holder.quit) {
            self.hook.quit();
        }
        // eg `q` inside it: out to its node
        if let Some(operation) = self.holder.tree.take_escaped() {
            self.hook.change_focus(operation);
        }
        self.holder.tree.update_view_root();
        if self.holder.tree.take_damage() {
            self.hook.damage_window();
            self.hook.damage_treeview();
        }
        self.holder.tree.update_layout();
        self.holder.update_wl_focus();
    }
}
