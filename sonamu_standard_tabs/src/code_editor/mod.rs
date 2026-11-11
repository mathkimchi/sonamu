use ropey::Rope;
use sonamu_common::{
    command::{self, CommandResult, Signature, Value},
    sap::packets::StandardEvent,
    settings::{Palette, Settings},
};
use sonamu_sttk::{
    creatable_applet::CreatableNodularApplet,
    nodular_applet::{
        AppletContext, AppletSpawner, AppletSpawnerTrait, NodularAppletInitializer, StandardApplet,
        with_opacity,
    },
    standard_keybinds::handle_standard_keybinds,
};
use sonamu_ui::{
    color::Color,
    ui_element::{CharCellStyle, CharGrid, Style, UIElement, UITree},
    ui_event::{KeyModifiers, KeySymbol},
};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::PathBuf,
};

/// Handles scroll offset
/// TODO: fold, lines that span multiple rows, in-line embeds like comments or document embeds (for custom note-taking language), horizontal scroll
struct ViewOffset {
    /// What text line is the uppermost row?
    scroll: usize,

    most_recent_num_rows: Option<usize>,
}
impl ViewOffset {
    // TODO: make something that iterates the line_idx's?
    const fn disp_row_to_line_idx(&self, display_row: usize) -> usize {
        display_row + self.scroll
    }

    /// Positive is down
    /// Negative is up
    const fn add_scroll(&mut self, offset: isize) {
        self.scroll = self.scroll.saturating_add_signed(offset);
    }

    fn clamp_to_cursor(&mut self, cursor_line: usize) {
        // the top row displayed (scroll) should be at cursor_line or above (minus)
        self.scroll = self.scroll.clamp(
            // at least one row, or the bounds cross (and `clamp` panics) in a pane too small for a row
            if let Some(num_rows) = self.most_recent_num_rows {
                (cursor_line + 1).saturating_sub(num_rows.max(1))
            } else {
                0
            },
            cursor_line,
        );
    }
}

#[derive(Debug, Clone, Copy)]
enum EditorMode {
    Normal,
    Insert,
}

/// What `command` takes
const COMMANDS: [Signature; 3] = [
    Signature {
        name: "path",
        params: "",
        returns: "string",
        effect: command::Effect::Read,
        doc: "the file it edits",
    },
    Signature {
        name: "text",
        params: "",
        returns: "string",
        effect: command::Effect::Read,
        doc: "what is in it now",
    },
    Signature {
        name: "save",
        params: "",
        returns: "nil",
        effect: command::Effect::Modify,
        doc: "write it to its file",
    },
];

pub struct CodeEditorApplet {
    file_path: PathBuf,

    buffer: Rope,
    /// Set if the file couldn't be read; saving is then disabled so it isn't overwritten
    load_error: Option<String>,

    /// In char, not bytes
    cursor: usize,
    focused: bool,
    mode: EditorMode,

    view_offset: ViewOffset,
    palette: Palette,

