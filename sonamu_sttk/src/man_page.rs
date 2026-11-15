//! The basic usage manual (`docs/usage.md`). Super+? opens it where a new wl surface would go (the root does that); Esc or q closes it.

use crate::{
    nodular_applet::{
        AppletContext, AppletSpawner, AppletSpawnerTrait, NodularAppletInitializer, StandardApplet,
    },
    standard_keybinds::handle_standard_keybinds,
    ui,
};
use sonamu_common::{
    sap::packets::StandardEvent,
    settings::{self, Action, Key, Keybind, NamedKey, Settings},
    utils::tree::world_tree::WorldTree,
};
use sonamu_ui::color::Color;
use sonamu_ui::{
    display_units::DisplayContainerSize,
    ui_element::{CharGrid, UITree},
    ui_event::{KeySymbol, MouseEventKind, UIEvent},
};

struct ManSection {
    title: &'static str,
    /// Keys and description
    entries: &'static [(&'static str, &'static str)],
}

/// The basic usage bullet points of `docs/usage.md` past the keybinds (`keybind_sections`)
const MAN_PAGE: &[ManSection] = &[
    ManSection {
        title: "Actions while focused on tree view",
        entries: &[
            ("Traversal keys", "as above, without Alt (but Alt+Enter)"),
            ("Enter", "focus on main content"),
            ("Space", "toggle children visibility"),
            ("Click top right circle", "pin/unpin tree view"),
            ("Double click on node", "makes it the view root"),
            ("Drag/drop nodes", "moves them"),
            ("Right click", "brings up options"),
        ],
    },
    ManSection {
        title: "Gestures",
        entries: &[
            ("3-finger swipe up/down", "previous/next sibling"),
            ("3-finger swipe left/right", "child/parent"),
            ("4-finger swipe up/down", "focus tree view/main content"),
        ],
    },
];

/// The setting `keybinds`: each group's name, and its binds as keys and descriptions
fn keybind_sections(settings: &Settings) -> Vec<(String, Vec<(String, String)>)> {
    settings
        .keybinds
        .iter()
        .map(|(group, keybinds)| {
            let entries = keybinds
                .iter()
                .map(|keybind| (keys_label(keybind), describe(&keybind.action)))
                .collect();
            (group.clone(), entries)
        })
        .collect()
}

/// A keybind's keys, eg `Super+Shift+T`
#[must_use]
pub fn keys_label(keybind: &Keybind) -> String {
    let has = |name: &str| keybind.mods.iter().any(|m| m.eq_ignore_ascii_case(name));
    let mut label = String::new();
    for (name, shown) in [
        ("ctrl", "Ctrl"),
        ("logo", "Super"),
        ("alt", "Alt"),
        ("shift", "Shift"),
    ] {
        if has(name) {
            label += shown;
            label.push('+');
        }
    }
    let key = match keybind.key {
        Key::Char(c) => c.to_uppercase().to_string(),
        Key::Named(named) => match named {
            NamedKey::Left => "Left",
            NamedKey::Right => "Right",
            NamedKey::Up => "Up",
            NamedKey::Down => "Down",
            NamedKey::Enter => "Enter",
            NamedKey::Tab => "Tab",
            NamedKey::Escape => "Esc",
            NamedKey::Backspace => "Backspace",
            NamedKey::PageUp => "PageUp",
            NamedKey::PageDown => "PageDown",
        }
        .to_owned(),
        // evdev codes
        Key::Code(code) => match code {
            99 => "Print".to_owned(),
            113 => "Mute".to_owned(),
            114 => "VolumeDown".to_owned(),
            115 => "VolumeUp".to_owned(),
            224 => "BrightnessDown".to_owned(),
            225 => "BrightnessUp".to_owned(),
            code => format!("key {code}"),
        },
    };
    label + &key
}

