mod capture;
mod floating;
pub(crate) mod dmabuf;
mod layer_shell;

use crate::runner::AppletRunner;
use calloop::{LoopHandle, PostAction};
use slotmap::{SecondaryMap, SlotMap};
use smithay::backend::allocator::Buffer as _;
use smithay::{
    backend::{
        input::{Axis, AxisSource, ButtonState, InputTime, KeyState, Keycode},
        renderer::{
            BufferType, buffer_type,
            utils::{
                Buffer, CommitCounter, RendererSurfaceStateUserData, SurfaceView,
                on_commit_buffer_handler,
            },
        },
    },
    desktop::{
        PopupKeyboardGrab, PopupKind, PopupManager, PopupPointerGrab, PopupUngrabStrategy,
        find_popup_root_surface, get_popup_toplevel_coords,
    },
    input::{
        Seat, SeatHandler, SeatState,
        dnd::{DnDGrab, DndGrabHandler, DndTarget, GrabType, Source},
        keyboard::FilterResult,
        pointer::{
            AxisFrame, ButtonEvent, CursorImageStatus, CursorImageSurfaceData, Focus,
            GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent, MotionEvent,
            PointerHandle, RelativeMotionEvent,
        },
        tablet::TabletSeatHandler,
    },
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback::Kind as PresentationFeedbackKind,
    reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
    reexports::wayland_protocols::xdg::shell::server::xdg_toplevel,
    reexports::wayland_server::{
        Client, Display, DisplayHandle, Resource as _,
        backend::ClientData,
        protocol::{wl_output::WlOutput, wl_seat::WlSeat, wl_surface::WlSurface},
    },
    utils::{
        Buffer as BufferCoords, Clock, Logical, Monotonic, Physical, Point, Rectangle,
        SERIAL_COUNTER, Serial, Size, Time, Transform,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            BufferAssignment, CompositorClientState, CompositorHandler, CompositorState,
            SurfaceAttributes, SurfaceData, TraversalAction, add_blocker, add_pre_commit_hook,
            send_surface_state, with_states, with_surface_tree_downward, with_surface_tree_upward,
        },
        cursor_shape::CursorShapeManagerState,
        fractional_scale::{
            FractionalScaleHandler, FractionalScaleManagerState, with_fractional_scale,
        },
        idle_inhibit::{IdleInhibitHandler, IdleInhibitManagerState},
        output::{OutputHandler, OutputManagerState},
        pointer_constraints::{
            PointerConstraint, PointerConstraintsHandler, PointerConstraintsState,
            with_pointer_constraint,
        },
        pointer_gestures::PointerGesturesState,
        presentation::{PresentationFeedbackCachedState, PresentationState, Refresh},
        relative_pointer::RelativePointerManagerState,
        security_context::SecurityContext,
        selection::{
            SelectionHandler, SelectionTarget,
            data_device::{
                DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler,
                current_data_device_selection_userdata, request_data_device_client_selection,
                set_data_device_focus, set_data_device_selection,
            },
            ext_data_control,
            primary_selection::{
                PrimarySelectionHandler, PrimarySelectionState, current_primary_selection_userdata,
                request_primary_client_selection, set_primary_focus, set_primary_selection,
            },
            wlr_data_control,
        },
        shell::wlr_layer::{LayerSurface, WlrLayerShellState},
        shell::xdg::{
            PopupSurface, PositionerState, SurfaceCachedState, ToplevelSurface, XdgShellHandler,
            XdgShellState, XdgToplevelSurfaceData,
            decoration::{XdgDecorationHandler, XdgDecorationState},
        },
        shm::{ShmHandler, ShmState},
        socket::ListeningSocketSource,
        text_input::{TextInputActivation, TextInputManagerState, TextInputSeat as _},
        viewporter::ViewporterState,
        xdg_activation::{
            XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
        },
    },
};
use sonamu_common::clipboard::Target;
use sonamu_common::sap::packets::{
    NewWlSurface, StandardEvent, WlSurfaceEvent, WlSurfaceId, WlSurfaceRequest,
};
use sonamu_ui::{
    DisplayInfo, ImeEvent, PointerGrab,
    display_units::{DisplayAreaPx, DisplayContainerSize},
    mipmaps,
    ui_element::{Cursor, Style, UIElement, UITree},
    ui_event::{MouseEvent, MouseEventKind, PinchPhase},
    winit_backend::drm::{REPEAT_DELAY, REPEAT_INTERVAL},
};
use std::{collections::HashMap, os::fd::OwnedFd, sync::Arc, time::Duration};
use wgpu::{TextureView, TextureViewDescriptor};

#[derive(Debug, Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
    pub security_context: Option<SecurityContext>,
}
impl ClientData for ClientState {
    /// Notification that a client was initialized
    fn initialized(&self, _client_id: smithay::reexports::wayland_server::backend::ClientId) {
        dbg!("Client initialized");
    }
    /// Notification that a client is disconnected
    fn disconnected(
        &self,
        client_id: smithay::reexports::wayland_server::backend::ClientId,
        reason: smithay::reexports::wayland_server::backend::DisconnectReason,
    ) {
        eprintln!("Client {client_id:?} disconnected: {reason:?}");
    }
}

/// TODO: rename to smithay client handle (adds consistency w/ ui handle and client handle)
#[derive(Debug)]
pub struct SmithayState {
    start_time: std::time::Instant,
    display_handle: DisplayHandle,

    // _loop_signal: LoopSignal,

    // smithay state
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    layer_shell_state: WlrLayerShellState,
    shm_state: ShmState,
    seat_state: SeatState<AppletRunner>,
    _output_manager_state: OutputManagerState,
    data_device_state: DataDeviceState,
    primary_selection_state: PrimarySelectionState,
    ext_data_control_state: ext_data_control::DataControlState,
    wlr_data_control_state: wlr_data_control::DataControlState,
    _xdg_decoration_state: XdgDecorationState,
    _idle_inhibit_state: IdleInhibitManagerState,
    popup_manager: PopupManager,
    _viewporter_state: ViewporterState,
    _presentation_state: PresentationState,
    /// For presentation times
    clock: Clock<Monotonic>,
    presented_frames: u64,
    /// Latest cursor set by the client under the pointer
    cursor_status: CursorImageStatus,
    _cursor_shape_state: CursorShapeManagerState,
    _fractional_scale_state: FractionalScaleManagerState,
    xdg_activation_state: XdgActivationState,
    _text_input_state: TextInputManagerState,
    _pointer_constraints_state: PointerConstraintsState,
    _relative_pointer_state: RelativePointerManagerState,
    _pointer_gestures_state: PointerGesturesState,
    capture: capture::CaptureState,
    dmabuf: dmabuf::DmabufImport,
    /// For waiting on clients' GPU work before applying their commits
    event_handle: LoopHandle<'static, AppletRunner>,
    /// The last surface the pointer was sent to and its origin, for relative motion
    pointer_focus: Option<(WlSurface, Point<f64, Logical>)>,
    /// From the last redraw, in Sonamu window px
    tile_areas: HashMap<WlSurfaceId, DisplayAreaPx>,
    /// Where to warp the pointer when the current lock ends (window px), from `cursor_position_hint`
    pub(crate) lock_end_position: Option<[f64; 2]>,
    /// Drawn at the pointer plus its offset during a drag and drop
    dnd_icon: Option<(WlSurface, Point<i32, Logical>)>,
    /// Of the Sonamu window; wl clients' logical units are px / scale
    scale: f64,
    refresh_millihertz: Option<u32>,

    seat: Seat<AppletRunner>,
    output: Output,
    /// What wl clients set `WAYLAND_DISPLAY` to, to connect here
    pub(crate) socket_name: std::ffi::OsString,

    // _main_client: Option<Client>,
    // surface: Option<WlSurface>,
    surfaces: SlotMap<WlSurfaceId, WlSurface>,
    /// To the applets of registered surfaces (`NewWlSurface::wl_event_queue`)
    wl_event_queues: SecondaryMap<WlSurfaceId, calloop::channel::Sender<WlSurfaceEvent>>,
    /// Layer surfaces that have committed (outside `surfaces` and the node tree)
    layer_surfaces: Vec<LayerSurface>,
    /// From the last `arrange_layers`, topmost layer first
    layer_placements: Vec<layer_shell::LayerPlacement>,
    /// Output minus the exclusive zones, in logical units: where the root's content goes
    usable_area: Rectangle<i32, Logical>,
    /// The layer surface (not an exclusive one) or floating toplevel that a click gave the keyboard
    layer_focus: Option<WlSurface>,
    /// Floating toplevels, topmost last
    floating: Vec<floating::Floating>,
    /// The index of the floating toplevel being dragged
    floating_drag: Option<usize>,
    /// The tile that Sonamu focus asked the keyboard to be on
    pub(crate) wl_target: Option<WlSurfaceId>,
    /// The screens' areas (`[x, y, width, height]`, window px): the monitors', or the whole window
    pub(crate) screens: Vec<[u32; 4]>,
    /// The fullscreen toplevel, and its tile's size to go back to.
    /// Drawn over everything but overlay layers, until it asks to stop or Sonamu focus leaves it (#121).
    fullscreen: Option<(WlSurfaceId, Size<i32, Logical>)>,
    /// The pointer is on a layer surface, or a button pressed on one is still held
    pointer_on_layer: bool,
    layer_buttons_down: u32,
    /// Toplevels not yet told to the root (or floated): they are once they map (`register_toplevel`)
    unregistered: Vec<WlSurfaceId>,
    // image: Arc<Mutex<Option<RgbaImage>>>,
    // // I think it might be more efficient to share the seat
    // // but this is easier for me to implement
    // input_queue: mpsc::Receiver<UIEvent>,
    // // Singularity stuff
    // hook: Box<dyn NodularRunnerHook>,
    /// events from wl surface applets that need to be sent to their surface
    /// Key events work by first setting key seat's focus to the target surface
    /// Ignores things like timestamp, serial, ...
    wl_request_queue: calloop::channel::Sender<(WlSurfaceId, WlSurfaceRequest)>,
}
impl SmithayState {
    /// To the surface's applet; does nothing if it isn't registered
    fn send_wl_event(&self, surface_id: WlSurfaceId, event: WlSurfaceEvent) {
        if let Some(queue) = self.wl_event_queues.get(surface_id) {
            // the applet may be gone
            let _ = queue.send(event);
        }
    }

