use crate::{
    color::Color,
    display_units::{DisplayArea, DisplayAreaPx, DisplayContainerSize, ToPx},
};
pub use smithay::reexports::winit::cursor::CursorIcon;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use wgpu::TextureView;

mod char_grid;
pub use char_grid::*;
mod style;
pub use style::*;

#[derive(Debug, Clone, Copy)]
pub struct RoundRect {
    pub corner_radius: f32,
    /// Eats into content
    pub border_width: f32,
    /// I think this should be rgba?
    /// REVIEW: confirm or deny above
    pub main_color: [f32; 4],
    pub border_color: [f32; 4],
}

/// Pixels for `UIElement::Image`: sRGB RGBA8, row-major, not premultiplied.
/// Cheap to clone; the renderer uploads each image once, while it is drawn every frame.
#[derive(Debug, Clone)]
pub struct Image(Arc<ImageData>);
#[derive(Debug)]
struct ImageData {
    /// Unique per `Image::new`, so equal images compare without their pixels
    id: u64,
    width: u32,
    height: u32,
    rgba: Box<[u8]>,
}
impl Image {
    /// `None` if `rgba` isn't `width * height * 4` bytes
    #[must_use]
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Self> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        (rgba.len() == width as usize * height as usize * 4).then(|| {
            Self(Arc::new(ImageData {
                id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
                width,
                height,
                rgba: rgba.into(),
            }))
        })
    }

    #[must_use]
    pub fn id(&self) -> u64 {
        self.0.id
    }

    #[must_use]
    pub fn size(&self) -> [u32; 2] {
        [self.0.width, self.0.height]
    }

    #[must_use]
    pub fn rgba(&self) -> &[u8] {
        &self.0.rgba
    }
}
impl PartialEq for Image {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

/// A rounded rect that a primitive is cut to, eg the inside of a rounded border
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clip {
    pub area: DisplayAreaPx,
    pub radius: f32,
}

/// Areas of nodes that set a cursor, parents before children
#[derive(Debug, Clone, Default)]
pub struct CursorAreas(pub Vec<(Cursor, DisplayAreaPx)>);
impl CursorAreas {
    /// The cursor of the topmost node under `position` that sets one
    pub fn cursor_at(&self, [x, y]: [f64; 2]) -> Option<&Cursor> {
        self.0
            .iter()
            .rev()
            .find(|(_, DisplayAreaPx([[left, top], [right, bot]]))| {
                (f64::from(*left)..f64::from(*right)).contains(&x)
                    && (f64::from(*top)..f64::from(*bot)).contains(&y)
            })
            .map(|(cursor, _)| cursor)
    }
}

/// Where laid out positions are drawn: `position * scale + offset`
#[derive(Debug, Clone, Copy)]
struct Transform {
    scale: f32,
    offset: [f32; 2],
}
impl Transform {
    const IDENTITY: Self = Self {
        scale: 1.0,
        offset: [0.0, 0.0],
    };

    fn apply(self, [x, y]: [f32; 2]) -> [f32; 2] {
        [
            x * self.scale + self.offset[0],
            y * self.scale + self.offset[1],
        ]
    }
}

/// What each taffy node holds while laying out
type NodeContext = (UIElement, Option<Cursor>);

/// On their own, a UI Primitive fills up the whole space
#[derive(Debug, Clone)]
pub enum UIPrimitiveElement {
    RoundRect(RoundRect),
    /// And how much it is scaled (`UIElement::Scaled`)
    Text(Vec<(String, glyphon::AttrsOwned)>, f32),
    CharGrid(CharGrid),
    /// Just general holder
    Texture(TextureView),
    Image(Image),
}

#[derive(Debug, Clone)]
pub struct PrimitiveScene {
    pub elements: Vec<(UIPrimitiveElement, DisplayAreaPx, Option<Clip>)>,
    pub cursors: CursorAreas,
    /// Where each subsurface was laid out, so whoever provides them knows
    pub subsurface_areas: Vec<(u64, DisplayAreaPx)>,
}
impl PrimitiveScene {
    pub fn new_empty() -> Self {
        Self {
            elements: Vec::new(),
            cursors: CursorAreas::default(),
            subsurface_areas: Vec::new(),
        }
    }

