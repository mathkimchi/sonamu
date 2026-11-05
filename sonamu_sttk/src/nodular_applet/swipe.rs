//! 3 finger swipes between nodes (#141, #142): the view follows the fingers along a strip of shown rows
//! (siblings up / down, parent and child left / right) and settles on one when they lift.
//! Natural direction: the content moves with the fingers, so fingers up bring in the next sibling.

use super::treeview::TreeviewRow;
use sonamu_common::{
    sap::packets::SwipeView, settings::SwipeSettings, utils::tree::world_tree::WorldTreePath,
};
use sonamu_ui::display_units::ToPx;
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

/// How far (px) the fingers move before the swipe picks its axis
const LOCK_DISTANCE: f64 = 10.0;
/// How far ahead the fingers' speed carries the swipe when they lift
const PROJECTION: Duration = Duration::from_millis(200);
/// The speed is taken over this much of the latest movement
const VELOCITY_WINDOW: Duration = Duration::from_millis(100);

/// The rows a swipe moves along, in order
#[derive(Debug, Clone)]
pub struct Strip {
    pub vertical: bool,
    pub paths: Vec<WorldTreePath>,
    /// Where focus was
    pub start: usize,
}
impl Strip {
    /// Its siblings (rows under the same parent row: inner children, then the node's own), or the row alone
    pub fn siblings(rows: &[TreeviewRow], from: usize) -> Self {
        let path = &rows[from].path;
        let paths: Vec<_> = match parent(path).filter(|parent| has_row(rows, parent)) {
            Some(parent) => children(rows, &parent).cloned().collect(),
            None => vec![path.clone()],
        };
        let start = paths
            .iter()
            .position(|sibling| sibling == path)
            .unwrap_or(0);
        Self {
            vertical: true,
            paths,
            start,
        }
    }

    /// Its ancestors up to the first row, then down through the child each one last had focus
    /// (`remembered`, else the first)
    pub fn lineage(
        rows: &[TreeviewRow],
        from: usize,
        remembered: &HashMap<WorldTreePath, WorldTreePath>,
    ) -> Self {
        let path = &rows[from].path;
        let mut paths = vec![path.clone()];
        while let Some(parent) = parent(&paths[0]).filter(|parent| has_row(rows, parent)) {
            paths.insert(0, parent);
        }
        let start = paths.len() - 1;
        loop {
            let last = &paths[paths.len() - 1];
            let children: Vec<_> = children(rows, last).collect();
            let Some(&first) = children.first() else {
                break;
            };
            let child = remembered
                .get(last)
                .filter(|child| children.contains(child))
                .unwrap_or(first)
                .clone();
            paths.push(child);
        }
        Self {
            vertical: false,
            paths,
            start,
        }
    }

    fn last(&self) -> f64 {
        (self.paths.len() - 1) as f64
    }

    /// Where `position` (in rows) is drawn: stretched less and less past the ends
    fn rubber_band(&self, position: f64, limit: f64) -> f64 {
        let past = |by: f64| limit * by / (by + limit);
        if position < 0.0 {
            -past(-position)
        } else if position > self.last() {
            self.last() + past(position - self.last())
        } else {
            position
        }
    }

    /// What holders draw with the view at `position` (rubber-banded)
    pub fn view(&self, position: f64) -> SwipeView {
        let index = (position.floor().to_px() as usize).min(self.paths.len() - 1);
        let progress = position - index as f64;
        SwipeView {
            vertical: self.vertical,
            from: self.paths[index].clone(),
            to: (progress > 0.0)
                .then(|| self.paths.get(index + 1).cloned())
                .flatten(),
            progress: progress as f32,
        }
    }
}

fn parent(path: &WorldTreePath) -> Option<WorldTreePath> {
    let mut parent = path.clone();
    parent.0.pop().map(|_| parent)
}

fn has_row(rows: &[TreeviewRow], path: &WorldTreePath) -> bool {
    rows.iter().any(|row| &row.path == path)
}

/// The rows right under `parent`, in order
fn children<'a>(
    rows: &'a [TreeviewRow],
    parent: &'a WorldTreePath,
) -> impl Iterator<Item = &'a WorldTreePath> {
    rows.iter()
        .map(|row| &row.path)
        .filter(move |path| path.0.len() == parent.0.len() + 1 && path.starts_with(parent))
}