    fn surface_id_of(&self, surface: &WlSurface) -> Option<WlSurfaceId> {
        self.surfaces
            .iter()
            .find_map(|(id, registered)| (registered == surface).then_some(id))
    }

    /// The screen the tile is on: the one with its center (else the first)
    fn screen_of_tile(&self, surface_id: WlSurfaceId) -> [u32; 4] {
        let center = self.tile_areas.get(&surface_id).map(
            |DisplayAreaPx([[left, top], [right, bottom]])| {
                [(left + right) / 2, (top + bottom) / 2]
            },
        );
        self.screens
            .iter()
            .copied()
            .find(|&[x, y, width, height]| {
                center.is_some_and(|[center_x, center_y]| {
                    let [x, y, right, bottom] = [x, y, x + width, y + height].map(u32::cast_signed);
                    (x..right).contains(&center_x) && (y..bottom).contains(&center_y)
                })
            })
            .or_else(|| self.screens.first().copied())
            .unwrap_or_default()
    }

    /// In logical units
    fn screen_size(&self, [_, _, width, height]: [u32; 4]) -> Size<i32, Logical> {
        self.to_logical(DisplayContainerSize::new(width, height))
    }

    pub fn fullscreen(&self) -> Option<WlSurfaceId> {
        self.fullscreen.map(|(surface_id, _)| surface_id)
    }

    /// The area of the screen the fullscreen toplevel covers
    pub fn fullscreen_screen(&self) -> Option<[u32; 4]> {
        Some(self.screen_of_tile(self.fullscreen()?))
    }

    /// Keeps the fullscreen toplevel at its screen's size
    pub fn resize_fullscreen(&self) {
        let (Some(toplevel), Some(screen)) = (
            self.fullscreen()
                .and_then(|surface_id| self.surfaces.get(surface_id))
                .and_then(|surface| self.toplevel_of(surface)),
            self.fullscreen_screen(),
        ) else {
            return;
        };
        toplevel.with_pending_state(|state| state.size = Some(self.screen_size(screen)));
        toplevel.send_configure();
    }

    /// Has the target's owner write its text into a copy of `fd` (`sonamu_common::clipboard::paste`); without one,
    /// the reader sees nothing once `fd` is dropped
    pub fn write_clipboard(&self, fd: &OwnedFd, target: Target) {
        // one a native applet copied (`set_clipboard`)
        let own = match target {
            Target::Clipboard => {
                current_data_device_selection_userdata(&self.seat).map(|text| Arc::clone(&text))
            }
            Target::Primary => {
                current_primary_selection_userdata(&self.seat).map(|text| Arc::clone(&text))
            }
        };
        if let Some(text) = own
            && let Ok(copy) = fd.try_clone()
        {
            write_text(copy, text);
            return;
        }
        // the first text type it offers; a failed request drops its fd, so each gets a copy
        for mime_type in TEXT_MIME_TYPES {
            let Ok(copy) = fd.try_clone() else {
                return;
            };
            let requested = match target {
                Target::Clipboard => {
                    request_data_device_client_selection(&self.seat, mime_type.to_string(), copy)
                        .is_ok()
                }
                Target::Primary => {
                    request_primary_client_selection(&self.seat, mime_type.to_string(), copy)
                        .is_ok()
                }
            };
            if requested {
                return;
            }
        }
    }

    /// Makes the runner the target's owner, offering `text` (a native applet copied it)
    pub fn set_clipboard(&self, text: String, target: Target) {
        let mime_types = TEXT_MIME_TYPES.map(ToString::to_string).to_vec();
        match target {
            Target::Clipboard => {
                set_data_device_selection(
                    &self.display_handle,
                    &self.seat,
                    mime_types,
                    Arc::new(text),
                );
            }
            Target::Primary => {
                set_primary_selection(&self.display_handle, &self.seat, mime_types, Arc::new(text));
            }
        }
    }

    pub fn flush_clients(&mut self) {
        if let Err(err) = self.display_handle.flush_clients() {
            dbg!("Failed to flush clients:", err);
        }
    }

    fn toplevel_of(&self, surface: &WlSurface) -> Option<ToplevelSurface> {
        self.xdg_shell_state
            .toplevel_surfaces()
            .iter()
            .find(|toplevel| toplevel.wl_surface() == surface)
            .cloned()
    }

    /// Advertises dmabufs to clients if `device` can import them
    pub fn init_dmabuf(&mut self, device: &wgpu::Device) {
        self.dmabuf.init(&self.display_handle, device);
    }

    /// Where each wl toplevel's tile was laid out in the last redraw
    pub fn set_tile_areas(
        &mut self,
        tile_areas: impl Iterator<Item = (WlSurfaceId, DisplayAreaPx)>,
    ) {
        self.tile_areas.clear();
        self.tile_areas.extend(tile_areas);

        // hidden toplevels may stop rendering (eg videos in an inactive node, #136)
        for toplevel in self.xdg_shell_state.toplevel_surfaces() {
            let Some(surface_id) = self
                .surfaces
                .iter()
                .find_map(|(id, surface)| (surface == toplevel.wl_surface()).then_some(id))
            else {
                continue;
            };
            let suspended = !self.tile_areas.contains_key(&surface_id);
            toplevel.with_pending_state(|state| {
                use wayland_protocols::xdg::shell::server::xdg_toplevel::State;
                if suspended {
                    state.states.set(State::Suspended);
                } else {
                    state.states.unset(State::Suspended);
                }
            });
            if toplevel.is_initial_configure_sent() {
                toplevel.send_pending_configure();
            }
        }
    }

