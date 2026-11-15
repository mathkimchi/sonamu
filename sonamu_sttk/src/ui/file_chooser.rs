//! A file picked visually: a popup with the path being typed (`PathInput`: Tab completes, Backspace) above a grid of
//! what it could complete to, folders first; images show as thumbnails. Clicking a folder goes in, a file picks it.

use super::{
    POPUP_PAD, Rows, ScrollSteps, frame, guide, path_input::PathInput, popup_bg, raised_bg,
    row_height, scroll_to, scrollbar, selected_bg, text,
};
use sonamu_common::settings::{ERROR, Palette};
use sonamu_ui::{
    ui_element::{Image, Style, UIElement, UITree, font_size},
    ui_event::{KeyModifiers, KeySymbol, MouseEventKind},
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
};

use super::path_input::{PathKey, expand, tilde};

/// Pixels per side thumbnails are decoded to
const THUMBNAIL_PIXELS: u32 = 128;
/// Images larger than this aren't decoded for a thumbnail (bytes)
const THUMBNAIL_MAX_BYTES: u64 = 32 << 20;
const FOLDER_ICON: &str = "\u{f07b}";
const FILE_ICON: &str = "\u{f15b}";
const UP: &str = "‹ up";

/// What an event did to the chooser
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen {
    /// Still choosing
    Not,
    Picked(PathBuf),
    /// Esc, or a click outside it
    Closed,
}

/// Where it was last drawn, for clicks
#[derive(Debug, Clone, Copy, Default)]
struct Layout {
    area: [f32; 4],
    columns: usize,
    shown_rows: usize,
}

pub struct FileChooser {
    pub input: PathInput,
    /// The first row of tiles shown
    scroll: usize,
    scrolled: ScrollSteps,
    hovered: Option<usize>,
    layout: Cell<Layout>,
    /// Decoded once per file (`None`: not an image it can read)
    thumbnails: RefCell<HashMap<String, Option<Image>>>,
}
impl FileChooser {
    /// In `start`'s folder (or `start` if it is one), with `start` selected
    #[must_use]
    pub fn new(start: &Path) -> Self {
        let dir = if start.is_dir() {
            start
        } else {
            start.parent().unwrap_or(start)
        };
        let mut input = PathInput::file(format!("{}/", tilde(dir).trim_end_matches('/')));
        let current = tilde(start);
        input.selected = input
            .options
            .iter()
            .position(|option| option.path == current);
        Self {
            input,
            scroll: 0,
            scrolled: ScrollSteps::default(),
            hovered: None,
            layout: Cell::new(Layout::default()),
            thumbnails: RefCell::new(HashMap::new()),
        }
    }

    pub fn handle_key(&mut self, key: KeySymbol, modifiers: KeyModifiers) -> Chosen {
        let count = self.input.options.len();
        let columns = self.layout.get().columns.max(1);
        let moved = match key {
            KeySymbol::ArrowKeyLeft => Some(-1),
            KeySymbol::ArrowKeyRight => Some(1),
            KeySymbol::ArrowKeyUp => Some(-(columns as isize)),
            KeySymbol::ArrowKeyDown => Some(columns as isize),
            _ => None,
        };
        if let Some(by) = moved {
            if count > 0 {
                let next = match self.input.selected {
                    None => 0,
                    Some(at) => at.saturating_add_signed(by).min(count - 1),
                };
                self.input.selected = Some(next);
                self.follow_selection();
            }
            return Chosen::Not;
        }
        match self.input.handle_key(key, modifiers) {
            PathKey::Enter => self.enter(),
            PathKey::Cancel => Chosen::Closed,
            PathKey::Changed => {
                self.scroll = 0;
                Chosen::Not
            }
            PathKey::Ignored => Chosen::Not,
        }
    }

