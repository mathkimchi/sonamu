//! The status bar (#150): a row at the top of every screen, drawn by the root, with popups under its segments.
//! See `.claude/STATUS_BAR.md`.

mod calendar;
mod desktop;
mod icons;
mod launcher;
mod notification_list;
mod notifications;
mod resources;
mod system;

use super::treeview::{BTN_LEFT, guide};
use crate::{command_hub::bar_bg, ui};
use chrono::{Local, NaiveDate};
use icons::Icons;
use launcher::Launcher;
use notifications::{CloseReason, Notifications};
use sonamu_common::settings::{Palette, Settings};
use sonamu_ui::{
    color::Color,
    ui_element::{Image, Style, UIElement, UITree, font_size},
    ui_event::{KeySymbol, MouseEvent, MouseEventKind},
};
use std::sync::Arc;
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
    /// Opens the app launcher
    Launcher,
    /// The date and time; opens the calendar
    Time,
    /// Battery, cpu and memory; opens the resources popup
    Resources,
    /// The notifications' bell, with their count; opens their list
    Bell,
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
    Notifications,
    /// `StatusBar::launcher`'s
    Launcher,
}
impl Popup {
    fn segment(self) -> Segment {
        match self {
            Self::Calendar(_) => Segment::Time,
            Self::Resources(_) => Segment::Resources,
            Self::Notifications => Segment::Bell,
            Self::Launcher => Segment::Launcher,
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

    /// `image` as a square icon at line `index`'s `column` (2 columns wide)
    #[expect(clippy::cast_precision_loss, reason = "columns")]
    fn icon(&self, index: usize, column: usize, image: Image) -> UITree {
        let size = font_size() * 1.1;
        let left = POPUP_PAD + column as f32 * char_width();
        let top = self.rows.top_of(index) + (self.rows.height - size) / 2.0;
        UITree::leaf_styled(
            UIElement::Image(image),
            Style::absolute([left, top], [size, size]),
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
    notifications: Notifications,
    icons: Icons,
    /// While its popup is open
    launcher: Option<Launcher>,
    /// What `tick` last saw: the clock's text and the popping up notifications
    shown: (String, Vec<u32>),
}

/// Under the bar's right end: the notifications popping up, `GAP` apart
const POPUP_NOTIFICATION_GAP: f32 = 8.0;

impl StatusBar {
    /// Starts reading the system and serving notifications; `changed` is called (on other threads) when there's
    /// something new to show
    pub fn new(changed: impl Fn() + Send + Sync + 'static) -> Self {
        let changed: Arc<dyn Fn() + Send + Sync> = Arc::new(changed);
        let system_changed = Arc::clone(&changed);
        Self {
            popup: None,
            system: System::start(move || system_changed()),
            notifications: Notifications::start(Arc::clone(&changed)),
            icons: Icons::start(changed),
            launcher: None,
            shown: Default::default(),
        }
    }

    /// The time as shown, eg `2026-12-25 12:00:00PM`
    fn time() -> String {
        Local::now().format("%Y-%m-%d %I:%M:%S%p").to_string()
    }

    /// The bell, and room for its count's superscript
    fn bell(&self, palette: &Palette) -> Spans {
        let icon = if self.notifications.store.lock().unwrap().do_not_disturb {
            notification_list::BELL_OFF
        } else {
            notification_list::BELL
        };
        vec![(format!("{icon} "), palette.fg)]
    }

    /// The segments with their text and place, for both drawing and clicking
    fn layout(&self, state: &BarState, palette: &Palette, width: f32) -> Vec<Placed> {
        let fg = palette.fg;
        let left = [
            (
                Segment::Tree,
                vec![(if state.treeview_shown { "▾" } else { "▴" }.to_string(), fg)],
            ),
            (Segment::Launcher, vec![(launcher::ICON.to_string(), fg)]),
            (Segment::Time, vec![(Self::time(), fg)]),
        ];
        let right = [
            (Segment::Bell, self.bell(palette)),
            (
                Segment::Resources,
                resources::segment(&self.system.snapshot.lock().unwrap(), palette),
            ),
        ];
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

    /// The count over the bell's top right corner, small (`left` is the bell's)
    fn bell_count(&self, palette: &Palette, left: f32) -> Option<UITree> {
        let count = self.notifications.store.lock().unwrap().list.len();
        if count == 0 {
            return None;
        }
        let text = if count > 9 {
            "9+".to_string()
        } else {
            count.to_string()
        };
        let scale = 0.55;
        let [w, h] = [text_width(&text), font_size() * 1.3];
        Some(UITree::new_styled(
            UIElement::Scaled(scale),
            Style::absolute([left + 1.5 * char_width(), 2.0], [w * scale, h * scale]),
            vec![UITree::leaf_styled(
                ui::text(text, palette.accent),
                Style::absolute([0.0, 0.0], [w, h]),
            )],
        ))
    }

    /// The bar, `width` wide, at the top of a screen, and on the active screen the open popup and the
    /// notifications popping up
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
            segment,
            spans,
            span: [left, w],
        } in &placed
        {
            children.push(
                rows.text(ui::spans(spans.clone()), *left, left + w)
                    .with_pointer(),
            );
            if *segment == Segment::Bell {
                children.extend(self.bell_count(&settings.palette, *left));
            }
        }
        let mut trees = vec![UITree::new_container_styled(
            Style::absolute([0.0, 0.0], [width, height]),
            children,
        )];
        if active {
            for (area, items) in self.popup_notifications(&settings.palette, width) {
                trees.push(popup_frame(settings, area, items));
            }
            if let Some(popup) = self.popup {
                let area = self.popup_area(popup, &placed, width);
                trees.push(popup_frame(
                    settings,
                    area,
                    self.popup_items(settings, popup),
                ));
            }
        }
        trees
    }

    /// The notifications popping up: each one's `[x, y, width, height]` and items, top down
    fn popup_notifications(&self, palette: &Palette, width: f32) -> Vec<([f32; 4], Vec<UITree>)> {
        let store = self.notifications.store.lock().unwrap();
        let [w, _] = Lines::size(0, notification_list::COLUMNS);
        let mut y = height() as f32 + POPUP_NOTIFICATION_GAP;
        store
            .popups()
            .take(notification_list::POPUPS)
            .map(|notification| {
                let [_, h] = Lines::size(
                    notification_list::popup_line_count(notification, palette),
                    notification_list::COLUMNS,
                );
                let area = [(width - GAP - w).max(0.0), y, w, h];
                y += h + POPUP_NOTIFICATION_GAP;
                (
                    area,
                    notification_list::draw_popup(notification, palette, &self.icons),
                )
            })
            .collect()
    }

    fn popup_items(&self, settings: &Settings, popup: Popup) -> Vec<UITree> {
        match popup {
            Popup::Calendar(month) => calendar::draw(settings, month),
            Popup::Resources(sort) => {
                resources::draw(settings, &self.system.snapshot.lock().unwrap(), sort)
            }
            Popup::Notifications => notification_list::draw(
                settings,
                &self.notifications.store.lock().unwrap(),
                &self.icons,
            ),
            Popup::Launcher => self
                .launcher
                .as_ref()
                .map(|launcher| launcher.draw(settings, &self.icons))
                .unwrap_or_default(),
        }
    }

    /// The popup's `[x, y, width, height]` on a screen `width` wide: under its segment
    fn popup_area(&self, popup: Popup, placed: &[Placed], width: f32) -> [f32; 4] {
        let [w, h] = match popup {
            Popup::Calendar(_) => calendar::size(),
            Popup::Resources(_) => resources::size(&self.system.snapshot.lock().unwrap()),
            Popup::Notifications => {
                notification_list::size(&self.notifications.store.lock().unwrap())
            }
            Popup::Launcher => Launcher::size(),
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

    fn set_popup(&mut self, popup: Option<Popup>) {
        self.popup = popup;
        if popup != Some(Popup::Launcher) {
            self.launcher = None;
        } else if self.launcher.is_none() {
            self.launcher = Some(Launcher::open());
        }
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
        let inside = |[ax, ay, aw, ah]: [f32; 4]| {
            let [ax, ay, aw, ah] = [ax, ay, aw, ah].map(f64::from);
            (ax..ax + aw).contains(&x) && (ay..ay + ah).contains(&y)
        };
        let was = self.popup;
        if let Some(popup) = self.popup {
            let area = self.popup_area(popup, &placed, width);
            if inside(area) {
                let damaged = pressed
                    && self.click_popup(popup, [x - f64::from(area[0]), y - f64::from(area[1])]);
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
        if let Some(([ax, ay, ..], index)) = self
            .popup_notifications(palette, width)
            .into_iter()
            .enumerate()
            .find_map(|(index, (area, _))| inside(area).then_some((area, index)))
        {
            if pressed {
                let click = {
                    let store = self.notifications.store.lock().unwrap();
                    let notification = store.popups().nth(index);
                    let at = Lines::new().at([x - f64::from(ax), y - f64::from(ay)]);
                    notification
                        .zip(at)
                        .map(|(n, (line, column))| notification_list::click_popup(n, line, column))
                };
                if let Some(click) = click {
                    self.apply_notification_click(click);
                }
            }
            return BarResponse {
                consumed: true,
                damaged: pressed,
                action: None,
            };
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
            Segment::Bell => self.set_popup(open(Popup::Notifications)),
            Segment::Launcher => self.set_popup(open(Popup::Launcher)),
        }
        response
    }

    fn apply_notification_click(&self, click: notification_list::Click) {
        match click {
            notification_list::Click::Activate(id) => self.notifications.activate(id),
            notification_list::Click::Dismiss(id) => {
                self.notifications.close(id, CloseReason::Dismissed);
            }
            notification_list::Click::ClearAll => self.notifications.clear(),
            notification_list::Click::DoNotDisturb => self.notifications.toggle_do_not_disturb(),
        }
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
            Popup::Notifications => {
                let click = notification_list::click(
                    &self.notifications.store.lock().unwrap(),
                    line,
                    column,
                );
                let Some(click) = click else {
                    return false;
                };
                self.apply_notification_click(click);
                return true;
            }
            Popup::Launcher => {
                let launched = self
                    .launcher
                    .as_ref()
                    .is_some_and(|launcher| launcher.click(line));
                if launched {
                    self.set_popup(None);
                }
                return launched;
            }
        };
        if let Some(popup) = popup {
            self.set_popup(Some(popup));
        }
        popup.is_some()
    }

    /// Whether keys go to the bar (the launcher's search), not the focused applet
    pub fn wants_keys(&self) -> bool {
        self.launcher.is_some()
    }

    /// A pressed key while `wants_keys`; whether it changed anything
    pub fn handle_key(&mut self, key: KeySymbol) -> bool {
        let Some(launcher) = &mut self.launcher else {
            return false;
        };
        match launcher.key(key) {
            launcher::KeyResult::Changed => true,
            launcher::KeyResult::Close => {
                self.set_popup(None);
                true
            }
            launcher::KeyResult::Ignored => false,
        }
    }

    /// Whether what changes by itself (the clock's second, which notifications pop up) changed since the last
    /// call, for the root's timer
    pub fn tick(&mut self) -> bool {
        let popups = self
            .notifications
            .store
            .lock()
            .unwrap()
            .popups()
            .map(|n| n.id)
            .collect();
        let shown = (Self::time(), popups);
        let changed = shown != self.shown;
        self.shown = shown;
        changed
    }
}

/// A popup's background and frame at `[x, y, width, height]` around `items`
fn popup_frame(settings: &Settings, [x, y, w, h]: [f32; 4], items: Vec<UITree>) -> UITree {
    UITree::new_styled(
        UIElement::Backgrounded(popup_bg(settings)),
        Style::absolute([x, y], [w, h]),
        vec![ui::frame(guide(), 0.0, 1.0, Style::full_size(), items)],
    )
}

/// Opaque: over the panes, even a little translucency lets their text show through (blending is linear)
fn popup_bg(settings: &Settings) -> Color {
    let Color([r, g, b, _]) = settings.palette.raised(10);
    Color([r, g, b, 0xFF])
}
