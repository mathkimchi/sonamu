//! The desktop notification server (`org.freedesktop.Notifications`, replacing DMS's), and what the bar keeps of
//! them: a list, the ones popping up, Do not disturb

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, mpsc,
        mpsc::{Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};
use zbus::{OwnedVariant, connection, interface};

const PATH: &str = "/org/freedesktop/Notifications";
const INTERFACE: &str = "org.freedesktop.Notifications";

/// Why one closed (the spec's codes; expiring only stops it popping up, it stays listed)
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum CloseReason {
    Dismissed = 2,
    /// `CloseNotification`
    ByCall = 3,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    /// An icon name or path (`app_icon`, else the `image-path` hint)
    #[expect(dead_code, reason = "drawn once icons are looked up")]
    pub icon: String,
    /// The `desktop-entry` hint (an app's `.desktop` name), for its icon
    #[expect(dead_code, reason = "drawn once icons are looked up")]
    pub desktop_entry: Option<String>,
    pub summary: String,
    pub body: String,
    /// `(key, label)`; `default` is clicking it
    pub actions: Vec<(String, String)>,
    pub critical: bool,
    /// Popping up until then (`None`: until dismissed, if critical); not popping up once passed
    pub popup_until: Option<Instant>,
}

#[derive(Default)]
pub struct Store {
    /// Oldest first
    pub list: Vec<Notification>,
    pub do_not_disturb: bool,
    next_id: u32,
}
impl Store {
    /// The ones popping up, newest first
    pub fn popups(&self) -> impl Iterator<Item = &Notification> {
        let now = Instant::now();
        self.list.iter().rev().filter(move |notification| {
            !self.do_not_disturb
                && match notification.popup_until {
                    Some(until) => now < until,
                    None => notification.critical,
                }
        })
    }
}

enum Signal {
    Closed(u32, CloseReason),
    Action(u32, String),
}

/// The server's state, shared with the bar; signals go out through the server's thread
pub struct Notifications {
    pub store: Arc<Mutex<Store>>,
    signals: Sender<Signal>,
}

impl Notifications {
    /// Serves `org.freedesktop.Notifications` on the session bus (logs and serves nothing if the name is taken, eg
    /// by DMS); `changed` is called on any change
    pub fn start(changed: Arc<dyn Fn() + Send + Sync>) -> Self {
        let store = Arc::new(Mutex::new(Store::default()));
        let (signals, receiver) = mpsc::channel();
        let server = Server {
            store: Arc::clone(&store),
            changed,
            signals: signals.clone(),
        };
        thread::spawn(move || serve(server, &receiver));
        Self { store, signals }
    }

    /// Removes it (`reason` to its sender)
    pub fn close(&self, id: u32, reason: CloseReason) {
        self.store.lock().unwrap().list.retain(|n| n.id != id);
        let _ = self.signals.send(Signal::Closed(id, reason));
    }

    pub fn clear(&self) {
        let ids: Vec<u32> = self
            .store
            .lock()
            .unwrap()
            .list
            .iter()
            .map(|n| n.id)
            .collect();
        for id in ids {
            self.close(id, CloseReason::Dismissed);
        }
    }

    /// Clicked: its default action if it has one, then dismissed
    pub fn activate(&self, id: u32) {
        let has_default = self
            .store
            .lock()
            .unwrap()
            .list
            .iter()
            .any(|n| n.id == id && n.actions.iter().any(|(key, _)| key == "default"));
        if has_default {
            let _ = self.signals.send(Signal::Action(id, "default".to_string()));
        }
        self.close(id, CloseReason::Dismissed);
    }

    pub fn toggle_do_not_disturb(&self) {
        let mut store = self.store.lock().unwrap();
        store.do_not_disturb = !store.do_not_disturb;
    }
}

fn serve(server: Server, signals: &Receiver<Signal>) {
    let connection = zbus::block_on(
        connection::Builder::session()
            .name(INTERFACE)
            .serve_at(PATH, server)
            .build(),
    );
    let connection = match connection {
        Ok(connection) => connection,
        Err(err) => {
            log::warn!("Not showing desktop notifications: couldn't serve {INTERFACE}: {err}");
            return;
        }
    };
    for signal in signals {
        let sent = match signal {
            Signal::Closed(id, reason) => zbus::block_on(connection.emit_signal(
                None::<&str>,
                PATH,
                INTERFACE,
                "NotificationClosed",
                &(id, reason as u32),
            )),
            Signal::Action(id, key) => zbus::block_on(connection.emit_signal(
                None::<&str>,
                PATH,
                INTERFACE,
                "ActionInvoked",
                &(id, key),
            )),
        };
        if let Err(err) = sent {
            log::warn!("Couldn't send a notification signal: {err}");
        }
    }
}

struct Server {
    store: Arc<Mutex<Store>>,
    changed: Arc<dyn Fn() + Send + Sync>,
    signals: Sender<Signal>,
}

#[interface(name = "org.freedesktop.Notifications")]
#[expect(
    clippy::unused_self,
    clippy::needless_pass_by_value,
    reason = "D-Bus methods, as the macro takes them"
)]
impl Server {
    fn get_capabilities(&self) -> Vec<&'static str> {
        vec!["body", "actions", "icon-static", "persistence"]
    }

    #[expect(clippy::too_many_arguments, reason = "the spec's")]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedVariant>,
        expire_timeout: i32,
    ) -> u32 {
        let hint_str = |name: &str| {
            hints
                .get(name)
                .and_then(|value| <&str>::try_from(value).ok())
                .map(str::to_string)
        };
        let critical = hints
            .get("urgency")
            .and_then(|value| u8::try_from(value).ok())
            == Some(2);
        let timeout = match expire_timeout {
            // the setting's
            -1 => Some(Duration::from_secs_f32(
                sonamu_common::settings::get()
                    .notifications
                    .timeout
                    .max(0.0),
            )),
            0 => None,
            ms => Some(Duration::from_millis(u64::from(ms.unsigned_abs()))),
        };
        let icon = if app_icon.is_empty() {
            hint_str("image-path")
                .or_else(|| hint_str("image_path"))
                .unwrap_or_default()
        } else {
            app_icon
        };
        let mut store = self.store.lock().unwrap();
        let id = if replaces_id == 0 {
            store.next_id += 1;
            store.next_id
        } else {
            replaces_id
        };
        let notification = Notification {
            id,
            app_name,
            icon,
            desktop_entry: hint_str("desktop-entry"),
            summary,
            body,
            actions: actions
                .as_chunks::<2>()
                .0
                .iter()
                .map(|[key, label]| (key.clone(), label.clone()))
                .collect(),
            critical,
            popup_until: if critical {
                None
            } else {
                timeout.map(|timeout| Instant::now() + timeout)
            },
        };
        match store.list.iter_mut().find(|n| n.id == id) {
            Some(old) => *old = notification,
            None => store.list.push(notification),
        }
        drop(store);
        (self.changed)();
        id
    }

    fn close_notification(&self, id: u32) {
        self.store.lock().unwrap().list.retain(|n| n.id != id);
        let _ = self.signals.send(Signal::Closed(id, CloseReason::ByCall));
        (self.changed)();
    }

    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("Sonamu", "Sonamu", env!("CARGO_PKG_VERSION"), "1.2")
    }
}
