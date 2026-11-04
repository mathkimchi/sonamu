//! NOTE: I wish it could be safe, but I don't think it will be.
//! Invalid states are representable, and removing nodes might mess up everything.
//! What this means that it is very easy for me to make an error in writing this.
//! However, if I don't screw up, then it should be safe from the outside.
//! The safest representation is to have a recursive struct that takes ownership of children, and stores only its own item and a vec of children.
//!
//! Terminology:
//! - The `index` of a node refers to the usize index of the node in the vector of flattened nodes.
//! - The `path` also corresponds to a node in a tree, but it specifies it in the context of the tree's structure.
//! - The `child number` of a child relative to its parent is the index that it appears in the parent's children indices.
//! - The index and path both refer to the same thing, but in different ways; the path is stored as a bunch of child numbers. Outside of this file, the index should be inaccessible.
//!
//! REVIEW: not sure if the path should correspond to a tree or not. I am leaning towards having it independent.
//!
//! CHECK: handle mutability of structure.
//!
//! TODO: macro for creation

use super::tree_node_path::{TraversableTree, TreeNodePath};

#[derive(Clone, PartialEq, Eq)]
struct Node<T> {
    item: T,
    // even this on its own can be contradictory (eg duplicates)
    children_flat_indices: Vec<usize>,

    // NOTE everything other than item and children are redundant
    // REVIEW: decide which are redundant
    _flat_index: usize,
    _path: TreeNodePath,
    _parent_flat_index: Option<usize>,
}

/// The rooted tree has exactly one root
#[derive(Clone, PartialEq, Eq)]
pub struct RootedTree<T> {
    /// REVIEW: put this in some kind of order?
    flattened_nodes: Vec<Node<T>>,
    root_flat_index: usize,
}
impl<T> RootedTree<T> {
    pub fn from_root(root_item: T) -> Self {
        Self {
            flattened_nodes: vec![Node {
                item: root_item,
                children_flat_indices: Vec::new(),
                _flat_index: 0,
                _path: TreeNodePath::new_root(),
                _parent_flat_index: None,
            }],
            root_flat_index: 0,
        }
    }

    /// If successful, returns the node's path. (Kind of arbitrary, I just wanted to use option)
    pub fn add_node(&mut self, item: T, parent_path: &TreeNodePath) -> Option<TreeNodePath> {
        // prepare parent info
        let parent_flat_index = self.get_node_flat_index(parent_path)?;

        // prepare info about the new node
        let child_number = self.flattened_nodes[parent_flat_index]
            .children_flat_indices
            .len();
        let path = {
            let mut path_vec = parent_path.0.clone();
            path_vec.push(child_number);
            TreeNodePath::from(path_vec)
        };
        let flat_index = self.flattened_nodes.len();

        // generate node
        let node = Node {
            item,
            children_flat_indices: Vec::new(),
            _flat_index: flat_index,
            _path: path.clone(),
            _parent_flat_index: Some(parent_flat_index),
        };

        // add node to flattened indices
        self.flattened_nodes.push(node);

        // update parent
        self.flattened_nodes[parent_flat_index]
            .children_flat_indices
            .push(flat_index);

        Some(path)
    }

    /// Same as add_node, but builder pattern for convenience
    #[must_use]
    pub fn builder_add_node(mut self, item: T, parent_path: &TreeNodePath) -> Self {
        self.add_node(item, parent_path);

        self
    }

    fn get_node_flat_index(&self, tree_node_path: &TreeNodePath) -> Option<usize> {
        let path_vec = tree_node_path.0.clone();
        let mut current_flat_index = self.root_flat_index;

        for child_number in path_vec {
            let current_node = &self.flattened_nodes[current_flat_index];

            let next_flat_index = current_node.children_flat_indices.get(child_number);

            current_flat_index = *next_flat_index?;
        }

        Some(current_flat_index)
    }

    /// safe for now, but might need change
    #[must_use]
    pub const fn num_nodes(&self) -> usize {
        self.flattened_nodes.len()
    }
}
impl<T> TraversableTree for RootedTree<T> {
    fn exists_at(&self, path: &TreeNodePath) -> bool {
        self.get_node_flat_index(path).is_some()
    }
}

/// REVIEW make this output option?
/// Currently just panics if impossible
impl<T> std::ops::Index<&TreeNodePath> for RootedTree<T> {
    type Output = T;

    fn index(&self, tree_node_path: &TreeNodePath) -> &Self::Output {
        &self.flattened_nodes[self.get_node_flat_index(tree_node_path).unwrap()].item
    }
}
impl<T> std::ops::IndexMut<&TreeNodePath> for RootedTree<T> {
    fn index_mut(&mut self, tree_node_path: &TreeNodePath) -> &mut Self::Output {
        let flattened_index = self.get_node_flat_index(tree_node_path).unwrap();
        &mut self.flattened_nodes[flattened_index].item
    }
}
