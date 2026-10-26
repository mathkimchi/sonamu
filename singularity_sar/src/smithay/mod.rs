mod capture;
mod layer_shell;

use crate::runner::AppletRunner;
use calloop::{LoopHandle, PostAction};
use singularity_common::sap::packets::{
    NewWlSurface, StandardEvent, WlSurfaceEvent, WlSurfaceId, WlSurfaceRequest,
};
use slotmap::{SecondaryMap, SlotMap};
use smithay::{
    backend::{
        input::{Axis, AxisSource, ButtonState, InputTime, KeyState, Keycode},
        renderer::{
            BufferType, buffer_type,
            utils::{Buffer, RendererSurfaceStateUserData, SurfaceView, on_commit_buffer_handler},
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
            AxisFrame, ButtonEvent, CursorImageStatus, CursorImageSurfaceData, Focus, MotionEvent,
            PointerHandle, RelativeMotionEvent,
        },
        tablet::TabletSeatHandler,
    },
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback::Kind as PresentationFeedbackKind,
    reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode,
    reexports::wayland_server::{
        Client, Display, DisplayHandle, Resource as _,
        backend::ClientData,
        protocol::{wl_seat::WlSeat, wl_surface::WlSurface},
    },
    utils::{
        Buffer as BufferCoords, Clock, Logical, Monotonic, Physical, Point, Rectangle,
        SERIAL_COUNTER, Serial, Size, Time, Transform,
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            CompositorClientState, CompositorHandler, CompositorState, SurfaceAttributes,
            SurfaceData, TraversalAction, send_surface_state, with_states,
            with_surface_tree_downward,
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
        presentation::{PresentationFeedbackCachedState, PresentationState, Refresh},
        relative_pointer::RelativePointerManagerState,
        security_context::SecurityContext,
        selection::{
            SelectionHandler,
            data_device::{
                DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
            },
            primary_selection::{
                PrimarySelectionHandler, PrimarySelectionState, set_primary_focus,
            },
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
use sonamu_ui::{
    DisplayInfo, ImeEvent, PointerGrab,
    display_units::{DisplayAreaPx, DisplayContainerSize},
    ui_element::{Cursor, Style, UIElement, UITree},
    ui_event::{MouseEvent, MouseEventKind},
};
use std::{collections::HashMap, sync::Arc, time::Duration};
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
    capture: capture::CaptureState,
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
    /// The layer surface (not an exclusive one) that a click gave the keyboard
    layer_focus: Option<WlSurface>,
    /// The tile that Sonamu focus asked the keyboard to be on
    pub(crate) wl_target: Option<WlSurfaceId>,
    /// The pointer is on a layer surface, or a button pressed on one is still held
    pointer_on_layer: bool,
    layer_buttons_down: u32,
    /// Toplevels not yet told to the root: they are on their first commit, once they have set their app id
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

    /// Where each wl toplevel's tile was laid out in the last redraw
    pub fn set_tile_areas(
        &mut self,
        tile_areas: impl Iterator<Item = (WlSurfaceId, DisplayAreaPx)>,
    ) {
        self.tile_areas.clear();
        self.tile_areas.extend(tile_areas);
    }

    /// Same as anvil: keeps the popup inside the output (the Sonamu window).
    /// Falls back to the toplevel's tile if the tile hasn't been laid out yet.
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let kind = PopupKind::Xdg(popup.clone());
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };

        // the output, relative to the tile
        let output_in_tile = self
            .surfaces
            .iter()
            .find_map(|(surface_id, surface)| (*surface == root).then_some(surface_id))
            .and_then(|surface_id| self.tile_areas.get(&surface_id))
            .zip(self.output.current_mode())
            .map(|(DisplayAreaPx([[left, top], _]), mode)| {
                Rectangle::new(
                    Point::<f64, Physical>::from((-f64::from(*left), -f64::from(*top)))
                        .to_logical(self.scale)
                        .to_i32_round(),
                    mode.size.to_f64().to_logical(self.scale).to_i32_round(),
                )
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
    /// TODO: skip surfaces that aren't visible (and discard their presentation feedback)
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
            .values()
            .chain(self.layer_surfaces.iter().map(LayerSurface::wl_surface))
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
            refresh: self
                .refresh_millihertz
                .map_or(60_000, |refresh| refresh.cast_signed()),
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

        if old_scale != self.scale {
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
        {
            self.send_scale(root);
            for (popup, _) in PopupManager::popups_for_surface(root) {
                self.send_scale(popup.wl_surface());
            }
        }
    }

    /// The client may have destroyed it since
    fn live_pointer_focus(&self) -> Option<&WlSurface> {
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

    pub fn new(event_handle: &LoopHandle<AppletRunner>) -> Self {
        let start_time = std::time::Instant::now();

        let display: Display<AppletRunner> = Display::new().unwrap();
        let display_handle = display.handle();

        let compositor_state = CompositorState::new::<AppletRunner>(&display_handle);
        let xdg_shell_state = XdgShellState::new::<AppletRunner>(&display_handle);
        let layer_shell_state = Self::new_layer_shell_state(&display_handle);
        let shm_state = ShmState::new::<AppletRunner>(&display_handle, Vec::new());
        let mut seat_state = SeatState::new();

        let mut seat = seat_state.new_wl_seat(&display_handle, "hello");

        seat.add_keyboard(smithay::input::keyboard::XkbConfig::default(), 200, 25)
            .unwrap();
        seat.add_pointer();

        let output_manager_state =
            OutputManagerState::new_with_xdg_output::<AppletRunner>(&display_handle);
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
        // the real scale is set by `set_display_info`
        output.change_current_state(None, None, Some(Scale::Integer(1)), Some((0, 0).into()));
        output.create_global::<AppletRunner>(&display_handle);

        let data_device_state = DataDeviceState::new::<AppletRunner>(&display_handle);
        let primary_selection_state = PrimarySelectionState::new::<AppletRunner>(&display_handle);
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
        let capture = capture::CaptureState::new(&display_handle);
        let clock = Clock::<Monotonic>::new();
        let presentation_state =
            PresentationState::new::<AppletRunner>(&display_handle, clock.id() as u32);

        // set up socket
        let socket_name = {
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

            // `SONAMU_WAYLAND_DISPLAY` is set to it by `AppletRunner::run`
            socket_name
        };

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
            capture,
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
            layer_placements: Vec::new(),
            usable_area: Rectangle::default(),
            layer_focus: None,
            wl_target: None,
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
                            toplevel.with_pending_state(|toplevel_state| {
                                toplevel_state.size = Some(size);
                            });
                            toplevel.send_configure();
                        }
                        WlSurfaceRequest::Mouse(MouseEvent {
                            position,
                            kind,
                            time_msec,
                        }) => {
                            // Motion first so the pointer is focused on the surface under it.
                            // `position` is already relative to the tile, which is where the root is drawn
                            let location = Point::<f64, Physical>::from((position[0], position[1]))
                                .to_logical(state.smithay_state.scale);
                            let focus = SmithayState::surface_under(&target_surface, location);
                            state.send_wl_mouse(location, focus, kind, time_msec);
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
                    ))
                })
        };

        let mut layers = Vec::new();
        with_surface_tree_downward(
            root,
            origin,
            |_, states, &location| match view(states) {
                Some((view, ..)) => TraversalAction::DoChildren(location + view.offset),
                // not mapped, so neither are its children
                None => TraversalAction::SkipChildren,
            },
            |surface, states, &location| {
                if let Some((view, buffer, buffer_scale)) = view(states) {
                    layers.push(Layer {
                        surface: surface.clone(),
                        buffer,
                        buffer_scale,
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
                let texture = Self::import_buffer(&layer.buffer, src, device, queue)?;
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
                read_cursor_surface(surface)
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

    /// The topmost surface of the toplevel under `point` (relative to the root),
    /// and that surface's offset from the root.
    ///
    /// TODO: respect input regions
    fn surface_under(
        root: &WlSurface,
        point: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        Self::window_layers(root)
            .into_iter()
            .rev()
            .find(|layer| {
                Rectangle::new(layer.location, layer.view.dst)
                    .to_f64()
                    .contains(point)
            })
            .map(|layer| (layer.surface, layer.location.to_f64()))
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
                    &wl_buffer,
                    |content_ptr, length, metadata| {
                        // SAFETY: valid for the body of this closure only. The client
                        // may mutate the pool concurrently, so don't hold this anywhere.
                        let data = unsafe { std::slice::from_raw_parts(content_ptr, length) };

                        // crop to `src`, kept inside the buffer
                        let left = (src.loc.x.floor() as i32).clamp(0, metadata.width);
                        let top = (src.loc.y.floor() as i32).clamp(0, metadata.height);
                        let right =
                            ((src.loc.x + src.size.w).ceil() as i32).clamp(left, metadata.width);
                        let bottom =
                            ((src.loc.y + src.size.h).ceil() as i32).clamp(top, metadata.height);
                        if right == left || bottom == top {
                            return None;
                        }

                        let texture_size = wgpu::Extent3d {
                            width: (right - left).cast_unsigned(),
                            height: (bottom - top).cast_unsigned(),
                            // All textures are stored as 3D
                            depth_or_array_layers: 1,
                        };

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
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format,
                            // COPY_DST means that we want to copy data to this texture
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_DST,
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
                                    (metadata.offset + top * metadata.stride + left * 4)
                                        .cast_unsigned(),
                                ),
                                bytes_per_row: Some(metadata.stride.cast_unsigned()),
                                rows_per_image: Some(texture_size.height),
                            },
                            texture_size,
                        );

                        Some(texture.create_view(&TextureViewDescriptor::default()))
                    },
                )
                .ok()
                .flatten()
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
/// A surface drawn as part of a toplevel
struct Layer {
    surface: WlSurface,
    buffer: Buffer,
    buffer_scale: i32,
    /// Offset from the root
    location: Point<i32, Logical>,
    /// The crop (`src`) and drawn size (`dst`), from viewporter or the buffer
    view: SurfaceView,
    /// Part of the root's surface tree (not a popup)
    in_root_tree: bool,
}

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
/// Its subsurfaces are ignored.
fn read_cursor_surface(surface: &WlSurface) -> Option<Cursor> {
    let (buffer, hotspot) = with_states(surface, |states| {
        let buffer = states
            .data_map
            .get::<RendererSurfaceStateUserData>()?
            .lock()
            .unwrap()
            .buffer()?
            .clone();
        let hotspot = states
            .data_map
            .get::<CursorImageSurfaceData>()?
            .lock()
            .unwrap()
            .hotspot;
        Some((buffer, hotspot))
    })?;

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
            for pixel in data.get(start..start + width * 4)?.chunks_exact(4) {
                let &[b, g, r, a] = pixel else { unreachable!() };
                let a = if has_alpha { a } else { u8::MAX };
                let unpremultiply = |c: u8| {
                    let c = (u16::from(c) * 255).checked_div(u16::from(a)).unwrap_or(0);
                    // clamp, in case the client's color is brighter than its alpha allows
                    u8::try_from(c).unwrap_or(u8::MAX)
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

    /// Tells the root about a new toplevel on its first commit, when its app id is set
    /// (it is matched by pid and app id to a restored tile)
    fn register_toplevel(&mut self, surface: &WlSurface) {
        let state = &mut self.smithay_state;
        let Some(index) = state
            .unregistered
            .iter()
            .position(|&id| state.surfaces.get(id) == Some(surface))
        else {
            return;
        };
        let surface_id = state.unregistered.remove(index);
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
    }
}
impl CompositorHandler for AppletRunner {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.smithay_state.compositor_state
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

impl XdgShellHandler for AppletRunner {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.smithay_state.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        dbg!("Handling new toplevel surface");
        surface.with_pending_state(|state| {
            use wayland_protocols::xdg::shell::server::xdg_toplevel::State;
            state.size = Some((800, 600).into());
            state.states.set(State::Activated);
            // like niri for tiled windows: tells CSD clients (eg GTK) to drop shadows and rounded corners
            for tiled in [
                State::TiledLeft,
                State::TiledRight,
                State::TiledTop,
                State::TiledBottom,
            ] {
                state.states.set(tiled);
            }
        });
        surface.send_configure();
        self.smithay_state.output.enter(surface.wl_surface());

        // keyboard focus is given once Sonamu focus reaches the new tile (`StandardRequest::WlKeyboardFocus`)

        // add surface to list of surfaces; the root is told on its first commit (`register_toplevel`)
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

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
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
    type SelectionUserData = ();
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
        // a layer surface that has the keyboard keeps it
        if let Some(layer) = self.smithay_state.keyboard_layer() {
            self.focus_wl_keyboard_on(layer);
            return;
        }
        match surface_id.and_then(|surface_id| self.smithay_state.surfaces.get(surface_id).cloned())
        {
            Some(surface) => self.focus_wl_keyboard_on(surface),
            None => {
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
