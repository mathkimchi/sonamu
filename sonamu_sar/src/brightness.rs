//! Each monitor's brightness (#158): a laptop panel's backlight (`brightnessctl`), else DDC/CI (`ddcutil`) where
//! setting `displays.<name>.ddc` allows it, else drawn darker (`DisplayRequest::SoftwareBrightness`).
//! Levels go to the root (saved in the session) and the bar (OSD, the hub's slider is the main screen's).

use crate::runner::AppletRunner;
use sonamu_common::sap::packets::StandardEvent;
use sonamu_ui::winit_backend::layout::MonitorInfo;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Condvar, Mutex},
    thread,
};

#[derive(Default)]
pub(crate) struct Brightness {
    /// Known levels (0 to 1), by monitor name
    levels: HashMap<String, f32>,
    /// From the session, set as monitors connect
    saved: BTreeMap<String, f32>,
    /// The monitors connected, by name, as last told
    connected: Vec<String>,
    /// Levels for the DDC thread to set, by I²C bus; only the latest of each is set
    ddc: Arc<(Mutex<HashMap<u32, f32>>, Condvar)>,
    ddc_started: bool,
}

/// How a monitor's brightness is set
#[derive(Clone, Copy, PartialEq)]
enum Method {
    Backlight,
    /// On this I²C bus
    Ddc(u32),
    Software,
}

impl AppletRunner {
    /// Key command `brightness(step)`: the screen under the pointer's, `step` percent up (negative: down)
    pub(crate) fn step_brightness(&mut self, step: i64) {
        let Some(monitor) = self.pointer_monitor() else {
            return;
        };
        let level = self.brightness_of(&monitor);
        self.set_brightness(&monitor, level + step as f32 / 100.0, true);
    }

    /// The hub's slider: the main screen's
    pub(crate) fn set_main_brightness(&mut self, level: f32) {
        if let Some(monitor) = self.monitors().iter().find(|monitor| monitor.primary) {
            let monitor = monitor.clone();
            self.set_brightness(&monitor, level, false);
        }
    }

    /// The session's levels (`StandardRequest::SavedBrightness`), set on the monitors connected
    pub(crate) fn saved_brightness(&mut self, saved: BTreeMap<String, f32>) {
        self.brightness.saved = saved;
        self.brightness.connected.clear();
        self.monitors_changed();
    }

    /// Monitors that connected get their saved level (not backlights: the system restores those), and the bar
    /// the main screen's
    pub(crate) fn monitors_changed(&mut self) {
        let monitors = self.monitors().to_vec();
        for monitor in &monitors {
            if self.brightness.connected.contains(&monitor.name) {
                continue;
            }
            if let Some(&level) = self.brightness.saved.get(&monitor.name)
                && method(monitor) != Method::Backlight
            {
                self.set_brightness(monitor, level, false);
            }
        }
        self.brightness.connected = monitors
            .iter()
            .map(|monitor| monitor.name.clone())
            .collect();
        if let Some(main) = monitors.iter().find(|monitor| monitor.primary) {
            let level = self.brightness_of(main);
            self.bar_client.send_event(StandardEvent::Brightness {
                screen: main.name.clone(),
                level,
                osd: false,
            });
        }
    }

    fn monitors(&self) -> &[MonitorInfo] {
        self.smithay_state.output_management.monitors()
    }

    /// The monitor of the screen under the pointer, else the main one
    fn pointer_monitor(&self) -> Option<MonitorInfo> {
        let [px, py] = self.pointer_position;
        let screen = self
            .smithay_state
            .screens
            .iter()
            .position(|&[x, y, width, height]| {
                (f64::from(x)..f64::from(x + width)).contains(&px)
                    && (f64::from(y)..f64::from(y + height)).contains(&py)
            })
            .unwrap_or_default();
        let name = self.screen_names.get(screen)?;
        self.monitors()
            .iter()
            .find(|monitor| &monitor.name == name)
            .cloned()
    }

    /// Its level as last set, else read (full for software)
    fn brightness_of(&mut self, monitor: &MonitorInfo) -> f32 {
        let method = method(monitor);
        // a backlight may change by other means (eg firmware keys)
        if method == Method::Backlight {
            return backlight()
                .and_then(|paths| read_backlight(&paths))
                .unwrap_or(1.0);
        }
        if let Some(&level) = self.brightness.levels.get(&monitor.name) {
            return level;
        }
        let level = match method {
            Method::Ddc(bus) => read_ddc(bus).unwrap_or(1.0),
            _ => 1.0,
        };
        self.brightness.levels.insert(monitor.name.clone(), level);
        level
    }