    pub fn add_primitive(
        &mut self,
        primitive_element: UIPrimitiveElement,
        container_area: DisplayArea,
        screen_size: DisplayContainerSize,
    ) {
        self.elements.push((
            primitive_element,
            container_area.map_onto_px_size(screen_size),
            None,
        ));
    }

    /* TODO: replace with taffy layout pass (old DisplayArea-based conversion kept for reference)
    fn add_ui_element(
        &mut self,
        element: UIElement,
        container_area: DisplayArea,
        screen_size: DisplayContainerSize,
        subsurfaces: &impl Fn(u64) -> UIElement,
    ) {
        match element {
            UIElement::Container(children) => {
                for child_element in children {
                    // draw the inner widget
                    self.add_ui_element(child_element, container_area, screen_size, subsurfaces);
                }
            }
            UIElement::Contained(inner_element, area) => {
                self.add_ui_element(
                    *inner_element,
                    area.map_onto(container_area),
                    screen_size,
                    subsurfaces,
                );
            }
            // FIXME: there are weird border lines
            UIElement::Bordered(inner_element, border_color) => {
                self.add_primitive(
                    UIPrimitiveElement::RoundRect(RoundRect {
                        corner_radius: 1.0,
                        border_width: 1.0,
                        main_color: Color::TRANSPARENT.to_rgba_f32_array(),
                        border_color: border_color.to_rgba_f32_array(),
                    }),
                    container_area,
                    screen_size,
                );

                let inner_area = DisplayArea(
                    DisplayCoord::new(1.into(), 1.into()),
                    DisplayCoord::new(
                        DisplayUnits::from_mixed(-1, 1.0),
                        DisplayUnits::from_mixed(-1, 1.0),
                    ),
                )
                .map_onto(container_area);

                // dbg!(&container_area);
                // dbg!(&container_area.size());
                // dbg!(&inner_area);

                // draw the inner widget
                self.add_ui_element(*inner_element, inner_area, screen_size, subsurfaces);
            }
            UIElement::Backgrounded(inner_element, bg_color) => {
                // clear the inside of the border
                // ^^^ no idea what I meant by this, just keeping it
                self.add_primitive(
                    UIPrimitiveElement::RoundRect(RoundRect {
                        corner_radius: 0.0,
                        border_width: 0.0,
                        main_color: bg_color.to_rgba_f32_array(),
                        // shouldn't matter
                        border_color: [0.; 4],
                    }),
                    container_area,
                    screen_size,
                );

                // draw the inner widget
                self.add_ui_element(*inner_element, container_area, screen_size, subsurfaces);
            }
            UIElement::Text(text) => {
                self.add_primitive(UIPrimitiveElement::Text(text), container_area, screen_size);
            }
            UIElement::CharGrid(char_grid) => {
                self.add_primitive(
                    UIPrimitiveElement::CharGrid(char_grid),
                    container_area,
                    screen_size,
                );
            }
            UIElement::Texture(texture_view) => {
                self.add_primitive(
                    UIPrimitiveElement::Texture(texture_view),
                    container_area,
                    screen_size,
                );
            }
            UIElement::Subsurface(surface_id) => {
                self.add_ui_element(
                    subsurfaces(surface_id),
                    container_area,
                    screen_size,
                    subsurfaces,
                );
            }
            UIElement::Nothing => {}
        }
    }
    */

    /// Adds `ui_tree` into `taffy` with each node's `UIElement` and cursor as its context.
    /// Subsurfaces are resolved here: the returned tree becomes the subsurface node's child.
    fn build_taffy_tree(
        taffy: &mut taffy::TaffyTree<NodeContext>,
        ui_tree: UITree,
        subsurfaces: &impl Fn(u64) -> UITree,
    ) -> taffy::NodeId {
        let UITree {
            node,
            style,
            cursor,
            mut children,
        } = ui_tree;

        if let UIElement::Subsurface(surface_id) = node {
            children.push(subsurfaces(surface_id));
        }

        let child_ids: Vec<_> = children
            .into_iter()
            .map(|child| Self::build_taffy_tree(taffy, child, subsurfaces))
            .collect();

        let node_id = taffy
            .new_with_children((&style).into(), &child_ids)
            .unwrap();
        taffy
            .set_node_context(node_id, Some((node, cursor)))
            .unwrap();
        node_id
    }

