#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
pub enum DisplayUnits {
    Pixels(i32),
    /// 0 to 1
    /// REVIEW: there would be some benefits to making this an uint and dividing my the max int each time
    Proportional(f32),
    MixedUnits {
        pixels: i32,
        proportion: f32,
    },
}

impl DisplayUnits {
    pub const ZERO: Self = Self::Pixels(0);
    pub const HALF: Self = Self::Proportional(0.5);
    pub const FULL: Self = Self::Proportional(1.0);

    #[must_use]
    pub const fn from_mixed(pixels: i32, proportion: f32) -> Self {
        match (pixels, proportion) {
            // (pixels, 0.) => Self::Pixels(pixels), // can not use non-const ops in const fn
            (0, proportion) => Self::Proportional(proportion),
            (pixels, proportion) => Self::MixedUnits { pixels, proportion },
        }
    }

    #[must_use]
    pub fn pixels(&self, container_pixels: i32) -> i32 {
        match self {
            Self::Pixels(pixels) => *pixels,
            Self::Proportional(proportion) => (container_pixels as f32 * proportion) as i32,
            Self::MixedUnits { pixels, proportion } => {
                // in essense, you can think of the proportional being applied first
                Self::Pixels(*pixels).pixels(container_pixels)
                    + Self::Proportional(*proportion).pixels(container_pixels)
            }
        }
    }

    #[must_use]
    pub const fn components(&self) -> (i32, f32) {
        match self {
            Self::Pixels(pixels) => (*pixels, 0.),
            Self::Proportional(proportion) => (0, *proportion),
            Self::MixedUnits { pixels, proportion } => (*pixels, *proportion),
        }
    }

    /// REVIEW
    #[must_use]
    pub fn map_onto(&self, container_min: Self, container_max: Self) -> Self {
        if let Self::Pixels(_) = self {
            return container_min + *self;
        }

        let (container_len_px, container_len_pr) = (container_max - container_min).components();
        let (unmapped_px, unmapped_pr) = self.components();

        // Math:
        // if we knew the `tot_px` and `container_len.pixels(tot_px)`, then we can the difference from min is:
        // `unmapped_px+unmapped_pr*container_len.pixels(tot_px)`
        // which =`unmapped_px+unmapped_pr*(container_len_px+container_len_pr*tot_px)`
        // =`unmapped_px+unmapped_pr*container_len_px+unmapped_pr*container_len_pr*tot_px`
        // =`(unmapped_px+unmapped_pr*container_len_px)+(unmapped_pr*container_len_pr)*tot_px`
        // =`(unmapped_px+unmapped_pr*container_len_px)+(unmapped_pr*container_len_pr)*tot_px`
        // so: delta_px=`unmapped_px+unmapped_pr*container_len_px` and
        // delta_pr=`unmapped_pr*container_len_pr`

        let (container_min_px, container_min_pr) = container_min.components();
        let delta_px = unmapped_px + ((unmapped_pr * container_len_px as f32) as i32);
        let delta_pr = unmapped_pr * container_len_pr;
        let final_px = container_min_px + delta_px;
        let final_pr = container_min_pr + delta_pr;

        Self::from_mixed(final_px, final_pr)
    }
}
impl From<f32> for DisplayUnits {
    fn from(value: f32) -> Self {
        Self::Proportional(value)
    }
}
impl From<i32> for DisplayUnits {
    fn from(value: i32) -> Self {
        Self::Pixels(value)
    }
}
impl From<u32> for DisplayUnits {
    fn from(value: u32) -> Self {
        Self::Pixels(value as i32)
    }
}
impl std::ops::Neg for DisplayUnits {
    type Output = Self;
    fn neg(self) -> Self::Output {
        match self {
            Self::Pixels(pixels) => Self::Pixels(-pixels),
            Self::Proportional(proportion) => Self::Proportional(-proportion),
            Self::MixedUnits { pixels, proportion } => Self::from_mixed(-pixels, -proportion),
        }
    }
}
impl std::ops::Add for DisplayUnits {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        // If they are both same type, we can leave it like that but otherwise convert to mixed then add
        match (self, rhs) {
            (Self::Pixels(l), Self::Pixels(r)) => Self::Pixels(l + r),
            (Self::Proportional(l), Self::Proportional(r)) => Self::Proportional(l + r),
            (l, r) => {
                let (lpx, lpr) = l.components();
                let (rpx, rpr) = r.components();
                Self::from_mixed(lpx + rpx, lpr + rpr)
            }
        }
    }
}
impl std::ops::Sub for DisplayUnits {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        // kinda dumb that implementing Add and Neg doesn't auto impl this
        self + (-rhs)
    }
}
impl std::ops::Div<f32> for DisplayUnits {
    type Output = Self;

