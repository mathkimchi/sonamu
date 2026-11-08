//! A project: an applet holding the project's contents as its inner tree, like the root holds the outer tree.
//! Its node doesn't know it is a project; it only sees an applet with an inner tree.

use crate::{
    command_hub::{BG, CommandHubApplet},
    nodular_applet::{
        Answer, AppletContext, AppletSpawner, AppletSpawnerTrait, EmbeddedId, NodeRequest,
        NodularAppletInitializer, StandardApplet, WrapInitializer,
        node_holder::NodeHolder,
        node_tree::{AttachClient, NodeId, NodeTree, PROJECT_ICON, SpawnClient, root_label_mut},
    },
    session,
    standard_keybinds::{browser, terminal},
    ui::{self, PAD, Rows, row_height},
};
use sonamu_common::{
    sap::{
        detached::{DetachedClient, DetachedTree},
        packets::{
            AwaitedWl, NewWlSurface, NodeAction, OpenApplet, SpawnPosition, StandardEvent,
            StandardRequest,
        },
        session::{SavedApplet, SavedTree},
    },
    settings,
    utils::tree::world_tree::{WorldTree, WorldTreePath},
};
use sonamu_ui::{
    color::Color,
    display_units::{DisplayContainerSize, ToPx},
    ui_element::{Style, UIElement, UITree},
    ui_event::{KeyModifiers, KeySymbol, MouseEvent, MouseEventKind, UIEvent},
};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
};

/// Its name until it is renamed: its inner tree root's label
const LABEL: &str = "Project";

// The overview's rows (`overview_rows`): the name, its directory, its icon, a hint, its contents, then `NEW_ROWS`
const DIRECTORY_ROW: usize = 1;
const ICON_ROW: usize = 2;
/// The only row a click does nothing on
const HINT_ROW: usize = 3;
/// Rows before its contents (or the directory or icon options, while setting one)
const CONTENTS_ROW: usize = 4;
/// The icons the picker lists (any typed character works too)
const ICONS: [(char, &str); 20] = [
    (PROJECT_ICON, "briefcase"),
    ('\u{f07b}', "folder"),
    ('\u{f121}', "code"),
    ('\u{f120}', "terminal"),
    ('\u{f0ac}', "globe"),
    ('\u{f02d}', "book"),
    ('\u{f040}', "pencil"),
    ('\u{f19d}', "school"),
    ('\u{f0c3}', "flask"),
    ('\u{f135}', "rocket"),
    ('\u{f013}', "cog"),
    ('\u{f015}', "home"),
    ('\u{f073}', "calendar"),
    ('\u{f11b}', "game"),
    ('\u{f001}', "music"),
    ('\u{f030}', "camera"),
    ('\u{f005}', "star"),
    ('\u{f004}', "heart"),
    ('\u{f024}', "flag"),
    ('\u{f06c}', "leaf"),
];
/// Most directory options shown
const MAX_OPTIONS: usize = 20;
/// What a `NEW_ROWS` row adds
#[derive(Clone, Copy)]
enum New {
    CommandHub,
    /// An empty project, in this one's directory
    Project,
    Program(fn() -> String),
}
/// Rows after its contents that add to it: label, key, and what it adds
const NEW_ROWS: [(&str, char, New); 4] = [
    ("+ command hub  (n)", 'n', New::CommandHub),
    ("+ project  (p)", 'p', New::Project),
    ("+ terminal  (t)", 't', New::Program(terminal)),
    ("+ browser  (b)", 'b', New::Program(browser)),
];
// Other text is the settings' `palette.fg`
const HINT: Color = Color([0x8A, 0x93, 0xA6, 0xFF]);
const ERROR: Color = Color([0xE0, 0x6C, 0x75, 0xFF]);
const BTN_LEFT: u32 = 0x110;

