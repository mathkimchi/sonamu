//! A bulk of this file originates from [Smithay's Client Toolkit example](https://github.com/Smithay/client-toolkit/blob/master/examples/simple_layer.rs).

use crate::ui_element::UIElement;
use font_kit::{font::Font, source::SystemSource};
use smithay_client_toolkit::{
    activation::{ActivationState, RequestData},
    compositor::CompositorState,
    output::OutputState,
    reexports::{
        calloop::{EventLoop, LoopHandle},
        calloop_wayland_source::WaylandSource,
    },
    registry::RegistryState,
    seat::SeatState,
    shell::{
        WaylandSurface,
        xdg::{
            XdgShell,
            window::{Window, WindowDecorations},
        },
    },
    shm::{
        Shm,
        slot::{Buffer, SlotPool},
    },
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use ui_event::{KeyModifiers, UIEvent};
use wayland_client::{
    Connection,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_pointer},
};

pub const FRAME_RATE: f32 = 30.;
pub const FRAME_DELTA_SECONDS: f32 = 1. / FRAME_RATE;

pub struct UIDisplay {
    /// TODO: use `EncapsulatedLock`?
    root_element: Arc<Mutex<UIElement>>,

    ui_event_queue: Arc<Mutex<Vec<UIEvent>>>,

    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    xdg_activation: Option<ActivationState>,

    /// REVIEW: Use `Arc<Mutex<bool>>`, `Arc<RwLock<bool>>`, or `Arc<AtomicBool>`?
    is_running: Arc<AtomicBool>,
    first_configure: bool,
    pool: SlotPool,
    width: u32,
    height: u32,
    _shift: Option<u32>,
    buffer: Option<Buffer>,
    window: Window,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    key_modifiers: KeyModifiers,
    pointer: Option<wl_pointer::WlPointer>,
    loop_handle: LoopHandle<'static, UIDisplay>,
    font: Font,
}
impl UIDisplay {
    /// Returns when display is closed.
    pub fn run_display(
        root_element: Arc<Mutex<UIElement>>,
        ui_event_queue: Arc<Mutex<Vec<UIEvent>>>,
        is_running: Arc<AtomicBool>,
    ) {
        // All Wayland apps start by connecting the compositor (server).
        let conn = Connection::connect_to_env().unwrap();

        // Enumerate the list of globals to get the protocols the server implements.
        let (globals, event_queue) = registry_queue_init(&conn).unwrap();
        let qh = event_queue.handle();
        let mut event_loop: EventLoop<UIDisplay> =
            EventLoop::try_new().expect("Failed to initialize the event loop!");
        let loop_handle = event_loop.handle();
        WaylandSource::new(conn.clone(), event_queue)
            .insert(loop_handle)
            .unwrap();

        // The compositor (not to be confused with the server which is commonly called the compositor) allows
        // configuring surfaces to be presented.
        let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor not available");
        // For desktop platforms, the XDG shell is the standard protocol for creating desktop windows.
        let xdg_shell = XdgShell::bind(&globals, &qh).expect("xdg shell is not available");
        // Since we are not using the GPU in this example, we use wl_shm to allow software rendering to a buffer
        // we share with the compositor process.
        let shm = Shm::bind(&globals, &qh).expect("wl shm is not available.");
        // If the compositor supports xdg-activation it probably wants us to use it to get focus
        let xdg_activation = ActivationState::bind(&globals, &qh).ok();

        // A window is created from a surface.
        let surface = compositor.create_surface(&qh);
        // And then we can create the window.
        let window = xdg_shell.create_window(surface, WindowDecorations::RequestServer, &qh);
        // Configure the window, this may include hints to the compositor about the desired minimum size of the
        // window, app id for WM identification, the window title, etc.
        window.set_title("A wayland window");
        // GitHub does not let projects use the `org.github` domain but the `io.github` domain is fine.
        window.set_app_id("io.github.smithay.client-toolkit.SimpleWindow");
        window.set_min_size(Some((256, 256)));
        window.set_maximized();

        // In order for the window to be mapped, we need to perform an initial commit with no attached buffer.
        // For more info, see WaylandSurface::commit
        //
        // The compositor will respond with an initial configure that we can then use to present to the window with
        // the correct options.
        window.commit();

        // To request focus, we first need to request a token
        if let Some(activation) = xdg_activation.as_ref() {
            activation.request_token(
                &qh,
                RequestData {
                    seat_and_serial: None,
                    surface: Some(window.wl_surface().clone()),
                    app_id: Some(String::from(
                        "io.github.smithay.client-toolkit.SimpleWindow",
                    )),
                },
            )
        }

        // We don't know how large the window will be yet, so lets assume the minimum size we suggested for the
        // initial memory allocation.
        let pool = SlotPool::new(256 * 256 * 4, &shm).expect("Failed to create pool");

        let mut ui_display = UIDisplay {
            root_element,
            ui_event_queue,

            // Seats and outputs may be hotplugged at runtime, therefore we need to setup a registry state to
            // listen for seats and outputs.
            registry_state: RegistryState::new(&globals),
            seat_state: SeatState::new(&globals, &qh),
            output_state: OutputState::new(&globals, &qh),
            shm,
            xdg_activation,

            is_running,
            first_configure: true,
            pool,
            width: 256,
            height: 256,
            _shift: None,
            buffer: None,
            window,
            keyboard: None,
            key_modifiers: KeyModifiers::default(),
            pointer: None,
            loop_handle: event_loop.handle(),
            font: SystemSource::new()
                .select_best_match(
                    &[font_kit::family_name::FamilyName::Monospace],
                    font_kit::properties::Properties::new()
                        .weight(font_kit::properties::Weight::MEDIUM),
                )
                .unwrap()
                .load()
                .unwrap(),
        };

        // We don't draw immediately, the configure will notify us when to first draw.
        while ui_display.is_running.load(Ordering::Relaxed) {
            event_loop
                .dispatch(
                    Duration::from_secs_f32(FRAME_DELTA_SECONDS),
                    &mut ui_display,
                )
                .unwrap();
        }
        println!("Graciously ending display loop.");
    }
}
mod drawing_impls {
    use super::UIDisplay;
    use crate::{
        color::Color,
        display_units::{DisplayArea, DisplayCoord, DisplaySize, DisplayUnits},
        ui_element::{CharCell, UIElement},
    };
    use font_kit::{canvas::Canvas, font::Font};
    use raqote::{DrawOptions, DrawTarget, SolidSource, Source};
    use smithay_client_toolkit::shell::WaylandSurface;
    use wayland_client::{Connection, QueueHandle, protocol::wl_shm};

