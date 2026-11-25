//! The history as a side view (#162): the closed nodes, newest first

use super::{
    history::{self, Closed},
    status_bar::char_width,
    treeview::close_width,
};
use crate::{
    session::saved_label,
    ui::{self, BTN_LEFT, PAD, Rows, row_height, with_opacity},
};
use sonamu_common::settings::{self, HistoryMode, Settings};
use sonamu_ui::{
    ui_element::{Style, UIElement, UITree, font_size},
    ui_event::{MouseEvent, MouseEventKind},
};

/// Rows above the entries: the title, the mode and Clear
const FIXED: usize = 3;
const MODE_ROW: usize = 1;
const CLEAR_ROW: usize = 2;
const MODE_LABEL: &str = "Save: ";
const MODE_SEPARATOR: &str = " · ";
const MODES: [(&str, HistoryMode); 3] = [
    ("full", HistoryMode::Full),
    ("session", HistoryMode::Session),
    ("none", HistoryMode::None),
];
const CLEAR_ASK: &str = "Clear all? Click again";

/// What a click on the view asks the root to do
#[derive(Clone, Copy)]
pub enum HistoryAction {
    /// `history.mode`'s value, eg `session`
    SetMode(&'static str),
    /// The entry with its group
    Restore(u64),
    /// The entry with its group
    Remove(u64),
    Clear,
}

pub struct HistoryResponse {
    pub action: Option<HistoryAction>,
    /// The view used the event, so the applets shouldn't get it
    pub consumed: bool,
    pub damaged: bool,
}

/// Mouse state over the history view
#[derive(Default)]
pub struct HistoryPointer {
    /// The row under the pointer: `FIXED` + the entry's row among the entries, past the fixed rows
    hovered: Option<usize>,
    hovered_close: bool,
    /// The first entry shown
    scroll: usize,
    /// Mouse scrolling not yet a row
    scrolled: ui::ScrollSteps,
    /// Clear was clicked once
    confirm: bool,
}

/// A closed node's name
fn label(closed: &Closed) -> String {
    closed
        .tree
        .main
        .as_ref()
        .map(saved_label)
        .or_else(|| closed.tree.label.clone())
        .unwrap_or_else(|| "Container".to_owned())
}

/// A row per entry, newest first, with the id of the entry it restores; a group is its newest's row and how many
/// more
fn rows(entries: &[Closed]) -> Vec<(u64, String)> {
    let mut rows: Vec<(u64, String)> = Vec::new();
    let mut seen = Vec::new();
    for entry in entries.iter().rev() {
        let Some(group) = entry.group else {
            rows.push((entry.id, label(entry)));
            continue;
        };
        if !seen.contains(&group) {
            seen.push(group);
            let more = entries
                .iter()
                .filter(|other| other.group == Some(group))
                .count()
                - 1;
            rows.push((
                entry.id,
                if more == 0 {
                    label(entry)
                } else {
                    format!("{} +{more}", label(entry))
                },
            ));
        }
    }
    rows
}

/// Each mode word and `[left, right)` in the mode row
fn mode_words() -> Vec<(&'static str, f32, f32)> {
    let mut left = PAD + MODE_LABEL.chars().count() as f32 * char_width();
    MODES
        .iter()
        .map(|&(word, _)| {
            let right = left + word.chars().count() as f32 * char_width();
            let span = (word, left, right);
            left = right + MODE_SEPARATOR.chars().count() as f32 * char_width();
            span
        })
        .collect()
}

/// How many entries fit in `height`
fn shown(height: f32) -> usize {
    Rows::new(0.0).fitting(height).saturating_sub(FIXED)
}

impl HistoryPointer {
    /// `event` is in the view's coords; `inside`: it's over the view
    pub fn handle_mouse(
        &mut self,
        event: &MouseEvent,
        inside: bool,
        [width, height]: [f32; 2],
    ) -> HistoryResponse {
        let [x, y] = event.position;
        let entries = rows(&history::entries());
        let physical = inside.then(|| Rows::new(0.0).row_at(y)).flatten();
        let row = physical
            .map(|physical| {
                if physical < FIXED {
                    physical
                } else {
                    physical + self.scroll
                }
            })
            .filter(|&row| row < FIXED + entries.len());
        let on_close = row.is_some_and(|row| row >= FIXED && x >= f64::from(width - close_width()));
        let mut damaged = std::mem::replace(&mut self.hovered, row) != row;
        damaged |= std::mem::replace(&mut self.hovered_close, on_close) != on_close;
        if row != Some(CLEAR_ROW) && self.confirm {
            self.confirm = false;
            damaged = true;
        }
        let mut action = None;
        match event.kind {
            MouseEventKind::Button {
                button: BTN_LEFT,
                pressed: true,
            } => match row {
                Some(MODE_ROW) => {
                    action = mode_words()
                        .into_iter()
                        .find(|&(_, left, right)| (f64::from(left)..f64::from(right)).contains(&x))
                        .map(|(mode, ..)| HistoryAction::SetMode(mode));
                }
                Some(CLEAR_ROW) => {
                    damaged = true;
                    if std::mem::take(&mut self.confirm) {
                        action = Some(HistoryAction::Clear);
                    } else {
                        self.confirm = true;
                    }
                }
                Some(row) if row >= FIXED => {
                    let id = entries[row - FIXED].0;
                    action = Some(if on_close {
                        HistoryAction::Remove(id)
                    } else {
                        HistoryAction::Restore(id)
                    });
                }
                _ => {}
            },
            MouseEventKind::Scroll { delta, v120, .. } if inside => {
                let steps = self.scrolled.add(delta, v120) as isize;
                let scroll = self
                    .scroll
                    .saturating_add_signed(steps)
                    .min(ui::max_scroll(entries.len(), shown(height)));
                damaged |= std::mem::replace(&mut self.scroll, scroll) != scroll;
            }
            _ => {}
        }
        HistoryResponse {
            action,
            consumed: inside,
            damaged,
        }
    }

