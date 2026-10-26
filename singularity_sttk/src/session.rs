//! Saving the tree and restoring it on the next start (see `.claude/SESSIONS.md`)

use crate::{
    command_hub::program_command,
    nodular_applet::{AppletContext, NodularAppletInitializer, StandardApplet},
    standard_keybinds::standard_keybind_request,
};
use singularity_common::{
    sap::{
        packets::{AwaitedWl, StandardEvent},
        session::{SavedApplet, SavedTree, SavedWl, Session, title_without_url},
    },
    utils::tree::world_tree::WorldTree,
};
use sonamu_ui::{
    color::Color,
    display_units::DisplayContainerSize,
    ui_element::{Style, UIElement, UITree},
    ui_event::UIEvent,
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

/// Saved as made by the spawner registered as `name` (in `singularity_sde`'s `main`) from this path, made absolute
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

/// Makes the saved applet again. A wl client isn't started: its tile hibernates until `wake`.
pub fn restore(saved: SavedApplet) -> NodularAppletInitializer {
    match &saved {
        SavedApplet::Spawner { name, args, inner } => {
            let (name, args, inner) = (name.clone(), args.clone(), inner.clone());
            let initializer: NodularAppletInitializer = Box::new(move |hook: AppletContext| {
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                match hook
                    .find_applet_spawner(&name)
                    .and_then(|spawner| match inner {
                        Some(inner) => spawner.restore_initializer(&args, *inner),
                        None => spawner.create_initializer(&args),
                    }) {
                    Some(initializer) => initializer(hook),
                    None => Box::new(RestoringApplet {
                        label: format!("Couldn't restore {name} {}", args.join(" ")),
                        saved,
                        hook,
                    }),
                }
            });
            initializer
        }
        SavedApplet::Wl(wl) => {
            let label = format!("{} (inactive)", wl_label(wl));
            RestoringApplet::initializer(saved, label)
        }
    }
}

/// Starts a hibernating wl client. Its tile stands in as "Starting ..." and awaits its toplevel.
pub fn wake(wl: &SavedWl) -> (NodularAppletInitializer, Option<AwaitedWl>) {
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
/// their id with the browser they ran in; other clients run as they were
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
        _ if let Some((id, profile)) = wl.web_app() => vec![
            program_name(wl.argv.first()?),
            format!("--app-id={id}"),
            format!("--profile-directory={profile}"),
        ],
        _ => {
            let (program, args) = wl.argv.split_first()?;
            std::iter::once(program_name(program))
                .chain(args.iter().cloned())
                .collect()
        }
    };
    let (program, args) = argv.split_first()?;
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut command = program_command(program, &args);
    if let Some(cwd) = wl.cwd.as_ref().filter(|cwd| cwd.is_dir()) {
        command.current_dir(cwd);
    }
    Some(command)
}

/// A Nix store path by its name, since the store path may be gone (eg garbage collected after an update),
/// and the unwrapped binary (`.foo-wrapped`) by its wrapper's
fn program_name(program: &str) -> String {
    if !program.starts_with("/nix/store/") {
        return program.to_string();
    }
    let name = Path::new(program)
        .file_name()
        .map_or_else(|| program.into(), |name| name.to_string_lossy());
    let name = name.strip_prefix('.').unwrap_or(&name);
    name.strip_suffix("-wrapped").unwrap_or(name).to_string()
}

/// The wl client with this pid, as it is now: its command line from `/proc`,
/// and its first child's working directory (eg a terminal's shell), else its own
pub fn saved_wl(pid: Option<u32>, app_id: Option<String>, title: &str) -> SavedWl {
    let argv = pid
        .and_then(|pid| fs::read(format!("/proc/{pid}/cmdline")).ok())
        .map(|cmdline| {
            cmdline
                .split(|&byte| byte == 0)
                .filter(|arg| !arg.is_empty())
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect()
        })
        .unwrap_or_default();
    SavedWl {
        argv,
        cwd: pid.and_then(working_dir),
        app_id,
        title: title.to_string(),
    }
}

fn working_dir(pid: u32) -> Option<PathBuf> {
    let children =
        fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")).unwrap_or_default();
    let process = children
        .split_whitespace()
        .next()
        .map_or_else(|| pid.to_string(), String::from);
    fs::read_link(format!("/proc/{process}/cwd")).ok()
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
        Box::new(move |hook| Box::new(Self { saved, label, hook }))
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
        let margin = 16.0;
        UITree::leaf_styled(
            UIElement::Text(vec![(
                self.label.clone(),
                UIElement::glyphon_attr(Color::LIGHT_GRAY),
            )]),
            Style::absolute(
                [margin, margin],
                [
                    (container_size.width as f32 - 2.0 * margin).max(0.0),
                    (container_size.height as f32 - 2.0 * margin).max(0.0),
                ],
            ),
        )
    }

    fn get_treeview(&self) -> WorldTree<String> {
        WorldTree::Base(self.label.clone())
    }

    fn save(&self) -> Option<SavedApplet> {
        Some(self.saved.clone())
    }
}