    /// Sets it (0 to 1, not capped above black), telling the root and the bar
    fn set_brightness(&mut self, monitor: &MonitorInfo, level: f32, osd: bool) {
        let level = level.clamp(0.0, 1.0);
        self.brightness.levels.insert(monitor.name.clone(), level);
        match method(monitor) {
            Method::Backlight => {
                let percent = format!("{:.0}%", level * 100.0);
                thread::spawn(move || run("brightnessctl", &["--quiet", "set", &percent]));
            }
            Method::Ddc(bus) => self.brightness.set_ddc(bus, level),
            Method::Software => self
                .ui_handle
                .set_software_brightness(monitor.monitor.connector.clone(), level),
        }
        let event = || StandardEvent::Brightness {
            screen: monitor.name.clone(),
            level,
            osd,
        };
        self.root_client.send_event(event());
        self.bar_client.send_event(event());
    }
}

impl Brightness {
    /// Queues it for the DDC thread (a write takes ~50 ms, so a slider drag only sets the latest)
    fn set_ddc(&mut self, bus: u32, level: f32) {
        let (pending, wake) = &*self.ddc;
        pending.lock().unwrap().insert(bus, level);
        wake.notify_one();
        if !std::mem::replace(&mut self.ddc_started, true) {
            let ddc = Arc::clone(&self.ddc);
            thread::spawn(move || {
                let (pending, wake) = &*ddc;
                loop {
                    let levels: Vec<(u32, f32)> = {
                        let mut pending = wake
                            .wait_while(pending.lock().unwrap(), |pending| pending.is_empty())
                            .unwrap();
                        pending.drain().collect()
                    };
                    for (bus, level) in levels {
                        let value = format!("{:.0}", level * 100.0);
                        run(
                            "ddcutil",
                            &["--bus", &bus.to_string(), "setvcp", "10", &value],
                        );
                    }
                }
            });
        }
    }
}

/// How the monitor's brightness is set
fn method(monitor: &MonitorInfo) -> Method {
    let connector = &monitor.monitor.connector;
    if ["eDP", "LVDS", "DSI"]
        .iter()
        .any(|panel| connector.starts_with(panel))
        && backlight().is_some()
    {
        return Method::Backlight;
    }
    let ddc = sonamu_common::settings::get()
        .displays
        .iter()
        .find(|(key, _)| {
            key.eq_ignore_ascii_case(&monitor.name) || key.eq_ignore_ascii_case(connector)
        })
        .is_some_and(|(_, display)| display.ddc);
    match ddc.then(|| i2c_bus(connector)).flatten() {
        Some(bus) => Method::Ddc(bus),
        None => Method::Software,
    }
}

/// Runs it, logging failures
fn run(program: &str, args: &[&str]) -> Option<String> {
    match Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) if output.status.success() => {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(output) => {
            eprintln!("{program} {args:?} failed: {}", output.status);
            None
        }
        Err(err) => {
            eprintln!("Can't run {program}: {err}");
            None
        }
    }
}

/// The first backlight's `[brightness, max_brightness]` files
fn backlight() -> Option<[PathBuf; 2]> {
    let dir = fs::read_dir("/sys/class/backlight")
        .ok()?
        .flatten()
        .next()?
        .path();
    Some([dir.join("brightness"), dir.join("max_brightness")])
}

fn read_backlight([now, max]: &[PathBuf; 2]) -> Option<f32> {
    let read = |path: &PathBuf| fs::read_to_string(path).ok()?.trim().parse::<u64>().ok();
    let (now, max) = (read(now)?, read(max)?);
    (max > 0).then(|| now as f32 / max as f32)
}

/// The I²C bus of the connector's DDC (`/sys/class/drm/card*-<connector>/ddc` → `i2c-<bus>`)
fn i2c_bus(connector: &str) -> Option<u32> {
    fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .find_map(|entry| {
            let name = entry.file_name();
            let (_, card_connector) = name.to_str()?.split_once('-')?;
            if card_connector != connector {
                return None;
            }
            let ddc = fs::read_link(entry.path().join("ddc")).ok()?;
            ddc.file_name()?
                .to_str()?
                .strip_prefix("i2c-")?
                .parse()
                .ok()
        })
}

/// The monitor's brightness from DDC
fn read_ddc(bus: u32) -> Option<f32> {
    parse_vcp(&run(
        "ddcutil",
        &["--bus", &bus.to_string(), "getvcp", "10", "--brief"],
    )?)
}

/// `VCP 10 C <current> <max>` as a level
fn parse_vcp(out: &str) -> Option<f32> {
    let mut numbers = out.split_whitespace().skip(3).map(str::parse::<f32>);
    let (now, max) = (numbers.next()?.ok()?, numbers.next()?.ok()?);
    (max > 0.0).then(|| now / max)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_ddcutil_brief() {
        assert_eq!(super::parse_vcp("VCP 10 C 30 100\n"), Some(0.3));
        assert_eq!(super::parse_vcp("VCP 10 ERR"), None);
    }
}
