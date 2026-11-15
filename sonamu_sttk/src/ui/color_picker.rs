//! An HSV color picker: a saturation / value square beside a hue strip

use super::{frame, guide, rect};
use sonamu_common::settings::Palette;
use sonamu_ui::{
    color::Color,
    ui_element::{Image, Style, UIElement, UITree},
    ui_event::{KeyModifiers, KeySymbol},
};
use std::sync::LazyLock;

/// The hue strip's width and its gap from the square (px)
pub const STRIP_WIDTH: f32 = 24.0;
pub const STRIP_GAP: f32 = 12.0;
/// How far an arrow key moves hue, saturation or value
pub const STEP: f32 = 0.05;
/// Pixels per side of the square's image, stretched when drawn
const SQUARE_PIXELS: u32 = 32;

/// What a key did to the picker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKey {
    Ignored,
    Changed,
    /// Enter: keep the color
    Done,
    /// Esc: back to `original`
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Square,
    Strip,
}

/// Picks a color by hue, saturation and value (each 0..=1). Placed by its top left; `height` is the square's side
#[derive(Debug, Clone)]
pub struct ColorPicker {
    pub hsv: [f32; 3],
    /// The color it opened with
    pub original: Color,
    /// The part being dragged
    dragging: Option<Part>,
    /// The square at `hsv`'s hue
    square: Image,
}
impl ColorPicker {
    #[must_use]
    pub fn new(original: Color) -> Self {
        let hsv = color_hsv(original);
        Self {
            hsv,
            original,
            dragging: None,
            square: square_image(hsv[0]),
        }
    }

    /// The picked color, with `original`'s alpha
    #[must_use]
    pub fn color(&self) -> Color {
        let Color([r, g, b, _]) = hsv_color(self.hsv);
        Color([r, g, b, self.original.0[3]])
    }

    /// A press at `at` (from the picker's top left); whether it hit the square or strip (then it drags)
    pub fn press(&mut self, at: [f32; 2], height: f32) -> bool {
        let [x, y] = at;
        let strip_left = height + STRIP_GAP;
        self.dragging = if (0.0..=height).contains(&x) && (0.0..=height).contains(&y) {
            Some(Part::Square)
        } else if (strip_left..=strip_left + STRIP_WIDTH).contains(&x)
            && (0.0..=height).contains(&y)
        {
            Some(Part::Strip)
        } else {
            None
        };
        self.drag(at, height)
    }

    /// The pointer moved to `at` while pressed; whether the color changed
    pub fn drag(&mut self, [x, y]: [f32; 2], height: f32) -> bool {
        let fraction = |offset: f32| (offset / height).clamp(0.0, 1.0);
        match self.dragging {
            Some(Part::Square) => {
                self.hsv[1] = fraction(x);
                self.hsv[2] = 1.0 - fraction(y);
            }
            Some(Part::Strip) => self.set_hue(fraction(y)),
            None => return false,
        }
        true
    }

    /// Whether a drag ended
    pub fn release(&mut self) -> bool {
        self.dragging.take().is_some()
    }

    /// Left / Right: saturation, Up / Down: value, Shift + Up / Down: hue
    pub fn handle_key(&mut self, symbol: KeySymbol, modifiers: KeyModifiers) -> PickerKey {
        let [hue, saturation, value] = self.hsv;
        let step = |at: f32, up: bool| (at + if up { STEP } else { -STEP }).clamp(0.0, 1.0);
        match symbol {
            KeySymbol::Enter => return PickerKey::Done,
            KeySymbol::Escape => return PickerKey::Cancel,
            KeySymbol::ArrowKeyUp | KeySymbol::ArrowKeyDown if modifiers.shift => {
                let down = matches!(symbol, KeySymbol::ArrowKeyDown);
                self.set_hue((hue + if down { STEP } else { -STEP }).rem_euclid(1.0));
            }
            KeySymbol::ArrowKeyLeft | KeySymbol::ArrowKeyRight => {
                self.hsv[1] = step(saturation, matches!(symbol, KeySymbol::ArrowKeyRight));
            }
            KeySymbol::ArrowKeyUp | KeySymbol::ArrowKeyDown => {
                self.hsv[2] = step(value, matches!(symbol, KeySymbol::ArrowKeyUp));
            }
            _ => return PickerKey::Ignored,
        }
        PickerKey::Changed
    }

