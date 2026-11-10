//! A Luau prompt with its history (`sonamu_script`), shared by the command hub and the root's buffer line

use super::{PROMPT, bar_bg, bg, fill_row, put};
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
    sync::{Arc, mpsc},
};

/// Kept; older ones are dropped
const MAX_ENTRIES: usize = 200;
/// Longer outputs are cut off
const MAX_OUTPUT_LINES: usize = 500;

pub(crate) enum Output {
    Text(String),
    Error(String),
    /// Still running on the script thread
    Running(u64),
}

pub(crate) struct Entry {
    pub command: String,
    /// Its `print`s so far
    pub printed: String,
    pub output: Output,
}

/// An entry's code running on the script thread
struct Eval {
    job: u64,
    handle: EvalHandle,
    outputs: mpsc::Receiver<sonamu_script::Output>,
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
    /// The thing that is currently being typed
    prompt: String,
    /// In chars, into `prompt`
    cursor: usize,
    /// Shown above the prompt until the next key (eg Tab's candidates)
    pub hint: Option<String>,

    /// Entered commands, for Up/Down
    recall: Vec<String>,
    /// Into `recall` while recalling, with the prompt from before
    recalling: Option<(usize, String)>,

    evals: Vec<Eval>,
    next_job: u64,
    /// Tab's candidates on the script thread, for this prompt
    completing: Option<(String, mpsc::Receiver<Vec<String>>)>,
    waker: Waker,
}
impl Repl {
    pub fn new(waker: Waker) -> Self {
        Self {
            history: Vec::new(),
            prompt: String::new(),
            cursor: 0,
            hint: None,
            recall: Vec::new(),
            recalling: None,
            evals: Vec::new(),
            next_job: 0,
            completing: None,
            waker,
        }
    }

    /// An eval is running: like a shell, no prompt until it ends
    pub fn running(&self) -> bool {
        !self.evals.is_empty()
    }

    /// Edits the prompt, runs it (Enter), recalls (Up/Down), completes (Tab), pastes (Ctrl+V), cancels (Ctrl+C).
    /// While running, only Ctrl+C. Returns whether it took the key.
    pub fn handle_key(&mut self, key: KeySymbol, modifiers: KeyModifiers) -> bool {
        self.hint = None;
        let cancel = matches!(key, KeySymbol::Char('c' | 'C')) && modifiers == KeyModifiers::CTRL;
        if self.running() && !cancel {
            return false;
        }
        match key {
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
            _ if cancel => {
                for eval in &self.evals {
                    eval.handle.cancel();
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
                self.prompt.insert(self.byte_index(self.cursor), key_char);
                self.cursor += 1;
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
        if self.recall.last() != Some(&command) {
            self.recall.push(command.clone());
            if self.recall.len() > MAX_ENTRIES {
                self.recall.remove(0);
            }
        }

        // like the old verb
        let code = if command.trim() == "help" {
            "help()"
        } else {
            &command
        };
        let job = self.next_job;
        self.next_job += 1;
        let waker = Arc::clone(&self.waker);
        let (handle, outputs) = sonamu_script::eval(code.to_string(), move || waker());
        self.evals.push(Eval {
            job,
            handle,
            outputs,
        });
        self.history.push(Entry {
            command,
            printed: String::new(),
            output: Output::Running(job),
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
                    None => self.prompt.clone(),
                };
                self.prompt = self.recall[index].clone();
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

    /// Asks the script thread what could follow the prompt, when the cursor is at its end
    fn start_completion(&mut self) {
        if self.cursor != self.prompt.chars().count() {
            return;
        }
        let waker = Arc::clone(&self.waker);
        let candidates = sonamu_script::complete(self.prompt.clone(), move || waker());
        self.completing = Some((self.prompt.clone(), candidates));
    }

    /// Completes the prompt's last word: the only candidate, or what they all start with (listed in the hint)
    fn complete(&mut self, candidates: &[String]) {
        let word_start = self
            .prompt
            .char_indices()
            .rev()
            .find(|&(_, c)| !(c.is_alphanumeric() || c == '_'))
            .map_or(0, |(index, c)| index + c.len_utf8());
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
        let text = text.trim_end_matches(['\n', '\r']).replace("\r\n", "\n");
        self.prompt.insert_str(self.byte_index(self.cursor), &text);
        self.cursor += text.chars().count();
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.prompt
            .char_indices()
            .nth(char_index)
            .map_or(self.prompt.len(), |(index, _)| index)
    }

    /// Takes what the script thread sent: evals' prints and results, Tab's candidates
    pub fn wake(&mut self) {
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
                let output = match eval.outputs.try_recv() {
                    Ok(output) => output,
                    Err(mpsc::TryRecvError::Empty) => break,
                    // eg it panicked
                    Err(mpsc::TryRecvError::Disconnected) => {
                        sonamu_script::Output::Done(Err("the script thread ended".to_string()))
                    }
                };
                let Some(entry) = entry.as_deref_mut() else {
                    break;
                };
                match output {
                    sonamu_script::Output::Print(line) => {
                        entry.printed.push_str(&line);
                        entry.printed.push('\n');
                    }
                    sonamu_script::Output::Done(result) => {
                        entry.output = trimmed(shown(result));
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

    /// The scrollback from entry `from` on, wrapped at `width`, then the hint; text in the palette's colors
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
            lines.extend(wrap(hint, width).map(|text| Line {
                text,
                fg: palette.inactive,
                bg: bg(palette),
                accent: 0,
            }));
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
