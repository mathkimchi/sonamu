//! Talks to the running Sonamu (fails if it isn't running, so eg xdg-open can try another):
//! - `sonamu-msg open file_manager ~/Documents` opens an applet (see `assets/sonamu-file-manager.desktop`)
//! - `sonamu-msg eval '<luau>'` (`-`: from stdin) runs Luau like the buffer line: `print`s to stderr, the result as
//!   JSON on stdout (one value as is, several as an array), an error to stderr with exit code 1

use sonamu_common::msg::{EvalReply, Message, SOCKET_ENV};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    process::ExitCode,
};

const USAGE: &str = "usage: sonamu-msg open <applet> [args...] | sonamu-msg eval <luau | ->";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let message = match (args.next().as_deref(), args.next()) {
        (Some("open"), Some(applet)) => Message::Open {
            applet,
            args: args.map(absolute_if_path).collect(),
        },
        (Some("eval"), Some(code)) if args.next().is_none() => {
            let code = if code == "-" {
                let mut code = String::new();
                if let Err(err) = std::io::stdin().read_to_string(&mut code) {
                    eprintln!("Couldn't read stdin: {err}");
                    return ExitCode::FAILURE;
                }
                code
            } else {
                code
            };
            Message::Eval(code)
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    let result = send(&message).and_then(|stream| match message {
        Message::Eval(_) => print_replies(stream),
        Message::Open { .. } => Ok(true),
    });
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("Couldn't reach Sonamu: {err}");
            ExitCode::FAILURE
        }
    }
}

fn send(message: &Message) -> std::io::Result<UnixStream> {
    let path = std::env::var_os(SOCKET_ENV)
        .ok_or_else(|| std::io::Error::other(format!("{SOCKET_ENV} isn't set")))?;
    let mut stream = UnixStream::connect(path)?;
    stream.write_all(&message.encode())?;
    stream.shutdown(std::net::Shutdown::Write)?;
    Ok(stream)
}

/// `Message::Eval`'s answer; whether it succeeded
fn print_replies(stream: UnixStream) -> std::io::Result<bool> {
    for line in BufReader::new(stream).lines() {
        let line = line?;
        // keepalives
        if line.is_empty() {
            continue;
        }
        let reply: EvalReply = serde_json::from_str(&line).map_err(std::io::Error::other)?;
        match reply {
            EvalReply::Print(text) => eprintln!("{text}"),
            EvalReply::Done(Ok(mut values)) => {
                let shown = match values.len() {
                    0 => return Ok(true),
                    1 => values.remove(0),
                    _ => serde_json::Value::Array(values),
                };
                println!("{shown}");
                return Ok(true);
            }
            EvalReply::Done(Err(err)) => {
                eprintln!("{err}");
                return Ok(false);
            }
        }
    }
    Err(std::io::Error::other("Sonamu hung up"))
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
