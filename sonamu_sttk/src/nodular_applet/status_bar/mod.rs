//! The status bar (#150): a row at the top of every screen, drawn by the root, with popups under its segments.
//! See `.claude/STATUS_BAR.md`.

mod calendar;
mod controls;
mod desktop;
mod hub;
mod icons;
mod launcher;
mod notification_list;
mod notifications;
mod resources;
mod system;
mod tray;

use super::treeview::{BTN_LEFT, BTN_RIGHT, guide};
use crate::{command_hub::bar_bg, ui};
use chrono::{Local, NaiveDate};
use controls::Controls;
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
use tray::Tray;

/// The middle mouse button (evdev)
const BTN_MIDDLE: u32 = 0x112;

/// Room between segments and at the bar's ends
const GAP: f32 = 12.0;
/// Room inside a popup
const POPUP_PAD: f32 = 12.0;
/// Touchpad scrolling (px) per step (eg 5% volume); a wheel notch is one
const SCROLL_STEP: f64 = 30.0;

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

fn columns_of(spans: &Spans) -> usize {
    spans.iter().map(|(text, _)| columns(text)).sum()
}

fn spans_width(spans: &Spans) -> f32 {
    spans.iter().map(|(text, _)| text_width(text)).sum()
}

/// What a segment of the bar is
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    /// `▾`/`▴`: shows or hides the treeview
    Tree,
    /// Opens the app launcher
    Launcher,
    /// The date and time; opens the calendar
    Time,
    /// Battery, cpu and memory; opens the resources popup
    Resources,
    /// Wifi, bluetooth, volume and brightness; opens the hub
    Hub,
    /// The tray's icons (`Tray::items`, passive ones left out)
    Tray,
    /// The notifications' bell, with their count; opens their list
    Bell,
}

impl Segment {
    /// Its name in `sonamu.bar_layout`
    const NAMES: [(&str, Self); 7] = [
        ("tree", Self::Tree),
        ("launcher", Self::Launcher),
        ("time", Self::Time),
        ("tray", Self::Tray),
        ("bell", Self::Bell),
        ("resources", Self::Resources),
        ("hub", Self::Hub),
    ];

    fn named(name: &str) -> Result<Self, String> {
        Self::NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, segment)| *segment)
            .ok_or_else(|| {
                let names: Vec<&str> = Self::NAMES.iter().map(|(name, _)| *name).collect();
                format!("no bar segment {name} (there are {})", names.join(", "))
            })
    }
}

