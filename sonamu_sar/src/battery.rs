//! Low battery warnings (#135): a desktop notification at 20%, then a critical one at 10%, while discharging.

use sonamu_common::notification::Replacing;
use std::{fs, path::Path, thread, time::Duration};

/// Warned at each, highest first; the last is critical
const THRESHOLDS: [u8; 2] = [20, 10];
const CHECK_INTERVAL: Duration = Duration::from_secs(30);

static NOTIFICATION: Replacing = Replacing::new();

/// Checks the batteries on a thread for as long as the process runs
pub fn watch() {
    thread::spawn(|| {
        // the lowest threshold warned about since it last charged
        let mut warned = None;
        loop {
            if let Some((capacity, discharging)) = read() {
                let warning = warning(capacity, discharging, warned);
                if let Some(threshold) = warning.filter(|&threshold| Some(threshold) != warned) {
                    notify(capacity, threshold);
                }
                warned = warning;
            }
            thread::sleep(CHECK_INTERVAL);
        }
    });
}

/// The lowest threshold `capacity` is at or under while discharging, or the one already warned about if lower
/// (`None` while charging, so the next discharge warns again)
fn warning(capacity: u8, discharging: bool, warned: Option<u8>) -> Option<u8> {
    if !discharging {
        return None;
    }
    let reached = THRESHOLDS
        .iter()
        .rev()
        .find(|&&threshold| capacity <= threshold)
        .copied();
    match (reached, warned) {
        (Some(reached), Some(warned)) => Some(reached.min(warned)),
        (reached, warned) => reached.or(warned),
    }
}

fn notify(capacity: u8, threshold: u8) {
    let critical = threshold == THRESHOLDS[THRESHOLDS.len() - 1];
    NOTIFICATION.send(
        format!("Battery low: {capacity}%"),
        if critical {
            "Plug it in now"
        } else {
            "Plug it in soon"
        }
        .to_string(),
        critical,
    );
}

/// The batteries' average charge (percent), and whether any is discharging; `None` without a battery
fn read() -> Option<(u8, bool)> {
    let read = |path: &Path, name: &str| {
        fs::read_to_string(path.join(name))
            .map(|value| value.trim().to_string())
            .ok()
    };
    let batteries: Vec<(u32, bool)> = fs::read_dir("/sys/class/power_supply")
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| read(path, "type").as_deref() == Some("Battery"))
        .filter_map(|path| {
            let capacity = read(&path, "capacity")?.parse().ok()?;
            Some((
                capacity,
                read(&path, "status").as_deref() == Some("Discharging"),
            ))
        })
        .collect();
    let count = u32::try_from(batteries.len())
        .ok()
        .filter(|&count| count > 0)?;
    let average = batteries.iter().map(|&(capacity, _)| capacity).sum::<u32>() / count;
    Some((
        u8::try_from(average).ok()?,
        batteries.iter().any(|&(_, discharging)| discharging),
    ))
}

#[cfg(test)]
mod tests {
    use super::warning;

    #[test]
    fn warns_once_per_threshold() {
        assert_eq!(warning(50, true, None), None);
        // the 20% one, not again at 19%
        assert_eq!(warning(20, true, None), Some(20));
        assert_eq!(warning(19, true, Some(20)), Some(20));
        // then the 10% one
        assert_eq!(warning(10, true, Some(20)), Some(10));
        assert_eq!(warning(5, true, Some(10)), Some(10));
        // charging resets
        assert_eq!(warning(5, false, Some(10)), None);
    }
}