    /// Enter: into the selected folder, or the selected or typed file
    fn enter(&mut self) -> Chosen {
        if let Some(option) = self
            .input
            .selected
            .and_then(|at| self.input.options.get(at))
            .cloned()
        {
            return self.open(&option.path, option.dir);
        }
        if let Some(file) = self.input.parse_file() {
            return Chosen::Picked(file);
        }
        match expand(self.input.path.trim()) {
            Some(path) if path.is_dir() => {
                let typed = self.input.path.trim().trim_end_matches('/').to_owned();
                self.input.set(format!("{typed}/"));
                self.scroll = 0;
            }
            _ => self.input.invalid = true,
        }
        Chosen::Not
    }

    fn open(&mut self, path: &str, dir: bool) -> Chosen {
        if dir {
            self.input.set(format!("{path}/"));
            self.scroll = 0;
            return Chosen::Not;
        }
        expand(path).map_or(Chosen::Not, Chosen::Picked)
    }

    fn follow_selection(&mut self) {
        let Layout {
            columns,
            shown_rows,
            ..
        } = self.layout.get();
        if let Some(at) = self.input.selected {
            self.scroll = scroll_to(self.scroll, at / columns.max(1), shown_rows.max(1));
        }
    }

    /// A mouse event at `position` in the window it's drawn in
    pub fn handle_mouse(&mut self, position: [f64; 2], kind: MouseEventKind) -> Chosen {
        let layout = self.layout.get();
        let [left, top, width, height] = layout.area;
        let [x, y] = [position[0] as f32 - left, position[1] as f32 - top];
        let inside = (0.0..width).contains(&x) && (0.0..height).contains(&y);
        let tile = self.tile_at([x, y]);
        match kind {
            MouseEventKind::Motion => self.hovered = tile,
            MouseEventKind::Button {
                button: super::BTN_LEFT,
                pressed: true,
            } => {
                if !inside {
                    return Chosen::Closed;
                }
                if let Some(at) = tile {
                    let option = self.input.options[at].clone();
                    return self.open(&option.path, option.dir);
                }
                if Rows::new(POPUP_PAD).row_at(f64::from(y)) == Some(0) && x < up_right() {
                    self.input.up();
                    self.scroll = 0;
                }
            }
            MouseEventKind::Scroll { delta, v120, .. } if inside => {
                let steps = self.scrolled.add(delta, v120);
                let rows = self.input.options.len().div_ceil(layout.columns.max(1));
                let max = rows.saturating_sub(layout.shown_rows);
                self.scroll = self.scroll.saturating_add_signed(steps as isize).min(max);
            }
            _ => {}
        }
        Chosen::Not
    }

    /// The tile at `[x, y]` in the popup
    fn tile_at(&self, [x, y]: [f32; 2]) -> Option<usize> {
        let Layout {
            columns,
            shown_rows,
            ..
        } = self.layout.get();
        let ([tile_width, tile_height], gap) = (tile_size(), POPUP_PAD / 2.0);
        let (x, y) = (x - POPUP_PAD, y - grid_top());
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let column = fit(x, tile_width + gap);
        let row = fit(y, tile_height + gap);
        let on_tile = x % (tile_width + gap) < tile_width && y % (tile_height + gap) < tile_height;
        let at = (self.scroll + row) * columns + column;
        (on_tile && column < columns && row < shown_rows && at < self.input.options.len())
            .then_some(at)
    }

