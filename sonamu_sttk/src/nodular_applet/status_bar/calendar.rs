//! The time's popup: a month grid (a placeholder for a better calendar)

use super::{
    Lines, Spans,
    widgets::{Button, Cell},
};
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

/// `‹` and `›`, at the ends of the title line
fn arrows() -> [(Button, &'static str, Months, bool); 2] {
    [
        (Button::new(0, [0, 2]), "‹", Months::new(1), false),
        (
            Button::new(0, [columns() - 2, columns()]),
            " ›",
            Months::new(1),
            true,
        ),
    ]
}

/// The month of `date`'s, today marked; `‹`/`›` at the ends of its title line change month
pub fn draw(settings: &Settings, date: NaiveDate, hover: Option<Cell>) -> Vec<UITree> {
    let palette = &settings.palette;
    let today = Local::now().date_naive();
    let first = date.with_day(1).unwrap_or(date);
    let title = first.format("%B %Y").to_string();
    let pad = columns().saturating_sub(title.chars().count() + 2);
    let header = format!(" {}{title}", " ".repeat(pad / 2));
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
    let mut items: Vec<UITree> = lines
        .into_iter()
        .enumerate()
        .map(|(index, spans)| text.text(index, 0, spans))
        .collect();
    for (button, label, ..) in arrows() {
        items.extend(button.draw(
            &text,
            palette,
            vec![(label.to_string(), palette.fg)],
            false,
            hover,
        ));
    }
    items
}

/// The month shown after a click on `line`, `column`, if it changes
pub fn click(month: NaiveDate, cell: Cell) -> Option<NaiveDate> {
    let (_, _, by, forward) = arrows()
        .into_iter()
        .find(|(button, ..)| button.contains(cell))?;
    if forward {
        month.checked_add_months(by)
    } else {
        month.checked_sub_months(by)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_change_month() {
        let month = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        let next = NaiveDate::from_ymd_opt(2026, 12, 1);
        assert_eq!(click(month, (0, columns() - 1)), next);
        assert_eq!(click(month, (0, 0)), NaiveDate::from_ymd_opt(2026, 10, 1));
        assert_eq!(click(month, (0, 8)), None);
    }
}
