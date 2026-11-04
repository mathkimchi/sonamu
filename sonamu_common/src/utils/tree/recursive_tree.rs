use super::{
    rooted_tree::RootedTree,
    tree_node_path::{TraversableTree, TreeNodePath},
};
use serde::{Deserialize, Serialize};
use std::ops::{Index, IndexMut};

/// This and `RootedTree` store the same information,
/// but this is the conceptual version of it.
///
/// This should be safe (all representations are valid)
/// but rooted tree is "optimized" (at least I tried to).
/// A recursive tree node only stores information downwards.
/// In other words, it does not know where it is in
/// relation to the root and what its parent is.
/// For some operations, it acts as if it was the root.
///
/// **This should only be used for
/// intermediate representations** between some other type
/// and rooted tree.
/// For representing trees that will be used for a while,
/// use rooted tree.
///
/// REVIEW: right now, I purposefully *didn't* implement
/// useful functions for recursive tree node, to ensure
/// it is converted to rooted tree, but idk if this
/// is a good idea.
///
/// NOTE: Idk, how serialize and deserialize can be derived
/// if T isn't guranteed to be either, but I am not complaining.
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct RecursiveTreeNode<T> {
    value: T,
    children: Vec<Self>,
}
impl<T> RecursiveTreeNode<T> {
    pub const fn new(value: T, children: Vec<Self>) -> Self {
        Self { value, children }
    }

    pub const fn from_value(value: T) -> Self {
        Self::new(value, Vec::new())
    }

    /// Gets the value held by this node.
    /// If this is the root node, we can say this is the root value.
    pub const fn get_value(&self) -> &T {
        &self.value
    }

    pub const fn get_value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// add child as last
    pub fn push_child_node(&mut self, child_node: Self) {
        self.children.push(child_node);
    }

    pub fn safe_get(&self, path: &TreeNodePath) -> Option<&Self> {
        let mut node = self;

        for index in &path.0 {
            node = node.children.get(*index)?;
        }

        Some(node)
    }
    pub fn safe_get_mut(&mut self, path: &TreeNodePath) -> Option<&mut Self> {
        let mut node = self;

        for index in &path.0 {
            node = node.children.get_mut(*index)?;
        }

        Some(node)
    }

    // fn append_child_to_display<O: AddAssign + Add<Output = O> + Clone>(
    //     &self,
    //     v: &mut O,
    //     value_getter: &impl Fn(TreeNodePath, &T) -> O,
    //     indent: O,
    //     pipe: O,
    //     branch: O,
    //     corner: O,
    //     new_line: O,
    //     prefix: O,
    //     last_child: bool,
    // ) {
    //     *v += prefix.clone();

    //     let child_prefix = if last_child {
    //         *v += corner.clone();
    //         prefix.clone() + indent.clone()
    //     } else {
    //         *v += branch.clone();
    //         prefix.clone() + pipe.clone()
    //     };

    //     self.append_to_display(
    //         v,
    //         value_getter,
    //         indent,
    //         pipe,
    //         branch,
    //         corner,
    //         new_line,
    //         child_prefix,
    //     );
    // }

    // fn append_to_display<O: AddAssign + Add + Clone>(
    //     &self,
    //     v: &mut O,
    //     value_getter: &impl Fn(TreeNodePath, &T) -> O,
    //     indent: O,
    //     pipe: O,
    //     branch: O,
    //     corner: O,
    //     new_line: O,
    //     prefix: O,
    // ) {
    //     // value stringizer should return a single line
    //     *v += value_getter(self.get_value());
    //     *v += new_line;

    //     if let Some((last_child, normal_children)) = self.children.split_last() {
    //         for child in normal_children {
    //             child.append_child_to_display(s, value_stringizer, prefix, false);
    //         }

    //         last_child.append_child_to_display(s, value_stringizer, prefix, true);
    //     }
    // }

    // pub fn generic_display<O: Default + AddAssign + Add + Clone>(
    //     &self,
    //     value_getter: &impl Fn(TreeNodePath, &T) -> O,
    //     indent: O,
    //     pipe: O,
    //     branch: O,
    //     corner: O,
    //     new_line: O,
    // ) -> O {
    //     let mut v = O::default();

    //     self.append_to_display(
    //         &mut v,
    //         value_getter,
    //         indent,
    //         pipe,
    //         branch,
    //         corner,
    //         new_line,
    //         O::default(),
    //     );

