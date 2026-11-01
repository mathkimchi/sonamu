//! Runs the display on a monitor directly (from a TTY, as the actual compositor), with libinput for input.
//!
//! Draws with the same wgpu renderer as the window, onto a Vulkan display surface
//! (`VK_EXT_acquire_drm_display`), so the Vulkan driver does the modesetting.

use crate::{
    display_units::{DisplayAreaPx, DisplayContainerSize},
    ui_element::{Cursor, CursorIcon, PrimitiveScene, RoundRect, UIPrimitiveElement},
    ui_event::{KeyModifiers, KeySymbol, MouseEvent, MouseEventKind, SwipeDirection, UIEvent},
    winit_backend::{
        DisplayInfo, DisplayNotification, DisplayProxy, DisplayRequest, Gpu, PointerGrab,
        UIDisplay, WgpuData, rendering::Skipped,
    },
};
use calloop::{
    EventLoop, LoopHandle, LoopSignal, RegistrationToken,
    channel::Sender,
    timer::{TimeoutAction, Timer},
};
use smithay::{
    backend::{
        drm::{DrmDeviceFd, DrmNode},
        input::{
            AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event as _,
            GestureBeginEvent as _, GestureSwipeUpdateEvent as _, InputEvent, KeyState,
            KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
        },
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        session::{Event as SessionEvent, Session, libseat::LibSeatSession},
        udev::{UdevBackend, UdevEvent, all_gpus, primary_gpu},
    },
    input::keyboard::{Keycode, Keysym, xkb},
    reexports::{
        drm::control::{Device as _, Mode, ModeTypeFlags, connector},
        input::{DeviceCapability, Libinput},
        rustix::fs::OFlags,
    },
    utils::DeviceFd,
};
use sonamu_sync::TrackedEncapsulatedLock;
use std::{
    os::fd::{AsFd as _, AsRawFd as _},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

/// Key repeat, also given to wl clients (which repeat keys themselves)
pub const REPEAT_DELAY: Duration = Duration::from_millis(600);
pub const REPEAT_INTERVAL: Duration = Duration::from_millis(40);
/// px per wheel notch, like the winit backend
const SCROLL_PER_NOTCH: f64 = 15.0;
/// Reversed ("natural") scrolling on touchpads: content follows the fingers
const NATURAL_SCROLL: bool = true;
/// Size of the fallback box, when the cursor theme has no image
const CURSOR_SIZE: [u32; 2] = [12, 18];
/// How far (px) the fingers have to move for each step of a swipe
const SWIPE_DISTANCE: f64 = 50.0;

/// A cursor uploaded to the GPU
struct CursorImage {
    texture: wgpu::TextureView,
    /// In screen px
    size: [u32; 2],
    /// In screen px
    hotspot: [u32; 2],
}

/// A monitor, showing its part of the scene
struct DrmOutput {
    connector: connector::Handle,
    mode: Mode,
    wgpu_data: WgpuData,
    /// Its top left in the scene
    origin: [u32; 2],
}

struct DrmDisplay {
    is_running: Arc<AtomicBool>,
    event_queue: Sender<UIEvent>,
    display_notification_queue: Sender<DisplayNotification>,
    ui_content: TrackedEncapsulatedLock<PrimitiveScene>,
    loop_signal: LoopSignal,
    loop_handle: LoopHandle<'static, Self>,

    /// Before `drm`, since the surfaces must be dropped first.
    /// Side by side, left to right, in connector order.
    outputs: Vec<DrmOutput>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// The last scene, redrawn when only the pointer moves
    scene: PrimitiveScene,
    needs_draw: bool,
    /// The next draw is sent back (`DisplayRequest::Capture`)
    capture: bool,
    /// Every output's area
    size: DisplayContainerSize,

    pointer: [f64; 2],
    pointer_grab: PointerGrab,
    cursor_theme: xcursor::CursorTheme,
    /// The last drawn cursor, `None` if it had no image
    cursor_image: Option<(Cursor, Option<CursorImage>)>,
    keyboard: xkb::State,
    /// The key being repeated (xkb keycode) and its timer
    repeat: Option<(Keycode, RegistrationToken)>,
    /// The swipe going on: its fingers, how far they moved since its last step, and whether it had a step
    swipe: Option<(u32, [f64; 2], bool)>,

    /// For the outputs' surfaces
    gpu: Gpu,
    /// Every connector ever shown, in order, since Mesa numbers their display planes that way
    acquired: Vec<connector::Handle>,
    /// Keeps the device open for the surfaces
    drm: DrmDeviceFd,
    session: LibSeatSession,
    libinput: Libinput,
}

/// Like `UIDisplay::run_display`, but on the monitor directly. Returns when the display is closed.
///
/// Needs a session (logind or seatd) and a Vulkan driver with `VK_EXT_acquire_drm_display` (Mesa has it).
/// If either is missing, it logs why and returns, which ends the runner.
// same arguments as `UIDisplay::run_display`
#[allow(clippy::needless_pass_by_value)]
pub fn run_drm_display(
    is_running: Arc<AtomicBool>,
    event_queue: Sender<UIEvent>,
    ui_content: TrackedEncapsulatedLock<PrimitiveScene>,
    gpu: Gpu,
    proxy_tx: std::sync::mpsc::Sender<DisplayProxy>,
    display_notification_queue: Sender<DisplayNotification>,
) {
    let mut event_loop: EventLoop<DrmDisplay> = EventLoop::try_new().unwrap();
    let (request_tx, request_rx) = calloop::channel::channel();
    // first, so whoever runs this isn't left waiting if setting up fails
    proxy_tx
        .send(Box::new(move |request| {
            // errs only if the display already closed
            let _ = request_tx.send(request);
        }))
        .unwrap();

    let mut display = match DrmDisplay::new(
        is_running,
        event_queue,
        display_notification_queue,
        ui_content,
        &gpu,
        &event_loop,
    ) {
        Ok(display) => display,
        Err(err) => {
            log::error!("Can't run on the monitor directly: {err}");
            return;
        }
    };

    event_loop
        .handle()
        .insert_source(request_rx, |event, &mut (), display| match event {
            calloop::channel::Event::Msg(request) => display.handle_request(request),
            calloop::channel::Event::Closed => display.loop_signal.stop(),
        })
        .unwrap();

    event_loop
        .run(None, &mut display, |display| {
            if !display
                .is_running
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                display.loop_signal.stop();
                return;
            }
            // once per batch, so pointer motion doesn't draw once per event
            if std::mem::take(&mut display.needs_draw) {
                display.draw();
            }
        })
        .unwrap();
    println!("Graciously ending display loop.");
}

