use crate::runner::AppletRunner;
use calloop::LoopHandle;
use sonamu_sync::TrackedEncapsulatedLock;
use sonamu_ui::{
    DisplayNotification, DisplayProxy, DisplayRequest, Gpu, PointerGrab, UIDisplay,
    run_drm_display, ui_element::PrimitiveScene,
};
use std::{
    sync::{Arc, atomic::AtomicBool, mpsc},
    thread,
};
use wgpu::InstanceDescriptor;

/// For the runner/main app being UI'd to hold
///
/// Instead of storing the event receiver in here,
/// it is registered in the calloop.
pub(crate) struct UIHandle {
    pub is_running: Arc<AtomicBool>,
    pub ui_content: TrackedEncapsulatedLock<PrimitiveScene>,
    /// Wakes the display to redraw
    display_proxy: DisplayProxy,

    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}
impl UIHandle {
    /// On Vulkan, with the extensions for importing clients' dmabufs when supported
    fn request_device(
        adapter: &wgpu::Adapter,
        desc: &wgpu::DeviceDescriptor,
    ) -> (wgpu::Device, wgpu::Queue) {
        use wgpu::hal::api::Vulkan;
        // SAFETY: the callback only adds supported extensions
        let hal_device = unsafe { adapter.as_hal::<Vulkan>() }.and_then(|hal_adapter| {
            let extensions = crate::smithay::dmabuf::device_extensions(&hal_adapter)?;
            unsafe {
                hal_adapter.open_with_callback(
                    desc.required_features,
                    &desc.required_limits,
                    &desc.memory_hints,
                    Some(Box::new(move |args| args.extensions.extend(extensions))),
                )
            }
            .inspect_err(|err| eprintln!("Failed to open Vulkan for dmabufs: {err}"))
            .ok()
        });
        match hal_device {
            // SAFETY: opened from this adapter with `desc`'s features and limits
            Some(hal_device) => unsafe {
                adapter.create_device_from_hal::<Vulkan>(hal_device, desc)
            },
            None => pollster::block_on(adapter.request_device(desc)),
        }
        .unwrap()
    }

    /// `on_tty`: on the monitor directly, rather than in a window
    pub fn init_ui(
        initial_content: PrimitiveScene,
        runner_event_loop: &LoopHandle<'_, AppletRunner>,
        on_tty: bool,
    ) -> Self {
        let is_running = Arc::new(AtomicBool::new(true));
        let ui_content = TrackedEncapsulatedLock::new(initial_content);
        let (tx, rx) = calloop::channel::channel();

        // I don't think order matters,
        // but just in-case I should start listening before I make the display
        runner_event_loop
            .insert_source(rx, |event, &mut (), runner| {
                let calloop::channel::Event::Msg(event) = event else {
                    // the window was closed or quit, so `AppletRunner::run` returns
                    runner.loop_signal.stop();
                    return;
                };
                runner.handle_ui_event(event);
            })
            .unwrap();

        let (notification_tx, notification_rx) = calloop::channel::channel();
        runner_event_loop
            .insert_source(notification_rx, |event, &mut (), runner| {
                let calloop::channel::Event::Msg(notification) = event else {
                    return;
                };
                match notification {
                    DisplayNotification::Presented => runner.smithay_state.frame_presented(),
                    DisplayNotification::Info(info) => {
                        runner.smithay_state.set_display_info(info);
                        // the scale changes the logical size of the layer surfaces
                        runner.relayout();
                    }
                    DisplayNotification::Ime(ime) => runner.smithay_state.ime_input(&ime),
                    DisplayNotification::RelativeMotion(delta) => {
                        runner.relative_pointer_motion(delta);
                    }
                    DisplayNotification::Captured(captured) => {
                        runner.smithay_state.frames_captured(captured.as_ref());
                    }
                    DisplayNotification::Screens(screens) => {
                        runner.display_screens = Some(screens);
                        runner.update_screens();
                        runner.send_content_size();
                    }
                }
            })
            .unwrap();

        let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
        if on_tty {
            // drawing into the monitors' scanout buffers needs Vulkan (`sonamu_ui::dmabuf`)
            instance_descriptor.backends = wgpu::Backends::VULKAN;
        }
        let instance = wgpu::Instance::new(instance_descriptor);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .unwrap();
        let (device, queue) = Self::request_device(
            &adapter,
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                // WebGL doesn't support all of wgpu's features, so if
                // we're building for the web we'll have to disable some.
                required_limits: if cfg!(target_arch = "wasm32") {
                    wgpu::Limits::downlevel_webgl2_defaults()
                } else {
                    wgpu::Limits::default()
                },
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off, // Trace path
            },
        );

        let (proxy_tx, proxy_rx) = mpsc::channel();
        {
            let is_running = is_running.clone();
            let ui_content = ui_content.clone();
            let gpu = Gpu {
                device: device.clone(),
                queue: queue.clone(),
                instance,
                adapter,
            };
            let run_display = if on_tty {
                run_drm_display
            } else {
                UIDisplay::run_display
            };
            thread::spawn(move || {
                run_display(is_running, tx, ui_content, gpu, proxy_tx, notification_tx);
            });

            dbg!("Started UI thread");
        }

        Self {
            is_running,
            ui_content,
            display_proxy: proxy_rx.recv().unwrap(),

            device,
            queue,
        }
    }

    pub fn set_ui_content(&self, new_content: PrimitiveScene) {
        self.ui_content.set(new_content);
        (self.display_proxy)(DisplayRequest::Redraw);
    }

    /// Closes the display, which then ends the runner's event loop
    pub fn quit(&self) {
        self.is_running
            .store(false, std::sync::atomic::Ordering::Relaxed);
        // wakes the display so it notices
        (self.display_proxy)(DisplayRequest::Redraw);
    }

    /// `position` is in window px
    pub fn warp_pointer(&self, position: [f64; 2]) {
        (self.display_proxy)(DisplayRequest::WarpPointer(position));
    }

    /// The display sends back its next drawn frame (`DisplayNotification::Captured`)
    pub fn capture(&self) {
        (self.display_proxy)(DisplayRequest::Capture);
    }

    pub fn set_pointer_grab(&self, grab: PointerGrab) {
        (self.display_proxy)(DisplayRequest::SetPointerGrab(grab));
    }
}
