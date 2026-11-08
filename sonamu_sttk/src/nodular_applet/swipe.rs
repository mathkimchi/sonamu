//! 3 finger swipes between nodes (#141, #142): the view follows the fingers along a strip of shown rows
//! (siblings left / right, parent and child up / down, #144) and settles on one when they lift.
//! Natural direction: the content moves with the fingers, so fingers left bring in the next sibling; where the rows are
//! all shown at once, the focus moves with the fingers instead.
//! `slide` and `ring` draw a swipe for holders (`StandardEvent::Swipe`).

use super::treeview::TreeviewRow;
use crate::ui;
use sonamu_common::{
    sap::packets::SwipeView,
    settings::{self, SwipeSettings},
    utils::tree::world_tree::WorldTreePath,
};
use sonamu_ui::{
    color::Color,
    display_units::{DisplayContainerSize, ToPx},
    ui_element::{Style, UITree},
};
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
    /// Its rows are all shown at once (`Treeview::shown_at_once`): the focus follows the fingers, not the content
    pub reversed: bool,
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
            vertical: false,
            paths,
            start,
            reversed: false,
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
            vertical: true,
            paths,
            start,
            reversed: false,
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
        // the content follows the fingers, so moving them up (or left) goes on; a focus ring follows them itself
        let (moved, distance) = if strip.vertical {
            (delta[1], settings.distance_y)
        } else {
            (delta[0], settings.distance_x)
        };
        let along = if strip.reversed { moved } else { -moved } / f64::from(distance.max(1.0));
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
        let span = self
            .moves
            .front()
            .map_or(Duration::ZERO, |&(at, _)| at.elapsed())
            .max(Duration::from_millis(10));
        let velocity = self.moves.iter().map(|&(_, along)| along).sum::<f64>() / span.as_secs_f64();
        let target = if cancelled {
            strip.start
        } else {
            settle_target(shown, velocity, strip.last())
        };
        Some(Settle {
            strip,
            from: shown,
            velocity,
            target,
            started: Instant::now(),
            spring: Spring {
                stiffness: f64::from(settings.stiffness.max(1.0)),
                damping_ratio: f64::from(settings.damping_ratio.max(0.01)),
            },
        })
    }
}

/// `from` moved `progress` of the way out (up, or left), and `to` right after it, cut to `size` (#141)
pub fn slide(
    size: DisplayContainerSize,
    vertical: bool,
    progress: f32,
    from: UITree,
    to: Option<UITree>,
) -> UITree {
    let [width, height] = [size.width as f32, size.height as f32];
    let at = |offset: f32, window| {
        let corner = if vertical {
            [0.0, offset * height]
        } else {
            [offset * width, 0.0]
        };
        UITree::new_container_styled(Style::absolute(corner, [width, height]), vec![window])
    };
    let mut windows = vec![at(-progress, from)];
    windows.extend(to.map(|to| at(1.0 - progress, to)));
    // a box, so nothing is drawn outside it
    UITree::new_container_styled(Style::absolute([0.0, 0.0], [width, height]), windows)
}

/// A focus ring `progress` of the way from `from` to `to` (`[x, y, width, height]`), stretching like a drop of liquid:
/// its leading edges get there early and its trailing edges late, so it is longest halfway (#141)
pub fn ring(
    from: [f32; 4],
    to: [f32; 4],
    progress: f32,
    color: Color,
    border: f32,
    radius: f32,
) -> UITree {
    let progress = progress.clamp(0.0, 1.0);
    let lead = 1.0 - (1.0 - progress).powi(2);
    let trail = progress.powi(2);
    // each axis's start (left / top) and end (right / bottom) edge
    let edges = |axis: usize| {
        let [start, end] = [from[axis], from[axis] + from[axis + 2]];
        let [to_start, to_end] = [to[axis], to[axis] + to[axis + 2]];
        let forward = to_start + to_end > start + end;
        let (start_t, end_t) = if forward {
            (trail, lead)
        } else {
            (lead, trail)
        };
        let start = start + (to_start - start) * start_t;
        [start, (end + (to_end - end) * end_t).max(start)]
    };
    let [[left, right], [top, bottom]] = [edges(0), edges(1)];
    ui::frame(
        color,
        radius,
        border,
        Style::absolute([left, top], [right - left, bottom - top]),
        Vec::new(),
    )
}

/// The row a swipe at `shown` moving at `velocity` (rows / s) settles on: one of the two shown, the one the speed
/// carries it nearer to, so a fast lift doesn't skip rows
fn settle_target(shown: f64, velocity: f64, last: f64) -> usize {
    let lower = shown.floor().clamp(0.0, last);
    let projected = shown + velocity * PROJECTION.as_secs_f64();
    let target = if projected - lower >= 0.5 {
        lower + 1.0
    } else {
        lower
    };
    target.min(last).to_px() as usize
}

