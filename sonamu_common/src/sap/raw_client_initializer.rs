use calloop::channel::{Channel, Sender};
use sonamu_ui::ui_element::UITree;

use crate::sap::{
    packets::{StandardEvent, StandardRequest, Treeview},
    shared_state::SharedState,
};

/// Look at 2026-07-30 DEVLOG for initial creation
///
/// Called "Raw" because expectation is to make a wrapper in sttk for less boilerplate
///
/// `Request` is what the client sends to whoever holds its `ClientHandle`
/// (the sar server takes `StandardRequest`, sttk nodes take their own superset).
pub trait RawClientInitializer<Request = StandardRequest>: Send {
    fn init(
        // smth smth box needs to know size
        self: Box<Self>,
        surface: SharedState<UITree>,
        treeview: SharedState<Treeview>,
        event_queue: Channel<StandardEvent>,
        // yeah, ik the naming is inconsistent bc I'm not saying "event_receiver" or "event_rx", but it's calm
        // (I am really trying to convince myself this is fine, I am the strawman)
        request_sender: Sender<Request>,
    );
}
