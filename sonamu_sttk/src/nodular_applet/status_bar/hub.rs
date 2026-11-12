//! The hub: the bar's wifi, bluetooth, volume and brightness icons, their popup, and the OSD shown when volume or
//! brightness change

use super::{
    Lines, Spans,
    controls::{Osd, State, Volume},
    notification_list::cut,
    widgets::{Button, Cell, label_color},
};
use sonamu_common::settings::{Palette, Settings};
use sonamu_ui::ui_element::UITree;
use std::time::Duration;

/// The popup's width in characters
const COLUMNS: usize = 44;
/// Slider cells
const SLIDER: usize = 24;
/// Where a slider starts, after its icon
const SLIDER_COLUMN: usize = 3;
/// Rows listed under an opened section at most
const LISTED: usize = 6;
/// How long the OSD shows
pub const OSD_TIME: Duration = Duration::from_millis(1500);

const WIFI_OFF: &str = "\u{f092d}";
const BLUETOOTH: &str = "\u{f00af}";
const BLUETOOTH_OFF: &str = "\u{f00b2}";
const BLUETOOTH_CONNECTED: &str = "\u{f00b1}";
const MUTED: &str = "\u{f075f}";
const BRIGHTNESS: &str = "\u{f00e0}";
const SETTINGS: &str = "\u{f0493}";
/// A header's on/off, at the line's end
const ON_OFF: [usize; 2] = [COLUMNS - 4, COLUMNS];
/// The volume's mute, its icon
const MUTE: [usize; 2] = [0, 2];
/// The volume's outputs, at the line's end
const OUTPUTS: [usize; 2] = [COLUMNS - 5, COLUMNS];

/// A section of the popup whose list is shown
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Section {
    Wifi,
    Bluetooth,
    Outputs,
}

/// What a line of the popup is
#[derive(Debug, Clone, PartialEq)]
enum Line {
    Wifi,
    Network(String),
    Bluetooth,
    Device(usize),
    Volume,
    Output(String),
    Brightness,
    Settings,
}

#[derive(Clone)]
pub enum Click {
    Open(Option<Section>),
    ToggleWifi,
    ConnectWifi(String),
    ToggleBluetooth,
    ToggleDevice(usize),
    ToggleMute,
    Volume(f32),
    Output(String),
    Brightness(f32),
}

fn wifi_icon(state: &State) -> &'static str {
    if !state.wifi_enabled {
        return WIFI_OFF;
    }
    // 󰤯 󰤟 󰤢 󰤥 󰤨 by signal
    match state.active_network().map(|network| network.signal) {
        None => "\u{f092f}",
        Some(0..25) => "\u{f091f}",
        Some(25..50) => "\u{f0922}",
        Some(50..75) => "\u{f0925}",
        Some(_) => "\u{f0928}",
    }
}

fn bluetooth_icon(state: &State) -> &'static str {
    if !state.bluetooth_powered {
        BLUETOOTH_OFF
    } else if state.devices.iter().any(|device| device.connected) {
        BLUETOOTH_CONNECTED
    } else {
        BLUETOOTH
    }
}

fn volume_icon(volume: Option<Volume>) -> &'static str {
    match volume {
        Some(Volume { muted: true, .. }) | None => MUTED,
        Some(Volume { level, .. }) if level < 0.34 => "\u{f057f}",
        Some(Volume { level, .. }) if level < 0.67 => "\u{f0580}",
        Some(_) => "\u{f057e}",
    }
}

/// The bar's segment: wifi, bluetooth, volume and brightness icons
pub fn segment(state: &State, palette: &Palette) -> Spans {
    let mut icons = vec![
        wifi_icon(state),
        bluetooth_icon(state),
        volume_icon(state.volume),
    ];
    if state.brightness.is_some() {
        icons.push(BRIGHTNESS);
    }
    vec![(icons.join(" "), palette.fg)]
}

/// The popup's lines, top down, with `open`'s list
fn lines(state: &State, open: Option<Section>) -> Vec<Line> {
    let mut lines = vec![Line::Wifi];
    if open == Some(Section::Wifi) {
        lines.extend(
            state
                .networks
                .iter()
                .take(LISTED)
                .map(|network| Line::Network(network.ssid.clone())),
        );
    }
    lines.push(Line::Bluetooth);
    if open == Some(Section::Bluetooth) {
        lines.extend((0..state.devices.len().min(LISTED)).map(Line::Device));
    }
    lines.push(Line::Volume);
    if open == Some(Section::Outputs) {
        lines.extend(
            state
                .sinks
                .iter()
                .take(LISTED)
                .map(|sink| Line::Output(sink.id.clone())),
        );
    }
    if state.brightness.is_some() {
        lines.push(Line::Brightness);
    }
    lines.push(Line::Settings);
    lines
}