/// A damped harmonic oscillator of mass 1 (as in libadwaita, niri and Apple's spring animations)
#[derive(Debug, Clone, Copy)]
struct Spring {
    stiffness: f64,
    damping_ratio: f64,
}
impl Spring {
    /// It stops this close to its target, in rows (and moving slower than this per second)
    const EPSILON: f64 = 0.001;
    /// It stops after this long anyway (eg barely damped)
    const MAX_TIME: f64 = 3.0;

    /// Where it is `t` s after starting `offset` away from its target, moving at `velocity` (per s)
    fn offset(self, offset: f64, velocity: f64, t: f64) -> f64 {
        let omega = self.stiffness.sqrt();
        let zeta = self.damping_ratio;
        let decay = (-zeta * omega * t).exp();
        if (zeta - 1.0).abs() < 1e-6 {
            decay * (offset + (velocity + omega * offset) * t)
        } else if zeta < 1.0 {
            let omega_d = omega * (1.0 - zeta * zeta).sqrt();
            decay
                * (offset * (omega_d * t).cos()
                    + (velocity + zeta * omega * offset) / omega_d * (omega_d * t).sin())
        } else {
            let omega_d = omega * (zeta * zeta - 1.0).sqrt();
            decay
                * (offset * (omega_d * t).cosh()
                    + (velocity + zeta * omega * offset) / omega_d * (omega_d * t).sinh())
        }
    }
}

/// How far (0 to 1) something springing from 0 to 1 since `started` is, with the swipe settle spring;
/// `None` once it arrived (eg the zoom-out view growing, #145)
pub fn spring_progress(started: Instant) -> Option<f32> {
    let settings = settings::get().swipe;
    let spring = Spring {
        stiffness: f64::from(settings.stiffness.max(1.0)),
        damping_ratio: f64::from(settings.damping_ratio.max(0.01)),
    };
    let t = started.elapsed().as_secs_f64();
    let offset = spring.offset(1.0, 0.0, t);
    let speed = (spring.offset(1.0, 0.0, t + 0.001) - offset) / 0.001;
    let arrived = t >= Spring::MAX_TIME
        || (offset.abs() < Spring::EPSILON && speed.abs() < Spring::EPSILON * 10.0);
    (!arrived).then_some((1.0 - offset) as f32)
}

/// Springing onto a row after the fingers lifted, starting at their speed
#[derive(Debug)]
pub struct Settle {
    pub strip: Strip,
    from: f64,
    /// Rows / s when the fingers lifted
    velocity: f64,
    pub target: usize,
    started: Instant,
    spring: Spring,
}
impl Settle {
    /// Where the view is at `now`, and whether it arrived
    fn position(&self, now: Instant) -> (f64, bool) {
        let t = (now - self.started).as_secs_f64();
        let target = self.target as f64;
        let offset = |t| self.spring.offset(self.from - target, self.velocity, t);
        let here = offset(t);
        let speed = (offset(t + 0.001) - here) / 0.001;
        let arrived = t >= Spring::MAX_TIME
            || (here.abs() < Spring::EPSILON && speed.abs() < Spring::EPSILON * 10.0);
        (if arrived { target } else { target + here }, arrived)
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
    fn settles_on_a_shown_row() {
        // a fast lift still only goes to the next row
        assert_eq!(super::settle_target(2.2, 20.0, 5.0), 3);
        assert_eq!(super::settle_target(2.8, -20.0, 5.0), 2);
        // slow: the nearer one
        assert_eq!(super::settle_target(2.4, 0.0, 5.0), 2);
        assert_eq!(super::settle_target(2.6, 0.0, 5.0), 3);
        // past the ends
        assert_eq!(super::settle_target(5.1, 3.0, 5.0), 5);
        assert_eq!(super::settle_target(-0.1, -3.0, 5.0), 0);
    }

    #[test]
    fn spring_settles() {
        let spring = super::Spring {
            stiffness: 1000.0,
            damping_ratio: 1.0,
        };
        assert!((spring.offset(1.0, 0.0, 0.0) - 1.0).abs() < 1e-9);
        assert!(spring.offset(1.0, 0.0, 0.5).abs() < 0.01);
        // critically damped: never overshoots from rest
        assert!((0..100).all(|ms| spring.offset(1.0, 0.0, f64::from(ms) / 100.0) >= 0.0));
        // a bouncy one overshoots, a sluggish one is still on its way
        let bouncy = super::Spring {
            damping_ratio: 0.3,
            ..spring
        };
        assert!((0..100).any(|ms| bouncy.offset(1.0, 0.0, f64::from(ms) / 100.0) < 0.0));
        let sluggish = super::Spring {
            damping_ratio: 3.0,
            ..spring
        };
        assert!(sluggish.offset(1.0, 0.0, 0.1) > spring.offset(1.0, 0.0, 0.1));
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