/// What a keybind does, in words
fn describe(action: &Action) -> String {
    let (command, args) = match action {
        Action::Shell { shell } => return format!("run `{shell}`"),
        Action::Command { command, args } => (command.as_str(), args.as_slice()),
    };
    let text = |index: usize| args.get(index).and_then(|arg| arg.as_str());
    let number = |index: usize| args.get(index).and_then(serde_json::Value::as_i64);
    let direction = |offset: Option<i64>| if offset < Some(0) { "previous" } else { "next" };
    match (command, text(0)) {
        ("quit", _) => "quit".to_owned(),
        ("close", _) => "close current tab".to_owned(),
        ("wrap_project", _) => "wrap current tab in a project".to_owned(),
        ("toggle_half", _) => "toggle half-view".to_owned(),
        ("spawn", Some(program)) => format!("spawn {program}"),
        ("spawn_sibling", Some(program)) => format!("spawn {program} as sibling"),
        ("spawn_terminal", _) => "spawn terminal".to_owned(),
        ("spawn_terminal_sibling", _) => "spawn terminal as sibling".to_owned(),
        ("spawn_browser", _) => "spawn browser".to_owned(),
        ("spawn_browser_sibling", _) => "spawn browser as sibling".to_owned(),
        ("swap_sibling", _) => format!("swap with the {} sibling", direction(number(0))),
        ("pluck_place", _) => "pluck/place the focused node".to_owned(),
        ("buffer_mode", _) => "buffer line: Luau prompt (Esc leaves)".to_owned(),
        ("help", _) => "bring up this basic usage manual".to_owned(),
        ("view_root", _) => "toggle the focused node as view root".to_owned(),
        ("view_parent", _) => "view root to the focus's parent".to_owned(),
        ("swipe", Some(direction)) => format!(
            "swipe to the {}",
            match direction {
                "left" => "previous sibling",
                "right" => "next sibling",
                "up" => "parent",
                _ => "child",
            }
        ),
        ("traverse", Some(kind)) => match kind {
            "next" => "next row in the tree view".to_owned(),
            "prev" => "previous row in the tree view".to_owned(),
            "parent" => "go to parent".to_owned(),
            "child" => format!("go to child {}", number(1).unwrap_or(1)),
            "recent_child" => "go to the child last focused".to_owned(),
            "last_child" => "go to the last child".to_owned(),
            "sibling" => format!("go to the {} sibling", direction(number(1))),
            "select" => "out of this applet".to_owned(),
            "enter" => "focus tree view/main content (toggles)".to_owned(),
            kind => format!("traverse {kind}"),
        },
        _ => format!("{command} {args:?}"),
    }
}

/// Below this, keys go above their descriptions instead of beside them
const MIN_DESCRIPTION_WIDTH: usize = 24;

/// A row of the list: keys (if any) and text
struct Row {
    keys: String,
    text: String,
    section: bool,
}

pub struct ManPageApplet {
    /// Rows scrolled past
    scroll: usize,
    /// Mouse scrolling not yet a step
    scrolled: ui::ScrollSteps,
    size: DisplayContainerSize,
    /// The palette, and keys in `border.focused` (`settings_changed`)
    palette: settings::Palette,
    key_color: Color,
    /// `keybind_sections`
    keybinds: Vec<(String, Vec<(String, String)>)>,
    hook: AppletContext,
}
impl ManPageApplet {
    #[must_use]
    pub fn get_boxed_initiator() -> NodularAppletInitializer {
        NodularAppletInitializer::new(|hook: AppletContext| {
            let settings = settings::get();
            Box::new(Self {
                scroll: 0,
                scrolled: ui::ScrollSteps::default(),
                size: DisplayContainerSize::new(800, 600),
                keybinds: keybind_sections(&settings),
                palette: settings.palette,
                key_color: settings.border.focused,
                hook,
            })
        })
    }

    #[must_use]
    pub fn get_applet_spawner() -> AppletSpawner {
        struct ManPageSpawner;
        impl AppletSpawnerTrait for ManPageSpawner {
            fn create_initializer(&self, _args: &[&str]) -> Option<NodularAppletInitializer> {
                Some(ManPageApplet::get_boxed_initiator())
            }

            fn duplicate(&self) -> AppletSpawner {
                Box::new(Self)
            }
        }
        Box::new(ManPageSpawner)
    }

    /// Every row at `width`, descriptions word-wrapped
    fn rows(&self, width: usize) -> Vec<Row> {
        let sections = self
            .keybinds
            .iter()
            .cloned()
            .chain(MAN_PAGE.iter().map(|section| {
                let entries = section.entries.iter();
                (
                    section.title.to_owned(),
                    entries
                        .map(|&(keys, text)| (keys.to_owned(), text.to_owned()))
                        .collect(),
                )
            }));
        let sections: Vec<(String, Vec<(String, String)>)> = sections.collect();
        let key_width = sections
            .iter()
            .flat_map(|(_, entries)| entries)
            .map(|(keys, _)| keys.len())
            .max()
            .unwrap_or(0);
        let beside = width >= 2 + key_width + 2 + MIN_DESCRIPTION_WIDTH;
        let mut rows = Vec::new();
        for (title, entries) in sections {
            if !rows.is_empty() {
                rows.push(Row {
                    keys: String::new(),
                    text: String::new(),
                    section: false,
                });
            }
            rows.push(Row {
                keys: String::new(),
                text: title,
                section: true,
            });
            for (keys, description) in entries {
                if !beside {
                    rows.push(Row {
                        keys: keys.clone(),
                        text: String::new(),
                        section: false,
                    });
                }
                let indent = if beside { 2 + key_width + 2 } else { 4 };
                for (index, line) in word_wrap(&description, width.saturating_sub(indent))
                    .into_iter()
                    .enumerate()
                {
                    rows.push(Row {
                        keys: if beside && index == 0 {
                            keys.clone()
                        } else {
                            String::new()
                        },
                        text: format!("{}{line}", " ".repeat(indent)),
                        section: false,
                    });
                }
            }
        }
        rows
    }

