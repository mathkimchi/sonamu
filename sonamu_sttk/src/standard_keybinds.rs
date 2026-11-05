use sonamu_common::{
    sap::{detached::Axis, packets::StandardRequest},
    settings,
};
use sonamu_ui::ui_event::{KeyModifiers, KeyTrait, UIEvent};

use crate::{
    command_hub::CommandHubApplet,
    nodular_applet::{AppletContext, NodeRequest, project_applet::ProjectApplet},
};

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

/// Started by Super+T (and a project's `+ terminal`)
#[must_use]
pub fn terminal() -> String {
    pick_program(settings::get().terminal, "TERMINAL", &TERMINALS)
}
/// Started by Super+N (and a project's `+ browser`)
#[must_use]
pub fn browser() -> String {
    pick_program(settings::get().browser, "BROWSER", &BROWSERS)
}

/// The setting, else `$var` if installed, else the first installed of `fallbacks` (else the first)
fn pick_program(setting: Option<String>, var: &str, fallbacks: &[&str]) -> String {
    setting
        .or_else(|| std::env::var(var).ok().filter(|program| installed(program)))
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
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|dir| dir.join(program).is_file())
    })
}
const SUPER_SHIFT: KeyModifiers = KeyModifiers::both(KeyModifiers::LOGO, KeyModifiers::SHIFT);

/// Handle the standard keybinds like:
/// - `Ctrl+Shift+Q` - Quit
/// - `Super+W` - Close tab
/// - `Super+T` / `Super+N` - Start the terminal / browser
/// - `Ctrl+Shift+Plus` - Add child
/// - `Super+Shift+|` / `Super+Shift+_` - Split side by side / stacked
/// - `Super+P` - Put the node in a project
/// - `Alt+Shift+W` / `Alt+Shift+S` - swap with the previous / next sibling
///
/// Traversal (`Alt+<traversal key>`, swipes) is the root's (`nodular_applet::traversal`).
///
/// Returns whether or not the keypress was used.
///
/// NOTE: I could technically just take in keypress,
/// but this is fine.
///
/// TODO: have an enum or something more generic for a keybind system
/// (could use abstraction similar to what I did with packets).
pub fn handle_standard_keybinds(ui_event: &UIEvent, hook: &AppletContext) -> bool {
    standard_keybind_request(ui_event)
        .map(|request| hook.request(request))
        .is_some()
}

/// The request a standard keybind makes to the applet's node, if `ui_event` is one.
/// For applets that handle some keybinds differently (eg `WlSurfaceApplet`'s Super+W).
pub fn standard_keybind_request(ui_event: &UIEvent) -> Option<NodeRequest> {
    if let UIEvent::Key {
        symbol: Some(key),
        modifiers: KeyModifiers::CTRL_SHIFT,
        pressed: true,
        ..
    } = &ui_event
        && key.to_char() == Some('Q')
    {
        // Ctrl+Shift+Q -> Quit
        Some(StandardRequest::Quit.into())
    } else if let UIEvent::Key {
        symbol: Some(key),
        modifiers: KeyModifiers { logo: true, .. },
        pressed: true,
        ..
    } = &ui_event
        && matches!(key.to_char(), Some('p' | 'P'))
    {
        // Super+P -> put the node and everything under it in a project
        Some(NodeRequest::Wrap(ProjectApplet::wrapper()))
    } else if let UIEvent::Key {
        symbol: Some(key),
        modifiers: KeyModifiers { logo: true, .. },
        pressed: true,
        ..
    } = &ui_event
        && key.to_char() == Some('w')
    {
        // Super+W -> close this tab, its children move to the parent
        Some(NodeRequest::Close)
    } else if let UIEvent::Key {
        symbol: Some(key),
        modifiers: KeyModifiers { logo: true, .. },
        pressed: true,
        ..
    } = &ui_event
        && let Some(program) = match key.to_char() {
            Some('t') => Some(terminal()),
            Some('n') => Some(browser()),
            _ => None,
        }
    {
        // Super+T / Super+N -> start the terminal / browser
        Some(NodeRequest::Run(program))
    } else if let UIEvent::Key {
        symbol: Some(key),
        modifiers: KeyModifiers::CTRL_SHIFT,
        pressed: true,
        ..
    } = &ui_event
        && key.to_char() == Some('+')
    {
        // Ctrl+Shift+Plus -> spawn CommandHub as child
        Some(NodeRequest::AddChild(
            CommandHubApplet::get_boxed_initiator(),
        ))
    } else if let UIEvent::Key {
        symbol: Some(key),
        modifiers: SUPER_SHIFT,
        pressed: true,
        ..
    } = &ui_event
        && let Some(axis) = match key.to_char() {
            Some('|') => Some(Axis::Horizontal),
            Some('_') => Some(Axis::Vertical),
            _ => None,
        }
    {
        // Super+Shift+| / Super+Shift+_ -> split with a CommandHub beside / below
        Some(NodeRequest::Split {
            axis,
            new_pane: CommandHubApplet::get_boxed_initiator(),
        })
    } else if let UIEvent::Key {
        symbol: Some(key),
        modifiers,
        pressed: true,
        ..
    } = &ui_event
        && modifiers.alt
        && modifiers.shift
        && !modifiers.ctrl
        && !modifiers.logo
        && let Some(shift) = match key.to_char() {
            Some('W') => Some(-1),
            Some('S') => Some(1),
            _ => None,
        }
    {
        // Alt+Shift+W / Alt+Shift+S -> swap with the previous / next sibling
        Some(NodeRequest::SwapSibling(shift))
    } else {
        None
    }
}