    /// The popup filling `[left, top, width, height]` of the window
    #[must_use]
    pub fn draw(&self, area: [f32; 4], palette: &Palette) -> UITree {
        let [left, top, width, height] = area;
        let ([tile_width, tile_height], gap) = (tile_size(), POPUP_PAD / 2.0);
        let columns = fit(width - 2.0 * POPUP_PAD + gap, tile_width + gap).max(1);
        let shown_rows = fit(height - grid_top() - POPUP_PAD + gap, tile_height + gap).max(1);
        self.layout.set(Layout {
            area,
            columns,
            shown_rows,
        });
        let header = Rows::new(POPUP_PAD);
        let line = |index: usize, parts| header.row(index, width, None, parts);
        let (hint, hint_color) = if self.input.invalid {
            ("Not a file", ERROR)
        } else {
            (
                "Type: path, Tab: complete, arrows: pick, Enter or click: open, Esc: cancel",
                palette.inactive,
            )
        };
        let mut parts = vec![
            line(
                0,
                vec![
                    header
                        .text(text(UP, palette.accent), POPUP_PAD, up_right())
                        .with_pointer(),
                    header.text(
                        text(format!("{}▏", self.input.path), palette.fg),
                        up_right() + POPUP_PAD,
                        width - POPUP_PAD,
                    ),
                ],
            ),
            line(
                1,
                vec![header.text(text(hint, hint_color), POPUP_PAD, width - POPUP_PAD)],
            ),
        ];
        let options = &self.input.options;
        let first = self.scroll * columns;
        for at in first..options.len().min(first + shown_rows * columns) {
            let (row, column) = (at / columns - self.scroll, at % columns);
            let corner = [
                POPUP_PAD + column as f32 * (tile_width + gap),
                grid_top() + row as f32 * (tile_height + gap),
            ];
            parts.push(self.tile(at, corner, palette));
        }
        if options.is_empty() {
            parts.push(Rows::new(grid_top()).row(
                0,
                width,
                None,
                vec![Rows::new(0.0).text(
                    text("Nothing here", palette.inactive),
                    POPUP_PAD,
                    width - POPUP_PAD,
                )],
            ));
        }
        parts.extend(scrollbar(
            palette,
            width - POPUP_PAD / 2.0,
            [grid_top(), height - grid_top() - POPUP_PAD],
            self.scroll,
            shown_rows,
            options.len().div_ceil(columns),
        ));
        frame(
            guide(palette),
            POPUP_PAD,
            1.0,
            Style::absolute([left, top], [width, height]),
            vec![UITree::new_styled(
                UIElement::Backgrounded(popup_bg(palette)),
                Style::absolute([0.0, 0.0], [width, height]),
                parts,
            )],
        )
    }

    /// Option `at`: its thumbnail or icon over its name, with its top left at `corner`
    fn tile(&self, at: usize, corner: [f32; 2], palette: &Palette) -> UITree {
        let [tile_width, tile_height] = tile_size();
        let option = &self.input.options[at];
        let background = if self.input.selected == Some(at) {
            Some(selected_bg(palette))
        } else if self.hovered == Some(at) {
            Some(raised_bg(palette))
        } else {
            None
        };
        let name = if option.dir {
            format!("{}/", option.name())
        } else {
            option.name().to_owned()
        };
        let picture = (!option.dir)
            .then(|| self.thumbnail(&option.path))
            .flatten()
            .map_or_else(
                || {
                    let (icon, color) = if option.dir {
                        (FOLDER_ICON, palette.accent)
                    } else {
                        (FILE_ICON, palette.fg)
                    };
                    UITree::leaf_styled(
                        text(icon, color),
                        Style::absolute(
                            [
                                tile_width / 2.0 - font_size() / 2.0,
                                thumbnail_height() / 2.0 - font_size() / 2.0,
                            ],
                            [2.0 * font_size(), 1.5 * font_size()],
                        ),
                    )
                },
                |image| thumbnail_tree(&image, [tile_width, thumbnail_height()]),
            );
        let below = Rows::with_height(thumbnail_height(), row_height());
        let label = below.row(
            0,
            tile_width,
            None,
            vec![below.text(
                text(name, palette.fg),
                POPUP_PAD / 2.0,
                tile_width - POPUP_PAD / 2.0,
            )],
        );
        UITree::new_styled(
            background.map_or(UIElement::Nothing, UIElement::Backgrounded),
            Style::absolute(corner, [tile_width, tile_height]),
            vec![picture, label],
        )
        .with_pointer()
    }

