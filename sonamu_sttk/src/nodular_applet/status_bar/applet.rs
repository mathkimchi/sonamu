//! The status bar as its own client, beside the root (not in the tree), like a layer surface (eg dms' bar): the runner
//! keeps its height free atop each screen, draws it over the root and gives it the input in its region (`BarInput`)

use super::{BarAction, BarState, StatusBar, height};
use calloop::{
    EventLoop,
    channel::{Channel, Sender},
    timer::{TimeoutAction, Timer},
};
use sonamu_common::{
    command::{self, Value},
    sap::{
        packets::{self, BarInput, BarRequest, StandardEvent, StateKind, Treeview},
        raw_client_initializer::RawClientInitializer,
        shared_state::{SharedState, StatePublisher},
    },
    settings,
};
use sonamu_ui::{
    ui_element::{Style, UITree},
    ui_event::{KeySymbol, MouseEvent, MouseEventKind, UIEvent},
};
use std::time::Duration;

/// How often the clock and the popping up notifications are checked
const TICK: Duration = Duration::from_millis(200);

pub struct StatusBarApplet {
    bar: StatusBar,
    surface: StatePublisher<UITree>,
    request_sender: Sender<BarRequest>,
    /// `[x, y, width, height]` in the window, from the runner: the bar is atop each
    screens: Vec<[u32; 4]>,
    /// From the root; the active screen also follows presses on the bar
    state: packets::BarState,
    /// What the runner was last told
    input: BarInput,
}

impl StatusBarApplet {
    pub fn get_initializer() -> Box<dyn RawClientInitializer<BarRequest>> {
        struct Initializer;
        impl RawClientInitializer<BarRequest> for Initializer {
            fn init(
                self: Box<Self>,
                surface: SharedState<UITree>,
                _treeview: SharedState<Treeview>,
                event_queue: Channel<StandardEvent>,
                request_sender: Sender<BarRequest>,
            ) {
                let mut event_loop = EventLoop::try_new().unwrap();
                let handle = event_loop.handle();
                handle
                    .insert_source(
                        event_queue,
                        |event, &mut (), applet: &mut StatusBarApplet| {
                            if let calloop::channel::Event::Msg(event) = event {
                                applet.handle_event(event);
                            }
                        },
                    )
                    .unwrap();
                handle
                    .insert_source(
                        Timer::immediate(),
                        |_, &mut (), applet: &mut StatusBarApplet| {
                            if applet.bar.tick() {
                                applet.surface.damage();
                            }
                            TimeoutAction::ToDuration(TICK)
                        },
                    )
                    .unwrap();
                let (settings_tx, settings_rx) = calloop::channel::channel();
                settings::subscribe(settings_tx);
                handle
                    .insert_source(settings_rx, |_, &mut (), applet: &mut StatusBarApplet| {
                        applet.surface.damage();
                    })
                    .unwrap();
                // redraws when the bar's system readings change
                let (ping, source) = calloop::ping::make_ping().unwrap();
                handle
                    .insert_source(source, |(), &mut (), applet: &mut StatusBarApplet| {
                        applet.surface.damage();
                    })
                    .unwrap();
                let mut applet = StatusBarApplet {
                    bar: StatusBar::new(move || ping.ping()),
                    surface: StatePublisher::new(surface),
                    request_sender,
                    screens: Vec::new(),
                    state: packets::BarState::default(),
                    input: BarInput::default(),
                };
                event_loop
                    .run(None, &mut applet, StatusBarApplet::flush)
                    .unwrap();
            }
        }
        Box::new(Initializer)
    }

    fn handle_event(&mut self, event: StandardEvent) {
        match event {
            StandardEvent::UIEvent(UIEvent::Mouse(event)) => self.handle_mouse(event),
            StandardEvent::UIEvent(UIEvent::Key {
                symbol: Some(key),
                pressed: true,
                ..
            }) => self.handle_key(key),
            StandardEvent::Screens(screens) => {
                self.screens = screens;
                self.state.active_screen = self
                    .state
                    .active_screen
                    .min(self.screens.len().saturating_sub(1));
                self.surface.damage();
            }
            StandardEvent::BarState(state) => {
                self.state = state;
                self.surface.damage();
            }
            StandardEvent::Command { name, args, reply } => {
                let _ = reply.send(self.command(&name, &args));
                self.surface.damage();
            }
            StandardEvent::DamageAck(StateKind::Surface) => self.surface.ack(),
            _ => {}
        }
    }

