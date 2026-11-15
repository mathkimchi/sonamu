//! Form fields (eg the settings editor's): a one-line text input, toggles, choices and color swatches

use super::{Rows, guide};
use sonamu_common::settings::Palette;
use sonamu_ui::{
    color::Color,
    ui_element::{Style, UIElement, UITree, font_size},
    ui_event::{KeyModifiers, KeySymbol},
};

/// What a key did to a `TextInput`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    /// Not a text key, or nothing to do (eg Backspace at the start); moving the cursor is this too
    Ignored,
    Changed,
    /// Enter
    Commit,
    /// Esc
    Cancel,
}

/// One line of text being typed, with a cursor
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInput {
    pub text: String,
    /// A byte index in `text`, on a char boundary
    cursor: usize,
}
impl TextInput {
    /// The cursor at the end
    #[must_use]
    pub fn new(text: String) -> Self {
        Self {
            cursor: text.len(),
            text,
        }
    }

    pub fn handle_key(&mut self, symbol: KeySymbol, modifiers: KeyModifiers) -> Edit {
        let before = self.text[..self.cursor]
            .chars()
            .next_back()
            .map(char::len_utf8);
        let after = self.text[self.cursor..].chars().next().map(char::len_utf8);
        match symbol {
            KeySymbol::Enter | KeySymbol::Char('\n' | '\r') => return Edit::Commit,
            KeySymbol::Escape => return Edit::Cancel,
            KeySymbol::ArrowKeyLeft => self.cursor -= before.unwrap_or(0),
            KeySymbol::ArrowKeyRight => self.cursor += after.unwrap_or(0),
            KeySymbol::Backspace if let Some(width) = before => {
                self.cursor -= width;
                self.text.remove(self.cursor);
                return Edit::Changed;
            }
            KeySymbol::Char(c)
                if !c.is_control() && !modifiers.ctrl && !modifiers.alt && !modifiers.logo =>
            {
                self.text.insert(self.cursor, c);
                self.cursor += c.len_utf8();
                return Edit::Changed;
            }
            _ => {}
        }
        Edit::Ignored
    }

    /// Its text, with a cursor when `focused` (place with `Rows::text`)
    #[must_use]
    pub fn element(&self, focused: bool, palette: &Palette) -> UIElement {
        let (before, after) = self.text.split_at(self.cursor);
        let cursor = if focused { "│" } else { "" };
        super::spans([
            (before.to_owned(), palette.fg),
            (cursor.to_owned(), palette.accent),
            (after.to_owned(), palette.fg),
        ])
    }
}

/// An on / off switch (place with `Rows::text`)
#[must_use]
pub fn toggle(on: bool, palette: &Palette) -> UIElement {
    if on {
        super::text("● on", palette.accent)
    } else {
        super::text("○ off", palette.inactive)
    }
}

/// One of several values, changed with Left / Right or clicks (`cycle`) (place with `Rows::text`)
#[must_use]
pub fn choice(value: &str, palette: &Palette) -> UIElement {
    super::spans([
        ("‹ ".to_owned(), palette.inactive),
        (value.to_owned(), palette.fg),
        (" ›".to_owned(), palette.inactive),
    ])
}

/// The index after (`forward`) or before `current` of `count` options, wrapping around
#[must_use]
pub const fn cycle(count: usize, current: usize, forward: bool) -> usize {
    if count == 0 {
        0
    } else if forward {
        (current + 1) % count
    } else {
        (current + count - 1) % count
    }
}

/// A square of `color` with a `guide` edge, at `left` in row `index`
#[must_use]
pub fn swatch(rows: &Rows, index: usize, left: f32, color: Color, palette: &Palette) -> UITree {
    let side = font_size();
    let top = rows.top_of(index) + (rows.height - side) / 2.0;
    UITree::new_styled(
        UIElement::Nothing,
        Style::absolute([left, top], [side, side]),
        vec![
            super::rect(guide(palette), [0.0, 0.0], [side, side]),
            super::rect(color, [1.0, 1.0], [side - 2.0, side - 2.0]),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(input: &mut TextInput, symbol: KeySymbol) -> Edit {
        input.handle_key(symbol, KeyModifiers::NONE)
    }

    #[test]
    fn typing_inserts_at_cursor() {
        let mut input = TextInput::new("ac".to_owned());
        assert_eq!(key(&mut input, KeySymbol::ArrowKeyLeft), Edit::Ignored);
        assert_eq!(key(&mut input, KeySymbol::Char('b')), Edit::Changed);
        assert_eq!(input.text, "abc");
    }

    #[test]
    fn backspace_at_start_does_nothing() {
        let mut input = TextInput::new("é".to_owned());
        key(&mut input, KeySymbol::ArrowKeyLeft);
        assert_eq!(key(&mut input, KeySymbol::Backspace), Edit::Ignored);
        assert_eq!(key(&mut input, KeySymbol::ArrowKeyRight), Edit::Ignored);
        assert_eq!(key(&mut input, KeySymbol::Backspace), Edit::Changed);
        assert_eq!(input.text, "");
    }

    #[test]
    fn enter_commits_escape_cancels() {
        let mut input = TextInput::new(String::new());
        assert_eq!(key(&mut input, KeySymbol::Enter), Edit::Commit);
        assert_eq!(key(&mut input, KeySymbol::Escape), Edit::Cancel);
    }

    #[test]
    fn cycle_wraps() {
        assert_eq!(cycle(3, 2, true), 0);
        assert_eq!(cycle(3, 0, false), 2);
        assert_eq!(cycle(3, 1, true), 2);
    }
}