    /// Pixels will get lost/created from rounding integer.
    fn div(self, rhs: f32) -> Self::Output {
        match self {
            Self::Pixels(pixels) => Self::Pixels((pixels as f32 / rhs).round() as i32),
            Self::Proportional(prop) => Self::Proportional(prop / rhs),
            Self::MixedUnits { pixels, proportion } => Self::MixedUnits {
                pixels: ((pixels as f32 / rhs).round() as i32),
                proportion: proportion / rhs,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct DisplaySize {
    pub width: DisplayUnits,
    pub height: DisplayUnits,
}
impl DisplaySize {
    #[must_use]
    pub const fn new(width: DisplayUnits, height: DisplayUnits) -> Self {
        Self { width, height }
    }
}

/// Used for the container's size when asking child for UI.
/// In exclusively pixels.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct DisplayContainerSize {
    pub width: u32,
    pub height: u32,
}
impl DisplayContainerSize {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Similar to `DisplayArea::map_onto`
    #[must_use]
    pub fn find_subsize(&self, inner_size: DisplaySize) -> Self {
        Self {
            width: inner_size.width.pixels(self.width as i32).cast_unsigned(),
            height: inner_size.height.pixels(self.height as i32).cast_unsigned(),
        }
    }
}
impl From<DisplayContainerSize> for DisplaySize {
    fn from(value: DisplayContainerSize) -> Self {
        Self::new((value.width as i32).into(), (value.height as i32).into())
    }
}

/// In exclusively pixels.
/// `[[left, top], [right, bot]]`
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct DisplayAreaPx(pub [[u32; 2]; 2]);
impl DisplayAreaPx {
    #[must_use]
    pub const fn new(left: u32, top: u32, right: u32, bot: u32) -> Self {
        Self([[left, top], [right, bot]])
    }

    /// The part in both; zero-sized if they don't overlap
    #[must_use]
    pub fn intersection(self, other: Self) -> Self {
        let [[l1, t1], [r1, b1]] = self.0;
        let [[l2, t2], [r2, b2]] = other.0;
        let (left, top) = (l1.max(l2), t1.max(t2));
        Self::new(left, top, r1.min(r2).max(left), b1.min(b2).max(top))
    }

    // /// Similar to `DisplayArea::map_onto`
    // #[must_use]
    // pub fn find_subsize(&self, inner_size: DisplaySize) -> Self {
    //     Self {
    //         width: inner_size.width.pixels(self.width as i32).cast_unsigned(),
    //         height: inner_size.height.pixels(self.height as i32).cast_unsigned(),
    //     }
    // }

    pub fn size(&self) -> DisplayContainerSize {
        DisplayContainerSize {
            width: self.0[1][0] - self.0[0][0],
            height: self.0[1][1] - self.0[0][1],
        }
    }
}
impl From<DisplayAreaPx> for DisplayArea {
    fn from(value: DisplayAreaPx) -> Self {
        Self::new(
            (value.0[0][0], value.0[0][1]),
            (value.0[1][0], value.0[1][1]),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct DisplayCoord {
    pub x: DisplayUnits,
    pub y: DisplayUnits,
}
impl DisplayCoord {
    pub const ZERO: Self = Self::new(DisplayUnits::ZERO, DisplayUnits::ZERO);

    #[must_use]
    pub const fn new(x: DisplayUnits, y: DisplayUnits) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn map_onto(&self, container_area: DisplayArea) -> Self {
        Self::new(
            self.x.map_onto(container_area.0.x, container_area.1.x),
            self.y.map_onto(container_area.0.y, container_area.1.y),
        )
    }

    #[cfg(feature = "wayland_backend")]
    pub fn into_raqote_point(&self, dt: &raqote::DrawTarget) -> raqote::Point {
        raqote::Point::new(
            self.x.pixels(dt.width()) as f32,
            self.y.pixels(dt.height()) as f32,
        )
    }
}

/// technically, any opposite extremes should work,
/// but usually do (upper left, lower right)
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct DisplayArea(pub DisplayCoord, pub DisplayCoord);
impl DisplayArea {
    pub const FULL: Self = Self(
        DisplayCoord::new(DisplayUnits::ZERO, DisplayUnits::ZERO),
        DisplayCoord::new(DisplayUnits::FULL, DisplayUnits::FULL),
    );

    /// TODO: make this impl From, not be new
    pub fn new(
        // x0: impl Into<DisplayUnits>,
        // y0: impl Into<DisplayUnits>,
        // x1: impl Into<DisplayUnits>,
        // y1: impl Into<DisplayUnits>,
        (x0, y0): (impl Into<DisplayUnits>, impl Into<DisplayUnits>),
        (x1, y1): (impl Into<DisplayUnits>, impl Into<DisplayUnits>),
    ) -> Self {
        Self(
            DisplayCoord {
                x: x0.into(),
                y: y0.into(),
            },
            DisplayCoord {
                x: x1.into(),
                y: y1.into(),
            },
        )
    }

    #[must_use]
    pub const fn new_proportional(corners: [[f32; 2]; 2]) -> Self {
        Self(
            DisplayCoord {
                x: DisplayUnits::Proportional(corners[0][0]),
                y: DisplayUnits::Proportional(corners[0][1]),
            },
            DisplayCoord {
                x: DisplayUnits::Proportional(corners[1][0]),
                y: DisplayUnits::Proportional(corners[1][1]),
            },
        )
    }

    #[must_use]
    pub fn size(&self) -> DisplaySize {
        DisplaySize::new(self.1.x - self.0.x, self.1.y - self.0.y)
    }

    #[must_use]
    pub fn from_corner_size(corner: DisplayCoord, size: DisplaySize) -> Self {
        Self(
            corner,
            DisplayCoord::new(corner.x + size.width, corner.y + size.height),
        )
    }

    #[must_use]
    pub fn from_center_half_size(center: DisplayCoord, half_size: DisplaySize) -> Self {
        Self(
            DisplayCoord::new(center.x - half_size.width, center.y - half_size.height),
            DisplayCoord::new(center.x + half_size.width, center.y + half_size.height),
        )
    }

    #[must_use]
    pub fn get_center(&self) -> DisplayCoord {
        DisplayCoord {
            x: (self.0.x + self.1.x) / 2.,
            y: (self.0.y + self.1.y) / 2.,
        }
    }

    /// Use: `child_area.map_onto(parent_area)`
    #[must_use]
    pub fn map_onto(&self, container_area: Self) -> Self {
        Self(
            self.0.map_onto(container_area),
            self.1.map_onto(container_area),
        )
    }

    #[must_use]
    pub fn map_onto_px_size(&self, screen_size: DisplayContainerSize) -> DisplayAreaPx {
        DisplayAreaPx([
            [
                self.0.x.pixels(screen_size.width as i32).cast_unsigned(),
                self.0.y.pixels(screen_size.height as i32).cast_unsigned(),
            ],
            [
                self.1.x.pixels(screen_size.width as i32).cast_unsigned(),
                self.1.y.pixels(screen_size.height as i32).cast_unsigned(),
            ],
        ])
    }

    #[must_use]
    pub fn contains(&self, coord: DisplayCoord, container_pixels: [i32; 2]) -> bool {
        let coord_x = coord.x.pixels(container_pixels[0]);
        let coord_y = coord.y.pixels(container_pixels[1]);

        // REVIEW: I think doing the == instead of && should handle cases where 0 and 1 aren't min and max respectively
        let contains_x = (self.0.x.pixels(container_pixels[0]) <= coord_x)
            == (coord_x <= self.1.x.pixels(container_pixels[0]));
        let contains_y = (self.0.y.pixels(container_pixels[1]) <= coord_y)
            == (coord_y <= self.1.y.pixels(container_pixels[1]));

        contains_x && contains_y
    }
}
