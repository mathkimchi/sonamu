//! Battery, cpu and memory: the bar's segment and its popup (power profile, processes like `top`)

use super::{
    Lines, Spans, columns,
    system::{Battery, PROCESSES, PROFILES, Snapshot},
    widgets::{Button, Cell, ScrollList, label_color},
};
use sonamu_common::settings::{ERROR, Palette, Settings};
use sonamu_ui::{color::Color, ui_element::UITree};
use std::{cmp::Ordering, time::Duration};

const GREEN: Color = Color([0x8f, 0xd1, 0x8f, 0xff]);
const YELLOW: Color = Color([0xe0, 0xc0, 0x68, 0xff]);

const CPU: &str = "\u{f4bc}";
const MEM: &str = "\u{f035b}";
const CHARGING: &str = "\u{f0084}";

/// The popup's width in characters
const COLUMNS: usize = 44;
/// Lines above the process list: battery, rate, profiles, blank, header
const HEADER_LINES: usize = 5;
/// Where the process list's cpu and memory columns start
const CPU_COLUMN: usize = 28;
const MEM_COLUMN: usize = 36;
/// The profile picker's line
const PROFILE_LINE: usize = 2;
/// The process list's header line
const SORT_LINE: usize = 4;
/// The profile picker's buttons: label and columns
const PROFILE_BUTTONS: [(&str, [usize; 2]); 3] = [
    ("\u{f032a} Saver", [0, 8]),
    ("\u{f24e} Balanced", [12, 23]),
    ("\u{f04c5} Performance", [28, 42]),
];
/// The process list's sort buttons: label and columns
const SORT_BUTTONS: [(&str, Sort, [usize; 2]); 2] = [
    ("CPU", Sort::Cpu, [CPU_COLUMN, CPU_COLUMN + 4]),
    ("Mem", Sort::Mem, [MEM_COLUMN, MEM_COLUMN + 4]),
];

/// What the process list is sorted by (most first)
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Sort {
    #[default]
    Cpu,
    Mem,
}

pub enum Click {
    Sort(Sort),
    Profile(&'static str),
}

/// >50% green, 20–50% yellow, <20% red
fn battery_color(percent: u8) -> Color {
    match percent {
        51.. => GREEN,
        20..=50 => YELLOW,
        _ => ERROR,
    }
}

/// The battery icon for its level, or charging
fn battery_icon(battery: &Battery) -> char {
    if battery.charging() {
        return CHARGING.chars().next().unwrap_or(' ');
    }
    // 󰂎 (empty), then 󰁺 10% … 󰂂 90% in order, and 󰁹 full
    match battery.percent {
        95.. => '\u{f0079}',
        percent => char::from_u32(0xf007a + u32::from(percent.saturating_sub(5) / 10).min(8))
            .unwrap_or('\u{f008e}'),
    }
}

/// `74%  12%  41%` with icons; without a battery only cpu and memory
pub fn segment(snapshot: &Snapshot, palette: &Palette) -> Spans {
    let mut spans = Vec::new();
    if let Some(battery) = &snapshot.battery {
        spans.push((
            format!("{} {}%  ", battery_icon(battery), battery.percent),
            battery_color(battery.percent),
        ));
    }
    spans.push((
        format!(
            "{CPU} {:.0}%  {MEM} {:.0}%",
            snapshot.cpu.round(),
            snapshot.mem.round()
        ),
        palette.fg,
    ));
    spans
}

fn shown_processes(snapshot: &Snapshot) -> usize {
    snapshot.processes.len().clamp(1, PROCESSES)
}

pub fn size(snapshot: &Snapshot) -> [f32; 2] {
    Lines::size(HEADER_LINES + shown_processes(snapshot), COLUMNS)
}

/// eg `3 h 12 min`, `12 min`
fn duration(duration: Duration) -> String {
    let minutes = duration.as_secs() / 60;
    match minutes / 60 {
        0 => format!("{minutes} min"),
        hours => format!("{hours} h {} min", minutes % 60),
    }
}

/// eg `1.9G`, `410M`
#[expect(clippy::cast_precision_loss, reason = "for show")]
fn bytes(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1}G", mb / 1024.0)
    } else {
        format!("{mb:.0}M")
    }
}

