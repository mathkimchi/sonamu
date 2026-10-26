//! TODO: move to sttk

use crate::{
    nodular_applet::{
        AppletContext, AppletSpawner, AppletSpawnerTrait, NodeRequest, NodularAppletInitializer,
        StandardApplet, project_applet::ProjectApplet, treeview::with_opacity,
    },
    standard_keybinds::handle_standard_keybinds,
};
use singularity_common::{sap::packets::StandardEvent, settings};
use sonamu_ui::{
    color::Color,
    display_units::DisplayContainerSize,
    ui_element::{CharGrid, UITree},
    ui_event::{KeyModifiers, KeySymbol, MouseEventKind, UIEvent},
};
use std::{fmt::Write as _, path::PathBuf, sync::mpsc};

const VERBS: [&str; 10] = [
    "add_child",
    "run",
    "$",
    "set_title",
    "test",
    "addition",
    "clear",
    "help",
    "project",
    "reload_settings",
];
const TESTS: [&str; 5] = ["image", "wl", "editor", "text_editor", "char"];
const HELP: &str = "\
add_child [applet] [args]  open an applet as a child (default: command_hub)
run <program> [args]       start a program without waiting (eg a Wayland client)
$ <program> [args]         run a program and show its output
set_title <title>          rename this hub
project                    put this hub's node (and under it) in a project; Ctrl+W on the project undoes it
reload_settings            load the settings again
test <name>                image, wl, editor, text_editor or char
addition <numbers>         add up numbers
clear                      clear the history
help                       this list
Tab completes, Up/Down recalls commands, PageUp/PageDown scrolls, Esc clears.
Super+? lists every keybind.";

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
    /// A `$` command still running on a worker thread
    Running(u64),
}

struct Entry {
    command: String,
    output: Output,
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

    /// `$` results from worker threads, by job
    job_sender: mpsc::Sender<(u64, Output)>,
    job_receiver: mpsc::Receiver<(u64, Output)>,
    next_job: u64,

    /// Where `run` and `$` start programs: the directory of the tree it is in (eg a project's); the runner's if `None`
    directory: Option<PathBuf>,

