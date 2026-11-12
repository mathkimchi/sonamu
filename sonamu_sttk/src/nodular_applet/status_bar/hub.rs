//! The hub: the bar's wifi, bluetooth, volume and brightness icons, their popup, and the OSD shown when volume or
//! brightness change

use super::{
    Lines, Spans,
    controls::{Osd, State, Volume},
    notification_list::cut,
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

/// A line's text, and whether it's highlighted
fn line_spans(
    line: Line,
    state: &State,
    open: Option<Section>,
    palette: &Palette,
) -> (Spans, bool) {
    let on_off = |on: bool| if on { "on" } else { "off" };
    match line {
        Line::Wifi => {
            let name = state
                .active_network()
                .map_or("Wi-Fi", |network| network.ssid.as_str());
            let left = format!(
                "{} {name} {}",
                wifi_icon(state),
                arrow(open == Some(Section::Wifi))
            );
            (
                vec![(justified(&left, on_off(state.wifi_enabled)), palette.fg)],
                false,
            )
        }
        Line::Network(ssid) => {
            let network = state.networks.iter().find(|network| network.ssid == ssid);
            let (signal, active) = network.map_or((0, false), |n| (n.signal, n.active));
            let color = if active { palette.accent } else { palette.fg };
            (
                vec![(
                    justified(&format!("   {ssid}"), &format!("{signal}%")),
                    color,
                )],
                false,
            )
        }
        Line::Bluetooth => {
            let left = format!(
                "{} Bluetooth {}",
                bluetooth_icon(state),
                arrow(open == Some(Section::Bluetooth))
            );
            (
                vec![(
                    justified(&left, on_off(state.bluetooth_powered)),
                    palette.fg,
                )],
                false,
            )
        }
        Line::Device(index) => {
            let device = &state.devices[index];
            let color = if device.connected {
                palette.accent
            } else {
                palette.fg
            };
            let right = if device.connected { "connected" } else { "" };
            (
                vec![(justified(&format!("   {}", device.name), right), color)],
                false,
            )
        }
        Line::Volume => {
            let level = state.volume.map_or(0.0, |volume| volume.level);
            let muted = state.volume.is_none_or(|volume| volume.muted);
            (
                vec![
                    (format!("{} ", volume_icon(state.volume)), palette.fg),
                    (
                        slider(level),
                        if muted { palette.inactive } else { palette.fg },
                    ),
                    (
                        format!("  out {}", arrow(open == Some(Section::Outputs))),
                        palette.fg,
                    ),
                ],
                false,
            )
        }
        Line::Output(id) => {
            let sink = state.sinks.iter().find(|sink| sink.id == id);
            let (name, default) = sink.map_or(("", false), |s| (s.name.as_str(), s.default));
            let color = if default { palette.accent } else { palette.fg };
            (vec![(cut(&format!("   {name}"), COLUMNS), color)], default)
        }
        Line::Brightness => (
            vec![(
                format!("{BRIGHTNESS} {}", slider(state.brightness.unwrap_or(0.0))),
                palette.fg,
            )],
            false,
        ),
        Line::Settings => (
            vec![(format!("{SETTINGS} Settings ›"), palette.inactive)],
            false,
        ),
    }
}

pub fn draw(settings: &Settings, state: &State, open: Option<Section>) -> Vec<UITree> {
    let palette = &settings.palette;
    let text = Lines::new();
    let mut items = Vec::new();
    for (index, line) in lines(state, open).into_iter().enumerate() {
        let (spans, highlight) = line_spans(line, state, open, palette);
        if highlight {
            items.push(text.highlight(index, [0, COLUMNS], palette.raised(30)));
        }
        items.push(text.text(index, 0, spans));
    }
    items
}

pub fn click(state: &State, open: Option<Section>, line: usize, column: usize) -> Option<Click> {
    let toggle = |section: Section| Click::Open((open != Some(section)).then_some(section));
    // the on/off at the end of a header line
    let at_end = column + 4 >= COLUMNS;
    Some(match lines(state, open).into_iter().nth(line)? {
        Line::Wifi if at_end => Click::ToggleWifi,
        Line::Wifi => toggle(Section::Wifi),
        Line::Network(ssid) => Click::ConnectWifi(ssid),
        Line::Bluetooth if at_end => Click::ToggleBluetooth,
        Line::Bluetooth => toggle(Section::Bluetooth),
        Line::Device(index) => Click::ToggleDevice(index),
        Line::Volume if column < SLIDER_COLUMN - 1 => Click::ToggleMute,
        Line::Volume => match slider_level(column) {
            Some(level) => Click::Volume(level),
            None => toggle(Section::Outputs),
        },
        Line::Output(id) => Click::Output(id),
        Line::Brightness => Click::Brightness(slider_level(column)?),
        Line::Settings => return None,
    })
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
