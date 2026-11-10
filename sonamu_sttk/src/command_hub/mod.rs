//! TODO: move to sttk

use crate::{
    nodular_applet::{
        AppletContext, AppletSpawner, AppletSpawnerTrait, NodeRequest, NodularAppletInitializer,
        StandardApplet, treeview::with_opacity,
    },
    standard_keybinds::handle_standard_keybinds,
};
use sonamu_common::{
    command::{self, CommandResult, Value},
    sap::{packets::StandardEvent, session::SavedWl},
};
use sonamu_script::EvalHandle;
use sonamu_ui::{
    color::Color,
    display_units::DisplayContainerSize,
    ui_element::{CharGrid, UITree},
    ui_event::{KeyModifiers, KeySymbol, MouseEventKind, UIEvent},
};
use std::{fmt::Write as _, path::PathBuf, sync::mpsc};

/// Kept per hub; older ones are dropped
const MAX_ENTRIES: usize = 200;
/// Longer outputs are cut off
const MAX_OUTPUT_LINES: usize = 500;
/// Lines per wheel notch
const SCROLL_STEP: usize = 3;
const PROMPT: &str = "> ";

// The help applet uses these too
pub(crate) const BG: Color = with_opacity(Color([0x0D, 0x0D, 0x16, 0]));
pub(crate) const BAR_BG: Color = with_opacity(Color([0x26, 0x2A, 0x32, 0]));
pub(crate) const ACCENT: Color = Color([0x5F, 0xD7, 0x87, 0xFF]);
pub(crate) const OUTPUT: Color = Color([0xC8, 0xCC, 0xD4, 0xFF]);
pub(crate) const DIM: Color = Color([0x8A, 0x93, 0xA6, 0xFF]);
const ERROR: Color = Color([0xFF, 0x6A, 0x6A, 0xFF]);
pub(crate) const RUNNING: Color = Color([0xE5, 0xC0, 0x7A, 0xFF]);
pub(crate) const CURSOR: Color = Color([0xDB, 0xDB, 0xDB, 0xFF]);
const COMMAND: Color = Color::WHITE;

enum Output {
    Text(String),
    Error(String),
    /// Still running on the script thread
    Running(u64),
}

struct Entry {
    command: String,
    /// Its `print`s so far
    printed: String,
    output: Output,
}

/// An entry's code running on the script thread
struct Eval {
    job: u64,
    handle: EvalHandle,
    outputs: mpsc::Receiver<sonamu_script::Output>,
}

/// A line of the scrollback
struct Line {
    text: String,
    fg: Color,
    bg: Color,
    /// Leading chars drawn in `ACCENT` (the prompt of a command)
    accent: usize,
}

/// Technically, this is a standard app, but it is so essential
/// that I am considering it a tool for sttk.
///
/// This is a like cross between a terminal and any fast command palette
/// like Windows' start menu (the thing that pops up when you press Windows).
pub struct CommandHubApplet {
    title: String,

    history: Vec<Entry>,
    /// The thing that is currently being typed.
    current_prompt: String,
    /// In chars, into `current_prompt`
    cursor: usize,
    /// Lines scrolled up from the bottom
    scroll: usize,
    /// Shown above the prompt until the next key (eg Tab's candidates)
    hint: Option<String>,

    /// Entered commands, for Up/Down
    recall: Vec<String>,
    /// Into `recall` while recalling, with the prompt from before
    recalling: Option<(usize, String)>,

    evals: Vec<Eval>,
    next_job: u64,

    /// Where `run_output` starts programs: the directory of the tree it is in (eg a project's); the runner's if `None`
    directory: Option<PathBuf>,

    size: DisplayContainerSize,
    hook: AppletContext,
}
impl CommandHubApplet {
    #[must_use]
    pub fn new(hook: AppletContext) -> Self {
        hook.get_directory();
        Self {
            title: "Command Hub".to_string(),
            history: Vec::new(),
            current_prompt: String::new(),
            cursor: 0,
            scroll: 0,
            hint: None,
            recall: Vec::new(),
            recalling: None,
            evals: Vec::new(),
            next_job: 0,
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

            fn duplicate(&self) -> AppletSpawner {
                Box::new(Self)
            }
        }
        Box::new(CommandHubSpawner)
    }