    // impl UIElement {
    //     fn fill_rect(canvas: &mut Canvas, area: DisplayArea, color: Color) {
    //         let mut pb = raqote::PathBuilder::new();
    //         pb.rect(
    //             area.0.x.pixels(dt.width()) as f32,
    //             area.0.y.pixels(dt.height()) as f32,
    //             area.size().width.pixels(dt.width()) as f32,
    //             area.size().height.pixels(dt.height()) as f32,
    //         );
    //         let path = pb.finish();
    //         dt.fill(&path, &Source::Solid(color.into()), &DrawOptions::new());
    //     }

    //     fn draw(&self, dt: &mut DrawTarget, container_area: DisplayArea, font: &Font) {
    //         /// think this is height in pixels
    //         const FONT_SIZE: i32 = 12;

    //         match self {
    //             UIElement::Container(children) => {
    //                 for ui_element in children {
    //                     // draw the inner widget
    //                     ui_element.draw(dt, container_area, font);
    //                 }
    //             }
    //             UIElement::Contained(inner_element, area) => {
    //                 inner_element.draw(dt, area.map_onto(container_area), font);
    //             }
    //             // FIXME: there are weird border lines
    //             UIElement::Bordered(inner_element, border_color) => {
    //                 // draw the border
    //                 let border_path = {
    //                     let mut pb = raqote::PathBuilder::new();
    //                     // top
    //                     pb.rect(
    //                         container_area.0.x.pixels(dt.width()) as f32,
    //                         container_area.0.y.pixels(dt.height()) as f32,
    //                         container_area.size().width.pixels(dt.width()) as f32,
    //                         1.,
    //                     );
    //                     // bot
    //                     pb.rect(
    //                         container_area.0.x.pixels(dt.width()) as f32 - 1.,
    //                         container_area.1.y.pixels(dt.height()) as f32 - 1.,
    //                         container_area.size().width.pixels(dt.width()) as f32 + 1.,
    //                         // NOTE: ^ the bottom right pixel is gone without this + 1. (both are needed for some reason)
    //                         1.,
    //                     );
    //                     // left
    //                     pb.rect(
    //                         container_area.0.x.pixels(dt.width()) as f32,
    //                         container_area.0.y.pixels(dt.height()) as f32,
    //                         1.,
    //                         container_area.size().height.pixels(dt.height()) as f32,
    //                     );
    //                     // right
    //                     pb.rect(
    //                         container_area.1.x.pixels(dt.width()) as f32 - 1.,
    //                         container_area.0.y.pixels(dt.height()) as f32 - 1.,
    //                         1.,
    //                         container_area.size().height.pixels(dt.height()) as f32 + 1.,
    //                         // NOTE: ^ the bottom right pixel is gone without this + 1. (both are needed for some reason)
    //                     );
    //                     pb.finish()
    //                 };
    //                 dt.fill(
    //                     &border_path,
    //                     &Source::Solid((*border_color).into()),
    //                     &DrawOptions::new(),
    //                 );

