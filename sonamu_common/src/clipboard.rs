//! Sonamu's own clipboard (what wl clients inside it copied), for native applets: `paste`, and `copy`, which makes
//! the runner the clipboard's owner.
//!
//! Not `arboard`: that connects to the process's `WAYLAND_DISPLAY`, which is the host's when nested and missing on a TTY.

use std::{
    io::Read,
    os::fd::OwnedFd,
    sync::{Mutex, PoisonError, mpsc},
    time::Duration,
};

/// To the runner
pub enum Request {
    /// Write the clipboard's text into it
    Paste(OwnedFd),
    /// Offer this text as the clipboard
    Copy(String),
}

static SERVICE: Mutex<Option<calloop::channel::Sender<Request>>> = Mutex::new(None);

/// How long `paste` waits for the selection's owner to write it
const TIMEOUT: Duration = Duration::from_secs(1);

/// Called by the runner, which answers the `Request`s
pub fn serve(requests: calloop::channel::Sender<Request>) {
    *SERVICE.lock().unwrap_or_else(PoisonError::into_inner) = Some(requests);
}

/// Makes `text` the clipboard (for wl clients too); false without a runner
pub fn copy(text: String) -> bool {
    SERVICE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|service| service.send(Request::Copy(text)).is_ok())
}

/// The clipboard's text, if there is any (waits up to `TIMEOUT`)
pub fn paste() -> Option<String> {
    let service = SERVICE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()?;
    let (mut reader, writer) = std::io::pipe().ok()?;
    service.send(Request::Paste(OwnedFd::from(writer))).ok()?;
    // on another thread, so an owner that never closes it can't hang the caller
    let (text, received) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if reader.read_to_end(&mut bytes).is_ok() {
            let _ = text.send(String::from_utf8_lossy(&bytes).into_owned());
        }
    });
    received
        .recv_timeout(TIMEOUT)
        .ok()
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn paste_reads_what_the_runner_writes() {
        assert_eq!(paste(), None, "no runner");
        let (requests, received) = calloop::channel::channel::<Request>();
        serve(requests);
        std::thread::spawn(move || {
            let mut event_loop = calloop::EventLoop::<()>::try_new().unwrap();
            event_loop
                .handle()
                .insert_source(received, |event, (), ()| {
                    if let calloop::channel::Event::Msg(Request::Paste(fd)) = event {
                        let _ = std::fs::File::from(fd).write_all("hi\nthere".as_bytes());
                    }
                })
                .unwrap();
            event_loop.run(None, &mut (), |()| {}).unwrap();
        });
        assert_eq!(paste().as_deref(), Some("hi\nthere"));
    }
}
