//! Small helpers for applets that draw `UITree`s by hand: metrics, text, rects, frames and lists of rows.
//!
//! Everything is placed absolutely (`Style::absolute`); `Rows` draws a list and hit-tests it with the same numbers.

use sonamu_ui::{
    color::Color,
    display_units::DisplayContainerSize,
    ui_element::{Length, Style, UIElement, UITree, font_size},
};

/// Rows of the treeview, its menus and the project overview
pub fn row_height() -> f32 {
    font_size() + 14.0
}
/// Denser rows (the file manager's)
pub fn compact_row_height() -> f32 {
    font_size() + 10.0
}
/// Space around content (eg the project overview's margin, the treeview's text inset)
pub const PAD: f32 = 16.0;

/// One run of text in one color
pub fn text(text: impl Into<String>, color: Color) -> UIElement {
    UIElement::Text(vec![(text.into(), UIElement::glyphon_attr(color))])
}

/// Runs of text, each in its color
pub fn spans(spans: impl IntoIterator<Item = (String, Color)>) -> UIElement {
    UIElement::Text(
        spans
            .into_iter()
            .map(|(text, color)| (text, UIElement::glyphon_attr(color)))
            .collect(),
    )
}

/// `element` over a container of `size`, `pad` in from each side (eg a message filling an applet)
pub fn inset(element: UIElement, size: DisplayContainerSize, pad: f32) -> UITree {
    UITree::leaf_styled(
        element,
        Style::absolute(
            [pad, pad],
            [size.width, size.height].map(|length| (length as f32 - 2.0 * pad).max(0.0)),
        ),
    )
}

/// A filled rectangle
pub fn rect(color: Color, position: [f32; 2], size: [f32; 2]) -> UITree {
    UITree::leaf_styled(
        UIElement::Backgrounded(color),
        Style::absolute(position, size),
    )
}

/// `style` with a border of `width` px (and corners of `radius`) around `children`, which are laid out inside it
pub fn frame(color: Color, radius: f32, width: f32, style: Style, children: Vec<UITree>) -> UITree {
    UITree::new_styled(
        UIElement::Bordered { color, radius },
        Style {
            border: Length::uniform_rect(Length::Px(width)),
            ..style
        },
        children,
    )
}

/// A vertical list of equal rows from `top`, the first `scroll` of them scrolled away.
///
/// Indices are the list's (not the shown rows'), for both drawing (`style`, `row`) and hit-testing (`row_at`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rows {
    pub top: f32,
    pub height: f32,
    pub scroll: usize,
}
impl Rows {
    /// Unscrolled, `row_height()` rows
    pub fn new(top: f32) -> Self {
        Self::with_height(top, row_height())
    }

    pub const fn with_height(top: f32, height: f32) -> Self {
        Self {
            top,
            height,
            scroll: 0,
        }
    }

    #[must_use]
    pub const fn scrolled(self, scroll: usize) -> Self {
        Self { scroll, ..self }
    }

    /// The top of row `index`, in the list's container
    pub fn top_of(&self, index: usize) -> f32 {
        self.top + (index as f32 - self.scroll as f32) * self.height
    }

    /// Row `index`, from `left`, `width` wide
    pub fn style(&self, index: usize, left: f32, width: f32) -> Style {
        Style::absolute([left, self.top_of(index)], [width, self.height])
    }

    /// Row `index` across `width`, with `background` behind `parts` (which are placed in the row)
    pub fn row(
        &self,
        index: usize,
        width: f32,
        background: Option<Color>,
        parts: Vec<UITree>,
    ) -> UITree {
        UITree::new_styled(
            background.map_or(UIElement::Nothing, UIElement::Backgrounded),
            self.style(index, 0.0, width),
            parts,
        )
    }

    /// Above a line of text centered in a row
    pub fn text_top(&self) -> f32 {
        (self.height - font_size()) / 2.0
    }

    /// `text` (eg from `text()`) in a row, from `left` to `right`, centered, with room below for descenders
    pub fn text(&self, text: UIElement, left: f32, right: f32) -> UITree {
        let top = self.text_top();
        UITree::leaf_styled(
            text,
            Style::absolute([left, top], [(right - left).max(0.0), self.height - top]),
        )
    }

    /// The index of the row at `y` (it may be past the last row); `None` above the rows
    // non-negative, since `y >= top`
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub fn row_at(&self, y: f64) -> Option<usize> {
        let offset = (y - f64::from(self.top)) / f64::from(self.height);
        (offset >= 0.0).then(|| self.scroll + offset as usize)
    }

    /// Where in its row `y` is, from 0 (its top) to 1
    pub fn within(&self, y: f64) -> f32 {
        ((y - f64::from(self.top)) / f64::from(self.height)).fract() as f32
    }

    /// How many rows fit in a container `height` tall (at least 1)
    // non-negative, since it's at least 0
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub fn fitting(&self, height: f32) -> usize {
        (((height - self.top) / self.height).floor().max(0.0) as usize).max(1)
    }
}

/// The scroll nearest `scroll` that shows row `index` with `shown` rows
pub fn scroll_to(scroll: usize, index: usize, shown: usize) -> usize {
    scroll.clamp((index + 1).saturating_sub(shown), index)
}

/// The furthest `count` rows can scroll with `shown` rows: until the last one is at the bottom
pub const fn max_scroll(count: usize, shown: usize) -> usize {
    count.saturating_sub(shown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_at_matches_drawing() {
        let rows = Rows::with_height(8.0, 20.0).scrolled(3);
        for index in 3..10 {
            let top = f64::from(rows.top_of(index));
            assert_eq!(rows.row_at(top), Some(index));
            assert_eq!(rows.row_at(top + 19.9), Some(index));
        }
        assert_eq!(rows.row_at(7.9), None);
        assert!((rows.within(8.0 + 25.0) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn fitting_rows() {
        let rows = Rows::with_height(20.0, 20.0);
        assert_eq!(rows.fitting(100.0), 4);
        assert_eq!(rows.fitting(119.0), 4);
        assert_eq!(rows.fitting(10.0), 1);
    }

    #[test]
    fn scrolling() {
        assert_eq!(scroll_to(0, 7, 5), 3);
        assert_eq!(scroll_to(6, 2, 5), 2);
        assert_eq!(scroll_to(4, 5, 5), 4);
        assert_eq!(max_scroll(10, 4), 6);
        assert_eq!(max_scroll(3, 4), 0);
    }
}