    /// The title, mode, Clear and as many entries as fit in a `[width, height]` box
    pub fn view(&mut self, settings: &Settings, [width, height]: [f32; 2]) -> UITree {
        let palette = &settings.palette;
        let fixed = Rows::new(0.0);
        let entries = rows(&history::entries());
        let shown = shown(height);
        self.scroll = self.scroll.min(ui::max_scroll(entries.len(), shown));
        let list = fixed.scrolled(self.scroll);
        let line = |layout: &Rows, index: usize, text: UIElement, background: Option<_>| {
            layout.row(
                index,
                width,
                background,
                vec![layout.text(text, PAD, width)],
            )
        };
        let hover = |index: usize| (self.hovered == Some(index)).then(|| ui::raised_bg(palette));

        let mut spans = vec![(MODE_LABEL.to_owned(), palette.inactive)];
        for (index, &(word, mode)) in MODES.iter().enumerate() {
            if index > 0 {
                spans.push((MODE_SEPARATOR.to_owned(), palette.inactive));
            }
            let current = settings::get().history.mode == mode;
            spans.push((
                word.to_owned(),
                if current { palette.accent } else { palette.fg },
            ));
        }
        let clear = if self.confirm {
            ui::text(CLEAR_ASK, settings::ERROR)
        } else {
            ui::text("Clear history", palette.fg)
        };
        let mut children = vec![
            line(&fixed, 0, ui::text("History", palette.accent), None),
            line(&fixed, MODE_ROW, ui::spans(spans), None).with_pointer(),
            line(&fixed, CLEAR_ROW, clear, hover(CLEAR_ROW)).with_pointer(),
        ];
        for (index, (_, text)) in entries.iter().enumerate().skip(self.scroll).take(shown) {
            let row = FIXED + index;
            let hovered = self.hovered == Some(row);
            let mut parts = vec![list.text(
                ui::text(text, palette.fg),
                PAD,
                width - if hovered { close_width() } else { 0.0 },
            )];
            if hovered {
                let color = if self.hovered_close {
                    palette.fg
                } else {
                    palette.inactive
                };
                let left = width - close_width() + (close_width() - font_size()) / 2.0;
                parts.push(
                    list.text(
                        ui::text("×", with_opacity(color)),
                        left,
                        left + close_width(),
                    )
                    .with_pointer(),
                );
            }
            children.push(list.row(row, width, hover(row), parts).with_pointer());
        }
        children.extend(ui::scrollbar(
            palette,
            ui::scrollbar_x(width),
            [FIXED as f32 * row_height(), shown as f32 * row_height()],
            self.scroll,
            shown,
            entries.len(),
        ));
        UITree::new_styled(
            UIElement::Backgrounded(ui::panel_bg(palette)),
            Style::full_size(),
            children,
        )
    }
}
