//! User settings, read from the config dir. See `.claude/SETTINGS.md`.

use crate::notification::Replacing;
use indexmap::IndexMap;
use notify::Watcher as _;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use sonamu_ui::{
    color::Color,
    display_units::ToPx,
    ui_event::{KeyModifiers, KeySymbol, UIEvent},
};
use std::{
    collections::HashSet,
    fs, io,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::{
        LazyLock, Mutex, PoisonError, RwLock,
        atomic::{AtomicU8, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
    time::{Duration, Instant},
};

const PROGRAM_TIMEOUT: Duration = Duration::from_secs(5);
/// Every setting; a settings file or program only gives the ones it changes
pub(crate) const DEFAULTS: &str = include_str!("../../assets/default_settings.json");

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
    #[schemars(range(min = 0, max = 64))]
    pub gap: f32,
    /// Of pane corners, in px
    #[schemars(range(min = 0, max = 64))]
    pub radius: f32,
    /// Border, gap and radius also on a pane alone in its tree, and on the treeview
    pub always_frame: bool,
    /// Of Sonamu's own translucent backgrounds and treeview (applets, panels, rows), so what's behind shows through:
    /// 1 is opaque. Menus and popups are always opaque
    #[schemars(range(min = 0, max = 1))]
    pub opacity: f32,
    /// Drawn behind everything, even wallpaper programs' layers (#104); on battery, `battery_wallpaper` if set
    pub wallpaper: Wallpaper,
    /// The wallpaper while on battery (eg a still image instead of a shader): `same` is `wallpaper`
    pub battery_wallpaper: BatteryWallpaper,
    /// Text height in px (Sonamu's own UI isn't scaled)
    #[schemars(range(min = 8, max = 64))]
    pub font_size: f32,
    /// Smaller copies of windows and images, so they look smooth when drawn small (eg the zoom-out view); a bit
    /// more GPU work each time a window updates
    pub mipmaps: bool,
    pub treeview: TreeviewSettings,
    /// When the buffer line (a prompt at the bottom, like vim's command line: the agent, `!` Luau, `$` shell) shows
    pub buffer_line: BufferLine,
    /// The buffer line's agent: an OpenAI-compatible server (eg llama.cpp's `llama-server` at
    /// `http://127.0.0.1:8080`); `None`: no agent
    pub agent_url: Option<String>,
    pub swipe: SwipeSettings,
    /// The zoom-out view (#145)
    pub zoom_out: ZoomOutSettings,
    /// Desktop notifications, shown by the status bar (#150)
    pub notifications: NotificationSettings,
    /// Global shortcuts in named groups (shown as help sections): Sonamu commands or shell commands. Given, it
    /// replaces the defaults (list the groups to keep)
    pub keybinds: IndexMap<String, Vec<Keybind>>,
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
    /// The wallpaper to draw: `battery_wallpaper` on battery if it's set, else `wallpaper`
    #[must_use]
    pub fn wallpaper_now(&self, on_battery: bool) -> &Wallpaper {
        match &self.battery_wallpaper.0 {
            Some(battery) if on_battery => battery,
            _ => &self.wallpaper,
        }
    }

    /// `gap` and `border.width`, rounded to whole px
    #[must_use]
    pub fn gap_and_border_px(&self) -> (u32, u32) {
        (self.gap.round().to_px(), self.border.width.round().to_px())
    }

    /// `opacity` as a color's alpha
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to 0..=255"
    )]
    pub fn alpha(&self) -> u8 {
        (self.opacity.clamp(0.0, 1.0) * 255.0).round() as u8
    }

    /// Every keybind: groups in order, then binds in order
    pub fn all_keybinds(&self) -> impl Iterator<Item = &Keybind> {
        self.keybinds.values().flatten()
    }
}

