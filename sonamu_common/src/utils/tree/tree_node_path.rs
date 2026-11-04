/// Like pointers but has the context of the tree structure
/// Also like a file path
///
/// NOTE: Most functions for a path are better thought of as functions for the Node that the path refers to
/// TODO: use slice instead
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct TreeNodePath(pub Vec<usize>);
impl TreeNodePath {
    #[must_use]
    pub const fn new_root() -> Self {
        Self(Vec::new())
    }

    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// root has depth=0
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.0.len()
    }
}
impl<T: Into<Vec<usize>>> From<T> for TreeNodePath {
    fn from(val: T) -> Self {
        Self(val.into())
    }
}

pub trait TraversableTree {
    fn exists_at(&self, path: &TreeNodePath) -> bool;

    fn iter_paths_dfs(&self) -> DfsPathsIterator<'_, Self>
    where
        Self: Sized,
    {
        DfsPathsIterator {
            tree_to_traverse: self,
            next_path: Some(TreeNodePath::new_root()),
        }
    }

    fn collect_paths_dfs(&self) -> Vec<TreeNodePath>
    where
        Self: Sized,
    {
        self.iter_paths_dfs().collect()
    }
}

pub const TREE_TRAVERSE_KEYS: [char; 16] = [
    'w', 'a', 's', 'd', 'q', 'e', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
];

/// - "wasd" should vaguely correspond to how it looks when the tree is vertically displayed
/// - "qe" is bfs prev and next (chosen for their proximity to "wasd" in qwerty layout)
/// - "0" goes to parent (same as "a")
/// - 1-8 goes to n-th child, but is 1-indexed (eldest child is 1)
/// - "9" goes to last child
#[derive(Debug, Clone, Copy)]
pub enum TreeTraverseOperation {
    /// `a` and `0`
    Parent,

    /// Stands for Relative Sibling
    /// `w` is `RelShiftSibling(-1)`, which was previously equivalent to `PrevSibling`
    /// `s` is `RelShiftSibling(1)`, which was previously equivalent to `NextSibling`
    RelShiftSibling(isize),

    /// `q`
    BfsPrev,
    /// `w`
    BfsNext,

    /// `1-8`
    /// This is 0-indexed
    /// `d` is `Child(0)`, which was previously equivalent to `FirstChild`
    Child(usize),
    /// `9`
    LastChild,
}
impl TreeTraverseOperation {
    #[must_use]
    pub fn from_char(traverse_key: char) -> Option<Self> {
        match traverse_key {
            'a' => Some(Self::Parent),
            'd' => Some(Self::Child(0)),
            'w' => Some(Self::RelShiftSibling(-1)),
            's' => Some(Self::RelShiftSibling(1)),

            'q' => Some(Self::BfsPrev),
            'e' => Some(Self::BfsNext),

            '0' => Some(Self::Parent), // same as 'a'
            // NOTE: the `?` should never happen so technically `panic` would be fine as well, but don't risk it
            '1'..='8' => Some(Self::Child(traverse_key.to_digit(10)? as usize - 1)),
            '9' => Some(Self::LastChild),

            _ => None,
        }
    }
}

/// For the traverse functions, some require the original tree to be safe
mod tree_node_path_traversal_impls {
    use super::{TraversableTree, TreeNodePath, TreeTraverseOperation};
    impl TreeNodePath {
        #[must_use]
        pub fn traverse_to_parent(&self) -> Option<Self> {
            if self.0.is_empty() {
                None
            } else {
                // the parent path is this path without the last element
                let mut parent_path_vec = self.0.clone();
                parent_path_vec.pop();
                Some(Self(parent_path_vec))
            }
        }

        #[must_use]
        pub fn unchecked_traverse_to_child(&self, child_index: usize) -> Self {
            let mut child_path_vec = self.0.clone();

            child_path_vec.push(child_index);

            Self(child_path_vec)
        }

        pub fn children_paths(&self, tree_to_traverse: &impl TraversableTree) -> Vec<Self> {
            let mut children_paths = Vec::new();
            for child_index in 0.. {
                if let Some(child_path) = self.traverse_to_child(tree_to_traverse, child_index) {
                    children_paths.push(child_path);
                } else {
                    break;
                }
            }
            children_paths
        }

