use sonamu_common::sap::{detached::Axis, packets::StandardRequest};
use sonamu_ui::ui_event::{KeyModifiers, KeyTrait, UIEvent};

use crate::{
    command_hub::CommandHubApplet,
    nodular_applet::{AppletContext, NodeRequest, project_applet::ProjectApplet},
};

/// Started by Super+T (and a project's `+ terminal`)
pub const TERMINAL: &str = "foot";
/// Started by Super+N (and a project's `+ browser`)
pub const BROWSER: &str = "firefox";
const SUPER_SHIFT: KeyModifiers = KeyModifiers::both(KeyModifiers::LOGO, KeyModifiers::SHIFT);

/// Handle the standard keybinds like:
/// - `Ctrl+Shift+Q` - Quit
/// - `Super+W` - Close tab
/// - `Super+T` / `Super+N` - Start foot / firefox
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
            Some('t') => Some(TERMINAL),
            Some('n') => Some(BROWSER),
            _ => None,
        }
    {
        // Super+T / Super+N -> start foot / firefox
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