impl DrmDisplay {
    #[allow(clippy::too_many_lines)]
    fn new(
        is_running: Arc<AtomicBool>,
        event_queue: Sender<UIEvent>,
        display_notification_queue: Sender<DisplayNotification>,
        ui_content: TrackedEncapsulatedLock<PrimitiveScene>,
        gpu: &Gpu,
        event_loop: &EventLoop<'static, Self>,
    ) -> Result<Self, String> {
        let (mut session, notifier) =
            LibSeatSession::new().map_err(|err| format!("no session (logind or seatd): {err}"))?;
        let seat = session.seat();

        let gpu_path = primary_gpu(&seat)
            .ok()
            .flatten()
            .or_else(|| all_gpus(&seat).ok()?.into_iter().next())
            .ok_or("no GPU found")?;
        let fd = session
            .open(
                &gpu_path,
                OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
            )
            .map_err(|err| format!("can't open {}: {err}", gpu_path.display()))?;
        let drm = DrmDeviceFd::new(DeviceFd::from(fd));
        log::info!(
            "Using {} ({:?})",
            gpu_path.display(),
            DrmNode::from_file(&drm).ok()
        );

        // keymap from the XKB_DEFAULT_* environment variables, like smithay's seat keyboard
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            "",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .ok_or("can't load the keymap")?;

        let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
        libinput
            .udev_assign_seat(&seat)
            .map_err(|()| "can't assign libinput to the seat")?;

        let handle = event_loop.handle();
        handle
            .insert_source(
                LibinputInputBackend::new(libinput.clone()),
                |event, &mut (), display: &mut Self| display.handle_input(event),
            )
            .map_err(|err| err.to_string())?;
        handle
            .insert_source(notifier, |event, &mut (), display: &mut Self| match event {
                SessionEvent::PauseSession => {
                    display.libinput.suspend();
                    display.stop_repeat();
                }
                SessionEvent::ActivateSession => {
                    if display.libinput.resume().is_err() {
                        log::error!("Couldn't resume libinput");
                    }
                    // monitors may have been plugged in meanwhile
                    display.update_outputs();
                    // the monitors showed something else meanwhile
                    for output in &display.outputs {
                        let WgpuData {
                            device,
                            surface,
                            surface_config,
                            ..
                        } = &output.wgpu_data;
                        surface.configure(device, surface_config);
                    }
                    display.needs_draw = true;
                }
            })
            .map_err(|err| err.to_string())?;
        // monitors plugged in or out
        handle
            .insert_source(
                UdevBackend::new(&seat).map_err(|err| format!("can't watch udev: {err}"))?,
                |event, _, display: &mut Self| {
                    if let UdevEvent::Changed { .. } | UdevEvent::Added { .. } = event {
                        display.update_outputs();
                    }
                },
            )
            .map_err(|err| err.to_string())?;

        let mut display = Self {
            is_running,
            event_queue,
            display_notification_queue,
            ui_content,
            loop_signal: event_loop.get_signal(),
            loop_handle: handle,
            outputs: Vec::new(),
            device: gpu.device.clone(),
            queue: gpu.queue.clone(),
            scene: PrimitiveScene::new_empty(),
            needs_draw: true,
            capture: false,
            size: DisplayContainerSize::new(0, 0),
            pointer: [0.0, 0.0],
            pointer_grab: PointerGrab::None,
            cursor_theme: xcursor::CursorTheme::load(
                &std::env::var("XCURSOR_THEME").unwrap_or_else(|_| "default".to_string()),
            ),
            cursor_image: None,
            keyboard: xkb::State::new(&keymap),
            repeat: None,
            swipe: None,
            gpu: gpu.clone(),
            acquired: Vec::new(),
            drm,
            session,
            libinput,
        };
        display.update_outputs();
        let Some(first) = display.outputs.first() else {
            return Err("can't show on any monitor".to_string());
        };
        display.pointer = [
            f64::from(display.size.width) / 2.0,
            f64::from(display.size.height) / 2.0,
        ];
        display
            .display_notification_queue
            .send(DisplayNotification::Info(DisplayInfo {
                scale_factor: 1.0,
                // the first monitor's, for the clients
                refresh_millihertz: Some(vulkan_refresh_millihertz(&first.mode)),
            }))
            .unwrap();
        Ok(display)
    }