/// A swipe while the fingers are down
#[derive(Debug)]
pub struct Swipe {
    /// Since it began, until it picks its axis
    moved: [f64; 2],
    /// Where a settle it interrupted was, from its target (on its axis), to carry on from there
    carried: Option<(bool, f64)>,
    /// Picked once the fingers moved `LOCK_DISTANCE`, and where the view is on it (in rows, not rubber-banded)
    lane: Option<(Strip, f64)>,
    /// Recent moves along the lane (in rows), for the speed
    moves: VecDeque<(Instant, f64)>,
}
impl Swipe {
    pub const fn new(carried: Option<(bool, f64)>) -> Self {
        Self {
            moved: [0.0; 2],
            carried,
            lane: None,
            moves: VecDeque::new(),
        }
    }

    /// The fingers moved by `delta` (px). Once it is past `LOCK_DISTANCE`, `strip` gives the rows on its axis
    /// (`None`: nothing to swipe), then it returns the view.
    pub fn update(
        &mut self,
        delta: [f64; 2],
        settings: &SwipeSettings,
        strip: impl FnOnce(bool) -> Option<Strip>,
    ) -> Option<SwipeView> {
        let delta = if self.lane.is_none() {
            self.moved = [self.moved[0] + delta[0], self.moved[1] + delta[1]];
            let [x, y] = self.moved;
            if x.hypot(y) < LOCK_DISTANCE {
                return None;
            }
            let strip = strip(y.abs() > x.abs())?;
            let carried = match self.carried {
                Some((vertical, offset)) if vertical == strip.vertical => offset,
                _ => 0.0,
            };
            let start = strip.start as f64 + carried;
            self.lane = Some((strip, start));
            // what it moved before picking counts
            self.moved
        } else {
            delta
        };
        let (strip, position) = self.lane.as_mut()?;
        // the content follows the fingers, so moving them up (or left) goes on
        let along = -if strip.vertical { delta[1] } else { delta[0] }
            / f64::from(settings.distance.max(1.0));
        *position += along;
        let now = Instant::now();
        self.moves.push_back((now, along));
        while self
            .moves
            .front()
            .is_some_and(|&(at, _)| now - at > VELOCITY_WINDOW)
        {
            self.moves.pop_front();
        }
        Some(strip.view(strip.rubber_band(*position, f64::from(settings.rubber_band))))
    }

    /// The fingers lifted: the row it settles on, eased there from where it is (`None`: it never picked an axis)
    pub fn end(self, cancelled: bool, settings: &SwipeSettings) -> Option<Settle> {
        let (strip, position) = self.lane?;
        let shown = strip.rubber_band(position, f64::from(settings.rubber_band));
        let target = if cancelled {
            strip.start
        } else {
            let span = self
                .moves
                .front()
                .map_or(Duration::ZERO, |&(at, _)| at.elapsed())
                .max(Duration::from_millis(10));
            let velocity =
                self.moves.iter().map(|&(_, along)| along).sum::<f64>() / span.as_secs_f64();
            (shown + velocity * PROJECTION.as_secs_f64())
                .round()
                .clamp(0.0, strip.last())
                .to_px() as usize
        };
        Some(Settle {
            strip,
            from: shown,
            target,
            started: Instant::now(),
            duration: Duration::from_millis(u64::from(settings.snap_ms)),
        })
    }
}

/// Easing onto a row after the fingers lifted
#[derive(Debug)]
pub struct Settle {
    pub strip: Strip,
    from: f64,
    pub target: usize,
    started: Instant,
    duration: Duration,
}
impl Settle {
    /// Where the view is at `now`, and whether it arrived
    fn position(&self, now: Instant) -> (f64, bool) {
        let t = if self.duration.is_zero() {
            1.0
        } else {
            ((now - self.started).as_secs_f64() / self.duration.as_secs_f64()).min(1.0)
        };
        // ease out (cubic)
        let eased = 1.0 - (1.0 - t).powi(3);
        let target = self.target as f64;
        (self.from + (target - self.from) * eased, t >= 1.0)
    }

    /// The view at `now`, `None` once it arrived
    pub fn view(&self, now: Instant) -> Option<SwipeView> {
        let (position, arrived) = self.position(now);
        (!arrived).then(|| self.strip.view(position))
    }