    //                 let inner_area = DisplayArea(
    //                     DisplayCoord::new(1.into(), 1.into()),
    //                     DisplayCoord::new(
    //                         DisplayUnits::from_mixed(-1, 1.0),
    //                         DisplayUnits::from_mixed(-1, 1.0),
    //                     ),
    //                 )
    //                 .map_onto(container_area);

    //                 // dbg!(&container_area);
    //                 // dbg!(&container_area.size());
    //                 // dbg!(&inner_area);

    //                 // draw the inner widget
    //                 inner_element.draw(dt, inner_area, font);
    //             }
    //             UIElement::Backgrounded(inner_element, bg_color) => {
    //                 // clear the inside of the border
    //                 Self::fill_rect(dt, container_area, *bg_color);

    //                 // draw the inner widget
    //                 inner_element.draw(dt, container_area, font);
    //             }
    //             UIElement::Text(text) => {
    //                 // FIXME: doesn't work with space
    //                 dt.draw_text(
    //                     font,
    //                     FONT_SIZE as f32,
    //                     text,
    //                     DisplayCoord::new(
    //                         container_area.0.x,
    //                         container_area.0.y + FONT_SIZE.into(),
    //                     )
    //                     .into_raqote_point(dt),
    //                     &Source::Solid(SolidSource {
    //                         r: 0,
    //                         g: 0xFF,
    //                         b: 0xFF,
    //                         a: 0xFF,
    //                     }),
    //                     &DrawOptions::new(),
    //                 );
    //             }
    //             UIElement::CharGrid(char_grid) => {
    //                 for (line_index, line) in char_grid.content.iter().enumerate() {
    //                     for (col_index, CharCell { character, fg, bg }) in line.iter().enumerate() {
    //                         let top_left = DisplayCoord::new(
    //                             container_area.0.x
    //                                 + DisplayUnits::Pixels(FONT_SIZE / 2 * (col_index as i32)),
    //                             container_area.0.y
    //                                 + DisplayUnits::Pixels(FONT_SIZE * (line_index as i32) + 1),
    //                         );

    //                         if !container_area.contains(top_left, [dt.width(), dt.height()]) {
    //                             // FIXME: not completely foolproof -- main purpose is just optimization
    //                             continue;
    //                         }

    //                         let bot_left = DisplayCoord::new(
    //                             container_area.0.x
    //                                 + DisplayUnits::Pixels(FONT_SIZE / 2 * (col_index as i32)),
    //                             container_area.0.y
    //                                 + DisplayUnits::Pixels(FONT_SIZE * (line_index + 1) as i32),
    //                         );

    //                         Self::fill_rect(
    //                             dt,
    //                             DisplayArea::from_corner_size(
    //                                 top_left,
    //                                 DisplaySize::new(
    //                                     (FONT_SIZE / 2 + 1).into(),
    //                                     (FONT_SIZE + 2).into(),
    //                                 ),
    //                             ),
    //                             *bg,
    //                         );

    //                         if character == &' ' {
    //                             continue;
    //                         }

    //                         dt.draw_text(
    //                             font,
    //                             FONT_SIZE as f32,
    //                             &character.to_string(),
    //                             // `start` is actually bottom left corner
    //                             bot_left.into_raqote_point(dt),
    //                             &raqote::Source::Solid((*fg).into()),
    //                             &DrawOptions::new(),
    //                         );
    //                     }
    //                 }
    //             }
    //             UIElement::Nothing => {}
    //         }
    //     }
    // }

    // impl UIDisplay {
    //     pub fn draw(&mut self, _conn: &Connection, qh: &QueueHandle<Self>) {
    //         let stride = self.width as i32 * 4;

