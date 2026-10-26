//! Where other processes ask the runner to open applets (`singularity_common::msg`)

use crate::runner::AppletRunner;
use calloop::{Interest, LoopHandle, Mode, PostAction, generic::Generic};
use singularity_common::{
    msg::Message,
    sap::packets::{OpenApplet, StandardEvent},
};
use std::{
    io::Read,
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    time::Duration,
};

/// Removes its file when dropped (with the event loop)
pub(crate) struct MsgSocket {
    listener: UnixListener,
    path: PathBuf,
}
impl MsgSocket {
    /// How long a sender may take to send its message, since reading it blocks the loop
    const READ_TIMEOUT: Duration = Duration::from_millis(100);

    /// Replaces a stale file left by a runner that didn't exit cleanly
    pub(crate) fn bind(path: PathBuf) -> std::io::Result<Self> {
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener, path })
    }

    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Passes the messages to the root
    pub(crate) fn insert(self, event_loop: &LoopHandle<'static, AppletRunner>) {
        event_loop
            .insert_source(
                Generic::new(self, Interest::READ, Mode::Level),
                |_, socket, runner| {
                    while let Ok((stream, _)) = socket.listener.accept() {
                        match read_message(stream) {
                            Ok(Message::Open { applet, args }) => runner
                                .root_client
                                .send_event(StandardEvent::Open(OpenApplet { applet, args })),
                            Err(err) => eprintln!("Bad message on the socket: {err}"),
                        }
                    }
                    Ok(PostAction::Continue)
                },
            )
            .unwrap();
    }
}
impl std::os::fd::AsFd for MsgSocket {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.listener.as_fd()
    }
}
impl Drop for MsgSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn read_message(mut stream: UnixStream) -> std::io::Result<Message> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(MsgSocket::READ_TIMEOUT))?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    Message::decode(&bytes).ok_or_else(|| std::io::Error::other("unknown message"))
}
