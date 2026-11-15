//! Moving focus through the treeview's rows (#83): the root does every traversal by what is shown,
//! so it crosses into and out of applets' inner trees like any other level.

use super::treeview::TreeviewRow;
use sonamu_common::{settings, utils::tree::world_tree::WorldTreePath};
use sonamu_ui::ui_event::{KeyModifiers, UIEvent};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Traversal {
    /// The next / previous row (Tab / Shift+Tab), wrapping around
    Next,
    Prev,
    /// `a` (and `0`)
    Parent,
    /// `1`-`8`
    Child(usize),
    /// `d`: the child focus was last under, else the first
    RecentChild,
    /// `9`
    LastChild,
    /// `w` / `s`
    Sibling(isize),
    /// Out of focus's world, to the node holding it (4 fingers up)
    Select,
    /// Into the next world, as deep as focus was there (4 fingers down, Alt+Enter)
    Enter,
}
impl Traversal {
    /// The traversal a key does (setting `keybinds`, command `traverse`; swipes are the root's, `swipe`). While
    /// `traversing` (the treeview has the keys), keybinds with Alt also work without it, but `enter`.
    pub fn of(ui_event: &UIEvent, traversing: bool) -> Option<Self> {
        let &UIEvent::Key { modifiers, .. } = ui_event else {
            return None;
        };
        let traversal = |modifiers| {
            let (command, args) = settings::keybind_command(ui_event, modifiers)?;
            (command == "traverse")
                .then(|| Self::from_args(&args))
                .flatten()
        };
        traversal(modifiers).or_else(|| {
            let with_alt = KeyModifiers {
                alt: true,
                ..modifiers
            };
            (traversing && !modifiers.alt)
                .then(|| traversal(with_alt))
                .flatten()
                .filter(|traversal| *traversal != Self::Enter)
        })
    }

    /// From a `traverse` keybind's args: `next`, `prev`, `parent`, `child` and n (from 1), `recent_child`,
    /// `last_child`, `sibling` and an offset, `select`, `enter`
    pub(crate) fn from_args(args: &[serde_json::Value]) -> Option<Self> {
        let number = || args.get(1)?.as_i64();
        Some(match args.first()?.as_str()? {
            "next" => Self::Next,
            "prev" => Self::Prev,
            "parent" => Self::Parent,
            "child" => Self::Child(usize::try_from(number()?).ok()?.checked_sub(1)?),
            "recent_child" => Self::RecentChild,
            "last_child" => Self::LastChild,
            "sibling" => Self::Sibling(isize::try_from(number()?).ok()?),
            "select" => Self::Select,
            "enter" => Self::Enter,
            _ => return None,
        })
    }
}

/// The path to focus for a move from the row at `from` (`None`: focus isn't shown), if it moves.
/// `rows` are the shown rows in order (no `‹` row); `remembered` is each row's child focus was last under.
pub fn target(
    rows: &[TreeviewRow],
    from: Option<usize>,
    traversal: Traversal,
    remembered: &HashMap<WorldTreePath, WorldTreePath>,
) -> Option<WorldTreePath> {
    let Some(from) = from else {
        return rows.first().map(|row| row.path.clone());
    };
    let depth = rows[from].depth;
    // its rows' children
    let children = || {
        rows[from + 1..]
            .iter()
            .take_while(move |row| row.depth > depth)
            .filter(move |row| row.depth == depth + 1)
    };
    let row = match traversal {
        Traversal::Next => &rows[(from + 1) % rows.len()],
        Traversal::Prev => &rows[(from + rows.len() - 1) % rows.len()],
        Traversal::Parent => {
            let Some(row) = rows[..from].iter().rev().find(|row| row.depth < depth) else {
                // past the view root: its parent, which widens the view
                let mut parent = rows[from].path.clone();
                return parent.0.pop().map(|_| parent);
            };
            row
        }
        Traversal::Child(index) => children().nth(index)?,
        Traversal::RecentChild => {
            let path = remembered.get(&rows[from].path);
            children()
                .find(|row| Some(&row.path) == path)
                .or_else(|| children().next())?
        }
        Traversal::LastChild => children().last()?,
        Traversal::Sibling(shift) => {
            let before = rows[..from]
                .iter()
                .rev()
                .take_while(|row| row.depth >= depth)
                .filter(|row| row.depth == depth);
            let after = rows[from + 1..]
                .iter()
                .take_while(|row| row.depth >= depth)
                .filter(|row| row.depth == depth);
            let found = if shift < 0 {
                before.take(shift.unsigned_abs()).last()
            } else {
                after.take(shift.unsigned_abs()).last()
            };
            // clamped at the first / last sibling
            found?
        }
        Traversal::Select | Traversal::Enter => return None,
    };
    Some(row.path.clone())
}

