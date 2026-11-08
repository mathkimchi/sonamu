//! Saving the tree and restoring it on the next start (see `.claude/SESSIONS.md`)

use crate::{
    command_hub::program_command,
    nodular_applet::{
        AppletContext, NodeRequest, NodularAppletInitializer, StandardApplet,
        project_applet::ProjectApplet,
    },
    standard_keybinds::standard_keybind_request,
    ui,
};
use sonamu_common::{
    sap::{
        packets::{AwaitedWl, StandardEvent},
        session::{SavedApplet, SavedRunning, SavedTree, SavedWl, Session, title_without_url},
    },
    settings::{self, RestoreProgram},
    utils::tree::world_tree::WorldTree,
};
use sonamu_ui::{
    color::Color,
    display_units::DisplayContainerSize,
    ui_element::UITree,
    ui_event::{KeySymbol, UIEvent},
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// `$SONAMU_SESSION` (set but empty: no session), else `$XDG_STATE_HOME/sonamu/session.json`
/// (`~/.local/state` by default)
pub fn session_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SONAMU_SESSION") {
        return (!path.is_empty()).then(|| path.into());
    }
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(state.join("sonamu/session.json"))
}

/// `None` if there is none. One that can't be read is moved aside (`.bad`), so the next save doesn't overwrite it.
pub fn load(path: &Path) -> Option<Session> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return None,
        Err(err) => {
            log::warn!("Couldn't read the session {}: {err}", path.display());
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(session) => Some(session),
        Err(err) => {
            log::warn!("Couldn't parse the session {}: {err}", path.display());
            let _ = fs::rename(path, path.with_extension("json.bad"));
            None
        }
    }
}

/// Through a temporary file, so a crash mid-write keeps the old one
pub fn write(path: &Path, session: &Session) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_string_pretty(session)?)?;
    fs::rename(temporary, path)
}

/// Saved as made by the spawner registered as `name` (in `sonamu_sde`'s `main`) from this path, made absolute
pub fn saved_with_path(name: &str, path: &Path) -> Option<SavedApplet> {
    let path = std::path::absolute(path).ok()?;
    Some(SavedApplet::Spawner {
        name: name.to_string(),
        args: vec![path.to_string_lossy().into_owned()],
        inner: None,
    })
}

/// Saved as made by the spawner registered as `name` from `args` and its inner tree
/// (`AppletSpawnerTrait::restore_initializer`)
pub fn saved_with_inner(name: &str, args: Vec<String>, inner: SavedTree) -> SavedApplet {
    SavedApplet::Spawner {
        name: name.to_string(),
        args,
        inner: Some(Box::new(inner)),
    }
}

/// Makes the saved applet again. A wl client isn't started: its tile hibernates (`hibernate`) until `wake`.
pub fn restore(saved: SavedApplet) -> NodularAppletInitializer {
    match &saved {
        SavedApplet::Spawner { name, args, inner } => {
            let (name, args, inner) = (name.clone(), args.clone(), inner.clone());
            let holds_children = holds_children(&saved);
            let mut initializer = NodularAppletInitializer::new(move |hook: AppletContext| {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                match hook
                    .find_applet_spawner(&name)
                    .and_then(|spawner| match inner {
                        Some(inner) => spawner.restore_initializer(&args, *inner),
                        None => spawner.create_initializer(&args),
                    }) {
                    Some(initializer) => initializer.init(hook),
                    None => Box::new(RestoringApplet {
                        label: format!("Couldn't restore {name} {}", args.join(" ")),
                        saved,
                        hook,
                    }),
                }
            });
            initializer.holds_children = holds_children;
            initializer
        }
        SavedApplet::Wl(_) => hibernate(saved),
    }
}

/// Whether the saved applet's node holds its children (a project's, even while it hibernates)
fn holds_children(saved: &SavedApplet) -> bool {
    matches!(saved, SavedApplet::Spawner { name, .. } if name == ProjectApplet::SPAWNER)
}

