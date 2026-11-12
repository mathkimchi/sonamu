//! What the bar shows about the machine: battery, power profile, cpu, memory and processes, read on a thread from
//! `/sys` and `/proc` (and power-profiles-daemon over the system bus).

use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// How often it's read
const INTERVAL: Duration = Duration::from_secs(2);
/// Processes listed at most
pub const PROCESSES: usize = 15;
/// Clock ticks per second of `/proc`'s cpu times (`USER_HZ`, 100 on Linux)
const TICKS: f64 = 100.0;

#[derive(Debug, Clone, Default)]
pub struct Battery {
    pub percent: u8,
    /// `Charging`, `Discharging`, `Full`, `Not charging`
    pub status: String,
    /// Into the battery (negative while discharging)
    pub watts: Option<f64>,
    /// Until empty while discharging, until full while charging
    pub time_left: Option<Duration>,
}
impl Battery {
    pub fn charging(&self) -> bool {
        self.status == "Charging"
    }
}

/// power-profiles-daemon's profiles, in the picker's order
pub const PROFILES: [&str; 3] = ["power-saver", "balanced", "performance"];

#[derive(Debug, Clone)]
pub struct Process {
    pub name: String,
    /// Of one core, like `top`
    pub cpu: f64,
    /// Resident, in bytes
    pub mem: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub battery: Option<Battery>,
    /// One of `PROFILES`; `None` without power-profiles-daemon
    pub profile: Option<String>,
    /// Percent of all cores
    pub cpu: f64,
    /// Percent used
    pub mem: f64,
    /// Every process (read only while `System::want_processes`)
    pub processes: Vec<Process>,
}

/// The reading thread's results, shared with the bar
pub struct System {
    pub snapshot: Arc<Mutex<Snapshot>>,
    /// The process list is read only while its popup is open
    want_processes: Arc<AtomicBool>,
    changed: Arc<dyn Fn() + Send + Sync>,
}

impl System {
    /// Starts reading every `INTERVAL`, calling `changed` after each read; sets the power saver profile first
    /// (the default asked for in #150)
    pub fn start(changed: impl Fn() + Send + Sync + 'static) -> Self {
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let want_processes = Arc::new(AtomicBool::new(false));
        let changed: Arc<dyn Fn() + Send + Sync> = Arc::new(changed);
        {
            let snapshot = Arc::clone(&snapshot);
            let want_processes = Arc::clone(&want_processes);
            let changed = Arc::clone(&changed);
            thread::spawn(move || {
                set_profile(PROFILES[0]);
                let mut reader = Reader {
                    profiles: profiles_proxy(),
                    ..Reader::default()
                };
                loop {
                    let read = reader.read(want_processes.load(Ordering::Relaxed));
                    *snapshot.lock().unwrap() = read;
                    changed();
                    thread::sleep(INTERVAL);
                }
            });
        }
        Self {
            snapshot,
            want_processes,
            changed,
        }
    }

    pub fn set_want_processes(&self, want: bool) {
        self.want_processes.store(want, Ordering::Relaxed);
    }

    /// Sets the power profile (on a thread), shown at once
    pub fn set_profile(&self, profile: &'static str) {
        self.snapshot.lock().unwrap().profile = Some(profile.to_string());
        let changed = Arc::clone(&self.changed);
        thread::spawn(move || {
            set_profile(profile);
            changed();
        });
    }
}

/// power-profiles-daemon's (`None` without it)
fn profiles_proxy() -> Option<zbus::Proxy<'static>> {
    let connection = zbus::block_on(zbus::Connection::system()).ok()?;
    zbus::block_on(
        zbus::proxy::Builder::new(&connection)
            .destination("org.freedesktop.UPower.PowerProfiles")
            .path("/org/freedesktop/UPower/PowerProfiles")
            .interface("org.freedesktop.UPower.PowerProfiles")
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build(),
    )
    .inspect_err(|err| log::debug!("No power profiles: {err}"))
    .ok()
}

fn set_profile(profile: &str) {
    let Some(proxy) = profiles_proxy() else {
        return;
    };
    if let Err(err) = zbus::block_on(proxy.set_property("ActiveProfile", profile)) {
        log::warn!("Couldn't set the power profile to {profile}: {err}");
    }
}

/// Keeps the last cpu times, for usage since then
#[derive(Default)]
struct Reader {
    /// `/proc/stat`'s busy and total ticks
    cpu: Option<(u64, u64)>,
    /// Each pid's ticks, and when read
    processes: HashMap<u32, u64>,
    processes_read: Option<Instant>,
    profiles: Option<zbus::Proxy<'static>>,
}