/// What's drawn behind everything
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Wallpaper {
    /// Nothing: what's under shows (eg a wallpaper program's layer, else black)
    None,
    Color(#[schemars(with = "ColorSchema")] Color),
    /// An image file (`~` or relative to the config dir), covering each screen
    Image(#[schemars(extend("format" = "path"))] PathBuf),
    /// A Shadertoy-style fragment shader, reloaded when the file changes. GLSL: `void mainImage(out vec4 fragColor,
    /// in vec2 fragCoord)` with `iTime`, `iResolution`, `iFrame`, `iMouse`. A `.wgsl` file: `fn main_image(position: vec2f)
    /// -> vec4f` with `wallpaper.time`, `.resolution`, `.frame` (`u32`), `.mouse`; it may declare `wallpaper` itself
    /// (`struct Wallpaper { resolution: vec2f, time: f32, frame: u32, mouse: vec4f }` at group 0, binding 0)
    Shader(ShaderWallpaper),
}

/// `battery_wallpaper`: a `Wallpaper`, or `"same"` (`None`) for `wallpaper`'s
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BatteryWallpaper(pub Option<Wallpaper>);
impl<'de> Deserialize<'de> for BatteryWallpaper {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value == "same" {
            return Ok(Self(None));
        }
        Wallpaper::deserialize(value)
            .map(|wallpaper| Self(Some(wallpaper)))
            .map_err(serde::de::Error::custom)
    }
}
impl JsonSchema for BatteryWallpaper {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "BatteryWallpaper".into()
    }

    /// `Wallpaper`'s kinds, `same` first
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = Wallpaper::json_schema(generator);
        if let Some(serde_json::Value::Array(kinds)) = schema.get_mut("oneOf") {
            kinds.insert(
                0,
                serde_json::json!({
                    "description": "The same as `wallpaper`",
                    "type": "string",
                    "const": "same"
                }),
            );
        }
        schema.insert(
            "description".to_owned(),
            "A wallpaper, or `same` for `wallpaper`'s".into(),
        );
        schema
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
pub struct ShaderWallpaper {
    /// The shader file (`~` or relative to the config dir)
    #[schemars(extend("format" = "path"))]
    pub path: PathBuf,
    /// Drawn at this fraction of the screen's resolution and stretched: lower is cheaper
    #[serde(default = "full_scale", deserialize_with = "scale_fraction")]
    #[schemars(range(min = 0.05, max = 1))]
    pub scale: f32,
    /// Frames per second it's drawn at
    #[serde(default = "default_fps")]
    #[schemars(range(min = 1, max = 240))]
    pub fps: u32,
}

const fn full_scale() -> f32 {
    1.0
}

const fn default_fps() -> u32 {
    30
}

/// A scale in `(0, 1]`
fn scale_fraction<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<f32, D::Error> {
    let scale = f32::deserialize(deserializer)?;
    if scale > 0.0 && scale <= 1.0 {
        Ok(scale)
    } else {
        Err(serde::de::Error::custom(format!(
            "scale {scale} isn't in (0, 1]"
        )))
    }
}

/// A path from the settings: `~/` is the home dir, relative ones are in the config dir `dir`
#[must_use]
pub fn config_path(path: &Path, dir: &Path) -> PathBuf {
    match (path.strip_prefix("~"), std::env::var_os("HOME")) {
        (Ok(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => dir.join(path),
    }
}

/// `path` as a setting writes it (`config_path` reverses it): relative inside the config dir `dir` (also through
/// symlinks), else `~/` inside
/// the home dir, else as it is
#[must_use]
pub fn config_relative(path: &Path, dir: &Path) -> PathBuf {
    if let Ok(inside) = path.strip_prefix(dir) {
        return inside.to_owned();
    }
    // the config dir may be a symlink (eg to a dotfiles repo), reached either way
    if let (Ok(path), Ok(dir)) = (fs::canonicalize(path), fs::canonicalize(dir))
        && let Ok(inside) = path.strip_prefix(dir)
    {
        return inside.to_owned();
    }
    match std::env::var_os("HOME").and_then(|home| path.strip_prefix(home).ok().map(Path::to_owned))
    {
        Some(rest) => Path::new("~").join(rest),
        None => path.to_owned(),
    }
}

/// Does `action` when `key` is pressed with exactly `mods` held
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
pub struct Keybind {
    /// Any of `ctrl`, `alt`, `shift`, `logo`
    #[serde(default)]
    pub mods: Vec<String>,
    pub key: Key,
    #[serde(flatten)]
    pub action: Action,
}

/// What a keybind does
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Action {
    /// Runs it with `sh -c`
    Shell { shell: String },
    /// A Sonamu command (see `.claude/SETTINGS.md`), eg `close`
    Command {
        command: String,
        #[serde(default)]
        args: Vec<serde_json::Value>,
    },
}

/// A character (case-insensitive), a key's name, or an evdev code for other keys (eg 115 is volume up)
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum Key {
    Code(u32),
    Char(char),
    Named(NamedKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NamedKey {
    Left,
    Right,
    Up,
    Down,
    Enter,
    Tab,
    Escape,
    Backspace,
    PageUp,
    PageDown,
}
impl NamedKey {
    fn is(self, symbol: KeySymbol) -> bool {
        Self::of(symbol) == Some(self)
    }

    /// The named key `symbol` is, if any
    #[must_use]
    pub const fn of(symbol: KeySymbol) -> Option<Self> {
        Some(match symbol {
            KeySymbol::ArrowKeyLeft => Self::Left,
            KeySymbol::ArrowKeyRight => Self::Right,
            KeySymbol::ArrowKeyUp => Self::Up,
            KeySymbol::ArrowKeyDown => Self::Down,
            KeySymbol::Enter | KeySymbol::Char('\n' | '\r') => Self::Enter,
            KeySymbol::Char('\t') => Self::Tab,
            KeySymbol::Escape => Self::Escape,
            KeySymbol::Backspace => Self::Backspace,
            KeySymbol::PageUp => Self::PageUp,
            KeySymbol::PageDown => Self::PageDown,
            KeySymbol::Char(_) => return None,
        })
    }
}

impl Keybind {
    pub fn matches(
        &self,
        symbol: Option<KeySymbol>,
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
                Key::Char(c) => {
                    matches!(symbol, Some(KeySymbol::Char(s)) if s.to_lowercase().eq(c.to_lowercase()))
                }
                Key::Named(named) => symbol.is_some_and(|symbol| named.is(symbol)),
            }
    }

    /// A pressed key as a keybind's mods and key (eg while recording one); `None` for a modifier key
    #[must_use]
    pub fn recorded(
        symbol: Option<KeySymbol>,
        raw_keycode: u32,
        modifiers: KeyModifiers,
    ) -> Option<(Vec<String>, Key)> {
        // evdev: ctrl, shift, alt, logo (left and right)
        if matches!(raw_keycode, 29 | 42 | 54 | 56 | 97 | 100 | 125 | 126) {
            return None;
        }
        let mods = [
            ("ctrl", modifiers.ctrl),
            ("alt", modifiers.alt),
            ("shift", modifiers.shift),
            ("logo", modifiers.logo),
        ]
        .into_iter()
        .filter(|&(_, held)| held)
        .map(|(name, _)| name.to_owned())
        .collect();
        let key = match symbol {
            Some(symbol) if let Some(named) = NamedKey::of(symbol) => Key::Named(named),
            Some(KeySymbol::Char(c)) => Key::Char(c.to_lowercase().next().unwrap_or(c)),
            _ => Key::Code(raw_keycode),
        };
        Some((mods, key))
    }

    /// `matches` for a key with a character (or none), eg Luau's `keymap.set`
    pub fn matches_char(&self, symbol: Option<char>, raw_keycode: u32, mods: [bool; 4]) -> bool {
        self.matches(symbol.map(KeySymbol::Char), raw_keycode, mods)
    }

    /// Runs a shell keybind's command without waiting for it
    pub fn run_shell(command: &str) {
        let mut child = Command::new("sh");
        child.arg("-c").arg(command);
        // connect wl clients (grim, swappy, ...) to Sonamu
        crate::connect_wl_client(&mut child);
        match child.spawn() {
            Ok(mut child) => {
                // reap it
                thread::spawn(move || child.wait());
            }
            Err(err) => eprintln!("keybind `{command}` failed: {err}"),
        }
    }
}

/// The command (and its args) of the first keybind for this key press with `modifiers`, if it's a command
#[must_use]
pub fn keybind_command(
    ui_event: &UIEvent,
    modifiers: KeyModifiers,
) -> Option<(String, Vec<serde_json::Value>)> {
    let UIEvent::Key {
        symbol,
        raw_keycode,
        pressed: true,
        ..
    } = *ui_event
    else {
        return None;
    };
    let mods = [
        modifiers.ctrl,
        modifiers.alt,
        modifiers.shift,
        modifiers.logo,
    ];
    match get()
        .all_keybinds()
        .find(|keybind| keybind.matches(symbol, raw_keycode, mods))?
        .action
        .clone()
    {
        Action::Command { command, args } => Some((command, args)),
        Action::Shell { .. } => None,
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
    #[schemars(range(min = 10, max = 60))]
    pub width: u32,
    /// Where an unpinned one floats (only while focus is on a node)
    pub floating: TreeviewFloating,
    /// A `Center` floating one's width and height, in percent of the window's
    pub floating_size: [u32; 2],
}

/// 3 finger touchpad swipes between nodes (#141): left / right between siblings, up / down between parent and child (#144)
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[schemars(transform = optional_fields)]
pub struct SwipeSettings {
    /// How far the fingers move left / right to go one node, in px
    #[schemars(range(min = 100, max = 5000))]
    pub distance_x: f32,
    /// How far the fingers move up / down to go one node, in px
    #[schemars(range(min = 100, max = 5000))]
    pub distance_y: f32,
    /// How far past the first / last node it stretches at most, in nodes
    #[schemars(range(min = 0, max = 1))]
    pub rubber_band: f32,
    /// Of the spring settling on a node after the fingers lift (mass 1): higher is faster
    #[schemars(range(min = 100, max = 5000))]
    pub stiffness: f32,
    /// Of that spring: 1 is critically damped (no bounce), below 1 overshoots and bounces, above 1 is sluggish
    #[schemars(range(min = 0.1, max = 2))]
    pub damping_ratio: f32,
}

/// Desktop notifications (#150)
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[schemars(transform = optional_fields)]
pub struct NotificationSettings {
    /// How long a new one pops up, in seconds (unless it asks for its own time; critical ones stay)
    #[schemars(range(min = 0, max = 60))]
    pub timeout: f32,
}

/// The zoom-out view (#145); pinches scale it
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[schemars(transform = optional_fields)]
pub struct ZoomOutSettings {
    /// Tiny previews of the children of the nodes beside each row's own, under their panes
    pub child_previews: bool,
    /// Whether the view floats freely (default), or keeps its center in the middle
    pub view: ZoomOutView,
}

/// How the zoom-out view moves
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZoomOutView {
    /// It pans to keep its center (where clicks, arrows and 2 finger scrolls move it) in the middle
    Center,
    /// 2 finger scrolls move it freely; clicks and arrows still move its center, but it stays put
    Free,
}

/// When the buffer line shows
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BufferLine {
    /// Only in buffer mode (Super+Esc), over the panes
    BufferMode,
    /// Always, below the panes; `INSERT` while not in buffer mode
    Always,
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
    /// The command hub's, buffer line's and help's background (translucent; their bars a bit toward `fg`). Not behind the
    /// panes: the background layer (eg a wallpaper) is there
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
impl Palette {
    /// `bg` moved `percent` toward `fg`: shades of the background (eg panels 5, menus 10, bars and hovered rows 15,
    /// guides 25); `bg`'s alpha
    #[must_use]
    pub fn raised(&self, percent: u8) -> Color {
        mix(self.bg, self.fg, percent)
    }

    /// `bg` moved `percent` toward black: darker shades of the background (eg the project overview's header 30)
    #[must_use]
    pub fn sunken(&self, percent: u8) -> Color {
        mix(self.bg, Color::BLACK, percent)
    }

    /// `bg` moved `percent` toward `accent` (eg a selected row)
    #[must_use]
    pub fn tinted(&self, percent: u8) -> Color {
        mix(self.bg, self.accent, percent)
    }
}

/// `from` moved `percent` toward `to`, keeping `from`'s alpha
fn mix(Color(from): Color, Color(to): Color, percent: u8) -> Color {
    let percent = u16::from(percent.min(100));
    let channel = |index: usize| {
        let mixed =
            (u16::from(from[index]) * (100 - percent) + u16::from(to[index]) * percent) / 100;
        u8::try_from(mixed).unwrap_or(u8::MAX)
    };
    Color([channel(0), channel(1), channel(2), from[3]])
}

/// Text for errors: the palette has no color for them
pub const ERROR: Color = Color([0xFF, 0x6A, 0x6A, 0xFF]);

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[schemars(transform = optional_fields)]
pub struct Border {
    /// Of panes, in px
    #[schemars(range(min = 0, max = 16))]
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

/// `Settings::alpha` of the current settings, read without cloning them (drawing asks for it often)
static ALPHA: AtomicU8 = AtomicU8::new(0xCC);

/// `Settings::alpha` of the current settings
pub fn alpha() -> u8 {
    ALPHA.load(Ordering::Relaxed)
}

/// Whether `a` and `b` differ, keybind groups' order too (maps compare without it)
#[must_use]
pub fn differs(a: &Settings, b: &Settings) -> bool {
    a != b || !a.keybinds.keys().eq(b.keybinds.keys())
}

/// The settings' JSON schema (`assets/settings.schema.json`)
#[must_use]
pub fn schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(Loaded)).expect("schema is JSON")
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
pub(crate) fn parse(text: &str) -> Result<Loaded, LoadError> {
    let mut value: serde_json::Value =
        serde_json::from_str(DEFAULTS).expect("valid default settings");
    let over: serde_json::Value = serde_json::from_str(text)?;
    // given keybinds replace the defaults' groups, not merge with them
    if over.get("keybinds").is_some()
        && let Some(defaults) = value.as_object_mut()
    {
        defaults.remove("keybinds");
    }
    merge(&mut value, over);
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

pub(crate) fn is_executable(path: &Path) -> bool {
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
/// The notification about the last (re)load, replaced by the next
static NOTIFICATION: Replacing = Replacing::new();

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

/// Loads the settings into [`get`]. On failure, keeps the current ones and logs.
pub fn reload() {
    if let Some(dir) = config_dir() {
        reload_from(&dir, true);
    }
}

/// Returns the extra paths to watch, `None` on failure. Notifies how it went (#143), except a first load that worked.
fn reload_from(dir: &Path, reloading: bool) -> Option<Vec<PathBuf>> {
    match load(dir) {
        Ok(Loaded { settings, watch }) => {
            // read by sonamu_ui itself (eg char grid sizes), which can't see the settings
            sonamu_ui::ui_element::set_font_size(settings.font_size);
            sonamu_ui::mipmaps::set_enabled(settings.mipmaps);
            ALPHA.store(settings.alpha(), Ordering::Relaxed);
            if let Err(err) = crate::alacritty_theme::write(&settings) {
                log::warn!("Could not write Alacritty's theme: {err}");
            }
            let changed = {
                let mut current = CURRENT.write().unwrap_or_else(PoisonError::into_inner);
                let old = std::mem::replace(&mut *current, settings);
                differs(&old, &current)
            };
            if reloading {
                NOTIFICATION.send("Settings reloaded".to_string(), String::new(), false);
            }
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
            NOTIFICATION.send(
                "Settings not loaded, keeping the previous ones".to_string(),
                err.to_string(),
                true,
            );
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
    let extra = reload_from(&dir, false);
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
                if let Some(extra) = reload_from(&self.dir, true) {
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

    #[test]
    fn example_config_parses() {
        let text = include_str!("../../examples/config/settings.json");
        assert!(parse(text).unwrap().settings.all_keybinds().count() > 1);
    }

    #[test]
    fn keybinds_replace_defaults_whole() {
        let loaded = parse(r#"{"keybinds": {"Mine": [{"key": "x", "shell": "true"}]}}"#).unwrap();
        assert_eq!(
            loaded.settings.keybinds.keys().collect::<Vec<_>>(),
            ["Mine"]
        );
    }

    #[test]
    fn keybind_groups_keep_order() {
        let groups: Vec<_> = Settings::default().keybinds.into_keys().collect();
        assert_eq!(groups, ["Global", "Windows", "Tree", "Traversal", "Swipe"]);
    }

    #[test]
    fn flat_keybinds_fail() {
        assert!(parse(r#"{"keybinds": [{"key": "x", "shell": "true"}]}"#).is_err());
    }

    /// Writes `assets/settings.schema.json` if it's out of date
    #[test]
    fn schema_is_current() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../assets/settings.schema.json"
        );
        let schema = serde_json::to_string_pretty(&schema()).unwrap() + "\n";
        if fs::read_to_string(path).ok().as_deref() != Some(&schema) {
            fs::write(path, schema).unwrap();
            panic!("{path} was out of date and is rewritten; commit it");
        }
    }

    #[test]
    fn font_size_range_in_schema() {
        let font = &schema()["properties"]["font_size"];
        assert_eq!(
            (font["minimum"].as_f64(), font["maximum"].as_f64()),
            (Some(8.0), Some(64.0))
        );
    }

    /// Every default is inside its schema range
    #[test]
    fn defaults_within_ranges() {
        fn check(
            schema: &serde_json::Value,
            defs: &serde_json::Value,
            value: &serde_json::Value,
            path: &str,
        ) {
            let schema = match schema["$ref"].as_str() {
                Some(reference) => &defs[reference.trim_start_matches("#/$defs/")],
                None => schema,
            };
            if let (Some(number), Some(min)) = (value.as_f64(), schema["minimum"].as_f64()) {
                assert!(number >= min, "{path}");
            }
            if let (Some(number), Some(max)) = (value.as_f64(), schema["maximum"].as_f64()) {
                assert!(number <= max, "{path}");
            }
            if let Some(object) = value.as_object() {
                for (key, value) in object {
                    check(
                        &schema["properties"][key],
                        defs,
                        value,
                        &format!("{path}.{key}"),
                    );
                }
            }
        }
        let schema = schema();
        let defaults: serde_json::Value = serde_json::from_str(DEFAULTS).unwrap();
        check(&schema, &schema["$defs"], &defaults, "");
    }

    /// Reordered keybind groups are a change (maps compare without order)
    #[test]
    fn settings_differ_on_group_order() {
        let settings = Settings::default();
        let mut reordered = settings.clone();
        reordered.keybinds.reverse();
        assert_eq!(settings, reordered);
        assert!(differs(&settings, &reordered));
        assert!(!differs(&settings, &settings.clone()));
    }

    /// `opacity` (0..=1) as a color's alpha byte; the default matches the old fixed 0xCC
    #[test]
    fn opacity_to_alpha() {
        let with = |opacity| Settings {
            opacity,
            ..Settings::default()
        };
        assert_eq!(Settings::default().alpha(), 0xCC);
        assert_eq!(with(1.0).alpha(), 0xFF);
        assert_eq!(with(0.0).alpha(), 0);
        assert_eq!(with(2.0).alpha(), 0xFF);
    }

    #[test]
    fn wallpaper_kinds() {
        let wallpaper = |json: &str| {
            parse(&format!(r#"{{"wallpaper": {json}}}"#))
                .unwrap()
                .settings
                .wallpaper
        };
        assert_eq!(Settings::default().wallpaper, Wallpaper::None);
        assert_eq!(
            wallpaper(r##"{"color": "#102030"}"##),
            Wallpaper::Color(Color([0x10, 0x20, 0x30, 0xff]))
        );
        assert_eq!(
            wallpaper(r#"{"image": "~/a.png"}"#),
            Wallpaper::Image(PathBuf::from("~/a.png"))
        );
        assert_eq!(
            wallpaper(r#"{"shader": {"path": "sine.glsl"}}"#),
            Wallpaper::Shader(ShaderWallpaper {
                path: PathBuf::from("sine.glsl"),
                scale: 1.0,
                fps: 30,
            })
        );
        assert!(parse(r#"{"wallpaper": {"shader": {"path": "x", "scale": 0}}}"#).is_err());
    }

    #[test]
    fn battery_wallpaper() {
        let same = parse(r##"{"wallpaper": {"color": "#102030"}}"##)
            .unwrap()
            .settings;
        assert_eq!(same.battery_wallpaper, BatteryWallpaper(None));
        assert_eq!(same.wallpaper_now(true), &same.wallpaper);
        let own = parse(r#"{"wallpaper": {"shader": {"path": "s.glsl"}}, "battery_wallpaper": {"image": "i.png"}}"#)
            .unwrap()
            .settings;
        assert_eq!(own.wallpaper_now(false), &own.wallpaper);
        assert_eq!(
            own.wallpaper_now(true),
            &Wallpaper::Image(PathBuf::from("i.png"))
        );
        assert!(parse(r#"{"battery_wallpaper": "other"}"#).is_err());
        let kinds: Vec<_> = schema()["$defs"]["BatteryWallpaper"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|kind| kind["const"].as_str().map(str::to_owned))
            .collect();
        assert_eq!(kinds, ["same", "none"]);
    }

    #[test]
    fn config_paths() {
        let dir = Path::new("/config");
        assert_eq!(
            config_path(Path::new("w/sine.glsl"), dir),
            PathBuf::from("/config/w/sine.glsl")
        );
        assert_eq!(
            config_path(Path::new("/abs.png"), dir),
            PathBuf::from("/abs.png")
        );
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            config_path(Path::new("~/a.png"), dir),
            Path::new(&home).join("a.png")
        );
        // through a symlinked config dir
        let real = std::env::temp_dir().join(format!("sonamu-real-{}", std::process::id()));
        let link = real.with_extension("link");
        let _ = (fs::remove_dir_all(&real), fs::remove_file(&link));
        fs::create_dir_all(real.join("w")).unwrap();
        fs::write(real.join("w/a.png"), "").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(
            config_relative(&real.join("w/a.png"), &link),
            PathBuf::from("w/a.png")
        );
        fs::remove_file(&link).unwrap();
        fs::remove_dir_all(&real).unwrap();
        // config_relative reverses it
        for written in ["w/sine.glsl", "/abs.png", "~/a.png"] {
            let path = config_path(Path::new(written), dir);
            assert_eq!(config_relative(&path, dir), PathBuf::from(written));
        }
    }

    #[test]
    fn recorded_keys() {
        let logo = KeyModifiers {
            logo: true,
            ..KeyModifiers::NONE
        };
        let recorded = |symbol, code, mods| Keybind::recorded(symbol, code, mods);
        assert_eq!(
            recorded(Some(KeySymbol::Char('T')), 20, logo),
            Some((vec!["logo".to_owned()], Key::Char('t')))
        );
        assert_eq!(
            recorded(Some(KeySymbol::Escape), 1, logo),
            Some((vec!["logo".to_owned()], Key::Named(NamedKey::Escape)))
        );
        assert_eq!(
            recorded(None, 115, KeyModifiers::NONE),
            Some((vec![], Key::Code(115)))
        );
        assert_eq!(recorded(None, 125, logo), None);
    }

    #[test]
    fn color_parsing() {
        assert_eq!("#11223344".parse(), Ok(Color([0x11, 0x22, 0x33, 0x44])));
        assert!("112233".parse::<Color>().is_err());
        assert!("#+12345".parse::<Color>().is_err());
        assert!("#12345".parse::<Color>().is_err());
    }
}
