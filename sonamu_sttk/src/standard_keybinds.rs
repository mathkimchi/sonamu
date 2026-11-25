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

/// What a keybind command's `args` are
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgKind {
    None,
    /// A program to run
    Program,
    /// A signed step (eg -1 sibling, -5 percent)
    Offset,
    /// `left`, `right`, `up` or `down`
    Direction,
    /// A kind from `TRAVERSAL_KINDS`, with a number when it takes one
    Traversal,
}
impl ArgKind {
    #[must_use]
    pub fn accepts(self, args: &[serde_json::Value]) -> bool {
        match self {
            Self::None => args.is_empty(),
            Self::Program => matches!(args, [program] if program.is_string()),
            Self::Offset => matches!(args, [offset] if offset.is_i64()),
            Self::Direction => matches!(
                args,
                [direction] if matches!(direction.as_str(), Some("left" | "right" | "up" | "down"))
            ),
            Self::Traversal => {
                crate::nodular_applet::traversal::Traversal::from_args(args).is_some()
            }
        }
    }
}

/// `traverse`'s kinds, and whether each takes a number (`child` n, `sibling` offset)
pub const TRAVERSAL_KINDS: &[(&str, bool)] = &[
    ("next", false),
    ("prev", false),
    ("parent", false),
    ("child", true),
    ("recent_child", false),
    ("last_child", false),
    ("sibling", true),
    ("select", false),
    ("enter", false),
];

/// A command a keybind can run
pub struct CommandSpec {
    pub name: &'static str,
    pub args: ArgKind,
}

/// Every command a keybind can run (see `.claude/SETTINGS.md`): the node's (`standard_keybind`), the root's
/// (`RootKeybind`, `buffer_mode`, `traverse`) and the runner's (`pluck_place`)
pub const KEYBIND_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "quit",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "help",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "buffer_mode",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "spawn_terminal",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "spawn_terminal_sibling",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "spawn_browser",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "spawn_browser_sibling",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "spawn",
        args: ArgKind::Program,
    },
    CommandSpec {
        name: "spawn_sibling",
        args: ArgKind::Program,
    },
    CommandSpec {
        name: "close",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "restore_closed",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "history",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "wrap_project",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "toggle_half",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "swap_sibling",
        args: ArgKind::Offset,
    },
    CommandSpec {
        name: "pluck_place",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "brightness",
        args: ArgKind::Offset,
    },
    CommandSpec {
        name: "view_root",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "view_parent",
        args: ArgKind::None,
    },
    CommandSpec {
        name: "swipe",
        args: ArgKind::Direction,
    },
    CommandSpec {
        name: "traverse",
        args: ArgKind::Traversal,
    },
];

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
            args: args.into_iter().map(Value::from).collect(),
            name,
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sonamu_common::settings::{Action, Settings};

    /// Every default keybind's command is in the catalog, with args it accepts
    #[test]
    fn default_keybinds_are_known() {
        for keybind in Settings::default().all_keybinds().cloned() {
            let Action::Command { command, args } = keybind.action else {
                continue;
            };
            let spec = KEYBIND_COMMANDS.iter().find(|spec| spec.name == command);
            assert!(
                spec.is_some_and(|spec| spec.args.accepts(&args)),
                "{command} {args:?}"
            );
        }
    }

    /// Each traversal kind parses, with a number when it takes one
    #[test]
    fn traversal_kinds_parse() {
        for &(kind, number) in TRAVERSAL_KINDS {
            let args = if number {
                json!([kind, 1])
            } else {
                json!([kind])
            };
            assert!(
                ArgKind::Traversal.accepts(args.as_array().unwrap()),
                "{kind}"
            );
        }
        assert!(!ArgKind::Traversal.accepts(&[json!("child")]));
        assert!(ArgKind::Direction.accepts(&[json!("up")]));
        assert!(!ArgKind::Offset.accepts(&[json!("up")]));
        assert!(ArgKind::None.accepts(&[]));
    }
}
