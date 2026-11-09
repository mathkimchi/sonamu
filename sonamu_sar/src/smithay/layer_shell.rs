//! wlr-layer-shell: surfaces anchored to the output's edges (eg status bars, wallpapers).
//! They are not in the node tree: they're drawn under or over the root's content,
//! and their exclusive zones shrink the area the root's content gets.

use super::{Layer as SurfaceLayer, SmithayState};
use crate::runner::AppletRunner;
use smithay::{
    backend::input::KeyState,
    desktop::{PopupKind, PopupManager, find_popup_root_surface},
    reexports::wayland_server::{
        Resource as _,
        protocol::{wl_output::WlOutput, wl_surface::WlSurface},
    },
    utils::{Logical, Physical, Point, Rectangle, Size},
    wayland::{
        compositor::{SurfaceAttributes, with_states},
        shell::{
            wlr_layer::{
                Anchor, ExclusiveZone, KeyboardInteractivity, Layer, LayerSurface,
                LayerSurfaceCachedState, LayerSurfaceData, WlrLayerShellHandler,
                WlrLayerShellState,
            },
            xdg::PopupSurface,
        },
    },
};
use sonamu_ui::{
    display_units::{DisplayContainerSize, ToPx},
    ui_element::{Style, UITree},
    ui_event::{MouseEvent, MouseEventKind},
};

/// A layer surface after layout
#[derive(Debug)]
pub(super) struct LayerPlacement {
    surface: LayerSurface,
    layer: Layer,
    /// In logical units, in the output
    rect: Rectangle<i32, Logical>,
    /// Its state when placed
    state: LayerSurfaceCachedState,
}

/// The edge that the exclusive zone is measured from, if the anchor allows one
fn exclusive_edge(state: &LayerSurfaceCachedState) -> Option<Anchor> {
    let anchor = state.anchor;
    if let Some(edge) = state.exclusive_edge
        && anchor.contains(edge)
    {
        return Some(edge);
    }
    let (horizontal, vertical) = (Anchor::LEFT | Anchor::RIGHT, Anchor::TOP | Anchor::BOTTOM);
    [
        (Anchor::TOP, horizontal),
        (Anchor::BOTTOM, horizontal),
        (Anchor::LEFT, vertical),
        (Anchor::RIGHT, vertical),
    ]
    .into_iter()
    .find(|&(edge, perpendicular)| {
        // the edge alone, or with both perpendicular edges
        anchor == edge || anchor == edge | perpendicular
    })
    .map(|(edge, _)| edge)
}

/// Where along one axis (`start`/`end` anchors) a surface goes in `bounds`, like wlroots
fn place_axis(
    bounds: (i32, i32),
    requested: i32,
    (start, end): (bool, bool),
    (margin_start, margin_end): (i32, i32),
) -> (i32, i32) {
    let (bounds_start, bounds_len) = bounds;
    let len = if requested == 0 {
        (bounds_len - margin_start - margin_end).max(1)
    } else {
        requested
    };
    let pos = match (start, end) {
        (true, true) => {
            bounds_start + margin_start + (bounds_len - margin_start - margin_end - len) / 2
        }
        (true, false) => bounds_start + margin_start,
        (false, true) => bounds_start + bounds_len - len - margin_end,
        (false, false) => bounds_start + (bounds_len - len) / 2,
    };
    (pos, len)
}

impl SmithayState {
    pub(super) fn new_layer_shell_state(
        display_handle: &super::DisplayHandle,
    ) -> WlrLayerShellState {
        WlrLayerShellState::new::<AppletRunner>(display_handle)
    }

    /// The area left for the root's content, in px of the window
    pub fn content_area(&self) -> ([f64; 2], DisplayContainerSize) {
        let usable = self.usable_area;
        let to_px = |logical: i32| (f64::from(logical) * self.scale).round();
        (
            [to_px(usable.loc.x), to_px(usable.loc.y)],
            DisplayContainerSize::new(to_px(usable.size.w).to_px(), to_px(usable.size.h).to_px()),
        )
    }