    fn set_hue(&mut self, hue: f32) {
        if (hue - self.hsv[0]).abs() > f32::EPSILON {
            self.hsv[0] = hue;
            self.square = square_image(hue);
        }
    }

    /// The square and strip, with markers, at `[left, top]`
    #[must_use]
    pub fn draw(&self, [left, top]: [f32; 2], height: f32, palette: &Palette) -> Vec<UITree> {
        let [hue, saturation, value] = self.hsv;
        let strip_left = left + height + STRIP_GAP;
        let marker = 10.0;
        let ring = |center: [f32; 2], size: [f32; 2]| {
            frame(
                palette.fg,
                3.0,
                2.0,
                Style::absolute([center[0] - size[0] / 2.0, center[1] - size[1] / 2.0], size),
                Vec::new(),
            )
        };
        vec![
            rect(
                guide(palette),
                [left - 1.0, top - 1.0],
                [height + 2.0, height + 2.0],
            ),
            UITree::leaf_styled(
                UIElement::Image(self.square.clone()),
                Style::absolute([left, top], [height, height]),
            ),
            UITree::leaf_styled(
                UIElement::Image(STRIP.clone()),
                Style::absolute([strip_left, top], [STRIP_WIDTH, height]),
            ),
            ring(
                [left + saturation * height, top + (1.0 - value) * height],
                [marker, marker],
            ),
            ring(
                [strip_left + STRIP_WIDTH / 2.0, top + hue * height],
                [STRIP_WIDTH + 4.0, 6.0],
            ),
        ]
    }
}

/// Every hue, top (red) to bottom
static STRIP: LazyLock<Image> = LazyLock::new(|| {
    let pixels = 64;
    let rgba = (0..pixels)
        .flat_map(|y| hsv_color([y as f32 / pixels as f32, 1.0, 1.0]).0)
        .collect();
    Image::new(1, pixels, rgba).expect("1 pixel wide")
});

/// Saturation left to right, value bottom to top, at `hue`
fn square_image(hue: f32) -> Image {
    let last = (SQUARE_PIXELS - 1) as f32;
    let rgba = (0..SQUARE_PIXELS)
        .flat_map(|y| {
            (0..SQUARE_PIXELS)
                .flat_map(move |x| hsv_color([hue, x as f32 / last, 1.0 - y as f32 / last]).0)
        })
        .collect();
    Image::new(SQUARE_PIXELS, SQUARE_PIXELS, rgba).expect("square")
}

/// An opaque color from hue, saturation and value (each 0..=1; hue wraps)
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the sector is 0..6 and channels are clamped to 0..=255"
)]
pub fn hsv_color([hue, saturation, value]: [f32; 3]) -> Color {
    let sector = hue.rem_euclid(1.0) * 6.0;
    let fraction = sector.fract();
    let low = value * (1.0 - saturation);
    let falling = value * (1.0 - saturation * fraction);
    let rising = value * (1.0 - saturation * (1.0 - fraction));
    let (red, green, blue) = match sector as u32 {
        0 => (value, rising, low),
        1 => (falling, value, low),
        2 => (low, value, rising),
        3 => (low, falling, value),
        4 => (rising, low, value),
        _ => (value, low, falling),
    };
    let channel = |c: f32| (c * 255.0).round().clamp(0.0, 255.0) as u8;
    Color([channel(red), channel(green), channel(blue), 0xFF])
}