pub fn size(state: &State, open: Option<Section>) -> [f32; 2] {
    Lines::size(lines(state, open).len(), COLUMNS)
}

/// `left` and then `right` at the end of the width
fn justified(left: &str, right: &str) -> String {
    let left = cut(left, COLUMNS.saturating_sub(super::columns(right) + 1));
    let pad = COLUMNS.saturating_sub(super::columns(&left) + super::columns(right));
    format!("{left}{}{right}", " ".repeat(pad))
}

/// `━━━━──── 60%`
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "slider cells"
)]
fn slider(level: f32) -> String {
    let filled = ((level.clamp(0.0, 1.0) * SLIDER as f32).round() as usize).min(SLIDER);
    format!(
        "{}{} {:.0}%",
        "━".repeat(filled),
        "─".repeat(SLIDER - filled),
        level * 100.0
    )
}

/// The level a click at `column` on a slider sets
#[expect(clippy::cast_precision_loss, reason = "slider cells")]
fn slider_level(column: usize) -> Option<f32> {
    let cell = column.checked_sub(SLIDER_COLUMN)?;
    (cell <= SLIDER).then(|| (cell as f32 / SLIDER as f32).clamp(0.0, 1.0))
}

fn arrow(open: bool) -> &'static str {
    if open { "▾" } else { "›" }
}

/// A button on a line: its columns, label, whether it's on, and what a click does
type LineButton = ([usize; 2], Spans, bool, Click);

/// A line's plain text (from its start) and its buttons
fn line_parts(
    line: Line,
    state: &State,
    open: Option<Section>,
    palette: &Palette,
) -> (Spans, Vec<LineButton>) {
    let on_off = |on: bool| {
        let label = format!("{:>4}", if on { "on" } else { "off" });
        (label, label_color(palette, on))
    };
    let toggle = |section: Section| Click::Open((open != Some(section)).then_some(section));
    // a header: its name opens its list, its on/off at the end toggles it
    let header = |left: String, section: Section, on: bool, toggle_click: Click| {
        let opened = open == Some(section);
        let left = format!("{left} {}", arrow(opened));
        let name = [0, ON_OFF[0] - 1];
        (
            Vec::new(),
            vec![
                (name, vec![(left, palette.fg)], opened, toggle(section)),
                (ON_OFF, vec![on_off(on)], on, toggle_click),
            ],
        )
    };
    // a list's line, all one button
    let row = |text: String, on: bool, click: Click| {
        (
            Vec::new(),
            vec![(
                [0, COLUMNS],
                vec![(text, label_color(palette, on))],
                on,
                click,
            )],
        )
    };
    match line {
        Line::Wifi => {
            let name = state
                .active_network()
                .map_or("Wi-Fi", |network| network.ssid.as_str());
            header(
                cut(&format!("{} {name}", wifi_icon(state)), ON_OFF[0] - 3),
                Section::Wifi,
                state.wifi_enabled,
                Click::ToggleWifi,
            )
        }
        Line::Network(ssid) => {
            let network = state.networks.iter().find(|network| network.ssid == ssid);
            let (signal, active) = network.map_or((0, false), |n| (n.signal, n.active));
            let text = justified(&format!("   {ssid}"), &format!("{signal}%"));
            row(text, active, Click::ConnectWifi(ssid))
        }
        Line::Bluetooth => header(
            format!("{} Bluetooth", bluetooth_icon(state)),
            Section::Bluetooth,
            state.bluetooth_powered,
            Click::ToggleBluetooth,
        ),
        Line::Device(index) => {
            let device = &state.devices[index];
            let right = if device.connected { "connected" } else { "" };
            let text = justified(&format!("   {}", device.name), right);
            row(text, device.connected, Click::ToggleDevice(index))
        }
        Line::Volume => volume_parts(state, open, palette),
        Line::Output(id) => {
            let sink = state.sinks.iter().find(|sink| sink.id == id);
            let (name, default) = sink.map_or(("", false), |s| (s.name.as_str(), s.default));
            row(
                cut(&format!("   {name}"), COLUMNS),
                default,
                Click::Output(id),
            )
        }
        Line::Brightness => (
            vec![(
                format!("{BRIGHTNESS} {}", slider(state.brightness.unwrap_or(0.0))),
                palette.fg,
            )],
            Vec::new(),
        ),
        Line::Settings => (
            vec![(format!("{SETTINGS} Settings ›"), palette.inactive)],
            Vec::new(),
        ),
    }
}

