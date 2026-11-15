use sonamu_common::{
    command::{self, Value},
    sap::packets::StandardRequest,
    settings,
};
use sonamu_ui::ui_event::UIEvent;

use crate::nodular_applet::{AppletContext, NodeRequest, project_applet::ProjectApplet};

/// Tried in order when neither the settings nor `$TERMINAL` give an installed terminal
const TERMINALS: [&str; 8] = [
    "foot",
    "alacritty",
    "kitty",
    "ghostty",
    "wezterm",
    "konsole",
    "gnome-terminal",
    "xterm",
];
/// Tried in order when neither the settings nor `$BROWSER` give an installed browser
const BROWSERS: [&str; 6] = [
    "firefox",
    "librewolf",
    "chromium",
    "google-chrome-stable",
    "google-chrome",
    "brave",
];

/// Tried in order for PDFs when the settings don't give a viewer: zathura starts fast and is keyboard driven
const PDF_VIEWERS: [&str; 3] = ["zathura", "okular", "evince"];

/// Started by Super+T (and a project's `+ terminal`)
#[must_use]
pub fn terminal() -> String {
    pick_program(settings::get().terminal, Some("TERMINAL"), &TERMINALS)
}
/// Started by Super+N (and a project's `+ browser`)
#[must_use]
pub fn browser() -> String {
    pick_program(settings::get().browser, Some("BROWSER"), &BROWSERS)
}
/// Opens PDFs in the file manager (its window becomes the document)
#[must_use]
pub fn pdf_viewer() -> String {
    pick_program(settings::get().pdf_viewer, None, &PDF_VIEWERS)
}

/// The setting, else `$var` if installed, else the first installed of `fallbacks` (else the first)
fn pick_program(setting: Option<String>, var: Option<&str>, fallbacks: &[&str]) -> String {
    setting
        .or_else(|| {
            std::env::var(var?)
                .ok()
                .filter(|program| installed(program))
        })
        .or_else(|| {
            fallbacks
                .iter()
                .find(|program| installed(program))
                .map(|program| (*program).to_owned())
        })
        .unwrap_or_else(|| fallbacks[0].to_owned())
}

/// Whether `program` is a file, or in a `$PATH` directory if it has no `/`
fn installed(program: &str) -> bool {
    if program.contains('/') {
        return std::path::Path::new(program).is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// A standard keybind's node command (see `.claude/COMMANDS.md`), with its args
pub struct KeybindCommand {
    pub name: String,
    pub args: Vec<Value>,
}
impl KeybindCommand {
    /// The same as a request to the node (eg for applets that handle some keybinds differently)
    #[must_use]
    pub fn request(&self) -> Option<NodeRequest> {
        Some(match (self.name.as_str(), self.args.as_slice()) {
            ("quit", []) => StandardRequest::Quit.into(),
            ("wrap_project", []) => NodeRequest::Wrap(ProjectApplet::wrapper()),
            ("close", []) => NodeRequest::Close,
            ("toggle_half", []) => NodeRequest::ToggleHalf,
            ("spawn", [Value::String(program)]) => NodeRequest::Run(program.clone()),
            ("spawn_sibling", [Value::String(program)]) => NodeRequest::RunSibling(program.clone()),
            ("swap_sibling", [Value::Number(offset)]) => NodeRequest::SwapSibling(*offset as isize),
            _ => return None,
        })
    }
}

/// Does the standard keybinds (see `standard_keybind`) as commands on the applet's node (as requests until it
/// knows its node). Returns whether the key was one.
pub fn handle_standard_keybinds(ui_event: &UIEvent, hook: &AppletContext) -> bool {
    let Some(keybind) = standard_keybind(ui_event) else {
        return false;
    };
    match hook.uid() {
        Some(uid) => command::send(uid, &keybind.name, keybind.args),
        None => {
            if let Some(request) = keybind.request() {
                hook.request(request);
            }
        }
    }
    true
}

/// The request a standard keybind makes to the applet's node, if `ui_event` is one.
/// For applets that handle some keybinds differently (eg `WlSurfaceApplet`'s Super+W).
pub fn standard_keybind_request(ui_event: &UIEvent) -> Option<NodeRequest> {
    standard_keybind(ui_event)?.request()
}

/// The keybind (setting `keybinds`) for this key, if it's a node command: `quit`, `wrap_project`, `close`,
/// `toggle_half`, `spawn(program)`, `spawn_sibling(program)`, `swap_sibling(offset)`, and `spawn_terminal`,
/// `spawn_browser` (`terminal()`, `browser()`; `_sibling` too).
///
/// Traversal and the root's own keybinds are the root's (`nodular_applet::traversal`, `RootKeybind`).
#[must_use]
pub fn standard_keybind(ui_event: &UIEvent) -> Option<KeybindCommand> {
    let &UIEvent::Key { modifiers, .. } = ui_event else {
        return None;
    };
    let (name, args) = settings::keybind_command(ui_event, modifiers)?;
    let spawn = |name: &str, program| KeybindCommand {
        name: name.to_owned(),
        args: vec![Value::String(program)],
    };
    Some(match name.as_str() {
        "spawn_terminal" => spawn("spawn", terminal()),
        "spawn_terminal_sibling" => spawn("spawn_sibling", terminal()),
        "spawn_browser" => spawn("spawn", browser()),
        "spawn_browser_sibling" => spawn("spawn_sibling", browser()),
        "quit" | "wrap_project" | "close" | "toggle_half" | "spawn" | "spawn_sibling"
        | "swap_sibling" => KeybindCommand {
            args: args.into_iter().map(json_value).collect(),
            name,
        },
        _ => return None,
    })
}

/// A keybind's arg as a command's
fn json_value(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Nil,
        serde_json::Value::Bool(value) => Value::Bool(value),
        serde_json::Value::Number(value) => Value::Number(value.as_f64().unwrap_or_default()),
        serde_json::Value::String(value) => Value::String(value),
        serde_json::Value::Array(values) => {
            Value::List(values.into_iter().map(json_value).collect())
        }
        serde_json::Value::Object(map) => Value::Map(
            map.into_iter()
                .map(|(key, value)| (key, json_value(value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use crate::nodular_applet::traversal::Traversal;
    use sonamu_common::settings::{Action, Settings};

    /// Every default keybind's command is one something handles
    #[test]
    fn default_keybinds_are_known() {
        for keybind in Settings::default().all_keybinds().cloned() {
            let Action::Command { command, args } = keybind.action else {
                continue;
            };
            let known = match command.as_str() {
                "traverse" => Traversal::from_args(&args).is_some(),
                "swipe" => matches!(
                    args.first().and_then(|arg| arg.as_str()),
                    Some("left" | "right" | "up" | "down")
                ),
                _ => [
                    "quit",
                    "wrap_project",
                    "close",
                    "toggle_half",
                    "spawn_terminal",
                    "spawn_terminal_sibling",
                    "spawn_browser",
                    "spawn_browser_sibling",
                    "swap_sibling",
                    "pluck_place",
                    "buffer_mode",
                    "help",
                    "view_root",
                    "view_parent",
                ]
                .contains(&command.as_str()),
            };
            assert!(known, "{command} {args:?}");
        }
    }
}
