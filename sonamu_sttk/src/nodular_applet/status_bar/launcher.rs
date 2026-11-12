//! The app launcher's popup: a search line over the matching apps; typing filters, Up/Down select, the wheel
//! scrolls, Enter or a click starts one

use super::{
    Lines,
    desktop::{self, App},
    icons::Icons,
    notification_list,
    widgets::{Button, Cell, ScrollList, label_color},
};
use crate::{command_hub::program_command, standard_keybinds::terminal};
use sonamu_common::settings::Settings;
use sonamu_ui::{ui_element::UITree, ui_event::KeySymbol};

pub const ICON: &str = "\u{f003b}";
const SEARCH: &str = "\u{f0349}";
/// Apps shown at once
const SHOWN: usize = 12;
const COLUMNS: usize = 36;
/// Where an app's name starts, after its icon
const NAME_COLUMN: usize = 3;

pub struct Launcher {
    query: String,
    /// In the matches
    selected: usize,
    list: ScrollList,
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
            list: ScrollList::default(),
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
        matches.into_iter().map(|(_, app)| app).collect()
    }

    pub fn size() -> [f32; 2] {
        Lines::size(1 + SHOWN, COLUMNS)
    }

    pub fn draw(&self, settings: &Settings, icons: &Icons, hover: Option<Cell>) -> Vec<UITree> {
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
        for index in self.list.visible(matches.len(), SHOWN) {
            let app = matches[index];
            let line = 1 + index - self.list.visible(matches.len(), SHOWN).start;
            let on = index == self.selected;
            let name = notification_list::cut(&app.name, COLUMNS - NAME_COLUMN);
            items.extend(Button::row(line, COLUMNS).draw(
                &lines,
                palette,
                vec![(format!("   {name}"), label_color(palette, on))],
                on,
                hover,
            ));
            if let Some(icon) = icons.get(&app.icon) {
                items.push(lines.icon(line, 0, icon));
            }
        }
        items.extend(
            self.list
                .bar(&lines, [1, COLUMNS], matches.len(), SHOWN, palette.inactive),
        );
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
                self.selected = (self.selected + 1).min(count.saturating_sub(1));
                self.list.show(self.selected, SHOWN);
            }
            KeySymbol::ArrowKeyUp => {
                self.selected = self.selected.saturating_sub(1);
                self.list.show(self.selected, SHOWN);
            }
            KeySymbol::Backspace => {
                self.query.pop();
                self.selected = 0;
                self.list.reset();
            }
            KeySymbol::Char(c) if !c.is_control() => {
                self.query.push(c);
                self.selected = 0;
                self.list.reset();
            }
            _ => return KeyResult::Ignored,
        }
        KeyResult::Changed
    }

    /// Scrolls the apps `steps` (positive down); whether they moved
    pub fn scroll(&mut self, steps: i32) -> bool {
        let count = self.matches().len();
        self.list.scroll(steps, count, SHOWN)
    }

    /// Starts the app on `line`; whether there was one
    pub fn click(&self, line: usize) -> bool {
        let matches = self.matches();
        let Some(index) = self.list.item_at(line, 1, matches.len(), SHOWN) else {
            return false;
        };
        launch(matches[index]);
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
            list: ScrollList::default(),
            apps: ["Files", "Firefox", "Foot", "Thunderbird"]
                .map(app)
                .to_vec(),
        };
        for c in "fo".chars() {
            launcher.key(KeySymbol::Char(c));
        }
        let names = |launcher: &Launcher| -> Vec<String> {
            launcher
                .matches()
                .iter()
                .map(|app| app.name.clone())
                .collect()
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