    /// Same as anvil: keeps the popup inside its tile's screen (or a layer surface's, inside the output).
    /// Falls back to the toplevel's tile if the tile hasn't been laid out yet.
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let kind = PopupKind::Xdg(popup.clone());
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };

        // the tile's screen, relative to the tile
        let output_in_tile = self
            .surfaces
            .iter()
            .find_map(|(surface_id, surface)| (*surface == root).then_some(surface_id))
            .and_then(|surface_id| {
                let DisplayAreaPx([[left, top], _]) = self.tile_areas.get(&surface_id)?;
                let [x, y, width, height] = self.screen_of_tile(surface_id);
                Some(Rectangle::new(
                    Point::<f64, Physical>::from((
                        f64::from(x) - f64::from(*left),
                        f64::from(y) - f64::from(*top),
                    ))
                    .to_logical(self.scale)
                    .to_i32_round(),
                    self.screen_size([x, y, width, height]),
                ))
            });
        let tile = || {
            self.xdg_shell_state
                .toplevel_surfaces()
                .iter()
                .find(|toplevel| toplevel.wl_surface() == &root)
                // the latest size sent, which is the tile size
                .and_then(|toplevel| toplevel.with_pending_state(|state| state.size))
                .map(Rectangle::from_size)
        };
        // for a layer surface's popup, relative to the layer surface
        let output_in_layer =
            self.layer_rect(&root)
                .zip(self.output.current_mode())
                .map(|(rect, mode)| {
                    Rectangle::new(
                        Point::default() - rect.loc,
                        mode.size.to_f64().to_logical(self.scale).to_i32_round(),
                    )
                });
        let Some(mut target) = output_in_tile.or(output_in_layer).or_else(tile) else {
            return;
        };

        // relative to the popup's parent's window geometry, like the positioner
        // (the tile is the root's window geometry)
        target.loc -= get_popup_toplevel_coords(&kind);

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }

    /// Called after a frame is presented: sends frame callbacks (so clients render at the display's pace)
    /// and presentation feedback.
    /// Tiles that weren't drawn keep their callbacks until they are, so hidden clients stop rendering (#136).
    pub fn frame_presented(&mut self) {
        self.presented_frames += 1;
        let presentation = Presentation {
            frame_time: self.start_time.elapsed().as_millis() as u32,
            output: &self.output,
            time: self.clock.now(),
            refresh: self.refresh_millihertz.map_or(Refresh::Unknown, |refresh| {
                Refresh::fixed(Duration::from_nanos(1_000_000_000_000 / u64::from(refresh)))
            }),
            seq: self.presented_frames,
        };

        if let CursorImageStatus::Surface(cursor_surface) = &self.cursor_status
            && cursor_surface.is_alive()
        {
            send_frames_surface_tree(cursor_surface, &presentation);
        }
        if let Some((icon, _)) = self.dnd_icon.as_ref().filter(|(icon, _)| icon.is_alive()) {
            send_frames_surface_tree(icon, &presentation);
        }
        for root in self
            .surfaces
            .iter()
            .filter(|(surface_id, _)| self.tile_areas.contains_key(surface_id))
            .map(|(_, surface)| surface)
            .chain(self.layer_surfaces.iter().map(LayerSurface::wl_surface))
            .chain(self.floating_roots())
        {
            send_frames_surface_tree(root, &presentation);
            for (popup, _) in PopupManager::popups_for_surface(root) {
                send_frames_surface_tree(popup.wl_surface(), &presentation);
            }
        }
    }

    /// The single output represents the whole Sonamu window
    pub fn resize_output(&self, size: DisplayContainerSize) {
        let mode = Mode {
            size: (size.width as i32, size.height as i32).into(),
            // 60 Hz when unknown
            refresh: self.refresh_millihertz.map_or(60_000, u32::cast_signed),
        };
        self.output
            .change_current_state(Some(mode), None, None, None);
        self.output.set_preferred(mode);
        self.update_capture_constraints();
    }

    /// Updates the output, and every surface's preferred scale and toplevel's size
    pub fn set_display_info(&mut self, info: DisplayInfo) {
        let old_scale = self.scale;
        self.scale = info.scale_factor;
        self.refresh_millihertz = info.refresh_millihertz;

        if let Some(mode) = self.output.current_mode() {
            self.resize_output(DisplayContainerSize::new(
                mode.size.w.cast_unsigned(),
                mode.size.h.cast_unsigned(),
            ));
        }
        self.output
            .change_current_state(None, None, Some(Scale::Fractional(self.scale)), None);

        #[expect(clippy::float_cmp, reason = "any change in scale")]
        if old_scale != self.scale {
            if let Some((_, size)) = &mut self.fullscreen {
                *size = size
                    .to_f64()
                    .to_physical(old_scale)
                    .to_logical(self.scale)
                    .to_i32_round();
            }
            for toplevel in self.xdg_shell_state.toplevel_surfaces() {
                // same px size, in the new logical units
                toplevel.with_pending_state(|state| {
                    state.size = state.size.map(|size| {
                        size.to_f64()
                            .to_physical(old_scale)
                            .to_logical(self.scale)
                            .to_i32_round()
                    });
                });
                if toplevel.is_initial_configure_sent() {
                    toplevel.send_pending_configure();
                }
            }
        }

        for root in self
            .surfaces
            .values()
            .chain(self.layer_surfaces.iter().map(LayerSurface::wl_surface))
            .chain(self.floating_roots())
        {
            self.send_scale(root);
            for (popup, _) in PopupManager::popups_for_surface(root) {
                self.send_scale(popup.wl_surface());
            }
        }
    }

    /// The client may have destroyed it since
    pub(crate) fn live_pointer_focus(&self) -> Option<&WlSurface> {
        self.pointer_focus
            .as_ref()
            .map(|(surface, _)| surface)
            .filter(|surface| surface.is_alive())
    }

    /// Activates the pointer-focused surface's constraint if the pointer is in its region, like anvil
    fn maybe_activate_pointer_constraint(&self) {
        let Some((surface, origin)) = self
            .pointer_focus
            .as_ref()
            .filter(|(surface, _)| surface.is_alive())
        else {
            return;
        };
        let pointer = self.seat.get_pointer().unwrap();
        let surface_location = (pointer.current_location() - *origin).to_i32_round();
        with_pointer_constraint(surface, &pointer, |constraint| {
            if let Some(constraint) = constraint
                && !constraint.is_active()
                && constraint
                    .region()
                    .is_none_or(|region| region.contains(surface_location))
            {
                constraint.activate();
            }
        });
    }

    /// How the Sonamu window should hold the pointer, from the pointer-focused surface's active constraint
    pub fn pointer_grab(&self) -> PointerGrab {
        let Some(surface) = self.live_pointer_focus() else {
            return PointerGrab::None;
        };
        let pointer = self.seat.get_pointer().unwrap();
        with_pointer_constraint(surface, &pointer, |constraint| match constraint {
            Some(constraint) if constraint.is_active() => match &*constraint {
                PointerConstraint::Confined(_) => PointerGrab::Confined,
                PointerConstraint::Locked(_) => PointerGrab::Locked,
            },
            _ => PointerGrab::None,
        })
    }

    /// Sends input method text to the keyboard-focused client's active text input
    pub fn ime_input(&self, ime: &ImeEvent) {
        let text_input = self.seat.text_input();
        text_input.with_active_text_input(|text_input, _| match ime {
            ImeEvent::Preedit { text, cursor } => {
                // -1, -1 hides the cursor
                let (begin, end) = cursor.map_or((-1, -1), |(begin, end)| {
                    (
                        i32::try_from(begin).unwrap_or(-1),
                        i32::try_from(end).unwrap_or(-1),
                    )
                });
                text_input.preedit_string(Some(text.clone()), begin, end);
            }
            ImeEvent::Commit(text) => {
                text_input.preedit_string(None, 0, 0);
                text_input.commit_string(Some(text.clone()));
            }
        });
        text_input.done(false);
    }

    /// Tells every surface in the tree the preferred scale (does nothing if unchanged)
    fn send_scale(&self, root: &WlSurface) {
        let integer_scale = self.scale.ceil() as i32;
        with_surface_tree_downward(
            root,
            (),
            |_, _, &()| TraversalAction::DoChildren(()),
            |surface, states, &()| {
                with_fractional_scale(states, |fractional_scale| {
                    fractional_scale.set_preferred_scale(self.scale);
                });
                send_surface_state(surface, states, integer_scale, Transform::Normal);
            },
            |_, _, &()| true,
        );
    }

    /// Input event time in CLOCK_MONOTONIC ms, the same clock as libinput's (like niri)
    fn now_input_time(&self) -> InputTime {
        InputTime::from_micros(Duration::from(self.clock.now()).as_micros() as u64)
    }

    /// Px in the Sonamu window to the logical units wl clients use
    fn to_logical(&self, size: DisplayContainerSize) -> Size<i32, Logical> {
        Size::<f64, Physical>::from((f64::from(size.width), f64::from(size.height)))
            .to_logical(self.scale)
            .to_i32_round()
    }

    /// A listening socket, automatically choosing the next available `wayland` socket name.
    /// `SONAMU_WAYLAND_DISPLAY` is set to it by `AppletRunner::run`
    fn listen(event_handle: &LoopHandle<'static, AppletRunner>) -> std::ffi::OsString {
        // Creates a new listening socket, automatically choosing the next available `wayland` socket name.
        let listening_socket = ListeningSocketSource::new_auto().unwrap();

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listening_socket.socket_name().to_os_string();

        event_handle
            .insert_source(listening_socket, |client_stream, (), state| {
                let _client = state
                    .smithay_state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();

                dbg!("Inserted new client!");
            })
            .unwrap();

        dbg!(&socket_name);

        socket_name
    }

    /// Dispatches the clients' requests
    fn insert_display(
        event_handle: &LoopHandle<'static, AppletRunner>,
        display: Display<AppletRunner>,
    ) {
        event_handle
            .insert_source(
                calloop::generic::Generic::new(
                    display,
                    calloop::Interest::READ,
                    calloop::Mode::Level,
                ),
                |_, display, state| {
                    // profiling::scope!("dispatch_clients");
                    // Safety: we don't drop the display
                    let display = unsafe { display.get_mut() };
                    display.dispatch_clients(state).unwrap();
                    display.flush_clients().unwrap();

                    Ok(PostAction::Continue)
                },
            )
            .unwrap();
    }

    /// With a keyboard (same repeat as the udev backend's) and a pointer
    fn new_seat(
        seat_state: &mut SeatState<AppletRunner>,
        display_handle: &DisplayHandle,
    ) -> Seat<AppletRunner> {
        let mut seat = seat_state.new_wl_seat(display_handle, "hello");
        seat.add_keyboard(
            smithay::input::keyboard::XkbConfig::default(),
            REPEAT_DELAY.as_millis() as i32,
            (1000 / REPEAT_INTERVAL.as_millis()) as i32,
        )
        .unwrap();
        seat.add_pointer();
        seat
    }

    /// The one wl output; the real scale is set by `set_display_info`
    fn new_output(display_handle: &DisplayHandle) -> Output {
        let output = Output::new(
            String::from("sonamu"),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: String::from("Sonamu"),
                model: String::from("Sonamu"),
                serial_number: String::from("Unknown"),
            },
        );
        output.change_current_state(None, None, Some(Scale::Integer(1)), Some((0, 0).into()));
        output.create_global::<AppletRunner>(display_handle);
        output
    }

    pub fn new(event_handle: &LoopHandle<'static, AppletRunner>) -> Self {
        let start_time = std::time::Instant::now();

        let display: Display<AppletRunner> = Display::new().unwrap();
        let display_handle = display.handle();

        let compositor_state = CompositorState::new::<AppletRunner>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<AppletRunner>(&display_handle);
        let layer_shell_state = Self::new_layer_shell_state(&display_handle);
        let shm_state = ShmState::new::<AppletRunner>(&display_handle, Vec::new());
        let mut seat_state = SeatState::new();

        let seat = Self::new_seat(&mut seat_state, &display_handle);

        let output_manager_state =
            OutputManagerState::new_with_xdg_output::<AppletRunner>(&display_handle);
        let output = Self::new_output(&display_handle);

        let data_device_state = DataDeviceState::new::<AppletRunner>(&display_handle);
        let primary_selection_state = PrimarySelectionState::new::<AppletRunner>(&display_handle);
        // clipboard tools (wl-copy) use these, else they open a window to get focus (#113)
        let ext_data_control_state = ext_data_control::DataControlState::new::<AppletRunner, _>(
            &display_handle,
            Some(&primary_selection_state),
            |_| true,
        );
        let wlr_data_control_state = wlr_data_control::DataControlState::new::<AppletRunner, _>(
            &display_handle,
            Some(&primary_selection_state),
            |_| true,
        );
        let xdg_decoration_state = XdgDecorationState::new::<AppletRunner>(&display_handle);
        let idle_inhibit_state = IdleInhibitManagerState::new::<AppletRunner>(&display_handle);
        // smithay applies the viewport to the surface view, which rendering uses
        let viewporter_state = ViewporterState::new::<AppletRunner>(&display_handle);
        // delivered through `SeatHandler::cursor_image` as named cursors
        let cursor_shape_state = CursorShapeManagerState::new::<AppletRunner>(&display_handle);
        let fractional_scale_state =
            FractionalScaleManagerState::new::<AppletRunner>(&display_handle);
        let xdg_activation_state = XdgActivationState::new::<AppletRunner>(&display_handle);
        // smithay moves text input focus with keyboard focus
        let text_input_state = TextInputManagerState::new::<AppletRunner>(&display_handle);
        let pointer_constraints_state =
            PointerConstraintsState::new::<AppletRunner>(&display_handle);
        let relative_pointer_state =
            RelativePointerManagerState::new::<AppletRunner>(&display_handle);
        // pinches (#152) go to the surface under the pointer
        let pointer_gestures_state = PointerGesturesState::new::<AppletRunner>(&display_handle);
        let capture = capture::CaptureState::new(&display_handle);
        let clock = Clock::<Monotonic>::new();
        let presentation_state =
            PresentationState::new::<AppletRunner>(&display_handle, clock.id() as u32);

        let socket_name = Self::listen(event_handle);
        Self::insert_display(event_handle, display);

        let wl_request_queue = Self::get_event_queue(event_handle);

        Self {
            start_time,
            display_handle,
            compositor_state,
            xdg_shell_state,
            layer_shell_state,
            shm_state,
            seat_state,
            _output_manager_state: output_manager_state,
            data_device_state,
            primary_selection_state,
            ext_data_control_state,
            wlr_data_control_state,
            _xdg_decoration_state: xdg_decoration_state,
            _idle_inhibit_state: idle_inhibit_state,
            popup_manager: PopupManager::default(),
            _viewporter_state: viewporter_state,
            _presentation_state: presentation_state,
            clock,
            presented_frames: 0,
            cursor_status: CursorImageStatus::default_named(),
            _cursor_shape_state: cursor_shape_state,
            _fractional_scale_state: fractional_scale_state,
            xdg_activation_state,
            _text_input_state: text_input_state,
            _pointer_constraints_state: pointer_constraints_state,
            _relative_pointer_state: relative_pointer_state,
            _pointer_gestures_state: pointer_gestures_state,
            capture,
            dmabuf: dmabuf::DmabufImport::default(),
            event_handle: event_handle.clone(),
            pointer_focus: None,
            tile_areas: HashMap::new(),
            lock_end_position: None,
            dnd_icon: None,
            scale: 1.0,
            refresh_millihertz: None,
            seat,
            output,
            socket_name,
            surfaces: SlotMap::with_key(),
            wl_event_queues: SecondaryMap::new(),
            unregistered: Vec::new(),
            layer_surfaces: Vec::new(),
            floating: Vec::new(),
            floating_drag: None,
            layer_placements: Vec::new(),
            usable_area: Rectangle::default(),
            layer_focus: None,
            wl_target: None,
            screens: Vec::new(),
            fullscreen: None,
            pointer_on_layer: false,
            layer_buttons_down: 0,
            wl_request_queue,
        }
    }

    fn get_event_queue(
        event_handle: &LoopHandle<AppletRunner>,
    ) -> calloop::channel::Sender<(WlSurfaceId, WlSurfaceRequest)> {
        let (wl_request_queue_sender, wl_request_queue_reciever) = calloop::channel::channel();

        event_handle
            .insert_source(
                wl_request_queue_reciever,
                |event: calloop::channel::Event<(WlSurfaceId, WlSurfaceRequest)>,
                 &mut (),
                 state| {
                    let calloop::channel::Event::Msg((target_surface_id, wl_event)) = event else {
                        return;
                    };

                    // the surface may already be gone (client quit between the applet
                    // queueing this and us handling it)
                    let Some(target_surface) =
                        state.smithay_state.surfaces.get(target_surface_id).cloned()
                    else {
                        dbg!("Event sent to unknown surface");
                        return;
                    };

                    match wl_event {
                        WlSurfaceRequest::Close => {
                            if let Some(toplevel) = state.smithay_state.toplevel_of(&target_surface)
                            {
                                toplevel.send_close();
                            }
                        }
                        WlSurfaceRequest::Resize(new_size) => {
                            let Some(toplevel) = state.smithay_state.toplevel_of(&target_surface)
                            else {
                                dbg!("Resize sent to surface with no toplevel");
                                return;
                            };

                            let size = state.smithay_state.to_logical(new_size);
                            // kept for when it stops being fullscreen
                            match &mut state.smithay_state.fullscreen {
                                Some((surface_id, tile_size))
                                    if *surface_id == target_surface_id =>
                                {
                                    *tile_size = size;
                                }
                                _ => {
                                    toplevel.with_pending_state(|toplevel_state| {
                                        toplevel_state.size = Some(size);
                                    });
                                    toplevel.send_configure();
                                }
                            }
                        }
                        // `position` is already relative to the tile
                        WlSurfaceRequest::Mouse(mouse_event) => {
                            state.tile_mouse(&target_surface, mouse_event);
                        }
                    }

                    // Events queued above only land in wayland-server's internal
                    // per-client buffers; this source isn't triggered by client
                    // socket activity like the display dispatch source is, so we
                    // have to flush explicitly or the bytes never hit the wire.
                    state.smithay_state.display_handle.flush_clients().unwrap();
                },
            )
            .unwrap();

        wl_request_queue_sender
    }

    /// Every surface of the toplevel (root, subsurfaces, then popups, in stacking order)
    /// that has a buffer, with its offset from the window geometry (the tile) and its size.
    fn window_layers(root: &WlSurface) -> Vec<Layer> {
        // like niri, the window geometry (not the surface, which may have CSD shadows) is the tile
        let geometry_loc = window_geometry(root).unwrap_or_default().loc;
        let mut layers = Self::surface_tree_layers(root, Point::default() - geometry_loc);
        for layer in &mut layers {
            layer.in_root_tree = true;
        }

        // popup positions are relative to the root's window geometry
        for (popup, offset) in PopupManager::popups_for_surface(root) {
            let origin = offset - popup.geometry().loc;
            layers.extend(Self::surface_tree_layers(popup.wl_surface(), origin));
        }

        layers
    }

    /// Every surface in the tree (root and subsurfaces, in stacking order) that has a buffer,
    /// with its offset from `origin` and its size.
    fn surface_tree_layers(root: &WlSurface, origin: Point<i32, Logical>) -> Vec<Layer> {
        // `location` is the parent's; each surface's view offset is relative to its parent
        let view = |states: &SurfaceData| {
            states
                .data_map
                .get::<RendererSurfaceStateUserData>()
                .and_then(|surface_state| {
                    let surface_state = surface_state.lock().unwrap();
                    Some((
                        surface_state.view()?,
                        surface_state.buffer()?.clone(),
                        surface_state.buffer_scale(),
                        surface_state.current_commit(),
                    ))
                })
        };

        let mut layers = Vec::new();
        // upward: bottom to top
        with_surface_tree_upward(
            root,
            origin,
            |_, states, &location| match view(states) {
                Some((view, ..)) => TraversalAction::DoChildren(location + view.offset),
                // not mapped, so neither are its children
                None => TraversalAction::SkipChildren,
            },
            |surface, states, &location| {
                if let Some((view, buffer, buffer_scale, commit)) = view(states) {
                    layers.push(Layer {
                        surface: surface.clone(),
                        buffer,
                        buffer_scale,
                        commit,
                        location: location + view.offset,
                        view,
                        in_root_tree: false,
                    });
                }
            },
            |_, _, _| true,
        );
        layers
    }

    /// Each surface is drawn 1:1 at its offset from the tile's top-left.
    ///
    /// Returns `None` when the surface has nothing displayable yet
    /// (eg the initial xdg_surface commit carries no buffer by protocol)
    /// or when the surface id is stale.
    pub fn get_wl_surface_as_element(
        &self,
        surface_id: WlSurfaceId,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<UITree> {
        let surface = self.surfaces.get(surface_id)?;

        let mut layers = Self::window_layers(surface);
        let cursor = self.wl_cursor(&layers);

        // the pointer isn't focused on anything during a drag, so the icon goes in every tile
        if let Some((icon, offset)) = self.dnd_icon.as_ref().filter(|(icon, _)| icon.is_alive()) {
            let location = self
                .seat
                .get_pointer()
                .unwrap()
                .current_location()
                .to_i32_round();
            layers.extend(Self::surface_tree_layers(icon, location + *offset));
        }

        // like niri's `clip-to-geometry`: the root's surfaces (eg CSD shadows and borders) stay in the tile,
        // popups may leave it
        let clip = window_geometry(surface).map(|geometry| Rectangle::from_size(geometry.size));

        let textures = self.layers_to_textures(layers, clip, device, queue);

        (!textures.is_empty())
            .then(|| UITree::new(UIElement::Nothing, textures).with_cursor(cursor))
    }

    /// Imports the layers into absolutely placed textures; the root's layers are cut to `clip`
    fn layers_to_textures(
        &self,
        layers: Vec<Layer>,
        clip: Option<Rectangle<i32, Logical>>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Vec<UITree> {
        layers
            .into_iter()
            .filter_map(|mut layer| {
                if let Some(clip) = clip.filter(|_| layer.in_root_tree)
                    && !layer.clip_to(clip)
                {
                    return None;
                }
                let src = layer.view.src.to_buffer(
                    f64::from(layer.buffer_scale),
                    Transform::Normal,
                    &layer.view.src.size,
                );
                let texture = Self::cached_texture(&layer, src, device, queue)?;
                Some(UITree::leaf_styled(
                    UIElement::Texture(texture),
                    Style::absolute(
                        [
                            (f64::from(layer.location.x) * self.scale) as f32,
                            (f64::from(layer.location.y) * self.scale) as f32,
                        ],
                        [
                            (f64::from(layer.view.dst.w) * self.scale) as f32,
                            (f64::from(layer.view.dst.h) * self.scale) as f32,
                        ],
                    ),
                ))
            })
            .collect()
    }

    /// The client's cursor if the pointer is over one of `layers` (one toplevel's surfaces)
    fn wl_cursor(&self, layers: &[Layer]) -> Option<Cursor> {
        let focus = self.seat.get_pointer()?.current_focus()?;
        if !layers.iter().any(|layer| layer.surface == focus) {
            return None;
        }
        match &self.cursor_status {
            CursorImageStatus::Hidden => Some(Cursor::Hidden),
            CursorImageStatus::Named(icon) => Some(Cursor::Named(*icon)),
            CursorImageStatus::Surface(surface) if surface.is_alive() => {
                read_cursor_surface(surface, self.scale)
            }
            CursorImageStatus::Surface(_) => None,
        }
    }

    /// Whether `surface` is drawn as part of `toplevel` (itself, a subsurface, or a popup)
    fn is_in_window(toplevel: &WlSurface, surface: &WlSurface) -> bool {
        Self::window_layers(toplevel)
            .iter()
            .any(|layer| &layer.surface == surface)
    }

    /// The topmost surface of the toplevel under `point` (relative to the root) that takes input there,
    /// and that surface's offset from the root.
    /// (eg Firefox's content subsurfaces have an empty input region, so its root gets the clicks)
    fn surface_under(
        root: &WlSurface,
        point: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        Self::window_layers(root)
            .into_iter()
            .rev()
            .find(|layer| {
                let point = point - layer.location.to_f64();
                Rectangle::from_size(layer.view.dst)
                    .to_f64()
                    .contains(point)
                    && with_states(&layer.surface, |states| {
                        states
                            .cached_state
                            .get::<SurfaceAttributes>()
                            .current()
                            .input_region
                            .as_ref()
                            .is_none_or(|region| region.contains(point.to_i32_floor()))
                    })
            })
            .map(|layer| (layer.surface, layer.location.to_f64()))
    }

    /// The layer's texture, imported again only after a commit or a new crop (#136)
    fn cached_texture(
        layer: &Layer,
        src: Rectangle<f64, BufferCoords>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<TextureView> {
        let key = ((*layer.buffer).clone(), layer.commit, src);
        with_states(&layer.surface, |states| {
            let cache = states.data_map.get_or_insert(TextureCache::default);
            let mut cache = cache.0.lock().unwrap();
            if let Some((_, texture)) = cache.as_ref().filter(|(cached, _)| *cached == key) {
                return Some(texture.clone());
            }
            let texture = Self::import_buffer(&layer.buffer, src, device, queue)?;
            *cache = Some((key, texture.clone()));
            Some(texture)
        })
    }

    /// Only the `src` part of the buffer is imported (viewporter crop).
    ///
    /// TODO: buffer transforms (eg rotated buffers) aren't handled
    fn import_buffer(
        wl_buffer: &Buffer,
        src: Rectangle<f64, BufferCoords>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<TextureView> {
        match buffer_type(wl_buffer) {
            Some(BufferType::Shm) => {
                // NOTE: everything touching the mapped pool has to stay inside this
                // closure: the pointer is only valid while it runs, and smithay only
                // installs its SIGBUS guard for that duration.
                smithay::wayland::shm::with_buffer_contents(
                    wl_buffer,
                    |content_ptr, length, metadata| {
                        // SAFETY: valid for the body of this closure only. The client
                        // may mutate the pool concurrently, so don't hold this anywhere.
                        let data = unsafe { std::slice::from_raw_parts(content_ptr, length) };

                        let (left, top, texture_size) = crop(src, metadata.width, metadata.height)?;

                        use smithay::reexports::wayland_server::protocol::wl_shm::Format;
                        let format = match metadata.format {
                            // Both are little-endian 0xAARRGGBB / 0xXXRRGGBB,
                            // ie B,G,R,A in byte order. Xrgb's X byte is ignored
                            // by the shader, so the two map to the same wgpu format.
                            Format::Argb8888 | Format::Xrgb8888 => {
                                wgpu::TextureFormat::Bgra8UnormSrgb
                            }
                            unsupported => {
                                // log::warn!("unsupported wl_shm format: {unsupported:?}");
                                dbg!("unsupported wl_shm format:", unsupported);
                                return None;
                            }
                        };

                        let texture = device.create_texture(&wgpu::TextureDescriptor {
                            size: texture_size,
                            mip_level_count: mipmaps::level_count(texture_size),
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format,
                            // COPY_DST means that we want to copy data to this texture
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_DST
                                | mipmaps::USAGE,
                            label: Some("wayland_buffer_texture"),
                            view_formats: &[],
                        });

                        queue.write_texture(
                            // Tells wgpu where to copy the pixel data
                            wgpu::TexelCopyTextureInfo {
                                texture: &texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            // The actual pixel data
                            // NOTE: this is the whole pool; `offset` below picks
                            // out this buffer's region of it.
                            data,
                            // The layout of the texture
                            wgpu::TexelCopyBufferLayout {
                                // both supported formats are 4 bytes per pixel
                                offset: u64::from(
                                    metadata.offset.cast_unsigned()
                                        + top * metadata.stride.cast_unsigned()
                                        + left * 4,
                                ),
                                bytes_per_row: Some(metadata.stride.cast_unsigned()),
                                rows_per_image: Some(texture_size.height),
                            },
                            texture_size,
                        );
                        mipmaps::generate(device, queue, &texture);

                        Some(texture.create_view(&TextureViewDescriptor::default()))
                    },
                )
                .ok()
                .flatten()
            }
            Some(BufferType::Dma) => {
                let dmabuf = smithay::wayland::dmabuf::get_dmabuf(wl_buffer).ok()?;
                let size = dmabuf.size();
                let (left, top, texture_size) = crop(src, size.w, size.h)?;
                let imported = dmabuf::import(device, dmabuf)?;

                // a copy, so the client can reuse the buffer once it's released
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    size: texture_size,
                    mip_level_count: mipmaps::level_count(texture_size),
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: dmabuf::WGPU_FORMAT,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_DST
                        | mipmaps::USAGE,
                    label: Some("wayland_dmabuf_texture"),
                    view_formats: &[],
                });
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &imported,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: left,
                            y: top,
                            z: 0,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    texture.as_image_copy(),
                    texture_size,
                );
                queue.submit([encoder.finish()]);
                mipmaps::generate(device, queue, &texture);
                Some(texture.create_view(&TextureViewDescriptor::default()))
            }
            // not advertised, so clients shouldn't send these; draw nothing instead of panicking
            unsupported => {
                dbg!("Unsupported buffer type:", unsupported);
                None
            }
        }
    }
}

/// NOTE: Disclosure: from by GitHub Copilot
/// Fixes the Alacritty problem.
/// `src` kept inside a `width`x`height` buffer, as its top-left and size; `None` if empty
fn crop(
    src: Rectangle<f64, BufferCoords>,
    width: i32,
    height: i32,
) -> Option<(u32, u32, wgpu::Extent3d)> {
    let left = (src.loc.x.floor() as i32).clamp(0, width);
    let top = (src.loc.y.floor() as i32).clamp(0, height);
    let right = ((src.loc.x + src.size.w).ceil() as i32).clamp(left, width);
    let bottom = ((src.loc.y + src.size.h).ceil() as i32).clamp(top, height);
    if right == left || bottom == top {
        return None;
    }
    Some((
        left.cast_unsigned(),
        top.cast_unsigned(),
        wgpu::Extent3d {
            width: (right - left).cast_unsigned(),
            height: (bottom - top).cast_unsigned(),
            // All textures are stored as 3D
            depth_or_array_layers: 1,
        },
    ))
}

/// A surface drawn as part of a toplevel
struct Layer {
    surface: WlSurface,
    buffer: Buffer,
    buffer_scale: i32,
    /// Of the buffer's damage, to reuse its texture
    commit: CommitCounter,
    /// Offset from the root
    location: Point<i32, Logical>,
    /// The crop (`src`) and drawn size (`dst`), from viewporter or the buffer
    view: SurfaceView,
    /// Part of the root's surface tree (not a popup)
    in_root_tree: bool,
}

/// A surface's last imported texture, by the buffer, commit and crop it was imported for
/// (a new buffer committed without damage doesn't change the commit counter)
type TextureKey = (
    smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer,
    CommitCounter,
    Rectangle<f64, BufferCoords>,
);
#[derive(Default)]
struct TextureCache(std::sync::Mutex<Option<(TextureKey, TextureView)>>);

impl Layer {
    /// Crops the layer to `clip` (same coordinates as `location`).
    /// Returns false if nothing is left.
    fn clip_to(&mut self, clip: Rectangle<i32, Logical>) -> bool {
        let Some(visible) = Rectangle::new(self.location, self.view.dst).intersection(clip) else {
            return false;
        };
        // src units per dst unit
        let [sx, sy] = [
            self.view.src.size.w / f64::from(self.view.dst.w),
            self.view.src.size.h / f64::from(self.view.dst.h),
        ];
        let cut = visible.loc - self.location;
        self.view.src = Rectangle::new(
            self.view.src.loc + Point::from((f64::from(cut.x) * sx, f64::from(cut.y) * sy)),
            (
                f64::from(visible.size.w) * sx,
                f64::from(visible.size.h) * sy,
            )
                .into(),
        );
        self.view.dst = visible.size;
        self.location = visible.loc;
        true
    }
}

/// The xdg window geometry in the surface (eg inside client-side shadows), if the client set one
fn window_geometry(surface: &WlSurface) -> Option<Rectangle<i32, Logical>> {
    with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
    })
}