    /// Parents are added before children, so children draw on top.
    /// Primitives are cut to `clip`; a rounded border cuts its children to its inside.
    /// Laid out positions are drawn at `transform` of them (scaled by `UIElement::Scaled`).
    fn add_taffy_node(
        &mut self,
        taffy: &mut taffy::TaffyTree<NodeContext>,
        node_id: taffy::NodeId,
        parent_location: taffy::Point<f32>,
        clip: Option<Clip>,
        transform: Transform,
    ) {
        let layout = *taffy.layout(node_id).unwrap();
        let location = taffy::Point {
            x: parent_location.x + layout.location.x,
            y: parent_location.y + layout.location.y,
        };
        let [left, top] = transform.apply([location.x, location.y]);
        let area = DisplayAreaPx::from_f32(
            left,
            top,
            left + layout.size.width * transform.scale,
            top + layout.size.height * transform.scale,
        );

        let (element, cursor) = std::mem::replace(
            taffy.get_node_context_mut(node_id).unwrap(),
            (UIElement::Nothing, None),
        );
        if let Some(cursor) = cursor {
            self.cursors.0.push((cursor, area));
        }
        let element_scale = match element {
            UIElement::Scaled(scale) => Some(scale),
            _ => None,
        };
        let primitive = match element {
            UIElement::Bordered { color, radius } => {
                // REVIEW: assumes the same border width on every side
                let width = layout.border.left * transform.scale;
                Some(UIPrimitiveElement::RoundRect(RoundRect {
                    corner_radius: radius * transform.scale,
                    border_width: width,
                    main_color: Color::TRANSPARENT.to_rgba_f32_array(),
                    border_color: color.to_rgba_f32_array(),
                }))
            }
            UIElement::Backgrounded(bg_color) => Some(UIPrimitiveElement::RoundRect(RoundRect {
                corner_radius: 0.0,
                border_width: 0.0,
                main_color: bg_color.to_rgba_f32_array(),
                // shouldn't matter
                border_color: [0.; 4],
            })),
            UIElement::Text(text) => Some(UIPrimitiveElement::Text(text, transform.scale)),
            UIElement::CharGrid(char_grid) => Some(UIPrimitiveElement::CharGrid(char_grid)),
            UIElement::Texture(texture_view) => Some(UIPrimitiveElement::Texture(texture_view)),
            UIElement::Image(image) => Some(UIPrimitiveElement::Image(image)),
            // already resolved into a child in `build_taffy_tree`
            UIElement::Subsurface(surface_id) => {
                self.subsurface_areas.push((surface_id, area));
                None
            }
            UIElement::Scaled(_) | UIElement::Nothing => None,
        };
        let child_transform = match element_scale {
            // scaled around its own top-left, which stays where it is drawn
            Some(scale) => {
                let scale = transform.scale * scale;
                Transform {
                    scale,
                    offset: [left - location.x * scale, top - location.y * scale],
                }
            }
            None => transform,
        };
        let style = taffy.style(node_id).unwrap();
        // a node with an explicit size is a box: its children can't draw outside it
        let is_box = !style.size.width.is_auto() && !style.size.height.is_auto();
        let child_clip = match &primitive {
            Some(UIPrimitiveElement::RoundRect(rect)) if rect.corner_radius > 0.0 => {
                let width = rect.border_width.to_px();
                let DisplayAreaPx([[left, top], [right, bot]]) = area;
                let inside = DisplayAreaPx::new(
                    left + width,
                    top + width,
                    right.saturating_sub(width),
                    bot.saturating_sub(width),
                );
                Some(Clip {
                    area: clip.map_or(inside, |clip| clip.area.intersection(inside)),
                    radius: (rect.corner_radius - rect.border_width).max(0.0),
                })
            }
            _ if is_box => Some(match clip {
                Some(clip) => {
                    let inside = clip.area.intersection(area);
                    Clip {
                        area: inside,
                        // the rounded corners are only still the edge if nothing cut them off
                        radius: if inside == clip.area {
                            clip.radius
                        } else {
                            0.0
                        },
                    }
                }
                None => Clip { area, radius: 0.0 },
            }),
            _ => clip,
        };
        if let Some(primitive) = primitive {
            self.elements.push((primitive, area, clip));
        }

        for child_id in taffy.children(node_id).unwrap() {
            self.add_taffy_node(taffy, child_id, location, child_clip, child_transform);
        }
    }

