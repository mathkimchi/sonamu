//! Messages other processes send to the runner over its socket (eg `sonamu-msg` from xdg-open).
//!
//! A connection carries one message: its fields, each ended by a NUL byte. `Eval`'s answer comes back on the same
//! connection as `EvalReply` lines.

use std::{ffi::OsStr, path::PathBuf};

/// Set by the runner to its socket's path
pub const SOCKET_ENV: &str = "SONAMU_SOCKET";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    /// Opens the applet registered under `applet` with `args` beside the focus (`StandardEvent::Open`)
    Open { applet: String, args: Vec<String> },
    /// Runs Luau on the script thread like the buffer line; answered with `EvalReply` lines, the last one `Done`
    Eval(String),
    /// Streams these `events.on` events (all if empty) back as `EventLine`s until the connection closes
    Subscribe(Vec<String>),
}

/// A line the runner writes back for `Message::Eval`, as JSON
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalReply {
    Print(String),
    /// Its return values (`command::Value` as JSON)
    Done(Result<Vec<serde_json::Value>, String>),
}
/// A line the runner writes back for `Message::Subscribe`, as JSON
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EventLine {
    pub event: String,
    /// `command::Value`s as JSON
    pub args: Vec<serde_json::Value>,
}

impl Message {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let fields: Vec<&str> = match self {
            Self::Open { applet, args } => ["open", applet.as_str()]
                .into_iter()
                .chain(args.iter().map(String::as_str))
                .collect(),
            Self::Eval(code) => vec!["eval", code],
            Self::Subscribe(events) => std::iter::once("subscribe")
                .chain(events.iter().map(String::as_str))
                .collect(),
        };
        let mut bytes = Vec::new();
        for field in fields {
            bytes.extend_from_slice(field.as_bytes());
            bytes.push(0);
        }
        bytes
    }

    #[must_use]
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(bytes.strip_suffix(&[0])?).ok()?;
        let mut fields = text.split('\0').map(str::to_owned);
        match fields.next()?.as_str() {
            "open" => Some(Self::Open {
                applet: fields.next()?,
                args: fields.collect(),
            }),
            "eval" => Some(Self::Eval(fields.next()?)),
            "subscribe" => Some(Self::Subscribe(fields.collect())),
            _ => None,
        }
    }
}

/// Where the runner of this wl socket listens: in `XDG_RUNTIME_DIR` (else the temp dir)
#[must_use]
pub fn socket_path(wayland_socket: &OsStr) -> PathBuf {
    let directory =
        std::env::var_os("XDG_RUNTIME_DIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let mut name = std::ffi::OsString::from("sonamu-");
    name.push(wayland_socket);
    name.push(".sock");
    directory.join(name)
}

#[cfg(test)]
mod tests {
    use super::Message;

    #[test]
    fn round_trip() {
        let message = Message::Open {
            applet: "file_manager".to_owned(),
            args: vec!["/tmp".to_owned(), String::new()],
        };
        assert_eq!(Message::decode(&message.encode()), Some(message));
        let eval = Message::Eval("print(1)\nreturn 2".to_owned());
        assert_eq!(Message::decode(&eval.encode()), Some(eval));
        assert_eq!(Message::decode(b"open"), None);
        assert_eq!(Message::decode(b"nope\0"), None);
    }
}