        /// Needs the tree to make sure that the child exists
        pub fn traverse_to_child(
            &self,
            tree_to_traverse: &impl TraversableTree,
            child_index: usize,
        ) -> Option<Self> {
            let child_path = {
                let mut child_path_vec = self.0.clone();

                child_path_vec.push(child_index);

                Self(child_path_vec)
            };

            // check that path points to an existing node
            if tree_to_traverse.exists_at(&child_path) {
                Some(child_path)
            } else {
                None
            }
        }

        /// Needs the tree to make sure that the child exists
        pub fn traverse_to_first_child(
            &self,
            tree_to_traverse: &impl TraversableTree,
        ) -> Option<Self> {
            self.traverse_to_child(tree_to_traverse, 0)
        }

        /// Needs the tree to make sure that the child exists
        pub fn traverse_to_last_child(
            &self,
            tree_to_traverse: &impl TraversableTree,
        ) -> Option<Self> {
            self.traverse_to_child(
                tree_to_traverse,
                self.children_paths(tree_to_traverse).len().checked_sub(1)?,
            )
        }

        /// No wrapping
        #[must_use]
        pub fn traverse_to_previous_sibling(&self) -> Option<Self> {
            let mut sibling_path_vec = self.0.clone();
            let last_child_number = sibling_path_vec.pop()?.checked_sub(1)?;
            sibling_path_vec.push(last_child_number);
            Some(Self(sibling_path_vec))
        }

        /// No wrapping
        pub fn traverse_to_next_sibling(
            &self,
            tree_to_traverse: &impl TraversableTree,
        ) -> Option<Self> {
            let sibling_path = {
                let mut sibling_path_vec = self.0.clone();
                let last_child_number = sibling_path_vec.pop()?.checked_add(1)?;
                sibling_path_vec.push(last_child_number);
                Self(sibling_path_vec)
            };

            // check that path points to an existing node
            if tree_to_traverse.exists_at(&sibling_path) {
                Some(sibling_path)
            } else {
                None
            }
        }

        /// No wrapping
        pub fn traverse_rel_shift_sibling(
            &self,
            tree_to_traverse: &impl TraversableTree,
            shift: isize,
        ) -> Option<Self> {
            let sibling_path = {
                let mut sibling_path_vec = self.0.clone();
                let last_child_number =
                    usize::try_from(sibling_path_vec.pop()?.cast_signed().checked_add(shift)?)
                        .ok()?;
                sibling_path_vec.push(last_child_number);
                Self(sibling_path_vec)
            };

            // check that path points to an existing node
            if tree_to_traverse.exists_at(&sibling_path) {
                Some(sibling_path)
            } else {
                None
            }
        }

        pub fn traverse_dfs_next(&self, tree_to_traverse: &impl TraversableTree) -> Option<Self> {
            self.traverse_to_first_child(tree_to_traverse).map_or_else(
                || {
                    // current is leaf
                    // climb up (traverse to parents) until there is a next sibling or until at root
                    let mut intermediate_path = self.clone();

                    loop {
                        if let Some(next_path) =
                            intermediate_path.traverse_to_next_sibling(tree_to_traverse)
                        {
                            break Some(next_path);
                        }

                        if let Some(intermediate_path_parent) =
                            intermediate_path.traverse_to_parent()
                        {
                            intermediate_path = intermediate_path_parent;
                        } else {
                            // intermediate path is root
                            break None;
                        }
                    }
                },
                Some,
            )
        }

        pub fn traverse_dfs_prev(&self, tree_to_traverse: &impl TraversableTree) -> Option<Self> {
            self.traverse_to_previous_sibling().map_or_else(
                || self.traverse_to_parent(),
                |previous_sibling| {
                    // traverse to previous sibling's last child's last child...
                    let mut path = previous_sibling;
                    while let Some(last_child) = path.traverse_to_last_child(tree_to_traverse) {
                        path = last_child;
                    }
                    Some(path)
                },
            )
        }