/// `left` and `right` across the popup's width
fn justified(left: &str, right: &str) -> String {
    let pad = COLUMNS.saturating_sub(columns(left) + columns(right));
    format!("{left}{}{right}", " ".repeat(pad))
}

pub fn draw(
    settings: &Settings,
    snapshot: &Snapshot,
    (sort, list): (Sort, ScrollList),
    hover: Option<Cell>,
) -> Vec<UITree> {
    let palette = &settings.palette;
    let lines = Lines::new();
    let mut items = Vec::new();
    match &snapshot.battery {
        Some(battery) => {
            let left = format!(
                "{} {}% · {}",
                battery_icon(battery),
                battery.percent,
                battery.status
            );
            let right = battery.time_left.map(duration).unwrap_or_default();
            items.push(lines.text(
                0,
                0,
                vec![(justified(&left, &right), battery_color(battery.percent))],
            ));
            let rate = battery.watts.map_or_else(String::new, |watts| {
                // a real minus sign
                format!("{watts:.1} W").replace('-', "\u{2212}")
            });
            items.push(lines.text(1, 0, vec![(justified("Rate", &rate), palette.fg)]));
        }
        None => items.push(lines.text(0, 0, vec![("No battery".to_string(), palette.inactive)])),
    }
    if let Some(profile) = &snapshot.profile {
        for ((label, columns), name) in PROFILE_BUTTONS.iter().zip(PROFILES) {
            let on = profile == name;
            let button = Button::new(PROFILE_LINE, *columns);
            items.extend(button.draw(
                &lines,
                palette,
                vec![((*label).to_string(), label_color(palette, on))],
                on,
                hover,
            ));
        }
    }
    items.push(lines.text(
        SORT_LINE,
        0,
        vec![("Process".to_string(), palette.inactive)],
    ));
    for (label, by, columns) in SORT_BUTTONS {
        let on = by == sort;
        let label = format!("{label}{}", if on { "▾" } else { " " });
        let button = Button::new(SORT_LINE, columns);
        let color = if on { palette.accent } else { palette.inactive };
        items.extend(button.draw(&lines, palette, vec![(label, color)], false, hover));
    }
    let processes = sorted(snapshot, sort);
    if processes.is_empty() {
        items.push(lines.text(
            HEADER_LINES,
            0,
            vec![("Reading…".to_string(), palette.inactive)],
        ));
    }
    let shown = list.visible(processes.len(), PROCESSES);
    for (line, process) in (HEADER_LINES..).zip(&processes[shown]) {
        let name: String = process.name.chars().take(CPU_COLUMN - 2).collect();
        items.push(lines.text(
            line,
            0,
            vec![(
                format!(
                    "{name:<CPU_COLUMN$}{:<w$}{}",
                    format!("{:.1}%", process.cpu),
                    bytes(process.mem),
                    w = MEM_COLUMN - CPU_COLUMN
                ),
                palette.fg,
            )],
        ));
    }
    items.extend(list.bar(
        &lines,
        [HEADER_LINES, COLUMNS],
        processes.len(),
        PROCESSES,
        palette.inactive,
    ));
    items
}

/// Its processes, most first
fn sorted(snapshot: &Snapshot, sort: Sort) -> Vec<&super::system::Process> {
    let mut processes: Vec<_> = snapshot.processes.iter().collect();
    processes.sort_by(|a, b| match sort {
        Sort::Cpu => b.cpu.partial_cmp(&a.cpu).unwrap_or(Ordering::Equal),
        Sort::Mem => b.mem.cmp(&a.mem),
    });
    processes
}

/// Scrolls the process list `steps` (positive down); whether it moved
pub fn scroll(snapshot: &Snapshot, list: &mut ScrollList, steps: i32) -> bool {
    list.scroll(steps, snapshot.processes.len(), PROCESSES)
}

pub fn click(snapshot: &Snapshot, sort: Sort, cell: Cell) -> Option<Click> {
    if snapshot.profile.is_some()
        && let Some((_, name)) = PROFILE_BUTTONS
            .iter()
            .zip(PROFILES)
            .find(|((_, columns), _)| Button::new(PROFILE_LINE, *columns).contains(cell))
    {
        return Some(Click::Profile(name));
    }
    let (_, by, _) = SORT_BUTTONS
        .into_iter()
        .find(|(_, _, columns)| Button::new(SORT_LINE, *columns).contains(cell))?;
    (by != sort).then_some(Click::Sort(by))
}