    fn max_scroll(&self) -> usize {
        let (width, height) = CharGrid::largest_fittable_size(self.size);
        self.rows(width)
            .len()
            .saturating_sub(height.saturating_sub(1))
    }

    /// Positive scrolls down
    fn scroll_by(&mut self, rows: isize) {
        self.scroll = self
            .scroll
            .saturating_add_signed(rows)
            .min(self.max_scroll());
        self.hook.damage_window();
    }
}
impl StandardApplet for ManPageApplet {
    fn settings_changed(&mut self, settings: &Settings) {
        self.palette = settings.palette.clone();
        self.key_color = settings.border.focused;
        self.keybinds = keybind_sections(settings);
        self.hook.damage_window();
    }

    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        let StandardEvent::UIEvent(ui_event) = standard_event else {
            return;
        };
        if handle_standard_keybinds(&ui_event, &self.hook) {
            return;
        }
        let (_, height) = CharGrid::largest_fittable_size(self.size);
        let page = ui::page(height.saturating_sub(1)) as isize;
        match ui_event {
            UIEvent::Key {
                symbol: Some(key),
                pressed: true,
                ..
            } => match key {
                KeySymbol::Escape | KeySymbol::Char('q') => self.hook.close(),
                KeySymbol::ArrowKeyDown | KeySymbol::Char('j') => self.scroll_by(1),
                KeySymbol::ArrowKeyUp | KeySymbol::Char('k') => self.scroll_by(-1),
                KeySymbol::PageDown | KeySymbol::Char(' ') => self.scroll_by(page),
                KeySymbol::PageUp => self.scroll_by(-page),
                _ => {}
            },
            UIEvent::Mouse(mouse_event) => {
                if let MouseEventKind::Scroll { delta, v120, .. } = mouse_event.kind {
                    let steps = self.scrolled.add(delta, v120) as isize;
                    self.scroll_by(steps);
                }
            }
            UIEvent::WindowResized(size) => {
                self.size = size;
                self.scroll = self.scroll.min(self.max_scroll());
            }
            UIEvent::Key { .. } | UIEvent::Swipe { .. } | UIEvent::Pinch(_) => {}
        }
    }

    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree {
        let (width, height) = CharGrid::largest_fittable_size(container_size);
        let mut grid = CharGrid::new_empty(width, height);
        if width == 0 || height == 0 {
            return grid.element().into();
        }
        for row in 0..height {
            ui::fill_row(&mut grid, row, ui::bg(&self.palette));
        }

        ui::fill_row(&mut grid, 0, ui::raised_bg(&self.palette));
        let title_end = ui::put(&mut grid, 0, 1, "Basic Usage", self.palette.fg);
        let note = "Esc closes, Up/Down scroll ";
        if let Some(column) = width
            .checked_sub(note.len())
            .filter(|&column| column > title_end + 1)
        {
            ui::put(&mut grid, 0, column, note, self.palette.inactive);
        }

        let rows = self.rows(width);
        for (row, line) in (1..height).zip(rows.iter().skip(self.scroll)) {
            if line.section {
                ui::put(&mut grid, row, 1, &line.text, self.palette.accent);
            } else {
                ui::put(&mut grid, row, 0, &line.text, self.palette.fg);
                ui::put(&mut grid, row, 2, &line.keys, self.key_color);
            }
        }

        let bar = ui::grid_scrollbar(&self.palette, width, 1, self.scroll, height - 1, rows.len());
        UITree::new(grid.element(), bar.into_iter().collect())
    }

    fn get_treeview(&self) -> WorldTree<String> {
        WorldTree::Base("Manual".to_string())
    }
}

/// Greedy, on spaces; words longer than `width` are cut
fn word_wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = vec![String::new()];
    for word in text.split(' ') {
        let line = lines.last_mut().expect("starts with one");
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(String::new());
        }
        let line = lines.last_mut().expect("starts with one");
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines
        .into_iter()
        .flat_map(|line| {
            line.chars()
                .collect::<Vec<_>>()
                .chunks(width)
                .map(|chunk| chunk.iter().collect())
                .collect::<Vec<String>>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use sonamu_common::settings::Settings;

    /// Every default keybind has its own words
    #[test]
    fn default_keybinds_described() {
        let sections = super::keybind_sections(&Settings::default());
        for (keys, text) in sections.into_iter().flat_map(|(_, entries)| entries) {
            println!("{keys:<20} {text}");
            assert!(!text.contains('['), "{keys}: {text}");
        }
    }

    #[test]
    fn sections_follow_keybind_groups() {
        let titles: Vec<_> = super::keybind_sections(&Settings::default())
            .into_iter()
            .map(|(title, _)| title)
            .collect();
        assert_eq!(titles, ["Global", "Windows", "Tree", "Traversal", "Swipe"]);
    }
}