    //         let buffer = self.buffer.get_or_insert_with(|| {
    //             self.pool
    //                 .create_buffer(
    //                     self.width as i32,
    //                     self.height as i32,
    //                     stride,
    //                     wl_shm::Format::Argb8888,
    //                 )
    //                 .expect("create buffer")
    //                 .0
    //         });

    //         let canvas = match self.pool.canvas(buffer) {
    //             Some(canvas) => canvas,
    //             None => {
    //                 // This should be rare, but if the compositor has not released the previous
    //                 // buffer, we need double-buffering.
    //                 let (second_buffer, canvas) = self
    //                     .pool
    //                     .create_buffer(
    //                         self.width as i32,
    //                         self.height as i32,
    //                         stride,
    //                         wl_shm::Format::Argb8888,
    //                     )
    //                     .expect("create buffer");
    //                 *buffer = second_buffer;
    //                 canvas
    //             }
    //         };

    //         // Draw to the window:
    //         // FIXME find an actual fix to the height difference
    //         if canvas.len() as u32 == 4 * self.width * self.height {
    //             let mut dt = DrawTarget::new(self.width as i32, self.height as i32);
    //             self.root_element
    //                 .lock()
    //                 .unwrap()
    //                 .draw(&mut dt, DisplayArea::FULL, &self.font);
    //             canvas.copy_from_slice(dt.get_data_u8());
    //         }

    //         // Damage the entire window
    //         self.window
    //             .wl_surface()
    //             .damage_buffer(0, 0, self.width as i32, self.height as i32);

    //         // Request our next frame
    //         self.window
    //             .wl_surface()
    //             .frame(qh, self.window.wl_surface().clone());

    //         // Attach and commit to present.
    //         buffer
    //             .attach_to(self.window.wl_surface())
    //             .expect("buffer attach");
    //         self.window.commit();
    //     }
    // }

    // /*
    impl UIElement {
        fn fill_rect(dt: &mut DrawTarget, area: DisplayArea, color: Color) {
            let mut pb = raqote::PathBuilder::new();
            pb.rect(
                area.0.x.pixels(dt.width()) as f32,
                area.0.y.pixels(dt.height()) as f32,
                area.size().width.pixels(dt.width()) as f32,
                area.size().height.pixels(dt.height()) as f32,
            );
            let path = pb.finish();
            dt.fill(&path, &Source::Solid(color.into()), &DrawOptions::new());
        }

