use crate::{
    msg_socket::MsgSocket, pluck_place::PluckPlace, smithay::SmithayState, ui_handle::UIHandle,
};
use calloop::{EventLoop, LoopHandle, LoopSignal};
use sonamu_common::{
    msg,
    sap::{
        client_handle::ClientHandle,
        detached::DetachedTree,
        packets::{StandardEvent, StandardRequest, StateKind, WlSurfaceId},
        raw_client_initializer::RawClientInitializer,
    },
};
use sonamu_ui::{
    PointerGrab,
    display_units::{DisplayContainerSize, ToPx},
    ui_element::{PrimitiveScene, Style, UIElement, UITree},
    ui_event::{KeyModifiers, KeySymbol, MouseEventKind, UIEvent},
};

/// The Singularity Applet Runner (SAR) is kind of just a wrapper around the Singularity UI.
/// The reason I want to abstract the UI is to make the recursive applet runners easier.
///
/// Using Singularity UI requires the user to be active,
/// but using Applet Runner is passive.
pub struct AppletRunner {
    ui_handle: UIHandle,
    pub(crate) root_client: ClientHandle,

    // // TODO: later
    // subsurfaces: SlotMap<TextureId, EncapsulatedLock<UIElement>>,
    pub(crate) smithay_state: SmithayState,

    window_size: DisplayContainerSize,
    /// The monitors' areas in the window, if the display has several
    pub(crate) display_screens: Option<Vec<[u32; 4]>>,
    /// Set by damage; the redraw happens once per event loop iteration
    pub(crate) needs_redraw: bool,
    /// What the display was last asked to do with the pointer
    pointer_grab: PointerGrab,
    /// Stops `run` once the display closes
    pub(crate) loop_signal: LoopSignal,

    /// Taken out of the root's tree by a pluck, until it's placed
    pub(crate) plucked: Option<DetachedTree>,
    pub(crate) pluck_place: PluckPlace,
    /// In window px, for drawing what's plucked under the pointer
    pub(crate) pointer_position: [f64; 2],
    /// Whether the applets act on a key. Other keys go straight to the focused wl client
    is_keybind: fn(&UIEvent) -> bool,
}
impl AppletRunner {
    fn new(
        event_loop: &LoopHandle<'static, Self>,
        loop_signal: LoopSignal,
        root_applet_initializer: Box<dyn RawClientInitializer>,
        is_keybind: fn(&UIEvent) -> bool,
        mut smithay_state: SmithayState,
        on_tty: bool,
    ) -> Self {
        let root_client = ClientHandle::spawn_new_client(
            root_applet_initializer,
            event_loop,
            |request, runner: &mut Self| runner.handle_client_request(request),
        );

        let ui_handle = UIHandle::init_ui(PrimitiveScene::new_empty(), event_loop, on_tty);
        smithay_state.init_dmabuf(&ui_handle.device);
        Self {
            ui_handle,
            smithay_state,
            root_client,
            // subsurfaces: SlotMap::with_key(),
            // idk if there's a way to actually get this
            window_size: DisplayContainerSize {
                width: 100,
                height: 100,
            },
            display_screens: None,
            needs_redraw: true,
            pointer_grab: PointerGrab::None,
            loop_signal,
            plucked: None,
            pluck_place: PluckPlace::Idle,
            pointer_position: [0.0, 0.0],
            is_keybind,
        }
    }

    /// Grabs the pointer while the wl surface under it has an active pointer constraint
    fn update_pointer_grab(&mut self) {
        let grab = self.smithay_state.pointer_grab();
        if grab != self.pointer_grab {
            // a lock ended, so put the pointer where the client asked
            if self.pointer_grab == PointerGrab::Locked
                && let Some(position) = self.smithay_state.lock_end_position.take()
            {
                self.ui_handle.warp_pointer(position);
            }
            self.pointer_grab = grab;
            self.ui_handle.set_pointer_grab(grab);
        }
    }

