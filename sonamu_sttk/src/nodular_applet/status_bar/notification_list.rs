//! The bell: its popup (the list, Do not disturb, clear all) and the notifications popping up under the bar

use super::{
    Lines, Spans, columns,
    icons::Icons,
    notifications::{Notification, Store},
    widgets::{Button, Cell, label_color},
};
use sonamu_common::settings::{Palette, Settings};
use sonamu_ui::ui_element::UITree;

pub const BELL: &str = "\u{f009a}";
pub const BELL_OFF: &str = "\u{f009b}";
const CLOSE: &str = "\u{f0156}";

/// The popup's and a popup notification's width in characters
pub const COLUMNS: usize = 44;
/// Lines above the list: title, Do not disturb, blank
const HEADER_LINES: usize = 3;
/// Notifications listed at most (2 lines each)
const LISTED: usize = 8;
/// Body lines a popping up one shows at most
const BODY_LINES: usize = 3;
/// Popping up at once at most
pub const POPUPS: usize = 3;
const CLEAR_ALL: &str = "Clear all";
const DO_NOT_DISTURB: Button = Button::row(1, COLUMNS);

fn clear_all() -> Button {
    Button::new(0, [COLUMNS - columns(CLEAR_ALL), COLUMNS])
}

#[derive(Clone)]
pub enum Click {
    /// Clicked it: its default action, then dismissed
    Activate(u32),
    /// One of its other actions (by key), then dismissed
    Action(u32, String),
    Dismiss(u32),
    ClearAll,
    DoNotDisturb,
}

