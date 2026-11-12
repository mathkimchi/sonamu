//! The app launcher's popup: a search line over the matching apps; typing filters, Up/Down select, Enter or a click
//! starts one

use super::{
    Lines,
    desktop::{self, App},
    icons::Icons,
    notification_list,
};
use crate::{command_hub::program_command, standard_keybinds::terminal};
use sonamu_common::settings::Settings;
use sonamu_ui::{ui_element::UITree, ui_event::KeySymbol};

pub const ICON: &str = "\u{f003b}";
const SEARCH: &str = "\u{f0349}";
/// Apps shown at most
const SHOWN: usize = 12;
const COLUMNS: usize = 36;
/// Where an app's name starts, after its icon
const NAME_COLUMN: usize = 3;

pub struct Launcher {
    query: String,
    /// In the matches
    selected: usize,
    apps: Vec<App>,
}

pub enum KeyResult {
    Changed,
    Close,
    Ignored,
}

impl Launcher {
    /// Reads the apps
    pub fn open() -> Self {
        Self {
            query: String::new(),
            selected: 0,
            apps: desktop::apps(),
        }
    }

    /// The apps matching the query, best first
    fn matches(&self) -> Vec<&App> {
        let mut matches: Vec<(u32, &App)> = self
            .apps
            .iter()
            .filter_map(|app| Some((desktop::score(&app.name, &self.query)?, app)))
            .collect();
        // stable: by name within a score
        matches.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        matches
            .into_iter()
            .map(|(_, app)| app)
            .take(SHOWN)
            .collect()
    }

    pub fn size() -> [f32; 2] {
        Lines::size(1 + SHOWN, COLUMNS)
    }

    pub fn draw(&self, settings: &Settings, icons: &Icons) -> Vec<UITree> {
        let palette = &settings.palette;
        let lines = Lines::new();
        let mut items = vec![
            lines.highlight(0, [0, COLUMNS], palette.raised(20)),
            lines.text(
                0,
                0,
                vec![
                    (format!("{SEARCH} "), palette.inactive),
                    (self.query.clone(), palette.fg),
                    ("▏".to_string(), palette.accent),
                ],
            ),
        ];
        let matches = self.matches();
        if matches.is_empty() {
            items.push(lines.text(1, 0, vec![("No apps".to_string(), palette.inactive)]));
        }
        for (index, app) in matches.into_iter().enumerate() {
            let line = 1 + index;
            if index == self.selected {
                items.push(lines.highlight(line, [0, COLUMNS], palette.raised(30)));
            }
            if let Some(icon) = icons.get(&app.icon) {
                items.push(lines.icon(line, 0, icon));
            }
            let color = if index == self.selected {
                palette.accent
            } else {
                palette.fg
            };
            items.push(lines.text(
                line,
                NAME_COLUMN,
                vec![(
                    notification_list::cut(&app.name, COLUMNS - NAME_COLUMN),
                    color,
                )],
            ));
        }
        items
    }

    pub fn key(&mut self, key: KeySymbol) -> KeyResult {
        let count = self.matches().len();
        match key {
            KeySymbol::Escape => return KeyResult::Close,
            KeySymbol::Enter => {
                if let Some(app) = self.matches().get(self.selected) {
                    launch(app);
                }
                return KeyResult::Close;
            }
            KeySymbol::ArrowKeyDown => {
                self.selected = (self.selected + 1).min(count.saturating_sub(1))
            }
            KeySymbol::ArrowKeyUp => self.selected = self.selected.saturating_sub(1),
            KeySymbol::Backspace => {
                self.query.pop();
                self.selected = 0;
            }
            KeySymbol::Char(c) if !c.is_control() => {
                self.query.push(c);
                self.selected = 0;
            }
            _ => return KeyResult::Ignored,
        }
        KeyResult::Changed
    }

    /// Starts the app on `line`; whether there was one
    pub fn click(&self, line: usize) -> bool {
        let Some(app) = line
            .checked_sub(1)
            .and_then(|index| self.matches().get(index).copied())
        else {
            return false;
        };
        launch(app);
        true
    }
}

/// Starts it like any program Sonamu runs (its window is placed like a new one); in the terminal if it asks
fn launch(app: &App) {
    let mut argv = app.exec.clone();
    if app.terminal {
        argv.splice(0..0, [terminal(), "-e".to_string()]);
    }
    let Some((program, args)) = argv.split_first() else {
        return;
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    if let Err(err) = program_command(program, &args).spawn() {
        log::warn!("Couldn't start {}: {err}", app.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> App {
        App {
            name: name.to_string(),
            icon: String::new(),
            exec: vec!["true".to_string()],
            terminal: false,
        }
    }

    #[test]
    fn typing_filters_and_arrows_select() {
        let mut launcher = Launcher {
            query: String::new(),
            selected: 0,
            apps: ["Files", "Firefox", "Foot", "Thunderbird"].map(app).to_vec(),
        };
        for c in "fo".chars() {
            launcher.key(KeySymbol::Char(c));
        }
        let names = |launcher: &Launcher| -> Vec<String> {
            launcher.matches().iter().map(|app| app.name.clone()).collect()
        };
        // a prefix first, then letters in order
        assert_eq!(names(&launcher), ["Foot", "Firefox"]);
        launcher.key(KeySymbol::ArrowKeyDown);
        launcher.key(KeySymbol::ArrowKeyDown);
        assert_eq!(launcher.selected, 1);
        launcher.key(KeySymbol::Backspace);
        assert_eq!(launcher.selected, 0);
        assert_eq!(names(&launcher).len(), 3);
        assert!(matches!(launcher.key(KeySymbol::Escape), KeyResult::Close));
    }
}
