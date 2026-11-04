// TODO: themes

/// RGBA
/// NOTE: NOT ARGB!
#[repr(C)]
#[derive(
    Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord, bytemuck::Pod, bytemuck::Zeroable,
)]
pub struct Color(pub [u8; 4]);
impl Color {
    pub const TRANSPARENT: Self = Self([0, 0, 0, 0]);
    pub const BLACK: Self = Self([0, 0, 0, 0xFF]);
    pub const WHITE: Self = Self([0xFF, 0xFF, 0xFF, 0xFF]);
    pub const LIGHT_GRAY: Self = Self([0xDF, 0xDF, 0xDF, 0xFF]);
    pub const MEDIUM_GRAY: Self = Self([0x7F, 0x7F, 0x7F, 0xFF]);
    pub const DARK_GRAY: Self = Self([0x1F, 0x1F, 0x1F, 0xFF]);
    pub const LIGHT_YELLOW: Self = Self([0xFF, 0xFF, 0, 0xFF]);
    pub const LIGHT_GREEN: Self = Self([0, 0xFF, 0, 0xFF]);
    pub const LIGHT_BLUE: Self = Self([0, 0, 0xFF, 0xFF]);
    pub const ORANGE: Self = Self([0xFF, 0xA5, 0, 0xFF]);
    pub const CYAN: Self = Self([0, 0xFF, 0xFF, 0xFF]);
    pub const RED: Self = Self([0xFF, 0x00, 0x00, 0xFF]);

    #[must_use]
    pub const fn to_argb_u32(self) -> u32 {
        let Self([r, g, b, a]) = self;
        u32::from_be_bytes([a, r, g, b])
    }

    #[must_use]
    pub const fn to_rgba_u32(self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    pub fn to_rgba_f32_array(self) -> [f32; 4] {
        self.0.map(|c| f32::from(c) / f32::from(u8::MAX))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseColorError(String);
impl std::fmt::Display for ParseColorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid color {:?}, expected #rrggbb or #rrggbbaa",
            self.0
        )
    }
}
impl std::error::Error for ParseColorError {}

impl std::str::FromStr for Color {
    type Err = ParseColorError;

    /// `#rrggbb` or `#rrggbbaa`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseColorError(s.to_string());
        let hex = s.strip_prefix('#').ok_or_else(err)?;
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let value = u32::from_str_radix(hex, 16).map_err(|_| err())?;
        match hex.len() {
            6 => Ok(Self(((value << 8) | 0xFF).to_be_bytes())),
            8 => Ok(Self(value.to_be_bytes())),
            _ => Err(err()),
        }
    }
}
impl<'de> serde::Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
#[cfg(feature = "wayland_backend")]
impl From<Color> for raqote::Color {
    fn from(value: Color) -> Self {
        // raqote is argb, but our color is rgba
        raqote::Color::new(value.0[3], value.0[0], value.0[1], value.0[2])
    }
}
#[cfg(feature = "wayland_backend")]
impl From<Color> for raqote::SolidSource {
    fn from(value: Color) -> Self {
        raqote::SolidSource {
            r: value.0[0],
            g: value.0[1],
            b: value.0[2],
            a: value.0[3],
        }
    }
}
#[cfg(feature = "winit_backend")]
impl From<Color> for glyphon::Color {
    /// TODO: figure out most idiomatic way of dealing with these references for copy-able types.
    fn from(value: Color) -> Self {
        Self(value.to_argb_u32())
    }
}
