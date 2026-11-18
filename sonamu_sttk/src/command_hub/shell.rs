//! `$` lines of a prompt: `sh -c`, its stdout and stderr streamed

use super::agent::Streamed;
use std::{
    io::Read,
    os::unix::process::CommandExt,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
};

/// A running command; dropping it doesn't stop it
pub(crate) struct Shell {
    /// Its process group's
    pid: i32,
}
impl Shell {
    /// Stops it and whatever it started
    pub fn cancel(&self) {
        // SAFETY: a plain syscall; the group is ours (`process_group(0)`)
        unsafe {
            libc::kill(-self.pid, libc::SIGTERM);
        }
    }
}

/// Runs `command` on a thread, `notify`ing for each piece of output
pub(crate) fn run(
    command: &str,
    notify: impl Fn() + Send + 'static,
) -> std::io::Result<(Shell, mpsc::Receiver<Streamed>)> {
    let (mut reader, writer) = std::io::pipe()?;
    let mut builder = Command::new("sh");
    builder
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(writer.try_clone()?)
        .stderr(writer)
        .process_group(0);
    // wl clients (eg `grim`) connect to Sonamu
    sonamu_common::connect_wl_client(&mut builder);
    let mut child = builder.spawn()?;
    // its copies of the pipe's writers: the reader then ends with the command
    drop(builder);
    let shell = Shell {
        pid: child.id() as i32,
    };
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut buffer = [0; 4096];
        let mut pending = Vec::new();
        while let Ok(read @ 1..) = reader.read(&mut buffer) {
            pending.extend_from_slice(&buffer[..read]);
            // only whole UTF-8 chars
            let valid = match std::str::from_utf8(&pending) {
                Ok(text) => text.len(),
                Err(err) if err.error_len().is_none() => err.valid_up_to(),
                Err(_) => pending.len(),
            };
            let text = String::from_utf8_lossy(&pending[..valid]).into_owned();
            pending.drain(..valid);
            let _ = sender.send(Streamed::Text(text));
            notify();
        }
        let result = match child.wait() {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(status.to_string()),
            Err(err) => Err(err.to_string()),
        };
        let _ = sender.send(Streamed::Done(result));
        notify();
    });
    Ok((shell, receiver))
}