        /// returns [`None`] if the traversal can not be done
        pub fn checked_traverse_on_operation(
            &self,
            tree_to_traverse: &impl TraversableTree,
            traverse_operation: TreeTraverseOperation,
        ) -> Option<Self> {
            match traverse_operation {
                TreeTraverseOperation::Parent => self.traverse_to_parent(),
                TreeTraverseOperation::RelShiftSibling(shift) => {
                    self.traverse_rel_shift_sibling(tree_to_traverse, shift)
                }
                TreeTraverseOperation::BfsPrev => self.traverse_dfs_prev(tree_to_traverse),
                TreeTraverseOperation::BfsNext => self.traverse_dfs_next(tree_to_traverse),
                TreeTraverseOperation::Child(child_index) => {
                    self.traverse_to_child(tree_to_traverse, child_index)
                }
                TreeTraverseOperation::LastChild => self.traverse_to_last_child(tree_to_traverse),
            }
        }

        /// `checked_traverse_on_operation` but if something goes wrong, return self
        #[must_use]
        pub fn clamped_traverse_on_operation(
            &self,
            tree_to_traverse: &impl TraversableTree,
            traverse_operation: TreeTraverseOperation,
        ) -> Self {
            self.checked_traverse_on_operation(tree_to_traverse, traverse_operation)
                .unwrap_or_else(|| self.clone())
        }

        /// REVIEW: not sure if this belongs here, as it should be pure logic but this is more input handling
        ///
        /// TODO: seperate functions for wrapped traversal
        #[deprecated]
        #[must_use]
        pub fn clamped_traverse_based_on_wasd(
            &self,
            tree_to_traverse: &impl TraversableTree,
            traverse_key: char,
        ) -> Self {
            if let Some(traverse_operation) = TreeTraverseOperation::from_char(traverse_key) {
                self.clamped_traverse_on_operation(tree_to_traverse, traverse_operation)
            } else {
                self.clone()
            }
        }
    }
}

/// depth first search post-order
///
/// Eg: 1 { 2 { 3, 4 }, 5 { 6 } }
///
/// REVIEW: if rooted tree stores nodes in post order, this could be much simpler
///
/// NOTE: code is based off of the Iter for Vec
pub struct DfsPathsIterator<'a, T: 'a + TraversableTree> {
    tree_to_traverse: &'a T,
    next_path: Option<TreeNodePath>,
}
impl<T: TraversableTree> Iterator for DfsPathsIterator<'_, T> {
    type Item = TreeNodePath;

    /// # Explanation of finding the next path non-recursively:
    ///
    /// ## Terminology
    /// - `visited`
    /// - `fully explored`: node and all its children are fully visited
    /// - `a is left of b`: a should be visited before b
    ///
    /// ## Properties
    ///
    /// The next node should:
    /// 1. be unvisited
    /// 2. have all ancestors visited
    /// 3. have all older sibling fully expored
    ///
    /// iff fully explored:
    /// - node's last child's last child's ... is visited
    /// - aka node [is visited] and [[is leaf] or [node's last child is fully explored]]
    ///
    /// iff a visited before b:
    /// - a is parent of b or when they first split, a stems from the older sibling
    /// - easiest to compare the paths
    ///
    /// ## Cases to gain insight for generalization:
    /// - current has child: then next should be the first child
    /// - current is leaf but has next sibling: then current is fully explored and next is next sibling
    /// - current is leaf and last sibling: then parent is fully explored
    /// - parent is fully explored but has next sibling: parent's next sibling is next
    /// - parent is fully explored and last sibling: then grandparent is fully explored if grandparent has
    /// - ancestor D is fully explored but has next sibling: ancestor D's next sibling is next
    /// - ancestor D(epth) is fully explored and last sibling: ancestor D-1 is fully explored
    ///
    /// So, once at a leaf, the youngest ancestor (or self)'s next sibling that exists is next
    /// if none of those exist then everything is fully explored
    fn next(&mut self) -> Option<Self::Item> {
        let current_path = self.next_path.clone()?;

        self.next_path = current_path.traverse_dfs_next(self.tree_to_traverse);

        Some(current_path)
    }
}