    //     v
    // }

    // /// Uses the algorithm from:
    // /// https://andrewlock.net/creating-an-ascii-art-tree-in-csharp/
    // pub fn simple_to_string(&self, value_getter: &impl Fn(TreeNodePath, &T) -> String) -> String {
    //     self.generic_display(
    //         value_getter,
    //         "    ".to_string(),
    //         "  │ ".to_string(),
    //         "  ├─".to_string(),
    //         "  └─".to_string(),
    //         "\n".to_string(),
    //     )
    //     .join("")
    // }

    fn append_child_to_string_with_prefix(
        &self,
        s: &mut String,
        value_stringizer: &impl Fn(&T) -> String,
        prefix: &str,
        last_child: bool,
        focus_path: Option<TreeNodePath>,
    ) {
        s.push_str(prefix);

        let child_prefix = if last_child {
            s.push('└');
            prefix.to_string() + " "
        } else {
            s.push('├');
            prefix.to_string() + "│"
        };

        self.append_to_string_with_prefix(s, value_stringizer, &child_prefix, focus_path);
    }

    fn append_to_string_with_prefix(
        &self,
        s: &mut String,
        value_stringizer: &impl Fn(&T) -> String,
        prefix: &str,
        focus_path: Option<TreeNodePath>,
    ) {
        if let Some(focus_path) = &focus_path {
            if focus_path.is_root() {
                s.push('>');
            } else {
                s.push('→');
            }
        }
        // value stringizer should return a single line
        s.push_str(&value_stringizer(self.get_value()));
        s.push('\n');

        if let Some((last_child, normal_children)) = self.children.split_last() {
            for (child_index, child) in normal_children.iter().enumerate() {
                child.append_child_to_string_with_prefix(
                    s,
                    value_stringizer,
                    prefix,
                    false,
                    focus_path.as_ref().and_then(|focus_path| {
                        if focus_path.0.first() == Some(&child_index) {
                            Some(TreeNodePath(focus_path.0[1..].to_vec()))
                        } else {
                            None
                        }
                    }),
                );
            }

            last_child.append_child_to_string_with_prefix(
                s,
                value_stringizer,
                prefix,
                true,
                focus_path.and_then(|focus_path| {
                    if focus_path.0.first() == Some(&(self.children.len() - 1)) {
                        Some(TreeNodePath(focus_path.0[1..].to_vec()))
                    } else {
                        None
                    }
                }),
            );
        }
    }

    /// Uses the algorithm from:
    /// https://andrewlock.net/creating-an-ascii-art-tree-in-csharp/
    pub fn simple_to_string(
        &self,
        value_stringizer: &impl Fn(&T) -> String,
        focus_path: Option<TreeNodePath>,
    ) -> String {
        let mut s = String::new();

        self.append_to_string_with_prefix(&mut s, value_stringizer, "", focus_path);

        s
    }
}
impl<T> From<RecursiveTreeNode<T>> for RootedTree<T> {
    fn from(recursive_tree_node: RecursiveTreeNode<T>) -> Self {
        let mut rooted_tree = Self::from_root(recursive_tree_node.value);

        let mut unvisited_children = vec![(TreeNodePath::new_root(), recursive_tree_node.children)];

        while !unvisited_children.is_empty() {
            let mut new_unvisited_children = Vec::new();

            for (parent_path, children) in unvisited_children {
                for child in children {
                    let child_path = rooted_tree.add_node(child.value, &parent_path).unwrap();
                    new_unvisited_children.push((child_path, child.children));
                }
            }

            unvisited_children = new_unvisited_children;
        }

        rooted_tree
    }
}

impl<T> Index<&TreeNodePath> for RecursiveTreeNode<T> {
    type Output = T;

    fn index(&self, path: &TreeNodePath) -> &Self::Output {
        &self.safe_get(path).unwrap().value
    }
}
impl<T> IndexMut<&TreeNodePath> for RecursiveTreeNode<T> {
    fn index_mut(&mut self, path: &TreeNodePath) -> &mut Self::Output {
        &mut self.safe_get_mut(path).unwrap().value
    }
}

impl<T> TraversableTree for RecursiveTreeNode<T> {
    fn exists_at(&self, path: &TreeNodePath) -> bool {
        self.safe_get(path).is_some()
    }
}