    /// Blocks until end.
    ///
    /// Runs on the monitor directly (as the actual compositor) when there's no display server to open a window in
    /// (no `WAYLAND_DISPLAY` or `DISPLAY`), or when `SONAMU_BACKEND=tty`; `SONAMU_BACKEND=winit` forces a window.
    /// `is_keybind`: whether the applets act on a key, rather than a focused wl client
    pub fn run(
        root_applet_initializer: Box<dyn RawClientInitializer>,
        is_keybind: fn(&UIEvent) -> bool,
    ) {
        let on_tty = match std::env::var("SONAMU_BACKEND").as_deref() {
            Ok("tty") => true,
            Ok("winit") => false,
            _ => {
                std::env::var_os("WAYLAND_DISPLAY").is_none()
                    && std::env::var_os("DISPLAY").is_none()
            }
        };
        let mut event_loop = EventLoop::try_new().unwrap();

        let smithay_state = SmithayState::new(&event_loop.handle());
        // For applets that launch wl clients (they set `WAYLAND_DISPLAY` to it).
        // Not `WAYLAND_DISPLAY` itself: Mesa's Vulkan device selection would connect here
        // before this loop runs, and wait forever (and in a window, winit connects to it).
        // For `sonamu-msg` (eg as the default file manager), in the clients' env
        let msg_socket = MsgSocket::bind(msg::socket_path(&smithay_state.socket_name))
            .inspect_err(|err| eprintln!("Failed to open the message socket: {err}"))
            .ok();
        // SAFETY: nothing else runs yet (the display, root and applet threads start below)
        unsafe {
            std::env::set_var(
                sonamu_common::WAYLAND_DISPLAY_ENV,
                &smithay_state.socket_name,
            );
            if let Some(msg_socket) = &msg_socket {
                std::env::set_var(msg::SOCKET_ENV, msg_socket.path());
            }
            // So clients pick Wayland over X11 (there's no Xwayland): Qt, Electron's `auto`,
            // and Nix-wrapped Electron/Chromium apps
            std::env::set_var("XDG_SESSION_TYPE", "wayland");
            std::env::set_var("ELECTRON_OZONE_PLATFORM_HINT", "auto");
            std::env::set_var("NIXOS_OZONE_WL", "1");
        }
        if on_tty {
            Self::export_session_env(&smithay_state.socket_name);
        }
        Self::run_startup();

        let mut runner = Self::new(
            &event_loop.handle(),
            event_loop.get_signal(),
            root_applet_initializer,
            is_keybind,
            smithay_state,
            on_tty,
        );
        if let Some(msg_socket) = msg_socket {
            msg_socket.insert(&event_loop.handle());
        }

        dbg!("Will run event loop");

        event_loop
            .run(None, &mut runner, |runner| {
                // Run after each batch of events, so multiple damages become one redraw
                if std::mem::take(&mut runner.needs_redraw) {
                    runner.redraw_ui();
                }
                // constraints change on client requests and pointer focus changes
                runner.update_pointer_grab();
                // events sent outside client dispatch (eg frame callbacks after a present)
                // only reach clients once flushed
                runner.smithay_state.flush_clients();
            })
            .unwrap();
    }

    /// For services D-Bus starts, eg xdg-desktop-portal-wlr (screen sharing) connects here
    /// and xdg-desktop-portal reads `sonamu-portals.conf` (see `assets/portal`).
    /// Only as the actual compositor, since in a window it would take over the host's session.
    fn export_session_env(socket_name: &std::ffi::OsStr) {
        let mut wayland_display = std::ffi::OsString::from("WAYLAND_DISPLAY=");
        wayland_display.push(socket_name);
        let result = std::process::Command::new("dbus-update-activation-environment")
            .arg("--systemd")
            .arg(wayland_display)
            .arg("XDG_CURRENT_DESKTOP=sonamu")
            // so programs D-Bus starts can reach `sonamu-msg`'s socket
            .arg(msg::SOCKET_ENV)
            // so they prefer Wayland too (set in `run`)
            .args([
                "XDG_SESSION_TYPE",
                "ELECTRON_OZONE_PLATFORM_HINT",
                "NIXOS_OZONE_WL",
            ])
            .status();
        if !result.as_ref().is_ok_and(std::process::ExitStatus::success) {
            eprintln!("Failed to export the session env, so portals may not work: {result:?}");
        }
    }

