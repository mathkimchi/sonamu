//! wl outputs: one per screen, in `screens`' order (the main one first), like other compositors.
//! Named after the monitor's connector (as wlr-output-management's heads, so clients match them), else the
//! screen's name (`window-<i>`).

use super::SmithayState;
use crate::runner::AppletRunner;
use smithay::{
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::wayland_server::{backend::GlobalId, protocol::wl_surface::WlSurface},
    utils::{Logical, Physical, Point, Rectangle},
    wayland::compositor::with_states,
};
use sonamu_ui::{display_units::DisplayContainerSize, winit_backend::layout::MonitorInfo};

/// A layer surface's output's name: the one it asked for, else the screen under the pointer.
/// Without it (made before there were outputs), the main one.
struct LayerOutput(String);

impl SmithayState {
    /// Makes the outputs match the screens (`names`, as `screens`): new screens get one, gone ones lose theirs
    /// (closing its layer surfaces), and each is placed
    pub(crate) fn sync_outputs(&mut self, names: &[String]) {
        let monitors = self.output_management.monitors();
        let wanted: Vec<(String, Option<MonitorInfo>)> = names
            .iter()
            .map(|name| {
                let monitor = monitors.iter().find(|monitor| &monitor.name == name);
                (
                    monitor
                        .map_or_else(|| name.clone(), |monitor| monitor.monitor.connector.clone()),
                    monitor.cloned(),
                )
            })
            .collect();
        let mut old = std::mem::take(&mut self.outputs);
        old.retain(|output| {
            let kept = wanted.iter().any(|(name, _)| *name == output.name());
            if !kept {
                self.remove_output(output);
            }
            kept
        });
        self.outputs = wanted
            .into_iter()
            .map(|(name, monitor)| {
                old.iter()
                    .find(|output| output.name() == name)
                    .cloned()
                    .unwrap_or_else(|| self.new_output(name, monitor.as_ref()))
            })
            .collect();
        self.place_outputs();
    }

    fn new_output(&self, name: String, monitor: Option<&MonitorInfo>) -> Output {
        let (size, [make, model, serial_number]) = monitor.map_or_else(
            || ([0; 2], ["Sonamu", "Sonamu", "Unknown"].map(String::from)),
            |monitor| {
                (
                    monitor.physical_size.map(u32::cast_signed),
                    monitor.monitor.identity.clone(),
                )
            },
        );
        let output = Output::new(
            name,
            PhysicalProperties {
                size: (size[0], size[1]).into(),
                subpixel: Subpixel::Unknown,
                make,
                model,
                serial_number,
            },
        );
        let global = output.create_global::<AppletRunner>(&self.display_handle);
        output.user_data().insert_if_missing(|| global);
        output
    }

    /// Its global goes, and its layer surfaces are closed (like wlroots)
    fn remove_output(&mut self, output: &Output) {
        let name = output.name();
        let on_it = |surface: &WlSurface| layer_output(surface).as_ref() == Some(&name);
        for layer_surface in self.layer_shell_state.layer_surfaces() {
            if on_it(layer_surface.wl_surface()) {
                layer_surface.send_close();
            }
        }
        self.layer_surfaces
            .retain(|layer_surface| !on_it(layer_surface.wl_surface()));
        output.leave_all();
        if let Some(global) = output.user_data().get::<GlobalId>() {
            self.display_handle
                .remove_global::<AppletRunner>(global.clone());
        }
    }

    /// Each output's mode (its screen's size, its monitor's refresh), place and scale
    pub(super) fn place_outputs(&self) {
        let monitors = self.output_management.monitors();
        for (output, &screen) in self.outputs.iter().zip(&self.screens) {
            let [_, _, width, height] = screen;
            let refresh = monitors
                .iter()
                .find(|monitor| monitor.monitor.connector == output.name())
                .map(|monitor| monitor.refresh_millihertz)
                .or(self.refresh_millihertz)
                // 60 Hz when unknown
                .unwrap_or(60_000);
            let mode = Mode {
                size: (width.cast_signed(), height.cast_signed()).into(),
                refresh: refresh.cast_signed(),
            };
            output.change_current_state(
                Some(mode),
                None,
                Some(Scale::Fractional(self.scale)),
                Some(self.screen_logical(screen).loc),
            );
            output.set_preferred(mode);
            for old in output.modes().into_iter().filter(|&old| old != mode) {
                output.delete_mode(old);
            }
        }
        self.update_capture_constraints();
    }

    /// A screen (window px) in logical units
    pub(super) fn screen_logical(
        &self,
        [x, y, width, height]: [u32; 4],
    ) -> Rectangle<i32, Logical> {
        Rectangle::new(
            Point::<f64, Physical>::from((f64::from(x), f64::from(y)))
                .to_logical(self.scale)
                .to_i32_round(),
            self.to_logical(DisplayContainerSize::new(width, height)),
        )
    }

    /// The index of the screen under `position` (window px)
    pub(crate) fn screen_at(&self, [px, py]: [f64; 2]) -> Option<usize> {
        self.screens.iter().position(|&[x, y, width, height]| {
            (f64::from(x)..f64::from(x + width)).contains(&px)
                && (f64::from(y)..f64::from(y + height)).contains(&py)
        })
    }

    /// Puts the surface on the screen's output only
    pub(super) fn enter_screen(&self, surface: &WlSurface, screen: usize) {
        for (index, output) in self.outputs.iter().enumerate() {
            if index == screen {
                output.enter(surface);
            } else {
                output.leave(surface);
            }
        }
    }

    /// The index of the layer surface's screen
    pub(super) fn layer_screen(&self, surface: &WlSurface) -> usize {
        layer_output(surface)
            .and_then(|name| self.outputs.iter().position(|output| output.name() == name))
            .unwrap_or_default()
    }

    /// Gives a new layer surface an output: `requested`, else the screen under `pointer`'s, else the main one
    pub(super) fn set_layer_output(
        &self,
        surface: &WlSurface,
        requested: Option<Output>,
        pointer: [f64; 2],
    ) {
        let output = requested
            .filter(|output| self.outputs.contains(output))
            .or_else(|| {
                self.screen_at(pointer)
                    .and_then(|screen| self.outputs.get(screen).cloned())
            });
        if let Some(output) = output {
            with_states(surface, |states| {
                states
                    .data_map
                    .insert_if_missing_threadsafe(|| LayerOutput(output.name()));
            });
        }
        self.enter_screen(surface, self.layer_screen(surface));
    }
}

fn layer_output(surface: &WlSurface) -> Option<String> {
    with_states(surface, |states| {
        states
            .data_map
            .get::<LayerOutput>()
            .map(|LayerOutput(name)| name.clone())
    })
}
