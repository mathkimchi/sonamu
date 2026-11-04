//! The basic usage manual (`docs/usage.md`). Super+? opens it where a new wl surface would go (the root does that); Esc or q closes it.

use crate::{
    command_hub::{ACCENT, BAR_BG, BG, CURSOR, DIM, OUTPUT, RUNNING, fill_row, put},
    nodular_applet::{
        AppletContext, AppletSpawner, AppletSpawnerTrait, NodularAppletInitializer, StandardApplet,
    },
    standard_keybinds::handle_standard_keybinds,
};
use sonamu_common::{sap::packets::StandardEvent, utils::tree::world_tree::WorldTree};
use sonamu_ui::{
    display_units::DisplayContainerSize,
    ui_element::{CharGrid, UITree},
    ui_event::{KeySymbol, MouseEventKind, UIEvent},
};

struct ManEntry {
    key: &'static str,
    description: &'static str,
}

struct ManSection {
    title: &'static str,
    entries: &'static [ManEntry],
}

const fn entry(key: &'static str, description: &'static str) -> ManEntry {
    ManEntry { key, description }
}

/// The basic usage bullet points of `docs/usage.md`
const MAN_PAGE: &[ManSection] = &[
    ManSection {
        title: "Global Shortcuts",
        entries: &[
            entry(
                "Super+Shift+/ (Super+?)",
                "bring up this basic usage manual",
            ),
            entry("Super+T", "spawn foot terminal"),
            entry("Super+N", "spawn firefox browser"),
            entry("Super+P", "wrap current tab in a project"),
            entry("Super+W", "close current tab"),
            entry("Super+Shift+\\ (Super+|)", "split to right"),
            entry("Super+Shift+-", "split to bottom"),
        ],
    },
    ManSection {
        title: "Traversal Keys (Alt+<traversal key> focuses the tree view)",
        entries: &[
            entry("(Shift+)Tab", "goes (up)/down in the tree view"),
            entry("W/S", "goes to prev/next sibling"),
            entry("A/D", "goes to parent/child"),
        ],
    },
    ManSection {
        title: "Actions while focused on tree view",
        entries: &[
            entry("Traversal keys", "as above"),
            entry("Enter", "focus on main content"),
            entry("Space", "toggle children visibility"),
            entry("Click top right circle", "pin/unpin tree view"),
            entry("Double click on node", "makes it the view root"),
            entry("Drag/drop nodes", "moves them"),
            entry("Right click", "brings up options"),
        ],
    },
    ManSection {
        title: "Gestures",
        entries: &[
            entry("3-finger swipe up/down", "same as Alt+(Shift+)Tab"),
            entry("4-finger swipe up/down", "focus tree view/main content"),
        ],
    },
];

/// Below this, keys go above their descriptions instead of beside them
const MIN_DESCRIPTION_WIDTH: usize = 24;
const SCROLL_STEP: usize = 3;

/// A row of the list: keys (if any) and text
struct Row {
    keys: &'static str,
    text: String,
    section: bool,
}

pub struct ManPageApplet {
    /// Rows scrolled past
    scroll: usize,
    size: DisplayContainerSize,
    hook: AppletContext,
}
impl ManPageApplet {
    #[must_use]
    pub fn get_boxed_initiator() -> NodularAppletInitializer {
        Box::new(|hook: AppletContext| {
            Box::new(Self {
                scroll: 0,
                size: DisplayContainerSize::new(800, 600),
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
    fn rows(width: usize) -> Vec<Row> {
        let key_width = MAN_PAGE
            .iter()
            .flat_map(|section| section.entries)
            .map(|entry| entry.key.len())
            .max()
            .unwrap_or(0);
        let beside = width >= 2 + key_width + 2 + MIN_DESCRIPTION_WIDTH;
        let mut rows = Vec::new();
        for section in MAN_PAGE {
            if !rows.is_empty() {
                rows.push(Row {
                    keys: "",
                    text: String::new(),
                    section: false,
                });
            }
            rows.push(Row {
                keys: "",
                text: section.title.to_string(),
                section: true,
            });
            for &ManEntry {
                key: keys,
                description,
            } in section.entries
            {
                if !beside {
                    rows.push(Row {
                        keys,
                        text: String::new(),
                        section: false,
                    });
                }
                let indent = if beside { 2 + key_width + 2 } else { 4 };
                for (index, line) in word_wrap(description, width.saturating_sub(indent))
                    .into_iter()
                    .enumerate()
                {
                    rows.push(Row {
                        keys: if beside && index == 0 { keys } else { "" },
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
        Self::rows(width)
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
    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        let StandardEvent::UIEvent(ui_event) = standard_event else {
            return;
        };
        if handle_standard_keybinds(&ui_event, &self.hook) {
            return;
        }
        let (_, height) = CharGrid::largest_fittable_size(self.size);
        let page = height.saturating_sub(2).max(1) as isize;
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
                if let MouseEventKind::Scroll { delta, .. } = mouse_event.kind {
                    let step = SCROLL_STEP as isize;
                    self.scroll_by(if delta[1] > 0.0 { step } else { -step });
                }
            }
            UIEvent::WindowResized(size) => {
                self.size = size;
                self.scroll = self.scroll.min(self.max_scroll());
            }
            UIEvent::Key { .. } | UIEvent::Swipe { .. } => {}
        }
    }

    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree {
        let (width, height) = CharGrid::largest_fittable_size(container_size);
        let mut grid = CharGrid::new_empty(width, height);
        if width == 0 || height == 0 {
            return grid.element().into();
        }
        for row in 0..height {
            fill_row(&mut grid, row, BG);
        }

        fill_row(&mut grid, 0, BAR_BG);
        let title_end = put(&mut grid, 0, 1, "Basic Usage", CURSOR);
        let note = "Esc closes, Up/Down scroll ";
        if let Some(column) = width
            .checked_sub(note.len())
            .filter(|&column| column > title_end + 1)
        {
            put(&mut grid, 0, column, note, DIM);
        }

        for (row, line) in (1..height).zip(Self::rows(width).iter().skip(self.scroll)) {
            if line.section {
                put(&mut grid, row, 1, &line.text, ACCENT);
            } else {
                put(&mut grid, row, 0, &line.text, OUTPUT);
                put(&mut grid, row, 2, line.keys, RUNNING);
            }
        }

        grid.element().into()
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