    /// `path`'s thumbnail, decoded the first time it's shown
    fn thumbnail(&self, path: &str) -> Option<Image> {
        self.thumbnails
            .borrow_mut()
            .entry(path.to_owned())
            .or_insert_with(|| load_thumbnail(&expand(path)?))
            .clone()
    }
}

fn load_thumbnail(path: &Path) -> Option<Image> {
    if std::fs::metadata(path).ok()?.len() > THUMBNAIL_MAX_BYTES {
        return None;
    }
    let image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .thumbnail(THUMBNAIL_PIXELS, THUMBNAIL_PIXELS)
        .into_rgba8();
    Image::new(image.width(), image.height(), image.into_raw())
}

/// `image` fitted, centered, into a box `size` big (less a margin)
fn thumbnail_tree(image: &Image, [width, height]: [f32; 2]) -> UITree {
    let margin = POPUP_PAD / 2.0;
    let [room_width, room_height] = [width - 2.0 * margin, height - margin];
    let [image_width, image_height] = image.size().map(|px| px as f32);
    let scale = (room_width / image_width).min(room_height / image_height);
    let size = [image_width * scale, image_height * scale];
    UITree::leaf_styled(
        UIElement::Image(image.clone()),
        Style::absolute(
            [
                (width - size[0]) / 2.0,
                margin + (room_height - size[1]) / 2.0,
            ],
            size,
        ),
    )
}

/// How many `unit`s fit in `length` (none if it's negative)
#[expect(clippy::cast_sign_loss, reason = "clamped non-negative")]
fn fit(length: f32, unit: f32) -> usize {
    (length / unit).max(0.0) as usize
}

fn thumbnail_height() -> f32 {
    4.5 * font_size()
}

/// A tile: its thumbnail or icon over its name
fn tile_size() -> [f32; 2] {
    [7.0 * font_size(), thumbnail_height() + row_height()]
}

/// Below the path and the hint
fn grid_top() -> f32 {
    POPUP_PAD + 2.0 * row_height() + POPUP_PAD / 2.0
}

fn up_right() -> f32 {
    POPUP_PAD + UP.chars().count() as f32 * 0.6 * font_size() + POPUP_PAD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_the_file_and_opens_folders() {
        let dir = std::env::temp_dir().join(format!("sonamu-chooser-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for file in ["a.glsl", "b.png", "sub/c.png"] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let mut chooser = FileChooser::new(&dir.join("b.png"));
        let names: Vec<&str> = chooser
            .input
            .options
            .iter()
            .map(super::super::path_input::PathOption::name)
            .collect();
        assert_eq!(names, ["sub", "a.glsl", "b.png"]);
        assert_eq!(chooser.input.selected, Some(2));
        assert_eq!(
            chooser.handle_key(KeySymbol::Enter, KeyModifiers::NONE),
            Chosen::Picked(dir.join("b.png"))
        );
        chooser.handle_key(KeySymbol::ArrowKeyLeft, KeyModifiers::NONE);
        chooser.handle_key(KeySymbol::ArrowKeyLeft, KeyModifiers::NONE);
        assert_eq!(
            chooser.handle_key(KeySymbol::Enter, KeyModifiers::NONE),
            Chosen::Not
        );
        assert_eq!(chooser.input.options.len(), 1);
        assert_eq!(
            chooser.handle_key(KeySymbol::Enter, KeyModifiers::NONE),
            Chosen::Not,
            "nothing selected, the typed path is a folder"
        );
        chooser.handle_key(KeySymbol::ArrowKeyRight, KeyModifiers::NONE);
        assert_eq!(
            chooser.handle_key(KeySymbol::Enter, KeyModifiers::NONE),
            Chosen::Picked(dir.join("sub/c.png"))
        );
        assert_eq!(
            chooser.handle_key(KeySymbol::Escape, KeyModifiers::NONE),
            Chosen::Closed
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