/// The default layout: `[left, right]`, each left to right
const DEFAULT_LAYOUT: [&[Segment]; 2] = [
    &[Segment::Tree, Segment::Launcher, Segment::Time],
    &[
        Segment::Tray,
        Segment::Bell,
        Segment::Resources,
        Segment::Hub,
    ],
];

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
    /// With this section's list shown
    Hub(Option<hub::Section>),
    /// A tray item's menu (`Tray::menu`, read meanwhile)
    TrayMenu,
    Notifications,
    /// `StatusBar::launcher`'s
    Launcher,
}
impl Popup {
    fn segment(self) -> Segment {
        match self {
            Self::Calendar(_) => Segment::Time,
            Self::Resources(_) => Segment::Resources,
            Self::Hub(_) => Segment::Hub,
            Self::TrayMenu => Segment::Tray,
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
    shown: (String, Vec<u32>, bool),
    controls: Controls,
    tray: Tray,
    /// `[left, right]`, each left to right (`sonamu.bar_layout`)
    layout: [Vec<Segment>; 2],
    /// Scrolling (touchpad px) not yet a step
    scrolled: f64,
    /// Under the pointer
    hovered: Option<Segment>,
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
            controls: Controls::start(Arc::clone(&changed)),
            tray: Tray::start(Arc::clone(&changed)),
            layout: DEFAULT_LAYOUT.map(<[Segment]>::to_vec),
            scrolled: 0.0,
            hovered: None,
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
        // empty ones (eg a tray without items) take no room
        let spans = |segments: &[Segment]| -> Vec<(Segment, Spans)> {
            segments
                .iter()
                .map(|&segment| {
                    let spans = match segment {
                        Segment::Tree => {
                            vec![(if state.treeview_shown { "▾" } else { "▴" }.to_string(), fg)]
                        }
                        Segment::Launcher => vec![(launcher::ICON.to_string(), fg)],
                        Segment::Time => vec![(Self::time(), fg)],
                        Segment::Tray => self.tray_spans(palette),
                        Segment::Bell => self.bell(palette),
                        Segment::Resources => {
                            resources::segment(&self.system.snapshot.lock().unwrap(), palette)
                        }
                        Segment::Hub => hub::segment(&self.controls.state.lock().unwrap(), palette),
                    };
                    (segment, spans)
                })
                .filter(|(_, spans)| !spans.is_empty())
                .collect()
        };
        let [left, right] = [spans(&self.layout[0]), spans(&self.layout[1])];
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

    /// The tray's shown items
    fn tray_items(&self) -> Vec<tray::Item> {
        let items = self.tray.items.lock().unwrap();
        items.iter().filter(|item| !item.passive).cloned().collect()
    }

    /// Room for the tray's icons: 2 columns each, 1 between
    fn tray_spans(&self, palette: &Palette) -> Spans {
        let count = self.tray_items().len();
        if count == 0 {
            return Vec::new();
        }
        vec![("   ".repeat(count - 1) + "  ", palette.fg)]
    }

    /// The tray's icons from `left`: its pixmap, else its icon by name
    #[expect(clippy::cast_precision_loss, reason = "a few icons")]
    fn tray_icons(&self, left: f32) -> Vec<UITree> {
        let size = font_size() * 1.1;
        let top = (height() as f32 - size) / 2.0;
        self.tray_items()
            .into_iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let image = item.pixmap.or_else(|| self.icons.get(&item.icon_name))?;
                let x = left + index as f32 * 3.0 * char_width();
                Some(
                    UITree::leaf_styled(
                        UIElement::Image(image),
                        Style::absolute([x, top], [size, size]),
                    )
                    .with_pointer(),
                )
            })
            .collect()
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
            // the hovered one, or the one whose popup is open, on a lighter background
            let lit = active
                && (self.hovered == Some(*segment)
                    || self.popup.map(Popup::segment) == Some(*segment));
            if lit {
                children.push(ui::rect(
                    settings.palette.raised(30),
                    [left - GAP / 2.0 + 2.0, 4.0],
                    [w + GAP - 4.0, height - 8.0],
                ));
            }
            children.push(
                rows.text(ui::spans(spans.clone()), *left, left + w)
                    .with_pointer(),
            );
            if *segment == Segment::Bell {
                children.extend(self.bell_count(&settings.palette, *left));
            }
            if *segment == Segment::Tray {
                children.extend(self.tray_icons(*left));
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
            // not over the hub, which shows the same
            let osd = hub::osd(&self.controls.state.lock().unwrap(), &settings.palette)
                .filter(|_| !matches!(self.popup, Some(Popup::Hub(_))));
            if let Some(spans) = osd {
                // in the middle, under the bar
                let [w, h] = Lines::size(1, columns_of(&spans));
                let area = [
                    ((width - w) / 2.0).max(0.0),
                    height + POPUP_NOTIFICATION_GAP,
                    w,
                    h,
                ];
                trees.push(popup_frame(
                    settings,
                    area,
                    vec![Lines::new().text(0, 0, spans)],
                ));
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
            Popup::Hub(open) => hub::draw(settings, &self.controls.state.lock().unwrap(), open),
            Popup::TrayMenu => tray::draw_menu(settings, self.tray.menu.lock().unwrap().as_ref()),
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
            Popup::Hub(open) => hub::size(&self.controls.state.lock().unwrap(), open),
            Popup::TrayMenu => tray::menu_size(self.tray.menu.lock().unwrap().as_ref()),
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
        self.controls
            .set_want_lists(matches!(popup, Some(Popup::Hub(_))));
        if popup != Some(Popup::TrayMenu) {
            self.tray.close_menu();
        }
    }

    /// A mouse event (in the screen's coords, `width` wide); the root passes it on unless `consumed`
    pub fn handle_mouse(
        &mut self,
        state: &BarState,
        palette: &Palette,
        event: &MouseEvent,
        width: f32,
    ) -> BarResponse {
        let placed = self.layout(state, palette, width);
        let [x, y] = event.position;
        let hovered = (y < f64::from(height()))
            .then(|| segment_at(&placed, x).map(|(segment, _)| segment))
            .flatten();
        let hover_changed = hovered != self.hovered;
        self.hovered = hovered;
        let mut response = self.mouse(&placed, palette, event, width);
        response.damaged |= hover_changed;
        response
    }

    fn mouse(
        &mut self,
        placed: &[Placed],
        palette: &Palette,
        event: &MouseEvent,
        width: f32,
    ) -> BarResponse {
        let [x, y] = event.position;
        let on_bar = y < f64::from(height());
        if let MouseEventKind::Scroll { delta, v120, .. } = event.kind {
            return self.scroll(placed, [x, y], width, delta, v120);
        }
        let button = match event.kind {
            MouseEventKind::Button {
                button,
                pressed: true,
            } => Some(button),
            _ => None,
        };
        let pressed = button == Some(BTN_LEFT);
        let inside = |[ax, ay, aw, ah]: [f32; 4]| {
            let [ax, ay, aw, ah] = [ax, ay, aw, ah].map(f64::from);
            (ax..ax + aw).contains(&x) && (ay..ay + ah).contains(&y)
        };
        let was = self.popup;
        if let Some(popup) = self.popup {
            let area = self.popup_area(popup, placed, width);
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
        if let Some(response) = self.mouse_on_popup_notification(palette, [x, y], width, pressed) {
            return response;
        }
        if !on_bar {
            return BarResponse::default();
        }
        let mut response = BarResponse {
            consumed: true,
            damaged: button.is_some(),
            action: None,
        };
        let Some(button) = button else {
            return response;
        };
        let Some((segment, [left, _])) = segment_at(placed, x) else {
            return response;
        };
        if segment == Segment::Tray {
            self.click_tray(button, x - f64::from(left));
            return response;
        }
        // other buttons only on the tray
        if button != BTN_LEFT {
            return response;
        }
        // clicking a segment again closes its popup
        let open = |popup: Popup| (was.map(Popup::segment) != Some(segment)).then_some(popup);
        match segment {
            Segment::Tree => response.action = Some(BarAction::ToggleTreeview),
            Segment::Time => self.set_popup(open(Popup::Calendar(Local::now().date_naive()))),
            Segment::Resources => {
                self.set_popup(open(Popup::Resources(resources::Sort::default())));
            }
            Segment::Bell => self.set_popup(open(Popup::Notifications)),
            Segment::Hub => self.set_popup(open(Popup::Hub(None))),
            Segment::Launcher => self.set_popup(open(Popup::Launcher)),
            // handled above
            Segment::Tray => {}
        }
        response
    }

    /// A scroll at `[x, y]`: over the hub's icons it changes the volume, over the calendar its month; any scroll over
    /// the bar or a popup is the bar's
    fn scroll(
        &mut self,
        placed: &[Placed],
        [x, y]: [f64; 2],
        width: f32,
        delta: [f64; 2],
        v120: Option<[i32; 2]>,
    ) -> BarResponse {
        let popup_area = self
            .popup
            .map(|popup| self.popup_area(popup, placed, width).map(f64::from));
        let in_popup = popup_area.is_some_and(|[px, py, pw, ph]| {
            (px..px + pw).contains(&x) && (py..py + ph).contains(&y)
        });
        let on_bar = y < f64::from(height());
        if !on_bar && !in_popup {
            return BarResponse::default();
        }
        // a step per wheel notch (hi-res ones' parts add up), or per `SCROLL_STEP` px of a touchpad
        self.scrolled += v120.map_or(delta[1], |[_, v120]| f64::from(v120) / 120.0 * SCROLL_STEP);
        #[expect(clippy::cast_possible_truncation, reason = "a few steps")]
        let steps = (self.scrolled / SCROLL_STEP).trunc() as i32;
        self.scrolled -= f64::from(steps) * SCROLL_STEP;
        let mut damaged = false;
        if steps != 0 {
            if in_popup && let Some(Popup::Calendar(month)) = self.popup {
                let month = if steps > 0 {
                    month.checked_add_months(chrono::Months::new(steps.unsigned_abs()))
                } else {
                    month.checked_sub_months(chrono::Months::new(steps.unsigned_abs()))
                };
                if let Some(month) = month {
                    self.popup = Some(Popup::Calendar(month));
                    damaged = true;
                }
            } else if on_bar
                && placed.iter().any(|placed| {
                    let [left, w] = placed.span.map(f64::from);
                    placed.segment == Segment::Hub && (left..left + w).contains(&x)
                })
            {
                let volume = self.controls.state.lock().unwrap().volume;
                if let Some(volume) = volume {
                    // up is louder
                    #[expect(clippy::cast_precision_loss, reason = "a few steps")]
                    let level = (volume.level - steps as f32 * 0.05).clamp(0.0, 1.0);
                    self.controls.set_volume(level);
                    damaged = true;
                }
            }
        }
        BarResponse {
            consumed: true,
            damaged,
            action: None,
        }
    }

    /// A mouse event at `[x, y]` over a notification popping up (a click dismisses or activates it)
    fn mouse_on_popup_notification(
        &self,
        palette: &Palette,
        [x, y]: [f64; 2],
        width: f32,
        pressed: bool,
    ) -> Option<BarResponse> {
        let ([ax, ay, ..], index) = self
            .popup_notifications(palette, width)
            .into_iter()
            .enumerate()
            .find_map(|(index, ([ax, ay, aw, ah], _))| {
                let [ax, ay, aw, ah] = [ax, ay, aw, ah].map(f64::from);
                ((ax..ax + aw).contains(&x) && (ay..ay + ah).contains(&y))
                    .then_some(([ax, ay, aw, ah], index))
            })?;
        if pressed {
            let click = {
                let store = self.notifications.store.lock().unwrap();
                let notification = store.popups().nth(index);
                let at = Lines::new().at([x - ax, y - ay]);
                notification
                    .zip(at)
                    .map(|(n, (line, column))| notification_list::click_popup(n, line, column))
            };
            if let Some(click) = click {
                self.apply_notification_click(click);
            }
        }
        Some(BarResponse {
            consumed: true,
            damaged: pressed,
            action: None,
        })
    }

    /// A click `x` px into the tray: left activates, middle secondary-activates, right opens the menu
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "an icon's index"
    )]
    fn click_tray(&mut self, button: u32, x: f64) {
        let index = (x.max(0.0) / f64::from(3.0 * char_width())) as usize;
        let Some(item) = self.tray_items().into_iter().nth(index) else {
            return;
        };
        match button {
            BTN_LEFT if item.item_is_menu => {
                self.tray.activate(&item);
                self.set_popup(Some(Popup::TrayMenu));
            }
            BTN_LEFT => self.tray.activate(&item),
            BTN_MIDDLE => self.tray.secondary_activate(&item),
            BTN_RIGHT => {
                self.set_popup(None);
                self.tray.open_menu(&item);
                self.set_popup(Some(Popup::TrayMenu));
            }
            _ => {}
        }
    }

    fn apply_notification_click(&self, click: notification_list::Click) {
        match click {
            notification_list::Click::Activate(id) => self.notifications.activate(id),
            notification_list::Click::Action(id, key) => self.notifications.invoke(id, key),
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
            Popup::TrayMenu => {
                let id = tray::menu_click(self.tray.menu.lock().unwrap().as_ref(), line);
                let Some(id) = id else {
                    return false;
                };
                self.tray.click_menu(id);
                self.set_popup(None);
                return true;
            }
            Popup::Hub(open) => {
                // not matched on directly: the lock would be held through the actions'
                let click = hub::click(&self.controls.state.lock().unwrap(), open, line, column);
                let Some(click) = click else {
                    return false;
                };
                self.apply_hub_click(click);
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

    fn apply_hub_click(&mut self, click: hub::Click) {
        let controls = &self.controls;
        match click {
            hub::Click::Open(section) => self.set_popup(Some(Popup::Hub(section))),
            hub::Click::ToggleWifi => controls.toggle_wifi(),
            hub::Click::ConnectWifi(ssid) => controls.connect_wifi(ssid),
            hub::Click::ToggleBluetooth => controls.toggle_bluetooth(),
            hub::Click::ToggleDevice(index) => {
                let device = controls.state.lock().unwrap().devices.get(index).cloned();
                if let Some(device) = device {
                    controls.toggle_device(&device);
                }
            }
            hub::Click::ToggleMute => controls.toggle_mute(),
            hub::Click::Volume(level) => controls.set_volume(level),
            hub::Click::Output(id) => controls.set_default_sink(id),
            hub::Click::Brightness(level) => controls.set_brightness(level),
        }
    }

    /// Sets the segments by name, `left` and `right` each left to right (`sonamu.bar_layout`)
    pub fn set_layout(&mut self, left: &[String], right: &[String]) -> Result<(), String> {
        let named = |names: &[String]| -> Result<Vec<Segment>, String> {
            names.iter().map(|name| Segment::named(name)).collect()
        };
        self.layout = [named(left)?, named(right)?];
        // its segment may be gone
        self.set_popup(None);
        Ok(())
    }

    pub fn has_popup(&self) -> bool {
        self.popup.is_some()
    }

    pub fn close_popup(&mut self) {
        self.set_popup(None);
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

    /// Whether what changes by itself (the clock's second, which notifications pop up, the OSD going away) changed
    /// since the last call, for the root's timer
    pub fn tick(&mut self) -> bool {
        let popups = self
            .notifications
            .store
            .lock()
            .unwrap()
            .popups()
            .map(|n| n.id)
            .collect();
        let osd = hub::osd(&self.controls.state.lock().unwrap(), &Palette::default()).is_some();
        let shown = (Self::time(), popups, osd);
        let changed = shown != self.shown;
        self.shown = shown;
        changed
    }
}

/// The segment at `x` on the bar, and its `[left, width]`; half the gap around each is its too
fn segment_at(placed: &[Placed], x: f64) -> Option<(Segment, [f32; 2])> {
    placed
        .iter()
        .find(|placed| {
            let [left, w] = placed.span;
            (f64::from(left - GAP / 2.0)..f64::from(left + w + GAP / 2.0)).contains(&x)
        })
        .map(|placed| (placed.segment, placed.span))
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