    hook: AppletContext,
}
impl<P> CreatableNodularApplet<P> for CodeEditorApplet
where
    P: AsRef<std::path::Path> + Send + 'static,
    PathBuf: From<P>,
{
    fn new(file_path: P, hook: AppletContext) -> Self {
        // Is reader overkill? Should I just have read?
        let read = File::open(&file_path).and_then(|file| Rope::from_reader(BufReader::new(file)));
        let (buffer, load_error) = match read {
            Ok(buffer) => (buffer, None),
            // a new file, created on save
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (Rope::new(), None),
            // shown instead of the file, which is never saved over
            Err(err) => (
                Rope::from(format!("Can't open: {err}")),
                Some(err.to_string()),
            ),
        };
        let file_path = PathBuf::from(file_path);

        Self {
            file_path,
            buffer,
            load_error,

            cursor: 0,
            focused: true,
            mode: EditorMode::Normal,

            view_offset: ViewOffset {
                scroll: 0,
                most_recent_num_rows: None,
            },
            palette: Palette::default(),

            hook,
        }
    }
}
impl CodeEditorApplet {
    #[must_use]
    pub fn get_applet_spawner() -> AppletSpawner {
        struct EditorSpawner;
        impl AppletSpawnerTrait for EditorSpawner {
            fn create_initializer(&self, args: &[&str]) -> Option<NodularAppletInitializer> {
                let file_path = args.first()?;

                Some(CodeEditorApplet::get_boxed_initiator(file_path.to_string()))
            }

            fn commands(&self) -> &'static [Signature] {
                &COMMANDS
            }

            fn duplicate(&self) -> AppletSpawner {
                Box::new(Self)
            }
        }
        Box::new(EditorSpawner)
    }

    fn get_title(&self) -> String {
        self.file_path.file_name().map_or_else(
            || self.file_path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    fn save_buffer(&self) {
        if let Some(err) = &self.load_error {
            log::warn!(
                "Not saving {}: it couldn't be read ({err})",
                self.file_path.display()
            );
            return;
        }
        let save = || -> std::io::Result<()> {
            let mut dest = BufWriter::new(File::create(&self.file_path)?);
            self.buffer.write_to(&mut dest)?;
            dest.flush()
        };
        if let Err(err) = save() {
            log::error!("Couldn't save {}: {err}", self.file_path.display());
        }
    }

    /// The char index at `column` of `line`, or the end of the line (before its line break) if it's shorter
    fn cursor_at(&self, line: usize, column: usize) -> usize {
        let line_slice = self.buffer.line(line);
        let line_break = usize::from(line_slice.chars().last() == Some('\n'));
        self.buffer.line_to_char(line) + column.min(line_slice.len_chars() - line_break)
    }

    fn clamp_view_to_cursor(&mut self) {
        self.view_offset
            .clamp_to_cursor(self.buffer.char_to_line(self.cursor));
    }

    // fn set_cursor(&mut self, new_cursor: usize) {
    //     self.cursor = new_cursor;
    //     self.clamp_view_to_cursor();
    // }

    fn handle_keypress_normal(&mut self, key: KeySymbol, key_modifiers: KeyModifiers) {
        match (key, key_modifiers) {
            (KeySymbol::Char('i'), KeyModifiers::NONE) => {
                self.mode = EditorMode::Insert;
            }
            (KeySymbol::Char('a'), KeyModifiers::NONE) => {
                self.mode = EditorMode::Insert;
                // TODO: do this with actions system (will need to make an actions system)
                self.cursor = (self.cursor + 1).min(self.buffer.len_chars());
                self.clamp_view_to_cursor();
            }
            _ => {}
        }
    }

    fn handle_keypress_insert(&mut self, key: KeySymbol, key_modifiers: KeyModifiers) {
        match (key, key_modifiers) {
            (KeySymbol::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.buffer.insert_char(self.cursor, c);
                self.cursor += 1;
                self.clamp_view_to_cursor();
            }
            (KeySymbol::Enter, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.buffer.insert_char(self.cursor, '\n');
                self.cursor += 1;
                self.clamp_view_to_cursor();
            }
            (KeySymbol::Backspace, KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                if let Some(prev_idx) = self.cursor.checked_sub(1) {
                    self.buffer.remove(prev_idx..self.cursor);
                    self.cursor = prev_idx;
                    self.clamp_view_to_cursor();
                }
            }
            (KeySymbol::Escape, _) => {
                self.mode = EditorMode::Normal;
            }
            (KeySymbol::Char('v'), KeyModifiers::CTRL)
            | (KeySymbol::Char('V'), KeyModifiers::CTRL_SHIFT) => {
                if let Some(text) = sonamu_common::clipboard::paste() {
                    self.buffer.insert(self.cursor, &text);
                    // in chars, like the cursor
                    self.cursor += text.chars().count();
                    self.clamp_view_to_cursor();
                }
            }
            _ => {}
        }
    }

    fn handle_keypress(&mut self, key: KeySymbol, key_modifiers: KeyModifiers) {
        match (key, key_modifiers) {
            // global keypresses
            (KeySymbol::Char('s'), KeyModifiers::CTRL) => {
                // log::info!("Saving!");
                self.save_buffer();
            }
            (KeySymbol::ArrowKeyLeft, KeyModifiers::NONE) => {
                self.cursor = self.cursor.saturating_sub(1);
                self.clamp_view_to_cursor();
            }
            (KeySymbol::ArrowKeyRight, KeyModifiers::NONE) => {
                self.cursor = (self.cursor + 1).min(self.buffer.len_chars());
                self.clamp_view_to_cursor();
            }
            (KeySymbol::ArrowKeyUp, KeyModifiers::NONE) => {
                let line = self.buffer.char_to_line(self.cursor);
                let column = self.cursor - self.buffer.line_to_char(line);

                self.cursor = self.cursor_at(line.saturating_sub(1), column);
                self.clamp_view_to_cursor();
            }
            (KeySymbol::ArrowKeyDown, KeyModifiers::NONE) => {
                let line = self.buffer.char_to_line(self.cursor);
                let column = self.cursor - self.buffer.line_to_char(line);

                let new_line = (line + 1).min(self.buffer.len_lines() - 1);
                self.cursor = self.cursor_at(new_line, column);
                self.clamp_view_to_cursor();
            }
            // these are mostly here for debug purposes
            (KeySymbol::PageDown, KeyModifiers::NONE) => self.view_offset.add_scroll(1),
            (KeySymbol::PageUp, KeyModifiers::NONE) => self.view_offset.add_scroll(-1),
            _ => match self.mode {
                EditorMode::Normal => self.handle_keypress_normal(key, key_modifiers),
                EditorMode::Insert => self.handle_keypress_insert(key, key_modifiers),
            },
        }
    }
}
impl StandardApplet for CodeEditorApplet {
    fn command(&mut self, name: &str, _args: Vec<Value>) -> CommandResult {
        Ok(match name {
            "path" => Value::String(self.file_path.to_string_lossy().into_owned()),
            "text" => Value::String(self.buffer.to_string()),
            "save" => {
                if let Some(err) = &self.load_error {
                    return Err(format!("not saving: it couldn't be read ({err})"));
                }
                self.save_buffer();
                Value::Nil
            }
            _ => return Err(command::unknown(name)),
        })
    }

    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        match standard_event {
            StandardEvent::UIEvent(ui_event) => {
                if handle_standard_keybinds(&ui_event, &self.hook) {
                    return;
                }

                match ui_event {
                    sonamu_ui::ui_event::UIEvent::Key {
                        symbol: Some(key),
                        modifiers,
                        pressed: true,
                        ..
                    } => {
                        self.handle_keypress(key, modifiers);
                    }
                    sonamu_ui::ui_event::UIEvent::Key { .. }
                    | sonamu_ui::ui_event::UIEvent::Swipe { .. }
                    | sonamu_ui::ui_event::UIEvent::Pinch(_) => {}
                    sonamu_ui::ui_event::UIEvent::WindowResized(display_container_size) => {
                        let (_width, height) =
                            CharGrid::largest_fittable_size(display_container_size);

                        self.view_offset.most_recent_num_rows = Some(height);
                    }
                    sonamu_ui::ui_event::UIEvent::Mouse(_) => {
                        log::debug!("TODO");
                    }
                }

                self.hook.damage_window();
            }
            StandardEvent::FocusChanged(focus) => {
                self.focused = focus;
                // println!("Yay focus {focus}!");
                // println!("The text is: {}", &self.buffer.);

                self.hook.damage_window();
            }
            _ => {}
        }
    }

    fn get_window_standard_applet(
        &self,
        container_size: sonamu_ui::display_units::DisplayContainerSize,
    ) -> UITree {
        let (width, height) = CharGrid::largest_fittable_size(container_size);

        // let line_idx = self.buffer.char_to_line(self.cursor);
        // log::info!("Cursor position: {}", self.cursor);

        // From ropey documentation: `char_idx` can be one-past-the-end, which will return the last line index.
        let cursor_line = self.buffer.char_to_line(self.cursor);

        let mut content = CharGrid::new_empty(width, height);
        for row in 0..height {
            let line_idx = self.view_offset.disp_row_to_line_idx(row);

            let Some(line) = self.buffer.get_line(line_idx) else {
                break;
            };

            for (col, mut c) in line.chars().take(width).enumerate() {
                if c == '\n' {
                    // we can't automatically rule out all final characters, bc of the final line
                    c = ' ';
                }

                content.set_char(c, row, col);
                content.set_fg(self.palette.fg, row, col);

                // // currently scroll is only horizontal
                // // ^- Erhm actchually, currently, scroll DNE. But the architecture for it only supports horizontal
                // //    ^- Erhm actchuahllie, scroll exists now
                // let content_idx = self.buffer.line_to_char(line_idx) + col;
            }

            if line_idx == cursor_line {
                let cursor_col = self.cursor - self.buffer.line_to_char(line_idx);
                if cursor_col < width {
                    match self.mode {
                        EditorMode::Normal => {
                            let cursor_bg = if self.focused {
                                self.palette.accent
                            } else {
                                self.palette.inactive
                            };
                            content
                                .get_char_mut(row, cursor_col)
                                .set_fg(Color::TRANSPARENT)
                                .set_bg(cursor_bg);
                        }
                        EditorMode::Insert => {
                            content
                                .get_char_mut(row, cursor_col)
                                .add_style(CharCellStyle::CURSOR_LINE);
                            // log::debug!(
                            //     "Cursor style: {}",
                            //     (content.get_char(row, col).style.0 >> 2) & 1
                            // );
                        }
                    }
                }
            }
        }

        /// I think this is good, but it might be a tad bid disorienting to resize a lot (it feels jiggly)
        const EXPAND_EDITOR_TEXT: bool = true;

        let content = if EXPAND_EDITOR_TEXT {
            content.element().into()
        } else {
            content.contained_element()
        };
        UITree::new_styled(
            UIElement::Backgrounded(with_opacity(self.palette.bg)),
            Style::full_size(),
            vec![content],
        )

        // UIElement::Text(vec![
        //     (
        //         self.buffer.slice(..self.cursor).to_string(),
        //         UIElement::glyphon_attr(Color::WHITE),
        //     ),
        //     (
        //         self.buffer.char(self.cursor).to_string(),
        //         // TODO: right now, this changes fg color,
        //         // glyphon doesn't do bg so I'll have to deal w that manually later
        //         // might even need to do glyph rendering manually bruh (i'm cryng)
        //         UIElement::glyphon_attr(cursor_color),
        //     ),
        //     (
        //         self.buffer.slice((self.cursor + 1)..).to_string(),
        //         UIElement::glyphon_attr(Color::WHITE),
        //     ),
        // ])
    }

    fn settings_changed(&mut self, settings: &Settings) {
        self.palette = settings.palette.clone();
    }

    fn get_treeview(&self) -> sonamu_common::utils::tree::world_tree::WorldTree<String> {
        sonamu_common::utils::tree::world_tree::WorldTree::Base(self.get_title())
    }

    fn save(&self) -> Option<sonamu_common::sap::session::SavedApplet> {
        sonamu_sttk::session::saved_with_path("code_editor", &self.file_path)
    }
}