impl Reader {
    fn read(&mut self, processes: bool) -> Snapshot {
        Snapshot {
            battery: battery(),
            profile: self.profile(),
            cpu: self.cpu(),
            mem: mem().unwrap_or(0.0),
            processes: if processes {
                self.processes()
            } else {
                self.processes_read = None;
                Vec::new()
            },
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "tick counts")]
    fn cpu(&mut self) -> f64 {
        let Some(line) = fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|stat| stat.lines().next().map(str::to_string))
        else {
            return 0.0;
        };
        let ticks: Vec<u64> = line
            .split_whitespace()
            .skip(1)
            .filter_map(|n| n.parse().ok())
            .collect();
        let total: u64 = ticks.iter().sum();
        // idle and iowait
        let idle = ticks.get(3).copied().unwrap_or(0) + ticks.get(4).copied().unwrap_or(0);
        let busy = total - idle;
        let usage = self.cpu.map_or(0.0, |(last_busy, last_total)| {
            let total = total.saturating_sub(last_total);
            if total == 0 {
                0.0
            } else {
                busy.saturating_sub(last_busy) as f64 / total as f64 * 100.0
            }
        });
        self.cpu = Some((busy, total));
        usage
    }

    #[expect(clippy::cast_precision_loss, reason = "tick counts")]
    fn processes(&mut self) -> Vec<Process> {
        let elapsed = self.processes_read.map(|read| read.elapsed().as_secs_f64());
        self.processes_read = Some(Instant::now());
        let page = page_size();
        let mut ticks = HashMap::new();
        let mut processes = Vec::new();
        for entry in fs::read_dir("/proc").into_iter().flatten().flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            let path = entry.path();
            let Some((name, used)) = process_stat(&path) else {
                continue;
            };
            let mem = fs::read_to_string(path.join("statm"))
                .ok()
                .and_then(|statm| statm.split_whitespace().nth(1)?.parse::<u64>().ok())
                .unwrap_or(0)
                * page;
            let cpu = match (elapsed, self.processes.get(&pid)) {
                (Some(elapsed), Some(&last)) if elapsed > 0.0 => {
                    used.saturating_sub(last) as f64 / TICKS / elapsed * 100.0
                }
                _ => 0.0,
            };
            ticks.insert(pid, used);
            processes.push(Process { name, cpu, mem });
        }
        self.processes = ticks;
        processes
    }
}

/// A process's name and cpu ticks (user + system) from its `stat`
fn process_stat(path: &Path) -> Option<(String, u64)> {
    let stat = fs::read_to_string(path.join("stat")).ok()?;
    // the name is in parentheses and may contain spaces
    let (start, end) = (stat.find('(')?, stat.rfind(')')?);
    let name = stat[start + 1..end].to_string();
    let fields: Vec<&str> = stat[end + 1..].split_whitespace().collect();
    // utime and stime are fields 14 and 15, the 12th and 13th after the name
    let used = fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
    Some((name, used))
}

fn page_size() -> u64 {
    // SAFETY: sysconf has no preconditions
    u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).unwrap_or(4096)
}

/// Percent of memory used (`MemTotal - MemAvailable`)
#[expect(clippy::cast_precision_loss, reason = "kB counts")]
fn mem() -> Option<f64> {
    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
    let field = |name: &str| -> Option<u64> {
        meminfo
            .lines()
            .find(|line| line.starts_with(name))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    };
    let (total, available) = (field("MemTotal:")?, field("MemAvailable:")?);
    Some((total - available) as f64 / total as f64 * 100.0)
}

impl Reader {
    fn profile(&self) -> Option<String> {
        zbus::block_on(self.profiles.as_ref()?.get_property::<String>("ActiveProfile")).ok()
    }
}

/// The first battery in `/sys/class/power_supply`
#[expect(clippy::cast_precision_loss, reason = "µW and µWh")]
fn battery() -> Option<Battery> {
    let path = fs::read_dir("/sys/class/power_supply")
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| read(path, "type").as_deref() == Some("Battery"))?;
    let number = |name: &str| read(&path, name)?.parse::<u64>().ok();
    let status = read(&path, "status").unwrap_or_default();
    // energy (µWh, µW) or charge (µAh, µA); the rate is unsigned
    let (now, full, rate) = match (number("energy_now"), number("energy_full")) {
        (Some(now), Some(full)) => (now, full, number("power_now")),
        _ => (
            number("charge_now")?,
            number("charge_full")?,
            number("current_now"),
        ),
    };
    let watts = number("power_now")
        .map(|uw| uw as f64 / 1e6)
        .or_else(|| Some(number("current_now")? as f64 * number("voltage_now")? as f64 / 1e12));
    let discharging = status == "Discharging";
    let time_left = rate.filter(|&rate| rate > 0).and_then(|rate| {
        let left = if discharging {
            now
        } else if status == "Charging" {
            full.saturating_sub(now)
        } else {
            return None;
        };
        Some(Duration::from_secs_f64(left as f64 / rate as f64 * 3600.0))
    });
    Some(Battery {
        percent: number("capacity").map_or(0, |c| u8::try_from(c.min(100)).unwrap_or(100)),
        watts: watts.map(|w| if discharging { -w } else { w }),
        status,
        time_left,
    })
}

fn read(path: &Path, name: &str) -> Option<String> {
    fs::read_to_string(path.join(name))
        .map(|value| value.trim().to_string())
        .ok()
}