    /// Places the layer surfaces and shrinks the usable area by their exclusive zones (like wlroots),
    /// telling each surface its size. Returns whether the usable area changed.
    ///
    /// TODO: one pass per layer, so a surface isn't shrunk by a later one in its own layer
    pub fn arrange_layers(&mut self) -> bool {
        let Some(mode) = self.output.current_mode() else {
            return false;
        };
        let full = Rectangle::from_size(self.to_logical(DisplayContainerSize::new(
            mode.size.w.cast_unsigned(),
            mode.size.h.cast_unsigned(),
        )));
        let mut usable = full;
        let mut placements = Vec::new();

        for layer in [Layer::Overlay, Layer::Top, Layer::Bottom, Layer::Background] {
            for surface in &self.layer_surfaces {
                let state = surface.with_cached_state(|state| *state);
                if state.layer != layer {
                    continue;
                }
                // `DontCare` surfaces ignore other zones
                let bounds = if state.exclusive_zone == ExclusiveZone::DontCare {
                    full
                } else {
                    usable
                };
                let anchor = state.anchor;
                let margin = state.margin;
                let (x, w) = place_axis(
                    (bounds.loc.x, bounds.size.w),
                    state.size.w,
                    (
                        anchor.contains(Anchor::LEFT),
                        anchor.contains(Anchor::RIGHT),
                    ),
                    (margin.left, margin.right),
                );
                let (y, h) = place_axis(
                    (bounds.loc.y, bounds.size.h),
                    state.size.h,
                    (
                        anchor.contains(Anchor::TOP),
                        anchor.contains(Anchor::BOTTOM),
                    ),
                    (margin.top, margin.bottom),
                );

                if let ExclusiveZone::Exclusive(zone) = state.exclusive_zone
                    && let Some(edge) = exclusive_edge(&state)
                {
                    let zone = zone as i32;
                    if edge == Anchor::TOP {
                        let taken = (zone + margin.top).min(usable.size.h);
                        usable.loc.y += taken;
                        usable.size.h -= taken;
                    } else if edge == Anchor::BOTTOM {
                        usable.size.h -= (zone + margin.bottom).min(usable.size.h);
                    } else if edge == Anchor::LEFT {
                        let taken = (zone + margin.left).min(usable.size.w);
                        usable.loc.x += taken;
                        usable.size.w -= taken;
                    } else {
                        usable.size.w -= (zone + margin.right).min(usable.size.w);
                    }
                }

                placements.push(LayerPlacement {
                    surface: surface.clone(),
                    layer,
                    rect: Rectangle::new(Point::from((x, y)), Size::from((w, h))),
                    state,
                });
            }
        }

        for placement in &placements {
            placement
                .surface
                .with_pending_state(|state| state.size = Some(placement.rect.size));
            placement.surface.send_pending_configure();
        }
        self.layer_placements = placements;

        let changed = self.usable_area != usable;
        self.usable_area = usable;
        changed
    }

