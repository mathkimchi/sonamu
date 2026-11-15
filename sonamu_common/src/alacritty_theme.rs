//! Sonamu's colors and opacity for Alacritty: its config imports this file and live reloads it on change.

use crate::settings::Settings;
use sonamu_ui::color::Color;
use std::{fs, io, path::PathBuf};

/// `$XDG_STATE_HOME/sonamu/alacritty.toml` (`~/.local/state` by default)
#[must_use]
pub fn path() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(state.join("sonamu/alacritty.toml"))
}

/// Writes it if it changed (else Alacritty would reload for nothing); renamed into place, so it's never read half
/// written
pub fn write(settings: &Settings) -> io::Result<()> {
    let Some(path) = path() else {
        return Ok(());
    };
    let text = toml(settings);
    if fs::read_to_string(&path).is_ok_and(|old| old == text) {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, text)?;
    fs::rename(temporary, path)
}

fn toml(settings: &Settings) -> String {
    let rgb = |Color([r, g, b, _]): Color| format!("\"#{r:02x}{g:02x}{b:02x}\"");
    format!(
        "# Written by Sonamu from its settings; changes here are overwritten\n\
         [window]\nopacity = {}\n\n\
         [colors.primary]\nbackground = {}\nforeground = {}\n",
        settings.opacity.clamp(0.0, 1.0),
        rgb(settings.palette.bg),
        rgb(settings.palette.fg),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_and_opacity() {
        let mut settings = Settings {
            opacity: 0.5,
            ..Default::default()
        };
        settings.palette.bg = Color([0x07, 0x21, 0x0f, 0xCC]);
        settings.palette.fg = Color([0xe7, 0xef, 0xe9, 0xFF]);
        assert_eq!(
            toml(&settings),
            "# Written by Sonamu from its settings; changes here are overwritten\n\
             [window]\nopacity = 0.5\n\n\
             [colors.primary]\nbackground = \"#07210f\"\nforeground = \"#e7efe9\"\n"
        );
    }
}