    /// `sonamu.bar_layout(left, right)` (its effect limit is checked by the root)
    fn command(&mut self, name: &str, args: &[Value]) -> command::CommandResult {
        if name != "bar_layout" {
            return Err(command::unknown(name));
        }
        let left = command::strings_arg(args, 0)?;
        let right = command::strings_arg(args, 1)?;
        self.bar.set_layout(&left, &right)?;
        Ok(Value::Nil)
    }

    fn bar_state(&self) -> BarState {
        BarState {
            treeview_shown: self.state.treeview_shown,
        }
    }

    /// The screen at `[x, y]` (in the window)
    fn screen_at(&self, [x, y]: [f64; 2]) -> Option<usize> {
        self.screens.iter().position(|&[sx, sy, w, h]| {
            let [sx, sy, w, h] = [sx, sy, w, h].map(f64::from);
            (sx..sx + w).contains(&x) && (sy..sy + h).contains(&y)
        })
    }

    /// From the runner: in its region, a press elsewhere while a popup is open, or the pointer leaving the region
    fn handle_mouse(&mut self, mut event: MouseEvent) {
        let pressed = matches!(event.kind, MouseEventKind::Button { pressed: true, .. });
        let under = self.screen_at(event.position);
        // a press on another screen's bar makes it active (its popups show there)
        if pressed
            && let Some(index) = under
            && event.position[1] < f64::from(self.screens[index][1] + height())
            && index != self.state.active_screen
        {
            self.state.active_screen = index;
            let _ = self.request_sender.send(BarRequest::ActivateScreen(index));
        }
        let Some(&[x, y, width, _]) = self.screens.get(self.state.active_screen) else {
            return;
        };
        // in the active screen's coords; elsewhere (eg another screen's bar) only clears its hover
        event.position = if under == Some(self.state.active_screen) || pressed {
            [
                event.position[0] - f64::from(x),
                event.position[1] - f64::from(y),
            ]
        } else {
            [-1.0, -1.0]
        };
        let palette = settings::get().palette;
        let response = self
            .bar
            .handle_mouse(&self.bar_state(), &palette, &event, width as f32);
        if response.damaged {
            self.surface.damage();
        }
        if let Some(BarAction::ToggleTreeview) = response.action {
            let _ = self.request_sender.send(BarRequest::ToggleTreeview);
        }
    }

    /// While the launcher takes keys, or a bare Esc while a popup is open (it closes)
    fn handle_key(&mut self, key: KeySymbol) {
        let changed = if self.bar.wants_keys() {
            self.bar.handle_key(key)
        } else if matches!(key, KeySymbol::Escape) && self.bar.has_popup() {
            self.bar.close_popup();
            true
        } else {
            false
        };
        if changed {
            self.surface.damage();
        }
    }

    /// Publishes the bars (and the active screen's popups) if damaged, and the input region if it changed
    fn flush(&mut self) {
        let settings = settings::get();
        let state = self.bar_state();
        let Self {
            bar,
            screens,
            state: packets::BarState { active_screen, .. },
            ..
        } = self;
        self.surface.flush(
            || {
                UITree::new_container(
                    screens
                        .iter()
                        .enumerate()
                        .map(|(index, &[x, y, width, height])| {
                            UITree::new_container_styled(
                                Style::absolute(
                                    [x as f32, y as f32],
                                    [width as f32, height as f32],
                                ),
                                bar.draw(&settings, &state, width as f32, index == *active_screen),
                            )
                        })
                        .collect(),
                )
            },
            &self.request_sender,
        );
        let input = self.input_region(&settings.palette);
        if input != self.input {
            let _ = self.request_sender.send(BarRequest::Input(input.clone()));
            self.input = input;
        }
    }

    /// Every screen's bar row, and the active one's popup and notifications popping up
    fn input_region(&self, palette: &settings::Palette) -> BarInput {
        let mut areas: Vec<[f32; 4]> = self
            .screens
            .iter()
            .map(|&[x, y, width, _]| [x as f32, y as f32, width as f32, height() as f32])
            .collect();
        if let Some(&[x, y, width, _]) = self.screens.get(self.state.active_screen) {
            areas.extend(
                self.bar
                    .popup_areas(&self.bar_state(), palette, width as f32)
                    .into_iter()
                    .map(|[ax, ay, aw, ah]| [x as f32 + ax, y as f32 + ay, aw, ah]),
            );
        }
        BarInput {
            height: height(),
            areas,
            popup: self.bar.has_popup(),
            keys: self.bar.wants_keys(),
        }
    }
}