        fn draw(&self, dt: &mut DrawTarget, container_area: DisplayArea, font: &Font) {
            /// think this is height in pixels
            const FONT_SIZE: i32 = 12;

            match self {
                UIElement::Container(children) => {
                    for ui_element in children {
                        // draw the inner widget
                        ui_element.draw(dt, container_area, font);
                    }
                }
                UIElement::Contained(inner_element, area) => {
                    inner_element.draw(dt, area.map_onto(container_area), font);
                }
                // FIXME: there are weird border lines
                UIElement::Bordered(inner_element, border_color) => {
                    // draw the border
                    let border_path = {
                        let mut pb = raqote::PathBuilder::new();
                        // top
                        pb.rect(
                            container_area.0.x.pixels(dt.width()) as f32,
                            container_area.0.y.pixels(dt.height()) as f32,
                            container_area.size().width.pixels(dt.width()) as f32,
                            1.,
                        );
                        // bot
                        pb.rect(
                            container_area.0.x.pixels(dt.width()) as f32 - 1.,
                            container_area.1.y.pixels(dt.height()) as f32 - 1.,
                            container_area.size().width.pixels(dt.width()) as f32 + 1.,
                            // NOTE: ^ the bottom right pixel is gone without this + 1. (both are needed for some reason)
                            1.,
                        );
                        // left
                        pb.rect(
                            container_area.0.x.pixels(dt.width()) as f32,
                            container_area.0.y.pixels(dt.height()) as f32,
                            1.,
                            container_area.size().height.pixels(dt.height()) as f32,
                        );
                        // right
                        pb.rect(
                            container_area.1.x.pixels(dt.width()) as f32 - 1.,
                            container_area.0.y.pixels(dt.height()) as f32 - 1.,
                            1.,
                            container_area.size().height.pixels(dt.height()) as f32 + 1.,
                            // NOTE: ^ the bottom right pixel is gone without this + 1. (both are needed for some reason)
                        );
                        pb.finish()
                    };
                    dt.fill(
                        &border_path,
                        &Source::Solid((*border_color).into()),
                        &DrawOptions::new(),
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
                    inner_element.draw(dt, inner_area, font);
                }
                UIElement::Backgrounded(inner_element, bg_color) => {
                    // clear the inside of the border
                    Self::fill_rect(dt, container_area, *bg_color);

                    // draw the inner widget
                    inner_element.draw(dt, container_area, font);
                }
                UIElement::Text(text) => {
                    // FIXME: doesn't work with space
                    dt.draw_text(
                        font,
                        FONT_SIZE as f32,
                        text,
                        DisplayCoord::new(
                            container_area.0.x,
                            container_area.0.y + FONT_SIZE.into(),
                        )
                        .into_raqote_point(dt),
                        &Source::Solid(SolidSource {
                            r: 0,
                            g: 0xFF,
                            b: 0xFF,
                            a: 0xFF,
                        }),
                        &DrawOptions::new(),
                    );
                }
                UIElement::CharGrid(char_grid) => {
                    for (line_index, line) in char_grid.content.iter().enumerate() {
                        for (col_index, CharCell { character, fg, bg }) in line.iter().enumerate() {
                            let top_left = DisplayCoord::new(
                                container_area.0.x
                                    + DisplayUnits::Pixels(FONT_SIZE / 2 * (col_index as i32)),
                                container_area.0.y
                                    + DisplayUnits::Pixels(FONT_SIZE * (line_index as i32) + 1),
                            );

                            if !container_area.contains(top_left, [dt.width(), dt.height()]) {
                                // FIXME: not completely foolproof -- main purpose is just optimization
                                continue;
                            }

                            let bot_left = DisplayCoord::new(
                                container_area.0.x
                                    + DisplayUnits::Pixels(FONT_SIZE / 2 * (col_index as i32)),
                                container_area.0.y
                                    + DisplayUnits::Pixels(FONT_SIZE * (line_index + 1) as i32),
                            );

                            Self::fill_rect(
                                dt,
                                DisplayArea::from_corner_size(
                                    top_left,
                                    DisplaySize::new(
                                        (FONT_SIZE / 2 + 1).into(),
                                        (FONT_SIZE + 2).into(),
                                    ),
                                ),
                                *bg,
                            );

                            if character == &' ' {
                                continue;
                            }

                            dt.draw_text(
                                font,
                                FONT_SIZE as f32,
                                &character.to_string(),
                                // `start` is actually bottom left corner
                                bot_left.into_raqote_point(dt),
                                &raqote::Source::Solid((*fg).into()),
                                &DrawOptions::new(),
                            );
                        }
                    }
                }
                UIElement::Nothing => {}
            }
        }
    }

    impl UIDisplay {
        pub fn draw(&mut self, _conn: &Connection, qh: &QueueHandle<Self>) {
            let stride = self.width as i32 * 4;

            let buffer = self.buffer.get_or_insert_with(|| {
                self.pool
                    .create_buffer(
                        self.width as i32,
                        self.height as i32,
                        stride,
                        wl_shm::Format::Argb8888,
                    )
                    .expect("create buffer")
                    .0
            });

            let canvas = match self.pool.canvas(buffer) {
                Some(canvas) => canvas,
                None => {
                    // This should be rare, but if the compositor has not released the previous
                    // buffer, we need double-buffering.
                    let (second_buffer, canvas) = self
                        .pool
                        .create_buffer(
                            self.width as i32,
                            self.height as i32,
                            stride,
                            wl_shm::Format::Argb8888,
                        )
                        .expect("create buffer");
                    *buffer = second_buffer;
                    canvas
                }
            };

            // Draw to the window:
            // FIXME find an actual fix to the height difference
            if canvas.len() as u32 == 4 * self.width * self.height {
                let mut dt = DrawTarget::new(self.width as i32, self.height as i32);
                self.root_element
                    .lock()
                    .unwrap()
                    .draw(&mut dt, DisplayArea::FULL, &self.font);
                canvas.copy_from_slice(dt.get_data_u8());
            }

            // Damage the entire window
            self.window
                .wl_surface()
                .damage_buffer(0, 0, self.width as i32, self.height as i32);

            // Request our next frame
            self.window
                .wl_surface()
                .frame(qh, self.window.wl_surface().clone());

            // Attach and commit to present.
            buffer
                .attach_to(self.window.wl_surface())
                .expect("buffer attach");
            self.window.commit();
        }
    }
    // */
}
mod ui_display_wayland_impls {
    use super::{
        UIDisplay,
        ui_event::{Key, KeyModifiers},
    };
    use crate::display_units::DisplayArea;
    use smithay_client_toolkit::{
        activation::{ActivationHandler, RequestData},
        compositor::CompositorHandler,
        delegate_activation, delegate_compositor, delegate_keyboard, delegate_output,
        delegate_pointer, delegate_registry, delegate_seat, delegate_shm, delegate_xdg_shell,
        delegate_xdg_window,
        output::{OutputHandler, OutputState},
        registry::{ProvidesRegistryState, RegistryState},
        registry_handlers,
        seat::{
            Capability, SeatHandler, SeatState,
            keyboard::{KeyboardHandler, Keysym, Modifiers, RawModifiers},
            pointer::{PointerEvent, PointerHandler},
        },
        shell::{
            WaylandSurface,
            xdg::window::{Window, WindowConfigure, WindowHandler},
        },
        shm::{Shm, ShmHandler},
    };
    use wayland_client::{
        Connection, QueueHandle,
        protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
    };