    /// Runs the `startup` commands from the settings here (eg dms: the bar, launcher, etc)
    fn run_startup() {
        for startup in &sonamu_common::settings::get().startup {
            let result = sonamu_common::connect_wl_client(
                std::process::Command::new("sh").arg("-c").arg(startup),
            )
            .spawn();
            if let Err(err) = result {
                eprintln!("Failed to run {startup:?}: {err}");
            }
        }
    }

    /// Tells the root its size (the whole window), and the screens in it
    pub(crate) fn send_content_size(&mut self) {
        self.root_client
            .send_event(StandardEvent::UIEvent(UIEvent::WindowResized(
                self.window_size,
            )));
        self.send_screens();
    }

    /// The monitors, else the window split into `SONAMU_SCREENS` side by side (to try several in a window)
    pub(crate) fn update_screens(&mut self) {
        self.smithay_state.screens = self.display_screens.clone().unwrap_or_else(|| {
            let count = std::env::var("SONAMU_SCREENS")
                .ok()
                .and_then(|count| count.parse().ok())
                .unwrap_or(1u32)
                .max(1);
            let DisplayContainerSize { width, height } = self.window_size;
            (0..count)
                .map(|index| {
                    let x = width * index / count;
                    [x, 0, width * (index + 1) / count - x, height]
                })
                .collect()
        });
        // the wl output is only the main screen, so layer surfaces (eg dms' bar) are only there
        let [_, _, width, height] = self.smithay_state.screens[0];
        self.smithay_state
            .resize_output(DisplayContainerSize::new(width, height));
        self.smithay_state.arrange_layers();
        self.smithay_state.resize_fullscreen();
    }

    /// Tells the root the screens' areas: the main one less the layer surfaces' exclusive zones (it has the wl output)
    fn send_screens(&mut self) {
        let ([left, top], size) = self.smithay_state.content_area();
        let mut screens = self.smithay_state.screens.clone();
        // none before the first `WindowResized`
        if screens.is_empty() {
            return;
        }
        // the output's origin is the main screen's
        let [x, y, ..] = screens[0];
        screens[0] = [x + left.to_px(), y + top.to_px(), size.width, size.height];
        self.root_client.send_event(StandardEvent::Screens(screens));
    }

    pub(crate) fn handle_ui_event(&mut self, mut event: UIEvent) {
        if let UIEvent::Key {
            raw_keycode,
            pressed: false,
            ..
        } = event
        {
            // evdev -> xkb
            self.release_wl_key(raw_keycode + 8);
        }

        if let UIEvent::WindowResized(screen_size) = event {
            self.window_size = screen_size;
            self.update_screens();
            self.send_content_size();
            self.needs_redraw = true;
            return;
        }

        if let UIEvent::Mouse(mouse_event) = &mut event {
            self.pointer_position = mouse_event.position;
            // what's plucked follows the pointer
            if self.plucked.is_some() {
                self.needs_redraw = true;
            }
            // layer surfaces are above (or beside) the root
            // a click on the fullscreen toplevel's screen while another screen has the keyboard goes to the root,
            // which makes that screen active (#60)
            let activates = matches!(
                mouse_event.kind,
                MouseEventKind::Button { pressed: true, .. }
            ) && self.smithay_state.fullscreen().is_some()
                && self.smithay_state.wl_target != self.smithay_state.fullscreen()
                && self.on_fullscreen_screen(mouse_event.position);
            if self.layer_mouse(mouse_event) || (!activates && self.fullscreen_mouse(*mouse_event))
            {
                return;
            }
        }

        // a layer surface with the keyboard (eg a launcher) gets the keys
        if let UIEvent::Key {
            raw_keycode,
            pressed: true,
            ..
        } = event
            && self.layer_key_press(raw_keycode)
        {
            return;
        }

        // global keybinds
        if let UIEvent::Key {
            symbol: Some(KeySymbol::Char('p' | 'P')),
            modifiers:
                KeyModifiers {
                    alt: true,
                    ctrl: false,
                    logo: false,
                    ..
                },
            pressed: true,
            ..
        } = event
        {
            self.toggle_pluck_place();
            return;
        }

        // keybinds from the settings
        if let UIEvent::Key {
            symbol,
            modifiers,
            raw_keycode,
            pressed: true,
        } = event
        {
            let symbol = match symbol {
                Some(KeySymbol::Char(c)) => Some(c),
                _ => None,
            };
            let mods = [
                modifiers.ctrl,
                modifiers.alt,
                modifiers.shift,
                modifiers.logo,
            ];
            let settings = sonamu_common::settings::get();
            if let Some(keybind) = settings
                .keybinds
                .iter()
                .find(|keybind| keybind.matches(symbol, raw_keycode, mods))
            {
                keybind.run();
                return;
            }
        }

        // like other compositors, keys for a focused wl client go straight to it, so its
        // presses and releases stay in order (#56); keybinds still go through the applets
        if let UIEvent::Key {
            raw_keycode,
            pressed: true,
            ..
        } = event
            && self.smithay_state.wl_target.is_some()
            && !(self.is_keybind)(&event)
        {
            // evdev -> xkb
            self.press_wl_key(raw_keycode + 8);
            return;
        }

        self.root_client.send_event(StandardEvent::UIEvent(event));
    }