    /// Runs the entered code on the script thread
    fn start_eval(&mut self, code: &str) -> Output {
        let job = self.next_job;
        self.next_job += 1;
        let waker = self.hook.waker();
        let (handle, outputs) = sonamu_script::eval(code.to_string(), move || {
            waker.wake();
        });
        self.evals.push(Eval {
            job,
            handle,
            outputs,
        });
        Output::Running(job)
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

    fn handle_enter(&mut self) {
        let command = std::mem::take(&mut self.current_prompt);
        self.cursor = 0;
        self.recalling = None;
        self.scroll = 0;
        if command.trim().is_empty() {
            return;
        }
        if self.recall.last() != Some(&command) {
            self.recall.push(command.clone());
            if self.recall.len() > MAX_ENTRIES {
                self.recall.remove(0);
            }
        }

        let output = self.start_eval(&command);
        self.history.push(Entry {
            command,
            printed: String::new(),
            output,
        });
        if self.history.len() > MAX_ENTRIES {
            self.history.remove(0);
        }
    }

    /// Up (`back`) or Down through entered commands; Down past the newest gives back the prompt from before
    fn recall_command(&mut self, back: bool) {
        let index = match (&self.recalling, back) {
            (None, false) => return,
            (None, true) => self.recall.len().checked_sub(1),
            (Some((index, _)), true) => Some(index.saturating_sub(1)),
            (Some((index, _)), false) => Some(index + 1).filter(|&next| next < self.recall.len()),
        };
        match index {
            Some(index) => {
                let draft = match self.recalling.take() {
                    Some((_, draft)) => draft,
                    None => self.current_prompt.clone(),
                };
                self.current_prompt = self.recall[index].clone();
                self.recalling = Some((index, draft));
            }
            None => {
                if let Some((_, draft)) = self.recalling.take() {
                    self.current_prompt = draft;
                }
            }
        }
        self.cursor = self.current_prompt.chars().count();
    }

    fn handle_key(&mut self, key: KeySymbol, modifiers: KeyModifiers) {
        self.hint = None;
        match key {
            KeySymbol::Enter => self.handle_enter(),
            KeySymbol::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.current_prompt.remove(self.byte_index(self.cursor));
                }
            }
            KeySymbol::ArrowKeyLeft => self.cursor = self.cursor.saturating_sub(1),
            KeySymbol::ArrowKeyRight => {
                self.cursor = (self.cursor + 1).min(self.current_prompt.chars().count());
            }
            KeySymbol::ArrowKeyUp => self.recall_command(true),
            KeySymbol::ArrowKeyDown => self.recall_command(false),
            KeySymbol::PageUp => self.scroll_by(self.body_rows().saturating_sub(1) as isize),
            KeySymbol::PageDown => self.scroll_by(-(self.body_rows().saturating_sub(1) as isize)),
            KeySymbol::Escape => {
                self.current_prompt.clear();
                self.cursor = 0;
                self.recalling = None;
            }
            // cancels the running evals
            KeySymbol::Char('c' | 'C') if modifiers == KeyModifiers::CTRL => {
                for eval in &self.evals {
                    eval.handle.cancel();
                }
            }
            // Ctrl/Alt/Super combinations aren't text
            KeySymbol::Char(key_char)
                if !(modifiers.ctrl
                    || modifiers.alt
                    || modifiers.logo
                    || key_char.is_control()) =>
            {
                self.current_prompt
                    .insert(self.byte_index(self.cursor), key_char);
                self.cursor += 1;
            }
            KeySymbol::Char(_) => {}
        }
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.current_prompt
            .char_indices()
            .nth(char_index)
            .map_or(self.current_prompt.len(), |(index, _)| index)
    }

    /// Positive scrolls up (back in the history)
    fn scroll_by(&mut self, lines: isize) {
        let (width, _) = CharGrid::largest_fittable_size(self.size);
        let max = self.lines(width).len().saturating_sub(self.body_rows());
        self.scroll = self.scroll.saturating_add_signed(lines).min(max);
    }

    /// Rows between the header and the prompt
    fn body_rows(&self) -> usize {
        let (_, height) = CharGrid::largest_fittable_size(self.size);
        height.saturating_sub(2)
    }

    /// The whole scrollback, wrapped at `width`
    fn lines(&self, width: usize) -> Vec<Line> {
        let mut lines = Vec::new();
        for entry in &self.history {
            for (index, text) in wrap(&format!("{PROMPT}{}", entry.command), width).enumerate() {
                lines.push(Line {
                    text,
                    fg: COMMAND,
                    bg: BAR_BG,
                    accent: if index == 0 { PROMPT.len() } else { 0 },
                });
            }
            lines.extend(wrap(&entry.printed, width).map(|text| Line {
                text,
                fg: OUTPUT,
                bg: BG,
                accent: 0,
            }));
            let (text, fg) = match &entry.output {
                Output::Text(text) => (text.as_str(), OUTPUT),
                Output::Error(err) => (err.as_str(), ERROR),
                Output::Running(_) => ("running...", RUNNING),
            };
            if !text.is_empty() {
                lines.extend(wrap(text, width).map(|text| Line {
                    text,
                    fg,
                    bg: BG,
                    accent: 0,
                }));
            }
        }
        if let Some(hint) = &self.hint {
            lines.extend(wrap(hint, width).map(|text| Line {
                text,
                fg: DIM,
                bg: BG,
                accent: 0,
            }));
        }
        lines
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

    /// An eval printed or finished
    fn wake(&mut self) {
        let mut finished = Vec::new();
        for eval in &self.evals {
            // gone if cleared meanwhile
            let mut entry = self.history.iter_mut().find(
                |entry| matches!(entry.output, Output::Running(running) if running == eval.job),
            );
            while let Ok(output) = eval.outputs.try_recv() {
                let Some(entry) = entry.as_deref_mut() else {
                    continue;
                };
                match output {
                    sonamu_script::Output::Print(line) => {
                        entry.printed.push_str(&line);
                        entry.printed.push('\n');
                    }
                    sonamu_script::Output::Done(result) => entry.output = trimmed(shown(result)),
                }
            }
            if !matches!(entry.map(|entry| &entry.output), Some(Output::Running(_))) {
                finished.push(eval.job);
            }
        }
        self.evals.retain(|eval| !finished.contains(&eval.job));
        self.hook.damage_window();
    }

    fn command(&mut self, name: &str, args: Vec<Value>) -> CommandResult {
        match name {
            "clear" => self.history.clear(),
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
            fill_row(&mut grid, row, BG);
        }

        // the prompt, scrolled sideways to keep the cursor in view
        let prompt_row = height - 1;
        fill_row(&mut grid, prompt_row, BAR_BG);
        let start = put(&mut grid, prompt_row, 0, PROMPT, ACCENT);
        let room = width.saturating_sub(start).max(1);
        let skip = self.cursor.saturating_sub(room - 1);
        let visible: String = self.current_prompt.chars().skip(skip).collect();
        put(&mut grid, prompt_row, start, &visible, COMMAND);
        if let Some(cell) = (start + self.cursor - skip < width)
            .then(|| grid.get_char_mut(prompt_row, start + self.cursor - skip))
        {
            cell.set_fg(BAR_BG).set_bg(CURSOR);
        }

        if height < 2 {
            return grid.element().into();
        }
        let lines = self.lines(width);
        let body_rows = height - 2;
        let scroll = self.scroll.min(lines.len().saturating_sub(body_rows));

        // header: title, and a hint or how far it's scrolled
        fill_row(&mut grid, 0, BAR_BG);
        put(&mut grid, 0, 1, &self.title, CURSOR);
        let note = if scroll > 0 {
            format!("{scroll} lines below (PageDown) ")
        } else {
            "sonamu.help() | Up/Down | Ctrl+C ".to_string()
        };
        let title_end = self.title.chars().count() + 2;
        if let Some(column) = width
            .checked_sub(note.len())
            .filter(|&column| column > title_end)
        {
            put(&mut grid, 0, column, &note, DIM);
        }

        // the newest lines, at the bottom of the body
        let end = lines.len() - scroll;
        let shown = &lines[end.saturating_sub(body_rows)..end];
        let top = 1 + body_rows - shown.len();
        for (row, line) in (top..).zip(shown) {
            fill_row(&mut grid, row, line.bg);
            // the prompt is ASCII, so this is a char boundary
            let (accent, rest) = line.text.split_at(line.accent.min(line.text.len()));
            let column = put(&mut grid, row, 0, accent, ACCENT);
            put(&mut grid, row, column, rest, line.fg);
        }

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

/// An eval's return values (nothing for none), or its error
fn shown(result: Result<Vec<Value>, String>) -> Output {
    match result {
        Ok(values) => Output::Text(
            values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        Err(err) => Output::Error(err),
    }
}

/// Cuts off long outputs
fn trimmed(output: Output) -> Output {
    let trim = |text: String| {
        let count = text.lines().count();
        if count <= MAX_OUTPUT_LINES {
            return text;
        }
        let mut kept: String = text
            .lines()
            .take(MAX_OUTPUT_LINES)
            .flat_map(|line| [line, "\n"])
            .collect();
        let _ = write!(kept, "... ({} more lines)", count - MAX_OUTPUT_LINES);
        kept
    };
    match output {
        Output::Text(text) => Output::Text(trim(text)),
        Output::Error(err) => Output::Error(trim(err)),
        running @ Output::Running(_) => running,
    }
}

/// Each line of `text`, split into rows of `width` chars (tabs become spaces, other control chars are dropped)
fn wrap(text: &str, width: usize) -> impl Iterator<Item = String> {
    text.lines().flat_map(move |line| {
        let chars: Vec<char> = line
            .replace('\t', "    ")
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        let rows: Vec<String> = if chars.is_empty() {
            vec![String::new()]
        } else {
            chars
                .chunks(width.max(1))
                .map(|row| row.iter().collect())
                .collect()
        };
        rows
    })
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
