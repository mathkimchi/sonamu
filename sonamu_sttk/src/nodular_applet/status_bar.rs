//! The status bar (#150): a row at the top of every screen, drawn by the root, with popups under its segments.
//! See `.claude/STATUS_BAR.md`.

use super::treeview::{BTN_LEFT, guide};
use crate::{command_hub::bar_bg, ui};
use chrono::{Datelike, Local, Months, NaiveDate};
use sonamu_common::settings::Settings;
use sonamu_ui::{
    color::Color,
    ui_element::{Style, UIElement, UITree, font_size},
    ui_event::{MouseEvent, MouseEventKind},
};

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

/// The width of `text` in the monospace font
#[expect(clippy::cast_precision_loss, reason = "short text")]
fn text_width(text: &str) -> f32 {
    text.chars().count() as f32 * char_width()
}

/// What a segment of the bar is
#[derive(Debug, Clone, Copy, PartialEq)]
enum Segment {
    /// `▾`/`▴`: shows or hides the treeview
    Tree,
    /// The date and time; opens the calendar
    Time,
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
}

/// What a mouse event did to the bar
#[derive(Default)]
pub struct BarResponse {
    /// The bar or a popup took it: the panes don't get it
    pub consumed: bool,
    pub damaged: bool,
    pub action: Option<BarAction>,
}

/// The bar's own state (one bar, drawn on every screen; popups only on the active one)
#[derive(Default)]
pub struct StatusBar {
    popup: Option<Popup>,
}

/// A placed segment: its text and `[left, width]`
struct Placed {
    segment: Segment,
    text: String,
    span: [f32; 2],
}

impl StatusBar {
    /// The time as shown, eg `2026-12-25 12:00:00PM`
    fn time() -> String {
        Local::now().format("%Y-%m-%d %I:%M:%S%p").to_string()
    }