    /// Its axis and how far from its target it is at `now`, for a swipe interrupting it to carry on
    pub fn carried(&self, now: Instant) -> (bool, f64) {
        let (position, _) = self.position(now);
        (self.strip.vertical, position - self.target as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::Strip;
    use crate::nodular_applet::treeview::treeview_rows;
    use sonamu_common::utils::tree::{
        recursive_tree::RecursiveTreeNode,
        world_tree::{
            Step::{self, Child, Inner},
            WorldTree, WorldTreePath,
        },
    };
    use std::collections::HashMap;

    fn leaf(label: &str) -> RecursiveTreeNode<WorldTree<String>> {
        RecursiveTreeNode::from_value(WorldTree::Base(label.to_string()))
    }

    /// root > x (inner i1, i2; children c1, c2 > d)
    fn rows() -> Vec<super::TreeviewRow> {
        let x_inner = WorldTree::World(Box::new(RecursiveTreeNode::new(
            WorldTree::Base("x".to_string()),
            vec![leaf("i1"), leaf("i2")],
        )));
        let c2 = RecursiveTreeNode::new(WorldTree::Base("c2".to_string()), vec![leaf("d")]);
        let x = RecursiveTreeNode::new(x_inner, vec![leaf("c1"), c2]);
        let tree = WorldTree::World(Box::new(RecursiveTreeNode::new(
            WorldTree::Base("root".to_string()),
            vec![x],
        )));
        treeview_rows(&tree, &WorldTreePath::root(), None)
    }

    fn index(rows: &[super::TreeviewRow], steps: &[Step]) -> usize {
        rows.iter().position(|row| row.path.0 == steps).unwrap()
    }

    fn paths(strip: &Strip) -> Vec<Vec<Step>> {
        strip.paths.iter().map(|path| path.0.clone()).collect()
    }

    #[test]
    fn siblings_cross_inner_and_own() {
        let rows = rows();
        let strip = Strip::siblings(&rows, index(&rows, &[Child(0), Child(0)]));
        assert_eq!(
            paths(&strip),
            [
                vec![Child(0), Inner(0)],
                vec![Child(0), Inner(1)],
                vec![Child(0), Child(0)],
                vec![Child(0), Child(1)],
            ]
        );
        assert_eq!(strip.start, 2);
        // the root alone
        assert_eq!(paths(&Strip::siblings(&rows, 0)), [vec![]]);
    }

    #[test]
    fn lineage_follows_remembered() {
        let rows = rows();
        let x = WorldTreePath(vec![Child(0)]);
        let c2 = WorldTreePath(vec![Child(0), Child(1)]);
        // nothing remembered: the first child
        let strip = Strip::lineage(&rows, index(&rows, &[Child(0)]), &HashMap::new());
        assert_eq!(
            paths(&strip),
            [vec![], vec![Child(0)], vec![Child(0), Inner(0)]]
        );
        assert_eq!(strip.start, 1);
        let remembered = HashMap::from([(x, c2)]);
        let strip = Strip::lineage(&rows, index(&rows, &[Child(0)]), &remembered);
        assert_eq!(
            paths(&strip),
            [
                vec![],
                vec![Child(0)],
                vec![Child(0), Child(1)],
                vec![Child(0), Child(1), Child(0)],
            ]
        );
    }

    #[test]
    fn view_and_rubber_band() {
        let rows = rows();
        let strip = Strip::siblings(&rows, index(&rows, &[Child(0), Child(0)]));
        let view = strip.view(2.25);
        assert_eq!(view.from.0, [Child(0), Child(0)]);
        assert_eq!(view.to.unwrap().0, [Child(0), Child(1)]);
        assert!((view.progress - 0.25).abs() < 1e-6);
        // past the last: stretched less, toward nothing
        let shown = strip.rubber_band(4.0, 0.15);
        assert!(shown > 3.0 && shown < 3.15);
        let view = strip.view(shown);
        assert_eq!(view.from.0, [Child(0), Child(1)]);
        assert!(view.to.is_none());
        // before the first
        let view = strip.view(strip.rubber_band(-1.0, 0.15));
        assert_eq!(view.from.0, [Child(0), Inner(0)]);
        assert!(view.progress < 0.0 && view.progress > -0.15);
    }
}