/// Copies a cursor surface's shm buffer into a custom cursor.
/// Its subsurfaces are ignored. `scale` is the screen's (screen px per logical px).
fn read_cursor_surface(surface: &WlSurface, scale: f64) -> Option<Cursor> {
    let (buffer, buffer_scale, hotspot) = with_states(surface, |states| {
        let (buffer, buffer_scale) = {
            let surface_state = states
                .data_map
                .get::<RendererSurfaceStateUserData>()?
                .lock()
                .unwrap();
            (
                surface_state.buffer()?.clone(),
                surface_state.buffer_scale(),
            )
        };
        let hotspot = states
            .data_map
            .get::<CursorImageSurfaceData>()?
            .lock()
            .unwrap()
            .hotspot;
        Some((buffer, buffer_scale, hotspot))
    })?;
    // the hotspot is logical, the image is in buffer px
    let hotspot = hotspot.upscale(buffer_scale);

    smithay::wayland::shm::with_buffer_contents(&buffer, |content_ptr, length, metadata| {
        // SAFETY: same as in `SmithayState::import_buffer`
        let data = unsafe { std::slice::from_raw_parts(content_ptr, length) };

        use smithay::reexports::wayland_server::protocol::wl_shm::Format;
        let has_alpha = match metadata.format {
            Format::Argb8888 => true,
            Format::Xrgb8888 => false,
            _ => return None,
        };

        let width = usize::try_from(metadata.width).ok()?;
        let height = usize::try_from(metadata.height).ok()?;
        let mut rgba = Vec::with_capacity(width * height * 4);
        for row in 0..height {
            let start = usize::try_from(metadata.offset + metadata.stride * row as i32).ok()?;
            // little-endian ARGB is B,G,R,A in bytes, and premultiplied
            for &[b, g, r, a] in data.get(start..start + width * 4)?.as_chunks::<4>().0 {
                let a = if has_alpha { a } else { u8::MAX };
                let unpremultiply = |channel: u8| {
                    let value = (u16::from(channel) * 255)
                        .checked_div(u16::from(a))
                        .unwrap_or(0);
                    // clamp, in case the client's color is brighter than its alpha allows
                    u8::try_from(value).unwrap_or(u8::MAX)
                };
                rgba.extend([unpremultiply(r), unpremultiply(g), unpremultiply(b), a]);
            }
        }

        Some(Cursor::Custom {
            rgba: rgba.into(),
            width: u16::try_from(width).ok()?,
            height: u16::try_from(height).ok()?,
            hotspot: [
                u16::try_from(hotspot.x).ok()?,
                u16::try_from(hotspot.y).ok()?,
            ],
            scale: (f64::from(buffer_scale) / scale) as f32,
        })
    })
    .ok()
    .flatten()
}

