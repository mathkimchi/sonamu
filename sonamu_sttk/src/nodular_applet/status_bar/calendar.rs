//! The time's popup: a month grid (a placeholder for a better calendar)

use super::{Lines, Spans};
use chrono::{Datelike, Local, Months, NaiveDate};
use sonamu_common::settings::Settings;
use sonamu_ui::ui_element::UITree;

/// The weekdays, the widest line
const WEEKDAYS: &str = "Mo Tu We Th Fr Sa Su";
/// Month, weekdays, up to 6 weeks
const LINES: usize = 8;

fn columns() -> usize {
    WEEKDAYS.chars().count()
}

pub fn size() -> [f32; 2] {
    Lines::size(LINES, columns())
}

/// The month of `date`'s, today marked; `‹`/`›` at the ends of its title line change month
pub fn draw(settings: &Settings, date: NaiveDate) -> Vec<UITree> {
    let palette = &settings.palette;
    let today = Local::now().date_naive();
    let first = date.with_day(1).unwrap_or(date);
    let title = first.format("%B %Y").to_string();
    let pad = columns().saturating_sub(title.chars().count() + 2);
    let header = format!(
        "‹{}{title}{}›",
        " ".repeat(pad / 2),
        " ".repeat(pad - pad / 2)
    );
    let mut lines: Vec<Spans> = vec![
        vec![(header, palette.fg)],
        vec![(WEEKDAYS.to_string(), palette.inactive)],
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
    let text = Lines::new();
    lines
        .into_iter()
        .enumerate()
        .map(|(index, spans)| text.text(index, 0, spans))
        .collect()
}

/// The month shown after a click on `line`, `column`, if it changes
pub fn click(month: NaiveDate, line: usize, column: usize) -> Option<NaiveDate> {
    if line != 0 {
        return None;
    }
    if column < 3 {
        month.checked_sub_months(Months::new(1))
    } else if column + 3 >= columns() {
        month.checked_add_months(Months::new(1))
    } else {
        None
    }
}