/// A color's hue, saturation and value (each 0..=1; hue 0 for greys)
#[must_use]
pub fn color_hsv(Color([red, green, blue, _]): Color) -> [f32; 3] {
    // which channel is largest, compared exactly as bytes
    let max_byte = red.max(green).max(blue);
    let delta = f32::from(max_byte - red.min(green).min(blue)) / 255.0;
    let [red_f, green_f, blue_f] = [red, green, blue].map(|channel| f32::from(channel) / 255.0);
    let hue = if delta <= 0.0 {
        0.0
    } else if max_byte == red {
        ((green_f - blue_f) / delta).rem_euclid(6.0) / 6.0
    } else if max_byte == green {
        ((blue_f - red_f) / delta + 2.0) / 6.0
    } else {
        ((red_f - green_f) / delta + 4.0) / 6.0
    };
    let max = f32::from(max_byte) / 255.0;
    let saturation = if max_byte == 0 { 0.0 } else { delta / max };
    [hue, saturation, max]
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEIGHT: f32 = 100.0;

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.02)
    }

    #[test]
    fn hsv_round_trips() {
        for color in [
            Color([0xff, 0, 0, 0xff]),
            Color([0x20, 0x80, 0xc0, 0xff]),
            Color([0x70, 0x75, 0x80, 0xff]),
        ] {
            assert_eq!(hsv_color(color_hsv(color)), color);
        }
        assert!(close(
            color_hsv(Color([0, 0xff, 0, 0xff])),
            [1.0 / 3.0, 1.0, 1.0]
        ));
    }

    #[test]
    fn square_sets_saturation_and_value() {
        let mut picker = ColorPicker::new(Color([0xff, 0, 0, 0xff]));
        assert!(picker.press([HEIGHT, 0.0], HEIGHT));
        assert!(close(picker.hsv, [0.0, 1.0, 1.0]));
        assert!(picker.drag([0.0, HEIGHT], HEIGHT));
        assert!(close(picker.hsv, [0.0, 0.0, 0.0]));
        assert!(picker.release());
        assert!(
            !picker.drag([HEIGHT, 0.0], HEIGHT),
            "not dragging after release"
        );
    }

    #[test]
    fn strip_sets_hue() {
        let mut picker = ColorPicker::new(Color([0xff, 0, 0, 0xff]));
        let strip = HEIGHT + STRIP_GAP + 1.0;
        assert!(picker.press([strip, HEIGHT / 2.0], HEIGHT));
        assert!(close(picker.hsv, [0.5, 1.0, 1.0]));
        assert!(
            !picker.press([strip + STRIP_WIDTH + 10.0, 0.0], HEIGHT),
            "past the strip"
        );
    }

    #[test]
    fn keys_step_and_finish() {
        let none = KeyModifiers::NONE;
        let shift = KeyModifiers {
            shift: true,
            ..none
        };
        let mut picker = ColorPicker::new(Color([0x80, 0x80, 0x80, 0xff]));
        let [_, s, v] = picker.hsv;
        assert_eq!(
            picker.handle_key(KeySymbol::ArrowKeyRight, none),
            PickerKey::Changed
        );
        assert!(close(picker.hsv, [0.0, s + STEP, v]));
        assert_eq!(
            picker.handle_key(KeySymbol::ArrowKeyUp, none),
            PickerKey::Changed
        );
        assert!(close(picker.hsv, [0.0, s + STEP, v + STEP]));
        assert_eq!(
            picker.handle_key(KeySymbol::ArrowKeyDown, shift),
            PickerKey::Changed
        );
        assert!(close(picker.hsv, [STEP, s + STEP, v + STEP]));
        assert_eq!(picker.handle_key(KeySymbol::Enter, none), PickerKey::Done);
        assert_eq!(
            picker.handle_key(KeySymbol::Escape, none),
            PickerKey::Cancel
        );
        assert_eq!(
            picker.handle_key(KeySymbol::Char('x'), none),
            PickerKey::Ignored
        );
    }

    #[test]
    fn keeps_alpha() {
        let mut picker = ColorPicker::new(Color([0x10, 0x20, 0x30, 0x80]));
        picker.press([HEIGHT, 0.0], HEIGHT);
        assert_eq!(picker.color().0[3], 0x80);
        assert_eq!(picker.original, Color([0x10, 0x20, 0x30, 0x80]));
    }
}
