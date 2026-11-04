use crate::{
    sap::{
        detached::DetachedClient,
        packets::{StandardEvent, StateKind, Treeview},
        raw_client_initializer::RawClientInitializer,
        shared_state::SharedState,
    },
    utils::tree::world_tree::WorldTree,
};
use calloop::{Dispatcher, LoopHandle, channel::Channel};
use sonamu_ui::ui_element::{UIElement, UITree};
use std::{any::Any, thread};

/// ~~Implemented by Applet,~~ used by whoever holds a client (the sar server or an sttk node).
///
/// Dropping it closes the client's event queue, which should end the client.
pub struct ClientHandle {
    event_queue: calloop::channel::Sender<StandardEvent>,
    surface: SharedState<UITree>,
    treeview: SharedState<Treeview>,
    /// Takes the request channel back out of the holder's loop
    detach_requests: Box<dyn FnOnce() -> Box<dyn Any + Send>>,
}
impl ClientHandle {
    /// Runs the client on a new thread.
    /// Its requests are handled by `on_request` on `event_loop`.
    pub fn spawn_new_client<Request: Send + 'static, Data: 'static>(
        client_initializer: Box<dyn RawClientInitializer<Request>>,
        event_loop: &LoopHandle<'static, Data>,
        on_request: impl FnMut(Request, &mut Data) + 'static,
    ) -> Self {
        let surface = SharedState::new(StateKind::Surface, UITree::leaf(UIElement::Nothing));
        let treeview = SharedState::new(
            StateKind::Treeview,
            Treeview::new(WorldTree::new_base(String::new())),
        );
        let (event_tx, event_rx) = calloop::channel::channel();
        let (request_tx, request_rx) = calloop::channel::channel();

        // I don't think order matters,
        // but just in-case I should start listening before I make the client
        let handle = Self::attach(
            event_tx,
            surface.clone(),
            treeview.clone(),
            request_rx,
            event_loop,
            on_request,
        );

        thread::spawn(|| {
            client_initializer.init(surface, treeview, event_rx, request_tx);
        });

        handle
    }

    /// Starts handling a client's requests on `event_loop`
    pub(crate) fn attach<Request: Send + 'static, Data: 'static>(
        event_queue: calloop::channel::Sender<StandardEvent>,
        surface: SharedState<UITree>,
        treeview: SharedState<Treeview>,
        requests: Channel<Request>,
        event_loop: &LoopHandle<'static, Data>,
        mut on_request: impl FnMut(Request, &mut Data) + 'static,
    ) -> Self {
        let dispatcher = Dispatcher::new(requests, move |request, &mut (), data| {
            // `Closed` means the client ended; calloop removes the source after that
            if let calloop::channel::Event::Msg(request) = request {
                on_request(request, data);
            }
        });
        let token = event_loop.register_dispatcher(dispatcher.clone()).unwrap();
        let event_loop = event_loop.clone();

        Self {
            event_queue,
            surface,
            treeview,
            detach_requests: Box::new(move || {
                event_loop.remove(token);
                Box::new(dispatcher.into_source_inner())
            }),
        }
    }

    /// Takes the client out of its holder's loop, keeping it running.
    ///
    /// # Panics
    ///
    /// If called while the loop is dispatching this client's requests (eg from its own `on_request`).
    pub fn detach(self) -> DetachedClient {
        DetachedClient {
            requests: (self.detach_requests)(),
            event_queue: self.event_queue,
            surface: self.surface,
            treeview: self.treeview,
        }
    }

    pub fn send_event(&self, event: StandardEvent) {
        // the client may have already ended (eg it closed itself)
        let _ = self.event_queue.send(event);
    }

    /// Like `send_event`, but gives the event back if the client has ended (eg it panicked)
    // the error is just the event, as big as what was passed in
    #[allow(clippy::result_large_err)]
    pub fn try_send_event(&self, event: StandardEvent) -> Result<(), StandardEvent> {
        self.event_queue.send(event).map_err(|err| err.0)
    }

    pub fn get_surface(&self) -> UITree {
        self.get_and_ack(&self.surface)
    }

    pub fn get_treeview(&self) -> Treeview {
        self.get_and_ack(&self.treeview)
    }

    fn get_and_ack<T: Clone>(&self, state: &SharedState<T>) -> T {
        // this order matters; you should generally do the action first then send notification that you did it
        let value = state.get();
        // TODO: just realized that I need to send this after the ui display confirms it too
        // TODO: make surface damage ack a UI Event so UI display can send it too
        self.send_event(StandardEvent::DamageAck(state.kind()));
        value
    }
}