/// `text` cut to `max` columns, with `…` if cut
pub fn cut(text: &str, max: usize) -> String {
    if columns(text) <= max {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        if columns(&out) + columns(&c.to_string()) + 1 > max {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

/// `text` wrapped at spaces to `width` columns, at most `max` lines (the last cut)
fn wrap(text: &str, width: usize, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if !line.is_empty() && columns(&line) + 1 + columns(word) > width {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        lines.push(line);
    }
    if lines.len() > max {
        let rest = lines.split_off(max - 1).join(" ");
        lines.push(rest);
    }
    lines.into_iter().map(|line| cut(&line, width)).collect()
}

/// `app: summary`
fn title(notification: &Notification) -> String {
    if notification.app_name.is_empty() {
        notification.summary.clone()
    } else {
        format!("{}: {}", notification.app_name, notification.summary)
    }
}

/// `left` and then `right` at the end of `width` columns
fn justified_in(width: usize, left: &str, right: &str) -> String {
    let left = cut(left, width.saturating_sub(columns(right) + 1));
    let pad = width.saturating_sub(columns(&left) + columns(right));
    format!("{left}{}{right}", " ".repeat(pad))
}

fn justified(left: &str, right: &str) -> String {
    justified_in(COLUMNS, left, right)
}

/// Where a notification's text starts, after its icon
const TEXT_COLUMN: usize = 3;

/// Its icon: `app_icon` (or the image path), else the icon named like its `.desktop` file or app
fn app_icon(notification: &Notification, icons: &Icons) -> Option<sonamu_ui::ui_element::Image> {
    [
        Some(notification.icon.clone()),
        notification.desktop_entry.clone(),
        Some(notification.app_name.to_lowercase()),
    ]
    .into_iter()
    .flatten()
    .find(|name| !name.is_empty())
    .and_then(|name| icons.get(&name))
}

fn listed(store: &Store) -> usize {
    store.list.len().min(LISTED)
}

pub fn size(store: &Store) -> [f32; 2] {
    Lines::size(HEADER_LINES + (2 * listed(store)).max(1), COLUMNS)
}

/// A button, its label, and what a click does
type Entry = (Button, Spans, Click);

/// A notification's buttons, from line `top`, `height` lines tall: itself (its title on the first line; activates
/// it) and its ✕ (dismisses it)
fn notification_buttons(
    notification: &Notification,
    top: usize,
    height: usize,
    palette: &Palette,
) -> [Entry; 2] {
    let title = cut(&title(notification), COLUMNS - TEXT_COLUMN - 4);
    [
        (
            Button::new(top, [0, COLUMNS - 3]).tall(height),
            vec![(format!("{:TEXT_COLUMN$}{title}", ""), palette.fg)],
            Click::Activate(notification.id),
        ),
        (
            Button::new(top, [COLUMNS - 2, COLUMNS]),
            vec![(CLOSE.to_string(), palette.fg)],
            Click::Dismiss(notification.id),
        ),
    ]
}

/// The list's buttons: clear all, Do not disturb, and each listed notification's
fn list_buttons(store: &Store, palette: &Palette) -> Vec<Entry> {
    let on = store.do_not_disturb;
    let (icon, state) = if on { (BELL_OFF, "on") } else { (BELL, "off") };
    let mut entries = vec![
        (
            clear_all(),
            vec![(CLEAR_ALL.to_string(), palette.fg)],
            Click::ClearAll,
        ),
        (
            DO_NOT_DISTURB,
            vec![(
                justified(&format!("{icon} Do not disturb"), state),
                label_color(palette, on),
            )],
            Click::DoNotDisturb,
        ),
    ];
    for (index, notification) in store.list.iter().rev().take(LISTED).enumerate() {
        entries.extend(notification_buttons(
            notification,
            HEADER_LINES + 2 * index,
            2,
            palette,
        ));
    }
    entries
}

pub fn draw(settings: &Settings, store: &Store, icons: &Icons, hover: Option<Cell>) -> Vec<UITree> {
    let palette = &settings.palette;
    let lines = Lines::new();
    let mut items = vec![lines.text(0, 0, vec![("Notifications".to_string(), palette.fg)])];
    for (button, spans, click) in list_buttons(store, palette) {
        let on = matches!(click, Click::DoNotDisturb) && store.do_not_disturb;
        items.extend(button.draw(&lines, palette, spans, on, hover));
    }
    if store.list.is_empty() {
        items.push(lines.text(
            HEADER_LINES,
            0,
            vec![("No notifications".to_string(), palette.inactive)],
        ));
    }
    for (index, notification) in store.list.iter().rev().take(LISTED).enumerate() {
        let line = HEADER_LINES + 2 * index;
        if let Some(icon) = app_icon(notification, icons) {
            items.push(lines.icon(line, 0, icon));
        }
        let body = wrap(&notification.body, COLUMNS - TEXT_COLUMN, 1)
            .pop()
            .unwrap_or_default();
        items.push(lines.text(line + 1, TEXT_COLUMN, vec![(body, palette.inactive)]));
    }
    items
}

pub fn click(store: &Store, cell: Cell) -> Option<Click> {
    list_buttons(store, &Palette::default())
        .into_iter()
        .find(|(button, ..)| button.contains(cell))
        .map(|(.., click)| click)
}

/// A popping up one's body lines, shown under its title
fn popup_body(notification: &Notification) -> Vec<String> {
    wrap(&notification.body, COLUMNS - TEXT_COLUMN, BODY_LINES)
        .into_iter()
        .filter(|line| !line.is_empty())
        .collect()
}

/// A popping up one's buttons: itself and its ✕ over its title and body, then its actions other than `default`
/// on the last line, `[label]`, while they fit
fn popup_buttons(notification: &Notification, palette: &Palette) -> Vec<Entry> {
    let height = 1 + popup_body(notification).len();
    let mut entries = notification_buttons(notification, 0, height, palette).to_vec();
    let mut column = TEXT_COLUMN;
    entries.extend(
        notification
            .actions
            .iter()
            .filter(|(key, _)| key != "default")
            .map_while(|(key, label)| {
                let label = format!("[{}]", cut(label, 16));
                let end = column + columns(&label);
                (end <= COLUMNS).then(|| {
                    let button = Button::new(height, [column, end]);
                    column = end + 2;
                    (
                        button,
                        vec![(label, palette.accent)],
                        Click::Action(notification.id, key.clone()),
                    )
                })
            }),
    );
    entries
}

/// Its height in lines: title, body, and its actions' line if any
pub fn popup_line_count(notification: &Notification) -> usize {
    let actions = popup_buttons(notification, &Palette::default()).len() > 2;
    1 + popup_body(notification).len() + usize::from(actions)
}

pub fn draw_popup(
    notification: &Notification,
    palette: &Palette,
    icons: &Icons,
    hover: Option<Cell>,
) -> Vec<UITree> {
    let lines = Lines::new();
    let mut items = Vec::new();
    for (button, spans, _) in popup_buttons(notification, palette) {
        items.extend(button.draw(&lines, palette, spans, false, hover));
    }
    for (index, body) in popup_body(notification).into_iter().enumerate() {
        items.push(lines.text(1 + index, TEXT_COLUMN, vec![(body, palette.inactive)]));
    }
    items.extend(app_icon(notification, icons).map(|icon| lines.icon(0, 0, icon)));
    items
}

pub fn click_popup(notification: &Notification, cell: Cell) -> Option<Click> {
    popup_buttons(notification, &Palette::default())
        .into_iter()
        .find(|(button, ..)| button.contains(cell))
        .map(|(.., click)| click)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_clicks_hit_the_drawn_buttons() {
        let notification = Notification {
            id: 7,
            app_name: "app".to_string(),
            icon: String::new(),
            desktop_entry: None,
            summary: "hi".to_string(),
            body: "body".to_string(),
            actions: vec![
                ("default".to_string(), String::new()),
                ("reply".to_string(), "Reply".to_string()),
            ],
            critical: false,
            popup_until: None,
        };
        // title, body, actions
        assert_eq!(popup_line_count(&notification), 3);
        let click = |cell| click_popup(&notification, cell);
        assert!(matches!(click((0, COLUMNS - 1)), Some(Click::Dismiss(7))));
        assert!(matches!(click((1, 5)), Some(Click::Activate(7))));
        assert!(matches!(click((2, TEXT_COLUMN)), Some(Click::Action(7, key)) if key == "reply"));
        assert!(click((2, COLUMNS - 1)).is_none());
    }
}