    impl CompositorHandler for UIDisplay {
        fn scale_factor_changed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _new_factor: i32,
        ) {
            // Not needed for this example.
        }

        fn transform_changed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _new_transform: wl_output::Transform,
        ) {
            // Not needed for this example.
        }

        fn frame(
            &mut self,
            conn: &Connection,
            qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _time: u32,
        ) {
            // Draw every frame
            // TODO: only draw when updated
            self.draw(conn, qh);
        }

        fn surface_enter(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _output: &wl_output::WlOutput,
        ) {
            // Not needed for this example.
        }

        fn surface_leave(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _surface: &wl_surface::WlSurface,
            _output: &wl_output::WlOutput,
        ) {
            // Not needed for this example.
        }
    }

    impl OutputHandler for UIDisplay {
        fn output_state(&mut self) -> &mut OutputState {
            &mut self.output_state
        }

        fn new_output(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }

        fn update_output(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }

        fn output_destroyed(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _output: wl_output::WlOutput,
        ) {
        }
    }

    impl WindowHandler for UIDisplay {
        fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
            self.is_running
                .store(false, std::sync::atomic::Ordering::Relaxed);
        }

        /// Called on first spawn and resize
        fn configure(
            &mut self,
            conn: &Connection,
            qh: &QueueHandle<Self>,
            _window: &Window,
            configure: WindowConfigure,
            _serial: u32,
        ) {
            self.buffer = None;
            self.width = configure.new_size.0.map(|v| v.get()).unwrap_or(256);
            self.height = configure.new_size.1.map(|v| v.get()).unwrap_or(256);

            self.ui_event_queue
                .lock()
                .unwrap()
                .push(super::ui_event::UIEvent::WindowResized([
                    self.width,
                    self.height,
                ]));

            // Initiate the first draw.
            if self.first_configure {
                self.first_configure = false;
                self.draw(conn, qh);
            }
        }
    }

    impl ActivationHandler for UIDisplay {
        type RequestData = RequestData;

        fn new_token(&mut self, token: String, _data: &Self::RequestData) {
            self.xdg_activation
                .as_ref()
                .unwrap()
                .activate::<UIDisplay>(self.window.wl_surface(), token);
        }
    }

    impl SeatHandler for UIDisplay {
        fn seat_state(&mut self) -> &mut SeatState {
            &mut self.seat_state
        }

        fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

        fn new_capability(
            &mut self,
            _conn: &Connection,
            qh: &QueueHandle<Self>,
            seat: wl_seat::WlSeat,
            capability: Capability,
        ) {
            if capability == Capability::Keyboard && self.keyboard.is_none() {
                println!("Set keyboard capability");
                let keyboard = self
                    .seat_state
                    .get_keyboard_with_repeat(
                        qh,
                        &seat,
                        None,
                        self.loop_handle.clone(),
                        Box::new(|_state, _wl_kbd, event| {
                            println!("Repeat: {:?} ", event);
                        }),
                    )
                    .expect("Failed to create keyboard");

                self.keyboard = Some(keyboard);
            }

            if capability == Capability::Pointer && self.pointer.is_none() {
                println!("Set pointer capability");
                let pointer = self
                    .seat_state
                    .get_pointer(qh, &seat)
                    .expect("Failed to create pointer");
                self.pointer = Some(pointer);
            }
        }

        fn remove_capability(
            &mut self,
            _conn: &Connection,
            _: &QueueHandle<Self>,
            _: wl_seat::WlSeat,
            capability: Capability,
        ) {
            if capability == Capability::Keyboard && self.keyboard.is_some() {
                println!("Unset keyboard capability");
                self.keyboard.take().unwrap().release();
            }

            if capability == Capability::Pointer && self.pointer.is_some() {
                println!("Unset pointer capability");
                self.pointer.take().unwrap().release();
            }
        }

        fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    }

    impl KeyboardHandler for UIDisplay {
        fn enter(
            &mut self,
            _: &Connection,
            _: &QueueHandle<Self>,
            _: &wl_keyboard::WlKeyboard,
            _surface: &wl_surface::WlSurface,
            _: u32,
            _: &[u32],
            _keysyms: &[Keysym],
        ) {
        }

        fn leave(
            &mut self,
            _: &Connection,
            _: &QueueHandle<Self>,
            _: &wl_keyboard::WlKeyboard,
            _surface: &wl_surface::WlSurface,
            _: u32,
        ) {
        }

        fn press_key(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _: &wl_keyboard::WlKeyboard,
            _: u32,
            event: Key,
        ) {
            if self.key_modifiers.caps_lock {
                // dbg when caps lock is on
                println!("key pressed in dbg mode (capslock)");
                dbg!(&event);
                dbg!(&event.keysym.key_char());
                dbg!(&self.key_modifiers);
            }
            self.ui_event_queue
                .lock()
                .unwrap()
                .push(super::ui_event::UIEvent::KeyPress(
                    event,
                    self.key_modifiers,
                ));
        }

        fn release_key(
            &mut self,
            _: &Connection,
            _: &QueueHandle<Self>,
            _: &wl_keyboard::WlKeyboard,
            _: u32,
            _event: Key,
        ) {
        }

        fn update_modifiers(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _keyboard: &wl_keyboard::WlKeyboard,
            _serial: u32,
            modifiers: Modifiers,
            _raw_modifiers: RawModifiers,
            _layout: u32,
        ) {
            self.key_modifiers = KeyModifiers::from(modifiers);
        }

        fn repeat_key(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _keyboard: &wl_keyboard::WlKeyboard,
            _serial: u32,
            _event: smithay_client_toolkit::seat::keyboard::KeyEvent,
        ) {
        }
    }

    impl PointerHandler for UIDisplay {
        fn pointer_frame(
            &mut self,
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
            _pointer: &wl_pointer::WlPointer,
            events: &[PointerEvent],
        ) {
            for event in events {
                // Ignore events for other surfaces
                if &event.surface != self.window.wl_surface() {
                    continue;
                }

                use smithay_client_toolkit::seat::pointer::PointerEventKind;
                match event.kind {
                    PointerEventKind::Enter { .. } => {
                        // println!("Pointer entered @{:?}", event.position);
                    }
                    PointerEventKind::Leave { .. } => {
                        // println!("Pointer left");
                    }
                    PointerEventKind::Motion { .. } => {}
                    PointerEventKind::Press { .. } => {
                        self.ui_event_queue.lock().unwrap().push(
                            super::ui_event::UIEvent::MousePress(
                                [
                                    [event.position.0 as u32, event.position.1 as u32],
                                    [self.width, self.height],
                                ],
                                DisplayArea::FULL,
                            ),
                        );

                        // println!("Press {:x} @ {:?}", button, event.position);
                        // self.shift = self.shift.xor(Some(0));
                    }
                    PointerEventKind::Release { .. } => {
                        // println!("Release {:x} @ {:?}", button, event.position);
                    }
                    PointerEventKind::Axis { .. } => {
                        // println!("Scroll H:{horizontal:?}, V:{vertical:?}");
                    }
                }
            }
        }
    }

    impl ShmHandler for UIDisplay {
        fn shm_state(&mut self) -> &mut Shm {
            &mut self.shm
        }
    }

    delegate_compositor!(UIDisplay);
    delegate_output!(UIDisplay);
    delegate_shm!(UIDisplay);

    delegate_seat!(UIDisplay);
    delegate_keyboard!(UIDisplay);
    delegate_pointer!(UIDisplay);

    delegate_xdg_shell!(UIDisplay);
    delegate_xdg_window!(UIDisplay);
    delegate_activation!(UIDisplay);

    delegate_registry!(UIDisplay);

    impl ProvidesRegistryState for UIDisplay {
        fn registry(&mut self) -> &mut RegistryState {
            &mut self.registry_state
        }
        registry_handlers![OutputState, SeatState,];
    }
}