    /// Drawn in the window's px; the textures are absolute in the container they're put in.
    /// `below` is the background and bottom layers, otherwise top and overlay.
    pub fn layer_surface_elements(
        &self,
        below: bool,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<UITree> {
        // absolute, or the wrappers would take up places in the flow and push the root's content aside
        let size = self
            .output
            .current_mode()
            .map_or([0.0; 2], |mode| [mode.size.w as f32, mode.size.h as f32]);
        // `layer_placements` is topmost first
        self.layer_placements
            .iter()
            .rev()
            .filter(|placement| {
                matches!(placement.layer, Layer::Background | Layer::Bottom) == below
                    // under the fullscreen toplevel
                    && !(self.fullscreen.is_some() && placement.layer == Layer::Top)
            })
            .map(|placement| {
                let layers = Self::layer_tree_layers(placement);
                let cursor = self.wl_cursor(&layers);
                UITree::new_container_styled(
                    Style::absolute([0.0, 0.0], size),
                    self.layers_to_textures(layers, None, device, queue),
                )
                .with_cursor(cursor)
            })
            .collect()
    }

    /// The surfaces of a layer surface (itself, subsurfaces, then popups), placed in the output
    fn layer_tree_layers(placement: &LayerPlacement) -> Vec<SurfaceLayer> {
        let root = placement.surface.wl_surface();
        let mut layers = Self::surface_tree_layers(root, placement.rect.loc);
        for (popup, offset) in PopupManager::popups_for_surface(root) {
            let origin = placement.rect.loc + offset - popup.geometry().loc;
            layers.extend(Self::surface_tree_layers(popup.wl_surface(), origin));
        }
        layers
    }

    /// Where the layer surface is in the output, if `root` is one that has been placed
    pub(super) fn layer_rect(&self, root: &WlSurface) -> Option<Rectangle<i32, Logical>> {
        self.layer_placements
            .iter()
            .find(|placement| placement.surface.wl_surface() == root)
            .map(|placement| placement.rect)
    }

    /// The topmost layer surface surface under `position` (window px), and its origin (output logical units).
    /// Background and bottom layers are only hit outside the root's content.
    fn layer_surface_under(&self, position: [f64; 2]) -> Option<(WlSurface, Point<f64, Logical>)> {
        let point = Point::<f64, Physical>::from((position[0], position[1])).to_logical(self.scale);
        let in_content = self.usable_area.to_f64().contains(point);
        self.layer_placements.iter().find_map(|placement| {
            if in_content && matches!(placement.layer, Layer::Background | Layer::Bottom) {
                return None;
            }
            // only overlay layers are over a fullscreen toplevel
            if self.fullscreen.is_some() && placement.layer != Layer::Overlay {
                return None;
            }
            Self::layer_tree_layers(placement)
                .into_iter()
                .rev()
                .find(|layer| {
                    let local = point - layer.location.to_f64();
                    Rectangle::new(layer.location, layer.view.dst)
                        .to_f64()
                        .contains(point)
                        && with_states(&layer.surface, |states| {
                            states
                                .cached_state
                                .get::<SurfaceAttributes>()
                                .current()
                                .input_region
                                .as_ref()
                                .is_none_or(|region| region.contains(local.to_i32_floor()))
                        })
                })
                .map(|layer| (layer.surface, layer.location.to_f64()))
        })
    }

    /// Whether the layer surface (not an exclusive one) can take the keyboard when clicked
    fn takes_keyboard(&self, root: &WlSurface) -> bool {
        self.layer_placements.iter().any(|placement| {
            placement.surface.wl_surface() == root
                && placement
                    .surface
                    .with_cached_state(|state| state.keyboard_interactivity)
                    != KeyboardInteractivity::None
        })
    }

    /// The layer surface that has the keyboard: the topmost exclusive one on the top or overlay layer,
    /// else the one that was clicked and takes the keyboard
    pub(super) fn keyboard_layer(&self) -> Option<WlSurface> {
        self.layer_placements
            .iter()
            .find(|placement| {
                matches!(placement.layer, Layer::Top | Layer::Overlay)
                    && placement
                        .surface
                        .with_cached_state(|state| state.keyboard_interactivity)
                        == KeyboardInteractivity::Exclusive
            })
            .map(|placement| placement.surface.wl_surface().clone())
            .or_else(|| {
                self.layer_focus
                    .clone()
                    .filter(|surface| surface.is_alive() && self.takes_keyboard(surface))
            })
    }

    /// Whether a surface is a layer surface that has been placed
    pub(super) fn is_layer_surface(&self, surface: &WlSurface) -> bool {
        self.layer_shell_state
            .layer_surfaces()
            .any(|layer_surface| layer_surface.wl_surface() == surface)
    }
}

impl AppletRunner {
    /// Called when a layer surface commits: the first commit (or the first after unmapping) makes it placed,
    /// unmapping unplaces it, and its state may have changed
    pub(super) fn layer_surface_committed(&mut self, surface: &WlSurface) {
        let state = &mut self.smithay_state;
        if !state.is_layer_surface(surface) {
            return;
        }
        if let Some(index) = state
            .layer_surfaces
            .iter()
            .position(|layer_surface| layer_surface.wl_surface() == surface)
        {
            // a null buffer unmaps it, so it isn't placed (or configured) until its next commit.
            // Smithay also resets its state (size 0, no anchors), but wlroots keeps it and clients
            // like wshowkeys rely on that, committing again without setting it.
            let unmapped = with_states(surface, |states| {
                let data = states.data_map.get::<LayerSurfaceData>().unwrap();
                if data.lock().unwrap().initial_configure_sent {
                    return false;
                }
                if let Some(placement) = state
                    .layer_placements
                    .iter()
                    .find(|placement| placement.surface.wl_surface() == surface)
                {
                    *states
                        .cached_state
                        .get::<LayerSurfaceCachedState>()
                        .pending() = LayerSurfaceCachedState {
                        last_acked: None,
                        ..placement.state
                    };
                }
                true
            });
            if unmapped {
                state.layer_surfaces.remove(index);
            }
        } else {
            let Some(layer_surface) = state
                .layer_shell_state
                .layer_surfaces()
                .find(|layer_surface| layer_surface.wl_surface() == surface)
            else {
                return;
            };
            state.layer_surfaces.push(layer_surface);
        }
        self.relayout();
    }

