use sonamu_common::{
    command::{self, Value},
    sap::packets::StandardRequest,
    settings,
};
use sonamu_ui::ui_event::{KeyModifiers, KeyTrait, UIEvent};

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
    pub name: &'static str,
    pub args: Vec<Value>,
}
impl KeybindCommand {
    const fn new(name: &'static str) -> Self {
        Self {
            name,
            args: Vec::new(),
        }
    }

    /// The same as a request to the node (eg for applets that handle some keybinds differently)
    #[must_use]
    pub fn request(&self) -> Option<NodeRequest> {
        Some(match (self.name, self.args.as_slice()) {
            ("quit", []) => StandardRequest::Quit.into(),
            ("wrap_project", []) => NodeRequest::Wrap(ProjectApplet::wrapper()),
            ("close", []) => NodeRequest::Close,
            ("toggle_half", []) => NodeRequest::ToggleHalf,
            ("spawn", [Value::String(program)]) => NodeRequest::Run(program.clone()),
            ("spawn_sibling", [Value::String(program)]) => NodeRequest::RunSibling(program.clone()),
            #[expect(clippy::cast_possible_truncation, reason = "a sibling offset")]
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
        Some(uid) => command::send(uid, keybind.name, keybind.args),
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

/// The standard keybinds:
/// - `Ctrl+Shift+Q` - Quit
/// - `Super+W` - Close tab
/// - `Super+T` / `Super+N` - Start the terminal / browser (with `Shift`: as a sibling)
/// - `Super+P` - Put the node in a project
/// - `Super+F` - Toggle half width
/// - `Alt+Shift+W` / `Alt+Shift+S` - swap with the previous / next sibling
///
/// Traversal (`Alt+<traversal key>`, swipes) is the root's (`nodular_applet::traversal`).
pub fn standard_keybind(ui_event: &UIEvent) -> Option<KeybindCommand> {
    let UIEvent::Key {
        symbol: Some(key),
        modifiers,
        pressed: true,
        ..
    } = *ui_event
    else {
        return None;
    };
    // the case comes from Shift and Caps Lock; the modifiers decide (Caps Lock changes nothing)
    let key = key.to_char()?.to_ascii_lowercase();
    Some(match (key, modifiers) {
        (
            'q',
            KeyModifiers {
                ctrl: true,
                shift: true,
                alt: false,
                logo: false,
                ..
            },
        ) => KeybindCommand::new("quit"),
        // the node and everything under it in a project
        ('p', KeyModifiers { logo: true, .. }) => KeybindCommand::new("wrap_project"),
        // its children move to the parent
        (
            'w',
            KeyModifiers {
                logo: true,
                shift: false,
                ..
            },
        ) => KeybindCommand::new("close"),
        // #144
        ('f', KeyModifiers { logo: true, .. }) => KeybindCommand::new("toggle_half"),
        ('t' | 'n', KeyModifiers { logo: true, .. }) => KeybindCommand {
            name: if modifiers.shift {
                "spawn_sibling"
            } else {
                "spawn"
            },
            args: vec![Value::String(if key == 't' {
                terminal()
            } else {
                browser()
            })],
        },
        (
            'w' | 's',
            KeyModifiers {
                alt: true,
                shift: true,
                ctrl: false,
                logo: false,
                ..
            },
        ) => KeybindCommand {
            name: "swap_sibling",
            args: vec![Value::Number(if key == 'w' { -1.0 } else { 1.0 })],
        },
        _ => return None,
    })
}
