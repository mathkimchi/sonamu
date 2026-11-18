//! A prompt with its history, shared by the command hub (Luau by default) and the root's buffer line (the agent):
//! `!` Luau (`sonamu_script`), `$` shell

use super::{
    PROMPT,
    agent::{self, Streamed},
    shell::{self, Shell},
};
use crate::ui::{bg, raised_bg as bar_bg};
use crate::ui::{fill_row, put};
use sonamu_common::command::Value;
use sonamu_common::settings::ERROR;
use sonamu_common::settings::Palette;
use sonamu_script::EvalHandle;
use sonamu_ui::color::Color;
use sonamu_ui::{
    ui_element::CharGrid,
    ui_event::{KeyModifiers, KeySymbol},
};
use std::{
    fmt::Write as _,
    sync::{Arc, Mutex, PoisonError, atomic::AtomicBool, mpsc},
};

/// Kept; older ones are dropped
const MAX_ENTRIES: usize = 200;
/// Entered commands, for Up/Down; shared by every prompt (hubs and the buffer line), like nvim's cmdline history
static RECALL: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Longer outputs are cut off
const MAX_OUTPUT_LINES: usize = 500;
/// The agent's last exchanges it's given
const MAX_CHAT_ENTRIES: usize = 20;

/// What an entered line runs: by its first char, else the prompt's default
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Agent,
    /// `!`
    Luau,
    /// `$`
    Shell,
}
impl Kind {
    /// The kind of `line` and the rest of it
    fn of(line: &str, default: Self) -> (Self, &str) {
        let kind = match line.chars().next() {
            Some('!') => Self::Luau,
            Some('$') => Self::Shell,
            _ => return (default, line),
        };
        (kind, &line[1..])
    }
}

pub(crate) enum Output {
    Text(String),
    Error(String),
    /// Still running on the script thread
    Running(u64),
}

pub(crate) struct Entry {
    pub kind: Kind,
    /// As typed (with its prefix)
    pub command: String,
    /// Its `print`s so far
    pub printed: String,
    pub output: Output,
}

/// An entry running
struct Eval {
    job: u64,
    running: Running,
}
enum Running {
    /// On the script thread
    Luau {
        handle: EvalHandle,
        outputs: mpsc::Receiver<sonamu_script::Output>,
    },
    Agent {
        cancel: Arc<AtomicBool>,
        outputs: mpsc::Receiver<Streamed>,
    },
    Shell {
        shell: Shell,
        outputs: mpsc::Receiver<Streamed>,
    },
}
impl Running {
    fn cancel(&self) {
        match self {
            Self::Luau { handle, .. } => handle.cancel(),
            Self::Agent { cancel, .. } => cancel.store(true, std::sync::atomic::Ordering::Relaxed),
            Self::Shell { shell, .. } => shell.cancel(),
        }
    }

    /// Its next text to append, or its output once done
    fn try_next(&self) -> Result<Result<String, Output>, mpsc::TryRecvError> {
        Ok(match self {
            Self::Luau { outputs, .. } => match outputs.try_recv()? {
                sonamu_script::Output::Print(line) => Ok(line + "\n"),
                sonamu_script::Output::Done(result) => Err(trimmed(shown(result))),
            },
            Self::Agent { outputs, .. } | Self::Shell { outputs, .. } => {
                match outputs.try_recv()? {
                    Streamed::Text(text) => Ok(text),
                    Streamed::Done(Ok(())) => Err(Output::Text(String::new())),
                    Streamed::Done(Err(err)) => Err(Output::Error(err)),
                }
            }
        })
    }
}

/// A line of the scrollback
pub(crate) struct Line {
    pub text: String,
    pub fg: Color,
    pub bg: Color,
    /// Leading chars drawn in the palette's accent (the prompt of a command)
    pub accent: usize,
}

/// Wakes the owner's thread, which then calls `Repl::wake`
pub(crate) type Waker = Arc<dyn Fn() + Send + Sync>;

pub(crate) struct Repl {
    pub history: Vec<Entry>,
    /// What a line without a prefix runs
    default: Kind,
    /// The thing that is currently being typed
    prompt: String,
    /// In chars, into `prompt`
    cursor: usize,
    /// Shown above the prompt until the next key (eg Tab's candidates)
    pub hint: Option<String>,
    /// Like `hint`, in red (eg that there's no agent)
    warning: Option<String>,

