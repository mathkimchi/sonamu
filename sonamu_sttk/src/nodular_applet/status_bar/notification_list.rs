//! The bell: its popup (the list, Do not disturb, clear all) and the notifications popping up under the bar

use super::{
    Lines, Spans, columns,
    icons::Icons,
    notifications::{Notification, Store},
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

pub fn draw(settings: &Settings, store: &Store, icons: &Icons) -> Vec<UITree> {
    let palette = &settings.palette;
    let lines = Lines::new();
    let mut items = vec![lines.text(
        0,
        0,
        vec![(justified("Notifications", CLEAR_ALL), palette.fg)],
    )];
    let (icon, state) = if store.do_not_disturb {
        (BELL_OFF, "on")
    } else {
        (BELL, "off")
    };
    let dnd = format!("{icon} Do not disturb");
    if store.do_not_disturb {
        items.push(lines.highlight(1, [0, columns(&dnd)], palette.raised(30)));
    }
    items.push(lines.text(
        1,
        0,
        vec![(
            justified(&dnd, state),
            if store.do_not_disturb {
                palette.accent
            } else {
                palette.fg
            },
        )],
    ));
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
        let width = COLUMNS - TEXT_COLUMN;
        items.push(lines.text(
            line,
            TEXT_COLUMN,
            vec![(justified_in(width, &title(notification), CLOSE), palette.fg)],
        ));
        let body = wrap(&notification.body, width, 1).pop().unwrap_or_default();
        items.push(lines.text(line + 1, TEXT_COLUMN, vec![(body, palette.inactive)]));
    }
    items
}

pub fn click(store: &Store, line: usize, column: usize) -> Option<Click> {
    match line {
        0 if column + columns(CLEAR_ALL) >= COLUMNS => Some(Click::ClearAll),
        1 => Some(Click::DoNotDisturb),
        _ => {
            let index = line.checked_sub(HEADER_LINES)? / 2;
            let notification = store.list.iter().rev().take(LISTED).nth(index)?;
            // the ✕ at the end of its first line
            if (line - HEADER_LINES).is_multiple_of(2) && column + 3 >= COLUMNS {
                Some(Click::Dismiss(notification.id))
            } else {
                Some(Click::Activate(notification.id))
            }
        }
    }
}

/// A popping up one's lines (after its icon): title, then its body
fn popup_lines(notification: &Notification, palette: &Palette) -> Vec<Spans> {
    let width = COLUMNS - TEXT_COLUMN;
    let mut lines = vec![vec![(
        justified_in(width, &title(notification), CLOSE),
        palette.fg,
    )]];
    lines.extend(
        wrap(&notification.body, width, BODY_LINES)
            .into_iter()
            .filter(|line| !line.is_empty())
            .map(|line| vec![(line, palette.inactive)]),
    );
    let buttons = action_buttons(notification);
    if !buttons.is_empty() {
        lines.push(
            buttons
                .into_iter()
                .map(|(_, label, _)| (format!("[{label}]  "), palette.accent))
                .collect(),
        );
    }
    lines
}

/// Its actions other than `default`, as buttons: key, label and columns (from `TEXT_COLUMN`), while they fit
fn action_buttons(notification: &Notification) -> Vec<(String, String, [usize; 2])> {
    let mut column = 0;
    notification
        .actions
        .iter()
        .filter(|(key, _)| key != "default")
        .map_while(|(key, label)| {
            let label = cut(label, 16);
            // `[label]  `
            let end = column + columns(&label) + 2;
            (end <= COLUMNS - TEXT_COLUMN).then(|| {
                let button = (key.clone(), label, [column, end]);
                column = end + 2;
                button
            })
        })
        .collect()
}

/// Its height in lines
pub fn popup_line_count(notification: &Notification, palette: &Palette) -> usize {
    popup_lines(notification, palette).len()
}

pub fn draw_popup(notification: &Notification, palette: &Palette, icons: &Icons) -> Vec<UITree> {
    let lines = Lines::new();
    let mut items: Vec<UITree> = popup_lines(notification, palette)
        .into_iter()
        .enumerate()
        .map(|(index, spans)| lines.text(index, TEXT_COLUMN, spans))
        .collect();
    items.extend(app_icon(notification, icons).map(|icon| lines.icon(0, 0, icon)));
    items
}

pub fn click_popup(notification: &Notification, line: usize, column: usize) -> Click {
    let palette = Palette::default();
    let buttons_line = popup_line_count(notification, &palette) - 1;
    let buttons = action_buttons(notification);
    if line == 0 && column + 3 >= COLUMNS {
        return Click::Dismiss(notification.id);
    }
    if line == buttons_line
        && let Some((key, ..)) = buttons
            .into_iter()
            .find(|(_, _, [from, to])| (from + TEXT_COLUMN..to + TEXT_COLUMN).contains(&column))
    {
        return Click::Action(notification.id, key);
    }
    Click::Activate(notification.id)
}
