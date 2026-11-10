//! Where other processes ask the runner to open applets or run Luau (`sonamu_common::msg`)

use crate::runner::AppletRunner;
use calloop::{Interest, LoopHandle, Mode, PostAction, generic::Generic};
use sonamu_common::{
    command::{self, Value},
    msg::{EvalReply, EventLine, Message},
    sap::packets::{OpenApplet, StandardEvent},
};
use std::{
    io::{Read, Write},
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
                    while let Ok((mut stream, _)) = socket.listener.accept() {
                        match read_message(&mut stream) {
                            Ok(Message::Open { applet, args }) => runner
                                .root_client
                                .send_event(StandardEvent::Open(OpenApplet { applet, args })),
                            // answered from its own thread, so the loop doesn't wait for it
                            Ok(Message::Eval { code, max }) => {
                                std::thread::spawn(move || answer_eval(stream, code, max));
                            }
                            Ok(Message::Subscribe(events)) => {
                                std::thread::spawn(move || answer_subscribe(stream, &events));
                            }
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

/// Writes the eval's outputs back as `EvalReply` lines; cancels it if the sender hangs up
fn answer_eval(mut stream: UnixStream, code: String, max: command::Effect) {
    const POLL: Duration = Duration::from_millis(100);
    let (handle, outputs) = sonamu_script::eval_limited(code, max, || {});
    loop {
        let reply = match outputs.recv_timeout(POLL) {
            Ok(sonamu_script::Output::Print(line)) => EvalReply::Print(line),
            Ok(sonamu_script::Output::Done(result)) => {
                EvalReply::Done(result.map(|values| values.iter().map(to_json).collect()))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // an empty line (ignored by the reader) fails once the sender is gone; reading can't tell, since
                // it already shut its writing half
                if stream.write_all(b"\n").is_err() {
                    handle.cancel();
                }
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let done = matches!(reply, EvalReply::Done(_));
        let mut line = serde_json::to_string(&reply).unwrap_or_default();
        line.push('\n');
        if stream.write_all(line.as_bytes()).is_err() {
            handle.cancel();
        }
        if done {
            return;
        }
    }
}

/// Writes each emitted event in `events` (all if empty) as an `EventLine` until the sender is gone
fn answer_subscribe(mut stream: UnixStream, events: &[String]) {
    const POLL: Duration = Duration::from_millis(100);
    let emitted = sonamu_script::subscribe();
    loop {
        let line = match emitted.recv_timeout(POLL) {
            Ok((event, _)) if !events.is_empty() && !events.contains(&event) => continue,
            Ok((event, args)) => {
                let line = EventLine {
                    event,
                    args: args.iter().map(to_json).collect(),
                };
                let mut line = serde_json::to_string(&line).unwrap_or_default();
                line.push('\n');
                line
            }
            // a keepalive, failing once the sender is gone (see `answer_eval`)
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => "\n".to_string(),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        };
        if stream.write_all(line.as_bytes()).is_err() {
            return;
        }
    }
}

/// `value` as JSON, a node as `{"type": .., "node": uid}` (its type asked now; null if it's gone)
fn to_json(value: &Value) -> serde_json::Value {
    match value {
        &Value::Node(uid) => {
            let applet_type = command::call(Some(uid), "type", Vec::new(), &|| false)
                .ok()
                .and_then(|applet_type| serde_json::to_value(applet_type).ok());
            serde_json::json!({ "type": applet_type, "node": uid })
        }
        Value::List(values) => values.iter().map(to_json).collect(),
        Value::Map(values) => values
            .iter()
            .map(|(key, value)| (key.clone(), to_json(value)))
            .collect(),
        value => serde_json::to_value(value).unwrap_or_default(),
    }
}

fn read_message(stream: &mut UnixStream) -> std::io::Result<Message> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(MsgSocket::READ_TIMEOUT))?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    Message::decode(&bytes).ok_or_else(|| std::io::Error::other("unknown message"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};

    #[test]
    fn eval_answers_prints_then_the_result() {
        let (client, server) = UnixStream::pair().unwrap();
        std::thread::spawn(move || {
            answer_eval(
                server,
                "print('a') return 1 + 1".to_string(),
                command::Effect::Read,
            )
        });
        let replies: Vec<EvalReply> = BufReader::new(client)
            .lines()
            .map(Result::unwrap)
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str(&line).unwrap())
            .collect();
        assert_eq!(
            replies,
            [
                EvalReply::Print("a".to_string()),
                EvalReply::Done(Ok(vec![serde_json::json!(2.0)])),
            ]
        );
    }

    #[test]
    fn subscribe_streams_events() {
        let (client, server) = UnixStream::pair().unwrap();
        std::thread::spawn(move || answer_subscribe(server, &["wanted".to_string()]));
        // once it subscribed
        std::thread::sleep(Duration::from_millis(100));
        sonamu_script::emit("unwanted", Vec::new());
        sonamu_script::emit("wanted", vec![Value::Number(1.0)]);
        let line = BufReader::new(client)
            .lines()
            .map(Result::unwrap)
            .find(|line| !line.is_empty())
            .unwrap();
        assert_eq!(
            serde_json::from_str::<EventLine>(&line).unwrap(),
            EventLine {
                event: "wanted".to_string(),
                args: vec![serde_json::json!(1.0)]
            }
        );
    }
}