    /// Shows on the connected monitors (at startup, and when they may have changed, eg one was plugged in):
    /// keeps the ones still connected at the same mode, adds the others side by side, and tells the runner.
    fn update_outputs(&mut self) {
        let connected = connected_outputs(&self.drm).unwrap_or_else(|err| {
            log::warn!("Can't list the monitors: {err}");
            Vec::new()
        });
        let mut old = std::mem::take(&mut self.outputs);
        // gone ones first, so a monitor with a new mode gets a new surface
        old.retain(|output| connected.contains(&(output.connector, output.mode)));
        for (connector, mode) in connected {
            let output = match old.iter().position(|output| output.connector == connector) {
                Some(index) => old.swap_remove(index),
                None => match self.new_output(connector, mode) {
                    Some(output) => output,
                    None => continue,
                },
            };
            self.outputs.push(output);
        }
        if self.outputs.is_empty() {
            return;
        }

        let mut size = DisplayContainerSize::new(0, 0);
        let mut screens = Vec::new();
        for output in &mut self.outputs {
            let (width, height) = output.mode.size();
            let (width, height) = (u32::from(width), u32::from(height));
            output.origin = [size.width, 0];
            screens.push([size.width, 0, width, height]);
            size = DisplayContainerSize::new(size.width + width, size.height.max(height));
        }
        if size != self.size {
            self.size = size;
            self.pointer = self.clamp_to_screen(self.pointer);
            self.send(UIEvent::WindowResized(size));
        }
        let _ = self
            .display_notification_queue
            .send(DisplayNotification::Screens(screens));
        self.needs_draw = true;
    }

