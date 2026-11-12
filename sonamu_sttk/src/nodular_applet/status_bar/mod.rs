//! The status bar (#150): a row at the top of every screen, drawn by the root, with popups under its segments.
//! See `.claude/STATUS_BAR.md`.

mod calendar;
mod resources;
mod system;

use super::treeview::{BTN_LEFT, guide};
use crate::{command_hub::bar_bg, ui};
use chrono::{Local, NaiveDate};
use sonamu_common::settings::{Palette, Settings};
use sonamu_ui::{
    color::Color,
    ui_element::{Style, UIElement, UITree, font_size},
    ui_event::{MouseEvent, MouseEventKind},
};
use system::System;

/// Room between segments and at the bar's ends
const GAP: f32 = 12.0;
/// Room inside a popup
const POPUP_PAD: f32 = 12.0;

/// The bar's height (px), taken from the top of every screen
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a row's height"
)]
pub fn height() -> u32 {
    ui::row_height().ceil() as u32
}

/// A character's advance in the monospace font (0.6 em, eg Noto Sans Mono's)
fn char_width() -> f32 {
    font_size() * 0.6
}

/// Character cells `text` takes: Nerd Font icons (private use areas, from the symbols font) are about 1.5 wide, so 2
fn columns(text: &str) -> usize {
    text.chars()
        .map(|c| match u32::from(c) {
            0xe000..=0xf8ff | 0xf0000.. => 2,
            _ => 1,
        })
        .sum()
}

/// The width of `text` in the monospace font (at least)
#[expect(clippy::cast_precision_loss, reason = "short text")]
fn text_width(text: &str) -> f32 {
    columns(text) as f32 * char_width()
}

/// Runs of text, each in its color
type Spans = Vec<(String, Color)>;

fn spans_width(spans: &Spans) -> f32 {
    spans.iter().map(|(text, _)| text_width(text)).sum()
}

/// What a segment of the bar is
#[derive(Debug, Clone, Copy, PartialEq)]
enum Segment {
    /// `▾`/`▴`: shows or hides the treeview
    Tree,
    /// The date and time; opens the calendar
    Time,
    /// Battery, cpu and memory; opens the resources popup
    Resources,
}

/// What a click on the bar does, for the root
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BarAction {
    ToggleTreeview,
}

/// What the root tells the bar
pub struct BarState {
    pub treeview_shown: bool,
}

/// A segment's popup
#[derive(Debug, Clone, Copy, PartialEq)]
enum Popup {
    /// Showing the month of this date's
    Calendar(NaiveDate),
    Resources(resources::Sort),
}
impl Popup {
    fn segment(self) -> Segment {
        match self {
            Self::Calendar(_) => Segment::Time,
            Self::Resources(_) => Segment::Resources,
        }
    }
}

/// What a mouse event did to the bar
#[derive(Default)]
pub struct BarResponse {
    /// The bar or a popup took it: the panes don't get it
    pub consumed: bool,
    pub damaged: bool,
    pub action: Option<BarAction>,
}

/// A placed segment: its text and `[left, width]`
struct Placed {
    segment: Segment,
    spans: Spans,
    span: [f32; 2],
}

/// Lines of monospace text in a popup, `compact_row_height()` apart, inside `POPUP_PAD`; columns are characters
struct Lines {
    rows: ui::Rows,
}
impl Lines {
    fn new() -> Self {
        Self {
            rows: ui::Rows::with_height(POPUP_PAD, ui::compact_row_height()),
        }
    }

    /// `spans` on line `index` from `column`
    #[expect(clippy::cast_precision_loss, reason = "columns")]
    fn text(&self, index: usize, column: usize, spans: Spans) -> UITree {
        let left = POPUP_PAD + column as f32 * char_width();
        let width = spans_width(&spans);
        self.rows.row(
            index,
            left + width,
            None,
            vec![self.rows.text(ui::spans(spans), left, left + width)],
        )
    }

    /// A background behind line `index`'s `columns`
    #[expect(clippy::cast_precision_loss, reason = "columns")]
    fn highlight(&self, index: usize, [from, to]: [usize; 2], color: Color) -> UITree {
        let left = POPUP_PAD + from as f32 * char_width();
        let width = (to - from) as f32 * char_width();
        UITree::leaf_styled(
            UIElement::Backgrounded(color),
            self.rows.style(index, left - 4.0, width + 8.0),
        )
    }

