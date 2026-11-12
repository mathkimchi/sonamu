//! The hub's controls, through their CLIs: volume and output (`wpctl`, changes followed with `pactl subscribe`),
//! brightness (`/sys/class/backlight`, set with `brightnessctl`), wifi (`nmcli`), bluetooth (`bluetoothctl`)

use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// How often brightness is read (to show the OSD when a key changes it)
const BRIGHTNESS_INTERVAL: Duration = Duration::from_millis(150);
/// How often wifi, bluetooth and outputs are read while the hub is open
const LISTS_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Volume {
    /// 0 to 1 (can go above with boosting)
    pub level: f32,
    pub muted: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Network {
    pub ssid: String,
    /// 0–100
    pub signal: u8,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub mac: String,
    pub name: String,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sink {
    pub id: String,
    pub name: String,
    pub default: bool,
}

/// What the OSD shows
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Osd {
    Volume,
    Brightness,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub volume: Option<Volume>,
    /// 0 to 1
    pub brightness: Option<f32>,
    pub wifi_enabled: bool,
    pub networks: Vec<Network>,
    pub bluetooth_powered: bool,
    pub devices: Vec<Device>,
    pub sinks: Vec<Sink>,
    /// Shown since then
    pub osd: Option<(Osd, Instant)>,
}
impl State {
    pub fn active_network(&self) -> Option<&Network> {
        self.networks.iter().find(|network| network.active)
    }
}

pub struct Controls {
    pub state: Arc<Mutex<State>>,
    /// Lists are read only while the hub is open
    want_lists: Arc<AtomicBool>,
    changed: Arc<dyn Fn() + Send + Sync>,
}

impl Controls {
    pub fn start(changed: Arc<dyn Fn() + Send + Sync>) -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let want_lists = Arc::new(AtomicBool::new(false));
        spawn_watch_brightness(Arc::clone(&state), Arc::clone(&changed));
        spawn_watch_volume(Arc::clone(&state), Arc::clone(&changed));
        {
            let (state, changed, want_lists) = (
                Arc::clone(&state),
                Arc::clone(&changed),
                Arc::clone(&want_lists),
            );
            thread::spawn(move || {
                // once at start, for the bar's icons
                read_lists(&state);
                changed();
                loop {
                    thread::sleep(LISTS_INTERVAL);
                    if want_lists.load(Ordering::Relaxed) {
                        read_lists(&state);
                        changed();
                    }
                }
            });
        }
        Self {
            state,
            want_lists,
            changed,
        }
    }

    /// Reads the lists now and while `want` (the hub is open)
    pub fn set_want_lists(&self, want: bool) {
        let was = self.want_lists.swap(want, Ordering::Relaxed);
        if want && !was {
            self.run(|| {});
        }
    }

    /// Runs `action` on a thread, then reads everything again
    fn run<T>(&self, action: impl FnOnce() -> T + Send + 'static) {
        let (state, changed) = (Arc::clone(&self.state), Arc::clone(&self.changed));
        thread::spawn(move || {
            action();
            read_lists(&state);
            changed();
        });
    }

    pub fn set_volume(&self, level: f32) {
        if let Some(volume) = &mut self.state.lock().unwrap().volume {
            volume.level = level;
        }
        self.run(move || {
            command(
                "wpctl",
                &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{level:.2}")],
            );
            command("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "0"]);
        });
    }

    pub fn toggle_mute(&self) {
        self.run(|| command("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]));
    }

    pub fn set_brightness(&self, level: f32) {
        self.state.lock().unwrap().brightness = Some(level);
        let percent = format!("{:.0}%", level * 100.0);
        self.run(move || command("brightnessctl", &["--quiet", "set", &percent]));
    }

    pub fn toggle_wifi(&self) {
        let on = !self.state.lock().unwrap().wifi_enabled;
        self.run(move || command("nmcli", &["radio", "wifi", if on { "on" } else { "off" }]));
    }

    /// Known networks connect; others need a password, which this can't ask for yet
    pub fn connect_wifi(&self, ssid: String) {
        self.run(move || command("nmcli", &["device", "wifi", "connect", &ssid]));
    }

    pub fn toggle_bluetooth(&self) {
        let on = !self.state.lock().unwrap().bluetooth_powered;
        self.run(move || command("bluetoothctl", &["power", if on { "on" } else { "off" }]));
    }

    pub fn toggle_device(&self, device: &Device) {
        let (mac, connected) = (device.mac.clone(), device.connected);
        self.run(move || {
            command(
                "bluetoothctl",
                &[if connected { "disconnect" } else { "connect" }, &mac],
            );
        });
    }

    pub fn set_default_sink(&self, id: String) {
        self.run(move || command("wpctl", &["set-default", &id]));
    }
}

/// Runs it, its output if it worked
fn command(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .inspect_err(|err| log::debug!("Couldn't run {program}: {err}"))
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
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

#[expect(clippy::cast_precision_loss, reason = "brightness steps")]
fn read_brightness([now, max]: &[PathBuf; 2]) -> Option<f32> {
    let read = |path: &PathBuf| fs::read_to_string(path).ok()?.trim().parse::<u64>().ok();
    let (now, max) = (read(now)?, read(max)?);
    (max > 0).then(|| now as f32 / max as f32)
}

/// Polls brightness; a change shows the OSD
fn spawn_watch_brightness(state: Arc<Mutex<State>>, changed: Arc<dyn Fn() + Send + Sync>) {
    thread::spawn(move || {
        let Some(paths) = backlight() else {
            return;
        };
        let mut last = read_brightness(&paths);
        state.lock().unwrap().brightness = last;
        loop {
            thread::sleep(BRIGHTNESS_INTERVAL);
            let now = read_brightness(&paths);
            if now != last {
                last = now;
                let mut state = state.lock().unwrap();
                state.brightness = now;
                state.osd = Some((Osd::Brightness, Instant::now()));
                drop(state);
                changed();
            }
        }
    });
}

fn read_volume() -> Option<Volume> {
    // `Volume: 0.40` or `Volume: 0.40 [MUTED]`
    let out = command("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])?;
    let level = out.split_whitespace().nth(1)?.parse().ok()?;
    Some(Volume {
        level,
        muted: out.contains("[MUTED]"),
    })
}

/// Follows sink changes with `pactl subscribe`; a change shows the OSD
fn spawn_watch_volume(state: Arc<Mutex<State>>, changed: Arc<dyn Fn() + Send + Sync>) {
    thread::spawn(move || {
        state.lock().unwrap().volume = read_volume();
        let child = Command::new("pactl")
            .arg("subscribe")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = child else {
            log::debug!("No pactl: volume changes won't show");
            return;
        };
        let Some(stdout) = child.stdout.take() else {
            return;
        };
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            // `Event 'change' on sink #56` (not `sink-input`), or the server (the default sink changed)
            if !(line.contains("on sink #") || line.contains("on server")) {
                continue;
            }
            let volume = read_volume();
            let mut state = state.lock().unwrap();
            if volume != state.volume {
                state.volume = volume;
                state.osd = Some((Osd::Volume, Instant::now()));
                drop(state);
                changed();
            }
        }
        let _ = child.wait();
    });
}

fn read_lists(state: &Mutex<State>) {
    let wifi_enabled =
        command("nmcli", &["radio", "wifi"]).is_some_and(|out| out.trim() == "enabled");
    let networks = if wifi_enabled { networks() } else { Vec::new() };
    let bluetooth_powered = command("bluetoothctl", &["show"])
        .is_some_and(|out| out.lines().any(|line| line.trim() == "Powered: yes"));
    let devices = if bluetooth_powered {
        devices()
    } else {
        Vec::new()
    };
    let sinks = sinks();
    let volume = read_volume();
    let mut state = state.lock().unwrap();
    state.wifi_enabled = wifi_enabled;
    state.networks = networks;
    state.bluetooth_powered = bluetooth_powered;
    state.devices = devices;
    state.sinks = sinks;
    state.volume = volume;
}

/// Visible networks, strongest first, one per SSID (the active one kept)
fn networks() -> Vec<Network> {
    let Some(out) = command(
        "nmcli",
        &["-t", "-f", "ACTIVE,SIGNAL,SSID", "device", "wifi", "list"],
    ) else {
        return Vec::new();
    };
    let mut networks: Vec<Network> = out
        .lines()
        .filter_map(|line| {
            // the SSID last: it may hold `:` (escaped as `\:`)
            let mut fields = line.splitn(3, ':');
            let active = fields.next()? == "yes";
            let signal = fields.next()?.parse().ok()?;
            let ssid = fields.next()?.replace("\\:", ":");
            (!ssid.is_empty()).then_some(Network {
                ssid,
                signal,
                active,
            })
        })
        .collect();
    networks.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)));
    let mut seen = std::collections::HashSet::new();
    networks.retain(|network| seen.insert(network.ssid.clone()));
    networks
}

/// Paired devices, connected first
fn devices() -> Vec<Device> {
    let Some(out) = command("bluetoothctl", &["devices", "Paired"]) else {
        return Vec::new();
    };
    let connected = command("bluetoothctl", &["devices", "Connected"]).unwrap_or_default();
    let mut devices: Vec<Device> = out
        .lines()
        .filter_map(|line| {
            // `Device A0:0C:E2:12:C4:EC OpenRun Pro 2 by Shokz`
            let mut words = line.splitn(3, ' ');
            (words.next()? == "Device").then_some(())?;
            let mac = words.next()?.to_string();
            let name = words.next().unwrap_or(&mac).to_string();
            Some(Device {
                connected: connected.contains(&mac),
                mac,
                name,
            })
        })
        .collect();
    devices.sort_by_key(|device| !device.connected);
    devices
}

/// The audio sinks in `wpctl status` (under `Audio`, `Sinks:`), `*` marking the default
fn sinks() -> Vec<Sink> {
    let Some(out) = command("wpctl", &["status"]) else {
        return Vec::new();
    };
    let mut sinks = Vec::new();
    let mut in_audio = false;
    let mut in_sinks = false;
    for line in out.lines() {
        if !line.starts_with([' ', '│', '├', '└']) {
            in_audio = line.trim() == "Audio";
            in_sinks = false;
            continue;
        }
        let text = line.trim_start_matches([' ', '│', '├', '└', '─']).trim();
        if text.ends_with(':') {
            in_sinks = in_audio && text == "Sinks:";
            continue;
        }
        if !in_sinks || text.is_empty() {
            continue;
        }
        // `* 56. Built-in Audio Analog Stereo [vol: 0.40]`
        let default = text.starts_with('*');
        let text = text.trim_start_matches('*').trim();
        let Some((id, rest)) = text.split_once(". ") else {
            continue;
        };
        let name = rest
            .split(" [vol:")
            .next()
            .unwrap_or(rest)
            .trim()
            .to_string();
        sinks.push(Sink {
            id: id.to_string(),
            name,
            default,
        });
    }
    sinks
}
