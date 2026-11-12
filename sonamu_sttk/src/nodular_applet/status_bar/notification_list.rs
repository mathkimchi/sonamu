//! The bell: its popup (the list, Do not disturb, clear all) and the notifications popping up under the bar

use super::{
    Lines, Spans, columns,
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

#[derive(Clone, Copy)]
pub enum Click {
    /// Clicked it: its default action, then dismissed
    Activate(u32),
    Dismiss(u32),
    ClearAll,
    DoNotDisturb,
}

/// `text` cut to `max` columns, with `…` if cut
fn cut(text: &str, max: usize) -> String {
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

/// `left` and then `right` at the end of the width
fn justified(left: &str, right: &str) -> String {
    let left = cut(left, COLUMNS.saturating_sub(columns(right) + 1));
    let pad = COLUMNS.saturating_sub(columns(&left) + columns(right));
    format!("{left}{}{right}", " ".repeat(pad))
}

fn listed(store: &Store) -> usize {
    store.list.len().min(LISTED)
}

pub fn size(store: &Store) -> [f32; 2] {
    Lines::size(HEADER_LINES + (2 * listed(store)).max(1), COLUMNS)
}

pub fn draw(settings: &Settings, store: &Store) -> Vec<UITree> {
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
        items.push(lines.text(
            line,
            0,
            vec![(justified(&title(notification), CLOSE), palette.fg)],
        ));
        let body = wrap(&notification.body, COLUMNS, 1)
            .pop()
            .unwrap_or_default();
        items.push(lines.text(line + 1, 0, vec![(body, palette.inactive)]));
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

/// A popping up one's lines: title, then its body
fn popup_lines(notification: &Notification, palette: &Palette) -> Vec<Spans> {
    let mut lines = vec![vec![(justified(&title(notification), CLOSE), palette.fg)]];
    lines.extend(
        wrap(&notification.body, COLUMNS, BODY_LINES)
            .into_iter()
            .filter(|line| !line.is_empty())
            .map(|line| vec![(line, palette.inactive)]),
    );
    lines
}

/// Its height in lines
pub fn popup_line_count(notification: &Notification, palette: &Palette) -> usize {
    popup_lines(notification, palette).len()
}

pub fn draw_popup(notification: &Notification, palette: &Palette) -> Vec<UITree> {
    let lines = Lines::new();
    popup_lines(notification, palette)
        .into_iter()
        .enumerate()
        .map(|(index, spans)| lines.text(index, 0, spans))
        .collect()
}

pub fn click_popup(notification: &Notification, line: usize, column: usize) -> Click {
    if line == 0 && column + 3 >= COLUMNS {
        Click::Dismiss(notification.id)
    } else {
        Click::Activate(notification.id)
    }
}