    size: DisplayContainerSize,
    hook: AppletContext,
}
impl CommandHubApplet {
    #[must_use]
    pub fn new(hook: AppletContext) -> Self {
        let (job_sender, job_receiver) = mpsc::channel();
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
            job_sender,
            job_receiver,
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
        Box::new(|hook: AppletContext| Box::new(Self::new(hook)))
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

    fn execute_command(&mut self, command: &str) -> Output {
        let tokens: Vec<_> = command.split_whitespace().collect();
        // runs on a worker thread, so the hub keeps working until it exits
        if let ["$", program, args @ ..] = tokens.as_slice() {
            let mut process = self.program_command(program, args);
            let job = self.next_job;
            self.next_job += 1;
            let sender = self.job_sender.clone();
            let waker = self.hook.waker();
            std::thread::spawn(move || {
                let _ = sender.send((job, run_captured(&mut process)));
                waker.wake();
            });
            return Output::Running(job);
        }
        match self.execute(command, &tokens) {
            Ok(text) => Output::Text(text),
            Err(err) => Output::Error(err),
        }
    }

    fn execute(&mut self, command: &str, tokens: &[&str]) -> Result<String, String> {
        let (verb, args) = tokens
            .split_first()
            .ok_or_else(|| "Couldn't split commands.".to_string())?;

        match *verb {
            "addition" => Ok(args
                .iter()
                .filter_map(|a| a.parse::<f32>().ok())
                .sum::<f32>()
                .to_string()),
            "add_child" => match args.first() {
                None => {
                    self.hook.add_child(Self::get_boxed_initiator());
                    Ok("Unspecified child defaulting to command_hub.".to_string())
                }
                Some(app_name) => match self.hook.find_applet_spawner(app_name) {
                    Some(applet_spawner) => match applet_spawner.create_initializer(&args[1..]) {
                        Some(applet_initializer) => {
                            self.hook.add_child(applet_initializer);
                            Ok("Great success!".to_string())
                        }
                        None => Err(format!(
                            "Couldn't create initializer from args {:?}.",
                            &args[1..]
                        )),
                    },
                    None => Err(format!(
                        "Couldn't find app {app_name}. Known applets: {}.",
                        self.applet_names().join(", ")
                    )),
                },
            },
            "set_title" => {
                self.title = command
                    .trim()
                    .split_once(' ')
                    .ok_or_else(|| format!("Couldn't get title from \"{command}\"."))?
                    .1
                    .trim()
                    .to_string();
                self.hook.damage_treeview();
                Ok("Done!".to_string())
            }

            "test" => {
                let test = args
                    .first()
                    .ok_or_else(|| format!("Must specify a test: {}.", TESTS.join(", ")))?;
                let tokens = test_command(test).ok_or_else(|| {
                    format!("`{test}` is an unknown test. Tests: {}.", TESTS.join(", "))
                })?;
                self.execute(command, tokens)
            }

            "help" => Ok(format!(
                "{HELP}\nApplets: {}.",
                self.applet_names().join(", ")
            )),

            "project" => {
                if let Some(arg) = args.first() {
                    return Err(format!(
                        "Unknown argument `{arg}`; Ctrl+W on the project's node undoes it."
                    ));
                }
                self.hook
                    .request(NodeRequest::Wrap(ProjectApplet::wrapper()));
                Ok("This node and everything under it are a project now.".to_string())
            }

            // the program may take a while
            "reload_settings" => {
                std::thread::spawn(settings::reload);
                Ok("Reloading.".to_string())
            }

            "clear" => {
                self.history.clear();
                Ok(String::new())
            }

            // Starts a program without waiting for it (eg a wl client), connected to Sonamu
            "run" => {
                let program = args.first().ok_or("Must specify a program.")?;
                self.program_command(program, &args[1..])
                    .spawn()
                    .map_err(|err| err.to_string())?;
                Ok(format!("Started {program}"))
            }

            "$" => Err("Must specify a program.".to_string()),
            verb => Err(format!(
                "`{verb}` is an unknown command. Commands: {}. Try `help`.",
                VERBS.join(", ")
            )),
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

        let output = self.execute_command(&command);
        // `clear` shows nothing
        if command.trim() == "clear" {
            return;
        }
        self.history.push(Entry {
            command,
            output: trimmed(output),
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

    /// Completes the last word of the prompt (verbs, then applet names or tests), when the cursor is at the end
    fn complete(&mut self) {
        if self.cursor != self.current_prompt.chars().count() {
            return;
        }
        let (before, word) = match self.current_prompt.rsplit_once(' ') {
            Some((before, word)) => (Some(before), word),
            None => (None, self.current_prompt.as_str()),
        };
        let candidates: Vec<String> = match before
            .map(str::split_whitespace)
            .map(Iterator::collect::<Vec<_>>)
        {
            None => VERBS.iter().map(ToString::to_string).collect(),
            Some(words) => match words.as_slice() {
                ["add_child"] => self.applet_names(),
                ["test"] => TESTS.iter().map(ToString::to_string).collect(),
                ["help"] => VERBS.iter().map(ToString::to_string).collect(),
                _ => Vec::new(),
            },
        };
        let matches: Vec<&String> = candidates
            .iter()
            .filter(|candidate| candidate.starts_with(word))
            .collect();

        let completed = match matches.as_slice() {
            [] => return,
            [only] => format!("{only} "),
            [first, rest @ ..] => {
                self.hint = Some(
                    matches
                        .iter()
                        .map(|candidate| candidate.as_str())
                        .collect::<Vec<_>>()
                        .join("  "),
                );
                let common = rest.iter().fold(first.as_str(), |common, candidate| {
                    let len = common
                        .chars()
                        .zip(candidate.chars())
                        .take_while(|(a, b)| a == b)
                        .map(|(a, _)| a.len_utf8())
                        .sum();
                    &common[..len]
                });
                common.to_string()
            }
        };
        let start = self.current_prompt.len() - word.len();
        self.current_prompt.replace_range(start.., &completed);
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
            KeySymbol::Char('\t') if modifiers == KeyModifiers::NONE => self.complete(),
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
            | StandardEvent::SettingsChanged(_)
            | StandardEvent::Pluck(_)
            | StandardEvent::Place(..)
            | StandardEvent::NodeAction(..)
            | StandardEvent::Focus(_)
            | StandardEvent::ToggleCollapsed(_) => {}
            // doesn't show wl surfaces (only the root gets these)
            StandardEvent::WlSurfaceRegistered(_)
            | StandardEvent::Capturing(_)
            | StandardEvent::Open(_) => {}
        }
    }

    /// A `$` command finished
    fn wake(&mut self) {
        while let Ok((job, output)) = self.job_receiver.try_recv() {
            // gone if cleared meanwhile
            if let Some(entry) = self
                .history
                .iter_mut()
                .find(|entry| matches!(entry.output, Output::Running(running) if running == job))
            {
                entry.output = trimmed(output);
            }
        }
        self.hook.damage_window();
    }

    fn get_treeview(&self) -> singularity_common::utils::tree::world_tree::WorldTree<String> {
        singularity_common::utils::tree::world_tree::WorldTree::Base(self.title.clone())
    }

    fn save(&self) -> Option<singularity_common::sap::session::SavedApplet> {
        Some(singularity_common::sap::session::SavedApplet::Spawner {
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
            "help | Tab | Up/Down ".to_string()
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

/// What `test <test>` runs
fn test_command(test: &str) -> Option<&'static [&'static str]> {
    Some(match test {
        "image" => &["add_child", "image_viewer", "examples/sonamu.jpg"],
        // "wl" => &["add_child", "wl_app"],
        "wl" => &["run", "alacritty"],
        "editor" => &[
            "add_child",
            "code_editor",
            "examples/root-project/file_to_edit.txt",
        ],
        "text_editor" => &[
            "add_child",
            "text_editor",
            "examples/root-project/file_to_edit.txt",
        ],
        "char" => &["add_child", "char_render_test"],
        _ => return None,
    })
}

/// A process for `program`, connected to Sonamu if it's a wl client
pub fn program_command(program: &str, args: &[&str]) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    command.args(args);
    if let Some(socket) = std::env::var_os("SONAMU_WAYLAND_DISPLAY") {
        command.env("WAYLAND_DISPLAY", socket);
    }
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
