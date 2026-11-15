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

/// How far a number field's +/- and slider steps: 1 for whole numbers and wide or unbounded ranges, finer for
/// narrow ones
#[must_use]
pub fn step_size(min: Option<f64>, max: Option<f64>, integer: bool) -> f64 {
    match (min, max) {
        _ if integer => 1.0,
        (Some(min), Some(max)) if max - min <= 2.0 => 0.05,
        (Some(min), Some(max)) if max - min <= 20.0 => 0.5,
        _ => 1.0,
    }
}

/// `value` one `step` up (`up`) or down, on the step grid, within `[min, max]`
#[must_use]
pub fn stepped(value: f64, step: f64, up: bool, (min, max): (Option<f64>, Option<f64>)) -> f64 {
    let next = snap(value + if up { step } else { -step }, step);
    next.clamp(min.unwrap_or(f64::MIN), max.unwrap_or(f64::MAX))
}

/// Where `value` is along a slider from `min` to `max` (0..=1)
#[must_use]
#[expect(clippy::cast_possible_truncation, reason = "a fraction")]
pub fn slider_fraction(value: f64, min: f64, max: f64) -> f32 {
    ((value - min) / (max - min)).clamp(0.0, 1.0) as f32
}

/// The value at `fraction` along a slider from `min` to `max`, on the step grid
#[must_use]
pub fn slider_value(fraction: f64, min: f64, max: f64, step: f64) -> f64 {
    snap(min + fraction.clamp(0.0, 1.0) * (max - min), step).clamp(min, max)
}

/// `value` rounded to a multiple of `step`, without float noise (eg 0.15, not 0.15000000000000002)
fn snap(value: f64, step: f64) -> f64 {
    ((value / step).round() * step * 1e6).round() / 1e6
}

/// A slider in a row of `rows`, from `left`, `width` wide, filled to `fraction` (place it in the row)
#[must_use]
pub fn slider(rows: &Rows, left: f32, width: f32, fraction: f32, palette: &Palette) -> UITree {
    let track = 4.0;
    let knob = 14.0;
    let middle = rows.height / 2.0;
    let at = left + fraction * width;
    UITree::new_styled(
        UIElement::Nothing,
        Style::absolute([0.0, 0.0], [left + width + knob, rows.height]),
        vec![
            super::rect(guide(palette), [left, middle - track / 2.0], [width, track]),
            super::rect(
                palette.accent,
                [left, middle - track / 2.0],
                [at - left, track],
            ),
            super::frame(
                palette.accent,
                knob / 2.0,
                knob / 2.0,
                Style::absolute([at - knob / 2.0, middle - knob / 2.0], [knob, knob]),
                Vec::new(),
            ),
        ],
    )
}

/// A square of `color` with a `guide` edge, at `left` in a row of `rows` (place it in the row, like `Rows::text`)
#[must_use]
pub fn swatch(rows: &Rows, left: f32, color: Color, palette: &Palette) -> UITree {
    let side = font_size();
    let top = (rows.height - side) / 2.0;
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
    fn steps_fit_the_range() {
        assert_eq!(step_size(Some(8.0), Some(64.0), false), 1.0);
        assert_eq!(step_size(Some(0.0), Some(1.0), false), 0.05);
        assert_eq!(step_size(Some(0.1), Some(2.0), false), 0.05);
        assert_eq!(step_size(Some(0.0), Some(16.0), false), 0.5);
        assert_eq!(step_size(Some(10.0), Some(60.0), true), 1.0);
        assert_eq!(step_size(None, None, false), 1.0);
    }

    #[test]
    fn stepping_snaps_and_clamps() {
        let range = (Some(0.0), Some(1.0));
        assert_eq!(stepped(0.1, 0.05, true, range), 0.15);
        assert_eq!(stepped(0.98, 0.05, true, range), 1.0);
        assert_eq!(stepped(0.02, 0.05, false, range), 0.0);
        assert_eq!(stepped(2.3, 1.0, true, (None, None)), 3.0);
    }

    #[test]
    fn slider_maps_both_ways() {
        assert_eq!(slider_fraction(24.0, 8.0, 64.0), 16.0 / 56.0);
        assert_eq!(slider_value(0.5, 8.0, 64.0, 1.0), 36.0);
        assert_eq!(slider_value(-1.0, 8.0, 64.0, 1.0), 8.0);
        assert_eq!(slider_value(0.333, 0.0, 1.0, 0.05), 0.35);
    }

    #[test]
    fn cycle_wraps() {
        assert_eq!(cycle(3, 2, true), 0);
        assert_eq!(cycle(3, 0, false), 2);
        assert_eq!(cycle(3, 1, true), 2);
    }
}
