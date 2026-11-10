//! TODO: move to sttk

pub(crate) mod repl;

use crate::{
    nodular_applet::{
        AppletContext, AppletSpawner, AppletSpawnerTrait, NodeRequest, NodularAppletInitializer,
        StandardApplet, treeview::with_opacity,
    },
    standard_keybinds::handle_standard_keybinds,
};
use repl::{Output, Repl};
use sonamu_common::{
    command::{self, CommandResult, Signature, Value},
    sap::{packets::StandardEvent, session::SavedWl},
    settings::{self, Palette, Settings},
};
use sonamu_ui::{
    color::Color,
    display_units::DisplayContainerSize,
    ui_element::{CharGrid, UITree},
    ui_event::{KeyModifiers, KeySymbol, MouseEventKind, UIEvent},
};
use std::{path::PathBuf, sync::Arc};

/// What `command` takes
const COMMANDS: [Signature; 4] = [
    Signature {
        name: "clear",
        params: "",
        returns: "nil",
        doc: "clear the history",
    },
    Signature {
        name: "set_title",
        params: "title: string",
        returns: "nil",
        doc: "rename this hub",
    },
    Signature {
        name: "open",
        params: "applet: string, args: {string}?",
        returns: "nil",
        doc: "replace this hub with an applet (`\"wl\", {argv}`: a program's window)",
    },
    Signature {
        name: "run_output",
        params: "program: string, args: {string}?",
        returns: "string",
        doc: "run a program and wait for its output",
    },
];

/// Lines per wheel notch
const SCROLL_STEP: usize = 3;
pub(crate) const PROMPT: &str = "> ";

// The help applet uses these too
pub(crate) const BG: Color = with_opacity(Color([0x0D, 0x0D, 0x16, 0]));
pub(crate) const BAR_BG: Color = with_opacity(Color([0x26, 0x2A, 0x32, 0]));

/// The hub's and buffer line's background: the palette's, translucent like the treeview
pub(crate) const fn bg(palette: &Palette) -> Color {
    with_opacity(palette.bg)
}

/// Their bars' (prompt, header, commands): the background a bit toward the text
pub(crate) fn bar_bg(palette: &Palette) -> Color {
    let (Color(bg), Color(fg)) = (palette.bg, palette.fg);
    let mix = |index: usize| {
        let (bg, fg) = (u16::from(bg[index]), u16::from(fg[index]));
        // 15% of the way, in u8 range
        u8::try_from((bg * 85 + fg * 15) / 100).unwrap_or(u8::MAX)
    };
    with_opacity(Color([mix(0), mix(1), mix(2), 0]))
}
pub(crate) const ACCENT: Color = Color([0x5F, 0xD7, 0x87, 0xFF]);
pub(crate) const OUTPUT: Color = Color([0xC8, 0xCC, 0xD4, 0xFF]);
pub(crate) const DIM: Color = Color([0x8A, 0x93, 0xA6, 0xFF]);
pub(crate) const ERROR: Color = Color([0xFF, 0x6A, 0x6A, 0xFF]);
pub(crate) const RUNNING: Color = Color([0xE5, 0xC0, 0x7A, 0xFF]);
pub(crate) const CURSOR: Color = Color([0xDB, 0xDB, 0xDB, 0xFF]);

/// Technically, this is a standard app, but it is so essential
/// that I am considering it a tool for sttk.
///
/// This is a like cross between a terminal and any fast command palette
/// like Windows' start menu (the thing that pops up when you press Windows).
pub struct CommandHubApplet {
    title: String,
    repl: Repl,
    /// Its text's colors (`settings_changed`)
    palette: Palette,
    /// Lines scrolled up from the bottom
    scroll: usize,

    /// Where `run_output` starts programs: the directory of the tree it is in (eg a project's); the runner's if `None`
    directory: Option<PathBuf>,