/// What a presented frame tells clients
struct Presentation<'a> {
    /// For frame callbacks
    frame_time: u32,
    output: &'a Output,
    time: Time<Monotonic>,
    refresh: Refresh,
    /// The presented frame count
    seq: u64,
}

/// See 2026-09-04 DEVLOG
/// I think this is just going through the entire tree and telling all the frame callbacks that the request was done
///
/// Also sends presentation feedback.
fn send_frames_surface_tree(surface: &WlSurface, presentation: &Presentation) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_surf, states, &()| {
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(presentation.frame_time);
            }

            for feedback in states
                .cached_state
                .get::<PresentationFeedbackCachedState>()
                .current()
                .callbacks
                .drain(..)
            {
                // the time isn't from the hardware, so no flags
                feedback.presented(
                    presentation.output,
                    presentation.time,
                    presentation.refresh,
                    presentation.seq,
                    PresentationFeedbackKind::empty(),
                );
            }
        },
        |_, _, &()| true,
    );
}

smithay::delegate_dispatch2!(AppletRunner);
// smithay::delegate_dispatch2!(SmithayState);

impl AppletRunner {
    /// Sends to whichever surface has keyboard focus
    fn wl_key_input(&mut self, keycode: u32, key_state: KeyState) {
        self.smithay_state
            .seat
            .get_keyboard()
            .unwrap()
            .input::<(), _>(
                self,
                Keycode::new(keycode),
                key_state,
                SERIAL_COUNTER.next_serial(),
                self.smithay_state.now_input_time(),
                |_, _, _| FilterResult::Forward,
            );
    }