    /// Into `RECALL` while recalling, with the prompt from before
    recalling: Option<(usize, String)>,

    evals: Vec<Eval>,
    next_job: u64,
    /// Tab's candidates on the script thread, for this prompt
    completing: Option<(String, mpsc::Receiver<Vec<String>>)>,
    /// `agent::check`'s, for `warning`
    checking: Option<mpsc::Receiver<Option<String>>>,
    waker: Waker,
}
impl Repl {
    pub fn new(default: Kind, waker: Waker) -> Self {
        Self {
            history: Vec::new(),
            default,
            prompt: String::new(),
            cursor: 0,
            hint: None,
            warning: None,
            recalling: None,
            evals: Vec::new(),
            next_job: 0,
            completing: None,
            checking: None,
            waker,
        }
    }

    /// An eval is running: like a shell, no prompt until it ends
    pub fn running(&self) -> bool {
        !self.evals.is_empty()
    }

    /// Edits the prompt, runs it (Enter), adds a newline (Shift+Enter), recalls (Up/Down), completes (Tab), pastes
    /// (Ctrl+V), copies the last output (Ctrl+Shift+C), cancels (Ctrl+C). While running too (type-ahead: shown once it ends, Enter queues).
    /// Returns whether it took the key.
    pub fn handle_key(&mut self, key: KeySymbol, modifiers: KeyModifiers) -> bool {
        self.hint = None;
        self.warning = None;
        let cancel = matches!(key, KeySymbol::Char('c' | 'C')) && modifiers == KeyModifiers::CTRL;
        match key {
            KeySymbol::Enter if modifiers.shift => self.insert("\n"),
            KeySymbol::Enter => self.enter(),
            KeySymbol::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.prompt.remove(self.byte_index(self.cursor));
                }
            }
            KeySymbol::ArrowKeyLeft => self.cursor = self.cursor.saturating_sub(1),
            KeySymbol::ArrowKeyRight => {
                self.cursor = (self.cursor + 1).min(self.prompt.chars().count());
            }
            KeySymbol::ArrowKeyUp => self.recall_command(true),
            KeySymbol::ArrowKeyDown => self.recall_command(false),
            KeySymbol::Escape => self.clear_prompt(),
            KeySymbol::Char('\t') if modifiers == KeyModifiers::NONE => self.start_completion(),
            KeySymbol::Char('v') if modifiers == KeyModifiers::CTRL => self.paste(),
            KeySymbol::Char('V') if modifiers == KeyModifiers::CTRL_SHIFT => self.paste(),
            KeySymbol::Char('C') if modifiers == KeyModifiers::CTRL_SHIFT => self.copy_output(),
            _ if cancel => {
                for eval in &self.evals {
                    eval.running.cancel();
                }
                // also whatever runs before them (eg a stuck binding)
                sonamu_script::interrupt();
            }
            // Ctrl/Alt/Super combinations aren't text
            KeySymbol::Char(key_char)
                if !(modifiers.ctrl
                    || modifiers.alt
                    || modifiers.logo
                    || key_char.is_control()) =>
            {
                self.insert(key_char.encode_utf8(&mut [0; 4]));
            }
            _ => return false,
        }
        true
    }

    pub fn prompt_is_empty(&self) -> bool {
        self.prompt.is_empty()
    }

    pub fn clear_prompt(&mut self) {
        self.prompt.clear();
        self.cursor = 0;
        self.recalling = None;
    }

    fn enter(&mut self) {
        let command = std::mem::take(&mut self.prompt);
        self.cursor = 0;
        self.recalling = None;
        if command.trim().is_empty() {
            return;
        }
        let mut recall = RECALL.lock().unwrap_or_else(PoisonError::into_inner);
        if recall.last() != Some(&command) {
            recall.push(command.clone());
            if recall.len() > MAX_ENTRIES {
                recall.remove(0);
            }
        }
        drop(recall);

        let (kind, text) = Kind::of(&command, self.default);
        let job = self.next_job;
        self.next_job += 1;
        let waker = Arc::clone(&self.waker);
        let notify = move || waker();
        let running = match kind {
            Kind::Agent => {
                let mut chat = self.chat();
                chat.push(agent::Message {
                    role: "user",
                    content: text.to_string(),
                });
                let cancel = Arc::new(AtomicBool::new(false));
                let outputs = agent::ask(chat, Arc::clone(&cancel), notify);
                Some(Running::Agent { cancel, outputs })
            }
            Kind::Shell => shell::run(text, notify)
                .inspect_err(|err| log::warn!("`{text}` failed: {err}"))
                .ok()
                .map(|(shell, outputs)| Running::Shell { shell, outputs }),
            Kind::Luau => {
                // like the old verb
                let code = if text.trim() == "help" {
                    "help()".to_string()
                } else {
                    text.to_string()
                };
                let (handle, outputs) = sonamu_script::eval(code, notify);
                Some(Running::Luau { handle, outputs })
            }
        };
        let output = match running {
            Some(running) => {
                self.evals.push(Eval { job, running });
                Output::Running(job)
            }
            None => Output::Error("couldn't start `sh`".to_string()),
        };
        self.history.push(Entry {
            kind,
            command,
            printed: String::new(),
            output,
        });
        if self.history.len() > MAX_ENTRIES {
            self.history.remove(0);
        }
    }

    /// The agent's finished exchanges so far (its replies are their `printed`)
    fn chat(&self) -> Vec<agent::Message> {
        let mut exchanges: Vec<_> = self
            .history
            .iter()
            .filter(|entry| entry.kind == Kind::Agent && matches!(entry.output, Output::Text(_)))
            .collect();
        exchanges.drain(..exchanges.len().saturating_sub(MAX_CHAT_ENTRIES));
        exchanges
            .into_iter()
            .flat_map(|entry| {
                [
                    agent::Message {
                        role: "user",
                        content: entry.command.clone(),
                    },
                    agent::Message {
                        role: "assistant",
                        content: entry.printed.clone(),
                    },
                ]
            })
            .collect()
    }

    /// Up (`back`) or Down through entered commands; Down past the newest gives back the prompt from before
    fn recall_command(&mut self, back: bool) {
        let recall = RECALL.lock().unwrap_or_else(PoisonError::into_inner);
        let index = match (&self.recalling, back) {
            (None, false) => return,
            (None, true) => recall.len().checked_sub(1),
            (Some((index, _)), true) => Some(index.saturating_sub(1)),
            (Some((index, _)), false) => Some(index + 1).filter(|&next| next < recall.len()),
        };
        match index {
            Some(index) => {
                // another prompt may have dropped old ones meanwhile
                let Some(command) = recall.get(index) else {
                    return;
                };
                let draft = match self.recalling.take() {
                    Some((_, draft)) => draft,
                    None => self.prompt.clone(),
                };
                self.prompt.clone_from(command);
                self.recalling = Some((index, draft));
            }
            None => {
                if let Some((_, draft)) = self.recalling.take() {
                    self.prompt = draft;
                }
            }
        }
        self.cursor = self.prompt.chars().count();
    }

    /// Checks the agent, if it's the default: `warning` when there's none
    pub fn check_agent(&mut self) {
        if self.default == Kind::Agent {
            let waker = Arc::clone(&self.waker);
            self.checking = Some(agent::check(move || waker()));
        }
    }

    /// Asks what could follow the prompt, when the cursor is at its end: the script thread for Luau, `shell` for
    /// shell
    fn start_completion(&mut self) {
        if self.cursor != self.prompt.chars().count() {
            return;
        }
        let waker = Arc::clone(&self.waker);
        let notify = move || waker();
        let candidates = match Kind::of(&self.prompt, self.default) {
            (Kind::Luau, code) => sonamu_script::complete(code.to_string(), notify),
            (Kind::Shell, line) => shell::complete(line.to_string(), notify),
            (Kind::Agent, _) => return,
        };
        self.completing = Some((self.prompt.clone(), candidates));
    }

    /// Completes the prompt's last word (Luau: an identifier; shell: up to whitespace): the only candidate, or what
    /// they all start with (listed in the hint)
    fn complete(&mut self, candidates: &[String]) {
        let shell = Kind::of(&self.prompt, self.default).0 == Kind::Shell;
        let word_start = self
            .prompt
            .char_indices()
            .rev()
            .find(|&(_, c)| {
                if shell {
                    c.is_whitespace()
                } else {
                    !(c.is_alphanumeric() || c == '_')
                }
            })
            // a shell line's first word starts after its `$`
            .map_or(usize::from(shell), |(index, c)| index + c.len_utf8());
        let completed = match candidates {
            [] => return,
            [only] => only.clone(),
            [first, rest @ ..] => {
                self.hint = Some(candidates.join("  "));
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
        self.prompt.replace_range(word_start.., &completed);
        self.cursor = self.prompt.chars().count();
    }

    /// Inserts the clipboard's text at the cursor (newlines kept: it's Luau)
    fn paste(&mut self) {
        let Some(text) = sonamu_common::clipboard::paste() else {
            return;
        };
        self.insert(&text.trim_end_matches(['\n', '\r']).replace("\r\n", "\n"));
    }

    /// Copies the last entry's prints and result (like a terminal's Ctrl+Shift+C)
    fn copy_output(&self) {
        let Some(entry) = self.history.last() else {
            return;
        };
        let result = match &entry.output {
            Output::Text(text) | Output::Error(text) => text.as_str(),
            Output::Running(_) => "",
        };
        sonamu_common::clipboard::copy(format!("{}{result}", entry.printed));
    }

    fn insert(&mut self, text: &str) {
        self.prompt.insert_str(self.byte_index(self.cursor), text);
        self.cursor += text.chars().count();
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.prompt
            .char_indices()
            .nth(char_index)
            .map_or(self.prompt.len(), |(index, _)| index)
    }

    /// Takes what threads sent: evals' prints and results, Tab's candidates, the agent's check
    pub fn wake(&mut self) {
        if let Some(checking) = &self.checking
            && let Ok(warning) = checking.try_recv()
        {
            self.warning = warning;
            self.checking = None;
        }
        if let Some((prompt, candidates)) = &self.completing
            && let Ok(candidates) = candidates.try_recv()
        {
            // unless it was typed on meanwhile
            if *prompt == self.prompt {
                self.complete(&candidates);
            }
            self.completing = None;
        }
        let mut finished = Vec::new();
        for eval in &self.evals {
            // gone if cleared meanwhile
            let mut entry = self.history.iter_mut().find(
                |entry| matches!(entry.output, Output::Running(running) if running == eval.job),
            );
            loop {
                let next = match eval.running.try_next() {
                    Ok(next) => next,
                    Err(mpsc::TryRecvError::Empty) => break,
                    // eg it panicked
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Err(Output::Error("its thread ended".to_string()))
                    }
                };
                let Some(entry) = entry.as_deref_mut() else {
                    break;
                };
                match next {
                    Ok(text) => entry.printed.push_str(&text),
                    Err(output) => {
                        entry.output = output;
                        break;
                    }
                }
            }
            if !matches!(entry.map(|entry| &entry.output), Some(Output::Running(_))) {
                finished.push(eval.job);
            }
        }
        self.evals.retain(|eval| !finished.contains(&eval.job));
    }

    /// The scrollback from entry `from` on, wrapped at `width`, then the hint and warning; text in the palette's colors
    pub fn lines(&self, width: usize, from: usize, palette: &Palette) -> Vec<Line> {
        let mut lines = Vec::new();
        for entry in self.history.iter().skip(from) {
            for (index, text) in wrap(&format!("{PROMPT}{}", entry.command), width).enumerate() {
                lines.push(Line {
                    text,
                    fg: palette.fg,
                    bg: bar_bg(palette),
                    accent: if index == 0 { PROMPT.len() } else { 0 },
                });
            }
            lines.extend(wrap(&entry.printed, width).map(|text| Line {
                text,
                fg: palette.fg,
                bg: bg(palette),
                accent: 0,
            }));
            let (text, fg) = match &entry.output {
                Output::Text(text) => (text.as_str(), palette.fg),
                Output::Error(err) => (err.as_str(), ERROR),
                Output::Running(_) => ("", palette.fg),
            };
            if !text.is_empty() {
                lines.extend(wrap(text, width).map(|text| Line {
                    text,
                    fg,
                    bg: bg(palette),
                    accent: 0,
                }));
            }
        }
        if let Some(hint) = &self.hint {
            lines.extend(text_lines(hint, palette.inactive, width, palette));
        }
        if let Some(warning) = &self.warning {
            lines.extend(text_lines(warning, ERROR, width, palette));
        }
        lines
    }

    /// The prompt on `row` of `grid` (bar background, `PROMPT`, the cursor), scrolled sideways to keep the cursor in
    /// view; nothing while running, like a shell
    pub fn draw_prompt(&self, grid: &mut CharGrid, row: usize, palette: &Palette) {
        if self.running() {
            return;
        }
        let width = grid.width();
        fill_row(grid, row, bar_bg(palette));
        let start = put(grid, row, 0, PROMPT, palette.accent);
        let room = width.saturating_sub(start).max(1);
        let skip = self.cursor.saturating_sub(room - 1);
        // a pasted newline shows as `↵`
        let visible: String = self
            .prompt
            .chars()
            .skip(skip)
            .map(|c| if c == '\n' { '↵' } else { c })
            .collect();
        put(grid, row, start, &visible, palette.fg);
        if let Some(cell) = (start + self.cursor - skip < width)
            .then(|| grid.get_char_mut(row, start + self.cursor - skip))
        {
            cell.set_fg(bar_bg(palette)).set_bg(palette.fg);
        }
    }
}

