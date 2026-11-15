//! A pastel palette from one hue, for the Colors section's "Generate pastel"

use sonamu_ui::color::Color;

/// The seven color settings for a dark theme around `hue` (degrees): a dark tinted background, near-white text,
/// pastel accent and focused border, muted greys for the rest
#[must_use]
pub fn pastel(hue: f32) -> [(&'static [&'static str], Color); 7] {
    [
        (&["palette", "bg"], hsl(hue, 0.25, 0.08)),
        (&["palette", "fg"], hsl(hue, 0.20, 0.92)),
        (&["palette", "accent"], hsl(hue, 0.60, 0.80)),
        (&["palette", "inactive"], hsl(hue, 0.08, 0.55)),
        (&["border", "focused"], hsl(hue + 30.0, 0.60, 0.78)),
        (&["border", "unfocused"], hsl(hue, 0.10, 0.28)),
        (&["border", "treeview"], hsl(hue, 0.15, 0.45)),
    ]
}

/// A hue (degrees) that differs each time
#[must_use]
pub fn random_hue() -> f32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.subsec_nanos());
    (nanos % 360) as f32
}

#[expect(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    reason = "`sector` is in 0..6 and channels are clamped to 0..=255"
)]
/// Opaque color from hue (degrees, wraps), saturation and lightness (0–1)
fn hsl(hue: f32, saturation: f32, lightness: f32) -> Color {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue.rem_euclid(360.0) / 60.0;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let lift = lightness - chroma / 2.0;
    let channel = |value: f32| ((value + lift) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color([channel(r), channel(g), channel(b), 0xFF])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings_editor::form::sections;
    use sonamu_common::settings;

    /// WCAG relative luminance
    fn luminance(Color([r, g, b, _]): Color) -> f64 {
        let channel = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }

    fn color(palette: &[(&[&str], Color); 7], path: &[&str]) -> Color {
        palette.iter().find(|(at, _)| *at == path).unwrap().1
    }

    #[test]
    fn readable_and_pastel() {
        for hue in [0.0, 120.0, 359.9] {
            let palette = pastel(hue);
            let (fg, bg) = (
                luminance(color(&palette, &["palette", "fg"])),
                luminance(color(&palette, &["palette", "bg"])),
            );
            assert!((fg + 0.05) / (bg + 0.05) >= 7.0, "{hue}");
            assert!(
                luminance(color(&palette, &["palette", "accent"]))
                    > luminance(color(&palette, &["palette", "inactive"])),
                "{hue}"
            );
        }
    }

    #[test]
    fn fills_every_color_setting() {
        let mut generated: Vec<String> = pastel(200.0)
            .iter()
            .map(|(path, _)| path.join("."))
            .collect();
        let mut fields: Vec<String> = sections(&settings::schema())[0]
            .fields
            .iter()
            .map(|field| field.path.join("."))
            .collect();
        generated.sort();
        fields.sort();
        assert_eq!(generated, fields);
    }

    #[test]
    fn random_hue_in_range() {
        assert!((0.0..360.0).contains(&random_hue()));
    }
}