/// The volume line's slider, and its mute and outputs buttons
fn volume_parts(
    state: &State,
    open: Option<Section>,
    palette: &Palette,
) -> (Spans, Vec<LineButton>) {
    let toggle = |section: Section| Click::Open((open != Some(section)).then_some(section));
    let level = state.volume.map_or(0.0, |volume| volume.level);
    let muted = state.volume.is_none_or(|volume| volume.muted);
    let opened = open == Some(Section::Outputs);
    (
        vec![
            ("   ".to_string(), palette.fg),
            (
                slider(level),
                if muted { palette.inactive } else { palette.fg },
            ),
        ],
        vec![
            (
                MUTE,
                vec![(volume_icon(state.volume).to_string(), palette.fg)],
                false,
                Click::ToggleMute,
            ),
            (
                OUTPUTS,
                vec![(format!("out {}", arrow(opened)), palette.fg)],
                opened,
                toggle(Section::Outputs),
            ),
        ],
    )
}

pub fn draw(
    settings: &Settings,
    state: &State,
    open: Option<Section>,
    hover: Option<Cell>,
) -> Vec<UITree> {
    let palette = &settings.palette;
    let text = Lines::new();
    let mut items = Vec::new();
    for (index, line) in lines(state, open).into_iter().enumerate() {
        let (spans, buttons) = line_parts(line, state, open, palette);
        if !spans.is_empty() {
            items.push(text.text(index, 0, spans));
        }
        for (columns, spans, on, _) in buttons {
            let button = Button {
                line: index,
                columns,
            };
            items.extend(button.draw(&text, palette, spans, on, hover));
        }
    }
    items
}

pub fn click(state: &State, open: Option<Section>, (line, column): Cell) -> Option<Click> {
    let shown = lines(state, open).into_iter().nth(line)?;
    let (_, buttons) = line_parts(shown.clone(), state, open, &Palette::default());
    let button = buttons.into_iter().find(|(columns, ..)| {
        Button {
            line,
            columns: *columns,
        }
        .contains((line, column))
    });
    if let Some((.., click)) = button {
        return Some(click);
    }
    match shown {
        Line::Volume => slider_level(column).map(Click::Volume),
        Line::Brightness => slider_level(column).map(Click::Brightness),
        _ => None,
    }
}

/// The OSD's columns and its one line, if it's showing
pub fn osd(state: &State, palette: &Palette) -> Option<Spans> {
    let (osd, since) = state.osd?;
    if since.elapsed() > OSD_TIME {
        return None;
    }
    Some(match osd {
        Osd::Volume => {
            let volume = state.volume?;
            vec![(
                format!("{} {}", volume_icon(Some(volume)), slider(volume.level)),
                if volume.muted {
                    palette.inactive
                } else {
                    palette.fg
                },
            )]
        }
        Osd::Brightness => vec![(
            format!("{BRIGHTNESS} {}", slider(state.brightness?)),
            palette.fg,
        )],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicks_hit_the_drawn_buttons() {
        let state = State::default();
        assert!(matches!(
            click(&state, None, (0, 1)),
            Some(Click::Open(Some(Section::Wifi)))
        ));
        assert!(matches!(
            click(&state, None, (0, COLUMNS - 1)),
            Some(Click::ToggleWifi)
        ));
        assert!(matches!(
            click(&state, None, (1, COLUMNS - 2)),
            Some(Click::ToggleBluetooth)
        ));
        // volume: icon mutes, slider sets, the end opens the outputs
        assert!(matches!(
            click(&state, None, (2, 0)),
            Some(Click::ToggleMute)
        ));
        assert!(matches!(
            click(&state, None, (2, SLIDER_COLUMN)),
            Some(Click::Volume(0.0))
        ));
        assert!(matches!(
            click(&state, None, (2, COLUMNS - 1)),
            Some(Click::Open(Some(Section::Outputs)))
        ));
        assert!(click(&state, None, (3, 0)).is_none());
    }
}
