use winit::platform::scancode::PhysicalKeyExtScancode;

use crate::{
    display_units::DisplayContainerSize,
    ui_element::{Cursor, CursorIcon},
    ui_event::{KeySymbol, MouseEvent, MouseEventKind, UIEvent},
    winit_backend::{
        DisplayInfo, DisplayNotification, DisplayRequest, ImeEvent, PointerGrab, UIDisplay,
        WgpuData, WinitData,
    },
};
use smithay::reexports::winit;
use std::sync::Arc;
use winit::{
    cursor::CustomCursorSource,
    dpi::{LogicalSize, PhysicalPosition},
    event_loop::ActiveEventLoop,
    window::{CursorGrabMode, Window, WindowAttributes},
};

impl winit::application::ApplicationHandler for UIDisplay {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.winit_data.is_some() {
            return;
        }

        // Set up window
        let window_attributes = WindowAttributes::default()
            .with_surface_size(LogicalSize::new(800, 600))
            .with_title("Sonamu");
        let window: Arc<dyn Window> = event_loop.create_window(window_attributes).unwrap().into();

        // the surface size is in physical px, which isn't 800x600 when scaled
        let size = window.surface_size();
        self.event_queue
            .send(UIEvent::WindowResized(DisplayContainerSize::new(
                size.width,
                size.height,
            )))
            .unwrap();
        self.send_display_info(&*window);
        #[allow(deprecated)] // `request_ime_update` needs a cursor area, which we don't track
        window.set_ime_allowed(true);

        let winit_data = WinitData::new(
            window,
            self.device.clone(),
            self.queue.clone(),
            &self.instance,
            &self.adapter,
        );
        self.winit_data = Some(winit_data);
    }

    /// Woken to read `DisplayRequest`s, eg when the ui content is updated
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        // a quit sends a request to wake this up
        if !self.is_running.load(std::sync::atomic::Ordering::Relaxed) {
            event_loop.exit();
            return;
        }

        let Some(winit_data) = &self.winit_data else {
            return;
        };
        let window = &winit_data.window;
        while let Ok(request) = self.requests.try_recv() {
            match request {
                DisplayRequest::Redraw => window.request_redraw(),
                DisplayRequest::Capture => {
                    self.capture = true;
                    window.request_redraw();
                }
                DisplayRequest::SetPointerGrab(grab) => {
                    let result = match grab {
                        PointerGrab::None => window.set_cursor_grab(CursorGrabMode::None),
                        PointerGrab::Confined => window.set_cursor_grab(CursorGrabMode::Confined),
                        // not every platform can lock (eg X11), so confine there
                        PointerGrab::Locked => window
                            .set_cursor_grab(CursorGrabMode::Locked)
                            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined)),
                    };
                    if let Err(err) = result {
                        dbg!("Couldn't grab pointer:", err);
                    }
                }
                DisplayRequest::WarpPointer([x, y]) => {
                    if let Err(err) = window.set_cursor_position(PhysicalPosition::new(x, y).into())
                    {
                        dbg!("Couldn't warp pointer:", err);
                    }
                }
            }
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &dyn ActiveEventLoop,
        _device_id: Option<winit::event::DeviceId>,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::PointerMotion { delta: (dx, dy) } = event {
            self.display_notification_queue
                .send(DisplayNotification::RelativeMotion([dx, dy]))
                .unwrap();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        if !self.is_running.load(std::sync::atomic::Ordering::Relaxed) {
            event_loop.exit();
            return;
        }

        let Some(state) = &mut self.winit_data else {
            return;
        };

        let WinitData {
            window,
            wgpu_data:
                WgpuData {
                    device,
                    // queue,
                    surface,
                    surface_config,
                    ..
                },
        } = state;

        // Other redraws are requested through `user_event` when content changes
        match event {
            // wgpu panics on a zero-sized surface (eg minimized on some platforms)
            winit::event::WindowEvent::SurfaceResized(size)
                if size.width == 0 || size.height == 0 => {}
            winit::event::WindowEvent::SurfaceResized(size) => {
                surface_config.width = size.width;
                surface_config.height = size.height;
                if let Some(surface) = surface {
                    surface.configure(device, surface_config);
                }

                self.event_queue
                    .send(UIEvent::WindowResized(DisplayContainerSize::new(
                        size.width,
                        size.height,
                    )))
                    .unwrap();

                window.request_redraw();
            }
            // the window may now be on a different monitor
            winit::event::WindowEvent::ScaleFactorChanged { .. }
            | winit::event::WindowEvent::Moved(_) => {
                let window = window.clone();
                self.send_display_info(&*window);
            }
            winit::event::WindowEvent::Ime(ime) => {
                let ime = match ime {
                    winit::event::Ime::Preedit(text, cursor) => ImeEvent::Preedit { text, cursor },
                    winit::event::Ime::Commit(text) => ImeEvent::Commit(text),
                    _ => return,
                };
                self.display_notification_queue
                    .send(DisplayNotification::Ime(ime))
                    .unwrap();
            }
            winit::event::WindowEvent::CloseRequested => {
                self.is_running
                    .store(false, std::sync::atomic::Ordering::Relaxed);
                event_loop.exit();
            }
            // winit::event::WindowEvent::Focused(focus) => self.ui_event_queue.lock().unwrap().push(crate::ui_event::UIEvent::Focused),
            winit::event::WindowEvent::ModifiersChanged(modifiers) => {
                self.key_modifiers = modifiers.into();
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                // log::debug!("{event:?}");
                self.event_queue
                    .send(UIEvent::Key {
                        symbol: KeySymbol::try_from(event.clone()).ok(),
                        modifiers: self.key_modifiers,
                        raw_keycode: event.physical_key.to_scancode().unwrap_or(0),
                        pressed: event.state.is_pressed(),
                    })
                    .unwrap();
            }
            winit::event::WindowEvent::PointerMoved { position, .. } => {
                self.cursor_position = [position.x, position.y];
                self.update_cursor(event_loop);
                Self::send_mouse_event(
                    &self.event_queue,
                    self.cursor_position,
                    MouseEventKind::Motion,
                );
            }
            winit::event::WindowEvent::PointerButton { state, button, .. } => {
                if let Some(kind) = button_kind(state, button) {
                    Self::send_mouse_event(&self.event_queue, self.cursor_position, kind);
                }
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                if let Some(kind) = scroll_kind(delta) {
                    Self::send_mouse_event(&self.event_queue, self.cursor_position, kind);
                }
            }
            winit::event::WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }
}
impl UIDisplay {
    fn redraw(&mut self, event_loop: &dyn ActiveEventLoop) {
        // a capture draws even if nothing changed
        let content = if self.capture {
            Some(self.ui_content.get().0)
        } else {
            self.ui_content.get_if_dirty()
        };
        if let Some(content) = content {
            if let Some(winit_data) = &mut self.winit_data
                // a skipped frame is captured on the next draw
                && let Ok(captured) =
                    Self::draw(&mut winit_data.wgpu_data, &content, self.capture, [0, 0])
                && std::mem::take(&mut self.capture)
            {
                self.display_notification_queue
                    .send(DisplayNotification::Captured(captured))
                    .unwrap();
            }
            self.display_notification_queue
                .send(DisplayNotification::Presented)
                .unwrap();

            // what's under the pointer may have changed
            self.drawn_cursors = content.cursors;
            self.update_cursor(event_loop);
        }
    }