    /// Arranges the layer surfaces, and tells the root if its area changed
    pub(crate) fn relayout(&mut self) {
        if self.smithay_state.arrange_layers() {
            self.send_content_size();
        }
        self.refresh_keyboard_focus();
        self.needs_redraw = true;
    }

    /// Gives the keyboard to the layer surface that should have it,
    /// or back to Sonamu's focus if a layer surface has it and shouldn't
    fn refresh_keyboard_focus(&mut self) {
        if let Some(layer) = self.smithay_state.keyboard_layer() {
            self.focus_wl_keyboard_on(layer);
            return;
        }
        let state = &self.smithay_state;
        let layer_focused = state
            .seat
            .get_keyboard()
            .and_then(|keyboard| keyboard.current_focus())
            .is_some_and(|focus| {
                let root = state
                    .popup_manager
                    .find_popup(&focus)
                    .and_then(|popup| find_popup_root_surface(&popup).ok())
                    .unwrap_or(focus);
                state.layer_rect(&root).is_some()
            });
        if layer_focused {
            self.focus_wl_keyboard(self.smithay_state.wl_target);
        }
    }

    /// Sends the mouse event (in window px) to the layer surface under it, or that a pressed button is held on.
    /// Returns false if there is none, for the root.
    pub(crate) fn layer_mouse(&mut self, event: &MouseEvent) -> bool {
        let state = &mut self.smithay_state;
        let hit = state.layer_surface_under(event.position);
        // a held button keeps the pointer on its surface
        let focus = if state.layer_buttons_down > 0 {
            state
                .pointer_focus
                .clone()
                .filter(|(surface, _)| surface.is_alive())
                .or(hit)
        } else {
            hit
        };
        let pressed = matches!(event.kind, MouseEventKind::Button { pressed: true, .. });
        let released = matches!(event.kind, MouseEventKind::Button { pressed: false, .. });
        let location = Point::<f64, Physical>::from((event.position[0], event.position[1]))
            .to_logical(state.scale);

        let Some(focus) = focus else {
            // clicking elsewhere takes the keyboard back
            let lost_keyboard = pressed && state.layer_focus.take().is_some();
            let left = std::mem::take(&mut state.pointer_on_layer);
            if left {
                self.send_wl_mouse(location, None, MouseEventKind::Motion, event.time_msec);
            }
            if lost_keyboard {
                self.refresh_keyboard_focus();
            }
            return false;
        };
        state.pointer_on_layer = true;
        if released {
            state.layer_buttons_down = state.layer_buttons_down.saturating_sub(1);
        }
        if pressed {
            state.layer_buttons_down += 1;
            let root = state
                .popup_manager
                .find_popup(&focus.0)
                .and_then(|popup| find_popup_root_surface(&popup).ok())
                .unwrap_or_else(|| focus.0.clone());
            if state.takes_keyboard(&root) {
                state.layer_focus = Some(root.clone());
                self.focus_wl_keyboard_on(root);
            }
        }
        self.send_wl_mouse(location, Some(focus), event.kind, event.time_msec);
        true
    }

    /// Sends a key press (evdev code) to the layer surface that has the keyboard.
    /// Returns false if there is none, for the root.
    pub(crate) fn layer_key_press(&mut self, raw_keycode: u32) -> bool {
        if self.smithay_state.keyboard_layer().is_none() {
            return false;
        }
        // evdev -> xkb
        self.wl_key_input(raw_keycode + 8, KeyState::Pressed);
        true
    }
}

impl WlrLayerShellHandler for AppletRunner {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.smithay_state.layer_shell_state
    }

    /// Placed (and told its size) on its first commit
    fn new_layer_surface(
        &mut self,
        surface: LayerSurface,
        _output: Option<WlOutput>,
        _layer: Layer,
        _namespace: String,
    ) {
        // there is only one output
        self.smithay_state.output.enter(surface.wl_surface());
    }

    fn new_popup(&mut self, _parent: LayerSurface, popup: PopupSurface) {
        // the same as an xdg toplevel's popup
        self.smithay_state.unconstrain_popup(&popup);
        if let Err(err) = self
            .smithay_state
            .popup_manager
            .track_popup(PopupKind::from(popup))
        {
            dbg!("Failed to track popup:", err);
        }
    }

    fn layer_destroyed(&mut self, surface: LayerSurface) {
        self.smithay_state
            .layer_surfaces
            .retain(|layer_surface| *layer_surface != surface);
        self.relayout();
    }
}