    /// The segments with their text and place, for both drawing and clicking
    fn layout(state: &BarState, width: f32) -> Vec<Placed> {
        let left = [
            (
                Segment::Tree,
                if state.treeview_shown { "▾" } else { "▴" }.to_string(),
            ),
            (Segment::Time, Self::time()),
        ];
        let right: [(Segment, String); 0] = [];
        let mut placed = Vec::new();
        let mut x = GAP;
        for (segment, text) in left {
            let w = text_width(&text);
            placed.push(Placed {
                segment,
                text,
                span: [x, w],
            });
            x += w + GAP;
        }
        let mut x = width - GAP;
        for (segment, text) in right.into_iter().rev() {
            let w = text_width(&text);
            x -= w;
            placed.push(Placed {
                segment,
                text,
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
        let placed = Self::layout(state, width);
        for Placed {
            text,
            span: [left, w],
            ..
        } in &placed
        {
            children.push(
                rows.text(ui::text(text.clone(), settings.palette.fg), *left, left + w)
                    .with_pointer(),
            );
        }
        let mut trees = vec![UITree::new_container_styled(
            Style::absolute([0.0, 0.0], [width, height]),
            children,
        )];
        if active && let Some(popup) = self.popup {
            trees.push(Self::draw_popup(settings, popup, &placed, width));
        }
        trees
    }

    /// The popup's segment and the popup's `[x, y, width, height]` on a screen `width` wide
    fn popup_area(popup: Popup, placed: &[Placed], width: f32) -> [f32; 4] {
        let segment = match popup {
            Popup::Calendar(_) => Segment::Time,
        };
        let [w, h] = match popup {
            Popup::Calendar(_) => [
                text_width(CALENDAR_HEADER) + 2.0 * POPUP_PAD,
                CALENDAR_ROWS as f32 * ui::compact_row_height() + 2.0 * POPUP_PAD,
            ],
        };
        let [left, _] = placed
            .iter()
            .find(|placed| placed.segment == segment)
            .map_or([0.0; 2], |placed| placed.span);
        // under its segment, in the screen
        let x = left.min(width - w).max(0.0);
        [x, height() as f32, w, h]
    }

    fn draw_popup(settings: &Settings, popup: Popup, placed: &[Placed], width: f32) -> UITree {
        let [x, y, w, h] = Self::popup_area(popup, placed, width);
        let items = match popup {
            Popup::Calendar(month) => calendar(settings, month),
        };
        UITree::new_styled(
            UIElement::Backgrounded(popup_bg(settings)),
            Style::absolute([x, y], [w, h]),
            vec![ui::frame(guide(), 0.0, 1.0, Style::full_size(), items)],
        )
    }

    /// A mouse event (in the screen's coords, `width` wide); the root passes it on unless `consumed`
    pub fn handle_mouse(
        &mut self,
        state: &BarState,
        event: &MouseEvent,
        width: f32,
    ) -> BarResponse {
        let [x, y] = event.position;
        let placed = Self::layout(state, width);
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
            let [px, py, pw, ph] = Self::popup_area(popup, &placed, width).map(f64::from);
            if (px..px + pw).contains(&x) && (py..py + ph).contains(&y) {
                let damaged = pressed
                    && self.click_popup(
                        popup,
                        [x - px - f64::from(POPUP_PAD), y - py - f64::from(POPUP_PAD)],
                    );
                return BarResponse {
                    consumed: true,
                    damaged,
                    action: None,
                };
            }
            // a click elsewhere only closes it (on the bar it may open another)
            if pressed {
                self.popup = None;
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
        self.popup = None;
        match segment {
            Segment::Tree => response.action = Some(BarAction::ToggleTreeview),
            Segment::Time => {
                if !matches!(was, Some(Popup::Calendar(_))) {
                    self.popup = Some(Popup::Calendar(Local::now().date_naive()));
                }
            }
        }
        response
    }

    /// A click at `[x, y]` inside the popup's padding; whether it changed
    fn click_popup(&mut self, popup: Popup, [x, y]: [f64; 2]) -> bool {
        match popup {
            Popup::Calendar(month) => {
                let header = f64::from(ui::compact_row_height());
                if !(0.0..header).contains(&y) {
                    return false;
                }
                let arrow = f64::from(3.0 * char_width());
                let total = f64::from(text_width(CALENDAR_HEADER));
                let month = if x < arrow {
                    month.checked_sub_months(Months::new(1))
                } else if x > total - arrow {
                    month.checked_add_months(Months::new(1))
                } else {
                    None
                };
                if let Some(month) = month {
                    self.popup = Some(Popup::Calendar(month));
                }
                month.is_some()
            }
        }
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

/// The calendar's widest line, which sets its width
const CALENDAR_HEADER: &str = "Mo Tu We Th Fr Sa Su";
/// Its rows: month, weekdays, up to 6 weeks
const CALENDAR_ROWS: usize = 8;

/// The month of `date`'s, today marked (a placeholder for a better calendar)
fn calendar(settings: &Settings, date: NaiveDate) -> Vec<UITree> {
    let palette = &settings.palette;
    let rows = ui::Rows::with_height(POPUP_PAD, ui::compact_row_height());
    let width = text_width(CALENDAR_HEADER);
    let today = Local::now().date_naive();
    let first = date.with_day(1).unwrap_or(date);
    let title = first.format("%B %Y").to_string();
    // ‹ title › across the width
    let inner = CALENDAR_HEADER.chars().count();
    let title_pad = inner.saturating_sub(title.chars().count() + 2);
    let header = format!(
        "‹{}{title}{}›",
        " ".repeat(title_pad / 2),
        " ".repeat(title_pad - title_pad / 2)
    );
    let mut lines: Vec<Vec<(String, Color)>> = vec![
        vec![(header, palette.fg)],
        vec![(CALENDAR_HEADER.to_string(), palette.inactive)],
    ];
    let mut week = vec![(
        "   ".repeat(first.weekday().num_days_from_monday() as usize),
        palette.fg,
    )];
    let mut day = first;
    while day.month() == first.month() {
        let color = if day == today {
            palette.accent
        } else {
            palette.fg
        };
        week.push((format!("{:>2} ", day.day()), color));
        if day.weekday().num_days_from_monday() == 6 {
            lines.push(std::mem::take(&mut week));
        }
        let Some(next) = day.succ_opt() else { break };
        day = next;
    }
    if !week.is_empty() {
        lines.push(week);
    }
    lines
        .into_iter()
        .enumerate()
        .map(|(index, spans)| {
            rows.row(
                index,
                POPUP_PAD + width,
                None,
                vec![rows.text(ui::spans(spans), POPUP_PAD, POPUP_PAD + width)],
            )
        })
        .collect()
}
