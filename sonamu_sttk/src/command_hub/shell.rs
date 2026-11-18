//! `$` lines of a prompt: `sh -c`, its stdout and stderr streamed; Tab completes programs and paths

use super::agent::Streamed;
use std::{
    io::Read,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::PathBuf,
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

/// What `line`'s last word could be, on a thread (`notify`ing once): a program in `$PATH` as its first word, else a
/// path (directories end in `/`)
pub(crate) fn complete(
    line: String,
    notify: impl Fn() + Send + 'static,
) -> mpsc::Receiver<Vec<String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(candidates(&line));
        notify();
    });
    receiver
}

fn candidates(line: &str) -> Vec<String> {
    let word = line.rsplit(char::is_whitespace).next().unwrap_or_default();
    let first = line[..line.len() - word.len()].trim().is_empty();
    let mut candidates = if first && !word.contains('/') {
        programs(word)
    } else {
        paths(word)
    };
    candidates.sort();
    candidates.dedup();
    candidates
}

/// Executables in `$PATH` starting with `start`
fn programs(start: &str) -> Vec<String> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    std::env::split_paths(&path)
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            // follows symlinks (eg nix profiles')
            let metadata = std::fs::metadata(entry.path()).ok()?;
            (name.starts_with(start)
                && metadata.is_file()
                && metadata.permissions().mode() & 0o111 != 0)
                .then_some(name)
        })
        .collect()
}

/// Paths starting with `word` (`~/` is home; hidden ones only when asked for with a `.`)
fn paths(word: &str) -> Vec<String> {
    let (typed_dir, start) = match word.rsplit_once('/') {
        Some((dir, start)) => (format!("{dir}/"), start),
        None => (String::new(), word),
    };
    let dir = match typed_dir.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME")
            .map_or_else(PathBuf::new, PathBuf::from)
            .join(rest),
        None if typed_dir.is_empty() => PathBuf::from("."),
        None => PathBuf::from(&typed_dir),
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with(start) || (name.starts_with('.') && !start.starts_with('.')) {
                return None;
            }
            let slash = if entry.path().is_dir() { "/" } else { "" };
            Some(format!("{typed_dir}{name}{slash}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_programs_then_paths() {
        assert!(candidates(" s").contains(&"sh".to_string()));
        let dir = std::env::temp_dir().join(format!("sonamu-complete-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("file"), "").unwrap();
        std::fs::write(dir.join(".hidden"), "").unwrap();
        let dir = dir.to_str().unwrap();
        assert_eq!(
            candidates(&format!("ls {dir}/")),
            [format!("{dir}/file"), format!("{dir}/sub/")]
        );
        assert_eq!(
            candidates(&format!("ls {dir}/.")),
            [format!("{dir}/.hidden")]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
