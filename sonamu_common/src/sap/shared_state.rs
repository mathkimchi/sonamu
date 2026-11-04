use crate::sap::packets::{StandardRequest, StateKind};
use calloop::channel::Sender;
use sonamu_sync::EncapsulatedLock;

/// Client state the server reads, like a wl surface's committed state.
///
/// The client publishes a value (`StatePublisher`) and sends `Damage(kind)`;
/// the server reads it and sends back `DamageAck(kind)`.
#[derive(Clone)]
pub struct SharedState<T: Clone> {
    kind: StateKind,
    lock: EncapsulatedLock<T>,
}
impl<T: Clone> SharedState<T> {
    pub fn new(kind: StateKind, initial: T) -> Self {
        Self {
            kind,
            lock: EncapsulatedLock::new(initial),
        }
    }

    pub const fn kind(&self) -> StateKind {
        self.kind
    }

    /// Server side; the caller sends the `DamageAck`
    pub fn get(&self) -> T {
        self.lock.get()
    }
}

/// Client side of a `SharedState`.
///
/// Renders lazily and keeps at most one unread publish,
/// so damage while waiting for an ack is merged into the next publish.
pub struct StatePublisher<T: Clone> {
    state: SharedState<T>,
    damaged: bool,
    awaiting_ack: bool,
}
impl<T: Clone> StatePublisher<T> {
    /// Starts damaged so the first value gets published
    pub const fn new(state: SharedState<T>) -> Self {
        Self {
            state,
            damaged: true,
            awaiting_ack: false,
        }
    }

    pub const fn damage(&mut self) {
        self.damaged = true;
    }

    /// Call on `DamageAck(kind)` for this state's kind
    pub const fn ack(&mut self) {
        self.awaiting_ack = false;
    }

    /// Publishes a new value if damaged and the last one was read.
    /// Call after each batch of events.
    pub fn flush<Request: From<StandardRequest>>(
        &mut self,
        render: impl FnOnce() -> T,
        request_sender: &Sender<Request>,
    ) {
        if self.damaged && !self.awaiting_ack {
            self.state.lock.set(render());
            // the server may already be gone while shutting down
            let _ = request_sender.send(StandardRequest::Damage(self.state.kind).into());
            self.damaged = false;
            self.awaiting_ack = true;
        }
    }
}