pub mod ui_event {
    use smithay_client_toolkit::seat::keyboard;

    use crate::display_units::DisplayArea;

    /// TODO: not great that I am reexporting smithay's event, given that the goal is to be backend agnostic.
    /// I am doing it right now because I'd rather get something working sooner, even if I have to compromise a bit
    ///
    /// TODO: also, figure out a way to easily match keypresses and shortcuts
    ///
    /// TODO: figure out a standard way of "forwarding" events to child
    #[derive(Debug, Clone)]
    pub enum UIEvent {
        KeyPress(Key, KeyModifiers),
        WindowResized([u32; 2]),
        /// ([mouse location [x, y], window size [w h]], container)
        ///
        /// REVIEW: definitely redundant, but might be helpful?
        ///
        /// NOTE: container should always be FULL for the outermost, but is helpful when trying to forward it to children:
        /// the forwarded area should be: `child_area.map_onto(parent_area)`
        MousePress([[u32; 2]; 2], DisplayArea),
    }
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct KeyModifiers {
        pub ctrl: bool,
        pub alt: bool,
        pub shift: bool,
        pub caps_lock: bool,
        pub logo: bool,
        // pub num_lock: bool,
    }
    pub type Key = keyboard::KeyEvent;

    pub trait KeyTrait {
        fn to_alphabet(&self) -> Option<char>;
        fn to_digit(&self) -> Option<u8>;
        fn to_char(&self) -> Option<char>;
    }
    impl KeyTrait for Key {
        fn to_alphabet(&self) -> Option<char> {
            let c = self.to_char()?;
            if c.is_ascii() { Some(c) } else { None }
        }