/// An eval's return values (nothing for none), or its error
fn shown(result: Result<Vec<Value>, String>) -> Output {
    match result {
        Ok(values) => Output::Text(
            values
                .iter()
                // a string as is (eg `sonamu.help()`), not quoted
                .map(|value| match value {
                    Value::String(text) => text.clone(),
                    value => value.to_string(),
                })
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

/// `text` wrapped at `width`, in `fg` on the palette's background (eg an echoed message)
pub(crate) fn text_lines(text: &str, fg: Color, width: usize, palette: &Palette) -> Vec<Line> {
    wrap(text, width)
        .map(|text| Line {
            text,
            fg,
            bg: bg(palette),
            accent: 0,
        })
        .collect()
}

/// Draws `lines` on the grid's rows from `top`, each with its background and prompt in `accent`
pub(crate) fn draw_lines<'a>(
    grid: &mut CharGrid,
    top: usize,
    lines: impl IntoIterator<Item = &'a Line>,
    accent_color: Color,
) {
    for (row, line) in (top..).zip(lines) {
        fill_row(grid, row, line.bg);
        // the prompt is ASCII, so this is a char boundary
        let (accent, rest) = line.text.split_at(line.accent.min(line.text.len()));
        let column = put(grid, row, 0, accent, accent_color);
        put(grid, row, column, rest, line.fg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_pick_the_kind() {
        assert!(Kind::of("hi", Kind::Agent) == (Kind::Agent, "hi"));
        assert!(Kind::of("!print(1)", Kind::Agent) == (Kind::Luau, "print(1)"));
        assert!(Kind::of("$ls", Kind::Luau) == (Kind::Shell, "ls"));
    }

    #[test]
    fn tab_completes_shell_words() {
        let mut repl = Repl::new(Kind::Agent, Arc::new(|| {}));
        for typed in ["$ls /us", "$/us"] {
            repl.clear_prompt();
            repl.insert(typed);
            repl.handle_key(KeySymbol::Char('\t'), KeyModifiers::NONE);
            while repl.completing.is_some() {
                repl.wake();
            }
            assert_eq!(repl.prompt, typed.replace("/us", "/usr/"));
        }
    }

    #[test]
    fn shell_streams_stdout_and_stderr() {
        let (_shell, outputs) = shell::run("echo out; echo err >&2; exit 3", || {}).unwrap();
        let mut text = String::new();
        let result = loop {
            match outputs.recv().unwrap() {
                Streamed::Text(piece) => text.push_str(&piece),
                Streamed::Done(result) => break result,
            }
        };
        assert_eq!(text, "out\nerr\n");
        assert!(result.unwrap_err().contains('3'));
    }

    #[test]
    fn no_agent_warns_and_errors() {
        let mut repl = Repl::new(Kind::Agent, Arc::new(|| {}));
        repl.check_agent();
        while repl.checking.is_some() {
            repl.wake();
        }
        assert!(
            repl.warning
                .as_ref()
                .unwrap()
                .contains("`agent_url` isn't set")
        );

        repl.insert("hi");
        repl.handle_key(KeySymbol::Enter, KeyModifiers::NONE);
        while repl.running() {
            repl.wake();
        }
        let Output::Error(err) = &repl.history[0].output else {
            panic!("no error");
        };
        assert!(err.contains("`$` runs shell, `!` runs Luau"), "{err}");
    }
}