#[cfg(test)]
mod tests {
    use super::{Traversal, target};
    use crate::nodular_applet::treeview::treeview_rows;
    use sonamu_common::utils::tree::{
        recursive_tree::RecursiveTreeNode,
        world_tree::{
            Step::{Child, Inner},
            WorldTree, WorldTreePath,
        },
    };
    use std::collections::HashMap;

    fn leaf(label: &str) -> RecursiveTreeNode<WorldTree<String>> {
        RecursiveTreeNode::from_value(WorldTree::Base(label.to_string()))
    }

    /// root > x (inner i1, i2; children c1, c2)
    fn rows() -> Vec<super::TreeviewRow> {
        let x_inner = WorldTree::World(Box::new(RecursiveTreeNode::new(
            WorldTree::Base("x".to_string()),
            vec![leaf("i1"), leaf("i2")],
        )));
        let x = RecursiveTreeNode::new(x_inner, vec![leaf("c1"), leaf("c2")]);
        let tree = WorldTree::World(Box::new(RecursiveTreeNode::new(
            WorldTree::Base("root".to_string()),
            vec![x],
        )));
        treeview_rows(&tree, &WorldTreePath::root(), None)
    }

    fn path(steps: &[sonamu_common::utils::tree::world_tree::Step]) -> WorldTreePath {
        WorldTreePath(steps.to_vec())
    }

    #[test]
    fn shown_order() {
        let paths: Vec<_> = rows().into_iter().map(|row| row.path.0).collect();
        assert_eq!(
            paths,
            [
                vec![],
                vec![Child(0)],
                vec![Child(0), Inner(0)],
                vec![Child(0), Inner(1)],
                vec![Child(0), Child(0)],
                vec![Child(0), Child(1)],
            ]
        );
    }

    #[test]
    fn crosses_levels() {
        let rows = rows();
        let i2 = Some(3);
        let c1 = Some(4);
        assert_eq!(
            target(&rows, i2, Traversal::Sibling(1), &HashMap::new()),
            Some(path(&[Child(0), Child(0)]))
        );
        assert_eq!(
            target(&rows, c1, Traversal::Sibling(-1), &HashMap::new()),
            Some(path(&[Child(0), Inner(1)]))
        );
        assert_eq!(
            target(&rows, i2, Traversal::Next, &HashMap::new()),
            Some(path(&[Child(0), Child(0)]))
        );
        assert_eq!(
            target(&rows, c1, Traversal::Parent, &HashMap::new()),
            Some(path(&[Child(0)]))
        );
        assert_eq!(
            target(&rows, Some(1), Traversal::Child(0), &HashMap::new()),
            Some(path(&[Child(0), Inner(0)]))
        );
        // the remembered child, else the first
        assert_eq!(
            target(&rows, Some(1), Traversal::RecentChild, &HashMap::new()),
            Some(path(&[Child(0), Inner(0)]))
        );
        let remembered = HashMap::from([(
            WorldTreePath(vec![Child(0)]),
            WorldTreePath(vec![Child(0), Child(1)]),
        )]);
        assert_eq!(
            target(&rows, Some(1), Traversal::RecentChild, &remembered),
            Some(path(&[Child(0), Child(1)]))
        );
        assert_eq!(
            target(&rows, Some(1), Traversal::LastChild, &HashMap::new()),
            Some(path(&[Child(0), Child(1)]))
        );
        assert_eq!(
            target(&rows, Some(5), Traversal::Next, &HashMap::new()),
            Some(path(&[]))
        );
        // the last sibling stays
        assert_eq!(
            target(&rows, Some(5), Traversal::Sibling(1), &HashMap::new()),
            None
        );
    }
}
