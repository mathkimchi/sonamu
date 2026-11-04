//! Moving focus through the treeview's rows (#83): the root does every traversal by what is shown,
//! so it crosses into and out of applets' inner trees like any other level.

use super::treeview::TreeviewRow;
use sonamu_common::utils::tree::world_tree::WorldTreePath;
use sonamu_ui::ui_event::{KeyTrait, SwipeDirection, UIEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Traversal {
    /// The next / previous row (Tab / Shift+Tab, 3 fingers down / up), wrapping around
    Next,
    Prev,
    /// `a` (and `0`)
    Parent,
    /// `d` is the first, `1`-`8` the others
    Child(usize),
    /// `9`
    LastChild,
    /// `w` / `s`
    Sibling(isize),
    /// The focused row's node itself, rather than inside its applet (4 fingers up)
    Select,
    /// Into the selected row's applet, like Enter (4 fingers down)
    Enter,
}
impl Traversal {
    /// The traversal a key or swipe does. While `traversing` (the treeview has the keys) plain keys do it,
    /// otherwise only with Alt.
    pub fn of(ui_event: &UIEvent, traversing: bool) -> Option<Self> {
        match *ui_event {
            UIEvent::Key {
                symbol: Some(key),
                modifiers,
                pressed: true,
                ..
            } if !modifiers.ctrl && !modifiers.logo && (traversing || modifiers.alt) => {
                let c = key.to_char()?;
                if c == '\t' {
                    return Some(if modifiers.shift {
                        Self::Prev
                    } else {
                        Self::Next
                    });
                }
                // Alt+Shift+W/S swap siblings
                if modifiers.shift {
                    return None;
                }
                Some(match c {
                    'a' | '0' => Self::Parent,
                    'd' => Self::Child(0),
                    '1'..='8' => Self::Child(c.to_digit(10)? as usize - 1),
                    '9' => Self::LastChild,
                    'w' => Self::Sibling(-1),
                    's' => Self::Sibling(1),
                    _ => return None,
                })
            }
            UIEvent::Swipe {
                fingers,
                direction,
                repeat,
            } => match (fingers, direction) {
                (3, SwipeDirection::Up) => Some(Self::Prev),
                (3, SwipeDirection::Down) => Some(Self::Next),
                // once per swipe
                (4, _) if repeat => None,
                (4, SwipeDirection::Up) => Some(Self::Select),
                (4, SwipeDirection::Down) if traversing => Some(Self::Enter),
                _ => None,
            },
            _ => None,
        }
    }
}

/// The path to focus for a move from the row at `from` (`None`: focus isn't shown), if it moves.
/// `rows` are the shown rows in order (no `‹` row).
pub fn target(
    rows: &[TreeviewRow],
    from: Option<usize>,
    traversal: Traversal,
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

    #[expect(
        clippy::unnecessary_wraps,
        reason = "compared with `target`'s `Option`"
    )]
    fn path(steps: &[sonamu_common::utils::tree::world_tree::Step]) -> Option<WorldTreePath> {
        Some(WorldTreePath(steps.to_vec()))
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
            target(&rows, i2, Traversal::Sibling(1)),
            path(&[Child(0), Child(0)])
        );
        assert_eq!(
            target(&rows, c1, Traversal::Sibling(-1)),
            path(&[Child(0), Inner(1)])
        );
        assert_eq!(
            target(&rows, i2, Traversal::Next),
            path(&[Child(0), Child(0)])
        );
        assert_eq!(target(&rows, c1, Traversal::Parent), path(&[Child(0)]));
        assert_eq!(
            target(&rows, Some(1), Traversal::Child(0)),
            path(&[Child(0), Inner(0)])
        );
        assert_eq!(
            target(&rows, Some(1), Traversal::LastChild),
            path(&[Child(0), Child(1)])
        );
        assert_eq!(target(&rows, Some(5), Traversal::Next), path(&[]));
        // the last sibling stays
        assert_eq!(target(&rows, Some(5), Traversal::Sibling(1)), None);
    }
}