    /// A surface on the monitor, `None` if it can't be made
    fn new_output(&mut self, connector: connector::Handle, mode: Mode) -> Option<DrmOutput> {
        let (width, height) = mode.size();
        let (width, height) = (u32::from(width), u32::from(height));
        let refresh_millihertz = vulkan_refresh_millihertz(&mode);
        log::info!(
            "Showing on connector {connector:?} at {width}x{height}@{refresh_millihertz}mHz"
        );
        let plane = match self
            .acquired
            .iter()
            .position(|&acquired| acquired == connector)
        {
            Some(plane) => plane,
            None => {
                self.acquired.push(connector);
                self.acquired.len() - 1
            }
        };
        // SAFETY: the fd is kept open (`drm`) until after the surfaces are dropped (`outputs`);
        // the connector and mode come from the device itself
        let surface = unsafe {
            self.gpu
                .instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::Drm {
                    fd: self.drm.as_fd().as_raw_fd(),
                    // Vulkan's display plane index, not a DRM plane id.
                    // Mesa has one plane per acquired connector, in the order they were acquired.
                    plane: std::env::var("SONAMU_VK_DISPLAY_PLANE")
                        .ok()
                        .and_then(|plane| plane.parse().ok())
                        .unwrap_or(0)
                        + plane as u32,
                    connector_id: connector.into(),
                    width,
                    height,
                    refresh_rate: refresh_millihertz,
                })
        };
        let surface = surface
            .inspect_err(|err| {
                log::error!("Can't make a Vulkan display surface for {connector:?}: {err}");
            })
            .ok()?;
        Some(DrmOutput {
            connector,
            mode,
            wgpu_data: WgpuData::new(
                surface,
                DisplayContainerSize::new(width, height),
                self.gpu.device.clone(),
                self.gpu.queue.clone(),
                &self.gpu.adapter,
            ),
            origin: [0, 0],
        })
    }

    fn handle_request(&mut self, request: DisplayRequest) {
        match request {
            DisplayRequest::Redraw => self.needs_draw = true,
            DisplayRequest::Capture => {
                self.capture = true;
                self.needs_draw = true;
            }
            // the pointer never leaves the monitor anyway; a lock just stops moving it
            DisplayRequest::SetPointerGrab(grab) => self.pointer_grab = grab,
            DisplayRequest::WarpPointer(position) => {
                self.pointer = self.clamp_to_screen(position);
                self.needs_draw = true;
            }
        }
    }

    fn draw(&mut self) {
        let new_content = self.ui_content.get_if_dirty();
        let presented = new_content.is_some();
        if let Some(scene) = new_content {
            self.scene = scene;
        }

        // a capture is of the whole window, so only of a single monitor
        let capture = self.capture && self.outputs.len() == 1;
        let mut outputs = std::mem::take(&mut self.outputs);
        let scene = self.scene_with_cursor();
        let mut drawn = Err(Skipped);
        for output in &mut outputs {
            drawn = UIDisplay::draw(&mut output.wgpu_data, &scene, capture, output.origin);
        }
        drop(scene);
        self.outputs = outputs;

        if presented {
            let _ = self
                .display_notification_queue
                .send(DisplayNotification::Presented);
        }
        // a skipped frame is captured on the next draw
        if self.capture && (drawn.is_ok() || !capture) {
            self.capture = false;
            let _ = self
                .display_notification_queue
                .send(DisplayNotification::Captured(drawn.ok().flatten()));
        }
    }

    /// The scene, with the cursor drawn in (a software cursor) unless hidden
    fn scene_with_cursor(&mut self) -> std::borrow::Cow<'_, PrimitiveScene> {
        let cursor = self
            .scene
            .cursors
            .cursor_at(self.pointer)
            .cloned()
            .unwrap_or(Cursor::Named(CursorIcon::Default));
        if cursor == Cursor::Hidden {
            std::borrow::Cow::Borrowed(&self.scene)
        } else {
            if self
                .cursor_image
                .as_ref()
                .is_none_or(|(drawn, _)| *drawn != cursor)
            {
                let image = self.load_cursor(&cursor);
                self.cursor_image = Some((cursor, image));
            }
            // software cursor
            let mut scene = self.scene.clone();
            // the pointer is clamped to the screen, so non-negative
            #[allow(clippy::cast_sign_loss)]
            let [x, y] = self.pointer.map(|value| value as u32);
            scene.elements.push(
                match self
                    .cursor_image
                    .as_ref()
                    .and_then(|(_, image)| image.as_ref())
                {
                    Some(image) => {
                        let x = x.saturating_sub(image.hotspot[0]);
                        let y = y.saturating_sub(image.hotspot[1]);
                        (
                            UIPrimitiveElement::Texture(image.texture.clone()),
                            DisplayAreaPx::new(x, y, x + image.size[0], y + image.size[1]),
                            None,
                        )
                    }
                    // a white box with a black outline
                    None => (
                        UIPrimitiveElement::RoundRect(RoundRect {
                            corner_radius: 2.0,
                            border_width: 1.0,
                            main_color: [1.0, 1.0, 1.0, 1.0],
                            border_color: [0.0, 0.0, 0.0, 1.0],
                        }),
                        DisplayAreaPx::new(x, y, x + CURSOR_SIZE[0], y + CURSOR_SIZE[1]),
                        None,
                    ),
                },
            );
            std::borrow::Cow::Owned(scene)
        }
    }

    /// Uploads the cursor's image, `None` if it has none (or is hidden)
    fn load_cursor(&self, cursor: &Cursor) -> Option<CursorImage> {
        let (rgba, size, hotspot, scale) = match cursor {
            Cursor::Hidden => return None,
            Cursor::Named(icon) => {
                let image = self.load_theme_cursor(*icon).or_else(|| {
                    log::warn!("No cursor {icon:?} in the theme");
                    self.load_theme_cursor(CursorIcon::Default)
                })?;
                (
                    image.pixels_rgba.into(),
                    [image.width, image.height],
                    [image.xhot, image.yhot],
                    1.0,
                )
            }
            Cursor::Custom {
                rgba,
                width,
                height,
                hotspot,
                scale,
            } => (
                rgba.clone(),
                [u32::from(*width), u32::from(*height)],
                hotspot.map(u32::from),
                *scale,
            ),
        };

        let Self { device, queue, .. } = self;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("cursor_texture"),
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            &rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * size[0]),
                rows_per_image: None,
            },
            texture.size(),
        );
        // image px to screen px; the texture is stretched to the drawn size
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let to_screen = |px: [u32; 2]| px.map(|px| (px as f32 / scale).round() as u32);
        Some(CursorImage {
            texture: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            size: to_screen(size).map(|px| px.max(1)),
            hotspot: to_screen(hotspot),
        })
    }

    /// The icon's first frame from the theme, at the size closest to `XCURSOR_SIZE` (24 by default)
    fn load_theme_cursor(&self, icon: CursorIcon) -> Option<xcursor::parser::Image> {
        let wanted_size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|size| size.parse().ok())
            .unwrap_or(24);
        let path = std::iter::once(icon.name())
            .chain(icon.alt_names().iter().copied())
            .find_map(|name| self.cursor_theme.load_icon(name))?;
        let images = xcursor::parser::parse_xcursor(&std::fs::read(path).ok()?)?;
        images
            .into_iter()
            .min_by_key(|image| image.size.abs_diff(wanted_size))
    }

    fn clamp_to_screen(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        [
            x.clamp(0.0, f64::from(self.size.width) - 1.0),
            y.clamp(0.0, f64::from(self.size.height) - 1.0),
        ]
    }

    fn send(&self, event: UIEvent) {
        // the runner may be gone while shutting down
        let _ = self.event_queue.send(event);
    }

    /// `time_msec` is the libinput event's, since events may be handled in batches (eg after a draw)
    fn send_mouse(&self, kind: MouseEventKind, time_msec: u32) {
        self.send(UIEvent::Mouse(MouseEvent {
            position: self.pointer,
            kind,
            time_msec: Some(time_msec),
        }));
    }

    fn handle_input(&mut self, event: InputEvent<LibinputInputBackend>) {
        match event {
            InputEvent::DeviceAdded { mut device } => {
                // touchpads only (they have gestures), so mouse wheels keep the usual direction
                if device.has_capability(DeviceCapability::Gesture)
                    && device.config_scroll_has_natural_scroll()
                {
                    let _ = device.config_scroll_set_natural_scroll_enabled(NATURAL_SCROLL);
                }
            }
            InputEvent::Keyboard { event } => {
                self.handle_key(event.key_code(), event.state() == KeyState::Pressed);
            }
            InputEvent::PointerMotion { event } => {
                let delta = event.delta_unaccel();
                let _ = self
                    .display_notification_queue
                    .send(DisplayNotification::RelativeMotion([delta.x, delta.y]));
                if self.pointer_grab != PointerGrab::Locked {
                    let delta = event.delta();
                    self.pointer = self
                        .clamp_to_screen([self.pointer[0] + delta.x, self.pointer[1] + delta.y]);
                    self.needs_draw = true;
                    self.send_mouse(MouseEventKind::Motion, event.time().millis());
                }
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let position = event
                    .position_transformed((self.size.width as i32, self.size.height as i32).into());
                self.pointer = self.clamp_to_screen([position.x, position.y]);
                self.needs_draw = true;
                self.send_mouse(MouseEventKind::Motion, event.time().millis());
            }
            InputEvent::PointerButton { event } => self.send_mouse(
                MouseEventKind::Button {
                    button: event.button_code(),
                    pressed: event.state() == ButtonState::Pressed,
                },
                event.time().millis(),
            ),
            InputEvent::PointerAxis { event } => {
                let axis_delta = |axis| {
                    // wheels give notches, touchpads give px
                    event.amount_v120(axis).map_or_else(
                        || (event.amount(axis).unwrap_or(0.0), None),
                        |v120| (v120 / 120.0 * SCROLL_PER_NOTCH, Some(v120 as i32)),
                    )
                };
                let (x, x_v120) = axis_delta(Axis::Horizontal);
                let (y, y_v120) = axis_delta(Axis::Vertical);
                // like niri: libinput ends a finger scroll with an explicit 0 on that axis
                let stop =
                    |axis| event.source() == AxisSource::Finger && event.amount(axis) == Some(0.0);
                self.send_mouse(
                    MouseEventKind::Scroll {
                        delta: [x, y],
                        v120: (x_v120.is_some() || y_v120.is_some())
                            .then(|| [x_v120.unwrap_or(0), y_v120.unwrap_or(0)]),
                        stop: [stop(Axis::Horizontal), stop(Axis::Vertical)],
                    },
                    event.time().millis(),
                );
            }
            InputEvent::GestureSwipeBegin { event } => {
                self.swipe = Some((event.fingers(), [0.0; 2], false));
            }
            InputEvent::GestureSwipeUpdate { event } => {
                let Some((fingers, moved, repeat)) = &mut self.swipe else {
                    return;
                };
                let delta = event.delta();
                moved[0] += delta.x;
                moved[1] += delta.y;
                let [x, y] = *moved;
                // a step each time, like scrolling
                if x.hypot(y) >= SWIPE_DISTANCE {
                    let direction = match (x.abs() > y.abs(), x > 0.0, y > 0.0) {
                        (true, true, _) => SwipeDirection::Right,
                        (true, false, _) => SwipeDirection::Left,
                        (false, _, true) => SwipeDirection::Down,
                        (false, _, false) => SwipeDirection::Up,
                    };
                    let event = UIEvent::Swipe {
                        fingers: *fingers,
                        direction,
                        repeat: *repeat,
                    };
                    *moved = [0.0; 2];
                    *repeat = true;
                    self.send(event);
                }
            }
            InputEvent::GestureSwipeEnd { .. } => self.swipe = None,
            _ => {}
        }
    }

    fn handle_key(&mut self, keycode: Keycode, pressed: bool) {
        // modifiers from before this key, like winit
        let modifiers = self.modifiers();
        let keysym = self.keyboard.key_get_one_sym(keycode);
        self.keyboard.update_key(
            keycode,
            if pressed {
                xkb::KeyDirection::Down
            } else {
                xkb::KeyDirection::Up
            },
        );

        if pressed {
            // the TTY can't be reached otherwise, since its keyboard is off while this runs
            if modifiers.ctrl && modifiers.alt && keysym == Keysym::BackSpace {
                log::warn!("Ctrl+Alt+Backspace: quitting");
                self.is_running
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            let vt = keysym.raw().wrapping_sub(Keysym::XF86_Switch_VT_1.raw());
            if vt < 12 {
                if let Err(err) = self.session.change_vt(vt as i32 + 1) {
                    log::error!("Couldn't switch VT: {err}");
                }
                return;
            }
        }

        self.send_key(keycode, keysym, modifiers, pressed);

        if pressed && self.keyboard.get_keymap().key_repeats(keycode) {
            self.start_repeat(keycode);
        } else if !pressed
            && self
                .repeat
                .is_some_and(|(repeating, _)| repeating == keycode)
        {
            self.stop_repeat();
        }
    }

    fn send_key(&self, keycode: Keycode, keysym: Keysym, modifiers: KeyModifiers, pressed: bool) {
        self.send(UIEvent::Key {
            symbol: key_symbol(keysym),
            modifiers,
            // xkb -> evdev
            raw_keycode: keycode.raw() - 8,
            pressed,
        });
    }

    /// Presses the key again every `REPEAT_INTERVAL`, after `REPEAT_DELAY`, until released
    fn start_repeat(&mut self, keycode: Keycode) {
        self.stop_repeat();
        let token = self
            .loop_handle
            .insert_source(
                Timer::from_duration(REPEAT_DELAY),
                move |_, &mut (), display: &mut Self| {
                    let keysym = display.keyboard.key_get_one_sym(keycode);
                    display.send_key(keycode, keysym, display.modifiers(), true);
                    TimeoutAction::ToDuration(REPEAT_INTERVAL)
                },
            )
            .unwrap();
        self.repeat = Some((keycode, token));
    }

    fn stop_repeat(&mut self) {
        if let Some((_, token)) = self.repeat.take() {
            self.loop_handle.remove(token);
        }
    }

    fn modifiers(&self) -> KeyModifiers {
        let active = |name| {
            self.keyboard
                .mod_name_is_active(name, xkb::STATE_MODS_EFFECTIVE)
        };
        KeyModifiers {
            ctrl: active(xkb::MOD_NAME_CTRL),
            alt: active(xkb::MOD_NAME_ALT),
            shift: active(xkb::MOD_NAME_SHIFT),
            caps_lock: active(xkb::MOD_NAME_CAPS),
            logo: active(xkb::MOD_NAME_LOGO),
        }
    }
}

