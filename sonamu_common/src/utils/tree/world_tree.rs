use crate::utils::tree::{recursive_tree::RecursiveTreeNode, tree_node_path::TreeNodePath};
use serde::{Deserialize, Serialize};

/// One step of a `WorldTreePath`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    /// To a child of the node, in the same world
    Child(usize),
    /// To a child of the root of the node's inner world (its value).
    /// That root is the node's own row, so it has no step of its own.
    Inner(usize),
}

/// Steps from a world's root.
/// Every node has exactly one path: a node and its inner world's root are the same.
///
/// So a path is "inside" a node when it goes into its value with `Inner`:
/// `[Child(2), Child(1)]` is on node `[2, 1]`, and `[Child(2), Child(1), Inner(0)]` is its inner root's first child.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldTreePath(pub Vec<Step>);
impl WorldTreePath {
    /// The world's root
    #[must_use]
    pub const fn root() -> Self {
        Self(Vec::new())
    }

    /// The node at `path` of the outermost world
    #[must_use]
    pub fn from_tree_path(path: &TreeNodePath) -> Self {
        Self(path.0.iter().map(|&index| Step::Child(index)).collect())
    }

    /// `inner` (a path of the node's own world, eg its applet's) from the node at `node` of this world
    #[must_use]
    pub fn join(node: &TreeNodePath, inner: &Self) -> Self {
        let mut path = Self::from_tree_path(node);
        path.push_inner(inner);
        path
    }

    /// Goes on to `inner`, a path of the node's own world
    pub fn push_inner(&mut self, inner: &Self) {
        let mut steps = inner.0.iter();
        match steps.next() {
            Some(&Step::Child(index) | &Step::Inner(index)) => self.0.push(Step::Inner(index)),
            None => return,
        }
        self.0.extend(steps);
    }

    /// The node in the outermost world, and the path inside it (as a path of its own world) if it goes in
    #[must_use]
    pub fn split_world(&self) -> (TreeNodePath, Option<Self>) {
        let outer = self
            .0
            .iter()
            .position(|step| matches!(step, Step::Inner(_)))
            .unwrap_or(self.0.len());
        let node = TreeNodePath(
            self.0[..outer]
                .iter()
                .map(|&step| match step {
                    Step::Child(index) | Step::Inner(index) => index,
                })
                .collect(),
        );
        let inner = (outer < self.0.len()).then(|| {
            let mut inner = self.0[outer..].to_vec();
            if let Step::Inner(index) = inner[0] {
                inner[0] = Step::Child(index);
            }
            Self(inner)
        });
        (node, inner)
    }

    /// Whether this is `other` or a path below it
    #[must_use]
    pub fn starts_with(&self, other: &Self) -> bool {
        self.0.starts_with(&other.0)
    }

    /// Whether it goes inside a node of the outermost world
    #[must_use]
    pub fn is_inner(&self) -> bool {
        self.0.iter().any(|step| matches!(step, Step::Inner(_)))
    }
}

/// A tree whose values are other trees or the base value.
#[derive(PartialEq, Eq, Clone)]
pub enum WorldTree<T> {
    Base(T),
    World(Box<RecursiveTreeNode<Self>>),
}
impl<T> WorldTree<T> {
    pub const fn new_base(val: T) -> Self {
        Self::Base(val)
    }

    /// Given an inner, returns a new world tree that has just the root and the root's value is inner.
    pub fn new_world(inner: Self) -> Self {
        Self::World(Box::new(RecursiveTreeNode::from_value(inner)))
    }

    /// Returns the value if base, otherwise, gets the root's value
    pub fn get_root_value(&self) -> &T {
        match self {
            Self::Base(value) => value,
            Self::World(recursive_tree_node) => recursive_tree_node.get_value().get_root_value(),
        }
    }

    /// The node at `path`, `None` for a base value
    pub fn safe_get(&self, path: &WorldTreePath) -> Option<&RecursiveTreeNode<Self>> {
        let Self::World(root) = self else {
            return None;
        };
        let mut node: &RecursiveTreeNode<Self> = root;
        for &step in &path.0 {
            node = match step {
                Step::Child(index) => node.safe_get(&TreeNodePath(vec![index]))?,
                Step::Inner(index) => match node.get_value() {
                    Self::World(inner_root) => inner_root.safe_get(&TreeNodePath(vec![index]))?,
                    Self::Base(_) => return None,
                },
            };
        }
        Some(node)
    }
}
impl WorldTree<String> {
    #[must_use]
    pub fn outer_world_to_string(&self, focus_path: Option<TreeNodePath>) -> String {
        match self {
            Self::Base(inner) => {
                if focus_path.is_some() {
                    format!(">{inner}")
                } else {
                    inner.clone()
                }
            }
            Self::World(recursive_tree_node) => recursive_tree_node
                .simple_to_string(&|node| node.get_root_value().clone(), focus_path),
        }
    }
}