    /// TODO: take in client id
    pub(crate) fn handle_client_request(&mut self, request: StandardRequest) {
        match request {
            StandardRequest::Damage(StateKind::Surface) => {
                self.needs_redraw = true;
            }
            // the root draws its treeview into its surface
            StandardRequest::Damage(StateKind::Treeview) => {}
            StandardRequest::Quit => self.ui_handle.quit(),
            StandardRequest::WlKeyboardFocus(surface_id) => self.focus_wl_keyboard(surface_id),
            StandardRequest::Plucked(plucked) => self.plucked(plucked),
            StandardRequest::Placed(given_back) => self.placed(given_back),
            StandardRequest::RequestPluck(path) => self.pluck(Some(path)),
            StandardRequest::RequestPlace(path, position) => self.request_place(path, position),
            StandardRequest::GetSettings => self.root_client.send_event(
                StandardEvent::SettingsChanged(sonamu_common::settings::get()),
            ),
        }
    }

    fn redraw_ui(&mut self) {
        // background and bottom layers, the root's content in the area left by the exclusive zones
        // (or a fullscreen toplevel over the whole window instead of them),
        // then top (unless fullscreen) and overlay layers, then what's plucked
        // other screens still show the root
        let mut layers =
            if self.smithay_state.fullscreen().is_some() && self.smithay_state.screens.len() <= 1 {
                Vec::new()
            } else {
                let mut layers = self.smithay_state.layer_surface_elements(
                    true,
                    &self.ui_handle.device,
                    &self.ui_handle.queue,
                );
                // over the whole window; its screens leave the exclusive zones free
                layers.push(UITree::new_container_styled(
                    Style::absolute(
                        [0.0, 0.0],
                        [
                            self.window_size.width as f32,
                            self.window_size.height as f32,
                        ],
                    ),
                    vec![self.root_client.get_surface()],
                ));
                layers
            };
        if let (Some(surface_id), Some([x, y, width, height])) = (
            self.smithay_state.fullscreen(),
            self.smithay_state.fullscreen_screen(),
        ) {
            layers.push(UITree::leaf_styled(
                UIElement::Subsurface(surface_id.as_u64()),
                Style::absolute([x as f32, y as f32], [width as f32, height as f32]),
            ));
        }
        layers.extend(self.smithay_state.layer_surface_elements(
            false,
            &self.ui_handle.device,
            &self.ui_handle.queue,
        ));
        layers.extend(self.plucked_ghost());
        let content = UITree::new_container(layers);
        let scene = PrimitiveScene::from_ui_tree(content, self.window_size, |surface_id| {
            // NOTE: currently only handles subsurfaces being the wl surfaces
            let wl_surface_id = WlSurfaceId::from_u64(surface_id);
            self.smithay_state
                .get_wl_surface_as_element(
                    wl_surface_id,
                    &self.ui_handle.device,
                    &self.ui_handle.queue,
                )
                .unwrap_or_else(|| {
                    UITree::leaf(UIElement::Backgrounded(sonamu_ui::color::Color::RED))
                })
        });
        self.smithay_state.set_tile_areas(
            scene
                .subsurface_areas
                .iter()
                .map(|&(surface_id, area)| (WlSurfaceId::from_u64(surface_id), area)),
        );
        self.ui_handle.set_ui_content(scene);
        if self.smithay_state.wants_capture() {
            self.ui_handle.capture();
        }
    }
}
