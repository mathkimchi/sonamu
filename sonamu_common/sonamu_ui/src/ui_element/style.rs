//! Own version of `taffy::Style` because taffy's lengths are `!Send` (they store a `*const ()`).
//! Taffy's plain enums and generic containers are `Send`, so those are reused.

pub use taffy::Display;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    Px(f32),
    /// 0 to 1
    Percent(f32),
}
impl Length {
    /// Same length on every side
    #[must_use]
    pub const fn uniform_rect(length: Self) -> taffy::Rect<Self> {
        taffy::Rect {
            left: length,
            right: length,
            top: length,
            bottom: length,
        }
    }
}
impl From<Length> for taffy::LengthPercentage {
    fn from(value: Length) -> Self {
        match value {
            Length::Px(px) => Self::length(px),
            Length::Percent(percent) => Self::percent(percent),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthAuto {
    Px(f32),
    /// 0 to 1
    Percent(f32),
    Auto,
}
impl From<LengthAuto> for taffy::LengthPercentageAuto {
    fn from(value: LengthAuto) -> Self {
        match value {
            LengthAuto::Px(px) => Self::length(px),
            LengthAuto::Percent(percent) => Self::percent(percent),
            LengthAuto::Auto => Self::auto(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    Px(f32),
    /// 0 to 1
    Percent(f32),
    Auto,
}
impl From<Dimension> for taffy::Dimension {
    fn from(value: Dimension) -> Self {
        match value {
            Dimension::Px(px) => Self::length(px),
            Dimension::Percent(percent) => Self::percent(percent),
            Dimension::Auto => Self::auto(),
        }
    }
}

/// Size of a single grid track (row or column)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackSize {
    Px(f32),
    /// 0 to 1
    Percent(f32),
    /// Fraction of the remaining space (CSS `fr`)
    Fr(f32),
    Auto,
    MinContent,
    MaxContent,
}
impl From<TrackSize> for taffy::TrackSizingFunction {
    fn from(value: TrackSize) -> Self {
        use taffy::{MaxTrackSizingFunction as Max, MinTrackSizingFunction as Min};
        let (min, max) = match value {
            TrackSize::Px(px) => (Min::length(px), Max::length(px)),
            TrackSize::Percent(percent) => (Min::percent(percent), Max::percent(percent)),
            // same as CSS: `1fr` is `minmax(auto, 1fr)`
            TrackSize::Fr(fr) => (Min::auto(), Max::fr(fr)),
            TrackSize::Auto => (Min::auto(), Max::auto()),
            TrackSize::MinContent => (Min::min_content(), Max::min_content()),
            TrackSize::MaxContent => (Min::max_content(), Max::max_content()),
        };
        Self { min, max }
    }
}

fn map_size<T, U: From<T>>(size: taffy::Size<T>) -> taffy::Size<U> {
    taffy::Size {
        width: size.width.into(),
        height: size.height.into(),
    }
}
fn map_rect<T, U: From<T>>(rect: taffy::Rect<T>) -> taffy::Rect<U> {
    taffy::Rect {
        left: rect.left.into(),
        right: rect.right.into(),
        top: rect.top.into(),
        bottom: rect.bottom.into(),
    }
}

fn map_tracks<U: From<taffy::TrackSizingFunction>>(tracks: &[TrackSize]) -> Vec<U> {
    tracks
        .iter()
        .map(|&track| taffy::TrackSizingFunction::from(track).into())
        .collect()
}

/// Subset of `taffy::Style`, look at taffy docs for what each field does.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub display: Display,
    pub position: taffy::Position,
    pub inset: taffy::Rect<LengthAuto>,
    pub size: taffy::Size<Dimension>,
    /// Children are laid out inside the border
    pub border: taffy::Rect<Length>,
    pub flex_direction: taffy::FlexDirection,
    pub flex_grow: f32,

    // grid container
    pub grid_template_rows: Vec<TrackSize>,
    pub grid_template_columns: Vec<TrackSize>,
    pub grid_auto_rows: Vec<TrackSize>,
    pub grid_auto_columns: Vec<TrackSize>,
    pub grid_auto_flow: taffy::GridAutoFlow,
    // grid child
    pub grid_row: taffy::Line<taffy::GridPlacement>,
    pub grid_column: taffy::Line<taffy::GridPlacement>,
}
impl Style {
    /// Takes up the full size of the container
    #[must_use]
    pub fn full_size() -> Self {
        Self {
            size: taffy::Size {
                width: Dimension::Percent(1.0),
                height: Dimension::Percent(1.0),
            },
            ..Default::default()
        }
    }

    /// Placed at `[left, top]` px from the container's top-left, with a px size
    #[must_use]
    pub fn absolute([left, top]: [f32; 2], [width, height]: [f32; 2]) -> Self {
        Self {
            position: taffy::Position::Absolute,
            inset: taffy::Rect {
                left: LengthAuto::Px(left),
                top: LengthAuto::Px(top),
                right: LengthAuto::Auto,
                bottom: LengthAuto::Auto,
            },
            size: taffy::Size {
                width: Dimension::Px(width),
                height: Dimension::Px(height),
            },
            ..Default::default()
        }
    }
}
/// Same as taffy's defaults
impl Default for Style {
    fn default() -> Self {
        Self {
            display: Display::default(),
            position: taffy::Position::default(),
            inset: taffy::Rect {
                left: LengthAuto::Auto,
                right: LengthAuto::Auto,
                top: LengthAuto::Auto,
                bottom: LengthAuto::Auto,
            },
            size: taffy::Size {
                width: Dimension::Auto,
                height: Dimension::Auto,
            },
            border: Length::uniform_rect(Length::Px(0.0)),
            flex_direction: taffy::FlexDirection::default(),
            flex_grow: 0.0,
            grid_template_rows: Vec::new(),
            grid_template_columns: Vec::new(),
            grid_auto_rows: Vec::new(),
            grid_auto_columns: Vec::new(),
            grid_auto_flow: taffy::GridAutoFlow::default(),
            grid_row: taffy::Line::default(),
            grid_column: taffy::Line::default(),
        }
    }
}
impl From<&Style> for taffy::Style {
    fn from(style: &Style) -> Self {
        Self {
            display: style.display,
            position: style.position,
            inset: map_rect(style.inset),
            size: map_size(style.size),
            border: map_rect(style.border),
            flex_direction: style.flex_direction,
            flex_grow: style.flex_grow,
            grid_template_rows: map_tracks(&style.grid_template_rows),
            grid_template_columns: map_tracks(&style.grid_template_columns),
            grid_auto_rows: map_tracks(&style.grid_auto_rows),
            grid_auto_columns: map_tracks(&style.grid_auto_columns),
            grid_auto_flow: style.grid_auto_flow,
            grid_row: style.grid_row.clone(),
            grid_column: style.grid_column.clone(),
            ..Self::DEFAULT
        }
    }
}