/// The home directory, which paths show as `~`
/// The overview's rows, each a line of text at its top (above the zoom-out view)
fn overview_rows() -> Rows {
    Rows::new(PAD)
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// The path, with the home directory as `~`
fn tilde(path: &Path) -> String {
    match home().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// The typed path with `~` expanded; `None` if it is relative
fn expand(path: &str) -> Option<PathBuf> {
    if path == "~" {
        home()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home().map(|home| home.join(rest))
    } else {
        path.starts_with('/').then(|| PathBuf::from(path))
    }
}

/// A directory path being typed (absolute or from `~`), with the directories it could complete to
struct DirectoryInput {
    path: String,
    /// Paths, as typed, of the directories in `path`'s parent whose names start with its last part
    options: Vec<String>,
    /// The last Enter was on a path that isn't a directory
    invalid: bool,
    /// The option picked with the arrows, which Tab fills in
    selected: Option<usize>,
}
impl DirectoryInput {
    fn new(path: String) -> Self {
        let mut input = Self {
            path: String::new(),
            options: Vec::new(),
            invalid: false,
            selected: None,
        };
        input.set(path);
        input
    }

    fn set(&mut self, path: String) {
        self.options = path
            .rsplit_once('/')
            .and_then(|(parent, prefix)| {
                // only the root's parent is empty
                let parent = if parent.is_empty() { "/" } else { parent };
                let join = if parent == "/" { "" } else { "/" };
                let entries = std::fs::read_dir(expand(parent)?).ok()?;
                let mut options: Vec<String> = entries
                    .flatten()
                    .filter(|entry| entry.path().is_dir())
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    // hidden ones only once a `.` is typed
                    .filter(|name| {
                        name.starts_with(prefix)
                            && (prefix.starts_with('.') || !name.starts_with('.'))
                    })
                    .map(|name| format!("{parent}{join}{name}"))
                    .collect();
                options.sort();
                Some(options)
            })
            .unwrap_or_default();
        self.path = path;
        self.invalid = false;
        self.selected = None;
    }

    /// Moves the arrow selection through the shown options, down if `down`; wraps around
    fn select(&mut self, down: bool) {
        let shown = self.options.len().min(MAX_OPTIONS);
        if shown == 0 {
            return;
        }
        self.selected = Some(match (self.selected, down) {
            (None, true) => 0,
            (None, false) => shown - 1,
            (Some(index), true) => (index + 1) % shown,
            (Some(index), false) => (index + shown - 1) % shown,
        });
    }

    /// Fills in the selected option, else completes to the options' longest common prefix,
    /// or into the only option once it is reached
    fn complete(&mut self) {
        if let Some(option) = self.selected.and_then(|index| self.options.get(index)) {
            self.set(option.clone());
            return;
        }
        let Some((first, rest)) = self.options.split_first() else {
            return;
        };
        if rest.is_empty() && *first == self.path {
            self.set(format!("{first}/"));
            return;
        }
        let common = rest.iter().fold(first.as_str(), |common, option| {
            let len = common
                .char_indices()
                .zip(option.chars())
                .take_while(|((_, a), b)| a == b)
                .last()
                .map_or(0, |((index, a), _)| index + a.len_utf8());
            &common[..len]
        });
        if common.len() > self.path.len() {
            self.set(common.to_string());
        }
    }

    /// The directory to save (`Ok(None)` clears it), or `Err` if the path isn't an absolute directory
    fn parse(&self) -> Result<Option<PathBuf>, ()> {
        let path = self.path.trim();
        if path.is_empty() {
            return Ok(None);
        }
        expand(path)
            .and_then(|path| std::fs::canonicalize(path).ok())
            .filter(|path| path.is_dir())
            .map(Some)
            .ok_or(())
    }
}

/// Shows its contents' panes; the root's treeview shows its contents as its node's inner tree.
/// Its inner tree's root is a container (no main), so focus inside the project at its root (`[[project], []]`)
/// can add children to the project, apart from focus on the project's node (`[[project]]`).
///
/// Closing it (`CloseRequest`, eg Ctrl+W on its node) gives its contents back to its node, which they replace.
///
/// While focus is at its inner root, it shows an overview instead of its panes: its name (`r` renames it), its directory (`d` sets it),
/// its contents (click to select) and rows that add to it (`NEW_ROWS`: a command hub, project, terminal or browser).
pub struct ProjectApplet {
    holder: NodeHolder,
    hook: AppletContext,
    /// The node each embedded applet (its contents' mains) is the main of
    nodes: Rc<RefCell<HashMap<EmbeddedId, NodeId>>>,
    /// Its contents were given back, so its tree is empty
    dissolved: bool,
    name: String,
    /// The new name being typed (empty keeps the old one)
    renaming: Option<String>,
    /// The new directory being typed (empty clears it)
    setting_directory: Option<DirectoryInput>,
    /// Its treeview icon (`None`: `PROJECT_ICON`)
    icon: Option<String>,
    /// Picking an icon: the index in `ICONS` selected with the arrows
    picking_icon: Option<usize>,
    size: DisplayContainerSize,
}
impl ProjectApplet {
    /// Makes a project holding a subtree (`NodeRequest::Wrap`), under its inner root,
    /// in the directory of the tree it is made in
    pub fn wrapper() -> WrapInitializer {
        Box::new(|subtree, directory| {
            NodularAppletInitializer::holding_children(move |hook| {
                let mut project = Self::new(subtree, hook);
                project.holder.directory = directory;
                Box::new(project) as Box<dyn StandardApplet>
            })
        })
    }

    /// Makes a saved project again, holding its saved contents
    pub fn restorer(mut contents: SavedTree) -> NodularAppletInitializer {
        NodularAppletInitializer::holding_children(move |hook| {
            let name = contents.label.take().unwrap_or_else(|| LABEL.to_string());
            let directory = contents.directory.take();
            let icon = contents.icon.take();
            let mut project = Self::with_tree(hook, |spawn_client, attach_client| {
                NodeTree::restore(contents, spawn_client, attach_client)
            });
            project.name = name;
            project.icon = icon;
            project.holder.directory = directory;
            Box::new(project) as Box<dyn StandardApplet>
        })
    }

    /// Its spawner's name in the registry
    pub const SPAWNER: &str = "project";

    /// Registered as `SPAWNER` by the root, to restore saved projects (`save`); no args: an empty project
    pub fn get_applet_spawner() -> AppletSpawner {
        struct Spawner;
        impl AppletSpawnerTrait for Spawner {
            fn create_initializer(&self, args: &[&str]) -> Option<NodularAppletInitializer> {
                args.is_empty().then(|| ProjectApplet::empty(None))
            }

            fn restore_initializer(
                &self,
                _args: &[&str],
                inner: SavedTree,
            ) -> Option<NodularAppletInitializer> {
                Some(ProjectApplet::restorer(inner))
            }

            fn duplicate(&self) -> AppletSpawner {
                Box::new(Self)
            }
        }
        Box::new(Spawner)
    }

    /// Makes a project with nothing in it, in `directory`
    pub fn empty(directory: Option<PathBuf>) -> NodularAppletInitializer {
        NodularAppletInitializer::holding_children(move |hook| {
            let contents = DetachedTree {
                main: None,
                wl_surface: None,
                hibernated: None,
                awaiting: None,
                children: Vec::new(),
            };
            let mut project = Self::with_tree(hook, |spawn_client, attach_client| {
                NodeTree::from_detached(contents, spawn_client, attach_client)
            });
            project.holder.directory = directory;
            Box::new(project) as Box<dyn StandardApplet>
        })
    }

    fn new(subtree: DetachedTree, hook: AppletContext) -> Self {
        let contents = DetachedTree {
            main: None,
            wl_surface: None,
            hibernated: None,
            awaiting: None,
            children: vec![subtree],
        };
        Self::with_tree(hook, |spawn_client, attach_client| {
            NodeTree::from_detached(contents, spawn_client, attach_client)
        })
    }

    /// Holds the tree made with clients embedded in it
    fn with_tree(
        hook: AppletContext,
        make_tree: impl FnOnce(SpawnClient, AttachClient) -> NodeTree,
    ) -> Self {
        let nodes: Rc<RefCell<HashMap<EmbeddedId, NodeId>>> = Rc::default();
        let spawn_client: SpawnClient = {
            let hook = hook.clone();
            let nodes = Rc::clone(&nodes);
            Box::new(move |id, initializer| {
                let (embedded, handle) = hook.embed(initializer);
                nodes.borrow_mut().insert(embedded, id);
                handle
            })
        };
        let attach_client: AttachClient = {
            let hook = hook.clone();
            let nodes = Rc::clone(&nodes);
            Box::new(move |id, client: DetachedClient| {
                let (embedded, handle) = hook.embed_detached(client);
                nodes.borrow_mut().insert(embedded, id);
                handle
            })
        };
        let tree = make_tree(spawn_client, attach_client);
        let send = {
            let hook = hook.clone();
            Box::new(move |request: StandardRequest| hook.request(request.into()))
        };
        Self {
            holder: NodeHolder::new(tree, send, true, hook.applet_spawners.clone()),
            hook,
            nodes,
            dissolved: false,
            name: LABEL.to_string(),
            renaming: None,
            setting_directory: None,
            icon: None,
            picking_icon: None,
            // until it gets one
            size: DisplayContainerSize::new(800, 600),
        }
    }

    /// Where the overview's zoom-out view of its tree starts (#145), below the rows.
    /// None while setting the directory or icon, whose options take those rows.
    fn zoom_top(&self) -> Option<f32> {
        if self.setting_directory.is_some() || self.picking_icon.is_some() {
            return None;
        }
        let tree = &self.holder.tree;
        let rows = CONTENTS_ROW + tree.children(tree.root()).len() + NEW_ROWS.len();
        Some(PAD + rows as f32 * row_height() + PAD)
    }

    /// Starts typing a new directory, from the current one (the runner's if it has none)
    fn start_setting_directory(&mut self) {
        let directory = self
            .holder
            .directory
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .map(|directory| tilde(&directory))
            .unwrap_or_default();
        self.renaming = None;
        self.picking_icon = None;
        self.setting_directory = Some(DirectoryInput::new(directory));
    }

    /// Starts picking an icon, at the current one if it is listed
    fn start_picking_icon(&mut self) {
        let current = self.icon.as_deref().and_then(|icon| icon.chars().next());
        self.renaming = None;
        self.setting_directory = None;
        self.picking_icon = Some(
            ICONS
                .iter()
                .position(|&(icon, _)| Some(icon) == current)
                .unwrap_or(0),
        );
    }

    /// Sets the icon (`None`: the default) and stops picking
    fn set_icon(&mut self, icon: Option<String>) {
        self.icon = icon;
        self.picking_icon = None;
    }

    /// A key while picking an icon; whether it took it
    fn handle_icon_key(&mut self, key: KeySymbol) -> bool {
        let Some(selected) = self.picking_icon else {
            return false;
        };
        match key {
            KeySymbol::Enter => self.set_icon(Some(ICONS[selected].0.to_string())),
            KeySymbol::Escape => self.picking_icon = None,
            KeySymbol::Backspace => self.set_icon(None),
            KeySymbol::ArrowKeyDown => self.picking_icon = Some((selected + 1) % ICONS.len()),
            KeySymbol::ArrowKeyUp => {
                self.picking_icon = Some((selected + ICONS.len() - 1) % ICONS.len());
            }
            // any character, eg an emoji
            KeySymbol::Char(key_char) if !key_char.is_control() => {
                self.set_icon(Some(key_char.to_string()));
            }
            _ => return false,
        }
        true
    }

    /// Its icon as shown
    fn shown_icon(&self) -> String {
        self.icon
            .clone()
            .unwrap_or_else(|| PROJECT_ICON.to_string())
    }

    /// Focus is on its inner root (the project itself), so it shows the overview
    fn at_root(&self) -> bool {
        !self.dissolved && self.holder.tree.focused_node() == self.holder.tree.root()
    }

    /// Like `at_root`, for the screen's focus (#60)
    fn at_root_on(&self, screen: usize) -> bool {
        !self.dissolved && self.holder.tree.screen_focused_node(screen) == self.holder.tree.root()
    }

    /// What `NEW_ROWS[index]` adds: a command hub or project in it, focused inside,
    /// or a program, whose window lands in it (focus is at its root)
    fn add_new(&mut self, index: usize) {
        let root = self.holder.tree.root();
        let initializer = match NEW_ROWS[index].2 {
            New::Program(program) => {
                self.holder
                    .handle_node_request(root, NodeRequest::Run(program()));
                return;
            }
            New::CommandHub => CommandHubApplet::get_boxed_initiator(),
            New::Project => Self::empty(self.holder.directory.clone()),
        };
        if let Some(id) = self.holder.tree.add_child(root, initializer) {
            self.holder.tree.focus(id, true);
        }
    }

    /// A key while setting the directory; whether it took it
    fn handle_directory_key(&mut self, key: KeySymbol, modifiers: KeyModifiers) -> bool {
        let Some(input) = &mut self.setting_directory else {
            return false;
        };
        match key {
            KeySymbol::Enter => match input.parse() {
                Ok(directory) => {
                    self.holder.set_directory(directory);
                    self.setting_directory = None;
                    // republish, so the session saves it
                    self.hook.damage_treeview();
                }
                Err(()) => input.invalid = true,
            },
            KeySymbol::Escape => self.setting_directory = None,
            KeySymbol::Backspace => {
                let mut path = input.path.clone();
                path.pop();
                input.set(path);
            }
            KeySymbol::Char('\t') if modifiers == KeyModifiers::NONE => input.complete(),
            KeySymbol::ArrowKeyDown => input.select(true),
            KeySymbol::ArrowKeyUp => input.select(false),
            KeySymbol::Char(key_char) if !key_char.is_control() => {
                let mut path = input.path.clone();
                path.push(key_char);
                input.set(path);
            }
            _ => return false,
        }
        true
    }

    /// The overview's keys and clicks; gives back what it doesn't take
    fn handle_overview_event(&mut self, ui_event: UIEvent) -> Option<UIEvent> {
        match ui_event {
            UIEvent::Mouse(MouseEvent {
                position,
                kind:
                    MouseEventKind::Button {
                        button: BTN_LEFT,
                        pressed: true,
                    },
                ..
            }) => {
                self.handle_overview_click(position);
                return None;
            }
            // its tree's zoom-out view
            UIEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Scroll { delta, stop, .. },
                ..
            }) => self.holder.tree.zoom_scroll(delta, stop),
            // the panes aren't shown
            UIEvent::Mouse(_) => {}
            UIEvent::Key {
                symbol: Some(key),
                modifiers,
                pressed: true,
                ..
            } if !(modifiers.ctrl || modifiers.alt || modifiers.logo) => {
                if !self.handle_overview_key(key, modifiers) {
                    return Some(ui_event);
                }
                return None;
            }
            _ => return Some(ui_event),
        }
        self.hook.damage_window();
        self.hook.damage_treeview();
        None
    }

    /// Damages what it changed
    fn handle_overview_click(&mut self, [x, y]: [f64; 2]) {
        if let Some(top) = self.zoom_top()
            && y >= f64::from(top)
        {
            let size = DisplayContainerSize::new(
                self.size.width,
                self.size.height.saturating_sub(top.to_px()),
            );
            self.holder.tree.zoom_click(size, [x, y - f64::from(top)]);
            return;
        }
        let root = self.holder.tree.root();
        let contents = self.holder.tree.children(root).len();
        match overview_rows().row_at(y) {
            None => {}
            Some(row) if row >= CONTENTS_ROW && self.picking_icon.is_some() => {
                if let Some(&(icon, _)) = ICONS.get(row - CONTENTS_ROW) {
                    self.set_icon(Some(icon.to_string()));
                }
            }
            Some(row) if row >= CONTENTS_ROW && self.setting_directory.is_some() => {
                if let Some(input) = &mut self.setting_directory
                    && let Some(option) = input.options.get(row - CONTENTS_ROW).cloned()
                {
                    input.set(option);
                }
            }
            Some(0) => {
                self.setting_directory = None;
                self.picking_icon = None;
                self.renaming = Some(String::new());
            }
            Some(DIRECTORY_ROW) => self.start_setting_directory(),
            Some(ICON_ROW) => self.start_picking_icon(),
            Some(row) if (CONTENTS_ROW..CONTENTS_ROW + contents).contains(&row) => {
                let id = self.holder.tree.children(root)[row - CONTENTS_ROW];
                self.holder.tree.focus(id, false);
            }
            Some(row) if row >= CONTENTS_ROW + contents => {
                let index = row - CONTENTS_ROW - contents;
                if index < NEW_ROWS.len() {
                    self.add_new(index);
                }
            }
            Some(_) => {}
        }
        self.hook.damage_window();
        self.hook.damage_treeview();
    }

    /// `false` if it doesn't take it; damages what it changed
    fn handle_overview_key(&mut self, key: KeySymbol, modifiers: KeyModifiers) -> bool {
        if self.setting_directory.is_some() {
            if !self.handle_directory_key(key, modifiers) {
                return false;
            }
            self.hook.damage_window();
            return true;
        }
        if self.picking_icon.is_some() {
            if !self.handle_icon_key(key) {
                return false;
            }
            // the treeview shows it
            self.hook.damage_window();
            self.hook.damage_treeview();
            return true;
        }
        match (&mut self.renaming, key) {
            (None, KeySymbol::Char('d')) if modifiers == KeyModifiers::NONE => {
                self.start_setting_directory();
            }
            (Some(_), KeySymbol::Enter) => {
                let name = self.renaming.take().unwrap_or_default();
                if !name.trim().is_empty() {
                    self.name = name.trim().to_string();
                }
            }
            (Some(_), KeySymbol::Escape) => self.renaming = None,
            (Some(name), KeySymbol::Backspace) => {
                name.pop();
            }
            (Some(name), KeySymbol::Char(key_char)) if !key_char.is_control() => {
                name.push(key_char);
            }
            (None, KeySymbol::Char('r')) if modifiers == KeyModifiers::NONE => {
                self.renaming = Some(String::new());
            }
            (None, KeySymbol::Char('i')) if modifiers == KeyModifiers::NONE => {
                self.start_picking_icon();
            }
            (None, KeySymbol::Char(key_char))
                if modifiers == KeyModifiers::NONE
                    && let Some(index) =
                        NEW_ROWS.iter().position(|&(_, key, _)| key == key_char) =>
            {
                self.add_new(index);
            }
            _ => return false,
        }
        self.hook.damage_window();
        self.hook.damage_treeview();
        true
    }

    /// The rows under the hint: the icons or directories to pick from, else the contents and what can be added
    fn overview_options(&self) -> Vec<(String, Color)> {
        let tree = &self.holder.tree;
        let text = settings::get().palette.fg;
        match (&self.setting_directory, self.picking_icon) {
            (_, Some(selected)) => (0..)
                .zip(ICONS)
                .map(|(index, (icon, name))| {
                    if index == selected {
                        (format!("› {icon}  {name}"), settings::get().palette.accent)
                    } else {
                        (format!("  {icon}  {name}"), text)
                    }
                })
                .collect(),
            (Some(input), None) => input
                .options
                .iter()
                .take(MAX_OPTIONS)
                .enumerate()
                .map(|(index, option)| {
                    if input.selected == Some(index) {
                        (format!("› {option}"), settings::get().palette.accent)
                    } else {
                        (format!("  {option}"), text)
                    }
                })
                .collect(),
            (None, None) => (1..)
                .zip(tree.children(tree.root()))
                .map(|(index, &id)| (format!("{index}  {}", tree.label(id)), text))
                .chain(NEW_ROWS.map(|(label, _, _)| (label.to_string(), text)))
                .collect(),
        }
    }

    fn get_overview(&self, size: DisplayContainerSize) -> UITree {
        let width = size.width as f32;
        let text = settings::get().palette.fg;
        let title = match &self.renaming {
            Some(name) => format!("{}  {name}▏", self.shown_icon()),
            None => format!("{}  {}", self.shown_icon(), self.name),
        };
        let icon_row = (
            format!(
                "icon: {}{}",
                self.shown_icon(),
                if self.icon.is_none() {
                    " (default)"
                } else {
                    ""
                }
            ),
            HINT,
        );
        let (directory, hint) = match (&self.renaming, &self.setting_directory) {
            (_, Some(input)) => (
                (format!("directory: {}▏", input.path), text),
                if input.invalid {
                    ("Not a directory", ERROR)
                } else {
                    (
                        "Up/Down + Tab or click: complete, Enter: save (empty clears), Esc: cancel",
                        HINT,
                    )
                },
            ),
            (renaming, None) => (
                (
                    format!(
                        "directory: {}",
                        self.holder.directory.as_ref().map_or_else(
                            || "none (the runner's)".to_string(),
                            |directory| tilde(directory),
                        )
                    ),
                    HINT,
                ),
                if renaming.is_some() {
                    ("Enter: save, Esc: cancel", HINT)
                } else if self.picking_icon.is_some() {
                    (
                        "Up/Down + Enter or click: pick, type: that character, Backspace: default, Esc: cancel",
                        HINT,
                    )
                } else {
                    (
                        "r: rename, d: set directory, i: set icon, 1-9: select",
                        HINT,
                    )
                },
            ),
        };
        let below = self.overview_options();
        let rows = [
            (title, text),
            directory,
            icon_row,
            (hint.0.to_string(), hint.1),
        ]
        .into_iter()
        .chain(below);
        let layout = overview_rows();
        let rows = (0..)
            .zip(rows)
            .map(|(index, (text, color))| {
                let row = UITree::leaf_styled(
                    ui::text(text, color),
                    layout.style(index, PAD, (width - PAD).max(0.0)),
                );
                if index == HINT_ROW {
                    row
                } else {
                    row.with_pointer()
                }
            })
            .chain(self.zoom_view(size))
            .collect();
        UITree::new_styled(UIElement::Backgrounded(BG), Style::full_size(), rows)
    }

    /// Its tree's zoom-out view below the overview's rows; while it springs, from or to all of `size`
    fn zoom_view(&self, size: DisplayContainerSize) -> Option<UITree> {
        let tree = &self.holder.tree;
        let top = self.zoom_top()? * tree.zoom_progress().unwrap_or(1.0);
        let [width, height] = [size.width, size.height].map(|length| length as f32);
        Some(UITree::new_container_styled(
            Style::absolute([0.0, top], [width, (height - top).max(0.0)]),
            vec![tree.zoom_window(DisplayContainerSize::new(
                size.width,
                size.height.saturating_sub(top.to_px()),
            ))],
        ))
    }
}
impl StandardApplet for ProjectApplet {
    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        if self.dissolved {
            return;
        }
        if let StandardEvent::UIEvent(UIEvent::WindowResized(size)) = standard_event {
            self.size = size;
        }
        let standard_event = match standard_event {
            StandardEvent::UIEvent(ui_event) if self.at_root() => {
                let Some(ui_event) = self.handle_overview_event(ui_event) else {
                    return;
                };
                StandardEvent::UIEvent(ui_event)
            }
            standard_event => standard_event,
        };
        match standard_event {
            StandardEvent::UIEvent(_)
            | StandardEvent::FocusChanged(..)
            | StandardEvent::Highlighted(..) => {
                if let Some(request) = self.holder.handle_ui_event(standard_event) {
                    self.hook.request(request);
                }
            }
            // it has its own directory
            StandardEvent::Directory(_) => {}
            // its tree shows each screen its own focus, like the root's
            StandardEvent::ScreenViews { sizes, active } => {
                let tree = &mut self.holder.tree;
                tree.set_screen_count(sizes.len());
                tree.activate_screen(active);
                for (index, size) in sizes.into_iter().enumerate() {
                    if index != active {
                        tree.resize_screen(index, size);
                    }
                }
            }
            // handled by the applet's client
            StandardEvent::CloseRequest
            | StandardEvent::Deactivate
            | StandardEvent::SettingsChanged(_)
            | StandardEvent::DamageAck(_)
            | StandardEvent::WlSurfaceRegistered(_)
            | StandardEvent::Open(_)
            | StandardEvent::Pluck(_)
            | StandardEvent::Place(..)
            | StandardEvent::Focus(_)
            | StandardEvent::ToggleCollapsed(_)
            | StandardEvent::NodeAction(..) => {}
            // only the root gets these
            StandardEvent::Capturing(_) | StandardEvent::Screens(_) => {}
            StandardEvent::Swipe(view) => self.holder.tree.set_swipe(view),
        }
    }

    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree {
        if self.dissolved {
            return UITree::leaf(UIElement::Nothing);
        }
        let tree = &self.holder.tree;
        let active = match self.zoom_view(container_size) {
            // zooming into a node: the zoom-out view grows into its view
            Some(zoom) if !self.at_root() && tree.zoom_progress().is_some() => zoom,
            _ => tree.get_window_with(Some(&|| self.get_overview(container_size))),
        };
        if !self.screen_windows() {
            return active;
        }
        // the other screens show what they would if active, so switching doesn't change them
        let mut active = Some(active);
        UITree::new_container(
            (0..tree.screen_count())
                .map(|screen| {
                    if screen == tree.active_screen() {
                        active
                            .take()
                            .unwrap_or_else(|| UITree::leaf(UIElement::Nothing))
                    } else if self.at_root_on(screen) {
                        self.get_overview(
                            tree.screen_size(screen)
                                .unwrap_or(DisplayContainerSize::new(0, 0)),
                        )
                    } else {
                        tree.get_screen_window(screen)
                    }
                })
                .collect(),
        )
    }

    fn zoom_focus(&self) -> Option<WorldTreePath> {
        (!self.dissolved)
            .then(|| self.holder.tree.zoom_target())
            .flatten()
    }

    fn screen_windows(&self) -> bool {
        !self.dissolved && self.holder.tree.screen_count() > 1
    }

    /// Its panes are framed by its own tree; the overview isn't
    fn screen_frames_itself(&self) -> Vec<bool> {
        (0..self.holder.tree.screen_count())
            .map(|screen| !self.at_root_on(screen))
            .collect()
    }

    fn screen_focus(&self) -> Vec<WorldTreePath> {
        if !self.screen_windows() {
            return Vec::new();
        }
        (0..self.holder.tree.screen_count())
            .map(|screen| self.holder.tree.screen_focus_path(screen))
            .collect()
    }

    fn shown_at_once(&self) -> Vec<WorldTreePath> {
        if self.dissolved {
            return Vec::new();
        }
        self.holder.tree.shown_at_once()
    }

    /// Its tree, whose root (a container) is labeled with the project's name
    fn get_treeview(&self) -> WorldTree<String> {
        if self.dissolved {
            return WorldTree::Base(self.name.clone());
        }
        let mut treeview = self.holder.tree.get_treeview();
        root_label_mut(&mut treeview).clone_from(&self.name);
        treeview
    }

    fn get_focus_path(&self) -> WorldTreePath {
        if self.dissolved {
            return WorldTreePath::root();
        }
        self.holder.tree.get_focus_path()
    }

    /// Not at its root: the overview has the keys there (selecting the project is its outer node's)
    fn is_traversing(&self) -> bool {
        !self.dissolved && !self.at_root() && self.holder.tree.is_traversing()
    }

    /// Its panes are framed by its own tree; the overview isn't
    fn frames_itself(&self) -> bool {
        !self.dissolved && !self.at_root()
    }

    fn focus(&mut self, path: &WorldTreePath) {
        if !self.dissolved {
            self.holder.focus(path);
        }
    }

    fn toggle_collapsed(&mut self, path: &WorldTreePath) {
        if !self.dissolved {
            self.holder.toggle_collapsed(path);
        }
    }

    fn node_action(&mut self, path: &WorldTreePath, action: NodeAction) {
        if !self.dissolved {
            self.holder.node_action(path, action);
        }
    }

    /// Answered by its holder, maybe after asking an applet inside
    fn pluck(&mut self, path: &WorldTreePath) -> Answer<Option<DetachedTree>> {
        if self.dissolved {
            return Answer::Now(None);
        }
        self.holder.start_pluck(Some(path));
        Answer::Later
    }

    fn place(
        &mut self,
        path: &WorldTreePath,
        position: SpawnPosition,
        tree: DetachedTree,
    ) -> Answer<Result<(), DetachedTree>> {
        if self.dissolved {
            return Answer::Now(Err(tree));
        }
        self.holder.start_place(Some(path), position, tree);
        Answer::Later
    }

    /// It closes into its contents
    fn close_requested(&mut self) -> bool {
        self.holder.dissolve = true;
        false
    }

    fn deactivate(&mut self) {
        self.holder.tree.deactivate_all();
    }

    fn take_wl_surface(&mut self, surface: NewWlSurface) -> Result<(), Box<NewWlSurface>> {
        if self.dissolved {
            return Err(Box::new(surface));
        }
        self.holder.add_wl_surface(surface);
        Ok(())
    }

    fn take_open(&mut self, open: OpenApplet) -> Result<(), OpenApplet> {
        if self.dissolved {
            return Err(open);
        }
        self.holder.open(open);
        Ok(())
    }

    fn save(&self) -> Option<SavedApplet> {
        if self.dissolved {
            return None;
        }
        let mut contents = self.holder.tree.save();
        contents.label = Some(self.name.clone());
        contents.directory.clone_from(&self.holder.directory);
        contents.icon.clone_from(&self.icon);
        Some(session::saved_with_inner(Self::SPAWNER, Vec::new(), contents))
    }

    fn awaiting_wl(&self) -> Vec<AwaitedWl> {
        if self.dissolved {
            return Vec::new();
        }
        self.holder.tree.awaiting_wl()
    }

    /// From a main of its contents
    fn handle_embedded_request(&mut self, id: EmbeddedId, request: NodeRequest) {
        let Some(&id) = self.nodes.borrow().get(&id) else {
            return;
        };
        if let Some(request) = self.holder.handle_node_request(id, request) {
            self.hook.request(request);
        }
    }

    fn update(&mut self) {
        if self.dissolved {
            return;
        }
        if self.holder.dissolve {
            let contents = self.holder.tree.detach_all();
            self.hook.request(NodeRequest::Unwrap(contents));
            self.dissolved = true;
            return;
        }
        self.holder.finish();
        if std::mem::take(&mut self.holder.quit) {
            self.hook.quit();
        }
        self.holder.tree.update_view_root();
        if self.holder.tree.take_damage() {
            self.hook.damage_window();
            self.hook.damage_treeview();
        }
        self.holder.tree.update_layout();
        if self.holder.tree.zoom_progress().is_some() {
            self.hook.damage_window();
        }
        self.holder.update_wl_focus();
    }
}
