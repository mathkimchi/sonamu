//! User settings, read from the config dir. See `.claude/SETTINGS.md`.

use notify::Watcher as _;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::Deserialize;
use sonamu_ui::{color::Color, display_units::ToPx};
use std::{
    collections::HashSet,
    fs, io,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::{
        LazyLock, Mutex, PoisonError, RwLock,
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

const PROGRAM_TIMEOUT: Duration = Duration::from_secs(5);
/// Every setting; a settings file or program only gives the ones it changes
const DEFAULTS: &str = include_str!("../../assets/default_settings.json");

/// Every field can be left out, since the defaults fill it in
fn optional_fields(schema: &mut Schema) {
    schema.remove("required");
}

/// `Color`'s schema (`#rrggbb` or `#rrggbbaa`)
struct ColorSchema;
impl JsonSchema for ColorSchema {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Color".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "pattern": "^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[schemars(transform = optional_fields)]
pub struct Settings {
    pub palette: Palette,
    pub border: Border,
    /// Between panes and around them, in px
    pub gap: f32,
    /// Of pane corners, in px
    pub radius: f32,
    /// Border, gap and radius also on a pane alone in its tree, and on the treeview
    pub always_frame: bool,
    /// Text height in px (Sonamu's own UI isn't scaled)
    pub font_size: f32,
    pub treeview: TreeviewSettings,
    pub swipe: SwipeSettings,
    /// Global shortcuts that run a shell command
    pub keybinds: Vec<Keybind>,
    /// Programs restored running in their terminal (see `.claude/SESSIONS.md`)
    pub restore_programs: Vec<RestoreProgram>,
    /// Shell commands run once when Sonamu starts (eg `dms run`)
    pub startup: Vec<String>,
    /// Program started by Super+T; `None`: `$TERMINAL`, else the first installed of a list
    pub terminal: Option<String>,
    /// Program started by Super+N; `None`: `$BROWSER`, else the first installed of a list
    pub browser: Option<String>,
    /// Program the file manager opens PDFs in; `None`: the first installed of zathura, okular, evince
    pub pdf_viewer: Option<String>,
}
impl Default for Settings {
    fn default() -> Self {
        serde_json::from_str(DEFAULTS).expect("valid default settings")
    }
}
impl Settings {
    /// `gap` and `border.width`, rounded to whole px
    #[must_use]
    pub fn gap_and_border_px(&self) -> (u32, u32) {
        (self.gap.round().to_px(), self.border.width.round().to_px())
    }
}

/// Runs `command` with `sh -c` when `key` is pressed with exactly `mods` held.
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
pub struct Keybind {
    /// Any of `ctrl`, `alt`, `shift`, `logo`
    #[serde(default)]
    pub mods: Vec<String>,
    pub key: Key,
    pub command: String,
}

/// A character (case-insensitive), or an evdev code for keys without one (eg 115 is volume up)
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Key {
    Code(u32),
    Char(char),
}

impl Keybind {
    pub fn matches(
        &self,
        symbol: Option<char>,
        raw_keycode: u32,
        [ctrl, alt, shift, logo]: [bool; 4],
    ) -> bool {
        let has = |name: &str| self.mods.iter().any(|m| m.eq_ignore_ascii_case(name));
        has("ctrl") == ctrl
            && has("alt") == alt
            && has("shift") == shift
            && has("logo") == logo
            && match self.key {
                Key::Code(code) => code == raw_keycode,
                Key::Char(c) => symbol.is_some_and(|s| s.to_lowercase().eq(c.to_lowercase())),
            }
    }

    /// Runs the command without waiting for it.
    pub fn run(&self) {
        let mut command = Command::new("sh");
        command.arg("-c").arg(&self.command);
        // connect wl clients (grim, swappy, ...) to Sonamu
        crate::connect_wl_client(&mut command);
        match command.spawn() {
            Ok(mut child) => {
                // reap it
                thread::spawn(move || child.wait());
            }
            Err(err) => eprintln!("keybind `{}` failed: {err}", self.command),
        }
    }
}

/// A program (matched by its name) restored running in its terminal: with its saved args,
/// or with these instead (eg `claude` with `--continue`)
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RestoreProgram {
    Name(String),
    WithArgs { program: String, args: Vec<String> },
}
impl RestoreProgram {
    pub fn program(&self) -> &str {
        match self {
            Self::Name(program) | Self::WithArgs { program, .. } => program,
        }
    }
}

/// Whether the treeview is pinned is part of the session, toggled by its pin button
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[schemars(transform = optional_fields)]
pub struct TreeviewSettings {
    /// The side a pinned one is on
    pub side: TreeviewSide,
    /// The side column's width, in percent of the window's
    pub width: u32,
    /// Where an unpinned one floats (only while focus is on a node)
    pub floating: TreeviewFloating,
    /// A `Center` floating one's width and height, in percent of the window's
    pub floating_size: [u32; 2],
}

/// 3 finger touchpad swipes between nodes (#141): up / down between siblings, left / right between parent and child
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[schemars(transform = optional_fields)]
pub struct SwipeSettings {
    /// How far the fingers move left / right to go one node, in px
    pub distance_x: f32,
    /// How far the fingers move up / down to go one node, in px
    pub distance_y: f32,
    /// How far past the first / last node it stretches at most, in nodes
    pub rubber_band: f32,
    /// How long it takes to settle on a node after the fingers lift, in ms
    pub snap_ms: u32,
}

/// A column beside the panes, which get the rest of the width
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TreeviewSide {
    Left,
    Right,
}

/// Where an unpinned treeview floats over the panes, which keep their size
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TreeviewFloating {
    /// Over the middle
    Center,
    /// At `side`
    Side,
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[schemars(transform = optional_fields)]
pub struct Palette {
    /// Unused for now: behind the panes is the background layer (eg a wallpaper)
    #[schemars(with = "ColorSchema")]
    pub bg: Color,
    /// Text of the treeview and projects
    #[schemars(with = "ColorSchema")]
    pub fg: Color,
    /// Focus on a node itself (traversal keys move it): its border and treeview marker
    #[schemars(with = "ColorSchema")]
    pub accent: Color,
    /// Text of inactive (eg hibernated) nodes in the treeview
    #[schemars(with = "ColorSchema")]
    pub inactive: Color,
}
impl Default for Palette {
    fn default() -> Self {
        Settings::default().palette
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[schemars(transform = optional_fields)]
pub struct Border {
    /// Of panes, in px
    pub width: f32,
    /// Focus is inside the pane's applet (keys go to it); also its treeview marker
    #[schemars(with = "ColorSchema")]
    pub focused: Color,
    #[schemars(with = "ColorSchema")]
    pub unfocused: Color,
    /// Of the treeview, when framed (`always_frame`)
    #[schemars(with = "ColorSchema")]
    pub treeview: Color,
}
/// What the settings file or program gives.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, JsonSchema)]
pub struct Loaded {
    #[serde(flatten)]
    pub settings: Settings,
    /// Extra files to watch for changes, eg the ones the program reads.
    #[serde(default)]
    pub watch: Vec<PathBuf>,
}

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Timeout,
    Exit(ExitStatus),
    Json(serde_json::Error),
}
impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::Timeout => write!(f, "settings program timed out after {PROGRAM_TIMEOUT:?}"),
            Self::Exit(status) => write!(f, "settings program failed: {status}"),
            Self::Json(err) => write!(f, "invalid settings: {err}"),
        }
    }
}
impl std::error::Error for LoadError {}
impl From<io::Error> for LoadError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}
impl From<serde_json::Error> for LoadError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

/// `$SONAMU_CONFIG`, else `$XDG_CONFIG_HOME/sonamu`, else `~/.config/sonamu`.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SONAMU_CONFIG") {
        return (!dir.is_empty()).then(|| dir.into());
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("sonamu"))
}