    /// Sends the window's scale and refresh rate, if they changed
    fn send_display_info(&mut self, window: &dyn Window) {
        let info = DisplayInfo {
            scale_factor: window.scale_factor(),
            refresh_millihertz: window
                .current_monitor()
                .and_then(|monitor| monitor.current_video_mode())
                .and_then(|mode| mode.refresh_rate_millihertz())
                .map(std::num::NonZeroU32::get),
        };
        if self.display_info != Some(info) {
            self.display_info = Some(info);
            self.display_notification_queue
                .send(DisplayNotification::Info(info))
                .unwrap();
        }
    }

    /// Sets the window's cursor to the one under the pointer, if it changed
    fn update_cursor(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(winit_data) = &self.winit_data else {
            return;
        };
        let cursor = self.drawn_cursors.cursor_at(self.cursor_position);
        if cursor == self.applied_cursor.as_ref() {
            return;
        }

        let window = &winit_data.window;
        window.set_cursor_visible(cursor != Some(&Cursor::Hidden));
        match cursor {
            None => window.set_cursor(CursorIcon::Default.into()),
            Some(Cursor::Named(icon)) => window.set_cursor((*icon).into()),
            Some(Cursor::Hidden) => {}
            Some(Cursor::Custom {
                rgba,
                width,
                height,
                hotspot: [hotspot_x, hotspot_y],
                // winit takes image px as is
                scale: _,
            }) => match CustomCursorSource::from_rgba(
                rgba.to_vec(),
                *width,
                *height,
                *hotspot_x,
                *hotspot_y,
            )
            .map_err(|err| err.to_string())
            .and_then(|source| {
                event_loop
                    .create_custom_cursor(source)
                    .map_err(|err| err.to_string())
            }) {
                Ok(cursor) => window.set_cursor(cursor.into()),
                Err(err) => {
                    dbg!("Bad custom cursor:", err);
                }
            },
        }
        self.applied_cursor = cursor.cloned();
    }

    fn send_mouse_event(
        event_queue: &calloop::channel::Sender<UIEvent>,
        cursor_position: [f64; 2],
        kind: MouseEventKind,
    ) {
        event_queue
            .send(UIEvent::Mouse(MouseEvent {
                position: cursor_position,
                kind,
                time_msec: None,
            }))
            .unwrap();
    }
}

fn button_kind(
    state: winit::event::ElementState,
    button: winit::event::ButtonSource,
) -> Option<MouseEventKind> {
    // REVIEW: (claude) evdev codes from linux/input-event-codes.h
    let button = match button.mouse_button() {
        Some(winit::event::MouseButton::Left) => 0x110,
        Some(winit::event::MouseButton::Right) => 0x111,
        Some(winit::event::MouseButton::Middle) => 0x112,
        Some(winit::event::MouseButton::Forward) => 0x115,
        Some(winit::event::MouseButton::Back) => 0x116,
        _ => return None,
    };
    Some(MouseEventKind::Button {
        button,
        pressed: state.is_pressed(),
    })
}

fn scroll_kind(delta: winit::event::MouseScrollDelta) -> Option<MouseEventKind> {
    // REVIEW: (claude) winit's positive is up/left while Wayland's is down/right, so negate
    match delta {
        winit::event::MouseScrollDelta::LineDelta(x, y) => Some(MouseEventKind::Scroll {
            // 15px per notch, like most compositors
            delta: [f64::from(-x) * 15., f64::from(-y) * 15.],
            v120: Some([(-x * 120.) as i32, (-y * 120.) as i32]),
            stop: [false; 2],
        }),
        winit::event::MouseScrollDelta::PixelDelta(pos) => Some(MouseEventKind::Scroll {
            delta: [-pos.x, -pos.y],
            v120: None,
            stop: [false; 2],
        }),
        _ => None,
    }
}