    /// The popup's size for `lines` lines `columns` wide
    #[expect(clippy::cast_precision_loss, reason = "columns")]
    fn size(lines: usize, columns: usize) -> [f32; 2] {
        [
            columns as f32 * char_width() + 2.0 * POPUP_PAD,
            lines as f32 * ui::compact_row_height() + 2.0 * POPUP_PAD,
        ]
    }

    /// The line and column at `[x, y]` in the popup
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "checked non-negative"
    )]
    fn at(&self, [x, y]: [f64; 2]) -> Option<(usize, usize)> {
        let x = x - f64::from(POPUP_PAD);
        let line = self.rows.row_at(y)?;
        (x >= 0.0).then(|| (line, (x / f64::from(char_width())) as usize))
    }
}

/// The bar's own state (one bar, drawn on every screen; popups only on the active one)
pub struct StatusBar {
    popup: Option<Popup>,
    system: System,
}

impl StatusBar {
    /// Starts reading the system; `changed` is called (on another thread) when there's something new to show
    pub fn new(changed: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            popup: None,
            system: System::start(changed),
        }
    }

    /// The time as shown, eg `2026-12-25 12:00:00PM`
    fn time() -> String {
        Local::now().format("%Y-%m-%d %I:%M:%S%p").to_string()
    }

    /// The segments with their text and place, for both drawing and clicking
    fn layout(&self, state: &BarState, palette: &Palette, width: f32) -> Vec<Placed> {
        let fg = palette.fg;
        let left = [
            (
                Segment::Tree,
                vec![(if state.treeview_shown { "▾" } else { "▴" }.to_string(), fg)],
            ),
            (Segment::Time, vec![(Self::time(), fg)]),
        ];
        let right = [(
            Segment::Resources,
            resources::segment(&self.system.snapshot.lock().unwrap(), palette),
        )];
        let mut placed = Vec::new();
        let mut x = GAP;
        for (segment, spans) in left {
            let w = spans_width(&spans);
            placed.push(Placed {
                segment,
                spans,
                span: [x, w],
            });
            x += w + GAP;
        }
        let mut x = width - GAP;
        for (segment, spans) in right.into_iter().rev() {
            let w = spans_width(&spans);
            x -= w;
            placed.push(Placed {
                segment,
                spans,
                span: [x, w],
            });
            x -= GAP;
        }
        placed
    }

    /// The bar, `width` wide, at the top of a screen, and on the active screen the open popup
    pub fn draw(
        &self,
        settings: &Settings,
        state: &BarState,
        width: f32,
        active: bool,
    ) -> Vec<UITree> {
        let height = height() as f32;
        let mut children = vec![ui::rect(
            bar_bg(&settings.palette),
            [0.0, 0.0],
            [width, height],
        )];
        let rows = ui::Rows::with_height(0.0, height);
        let placed = self.layout(state, &settings.palette, width);
        for Placed {
            spans,
            span: [left, w],
            ..
        } in &placed
        {
            children.push(
                rows.text(ui::spans(spans.clone()), *left, left + w)
                    .with_pointer(),
            );
        }
        let mut trees = vec![UITree::new_container_styled(
            Style::absolute([0.0, 0.0], [width, height]),
            children,
        )];
        if active && let Some(popup) = self.popup {
            trees.push(self.draw_popup(settings, popup, &placed, width));
        }
        trees
    }

    fn popup_items(&self, settings: &Settings, popup: Popup) -> Vec<UITree> {
        match popup {
            Popup::Calendar(month) => calendar::draw(settings, month),
            Popup::Resources(sort) => {
                resources::draw(settings, &self.system.snapshot.lock().unwrap(), sort)
            }
        }
    }

    /// The popup's `[x, y, width, height]` on a screen `width` wide: under its segment
    fn popup_area(&self, popup: Popup, placed: &[Placed], width: f32) -> [f32; 4] {
        let [w, h] = match popup {
            Popup::Calendar(_) => calendar::size(),
            Popup::Resources(_) => resources::size(&self.system.snapshot.lock().unwrap()),
        };
        let [left, segment_width] = placed
            .iter()
            .find(|placed| placed.segment == popup.segment())
            .map_or([0.0; 2], |placed| placed.span);
        // left segments' popups start at their left, right ones' end at their right
        let x = if left > width / 2.0 {
            left + segment_width - w
        } else {
            left
        };
        [x.min(width - w).max(0.0), height() as f32, w, h]
    }

    fn draw_popup(
        &self,
        settings: &Settings,
        popup: Popup,
        placed: &[Placed],
        width: f32,
    ) -> UITree {
        let [x, y, w, h] = self.popup_area(popup, placed, width);
        let items = self.popup_items(settings, popup);
        UITree::new_styled(
            UIElement::Backgrounded(popup_bg(settings)),
            Style::absolute([x, y], [w, h]),
            vec![ui::frame(guide(), 0.0, 1.0, Style::full_size(), items)],
        )
    }

    fn set_popup(&mut self, popup: Option<Popup>) {
        self.popup = popup;
        self.system
            .set_want_processes(matches!(popup, Some(Popup::Resources(_))));
    }

    /// A mouse event (in the screen's coords, `width` wide); the root passes it on unless `consumed`
    pub fn handle_mouse(
        &mut self,
        state: &BarState,
        palette: &Palette,
        event: &MouseEvent,
        width: f32,
    ) -> BarResponse {
        let [x, y] = event.position;
        let placed = self.layout(state, palette, width);
        let on_bar = y < f64::from(height());
        let pressed = matches!(
            event.kind,
            MouseEventKind::Button {
                button: BTN_LEFT,
                pressed: true,
            }
        );
        let was = self.popup;
        if let Some(popup) = self.popup {
            let [px, py, pw, ph] = self.popup_area(popup, &placed, width).map(f64::from);
            if (px..px + pw).contains(&x) && (py..py + ph).contains(&y) {
                let damaged = pressed && self.click_popup(popup, [x - px, y - py]);
                return BarResponse {
                    consumed: true,
                    damaged,
                    action: None,
                };
            }
            // a click elsewhere only closes it (on the bar it may open another)
            if pressed {
                self.set_popup(None);
                if !on_bar {
                    return BarResponse {
                        consumed: true,
                        damaged: true,
                        action: None,
                    };
                }
            }
        }
        if !on_bar {
            return BarResponse::default();
        }
        let mut response = BarResponse {
            consumed: true,
            damaged: pressed,
            action: None,
        };
        if !pressed {
            return response;
        }
        // half the gap around each segment is its too
        let Some(segment) = placed
            .iter()
            .find(|placed| {
                let [left, w] = placed.span;
                (f64::from(left - GAP / 2.0)..f64::from(left + w + GAP / 2.0)).contains(&x)
            })
            .map(|placed| placed.segment)
        else {
            return response;
        };
        // clicking a segment again closes its popup
        let open = |popup: Popup| (was.map(Popup::segment) != Some(segment)).then_some(popup);
        match segment {
            Segment::Tree => response.action = Some(BarAction::ToggleTreeview),
            Segment::Time => self.set_popup(open(Popup::Calendar(Local::now().date_naive()))),
            Segment::Resources => {
                self.set_popup(open(Popup::Resources(resources::Sort::default())));
            }
        }
        response
    }

    /// A click at `[x, y]` in the popup; whether it changed
    fn click_popup(&mut self, popup: Popup, position: [f64; 2]) -> bool {
        let Some((line, column)) = Lines::new().at(position) else {
            return false;
        };
        let popup = match popup {
            Popup::Calendar(month) => calendar::click(month, line, column).map(Popup::Calendar),
            Popup::Resources(sort) => {
                // not matched on directly: the lock would be held through `set_profile`'s
                let click =
                    resources::click(&self.system.snapshot.lock().unwrap(), sort, line, column);
                match click {
                    Some(resources::Click::Sort(sort)) => Some(Popup::Resources(sort)),
                    Some(resources::Click::Profile(profile)) => {
                        self.system.set_profile(profile);
                        return true;
                    }
                    None => None,
                }
            }
        };
        if let Some(popup) = popup {
            self.set_popup(Some(popup));
        }
        popup.is_some()
    }

    /// Whether the shown second changed since `last` (and the new one), for the root's clock timer
    pub fn tick(last: &mut String) -> bool {
        let now = Self::time();
        let changed = now != *last;
        *last = now;
        changed
    }
}

/// Opaque: over the panes, even a little translucency lets their text show through (blending is linear)
fn popup_bg(settings: &Settings) -> Color {
    let Color([r, g, b, _]) = settings.palette.raised(10);
    Color([r, g, b, 0xFF])
}