/// Runs `settings` in `dir` if it is executable, else reads `settings.json`.
/// Defaults if there is neither.
pub fn load(dir: &Path) -> Result<Loaded, LoadError> {
    let program = dir.join("settings");
    let text = if is_executable(&program) {
        run(&program)?
    } else {
        match fs::read_to_string(dir.join("settings.json")) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Loaded::default()),
            Err(err) => return Err(err.into()),
        }
    };
    parse(&text)
}

/// `text`'s settings over the defaults
fn parse(text: &str) -> Result<Loaded, LoadError> {
    let mut value: serde_json::Value =
        serde_json::from_str(DEFAULTS).expect("valid default settings");
    merge(&mut value, serde_json::from_str(text)?);
    Ok(serde_json::from_value(value)?)
}

/// Puts `over` into `base`, keeping what `base` has in objects `over` leaves out
fn merge(base: &mut serde_json::Value, over: serde_json::Value) {
    match (base, over) {
        (serde_json::Value::Object(base), serde_json::Value::Object(over)) => {
            for (key, value) in over {
                match base.get_mut(&key) {
                    Some(base) => merge(base, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, over) => *base = over,
    }
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

fn run(program: &Path) -> Result<String, LoadError> {
    let mut child = Command::new(program)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
    // read while waiting, so a full pipe can't block the program
    let reader = thread::spawn(move || {
        let mut text = String::new();
        stdout.read_to_string(&mut text).map(|_| text)
    });

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > PROGRAM_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(LoadError::Timeout);
        }
        thread::sleep(Duration::from_millis(10));
    };
    if !status.success() {
        return Err(LoadError::Exit(status));
    }
    Ok(reader.join().expect("settings reader panicked")?)
}

static CURRENT: LazyLock<RwLock<Settings>> = LazyLock::new(Default::default);
static SUBSCRIBERS: Mutex<Vec<calloop::channel::Sender<()>>> = Mutex::new(Vec::new());
static ERROR_SUBSCRIBERS: Mutex<Vec<calloop::channel::Sender<String>>> = Mutex::new(Vec::new());
/// Why the last load failed, until one succeeds
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// The current settings.
pub fn get() -> Settings {
    CURRENT
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// `sender` gets a message whenever the settings change.
pub fn subscribe(sender: calloop::channel::Sender<()>) {
    SUBSCRIBERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(sender);
}

/// `sender` gets why loading the settings failed, whenever it does (the previous ones are kept), eg to show it;
/// right away if the last load failed (eg at startup)
pub fn subscribe_errors(sender: calloop::channel::Sender<String>) {
    if let Some(err) = LAST_ERROR
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
    {
        let _ = sender.send(err);
    }
    ERROR_SUBSCRIBERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(sender);
}

/// Loads the settings into [`get`]. On failure, keeps the current ones and logs.
pub fn reload() {
    if let Some(dir) = config_dir() {
        reload_from(&dir);
    }
}

/// Returns the extra paths to watch, `None` on failure.
fn reload_from(dir: &Path) -> Option<Vec<PathBuf>> {
    match load(dir) {
        Ok(Loaded { settings, watch }) => {
            // read by sonamu_ui itself (eg char grid sizes), which can't see the settings
            sonamu_ui::ui_element::set_font_size(settings.font_size);
            *LAST_ERROR.lock().unwrap_or_else(PoisonError::into_inner) = None;
            let changed = {
                let mut current = CURRENT.write().unwrap_or_else(PoisonError::into_inner);
                std::mem::replace(&mut *current, settings) != *current
            };
            if changed {
                // drops the subscribers that are gone
                SUBSCRIBERS
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .retain(|sender| sender.send(()).is_ok());
            }
            Some(watch.into_iter().map(|path| dir.join(path)).collect())
        }
        Err(err) => {
            log::warn!(
                "Keeping previous settings, could not load from {}: {err}",
                dir.display()
            );
            let err = err.to_string();
            ERROR_SUBSCRIBERS
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .retain(|sender| sender.send(err.clone()).is_ok());
            *LAST_ERROR.lock().unwrap_or_else(PoisonError::into_inner) = Some(err);
            None
        }
    }
}

/// Loads the settings, then reloads them on a thread whenever the settings file or program,
/// or a file it asked to watch, changes.
pub fn load_and_watch() {
    let Some(dir) = config_dir() else {
        log::warn!("No config dir, using default settings");
        return;
    };
    let extra = reload_from(&dir);
    let (event_sender, events) = mpsc::channel();
    let watcher = match notify::recommended_watcher(event_sender) {
        Ok(watcher) => watcher,
        Err(err) => {
            log::warn!("Settings won't live reload: {err}");
            return;
        }
    };
    let mut watcher = Watcher {
        watcher,
        dir,
        dirs: HashSet::new(),
        extra: HashSet::new(),
        // suspects: HashMap::new(),
        ignored: HashSet::new(),
    };
    watcher.sync(extra.unwrap_or_default());
    thread::spawn(move || watcher.run(&events));
}

/// How long changes must stop before reloading
const SETTLE: Duration = Duration::from_millis(100);
// /// Runs in a row a file must change during before it's taken as written by the program.
// /// Once could be a coincidence (eg DMS writes its settings while the program reads its colors).
// const LOOP_RUNS: u32 = 3;

struct Watcher {
    watcher: notify::RecommendedWatcher,
    dir: PathBuf,
    /// Files are watched through their dirs, so files replaced by a rename are seen too
    dirs: HashSet<PathBuf>,
    /// The extra paths from the last load
    extra: HashSet<PathBuf>,
    // /// Extra paths that changed while the program ran, by how many runs in a row
    // suspects: HashMap<PathBuf, u32>,
    /// Extra paths the program changes itself, which would reload forever
    ignored: HashSet<PathBuf>,
}
impl Watcher {
    fn run(mut self, events: &Receiver<notify::Result<notify::Event>>) {
        while let Ok(event) = events.recv() {
            if self.changed_paths(event).is_empty() {
                continue;
            }
            let mut pending = true;
            while pending {
                self.settle(events);
                // let ran_program = is_executable(&self.dir.join("settings"));
                if let Some(extra) = reload_from(&self.dir) {
                    self.sync(extra);
                }
                // changed while loading: reload again
                let changed = self.settle(events);
                // Loop detection is off: DMS writes its settings continuously (eg while dragging a slider),
                // which looked like the program changing them. A program that changes its own `watch` files
                // reloads forever.
                // self.suspects.retain(|path, _| ran_program && changed.contains(path));
                // for path in &changed {
                //     if !ran_program || !self.extra.contains(path) {
                //         continue;
                //     }
                //     let runs = self.suspects.entry(path.clone()).or_default();
                //     *runs += 1;
                //     if *runs >= LOOP_RUNS {
                //         log::warn!(
                //             "The settings program changes {}, ignoring it so settings don't reload forever",
                //             path.display()
                //         );
                //         self.suspects.remove(path);
                //         self.ignored.insert(path.clone());
                //     }
                // }
                pending = changed.iter().any(|path| !self.ignored.contains(path));
            }
        }
    }

    /// The watched paths changed until nothing changes for `SETTLE`
    fn settle(&self, events: &Receiver<notify::Result<notify::Event>>) -> HashSet<PathBuf> {
        let mut changed = HashSet::new();
        while let Ok(event) = events.recv_timeout(SETTLE) {
            changed.extend(self.changed_paths(event));
        }
        changed
    }

    fn changed_paths(&self, event: notify::Result<notify::Event>) -> Vec<PathBuf> {
        let event = match event {
            Ok(event) => event,
            Err(err) => {
                log::warn!("Settings watch error: {err}");
                return Vec::new();
            }
        };
        if matches!(event.kind, notify::EventKind::Access(_)) {
            return Vec::new();
        }
        event
            .paths
            .into_iter()
            .filter(|path| self.is_watched(path))
            .collect()
    }

    fn is_watched(&self, path: &Path) -> bool {
        let program = self.dir.join("settings");
        // `settings.json` is unused while there is a program
        path == program
            || (path == self.dir.join("settings.json") && !is_executable(&program))
            || (self.extra.contains(path) && !self.ignored.contains(path))
    }

    fn sync(&mut self, extra: Vec<PathBuf>) {
        self.extra = extra.into_iter().collect();
        let dirs: HashSet<PathBuf> = std::iter::once(self.dir.clone())
            .chain(
                self.extra
                    .iter()
                    .filter_map(|path| path.parent().map(Path::to_path_buf)),
            )
            .collect();
        for dir in self.dirs.difference(&dirs) {
            let _ = self.watcher.unwatch(dir);
        }
        for dir in dirs.difference(&self.dirs) {
            if let Err(err) = self.watcher.watch(dir, notify::RecursiveMode::NonRecursive) {
                log::warn!("Can't watch {} for settings: {err}", dir.display());
            }
        }
        self.dirs = dirs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_json_uses_defaults() {
        let loaded = parse(r##"{"border": {"focused": "#a0cafd"}, "watch": ["/a"]}"##).unwrap();
        assert_eq!(
            loaded.settings.border,
            Border {
                focused: Color([0xa0, 0xca, 0xfd, 0xff]),
                ..Settings::default().border
            }
        );
        assert_eq!(loaded.settings.palette, Palette::default());
        assert_eq!(loaded.watch, vec![PathBuf::from("/a")]);
    }

    /// Writes `assets/settings.schema.json` if it's out of date
    #[test]
    fn schema_is_current() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../assets/settings.schema.json"
        );
        let schema = serde_json::to_string_pretty(&schemars::schema_for!(Loaded)).unwrap() + "\n";
        if fs::read_to_string(path).ok().as_deref() != Some(&schema) {
            fs::write(path, schema).unwrap();
            panic!("{path} was out of date and is rewritten; commit it");
        }
    }

    #[test]
    fn color_parsing() {
        assert_eq!("#11223344".parse(), Ok(Color([0x11, 0x22, 0x33, 0x44])));
        assert!("112233".parse::<Color>().is_err());
        assert!("#+12345".parse::<Color>().is_err());
        assert!("#12345".parse::<Color>().is_err());
    }
}