/// Stands in for an applet that isn't running (restored or deactivated), saved as it was, until it is woken.
/// Labeled by a project's name, else the file it shows (its last arg), else its spawner's name.
pub fn hibernate(saved: SavedApplet) -> NodularAppletInitializer {
    let label = match &saved {
        SavedApplet::Wl(wl) => wl_label(wl),
        SavedApplet::Spawner { name, args, inner } => inner
            .as_ref()
            .and_then(|inner| inner.label.clone())
            .or_else(|| {
                let file_name = Path::new(args.last()?).file_name()?;
                Some(file_name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| name.clone()),
    };
    RestoringApplet::initializer(saved, format!("{label} (inactive)"))
}

/// Starts a hibernating wl client, with the program running in its terminal if it is in `restore_programs`.
/// If it had another one, its tile asks first whether to run it again (`RestorePrompt`).
pub fn wake(wl: &SavedWl) -> (NodularAppletInitializer, Option<AwaitedWl>) {
    if wl
        .running
        .as_ref()
        .is_some_and(|running| restore_program(running).is_none())
    {
        let wl = wl.clone();
        return (
            NodularAppletInitializer::new(move |hook| Box::new(RestorePrompt { wl, hook })),
            None,
        );
    }
    start(wl)
}

/// Starts the wl client (with the program running in its terminal, if any). Its tile stands in as "Starting ..."
/// and awaits its toplevel.
pub fn start(wl: &SavedWl) -> (NodularAppletInitializer, Option<AwaitedWl>) {
    let name = wl_label(wl);
    let (label, awaiting) = match restore_command(wl).map(|mut command| command.spawn()) {
        Some(Ok(mut child)) => {
            let awaiting = AwaitedWl {
                pid: child.id(),
                app_id: wl.app_id.clone(),
            };
            // reaped, so it doesn't stay a zombie
            std::thread::spawn(move || child.wait());
            (format!("Starting {name}…"), Some(awaiting))
        }
        Some(Err(err)) => (format!("Couldn't start {name}: {err}"), None),
        None => (format!("Couldn't restore {name}: no command"), None),
    };
    let saved = SavedApplet::Wl(wl.clone());
    (RestoringApplet::initializer(saved, label), awaiting)
}

fn wl_label(wl: &SavedWl) -> String {
    if !wl.title.is_empty() {
        title_without_url(&wl.title).to_string()
    } else if let Some(app_id) = &wl.app_id {
        app_id.clone()
    } else {
        wl.argv
            .first()
            .cloned()
            .unwrap_or_else(|| "a wl client".to_string())
    }
}

/// Browsers open the saved url in a new window (of a running instance, if any), Chromium web apps (PWAs) open by
/// their id with the browser they ran in; other clients run as they were, terminals with the program running in them
pub fn restore_command(wl: &SavedWl) -> Option<std::process::Command> {
    let argv: Vec<String> = match (wl.app_id.as_deref(), wl.url()) {
        (Some("org.qutebrowser.qutebrowser" | "qutebrowser"), url) => {
            ["qutebrowser", "--target", "window"]
                .into_iter()
                .chain(url)
                .map(String::from)
                .collect()
        }
        (Some("firefox"), Some(url)) => ["firefox", "--new-window", url].map(String::from).into(),
        // Chromium rewrites its command line as one space-separated string
        _ if let Some((id, profile)) = wl.web_app() => vec![
            program_name(wl.argv.first()?.split(' ').next()?),
            format!("--app-id={id}"),
            format!("--profile-directory={profile}"),
        ],
        _ => {
            let (program, args) = wl.argv.split_first()?;
            let mut argv: Vec<String> = std::iter::once(program_name(program))
                .chain(args.iter().cloned())
                .collect();
            // restored before, so it would run the program again even if it has exited since
            if let Some(start) = wrapper_start(&argv) {
                argv.truncate(start);
                if argv.last().is_some_and(|arg| arg == "-e") {
                    argv.pop();
                }
            }
            match &wl.running {
                Some(running) => running_in_terminal(argv, running),
                None => argv,
            }
        }
    };
    let (program, args) = argv.split_first()?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut command = program_command(program, &args);
    let cwd = wl.running.as_ref().and_then(|running| running.cwd.as_ref());
    if let Some(cwd) = cwd.or(wl.cwd.as_ref()).filter(|cwd| cwd.is_dir()) {
        command.current_dir(cwd);
    }
    Some(command)
}

/// The setting for the program, if it is in `restore_programs`
fn restore_program(running: &SavedRunning) -> Option<RestoreProgram> {
    let name = base_name(running.argv.first()?);
    settings::get()
        .restore_programs
        .into_iter()
        .find(|program| program.program() == name)
}

/// The terminal's command line, running the program (with the setting's args, if it has them),
/// then its shell once it exits
fn running_in_terminal(mut argv: Vec<String>, running: &SavedRunning) -> Vec<String> {
    // saved before restored terminals' wrappers were left out of their command line
    let saved = &running.argv[..wrapper_start(&running.argv).unwrap_or(running.argv.len())];
    let Some((program, args)) = saved.split_first() else {
        return argv;
    };
    let args = match restore_program(running) {
        Some(RestoreProgram::WithArgs { args, .. }) => args,
        _ => args.to_vec(),
    };
    // foot and kitty take the command as their last args, most others after `-e`
    if !argv
        .first()
        .is_some_and(|terminal| matches!(base_name(terminal).as_str(), "foot" | "kitty"))
    {
        argv.push("-e".to_string());
    }
    let shell = running.shell.as_deref().map_or_else(
        || std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string()),
        program_name,
    );
    argv.extend(["sh", "-c", WRAPPER].map(String::from));
    argv.push(shell);
    argv.push(program_name(program));
    argv.extend(args);
    argv
}

/// A Nix store path by its name, since the store path may be gone (eg garbage collected after an update)
fn program_name(program: &str) -> String {
    if !program.starts_with("/nix/store/") {
        return program.to_string();
    }
    base_name(program)
}

/// Its file name, and the unwrapped binary (`.foo-wrapped`) by its wrapper's
fn base_name(program: &str) -> String {
    let name = Path::new(program)
        .file_name()
        .map_or_else(|| program.into(), |name| name.to_string_lossy());
    let name = name.strip_prefix('.').unwrap_or(&name);
    name.strip_suffix("-wrapped").unwrap_or(name).to_string()
}

/// The wl client with this pid, as it is now: its command line from `/proc`,
/// its first child's working directory (eg a terminal's shell), else its own, and the program running in its terminal
pub fn saved_wl(pid: Option<u32>, app_id: Option<String>, title: &str) -> SavedWl {
    SavedWl {
        argv: pid.map(command_line).unwrap_or_default(),
        cwd: pid.and_then(|pid| {
            fs::read_link(format!("/proc/{}/cwd", first_child(pid).unwrap_or(pid))).ok()
        }),
        app_id,
        title: title.to_string(),
        running: pid.and_then(running),
    }
}

fn command_line(pid: u32) -> Vec<String> {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|cmdline| {
            cmdline
                .split(|&byte| byte == 0)
                .filter(|arg| !arg.is_empty())
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn first_child(pid: u32) -> Option<u32> {
    fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Runs a restored program (`"$@"`), then its shell (`$0`), as `sh -c WRAPPER <shell> <program> <args>`
const WRAPPER: &str = r#""$@"; exec "$0""#;

/// Where `WRAPPER` starts in a command line (`sh -c WRAPPER ...`), if it has it
fn wrapper_start(argv: &[String]) -> Option<usize> {
    argv.windows(3)
        .position(|window| window[0] == "sh" && window[1] == "-c" && window[2] == WRAPPER)
}

/// The foreground job of its first child's tty (eg what a terminal's shell runs), if it isn't that child,
/// or the program a restored terminal's `WRAPPER` runs
fn running(pid: u32) -> Option<SavedRunning> {
    let child = first_child(pid)?;
    // its program is in its process group (`sh -c` has no job control)
    if let [_, flag, script, shell, ..] = command_line(child).as_slice()
        && flag == "-c"
        && script == WRAPPER
    {
        let program = first_child(child)?;
        return Some(SavedRunning {
            argv: command_line(program),
            cwd: fs::read_link(format!("/proc/{program}/cwd")).ok(),
            shell: Some(shell.clone()),
        });
    }
    let (tty, job) = tty_and_foreground(child)?;
    let (client_tty, _) = tty_and_foreground(pid)?;
    // the child's own tty, not one it shares with the client (eg Sonamu's, when Sonamu runs on a tty)
    if tty == 0 || tty == client_tty || job <= 0 || job == i64::from(child) {
        return None;
    }
    let job = u32::try_from(job).ok()?;
    let argv = command_line(job);
    (!argv.is_empty()).then(|| SavedRunning {
        argv,
        cwd: fs::read_link(format!("/proc/{job}/cwd")).ok(),
        shell: command_line(child).into_iter().next(),
    })
}

/// Its controlling tty and that tty's foreground process group (`tty_nr` and `tpgid` in `/proc/<pid>/stat`)
fn tty_and_foreground(pid: u32) -> Option<(i64, i64)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // after the command name, which may have spaces: state, ppid, pgrp, session, tty_nr, tpgid
    let mut fields = stat.rsplit_once(')')?.1.split_whitespace().skip(4);
    Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
}

/// Stands in for a terminal being restored, asking whether to run the program that was running in it again
struct RestorePrompt {
    wl: SavedWl,
    hook: AppletContext,
}
impl RestorePrompt {
    fn command(&self) -> String {
        self.wl
            .running
            .as_ref()
            .map(|running| {
                let mut argv = running.argv.clone();
                if let Some(program) = argv.first_mut() {
                    *program = base_name(program);
                }
                argv.join(" ")
            })
            .unwrap_or_default()
    }
}
impl StandardApplet for RestorePrompt {
    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        let StandardEvent::UIEvent(ui_event @ UIEvent::Key { .. }) = standard_event else {
            return;
        };
        if let Some(request) = standard_keybind_request(&ui_event) {
            self.hook.request(request);
            return;
        }
        let UIEvent::Key {
            symbol: Some(key),
            pressed: true,
            ..
        } = ui_event
        else {
            return;
        };
        let mut wl = self.wl.clone();
        match key {
            KeySymbol::Char('y') | KeySymbol::Enter => {}
            KeySymbol::Char('n') | KeySymbol::Escape => wl.running = None,
            _ => return,
        }
        self.hook.request(NodeRequest::StartWl(wl));
    }

    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree {
        let cwd = self
            .wl
            .running
            .as_ref()
            .and_then(|running| running.cwd.as_ref())
            .map(|cwd| format!(" in {}", cwd.display()))
            .unwrap_or_default();
        let text = format!(
            "{} was running `{}`{cwd}.\n\nRun it again?\n  y / Enter: yes\n  n / Esc: no, just the terminal",
            wl_label(&self.wl),
            self.command()
        );
        ui::inset(ui::text(text, Color::LIGHT_GRAY), container_size, ui::PAD)
    }

    fn get_treeview(&self) -> WorldTree<String> {
        WorldTree::Base(format!("Restore `{}`?", self.command()))
    }

    fn save(&self) -> Option<SavedApplet> {
        Some(SavedApplet::Wl(self.wl.clone()))
    }
}

/// Stands in for an applet being restored: a wl client starting, or one that couldn't be made.
/// Stays saved as it was, until it is closed (Ctrl+W).
struct RestoringApplet {
    saved: SavedApplet,
    label: String,
    hook: AppletContext,
}
impl RestoringApplet {
    fn initializer(saved: SavedApplet, label: String) -> NodularAppletInitializer {
        let holds_children = holds_children(&saved);
        let mut initializer =
            NodularAppletInitializer::new(move |hook| Box::new(Self { saved, label, hook }));
        initializer.holds_children = holds_children;
        initializer
    }
}
impl StandardApplet for RestoringApplet {
    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        if let StandardEvent::UIEvent(ui_event @ UIEvent::Key { .. }) = standard_event
            && let Some(request) = standard_keybind_request(&ui_event)
        {
            self.hook.request(request);
        }
    }

    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree {
        ui::inset(
            ui::text(self.label.clone(), Color::LIGHT_GRAY),
            container_size,
            ui::PAD,
        )
    }

    fn get_treeview(&self) -> WorldTree<String> {
        WorldTree::Base(self.label.clone())
    }

    fn save(&self) -> Option<SavedApplet> {
        Some(self.saved.clone())
    }
}