    pub fn from_ui_tree(
        content: UITree,
        screen_size: DisplayContainerSize,
        subsurfaces: impl Fn(u64) -> UITree,
    ) -> Self {
        // TODO: keep the taffy tree between frames to use its cache
        let mut taffy = taffy::TaffyTree::new();
        let root = Self::build_taffy_tree(&mut taffy, content, &subsurfaces);

        taffy
            .compute_layout_with_measure(
                root,
                taffy::Size {
                    width: taffy::AvailableSpace::Definite(screen_size.width as f32),
                    height: taffy::AvailableSpace::Definite(screen_size.height as f32),
                },
                |inputs, _node_id, _element, style| {
                    // leaves with auto size take all the space they can
                    taffy::compute_leaf_layout(
                        inputs,
                        style,
                        |_, _| 0.0,
                        |known_dimensions, available_space| {
                            let fill = |space| match space {
                                taffy::AvailableSpace::Definite(space) => space,
                                _ => 0.0,
                            };
                            taffy::Size {
                                width: known_dimensions
                                    .width
                                    .unwrap_or_else(|| fill(available_space.width)),
                                height: known_dimensions
                                    .height
                                    .unwrap_or_else(|| fill(available_space.height)),
                            }
                        },
                    )
                },
            )
            .unwrap();

        let mut scene = Self::new_empty();
        scene.add_taffy_node(
            &mut taffy,
            root,
            taffy::Point { x: 0.0, y: 0.0 },
            None,
            Transform::IDENTITY,
        );
        scene
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UIElement {
    /// With `radius > 0`, children are cut to the rounded inside
    Bordered {
        color: Color,
        radius: f32,
    },
    /// TODO: better name
    Backgrounded(Color),

    Text(Vec<(String, glyphon::AttrsOwned)>),

    /// should display like a terminal
    ///
    /// most important feature is that each character is the same size
    CharGrid(CharGrid),

    /// TODO: replace image with this
    Texture(TextureView),

    /// Pixels from an applet, which has no GPU device to make a `Texture`; stretched over the node
    Image(Image),

    /// Reference to an embedded UI element
    Subsurface(u64),

    /// Draws its children this many times as big, from its top-left (eg a preview of an applet at 0.2).
    /// They are laid out as usual, so give them their full size, and it the scaled size.
    Scaled(f32),

    Nothing,
}

/// The pointer's look while it is over a node
#[derive(Debug, Clone, PartialEq)]
pub enum Cursor {
    Named(CursorIcon),
    Hidden,
    Custom {
        /// RGBA8, row-major
        rgba: Arc<[u8]>,
        width: u16,
        height: u16,
        /// In image px
        hotspot: [u16; 2],
        /// Image px per screen px (eg 2.0 for a scale 2 buffer on a scale 1 screen)
        scale: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct UITree {
    node: UIElement,
    style: Style,
    /// `None` uses the parent's cursor (the default cursor at the root)
    cursor: Option<Cursor>,

    children: Vec<UITree>,
}
impl UITree {
    #[must_use]
    pub fn new(node: UIElement, children: Vec<Self>) -> Self {
        Self::new_styled(node, Style::full_size(), children)
    }

    #[must_use]
    pub const fn new_styled(node: UIElement, style: Style, children: Vec<Self>) -> Self {
        Self {
            node,
            style,
            cursor: None,
            children,
        }
    }

    #[must_use]
    pub fn with_cursor(mut self, cursor: Option<Cursor>) -> Self {
        self.cursor = cursor;
        self
    }

    #[must_use]
    pub fn leaf(node: UIElement) -> Self {
        Self::new(node, Vec::new())
    }

    #[must_use]
    pub fn leaf_styled(node: UIElement, style: Style) -> Self {
        Self::new_styled(node, style, Vec::new())
    }

    #[must_use]
    pub fn into_children(self) -> Vec<Self> {
        self.children
    }

    pub fn add_child(&mut self, child: Self) {
        self.children.push(child);
    }

    /// Wraps self in a full-size node with a 1px border
    #[must_use]
    pub fn bordered(self, border: Color) -> Self {
        Self::new_styled(
            UIElement::border(border),
            Style {
                border: Length::uniform_rect(Length::Px(1.0)),
                ..Style::full_size()
            },
            vec![self],
        )
    }

    /// Wraps self in a full-size backgrounded node
    #[must_use]
    pub fn fill_bg(self, bg: Color) -> Self {
        Self::new(UIElement::Backgrounded(bg), vec![self])
    }

    /// No node content, just children
    pub fn new_container(children: Vec<Self>) -> Self {
        Self::new(UIElement::Nothing, children)
    }

    /// No node content, just children
    pub fn new_container_styled(style: Style, children: Vec<Self>) -> Self {
        Self::new_styled(UIElement::Nothing, style, children)
    }

    /// Takes in a list of full-size elements and returns a combined ui element where they are equally spaced
    /// across the horizontal axis and take full height.
    /// TODO: take in horizontal vs vertical as input or make this into two functions
    pub fn combine_displays(subdisplays: Vec<Self>) -> Self {
        Self::new_container_styled(
            Style {
                display: Display::Grid,
                // one equal column per subdisplay, one full-height row
                grid_template_columns: vec![TrackSize::Fr(1.0); subdisplays.len()],
                grid_template_rows: vec![TrackSize::Fr(1.0)],
                ..Style::full_size()
            },
            subdisplays,
        )
    }
}
impl From<UIElement> for UITree {
    fn from(node: UIElement) -> Self {
        Self::leaf(node)
    }
}
impl From<String> for UITree {
    fn from(raw_content: String) -> Self {
        Self::leaf(raw_content.into())
    }
}

impl UIElement {
    /// A square-cornered border
    #[must_use]
    pub const fn border(color: Color) -> Self {
        Self::Bordered { color, radius: 0.0 }
    }

    /// For background, you must draw a rectangle
    #[must_use]
    pub fn glyphon_attr(fg: Color) -> glyphon::AttrsOwned {
        glyphon::AttrsOwned::new(
            &glyphon::Attrs::new()
                .family(glyphon::Family::Monospace)
                .color(fg.into()),
        )
    }

    #[must_use]
    pub const fn inner_size_of_bordered(
        container_size: DisplayContainerSize,
    ) -> DisplayContainerSize {
        DisplayContainerSize {
            // TODO: figure out all the edge cases like this
            width: container_size.width.saturating_sub(2),
            height: container_size.height.saturating_sub(2),
        }
    }

    pub fn as_tree(self) -> UITree {
        self.into()
    }

    /// Taffy equivalent of the old `Contained(DisplayArea)`:
    /// absolutely positioned, with each side's distance from the matching side of the parent.
    /// Size is determined by the insets.
    ///
    /// Ex: area `[[0.2, 0.0], [1.0, 1.0]]` is
    /// `left: Percent(0.2), right: Percent(0.0), top: Percent(0.0), bottom: Percent(0.0)`.
    #[must_use]
    pub fn contained(self, inset: taffy::Rect<LengthAuto>) -> UITree {
        UITree::leaf_styled(
            self,
            Style {
                position: taffy::Position::Absolute,
                inset,
                ..Default::default()
            },
        )
    }
}
impl From<Option<Self>> for UIElement {
    fn from(value: Option<Self>) -> Self {
        value.unwrap_or(Self::Nothing)
    }
}
impl From<String> for UIElement {
    fn from(raw_content: String) -> Self {
        Self::Text(vec![(raw_content, Self::glyphon_attr(Color::WHITE))])
    }
}