    /// Repeats of a held key are dropped, since clients repeat keys themselves
    pub(crate) fn press_wl_key(&mut self, keycode: u32) {
        let keyboard = self.smithay_state.seat.get_keyboard().unwrap();
        if !keyboard.pressed_keys().contains(&Keycode::new(keycode)) {
            self.wl_key_input(keycode, KeyState::Pressed);
        }
    }

    /// Called for every release, even if sonamu focus is elsewhere,
    /// otherwise the client (and smithay) think the key is still held.
    /// Does nothing if the key isn't pressed, so it is fine to call more than once.
    pub(crate) fn release_wl_key(&mut self, keycode: u32) {
        let keyboard = self.smithay_state.seat.get_keyboard().unwrap();
        if keyboard.pressed_keys().contains(&Keycode::new(keycode)) {
            self.wl_key_input(keycode, KeyState::Released);
        }
    }

    /// Tells the root about a new toplevel (or floats it) once it maps (commits a buffer): its app id is set
    /// (it is matched by pid and app id to a restored tile), and so are its parent and size limits (GTK 3 sets
    /// a fixed size only then)
    fn register_toplevel(&mut self, surface: &WlSurface) {
        let state = &mut self.smithay_state;
        let Some(index) = state
            .unregistered
            .iter()
            .position(|&id| state.surfaces.get(id) == Some(surface))
        else {
            return;
        };
        let mapped = with_states(surface, |states| {
            states
                .data_map
                .get::<RendererSurfaceStateUserData>()
                .is_some_and(|data| data.lock().unwrap().buffer().is_some())
        });
        if !mapped {
            return;
        }
        let surface_id = state.unregistered.remove(index);
        if let Some(toplevel) = state
            .toplevel_of(surface)
            .filter(floating::floats)
        {
            state.surfaces.remove(surface_id);
            self.float(toplevel);
            return;
        }
        let (app_id, title) = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .map(|data| {
                    let data = data.lock().unwrap();
                    (data.app_id.clone(), data.title.clone())
                })
                .unwrap_or_default()
        });
        let pid = state
            .display_handle
            .get_client(surface.id())
            .ok()
            .and_then(|client| client.get_credentials(&state.display_handle).ok())
            .and_then(|credentials| u32::try_from(credentials.pid).ok());
        let (wl_event_sender, wl_event_queue) = calloop::channel::channel();
        // set before, when no applet could be told; waits in the queue until the applet listens
        if let Some(title) = title {
            let _ = wl_event_sender.send(WlSurfaceEvent::Title(title));
        }
        state.wl_event_queues.insert(surface_id, wl_event_sender);
        self.root_client
            .send_event(StandardEvent::WlSurfaceRegistered(NewWlSurface {
                surface_id,
                wl_request_queue: state.wl_request_queue.clone(),
                wl_event_queue,
                pid,
                app_id,
            }));
    }
}

impl SeatHandler for AppletRunner {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.smithay_state.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        // applied to the pointer focus's tile on redraw
        self.smithay_state.cursor_status = image;
        self.needs_redraw = true;
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        // selections are only offered to the focused client
        let display_handle = &self.smithay_state.display_handle;
        let client = focused.and_then(|surface| display_handle.get_client(surface.id()).ok());
        set_data_device_focus(display_handle, seat, client.clone());
        set_primary_focus(display_handle, seat, client);

        // like other compositors, only the keyboard focus's toplevel is activated
        // (eg Firefox only shows its doorhangers in an active window, #77)
        let focused = focused.map(|surface| {
            self.smithay_state
                .popup_manager
                .find_popup(surface)
                .and_then(|popup| find_popup_root_surface(&popup).ok())
                .unwrap_or_else(|| surface.clone())
        });
        for toplevel in self.smithay_state.xdg_shell_state.toplevel_surfaces() {
            let activated = focused.as_ref() == Some(toplevel.wl_surface());
            toplevel.with_pending_state(|state| {
                use wayland_protocols::xdg::shell::server::xdg_toplevel::State;
                if activated {
                    state.states.set(State::Activated);
                } else {
                    state.states.unset(State::Activated);
                }
            });
            if toplevel.is_initial_configure_sent() {
                toplevel.send_pending_configure();
            }
        }
    }
}
impl CompositorHandler for AppletRunner {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.smithay_state.compositor_state
    }

    /// Like niri: a commit with a dmabuf waits until the client's GPU work on it is done
    fn new_surface(&mut self, surface: &WlSurface) {
        add_pre_commit_hook::<Self, _>(surface, |runner, _, surface| {
            let dmabuf = with_states(surface, |states| {
                match states
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .pending()
                    .buffer
                    .as_ref()
                {
                    Some(BufferAssignment::NewBuffer(buffer)) => {
                        smithay::wayland::dmabuf::get_dmabuf(buffer).cloned().ok()
                    }
                    _ => None,
                }
            });
            let Some((blocker, source)) =
                dmabuf.and_then(|dmabuf| dmabuf.generate_blocker(calloop::Interest::READ).ok())
            else {
                return;
            };
            let Some(client) = surface.client() else {
                return;
            };
            let inserted =
                runner
                    .smithay_state
                    .event_handle
                    .insert_source(source, move |(), _, runner| {
                        let display_handle = runner.smithay_state.display_handle.clone();
                        runner
                            .client_compositor_state(&client)
                            .blocker_cleared(runner, &display_handle);
                        Ok(())
                    });
            if inserted.is_ok() {
                add_blocker(surface, blocker);
            }
        });
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        // dbg!("Committed");
        on_commit_buffer_handler::<Self>(surface);
        self.register_toplevel(surface);
        self.layer_surface_committed(surface);

        // covers new subsurfaces, popups and cursors; does nothing if already sent
        self.smithay_state.send_scale(surface);

        // the drag icon moves by its attach offset, like anvil
        if let Some((icon, offset)) = &mut self.smithay_state.dnd_icon
            && icon == surface
        {
            *offset += with_states(icon, |states| {
                states
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .current()
                    .buffer_delta
                    .take()
                    .unwrap_or_default()
            });
        }

        // the cursor's hotspot moves against its attach offset, like anvil
        if matches!(&self.smithay_state.cursor_status, CursorImageStatus::Surface(cursor) if cursor == surface)
        {
            with_states(surface, |states| {
                let Some(cursor) = states.data_map.get::<CursorImageSurfaceData>() else {
                    return;
                };
                if let Some(delta) = states
                    .cached_state
                    .get::<SurfaceAttributes>()
                    .current()
                    .buffer_delta
                    .take()
                {
                    cursor.lock().unwrap().hotspot -= delta;
                }
            });
        }

        self.smithay_state.popup_manager.commit(surface);
        if let Some(PopupKind::Xdg(popup)) = self.smithay_state.popup_manager.find_popup(surface)
            && !popup.is_initial_configure_sent()
        {
            // the initial configure is always allowed, so this can't fail
            popup.send_configure().expect("initial configure failed");
        }

        // frame callbacks are sent after the redraw is presented
        self.needs_redraw = true;
    }
}

impl BufferHandler for AppletRunner {
    fn buffer_destroyed(
        &mut self,
        _buffer: &smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer,
    ) {
    }
}

impl ShmHandler for AppletRunner {
    fn shm_state(&self) -> &ShmState {
        &self.smithay_state.shm_state
    }
}

/// Like niri for tiled windows: tells CSD clients (eg GTK) to drop shadows and rounded corners
const TILED: [xdg_toplevel::State; 4] = [
    xdg_toplevel::State::TiledLeft,
    xdg_toplevel::State::TiledRight,
    xdg_toplevel::State::TiledTop,
    xdg_toplevel::State::TiledBottom,
];

/// Tells the toplevel it is fullscreen, or tiled, at `size`
fn configure_fullscreen(toplevel: &ToplevelSurface, fullscreen: bool, size: Size<i32, Logical>) {
    toplevel.with_pending_state(|state| {
        state.size = Some(size);
        if fullscreen {
            state.states.set(xdg_toplevel::State::Fullscreen);
        } else {
            state.states.unset(xdg_toplevel::State::Fullscreen);
        }
        for tiled in TILED {
            if fullscreen {
                state.states.unset(tiled);
            } else {
                state.states.set(tiled);
            }
        }
    });
    toplevel.send_configure();
}

