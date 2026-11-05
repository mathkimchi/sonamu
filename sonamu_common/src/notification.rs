//! Desktop notifications: the freedesktop Notifications D-Bus API, which the notification daemon (eg DMS) shows.
//! Sent with `busctl`, like the runner's other D-Bus calls, so there is no D-Bus crate.

use std::{
    process::Command,
    sync::atomic::{AtomicU32, Ordering},
    thread,
    time::Duration,
};

/// Tries this many times, a second apart, since the daemon may not be up yet (eg DMS still starting)
const ATTEMPTS: u32 = 10;

/// A notification that replaces the last one sent through it, rather than piling up (eg settings reloads)
pub struct Replacing(AtomicU32);
impl Replacing {
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicU32::new(0))
    }

    /// Shows `summary` and `body` on a thread, in place of this one's last notification if still shown.
    /// `critical` ones stay until dismissed.
    pub fn send(&'static self, summary: String, body: String, critical: bool) {
        thread::spawn(move || {
            for attempt in 0..ATTEMPTS {
                if attempt > 0 {
                    thread::sleep(Duration::from_secs(1));
                }
                match notify(&summary, &body, critical, self.0.load(Ordering::Relaxed)) {
                    Ok(id) => {
                        self.0.store(id, Ordering::Relaxed);
                        return;
                    }
                    Err(err) => log::debug!("Notification not sent yet: {err}"),
                }
            }
            log::warn!("Couldn't send the notification `{summary}`");
        });
    }
}
impl Default for Replacing {
    fn default() -> Self {
        Self::new()
    }
}

/// Sends a notification (replacing `replaces` unless it is 0); its id
fn notify(summary: &str, body: &str, critical: bool, replaces: u32) -> Result<u32, String> {
    let output = Command::new("busctl")
        .args(["--user", "call", "--"])
        .args([
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
            "Notify",
            "susssasa{sv}i",
            "Sonamu",
            &replaces.to_string(),
            "",
            summary,
            body,
            // no actions
            "0",
        ])
        // hints: urgency (2 is critical), then the daemon's default timeout
        .args(if critical {
            &["1", "urgency", "y", "2"][..]
        } else {
            &["0"][..]
        })
        .arg("-1")
        .output()
        .map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    // `u 42`
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .nth(1)
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| "no id in the reply".to_string())
}
