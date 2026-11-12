//! The status bar (#150): a row at the top of every screen, drawn by the root. See `.claude/STATUS_BAR.md`.

use crate::{command_hub::bar_bg, ui};
use sonamu_common::settings::Settings;
use sonamu_ui::{
    ui_element::{Style, UITree, font_size},
    ui_event::{MouseEvent, MouseEventKind},
};

use super::treeview::BTN_LEFT;

/// Room between segments and at the bar's ends
const GAP: f32 = 12.0;

/// The bar's height (px), taken from the top of every screen
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a row's height"
)]
pub fn height() -> u32 {
    ui::row_height().ceil() as u32
}

/// A monospace character's width, about (`CharGrid`'s cell)
fn char_width() -> f32 {
    font_size() / 1.933
}

/// What a segment of the bar is
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    /// `▾`/`▴`: shows or hides the treeview
    Tree,
}

/// What a click on the bar does, for the root
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BarAction {
    ToggleTreeview,
}

/// What the bar shows
pub struct BarState {
    pub treeview_shown: bool,
}

/// The segments with their text and horizontal place (`[left, width]`), for both drawing and clicking
fn layout(state: &BarState, width: f32) -> Vec<(Segment, String, [f32; 2])> {
    let left = [(
        Segment::Tree,
        if state.treeview_shown { "▾" } else { "▴" }.to_string(),
    )];
    let right: [(Segment, String); 0] = [];
    let mut placed = Vec::new();
    let mut x = GAP;
    for (segment, text) in left {
        let w = text.chars().count() as f32 * char_width();
        placed.push((segment, text, [x, w]));
        x += w + GAP;
    }
    let mut x = width - GAP;
    for (segment, text) in right.into_iter().rev() {
        let w = text.chars().count() as f32 * char_width();
        x -= w;
        placed.push((segment, text, [x, w]));
        x -= GAP;
    }
    placed
}

/// The bar, `width` wide, at the top of a screen
pub fn draw(settings: &Settings, state: &BarState, width: f32) -> UITree {
    let height = height() as f32;
    let mut children = vec![ui::rect(bar_bg(&settings.palette), [0.0, 0.0], [width, height])];
    let rows = ui::Rows::with_height(0.0, height);
    for (_, text, [left, w]) in layout(state, width) {
        children.push(
            rows.text(ui::text(text, settings.palette.fg), left, left + w)
                .with_pointer(),
        );
    }
    UITree::new_container_styled(Style::absolute([0.0, 0.0], [width, height]), children)
}

/// A mouse event over the bar (in its coords)
pub fn handle_mouse(state: &BarState, event: &MouseEvent, width: f32) -> Option<BarAction> {
    let MouseEventKind::Button {
        button: BTN_LEFT,
        pressed: true,
    } = event.kind
    else {
        return None;
    };
    let [x, _] = event.position;
    let (segment, ..) = layout(state, width)
        .into_iter()
        // half the gap around each segment is its too
        .find(|(_, _, [left, w])| {
            (f64::from(left - GAP / 2.0)..f64::from(left + w + GAP / 2.0)).contains(&x)
        })?;
    match segment {
        Segment::Tree => Some(BarAction::ToggleTreeview),
    }
}