impl XdgShellHandler for AppletRunner {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.smithay_state.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        dbg!("Handling new toplevel surface");
        surface.with_pending_state(|state| {
            state.size = Some((800, 600).into());
            for tiled in TILED {
                state.states.set(tiled);
            }
            // else clients may hide their fullscreen buttons
            state
                .capabilities
                .set(xdg_toplevel::WmCapabilities::Fullscreen);
        });
        surface.send_configure();
        self.smithay_state.output.enter(surface.wl_surface());

        // keyboard focus is given once Sonamu focus reaches the new tile (`StandardRequest::WlKeyboardFocus`)

        // add surface to list of surfaces; the root is told once it maps (`register_toplevel`)
        let surface_id = self
            .smithay_state
            .surfaces
            .insert(surface.wl_surface().clone());
        self.smithay_state.unregistered.push(surface_id);
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        let wl_surface = surface.wl_surface();
        let Some(surface_id) = self
            .smithay_state
            .surfaces
            .iter()
            .find_map(|(id, registered)| (registered == wl_surface).then_some(id))
        else {
            return;
        };
        let title = with_states(wl_surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().unwrap().title.clone())
        });
        if let Some(title) = title {
            self.smithay_state
                .send_wl_event(surface_id, WlSurfaceEvent::Title(title));
        }
    }

    fn fullscreen_request(&mut self, surface: ToplevelSurface, _output: Option<WlOutput>) {
        // there is only one output
        self.set_fullscreen(&surface);
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        let surface_id = self.smithay_state.surface_id_of(surface.wl_surface());
        if surface_id.is_some() && surface_id == self.smithay_state.fullscreen() {
            self.unset_fullscreen();
        } else {
            surface.send_configure();
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        if self.floating_destroyed(&surface) {
            return;
        }
        // Without this the slotmap keeps a dead `WlSurface` forever
        let wl_surface = surface.wl_surface();
        let Some(surface_id) = self
            .smithay_state
            .surfaces
            .iter()
            .find_map(|(id, registered)| (registered == wl_surface).then_some(id))
        else {
            return;
        };
        self.smithay_state.surfaces.remove(surface_id);
        if self.smithay_state.fullscreen() == Some(surface_id) {
            self.smithay_state.fullscreen = None;
        }
        let wl_event_queue = self.smithay_state.wl_event_queues.remove(surface_id);
        // the root never heard of it
        if let Some(index) = self
            .smithay_state
            .unregistered
            .iter()
            .position(|&id| id == surface_id)
        {
            self.smithay_state.unregistered.remove(index);
            return;
        }

        // its tile may be plucked, out of the root's tree
        if let Some(plucked) = &mut self.plucked
            && plucked.remove_wl_surface(surface_id)
        {
            if plucked.main.is_none() && plucked.children.is_empty() {
                self.plucked = None;
            }
        } else {
            // the owning applet closes its tab
            if let Some(queue) = wl_event_queue {
                let _ = queue.send(WlSurfaceEvent::Destroyed);
            }
        }

        self.needs_redraw = true;
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        // the initial configure is sent on the popup's first commit
        self.smithay_state.unconstrain_popup(&surface);

        if let Err(err) = self
            .smithay_state
            .popup_manager
            .track_popup(PopupKind::from(surface))
        {
            dbg!("Failed to track popup:", err);
        }
    }

    fn popup_destroyed(&mut self, _surface: PopupSurface) {
        self.smithay_state.popup_manager.cleanup();
        self.needs_redraw = true;
    }

    /// Same as anvil
    fn grab(&mut self, surface: PopupSurface, seat: WlSeat, serial: Serial) {
        let seat: Seat<Self> = Seat::from_resource(&seat).unwrap();
        let kind = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Ok(mut grab) = self
            .smithay_state
            .popup_manager
            .grab_popup(root, kind, &seat, serial)
        else {
            return;
        };

        if let Some(keyboard) = seat.get_keyboard() {
            if keyboard.is_grabbed()
                && !(keyboard.has_grab(serial)
                    || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            if pointer.is_grabbed()
                && !(pointer.has_grab(serial)
                    || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.smithay_state.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }
}

impl OutputHandler for AppletRunner {}

impl SelectionHandler for AppletRunner {
    /// The text a native applet copied (`SmithayState::set_clipboard`)
    type SelectionUserData = Arc<String>;

    fn send_selection(
        &mut self,
        _ty: SelectionTarget,
        _mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        text: &Arc<String>,
    ) {
        write_text(fd, Arc::clone(text));
    }
}

/// Text mime types, preferred first
const TEXT_MIME_TYPES: [&str; 4] = [
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "STRING",
];

/// On another thread: a reader that is slow (or never reads) can't block the loop
fn write_text(fd: OwnedFd, text: Arc<String>) {
    std::thread::spawn(move || {
        use std::io::Write as _;
        let _ = std::fs::File::from(fd).write_all(text.as_bytes());
    });
}

impl DataDeviceHandler for AppletRunner {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.smithay_state.data_device_state
    }
}

/// Same as anvil
impl WaylandDndGrabHandler for AppletRunner {
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        icon: Option<WlSurface>,
        seat: Seat<Self>,
        serial: Serial,
        type_: GrabType,
    ) {
        self.smithay_state.dnd_icon = icon.map(|icon| (icon, Point::from((0, 0))));

        match type_ {
            GrabType::Pointer => {
                let pointer = seat.get_pointer().unwrap();
                let Some(start_data) = pointer.grab_start_data() else {
                    source.cancel();
                    return;
                };
                pointer.set_grab(
                    self,
                    DnDGrab::new_pointer(
                        &self.smithay_state.display_handle,
                        start_data,
                        source,
                        seat,
                    ),
                    serial,
                    Focus::Keep,
                );
            }
            // there is no touch
            GrabType::Touch => source.cancel(),
        }
    }
}

impl DndGrabHandler for AppletRunner {
    fn dropped(
        &mut self,
        _target: Option<DndTarget<'_, Self>>,
        _validated: bool,
        _seat: Seat<Self>,
        _location: Point<f64, Logical>,
    ) {
        self.smithay_state.dnd_icon = None;
        self.needs_redraw = true;
    }

    fn cancelled(&mut self, _seat: Seat<Self>, _location: Point<f64, Logical>) {
        self.smithay_state.dnd_icon = None;
        self.needs_redraw = true;
    }
}

impl PrimarySelectionHandler for AppletRunner {
    fn primary_selection_state(&mut self) -> &mut PrimarySelectionState {
        &mut self.smithay_state.primary_selection_state
    }
}

impl ext_data_control::DataControlHandler for AppletRunner {
    fn data_control_state(&mut self) -> &mut ext_data_control::DataControlState {
        &mut self.smithay_state.ext_data_control_state
    }
}

impl wlr_data_control::DataControlHandler for AppletRunner {
    fn data_control_state(&mut self) -> &mut wlr_data_control::DataControlState {
        &mut self.smithay_state.wlr_data_control_state
    }
}

/// Sonamu draws the borders, so always tell clients to not draw decorations
fn use_server_side_decoration(toplevel: &ToplevelSurface) {
    toplevel.with_pending_state(|state| {
        state.decoration_mode = Some(DecorationMode::ServerSide);
    });
    if toplevel.is_initial_configure_sent() {
        toplevel.send_pending_configure();
    }
}
impl XdgDecorationHandler for AppletRunner {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        use_server_side_decoration(&toplevel);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: DecorationMode) {
        use_server_side_decoration(&toplevel);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        use_server_side_decoration(&toplevel);
    }
}

/// Sonamu has no idle handling yet, so there is nothing to inhibit
impl IdleInhibitHandler for AppletRunner {
    fn inhibit(&mut self, _surface: WlSurface) {}

    fn uninhibit(&mut self, _surface: WlSurface) {}
}

impl PointerConstraintsHandler for AppletRunner {
    fn new_constraint(&mut self, _surface: &WlSurface, _pointer: &PointerHandle<Self>) {
        self.smithay_state.maybe_activate_pointer_constraint();
    }

    /// The window's grab is released by `AppletRunner::update_pointer_grab`
    fn remove_constraint(
        &mut self,
        _surface: &WlSurface,
        _pointer: &PointerHandle<Self>,
        _remove: smithay::wayland::pointer_constraints::ConstraintRemove,
    ) {
    }

    /// Where the pointer goes when the lock ends, like anvil.
    /// Converted to window px now, while the surface's position is known.
    fn cursor_position_hint(
        &mut self,
        surface: &WlSurface,
        pointer: &PointerHandle<Self>,
        location: Point<f64, Logical>,
    ) {
        let state = &self.smithay_state;
        if !with_pointer_constraint(surface, pointer, |constraint| {
            constraint.is_some_and(|constraint| constraint.is_active())
        }) {
            return;
        }
        let Some((_, origin)) = state
            .pointer_focus
            .as_ref()
            .filter(|(focus, _)| focus == surface)
        else {
            return;
        };
        // the pointer focus is in the visible tile; find its toplevel's tile
        let Some(DisplayAreaPx([[left, top], _])) =
            state.surfaces.iter().find_map(|(surface_id, toplevel)| {
                state
                    .tile_areas
                    .get(&surface_id)
                    .filter(|_| SmithayState::is_in_window(toplevel, surface))
            })
        else {
            return;
        };

        let in_tile = (*origin + location).to_physical(state.scale);
        self.smithay_state.lock_end_position =
            Some([f64::from(*left) + in_tile.x, f64::from(*top) + in_tile.y]);
    }
}

