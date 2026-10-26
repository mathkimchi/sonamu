//! an example of how to make a basic client with sttk,
//! as well as code to actually run it

use calloop::{EventLoop, channel::Sender};
use singularity_common::sap::{
    packets::{StandardEvent, StandardRequest, StateKind, Treeview},
    raw_client_initializer::RawClientInitializer,
    shared_state::{SharedState, StatePublisher},
};
use singularity_sar::runner::AppletRunner;
use sonamu_ui::{
    ui_element::{CharGrid, UITree},
    ui_event::{self, KeyModifiers, KeySymbol, KeyTrait},
};

struct BasicApp {
    content_str: String,
    content: StatePublisher<UITree>,
    request_sender: Sender<StandardRequest>,
}
impl BasicApp {
    fn handle_key_press(&mut self, key: KeySymbol, mods: KeyModifiers) {
        match (key, mods) {
            (KeySymbol::Char('Q'), KeyModifiers::CTRL_SHIFT) => {
                self.request_sender.send(StandardRequest::Quit).unwrap();
            }
            (KeySymbol::Backspace, _) => {
                self.content_str.pop();
                self.content.damage();
            }
            _ => {
                match key.to_char() {
                    // \b is not supported by rust bruh
                    Some('\x08') => {}
                    Some(key_char) => {
                        self.content_str.push(key_char);
                        self.content.damage();
                    }
                    _ => (),
                }
            }
        }
    }
}

struct BasicInitializer;
impl RawClientInitializer for BasicInitializer {
    fn init(
        // smth smth box needs to know size
        self: Box<Self>,
        content: SharedState<UITree>,
        // this client has no treeview
        _treeview: SharedState<Treeview>,
        event_queue: calloop::channel::Channel<StandardEvent>,
        // yeah, ik the naming is inconsistent bc I'm not saying "event_receiver" or "event_rx", but it's calm
        // (I am really trying to convince myself this is fine, I am the strawman)
        request_sender: Sender<StandardRequest>,
    ) {
        let mut event_loop = EventLoop::try_new().unwrap();

        let mut app = BasicApp {
            content_str: "placeholder".to_string(),
            content: StatePublisher::new(content),
            request_sender,
        };

        event_loop
            .handle()
            .insert_source(
                event_queue,
                |event, &mut (), app_state: &mut BasicApp| match event {
                    calloop::channel::Event::Msg(StandardEvent::UIEvent(
                        ui_event::UIEvent::Key {
                            symbol: Some(key),
                            modifiers,
                            ..
                        },
                    )) => {
                        app_state.handle_key_press(key, modifiers);
                    }
                    calloop::channel::Event::Msg(StandardEvent::DamageAck(StateKind::Surface)) => {
                        app_state.content.ack();
                    }
                    calloop::channel::Event::Msg(_) => {}
                    calloop::channel::Event::Closed => {
                        println!("Goodbye!");
                    }
                },
            )
            .unwrap();

        event_loop
            .run(None, &mut app, |app| {
                app.content.flush(
                    || CharGrid::from(app.content_str.as_str()).element().into(),
                    &app.request_sender,
                );
            })
            .unwrap();
    }
}

pub fn main() {
    AppletRunner::run(Box::new(BasicInitializer), |_| false);
}