/// Like the winit backend's `KeySymbol::try_from`: named keys, otherwise the key's character
/// (without Ctrl's control-character transform, so Ctrl+W is still `w`)
fn key_symbol(keysym: Keysym) -> Option<KeySymbol> {
    Some(match keysym {
        Keysym::Left => KeySymbol::ArrowKeyLeft,
        Keysym::Right => KeySymbol::ArrowKeyRight,
        Keysym::Up => KeySymbol::ArrowKeyUp,
        Keysym::Down => KeySymbol::ArrowKeyDown,
        Keysym::Return | Keysym::KP_Enter => KeySymbol::Enter,
        Keysym::Page_Down => KeySymbol::PageDown,
        Keysym::Page_Up => KeySymbol::PageUp,
        Keysym::BackSpace => KeySymbol::Backspace,
        Keysym::Escape => KeySymbol::Escape,
        Keysym::Tab | Keysym::ISO_Left_Tab => KeySymbol::Char('\t'),
        _ => KeySymbol::Char(keysym.key_char()?),
    })
}

/// The connected connectors, at their preferred modes
fn connected_outputs(drm: &DrmDeviceFd) -> Result<Vec<(connector::Handle, Mode)>, String> {
    let resources = drm
        .resource_handles()
        .map_err(|err| format!("can't read DRM resources: {err}"))?;
    let outputs = resources
        .connectors()
        .iter()
        .filter_map(|&handle| drm.get_connector(handle, true).ok())
        .filter(|info| info.state() == connector::State::Connected)
        .filter_map(|info| {
            if info.modes().is_empty() {
                log::warn!(
                    "Connector {:?} is connected but has no modes",
                    info.handle()
                );
            }
            let mode = info
                .modes()
                .iter()
                .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
                .or_else(|| info.modes().first())?;
            Some((info.handle(), *mode))
        })
        .collect::<Vec<_>>();
    if outputs.is_empty() {
        return Err("no connected monitor".to_string());
    }
    Ok(outputs)
}

/// The refresh rate the way Mesa reports it for Vulkan display modes,
/// since wgpu picks the mode by an exact match
#[allow(clippy::cast_sign_loss)]
fn vulkan_refresh_millihertz(mode: &Mode) -> u32 {
    let (_, _, htotal) = mode.hsync();
    let (_, _, vtotal) = mode.vsync();
    let hz = f64::from(mode.clock()) * 1000.0
        / (f64::from(htotal) * f64::from(vtotal) * f64::from(mode.vscan().max(1)));
    (hz * 1000.0 + 0.5) as u32
}
