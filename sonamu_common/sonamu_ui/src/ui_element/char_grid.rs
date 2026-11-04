use crate::ui_element::{Dimension, Style, UIElement, UITree};
use crate::{color::Color, display_units::DisplayContainerSize};
use std::sync::atomic::{AtomicU32, Ordering};

/// 0 and 1 are planned to be bold and italic
/// 2 is cursor line
#[repr(C)]
#[derive(
    Copy,
    Clone,
    bytemuck::Pod,
    bytemuck::Zeroable,
    Debug,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
)]
pub struct CharCellStyle(pub u32);
impl CharCellStyle {
    pub const UNSTYLED: Self = Self(0);
    pub const CURSOR_LINE: Self = Self(1 << 2);
}
/// For me, add makes more sense than bitwise OR
impl std::ops::Add for CharCellStyle {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        #[allow(clippy::suspicious_arithmetic_impl)]
        Self(self.0 | rhs.0)
    }
}
impl std::ops::AddAssign for CharCellStyle {
    #[allow(clippy::suspicious_op_assign_impl)]
    fn add_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// Made so this can instantly be turned to bytes
/// REVIEW: rename to just `CharCell`?
#[repr(C)]
#[derive(
    Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug, Hash, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct InternalCharCell {
    pub character: u32,
    /// RGBA
    pub fg: Color,
    /// RGBA
    pub bg: Color,
    pub style: CharCellStyle,
}
impl InternalCharCell {
    pub const BYTES: usize = 16;

    pub const fn set_char(&mut self, c: char) -> &mut Self {
        self.character = c as u32;
        self
    }
    pub const fn set_fg(&mut self, fg: Color) -> &mut Self {
        self.fg = fg;
        self
    }
    pub const fn set_bg(&mut self, bg: Color) -> &mut Self {
        self.bg = bg;
        self
    }

    pub fn add_style(&mut self, new_style: CharCellStyle) -> &mut Self {
        self.style += new_style;
        self
    }
    pub const fn set_style(&mut self, new_style: CharCellStyle) -> &mut Self {
        self.style = new_style;
        self
    }
}
impl Default for InternalCharCell {
    fn default() -> Self {
        Self {
            character: u32::from(' '),
            fg: Color::WHITE,
            bg: Color::TRANSPARENT,
            style: CharCellStyle::UNSTYLED,
        }
    }
}

/// Font height in px, as `f32` bits; set from the settings
static FONT_SIZE: AtomicU32 = AtomicU32::new(46.0_f32.to_bits());

/// Font height in px
pub fn font_size() -> f32 {
    f32::from_bits(FONT_SIZE.load(Ordering::Relaxed))
}

/// Font height in px. Applets read it when they draw, so they should redraw after.
pub fn set_font_size(px: f32) {
    FONT_SIZE.store(px.max(1.0).to_bits(), Ordering::Relaxed);
}

/// Char grid cell height in px
fn cell_height() -> u32 {
    font_size().round() as u32
}

/// Char grid cell width in px. The actual aspect ratio of DejaVu Mono is 1:1.933
fn cell_width() -> u32 {
    (font_size() / 1.933).ceil() as u32
}

/// #[deprecated]
/// Idk why I deprecated this. If anything, CharGrid should be better than Glyphon Richtext
/// bc Glyphon doesn't support background colors.
/// Assumed invariant: `len(content)==width*height`
///
/// Immutable
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct CharGrid {
    width: usize,
    height: usize,
    content: Vec<InternalCharCell>,
}
impl From<&str> for CharGrid {
    /// Makes width and height the smallest necessary to fit everything.
    fn from(raw_content: &str) -> Self {
        Self::new_monostyled(
            raw_content,
            Color::LIGHT_YELLOW,
            Color::TRANSPARENT,
            CharCellStyle::UNSTYLED,
        )
    }
}
impl CharGrid {
    #[must_use]
    pub fn new(width: usize, height: usize, content: Vec<InternalCharCell>) -> Self {
        debug_assert_eq!(width * height, content.len());

        Self {
            width,
            height,
            content,
        }
    }

    /// TODO: make a CharGridSize struct?
    #[must_use]
    pub fn new_empty(width: usize, height: usize) -> Self {
        Self::new(
            width,
            height,
            vec![InternalCharCell::default(); width * height],
        )
    }

    /// Makes width and height the smallest necessary to fit everything.
    #[must_use]
    pub fn new_monostyled(raw_content: &str, fg: Color, bg: Color, style: CharCellStyle) -> Self {
        let Some(width) = raw_content.lines().map(str::len).max() else {
            return Self {
                width: 0,
                height: 0,
                content: Vec::new(),
            };
        };
        let height = raw_content.lines().count();

        let mut s = Self {
            width,
            height,
            content: vec![
                InternalCharCell {
                    fg,
                    bg,
                    style,
                    character: u32::from(' ')
                };
                width * height
            ],
        };
        for (row, line_str) in raw_content.split('\n').enumerate() {
            for (col, c) in line_str.chars().enumerate() {
                s.set_char(c, row, col);
            }
        }

        s
    }

    /// Returns (width, height)
    #[must_use]
    pub fn largest_fittable_size(container_size: DisplayContainerSize) -> (usize, usize) {
        // log::debug!("Largest fittable size called, container size: {container_size:?}");
        (
            // Font size is height, width is twice the height
            // I was gonna do size.width / (fontsize / 2) bc it is what is happening logically, but this is actually safer
            (container_size.width / cell_width()) as usize,
            (container_size.height / cell_height()) as usize,
        )
    }

    #[must_use]
    pub fn display_size(&self) -> DisplayContainerSize {
        DisplayContainerSize {
            width: (self.width as u32) * cell_width(),
            height: (self.height as u32) * cell_height(),
        }
    }

    // pub fn get_text_as_string(&self) -> String {
    //     self.content
    //         .iter()
    //         .map(|line| {
    //             line.iter()
    //                 .map(|c| c.character.to_string())
    //                 .collect::<Vec<_>>()
    //                 .join("")
    //         })
    //         .collect::<Vec<_>>()
    //         .join("\n")
    // }

    #[must_use]
    pub const fn element(self) -> UIElement {
        UIElement::CharGrid(self)
    }

    #[must_use]
    pub fn contained_element(self) -> UITree {
        let size = self.display_size();
        UITree::leaf_styled(
            self.element(),
            Style {
                size: taffy::Size {
                    width: Dimension::Px(size.width as f32),
                    height: Dimension::Px(size.height as f32),
                },
                ..Default::default()
            },
        )
    }

    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> usize {
        self.height
    }

    /// Returns char cell at index `row * self.width + col`
    #[must_use]
    pub fn get_char(&self, row: usize, col: usize) -> InternalCharCell {
        self.content[row * self.width + col]
    }

    /// Returns char cell at index `row * self.width + col`
    pub fn get_char_mut(&mut self, row: usize, col: usize) -> &mut InternalCharCell {
        &mut self.content[row * self.width + col]
    }

    /// TODO: move these into InternalCharCell
    /// at index `row * self.width + col`
    pub fn set_char(&mut self, c: char, row: usize, col: usize) {
        self.content[row * self.width + col].character = c as u32;
    }
    /// at index `row * self.width + col`
    pub fn set_fg(&mut self, fg: Color, row: usize, col: usize) {
        self.content[row * self.width + col].fg = fg;
    }
    /// at index `row * self.width + col`
    pub fn set_bg(&mut self, bg: Color, row: usize, col: usize) {
        self.content[row * self.width + col].bg = bg;
    }

    #[must_use]
    pub fn content(&self) -> &[InternalCharCell] {
        &self.content
    }
}