        fn to_digit(&self) -> Option<u8> {
            let c = self.to_char()?;
            c.to_digit(10).map(|c| c as u8)
        }

        fn to_char(&self) -> Option<char> {
            if self.raw_code == 28 {
                // FIXME: I added this bc I thought ENTER had no char, but it is actually `\r` already.
                return Some('\n');
            }
            self.keysym.key_char()
        }
    }

    impl KeyModifiers {
        pub const NONE: Self = KeyModifiers {
            ctrl: false,
            alt: false,
            shift: false,
            caps_lock: false,
            logo: false,
            // num_lock: false,
        };

        pub const CTRL: Self = KeyModifiers {
            ctrl: true,
            alt: false,
            shift: false,
            caps_lock: false,
            logo: false,
            // num_lock: false,
        };

        pub const ALT: Self = KeyModifiers {
            ctrl: false,
            alt: true,
            shift: false,
            caps_lock: false,
            logo: false,
            // num_lock: false,
        };

        pub const SHIFT: Self = KeyModifiers {
            ctrl: false,
            alt: false,
            shift: true,
            caps_lock: false,
            logo: false,
            // num_lock: false,
        };

        pub const LOGO: Self = KeyModifiers {
            ctrl: false,
            alt: false,
            shift: false,
            caps_lock: false,
            logo: true,
            // num_lock: false,
        };

        pub const CTRL_SHIFT: Self = KeyModifiers::both(Self::CTRL, Self::SHIFT);

        /// For example, combine(CTRL, SHIFT) is CTRL_SHIFT
        pub const fn both(self, rhs: Self) -> Self {
            Self {
                ctrl: self.ctrl | rhs.ctrl,
                alt: self.alt | rhs.alt,
                shift: self.shift | rhs.shift,
                caps_lock: self.caps_lock | rhs.caps_lock,
                logo: self.logo | rhs.logo,
                // num_lock: self.num_lock | rhs.num_lock,
            }
        }
    }
    impl From<keyboard::Modifiers> for KeyModifiers {
        fn from(
            keyboard::Modifiers {
                ctrl,
                alt,
                shift,
                caps_lock,
                logo,
                num_lock: _,
            }: keyboard::Modifiers,
        ) -> Self {
            Self {
                ctrl,
                alt,
                shift,
                caps_lock,
                logo,
                // num_lock,
            }
        }
    }
    impl std::ops::BitOr for KeyModifiers {
        type Output = Self;

        fn bitor(self, rhs: Self) -> Self::Output {
            Self::both(self, rhs)
        }
    }
    impl std::ops::BitAnd for KeyModifiers {
        type Output = Self;

        fn bitand(self, rhs: Self) -> Self::Output {
            Self {
                ctrl: self.ctrl & rhs.ctrl,
                alt: self.alt & rhs.alt,
                shift: self.shift & rhs.shift,
                caps_lock: self.caps_lock & rhs.caps_lock,
                logo: self.logo & rhs.logo,
                // num_lock: self.num_lock & rhs.num_lock,
            }
        }
    }
}
