//! Floating toplevels (#148), like niri: dialogs (with a parent) and fixed-size ones (eg password prompts).
//! Not in the node tree: centered over a screen, drawn and given input like overlay layer surfaces.

use super::{Layer, SmithayState, TILED, window_geometry};
use crate::runner::AppletRunner;
use smithay::{
    desktop::utils::bbox_from_surface_tree,
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Logical, Physical, Point, Rectangle},
    wayland::{
        compositor::with_states,
        shell::xdg::{SurfaceCachedState, ToplevelSurface},
    },
};
use sonamu_ui::{
    ui_element::{Style, UITree},
    ui_event::{MouseEvent, MouseEventKind},
};

const BTN_LEFT: u32 = 0x110;

/// A floating toplevel, centered on its screen (read when drawn: it may map before the screens are known)
/// and moved by `offset` (Super+drag)
#[derive(Debug)]
pub(super) struct Floating {
    toplevel: ToplevelSurface,
    screen: usize,
    offset: Point<f64, Logical>,
}

/// Whether the toplevel floats instead of tiling
pub(super) fn floats(toplevel: &ToplevelSurface) -> bool {
    toplevel.parent().is_some()
        || with_states(toplevel.wl_surface(), |states| {
            let mut cached = states.cached_state.get::<SurfaceCachedState>();
            let SurfaceCachedState {
                min_size, max_size, ..
            } = *cached.current();
            min_size.w > 0 && min_size.h > 0 && min_size == max_size
        })
}

impl SmithayState {
    /// Where the floating toplevel is in the output: its window geometry, centered on its screen, then moved
    pub(super) fn floating_rect(&self, root: &WlSurface) -> Option<Rectangle<i32, Logical>> {
        let &Floating { screen, offset, .. } = self
            .floating
            .iter()
            .find(|floating| floating.toplevel.wl_surface() == root)?;
        let [x, y, width, height] = self
            .screens
            .get(screen)
            .or(self.screens.first())
            .copied()
            .unwrap_or_default();
        let size = window_geometry(root)
            .unwrap_or_else(|| bbox_from_surface_tree(root, (0, 0)))
            .size;
        let center = Point::<f64, Physical>::from((
            f64::from(x) + f64::from(width) / 2.0,
            f64::from(y) + f64::from(height) / 2.0,
        ))
        .to_logical(self.scale);
        Some(Rectangle::new(
            (center + offset).to_i32_round() - Point::from((size.w / 2, size.h / 2)),
            size,
        ))
    }

    /// Its surfaces (with popups), placed in the output
    pub(super) fn floating_layers(&self, root: &WlSurface) -> Vec<Layer> {
        let Some(rect) = self.floating_rect(root) else {
            return Vec::new();
        };
        let mut layers = Self::window_layers(root);
        for layer in &mut layers {
            layer.location += rect.loc;
        }
        layers
    }

    /// The floating toplevels' roots, bottom first
    pub(super) fn floating_roots(&self) -> impl DoubleEndedIterator<Item = &WlSurface> {
        self.floating
            .iter()
            .map(|floating| floating.toplevel.wl_surface())
    }

    /// Drawn in the window's px, over everything
    pub fn floating_elements(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<UITree> {
        let size = self
            .output
            .current_mode()
            .map_or([0.0; 2], |mode| [mode.size.w as f32, mode.size.h as f32]);
        self.floating_roots()
            .map(|root| {
                let layers = self.floating_layers(root);
                let cursor = self.wl_cursor(&layers);
                // like a tile, the root's surfaces are cut to the window geometry
                let clip = window_geometry(root).and(self.floating_rect(root));
                UITree::new_container_styled(
                    Style::absolute([0.0, 0.0], size),
                    self.layers_to_textures(layers, clip, device, queue),
                )
                .with_cursor(cursor)
            })
            .collect()
    }
}

impl AppletRunner {
    /// Floats the newly committed toplevel over the screen under the pointer, and gives it the keyboard
    pub(super) fn float(&mut self, toplevel: ToplevelSurface) {
        let [px, py] = self.pointer_position;
        let state = &mut self.smithay_state;
        let screen = state
            .screens
            .iter()
            .position(|&[x, y, width, height]| {
                (f64::from(x)..f64::from(x + width)).contains(&px)
                    && (f64::from(y)..f64::from(y + height)).contains(&py)
            })
            .unwrap_or_default();
        // its own size, not a tile's
        toplevel.with_pending_state(|state| {
            state.size = None;
            for tiled in TILED {
                state.states.unset(tiled);
            }
        });
        toplevel.send_configure();
        let root = toplevel.wl_surface().clone();
        state.floating.push(Floating {
            toplevel,
            screen,
            offset: Point::default(),
        });
        state.layer_focus = Some(root);
        self.refresh_keyboard_focus();
        self.needs_redraw = true;
    }

    /// Returns false if the toplevel doesn't float
    pub(super) fn floating_destroyed(&mut self, toplevel: &ToplevelSurface) -> bool {
        let state = &mut self.smithay_state;
        let Some(index) = state
            .floating
            .iter()
            .position(|floating| floating.toplevel == *toplevel)
        else {
            return false;
        };
        state.floating.remove(index);
        if state.floating_drag == Some(index) {
            state.floating_drag = None;
        }
        if state.layer_focus.as_ref() == Some(toplevel.wl_surface()) {
            state.layer_focus = None;
        }
        // the keyboard goes back to Sonamu's focus (or a layer surface)
        self.focus_wl_keyboard(self.smithay_state.wl_target);
        self.needs_redraw = true;
        true
    }

    /// Super+left drag moves a floating toplevel (raising it); returns whether the event was taken
    pub(crate) fn floating_drag_mouse(&mut self, event: &MouseEvent, last_position: [f64; 2]) -> bool {
        let state = &mut self.smithay_state;
        if let Some(index) = state.floating_drag {
            match event.kind {
                MouseEventKind::Motion => {
                    let [x, y] = [0, 1].map(|axis| event.position[axis] - last_position[axis]);
                    state.floating[index].offset +=
                        Point::<f64, Physical>::from((x, y)).to_logical(state.scale);
                    self.needs_redraw = true;
                }
                MouseEventKind::Button {
                    button: BTN_LEFT,
                    pressed: false,
                } => state.floating_drag = None,
                _ => {}
            }
            return true;
        }
        if !(self.super_held
            && event.kind
                == (MouseEventKind::Button {
                    button: BTN_LEFT,
                    pressed: true,
                }))
        {
            return false;
        }
        let point = Point::<f64, Physical>::from((event.position[0], event.position[1]))
            .to_logical(state.scale);
        let Some(index) = state.floating.iter().rposition(|floating| {
            state
                .floating_rect(floating.toplevel.wl_surface())
                .is_some_and(|rect| rect.to_f64().contains(point))
        }) else {
            return false;
        };
        let floating = state.floating.remove(index);
        let root = floating.toplevel.wl_surface().clone();
        state.floating.push(floating);
        state.floating_drag = Some(state.floating.len() - 1);
        state.layer_focus = Some(root.clone());
        self.focus_wl_keyboard_on(root);
        self.needs_redraw = true;
        true
    }
}
