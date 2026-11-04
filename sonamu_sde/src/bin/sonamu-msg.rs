//! Asks the running Sonamu to open an applet, eg `sonamu-msg open file_manager ~/Documents`
//! (see `assets/sonamu-file-manager.desktop`). Fails if Sonamu isn't running, so xdg-open can try another.

use sonamu_common::msg::{Message, SOCKET_ENV};
use std::{io::Write, os::unix::net::UnixStream, path::Path, process::ExitCode};

const USAGE: &str = "usage: sonamu-msg open <applet> [args...]";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some("open"), Some(applet)) = (args.next().as_deref(), args.next()) else {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    };
    let message = Message::Open {
        applet,
        args: args.map(absolute_if_path).collect(),
    };
    match send(&message) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Couldn't reach Sonamu: {err}");
            ExitCode::FAILURE
        }
    }
}

fn send(message: &Message) -> std::io::Result<()> {
    let path = std::env::var_os(SOCKET_ENV)
        .ok_or_else(|| std::io::Error::other(format!("{SOCKET_ENV} isn't set")))?;
    let mut stream = UnixStream::connect(path)?;
    stream.write_all(&message.encode())?;
    stream.shutdown(std::net::Shutdown::Write)
}

/// Sonamu runs in another directory, so relative paths that exist here are made absolute
fn absolute_if_path(arg: String) -> String {
    let path = Path::new(&arg);
    if path.is_relative()
        && path.exists()
        && let Ok(path) = std::path::absolute(path)
    {
        return path.to_string_lossy().into_owned();
    }
    arg
}