impl AppletRunner {
    /// Follows Sonamu focus; `None` (or a stale id) clears keyboard focus
    pub(crate) fn focus_wl_keyboard(&mut self, surface_id: Option<WlSurfaceId>) {
        self.smithay_state.wl_target = surface_id;
        // Sonamu focus left the fullscreen tile (eg traversing, so the treeview shows),
        // other than by the pointer going to another screen (#60)
        if self.smithay_state.fullscreen().is_some()
            && self.smithay_state.fullscreen() != surface_id
            && self.on_fullscreen_screen(self.pointer_position)
        {
            self.unset_fullscreen();
        }
        // a layer surface that has the keyboard keeps it
        if let Some(layer) = self.smithay_state.keyboard_layer() {
            self.focus_wl_keyboard_on(layer);
            return;
        }
        if let Some(surface) =
            surface_id.and_then(|surface_id| self.smithay_state.surfaces.get(surface_id).cloned())
        {
            self.focus_wl_keyboard_on(surface);
        } else {
            let keyboard = self.smithay_state.seat.get_keyboard().unwrap();
            if keyboard.current_focus().is_some() {
                // also ends any popup grab
                keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
            }

            // the pointer leaves too, which ends any pointer lock (and so the window's grab)
            if self.smithay_state.pointer_focus.take().is_some() {
                let pointer = self.smithay_state.seat.get_pointer().unwrap();
                let location = pointer.current_location();
                pointer.motion(
                    self,
                    None,
                    &MotionEvent {
                        location,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: self.smithay_state.now_input_time(),
                    },
                );
                pointer.frame(self);
            }
        }
    }

    /// Focuses the toplevel, unless it or one of its popups already is
    /// (moving focus off a popup would end its grab)
    fn focus_wl_keyboard_on(&mut self, toplevel: WlSurface) {
        let toplevel = self.smithay_state.keyboard_layer().unwrap_or(toplevel);
        let keyboard = self.smithay_state.seat.get_keyboard().unwrap();
        let focus_in_toplevel = keyboard.current_focus().is_some_and(|focus| {
            focus == toplevel
                || self
                    .smithay_state
                    .popup_manager
                    .find_popup(&focus)
                    .and_then(|popup| find_popup_root_surface(&popup).ok())
                    .is_some_and(|root| root == toplevel)
        });
        if !focus_in_toplevel {
            keyboard.set_focus(self, Some(toplevel), SERIAL_COUNTER.next_serial());
        }
    }

    /// Makes the toplevel fullscreen at its screen's size (ending any other's), and focuses its tile
    fn set_fullscreen(&mut self, toplevel: &ToplevelSurface) {
        let state = &mut self.smithay_state;
        let Some(surface_id) = state.surface_id_of(toplevel.wl_surface()) else {
            toplevel.send_configure();
            return;
        };
        let size = state.screen_size(state.screen_of_tile(surface_id));
        if state.fullscreen() != Some(surface_id) {
            self.unset_fullscreen();
            let state = &mut self.smithay_state;
            // the tile's size, from its last configure
            let tile_size = toplevel
                .with_pending_state(|state| state.size)
                .unwrap_or(size);
            state.fullscreen = Some((surface_id, tile_size));
            if state.wl_target != Some(surface_id) {
                state.send_wl_event(surface_id, WlSurfaceEvent::Focus);
            }
        }
        configure_fullscreen(toplevel, true, size);
        self.needs_redraw = true;
    }

    /// The fullscreen toplevel goes back to its tile
    fn unset_fullscreen(&mut self) {
        let state = &mut self.smithay_state;
        let Some((surface_id, tile_size)) = state.fullscreen.take() else {
            return;
        };
        if let Some(toplevel) = state
            .surfaces
            .get(surface_id)
            .and_then(|surface| state.toplevel_of(surface))
        {
            configure_fullscreen(&toplevel, false, tile_size);
        }
        self.needs_redraw = true;
    }

    /// Sends the mouse event (in px from the tile's corner) to the toplevel's surface under it
    fn tile_mouse(
        &mut self,
        toplevel: &WlSurface,
        MouseEvent {
            position,
            kind,
            time_msec,
        }: MouseEvent,
    ) {
        // Motion first so the pointer is focused on the surface under it
        let location = Point::<f64, Physical>::from((position[0], position[1]))
            .to_logical(self.smithay_state.scale);
        let focus = SmithayState::surface_under(toplevel, location);
        self.send_wl_mouse(location, focus, kind, time_msec);
    }

    /// Sends the mouse event (in window px) to the fullscreen toplevel, which is over the root on its screen.
    /// Returns false if there is none, or the pointer is on another screen.
    pub(crate) fn fullscreen_mouse(&mut self, mut event: MouseEvent) -> bool {
        let (Some(surface), Some([x, y, ..])) = (
            self.smithay_state
                .fullscreen()
                .and_then(|surface_id| self.smithay_state.surfaces.get(surface_id).cloned()),
            self.smithay_state.fullscreen_screen(),
        ) else {
            return false;
        };
        if !self.on_fullscreen_screen(event.position) {
            return false;
        }
        let [pointer_x, pointer_y] = event.position;
        event.position = [pointer_x - f64::from(x), pointer_y - f64::from(y)];
        self.tile_mouse(&surface, event);
        true
    }

    /// Whether `position` (window px) is on the fullscreen toplevel's screen
    pub(crate) fn on_fullscreen_screen(&self, [pointer_x, pointer_y]: [f64; 2]) -> bool {
        self.smithay_state
            .fullscreen_screen()
            .is_some_and(|[x, y, width, height]| {
                let [x, y, width, height] = [x, y, width, height].map(f64::from);
                (x..x + width).contains(&pointer_x) && (y..y + height).contains(&pointer_y)
            })
    }

    /// Moves the pointer to `location` (focused on `focus` and its origin, same coordinates), then sends the event
    pub(crate) fn send_wl_mouse(
        &mut self,
        location: Point<f64, Logical>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        kind: MouseEventKind,
        time_msec: Option<u32>,
    ) {
        let pointer = self.smithay_state.seat.get_pointer().unwrap();
        let time = time_msec.map_or_else(
            || self.smithay_state.now_input_time(),
            InputTime::from_millis,
        );

        // like niri, only real motion reaches clients (not one before every button/scroll)
        let moved = kind == MouseEventKind::Motion
            || pointer.current_location() != location
            || pointer.current_focus().as_ref() != focus.as_ref().map(|(surface, _)| surface);
        self.smithay_state.pointer_focus.clone_from(&focus);
        if moved {
            pointer.motion(
                self,
                focus,
                &MotionEvent {
                    location,
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                },
            );
        }
        self.smithay_state.maybe_activate_pointer_constraint();
        match kind {
            MouseEventKind::Motion => {}
            MouseEventKind::Button { button, pressed } => {
                pointer.button(
                    self,
                    &ButtonEvent {
                        serial: SERIAL_COUNTER.next_serial(),
                        time,
                        button,
                        state: if pressed {
                            ButtonState::Pressed
                        } else {
                            ButtonState::Released
                        },
                    },
                );
            }
            MouseEventKind::Scroll {
                delta: [dx, dy],
                v120,
                stop: [stop_x, stop_y],
                ..
            } => {
                let mut frame = AxisFrame::new(time).source(if v120.is_some() {
                    AxisSource::Wheel
                } else {
                    AxisSource::Finger
                });
                if dx != 0. {
                    frame = frame.value(Axis::Horizontal, dx);
                }
                if dy != 0. {
                    frame = frame.value(Axis::Vertical, dy);
                }
                // without these, eg Firefox never ends its touchpad pan gesture
                if stop_x {
                    frame = frame.stop(Axis::Horizontal);
                }
                if stop_y {
                    frame = frame.stop(Axis::Vertical);
                }
                if let Some([h, v]) = v120 {
                    frame = frame.v120(Axis::Horizontal, h).v120(Axis::Vertical, v);
                }
                pointer.axis(self, frame);
            }
        }
        pointer.frame(self);
    }

    /// Raw mouse movement, sent to the surface the pointer was last sent to
    pub(crate) fn relative_pointer_motion(&mut self, [dx, dy]: [f64; 2]) {
        let Some(focus) = self
            .smithay_state
            .pointer_focus
            .clone()
            .filter(|(surface, _)| surface.is_alive())
        else {
            return;
        };
        let pointer = self.smithay_state.seat.get_pointer().unwrap();
        // physical px to logical
        let delta = Point::<f64, Physical>::from((dx, dy)).to_logical(self.smithay_state.scale);
        pointer.relative_motion(
            self,
            Some(focus),
            &RelativeMotionEvent {
                delta,
                // winit's delta is raw, so both are the same
                delta_unaccel: delta,
                time: self.smithay_state.now_input_time(),
            },
        );
        pointer.frame(self);
    }

    /// A touchpad pinch, sent to the pointer's focus (#152)
    pub(crate) fn pinch(&mut self, phase: PinchPhase) {
        let pointer = self.smithay_state.seat.get_pointer().unwrap();
        let time = self.smithay_state.now_input_time();
        match phase {
            PinchPhase::Begin { fingers } => pointer.gesture_pinch_begin(
                self,
                &GesturePinchBeginEvent {
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                    fingers,
                },
            ),
            PinchPhase::Update {
                delta: [dx, dy],
                scale,
                rotation,
            } => pointer.gesture_pinch_update(
                self,
                &GesturePinchUpdateEvent {
                    time,
                    // physical px to logical
                    delta: Point::<f64, Physical>::from((dx, dy))
                        .to_logical(self.smithay_state.scale),
                    scale,
                    rotation,
                },
            ),
            PinchPhase::End { cancelled } => pointer.gesture_pinch_end(
                self,
                &GesturePinchEndEvent {
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                    cancelled,
                },
            ),
        }
        pointer.frame(self);
    }
}

impl XdgActivationHandler for AppletRunner {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.smithay_state.xdg_activation_state
    }

    /// Like anvil: granted if the token is recent
    fn request_activation(
        &mut self,
        _token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        if token_data.timestamp.elapsed().as_secs() >= 10 {
            return;
        }
        let Some(surface_id) = self
            .smithay_state
            .surfaces
            .iter()
            .find_map(|(surface_id, registered)| (*registered == surface).then_some(surface_id))
        else {
            return;
        };

        self.smithay_state
            .send_wl_event(surface_id, WlSurfaceEvent::Focus);
        self.smithay_state.seat.get_keyboard().unwrap().set_focus(
            self,
            Some(surface),
            SERIAL_COUNTER.next_serial(),
        );
    }
}

impl FractionalScaleHandler for AppletRunner {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        self.smithay_state.send_scale(&surface);
    }
}

/// Needed by cursor shape; there's no tablet support, so the default does nothing
impl TabletSeatHandler for AppletRunner {
    type ToolFocus = WlSurface;
}

/// Needed by text input; the IME's state is sent through the display instead
impl TextInputActivation for AppletRunner {}
