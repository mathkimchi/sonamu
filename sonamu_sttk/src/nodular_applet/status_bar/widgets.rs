//! Popup parts placed on `Lines`: buttons, and lists that scroll

use super::{Lines, POPUP_PAD, Spans, char_width};
use crate::ui;
use sonamu_common::settings::Palette;
use sonamu_ui::{color::Color, ui_element::UITree};
use std::ops::Range;

/// A popup's line and column, eg under the pointer
pub type Cell = (usize, usize);

/// A clickable label: `columns` (from..to) of line `line`. The same one draws and hit-tests, so they agree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Button {
    pub line: usize,
    pub columns: [usize; 2],
}
impl Button {
    /// Across the popup's `width` columns
    pub const fn row(line: usize, width: usize) -> Self {
        Self {
            line,
            columns: [0, width],
        }
    }

    pub fn contains(&self, (line, column): Cell) -> bool {
        line == self.line && (self.columns[0]..self.columns[1]).contains(&column)
    }

    /// `spans` from its start, lit while `on` (eg the current choice), lighter while `hover` is on it
    pub fn draw(
        &self,
        lines: &Lines,
        palette: &Palette,
        spans: Spans,
        on: bool,
        hover: Option<Cell>,
    ) -> Vec<UITree> {
        let hovered = hover.is_some_and(|cell| self.contains(cell));
        let background = match (on, hovered) {
            (true, true) => Some(palette.raised(40)),
            (true, false) => Some(palette.raised(30)),
            (false, true) => Some(palette.raised(20)),
            (false, false) => None,
        };
        let mut items: Vec<UITree> = background
            .map(|color| lines.highlight(self.line, self.columns, color))
            .into_iter()
            .collect();
        items.push(lines.text(self.line, self.columns[0], spans).with_pointer());
        items
    }
}

/// The text color of a button's label: `accent` while on
pub const fn label_color(palette: &Palette, on: bool) -> Color {
    if on { palette.accent } else { palette.fg }
}

/// A list of items showing `shown` of them from `offset`: the wheel scrolls it, a selection keeps itself in view
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScrollList {
    offset: usize,
}
impl ScrollList {
    /// Moves `steps` items (positive down) within `count`; whether it moved
    pub fn scroll(&mut self, steps: i32, count: usize, shown: usize) -> bool {
        let steps = isize::try_from(steps).unwrap_or(0);
        let offset = self
            .offset
            .saturating_add_signed(steps)
            .min(ui::max_scroll(count, shown));
        let moved = offset != self.offset;
        self.offset = offset;
        moved
    }

    /// Scrolls the least that shows item `index`
    pub fn show(&mut self, index: usize, shown: usize) {
        self.offset = ui::scroll_to(self.offset, index, shown);
    }

    /// Back to the first item
    pub fn reset(&mut self) {
        self.offset = 0;
    }

    /// The shown items' indices, of `count`
    pub fn visible(self, count: usize, shown: usize) -> Range<usize> {
        let start = self.offset.min(ui::max_scroll(count, shown));
        start..(start + shown).min(count)
    }

    /// The item on shown line `line` (lines `first..first + shown` show the list), of `count`
    pub fn item_at(self, line: usize, first: usize, count: usize, shown: usize) -> Option<usize> {
        let index = self.visible(count, shown).start + line.checked_sub(first)?;
        (line < first + shown && index < count).then_some(index)
    }

    /// A thin bar in the right pad of a popup `width` columns wide, beside lines `first..first + shown`, while not
    /// all `count` fit
    #[expect(clippy::cast_precision_loss, reason = "list lengths")]
    pub fn bar(
        self,
        lines: &Lines,
        [first, width]: [usize; 2],
        count: usize,
        shown: usize,
        color: Color,
    ) -> Option<UITree> {
        if count <= shown {
            return None;
        }
        let top = lines.rows.top_of(first);
        let height = shown as f32 * lines.rows.height;
        let start = self.visible(count, shown).start as f32 / count as f32;
        let x = POPUP_PAD + width as f32 * char_width() + POPUP_PAD / 2.0 - 1.5;
        Some(ui::rect(
            color,
            [x, top + start * height],
            [3.0, height * shown as f32 / count as f32],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolls_within_the_list() {
        let mut list = ScrollList::default();
        assert!(!list.scroll(-1, 20, 5));
        assert!(list.scroll(3, 20, 5));
        assert_eq!(list.visible(20, 5), 3..8);
        list.scroll(100, 20, 5);
        assert_eq!(list.visible(20, 5), 15..20);
        // fewer items later (eg a narrower search): the end still shows
        assert_eq!(list.visible(7, 5), 2..7);
        list.show(0, 5);
        assert_eq!(list.visible(20, 5), 0..5);
        list.show(9, 5);
        assert_eq!(list.item_at(1, 1, 20, 5), Some(5));
        assert_eq!(list.item_at(6, 1, 20, 5), None);
        assert_eq!(list.item_at(0, 1, 20, 5), None);
    }

    #[test]
    fn buttons_hit_their_columns() {
        let button = Button {
            line: 2,
            columns: [4, 8],
        };
        assert!(button.contains((2, 4)));
        assert!(!button.contains((2, 8)));
        assert!(!button.contains((1, 5)));
    }
}
