//! Clients with no holder, for moving them between holders (pluck and place)

use crate::{
    sap::{
        client_handle::ClientHandle,
        packets::{StandardEvent, Treeview, WlSurfaceId},
        shared_state::SharedState,
    },
    utils::tree::{recursive_tree::RecursiveTreeNode, world_tree::WorldTree},
};
use calloop::{LoopHandle, channel::Channel};
use sonamu_ui::ui_element::UITree;
use std::any::Any;

/// A client taken out of its holder's event loop (`ClientHandle::detach`).
///
/// Its thread keeps running; its requests queue up until it is attached again.
pub struct DetachedClient {
    pub(crate) event_queue: calloop::channel::Sender<StandardEvent>,
    pub(crate) surface: SharedState<UITree>,
    pub(crate) treeview: SharedState<Treeview>,
    /// The client's `Channel<Request>`; type-erased so holders that can't name `Request` can still hold it
    pub(crate) requests: Box<dyn Any + Send>,
}
impl DetachedClient {
    /// Queued requests are handled once the event loop runs.
    ///
    /// # Panics
    ///
    /// If `Request` isn't the type the client was spawned with.
    pub fn attach<Request: Send + 'static, Data: 'static>(
        self,
        event_loop: &LoopHandle<'static, Data>,
        on_request: impl FnMut(Request, &mut Data) + 'static,
    ) -> ClientHandle {
        let requests = *self
            .requests
            .downcast::<Channel<Request>>()
            .expect("attached with a different request type");
        ClientHandle::attach(
            self.event_queue,
            self.surface,
            self.treeview,
            requests,
            event_loop,
            on_request,
        )
    }

    /// The last published treeview, without acking it
    pub fn peek_treeview(&self) -> WorldTree<String> {
        self.treeview.get().tree
    }
}
impl std::fmt::Debug for DetachedClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DetachedClient").finish_non_exhaustive()
    }
}

/// How a node shows itself and its children
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Layout {
    /// Its main, or the focused child
    #[default]
    Single,
    /// All children at once, side by side along the axis (a split has no main)
    Split(Axis),
}
impl Layout {
    /// The treeview label of a node with no main
    pub const fn label(self) -> &'static str {
        match self {
            Self::Single => "(empty)",
            Self::Split(Axis::Horizontal) => "Split ⇆",
            Self::Split(Axis::Vertical) => "Split ⇅",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Axis {
    /// Left to right
    Horizontal,
    /// Top to bottom
    Vertical,
}

/// A plucked part of a tree: a client with its children
#[derive(Debug)]
pub struct DetachedTree {
    /// `None` for nodes that only lay out their children (splits)
    pub main: Option<DetachedClient>,
    pub layout: Layout,
    /// Set if `main` shows this wl surface
    pub wl_surface: Option<WlSurfaceId>,
    pub children: Vec<Self>,
}
impl DetachedTree {
    /// A tree of just this client
    pub const fn leaf(main: DetachedClient) -> Self {
        Self {
            main: Some(main),
            layout: Layout::Single,
            wl_surface: None,
            children: Vec::new(),
        }
    }

    /// Drops the client showing the surface (eg it was destroyed); its children take its place.
    /// At this tree's own root, a single child replaces it, otherwise the root keeps its children with no client.
    ///
    /// Returns whether the surface was in the tree.
    pub fn remove_wl_surface(&mut self, surface_id: WlSurfaceId) -> bool {
        if self.wl_surface == Some(surface_id) {
            if let [_] = self.children.as_slice() {
                *self = self.children.pop().expect("has one child");
            } else {
                self.main = None;
                self.wl_surface = None;
            }
            return true;
        }
        match self
            .children
            .iter()
            .position(|child| child.wl_surface == Some(surface_id))
        {
            Some(index) => {
                let removed = self.children.remove(index);
                self.children.splice(index..index, removed.children);
                true
            }
            None => self
                .children
                .iter_mut()
                .any(|child| child.remove_wl_surface(surface_id)),
        }
    }

    /// Like a holder's treeview: each client's treeview, arranged as the tree
    pub fn peek_treeview(&self) -> WorldTree<String> {
        let treeview = self.main.as_ref().map_or_else(
            || WorldTree::Base(self.layout.label().to_string()),
            DetachedClient::peek_treeview,
        );
        let mut node = RecursiveTreeNode::from_value(treeview);
        for child in &self.children {
            match child.peek_treeview() {
                WorldTree::World(child_node) => node.push_child_node(*child_node),
                base => node.push_child_node(RecursiveTreeNode::from_value(base)),
            }
        }
        WorldTree::World(Box::new(node))
    }
}
