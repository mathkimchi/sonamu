//! For things that are used by both the manager and subapps.

pub mod clipboard;
pub mod msg;
pub mod notification;
pub mod sap;
pub mod settings;
pub use sonamu_command as command;
/// REVIEW: this was path of least resistance. actually organize later
pub use sonamu_sync as sync;
pub mod utils;

/// Set by the runner to its wl socket's name.
/// Not `WAYLAND_DISPLAY` itself: see `AppletRunner::run`.
pub const WAYLAND_DISPLAY_ENV: &str = "SONAMU_WAYLAND_DISPLAY";

/// In the root's buffer mode (Super+Esc): every key goes to its buffer line, and the runner skips settings and
/// `keymap.set` bindings that would take typing (keys with a symbol, without Super)
pub static BUFFER_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Connects the process to Sonamu if it's a wl client
pub fn connect_wl_client(command: &mut std::process::Command) -> &mut std::process::Command {
    if let Some(socket) = std::env::var_os(WAYLAND_DISPLAY_ENV) {
        command.env("WAYLAND_DISPLAY", socket);
    }
    command
}

/// Placeholder type: <https://www.reddit.com/r/rust/comments/1jbpfsh/todo_type>/
#[deprecated]
pub enum Todo {}