    size: DisplayContainerSize,
    hook: AppletContext,
}
impl CommandHubApplet {
    #[must_use]
    pub fn new(hook: AppletContext) -> Self {
        hook.get_directory();
        let waker = hook.waker();
        Self {
            title: "Command Hub".to_string(),
            repl: Repl::new(Arc::new(move || {
                waker.wake();
            })),
            scroll: 0,
            palette: settings::get().palette,
            directory: None,
            size: DisplayContainerSize::new(800, 600),
            hook,
        }
    }
    pub fn get_initiator() -> impl FnOnce(AppletContext) -> Self {
        |hook: AppletContext| Self::new(hook)
    }
    #[must_use]
    pub fn get_boxed_initiator() -> NodularAppletInitializer {
        NodularAppletInitializer::new(|hook: AppletContext| Box::new(Self::new(hook)))
    }
    #[must_use]
    pub fn get_applet_spawner() -> AppletSpawner {
        struct CommandHubSpawner;
        impl AppletSpawnerTrait for CommandHubSpawner {
            fn create_initializer(&self, args: &[&str]) -> Option<NodularAppletInitializer> {
                if !args.is_empty() {
                    println!("Warning: command hub doesn't use spawn args.");
                }

                Some(CommandHubApplet::get_boxed_initiator())
            }

            fn commands(&self) -> &'static [Signature] {
                &COMMANDS
            }

            fn duplicate(&self) -> AppletSpawner {
                Box::new(Self)
            }
        }
        Box::new(CommandHubSpawner)
    }

    /// `open`: replaces the hub with an applet, or with a program's window (`wl`)
    fn open(&self, args: &[&str]) -> Result<String, String> {
        match args {
            [] => Err("Must specify an applet, or `wl` and a program.".to_string()),
            ["wl"] => Err("Must specify a program.".to_string()),
            ["wl", argv @ ..] => {
                self.hook.request(NodeRequest::StartWl(SavedWl {
                    argv: argv.iter().map(ToString::to_string).collect(),
                    cwd: self.directory.clone(),
                    app_id: None,
                    title: String::new(),
                    running: None,
                }));
                Ok(format!("Starting {}", argv[0]))
            }
            [app_name, args @ ..] => {
                let spawner = self.hook.find_applet_spawner(app_name).ok_or_else(|| {
                    format!(
                        "Couldn't find app {app_name}. Known applets: {}.",
                        self.applet_names().join(", ")
                    )
                })?;
                let initializer = spawner
                    .create_initializer(args)
                    .ok_or_else(|| format!("Couldn't create initializer from args {args:?}."))?;
                self.hook.request(NodeRequest::Replace(initializer));
                Ok(String::new())
            }
        }
    }

    /// `program_command`, in the hub's directory
    fn program_command(&self, program: &str, args: &[&str]) -> std::process::Command {
        let mut command = program_command(program, args);
        if let Some(directory) = &self.directory {
            command.current_dir(directory);
        }
        command
    }

    fn applet_names(&self) -> Vec<String> {
        self.hook.get_applet_spawners().into_keys().collect()
    }

    /// Positive scrolls up (back in the history)
    fn scroll_by(&mut self, lines: isize) {
        let (width, _) = CharGrid::largest_fittable_size(self.size);
        let max = self
            .repl
            .lines(width, 0, &self.palette)
            .len()
            .saturating_sub(self.body_rows());
        self.scroll = self.scroll.saturating_add_signed(lines).min(max);
    }

    /// Rows between the header and the prompt
    fn body_rows(&self) -> usize {
        let (_, height) = CharGrid::largest_fittable_size(self.size);
        height.saturating_sub(2)
    }

    fn handle_key(&mut self, key: KeySymbol, modifiers: KeyModifiers) {
        match key {
            KeySymbol::PageUp => self.scroll_by(self.body_rows().saturating_sub(1) as isize),
            KeySymbol::PageDown => self.scroll_by(-(self.body_rows().saturating_sub(1) as isize)),
            KeySymbol::Enter => {
                self.scroll = 0;
                self.repl.handle_key(key, modifiers);
            }
            _ => {
                self.repl.handle_key(key, modifiers);
            }
        }
    }
}
impl StandardApplet for CommandHubApplet {
    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        match standard_event {
            StandardEvent::UIEvent(ui_event) => {
                if handle_standard_keybinds(&ui_event, &self.hook) {
                    return;
                }

                match ui_event {
                    UIEvent::Key {
                        symbol: Some(key),
                        modifiers,
                        pressed: true,
                        ..
                    } => self.handle_key(key, modifiers),
                    UIEvent::Mouse(mouse_event) => {
                        let MouseEventKind::Scroll { delta, .. } = mouse_event.kind else {
                            return;
                        };
                        // positive scrolls down, toward the newest
                        let step = SCROLL_STEP as isize;
                        self.scroll_by(if delta[1] > 0.0 { -step } else { step });
                    }
                    UIEvent::WindowResized(size) => self.size = size,
                    UIEvent::Key { .. } | UIEvent::Swipe { .. } => return,
                }

                self.hook.damage_window();
            }
            StandardEvent::Directory(directory) => self.directory = directory,
            // looks the same either way
            StandardEvent::FocusChanged(_) | StandardEvent::Highlighted(_) => {}
            // nothing to ask about first
            StandardEvent::CloseRequest => self.hook.close(),
            // handled by the applet's client
            StandardEvent::DamageAck(_)
            | StandardEvent::Deactivate
            | StandardEvent::SettingsChanged(_)
            | StandardEvent::Pluck(_)
            | StandardEvent::Place(..)
            | StandardEvent::NodeAction(..)
            | StandardEvent::Focus(_)
            | StandardEvent::ToggleCollapsed(_)
            | StandardEvent::Command { .. } => {}
            // doesn't show wl surfaces (only the root gets these)
            StandardEvent::WlSurfaceRegistered(_)
            | StandardEvent::Capturing(_)
            | StandardEvent::Screens(_)
            | StandardEvent::ScreenViews { .. }
            | StandardEvent::Open(_) => {}
            // has no inner world
            StandardEvent::Swipe(_) => {}
        }
    }

    fn settings_changed(&mut self, settings: &Settings) {
        self.palette = settings.palette.clone();
    }

    /// An eval printed or finished, or Tab's candidates came
    fn wake(&mut self) {
        self.repl.wake();
        self.hook.damage_window();
    }

    fn command(&mut self, name: &str, args: Vec<Value>) -> CommandResult {
        match name {
            "clear" => self.repl.history.clear(),
            "set_title" => {
                self.title = command::string_arg(&args, 0)?.to_string();
                self.hook.damage_treeview();
            }
            // replaces the hub in its node (#116)
            "open" => {
                let mut words = vec![command::string_arg(&args, 0)?.to_string()];
                words.extend(command::strings_arg(&args, 1)?);
                let words: Vec<&str> = words.iter().map(String::as_str).collect();
                self.open(&words)?;
            }
            // NOTE: the hub waits for the program
            "run_output" => {
                let program = command::string_arg(&args, 0)?;
                let program_args = command::strings_arg(&args, 1)?;
                let program_args: Vec<&str> = program_args.iter().map(String::as_str).collect();
                return match run_captured(&mut self.program_command(program, &program_args)) {
                    Output::Text(text) => Ok(Value::String(text)),
                    Output::Error(err) => Err(err),
                    Output::Running(_) => unreachable!("it waited"),
                };
            }
            _ => return Err(command::unknown(name)),
        }
        Ok(Value::Nil)
    }

    fn get_treeview(&self) -> sonamu_common::utils::tree::world_tree::WorldTree<String> {
        sonamu_common::utils::tree::world_tree::WorldTree::Base(self.title.clone())
    }

    fn save(&self) -> Option<sonamu_common::sap::session::SavedApplet> {
        Some(sonamu_common::sap::session::SavedApplet::Spawner {
            name: "command_hub".to_string(),
            args: Vec::new(),
            inner: None,
        })
    }

    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree {
        let (width, height) = CharGrid::largest_fittable_size(container_size);
        let mut grid = CharGrid::new_empty(width, height);
        if width == 0 || height == 0 {
            return grid.element().into();
        }
        for row in 0..height {
            fill_row(&mut grid, row, bg(&self.palette));
        }

        self.repl.draw_prompt(&mut grid, height - 1, &self.palette);

        if height < 2 {
            return grid.element().into();
        }
        let lines = self.repl.lines(width, 0, &self.palette);
        let body_rows = height - 2;
        let scroll = self.scroll.min(lines.len().saturating_sub(body_rows));

        // header: title, and a hint or how far it's scrolled
        fill_row(&mut grid, 0, bar_bg(&self.palette));
        put(&mut grid, 0, 1, &self.title, self.palette.fg);
        let note = if scroll > 0 {
            format!("{scroll} lines below (PageDown) ")
        } else {
            "help | Tab | Up/Down | Ctrl+C ".to_string()
        };
        let title_end = self.title.chars().count() + 2;
        if let Some(column) = width
            .checked_sub(note.len())
            .filter(|&column| column > title_end)
        {
            put(&mut grid, 0, column, &note, self.palette.inactive);
        }

        // the newest lines, at the bottom of the body
        let end = lines.len() - scroll;
        let shown = &lines[end.saturating_sub(body_rows)..end];
        let top = 1 + body_rows - shown.len();
        repl::draw_lines(&mut grid, top, shown, self.palette.accent);

        grid.element().into()
    }
}

/// A process for `program`, connected to Sonamu if it's a wl client
pub fn program_command(program: &str, args: &[&str]) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    command.args(args);
    sonamu_common::connect_wl_client(&mut command);
    command
}

/// Waits for the process; its stdout, then stderr. An error if it failed.
fn run_captured(command: &mut std::process::Command) -> Output {
    match command.output() {
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            if output.status.success() {
                Output::Text(text)
            } else {
                text.push_str(&output.status.to_string());
                Output::Error(text)
            }
        }
        Err(err) => Output::Error(err.to_string()),
    }
}

pub(crate) fn fill_row(grid: &mut CharGrid, row: usize, bg: Color) {
    for column in 0..grid.width() {
        grid.get_char_mut(row, column).set_bg(bg);
    }
}

/// Writes `text` from `column`, cut off at the grid's edge; returns the column after it
pub(crate) fn put(grid: &mut CharGrid, row: usize, column: usize, text: &str, fg: Color) -> usize {
    let mut end = column;
    for c in text.chars() {
        if end >= grid.width() {
            break;
        }
        grid.get_char_mut(row, end).set_char(c).set_fg(fg);
        end += 1;
    }
    end
}
