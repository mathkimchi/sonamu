//! For things that are used by both the manager and subapps.

pub mod clipboard;
pub mod msg;
pub mod notification;
pub mod sap;
pub mod settings;
pub mod settings_file;
pub use sonamu_command as command;
/// REVIEW: this was path of least resistance. actually organize later
pub use sonamu_sync as sync;
pub mod utils;

/// Set by the runner to its wl socket's name.
/// Not `WAYLAND_DISPLAY` itself: see `AppletRunner::run`.
pub const WAYLAND_DISPLAY_ENV: &str = "SONAMU_WAYLAND_DISPLAY";

/// In the root's buffer mode (Super+Esc): every key goes to the root, and the runner skips settings and `keymap.set`
/// bindings that would take typing (keys with a symbol, without Super)
pub static BUFFER_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Applets recording a keybind (`Capture`s held): every key goes to the focused applet; settings, `keymap.set` and
/// the root's bindings are skipped. A count, so a new capture can start before the old one drops
pub static KEY_CAPTURE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Whether keys are being captured
pub fn capturing() -> bool {
    KEY_CAPTURE.load(std::sync::atomic::Ordering::Relaxed) > 0
}

/// Ends every capture, eg when focus leaves the applet capturing (its `Capture`s then do nothing)
pub fn end_capture() {
    KEY_CAPTURE.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// Holds `KEY_CAPTURE` on; dropping it (eg its applet closing) ends the capture
#[must_use = "the capture ends when it's dropped"]
pub struct Capture(());
impl Capture {
    pub fn start() -> Self {
        KEY_CAPTURE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self(())
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        let _ = KEY_CAPTURE.try_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |count| Some(count.saturating_sub(1)),
        );
    }
}

/// Sonamu runs on the monitor as the actual compositor, not in a window (eg a test run); set before any client starts
pub static ON_TTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Connects the process to Sonamu if it's a wl client.
/// Run from the dev shell, leaves out its `LD_LIBRARY_PATH`, which overrides clients' own libraries (#153: Signal
/// got the shell's OpenSSL instead of its BoringSSL).
pub fn connect_wl_client(command: &mut std::process::Command) -> &mut std::process::Command {
    if let Some(socket) = std::env::var_os(WAYLAND_DISPLAY_ENV) {
        command.env("WAYLAND_DISPLAY", socket);
    }
    if std::env::var_os("IN_NIX_SHELL").is_some() {
        command.env_remove("LD_LIBRARY_PATH");
    }
    command
}

/// Placeholder type: <https://www.reddit.com/r/rust/comments/1jbpfsh/todo_type>/
#[deprecated]
pub enum Todo {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new capture before the old one's dropped (eg recording another bind) keeps capturing
    #[test]
    fn overlapping_captures() {
        let first = Capture::start();
        let second = Capture::start();
        drop(first);
        assert!(capturing());
        drop(second);
        assert!(!capturing());
    }

    #[test]
    fn capture_ends_on_drop() {
        drop(Capture::start());
        assert!(!capturing());
    }
}
