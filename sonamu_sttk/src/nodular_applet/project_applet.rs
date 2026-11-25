//! A project: an applet holding the project's contents as its inner tree, like the root holds the outer tree.
//! Its node doesn't know it is a project; it only sees an applet with an inner tree.

use crate::{
    nodular_applet::{
        Answer, AppletContext, AppletSpawner, AppletSpawnerTrait, EmbeddedId, NodeRequest,
        NodularAppletInitializer, StandardApplet, WrapInitializer,
        node_holder::NodeHolder,
        node_tree::{AttachClient, NodeId, NodeTree, PROJECT_ICON, SpawnClient, root_label_mut},
    },
    session,
    standard_keybinds::{browser, terminal},
    ui::{
        self, PAD, Rows,
        path_input::{PathInput, PathKey, tilde},
        row_height,
    },
};
use sonamu_common::{
    command::{self, CommandResult, NodeCall, NodeUid, Value, World},
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
    ui_element::{Style, UIElement, UITree, font_size},
    ui_event::{KeyModifiers, KeySymbol, MouseEvent, MouseEventKind, UIEvent},
};
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

/// Its name until it is renamed: its inner tree root's label
const LABEL: &str = "Project";

// The overview's rows (`overview_rows`), in a box: the name, its directory, its icon, its color, a hint, then
// `NEW_ROWS` on one line (its contents are in the zoom-out view below, #154)
const DIRECTORY_ROW: usize = 1;
const ICON_ROW: usize = 2;
const COLOR_ROW: usize = 3;
/// The only row a click does nothing on
const HINT_ROW: usize = 4;
/// Rows before `NEW_ROWS` (or the directory or icon options, while setting one)
const OPTIONS_ROW: usize = 5;
// The color row (#76), while picking: `color:`, a button for none (plain), then a hue slider to the row's end, in
// font sizes
const PLAIN_LEFT: f32 = 5.0;
const PLAIN_WIDTH: f32 = 4.0;
const HUE_LEFT: f32 = 10.0;
/// Rects the hue slider is drawn with
const HUE_STEPS: usize = 60;

/// Picking a project's color
struct ColorPick {
    /// The color before, which Esc gives back
    before: Option<Color>,
}

/// The hue slider's left and right ends in a window `width` wide
fn hue_bar(width: f32) -> (f32, f32) {
    let left = PAD + HUE_LEFT * font_size();
    (left, (width - PAD).max(left + 1.0))
}

/// A pastel color of the hue (0..1, around the wheel)
fn hue_color(hue: f32) -> Color {
    // HSL with saturation 0.8, lightness 0.75
    let (saturation, lightness) = (0.8, 0.75);
    let chroma = (1.0 - (2.0 * lightness - 1.0_f32).abs()) * saturation;
    let channel = |n: f32| {
        let k = (n + hue * 12.0) % 12.0;
        let value = lightness - chroma / 2.0 * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
        (value * 255.0).round().to_px() as u8
    };
    Color([channel(0.0), channel(8.0), channel(4.0), 0xFF])
}

/// The color's hue (0..1); 0 for grays
fn hue_of(Color([r, g, b, _]): Color) -> f32 {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    if max == min {
        return 0.0;
    }
    let delta = f32::from(max - min);
    let [rf, gf, bf] = [r, g, b].map(f32::from);
    let sixths = if max == r {
        (gf - bf) / delta
    } else if max == g {
        (bf - rf) / delta + 2.0
    } else {
        (rf - gf) / delta + 4.0
    };
    (sixths / 6.0).rem_euclid(1.0)
}
/// Of each of `NEW_ROWS`' cells on their line, in font sizes
const NEW_CELL: f32 = 10.0;
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
    /// An empty project, in this one's directory
    Project,
    Program(fn() -> String),
}
/// Cells on a line that add to it: label, key, and what it adds
const NEW_ROWS: [(&str, char, New); 3] = [
    ("+ project  (p)", 'p', New::Project),
    ("+ terminal  (t)", 't', New::Program(terminal)),
    ("+ browser  (b)", 'b', New::Program(browser)),
];
// Text is the settings' palette: `fg`, hints `inactive`
fn hint() -> Color {
    settings::get().palette.inactive
}

/// A `NEW_ROWS` cell's width, on `OPTIONS_ROW`
fn new_cell_width() -> f32 {
    NEW_CELL * font_size()
}

/// The `NEW_ROWS` cell at `x`
fn new_cell_at(x: f64) -> Option<usize> {
    let cell = f64::from(new_cell_width());
    let index = ((x - f64::from(PAD)) / cell).floor();
    #[expect(clippy::cast_sign_loss, reason = "checked non-negative")]
    (index >= 0.0 && index < NEW_ROWS.len() as f64).then_some(index as usize)
}

/// The overview's rows, each a line of text at its top (above the zoom-out view)
fn overview_rows() -> Rows {
    Rows::new(PAD)
}

/// Shows its contents' panes; the root's treeview shows its contents as its node's inner tree.
/// Its inner tree's root is a container (no main), so focus inside the project at its root (`[[project], []]`)
/// can add children to the project, apart from focus on the project's node (`[[project]]`).
///
/// Closing it (`CloseRequest`, eg Ctrl+W on its node) gives its contents back to its node, which they replace.
///
/// While focus is at its inner root, it shows an overview instead of its panes: its name (`r` renames it), its directory (`d` sets it),
/// and rows that add to it (`NEW_ROWS`: a project, terminal or browser).
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
    setting_directory: Option<PathInput>,
    /// Its treeview icon (`None`: `PROJECT_ICON`)
    icon: Option<String>,
    /// Picking an icon: the index in `ICONS` selected with the arrows
    picking_icon: Option<usize>,
    /// Its treeview color (`None`: the palette's)
    color: Option<Color>,
    /// The color row shows its plain button and hue slider
    picking_color: Option<ColorPick>,
    /// The color Space last made plain, which Space gives back
    plain_from: Option<Color>,
    /// The hue slider is being dragged
    dragging_hue: bool,
    /// The first icon or directory option shown
    options_scroll: usize,
    /// Mouse scrolling of the options not yet a row
    options_scrolled: ui::ScrollSteps,
    /// Where the pointer is over the overview's rows (above the zoom-out view), to highlight what it's on
    hovered: Option<[f64; 2]>,
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
            let color = contents.color.take();
            let mut project = Self::with_tree(hook, |spawn_client, attach_client, world| {
                NodeTree::restore(contents, spawn_client, attach_client, world)
            });
            project.name = name;
            project.icon = icon;
            project.color = color;
            project.holder.directory = directory;
            Box::new(project) as Box<dyn StandardApplet>
        })
    }

    /// Makes a project again around placed contents (`NodeTree::restore_closed`), with the look saved in `inner`
    pub fn rewrapper(contents: DetachedTree, mut inner: SavedTree) -> NodularAppletInitializer {
        NodularAppletInitializer::holding_children(move |hook| {
            let mut project = Self::with_tree(hook, |spawn_client, attach_client, world| {
                NodeTree::from_detached(contents, spawn_client, attach_client, world)
            });
            project.name = inner.label.take().unwrap_or_else(|| LABEL.to_string());
            project.icon = inner.icon.take();
            project.color = inner.color.take();
            project.holder.directory = inner.directory.take();
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
                uid: None,
                children: Vec::new(),
            };
            let mut project = Self::with_tree(hook, |spawn_client, attach_client, world| {
                NodeTree::from_detached(contents, spawn_client, attach_client, world)
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
            uid: None,
            children: vec![subtree],
        };
        Self::with_tree(hook, |spawn_client, attach_client, world| {
            NodeTree::from_detached(contents, spawn_client, attach_client, world)
        })
    }

    /// Holds the tree made with clients embedded in it
    fn with_tree(
        hook: AppletContext,
        make_tree: impl FnOnce(SpawnClient, AttachClient, World) -> NodeTree,
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
        let tree = make_tree(spawn_client, attach_client, hook.world());
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
            color: None,
            picking_color: None,
            plain_from: None,
            dragging_hue: false,
            hovered: None,
            options_scroll: 0,
            options_scrolled: ui::ScrollSteps::default(),
            // until it gets one
            size: DisplayContainerSize::new(800, 600),
        }
    }

    /// Where the overview's zoom-out view of its tree starts (#145), below the rows.
    /// None while setting the directory or icon, whose options take those rows.
    /// Of the zoom-out view below `zoom_top`
    fn zoom_size(&self, top: f32) -> DisplayContainerSize {
        DisplayContainerSize::new(
            self.size.width,
            self.size.height.saturating_sub(top.to_px()),
        )
    }

    fn zoom_top(&self) -> Option<f32> {
        if self.setting_directory.is_some() || self.picking_icon.is_some() {
            return None;
        }
        Some(PAD + (OPTIONS_ROW + 1) as f32 * row_height() + PAD)
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
        self.picking_color = None;
        self.setting_directory = Some(PathInput::directory(directory));
        self.options_scroll = 0;
    }

    /// Starts picking an icon, at the current one if it is listed
    fn start_picking_icon(&mut self) {
        let current = self.icon.as_deref().and_then(|icon| icon.chars().next());
        self.renaming = None;
        self.setting_directory = None;
        self.picking_color = None;
        self.picking_icon = Some(
            ICONS
                .iter()
                .position(|&(icon, _)| Some(icon) == current)
                .unwrap_or(0),
        );
        self.options_scroll = 0;
        self.follow_selection();
    }

    /// How many icons or directory options there are to pick from
    fn options_count(&self) -> usize {
        match (&self.setting_directory, self.picking_icon) {
            (_, Some(_)) => ICONS.len(),
            (Some(input), None) => input.options.len().min(MAX_OPTIONS),
            (None, None) => 0,
        }
    }

    /// How many options fit under the overview's other rows
    fn options_shown(&self) -> usize {
        overview_rows()
            .fitting(self.size.height as f32)
            .saturating_sub(OPTIONS_ROW)
            .max(1)
    }

    /// Scrolls the options the least that shows the selected one, and no further than the last at the bottom
    fn follow_selection(&mut self) {
        let shown = self.options_shown();
        let selected = match (&self.setting_directory, self.picking_icon) {
            (_, Some(selected)) => Some(selected),
            (Some(input), None) => input.selected,
            (None, None) => None,
        };
        if let Some(selected) = selected {
            self.options_scroll = ui::scroll_to(self.options_scroll, selected, shown);
        }
        self.options_scroll = self
            .options_scroll
            .min(ui::max_scroll(self.options_count(), shown));
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
            KeySymbol::ArrowKeyDown | KeySymbol::ArrowKeyUp => {
                let down = matches!(key, KeySymbol::ArrowKeyDown);
                self.picking_icon = ui::step(Some(selected), down, ICONS.len());
            }
            // any character, eg an emoji
            KeySymbol::Char(key_char) if !key_char.is_control() => {
                self.set_icon(Some(key_char.to_string()));
            }
            _ => return false,
        }
        true
    }

    /// A key while picking a color; whether it took it
    fn handle_color_key(&mut self, key: KeySymbol) -> bool {
        match key {
            KeySymbol::ArrowKeyLeft | KeySymbol::ArrowKeyRight => {
                let step = if matches!(key, KeySymbol::ArrowKeyLeft) {
                    -1
                } else {
                    1
                };
                // snapped to the slider's steps, so steps don't drift
                let index = self.color.map_or(0, |color| {
                    (hue_of(color) * HUE_STEPS as f32).round() as isize + step
                });
                let hue = index.rem_euclid(HUE_STEPS as isize) as f32 / HUE_STEPS as f32;
                self.color = Some(hue_color(hue));
            }
            KeySymbol::Char(' ') => {
                if let Some(color) = self.color.take() {
                    self.plain_from = Some(color);
                } else {
                    self.color = Some(self.plain_from.unwrap_or_else(|| hue_color(0.0)));
                }
            }
            KeySymbol::Enter => self.picking_color = None,
            KeySymbol::Escape => {
                if let Some(pick) = self.picking_color.take() {
                    self.color = pick.before;
                }
            }
            _ => return false,
        }
        true
    }

    /// Sets the color from the hue slider at `x`
    fn set_hue_at(&mut self, x: f64) {
        let (left, right) = hue_bar(self.size.width as f32);
        let hue = ((x as f32 - left) / (right - left)).clamp(0.0, 0.999);
        self.color = Some(hue_color(hue));
    }

    fn start_picking_color(&mut self) {
        self.renaming = None;
        self.setting_directory = None;
        self.picking_icon = None;
        self.picking_color = Some(ColorPick { before: self.color });
    }

    /// The color row's plain button and hue slider while picking (its `color:` is a row of text)
    fn color_controls(&self, width: f32) -> Vec<UITree> {
        let settings = settings::get();
        let layout = overview_rows();
        let top = layout.top_of(COLOR_ROW);
        let font = font_size();
        let plain_left = PAD + PLAIN_LEFT * font;
        if self.picking_color.is_none() {
            // the chosen color, after `color:`
            return self.color.map_or_else(Vec::new, |color| {
                vec![ui::rect(color, [plain_left, top], [font, font])]
            });
        }
        let plain = ui::frame(
            if self.color.is_none() {
                settings.palette.accent
            } else {
                settings.border.unfocused
            },
            settings.radius.min(font / 2.0),
            1.0,
            Style::absolute([plain_left, top], [PLAIN_WIDTH * font, font]),
            vec![UITree::leaf_styled(
                ui::text("plain", settings.palette.fg),
                Style::absolute([font / 2.0, 0.0], [PLAIN_WIDTH * font, font]),
            )],
        )
        .with_pointer();
        let (left, right) = hue_bar(width);
        let step = (right - left) / HUE_STEPS as f32;
        let mut parts: Vec<UITree> = (0..HUE_STEPS)
            .map(|index| {
                let hue = index as f32 / HUE_STEPS as f32;
                ui::rect(
                    hue_color(hue),
                    [left + index as f32 * step, top],
                    [step.ceil(), font],
                )
                .with_pointer()
            })
            .collect();
        parts.push(plain);
        if let Some(color) = self.color {
            let x = left + hue_of(color) * (right - left);
            parts.push(ui::rect(
                settings.palette.fg,
                [x - 2.0, top - 2.0],
                [4.0, font + 4.0],
            ));
        }
        parts
    }

    /// The hint row's text while not setting the directory
    fn hint_text(&self) -> &'static str {
        if self.renaming.is_some() {
            "Enter: save, Esc: cancel"
        } else if self.picking_color.is_some() {
            "Left/Right or click: hue, Space: plain or not, Enter: save, Esc: cancel"
        } else if self.picking_icon.is_some() {
            "Up/Down + Enter or click: pick, type: that character, Backspace: default, Esc: cancel"
        } else {
            "r: rename, d: set directory, i: set icon, c: set color, 1-9: select"
        }
    }

    /// The color row's text: while picking, its controls follow it
    fn color_row_text(&self) -> &'static str {
        if self.picking_color.is_some() || self.color.is_some() {
            "color:"
        } else {
            "color: plain (default)"
        }
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

    /// What `NEW_ROWS[index]` adds: a project in it, focused inside,
    /// or a program, whose window lands in it (focus is at its root)
    fn add_new(&mut self, index: usize) {
        let root = self.holder.tree.root();
        let initializer = match NEW_ROWS[index].2 {
            New::Program(program) => {
                self.holder
                    .handle_node_request(root, NodeRequest::Run(program()));
                return;
            }
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
        let shown = input.options.len().min(MAX_OPTIONS);
        match (key, input.handle_key(key, modifiers)) {
            (_, PathKey::Enter) => match input.parse_directory() {
                Ok(directory) => {
                    self.holder.set_directory(directory);
                    self.setting_directory = None;
                    // republish, so the session saves it
                    self.hook.damage_treeview();
                }
                Err(_) => input.invalid = true,
            },
            (_, PathKey::Cancel) => self.setting_directory = None,
            (_, PathKey::Changed) => {}
            (KeySymbol::ArrowKeyDown | KeySymbol::ArrowKeyUp, PathKey::Ignored) => {
                let down = matches!(key, KeySymbol::ArrowKeyDown);
                input.selected = ui::step(input.selected, down, shown).or(input.selected);
            }
            (_, PathKey::Ignored) => return false,
        }
        true
    }

    /// The overview's keys and clicks; gives back what it doesn't take
    fn handle_overview_event(&mut self, ui_event: UIEvent) -> Option<UIEvent> {
        // its tree's zoom-out view's right-click menu
        if let UIEvent::Mouse(mut mouse_event) = ui_event
            && let Some(top) = self.zoom_top()
        {
            mouse_event.position[1] -= f64::from(top);
            let size = self.zoom_size(top);
            if self.holder.tree.zoom_menu_mouse(size, &mouse_event) {
                self.holder.do_zoom_action();
                self.hook.damage_window();
                self.hook.damage_treeview();
                return None;
            }
        }
        match ui_event {
            UIEvent::Mouse(MouseEvent {
                position,
                kind:
                    MouseEventKind::Button {
                        button: ui::BTN_LEFT,
                        pressed,
                    },
                ..
            }) => {
                self.handle_overview_button(position, pressed);
                return None;
            }
            // its tree's zoom-out view
            UIEvent::Mouse(MouseEvent {
                kind:
                    MouseEventKind::Scroll {
                        delta,
                        v120,
                        stop,
                        ctrl,
                        ..
                    },
                ..
            }) => {
                if let Some(top) = self.zoom_top() {
                    let size = self.zoom_size(top);
                    self.holder.tree.zoom_scroll(size, delta, stop, ctrl);
                } else {
                    let steps = self.options_scrolled.add(delta, v120) as isize;
                    let max = ui::max_scroll(self.options_count(), self.options_shown());
                    self.options_scroll = self.options_scroll.saturating_add_signed(steps).min(max);
                }
            }
            // a drag in its tree's zoom-out view
            UIEvent::Mouse(MouseEvent {
                position: [x, y],
                kind: MouseEventKind::Motion,
                ..
            }) => {
                let top = self.zoom_top();
                self.hovered = top.is_none_or(|top| y < f64::from(top)).then_some([x, y]);
                if self.dragging_hue {
                    self.set_hue_at(x);
                } else if let Some(top) = top {
                    self.holder.tree.zoom_motion([x, y - f64::from(top)]);
                }
            }
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
                self.follow_selection();
                return None;
            }
            _ => return Some(ui_event),
        }
        self.hook.damage_window();
        self.hook.damage_treeview();
        None
    }

    /// Damages what it changed
    /// A left press or release; releases are only the zoom-out view's (eg the end of a drag there)
    fn handle_overview_button(&mut self, [x, y]: [f64; 2], pressed: bool) {
        if !pressed && self.dragging_hue {
            self.dragging_hue = false;
            self.picking_color = None;
            self.hook.damage_window();
            return;
        }
        if let Some(top) = self.zoom_top()
            && (y >= f64::from(top) || !pressed)
        {
            let size = self.zoom_size(top);
            self.holder
                .tree
                .zoom_button(size, [x, y - f64::from(top)], pressed);
            return;
        }
        if !pressed {
            return;
        }
        match overview_rows().row_at(y) {
            None => {}
            Some(row) if row >= OPTIONS_ROW && self.picking_icon.is_some() => {
                if let Some(&(icon, _)) = ICONS.get(row - OPTIONS_ROW + self.options_scroll) {
                    self.set_icon(Some(icon.to_string()));
                }
            }
            Some(row) if row >= OPTIONS_ROW && self.setting_directory.is_some() => {
                if let Some(input) = &mut self.setting_directory
                    && let Some(option) = (input.options.iter().take(MAX_OPTIONS))
                        .nth(row - OPTIONS_ROW + self.options_scroll)
                        .cloned()
                {
                    input.set(option.path);
                }
            }
            Some(0) => {
                self.setting_directory = None;
                self.picking_icon = None;
                self.picking_color = None;
                self.renaming = Some(String::new());
            }
            Some(DIRECTORY_ROW) => self.start_setting_directory(),
            Some(ICON_ROW) => self.start_picking_icon(),
            Some(COLOR_ROW) if self.picking_color.is_none() => self.start_picking_color(),
            Some(COLOR_ROW) => {
                let font = f64::from(font_size());
                let plain_left = f64::from(PAD) + f64::from(PLAIN_LEFT) * font;
                if (plain_left..plain_left + f64::from(PLAIN_WIDTH) * font).contains(&x) {
                    self.color = None;
                    self.picking_color = None;
                } else if x >= f64::from(hue_bar(self.size.width as f32).0) {
                    self.dragging_hue = true;
                    self.set_hue_at(x);
                }
            }
            Some(OPTIONS_ROW) => {
                if let Some(index) = new_cell_at(x) {
                    self.add_new(index);
                }
            }
            Some(_) => self.picking_color = None,
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
        if self.picking_color.is_some() {
            if !self.handle_color_key(key) {
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
            (None, KeySymbol::Char('c')) if modifiers == KeyModifiers::NONE => {
                self.start_picking_color();
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

    /// The rows under the hint: the shown icons or directories to pick from, else none (what can be added is on
    /// one line)
    fn overview_options(&self) -> Vec<(String, Color)> {
        let text = settings::get().palette.fg;
        let all: Vec<_> = match (&self.setting_directory, self.picking_icon) {
            (_, Some(_)) => ICONS
                .iter()
                .map(|(icon, name)| (format!("{icon}  {name}"), text))
                .collect(),
            (Some(input), None) => input
                .options
                .iter()
                .take(MAX_OPTIONS)
                .map(|option| (option.path.clone(), text))
                .collect(),
            (None, None) => Vec::new(),
        };
        all.into_iter()
            .skip(self.options_scroll)
            .take(self.options_shown())
            .collect()
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
            hint(),
        );
        let (directory, hint) = match (&self.renaming, &self.setting_directory) {
            (_, Some(input)) => (
                (format!("directory: {}▏", input.path), text),
                if input.invalid {
                    ("Not a directory", settings::ERROR)
                } else {
                    (
                        "Up/Down + Tab or click: complete, Enter: save (empty clears), Esc: cancel",
                        hint(),
                    )
                },
            ),
            (_, None) => (
                (
                    format!(
                        "directory: {}",
                        self.holder.directory.as_ref().map_or_else(
                            || "none (the runner's)".to_string(),
                            |directory| tilde(directory),
                        )
                    ),
                    hint(),
                ),
                (self.hint_text(), hint()),
            ),
        };
        let below = self.overview_options();
        let count = OPTIONS_ROW + below.len().max(1);
        let options = !below.is_empty();
        let rows = [
            (title, self.color.unwrap_or(text)),
            directory,
            icon_row,
            (
                self.color_row_text().to_string(),
                self.color.unwrap_or(text),
            ),
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
            .collect::<Vec<_>>();
        let header = Self::overview_header(width, count, options);
        let rows = header
            .into_iter()
            .chain(self.overview_selected(width))
            .chain(self.options_scrollbar(width))
            .chain(self.overview_hover(width, count, options))
            .chain(rows)
            .chain(self.color_controls(width))
            .chain(self.zoom_view(size))
            .collect();
        UITree::new_styled(
            UIElement::Backgrounded(ui::bg(&settings::get().palette)),
            Style::full_size(),
            rows,
        )
    }

    /// The row of the selected icon or directory option
    fn selected_option_row(&self) -> Option<usize> {
        let selected = match (&self.setting_directory, self.picking_icon) {
            (_, Some(selected)) => selected,
            (Some(input), None) => input.selected?,
            (None, None) => return None,
        };
        let shown = selected.checked_sub(self.options_scroll)?;
        (shown < self.options_shown()).then_some(OPTIONS_ROW + shown)
    }

    /// Beside the icons or directory options, inside the overview's box
    fn options_scrollbar(&self, width: f32) -> Option<UITree> {
        let shown = self.options_shown();
        ui::scrollbar(
            &settings::get().palette,
            ui::scrollbar_x(width - PAD / 2.0),
            [
                overview_rows().top_of(OPTIONS_ROW),
                shown as f32 * row_height(),
            ],
            self.options_scroll,
            shown,
            self.options_count(),
        )
    }

    /// Behind the selected icon or directory option, like selected rows elsewhere
    fn overview_selected(&self, width: f32) -> Option<UITree> {
        let layout = overview_rows();
        Some(ui::rect(
            ui::selected_bg(&settings::get().palette),
            [PAD / 2.0, layout.top_of(self.selected_option_row()?)],
            [(width - PAD).max(0.0), layout.height],
        ))
    }

    /// Behind the clickable row (or `NEW_ROWS` cell) of the overview's `count` the pointer is on, like hovered rows
    /// elsewhere; not the color row while picking (its parts aren't the whole row) or the selected one
    fn overview_hover(&self, width: f32, count: usize, options: bool) -> Option<UITree> {
        let [x, y] = self.hovered?;
        let layout = overview_rows();
        let row = layout.row_at(y).filter(|&row| row < count)?;
        if row == HINT_ROW
            || (row == COLOR_ROW && self.picking_color.is_some())
            || self.selected_option_row() == Some(row)
        {
            return None;
        }
        let [left, w] = if row == OPTIONS_ROW && !options {
            let cell = new_cell_width();
            [PAD + new_cell_at(x)? as f32 * cell, cell]
        } else {
            [PAD / 2.0, (width - PAD).max(0.0)]
        };
        Some(ui::rect(
            ui::raised_bg(&settings::get().palette),
            [left, layout.top_of(row)],
            [w, layout.height],
        ))
    }

    /// The box around the overview's `count` rows, apart from the zoom-out view, and unless `options` show,
    /// `NEW_ROWS` on one line
    fn overview_header(width: f32, count: usize, options: bool) -> Vec<UITree> {
        let settings = settings::get();
        let layout = overview_rows();
        // darker inside, cut to its rounded corners
        let header = ui::frame(
            settings.border.unfocused,
            settings.radius,
            settings.gap_and_border_px().1 as f32,
            Style::absolute(
                [PAD / 2.0; 2],
                [(width - PAD).max(0.0), count as f32 * row_height() + PAD],
            ),
            vec![UITree::leaf_styled(
                UIElement::Backgrounded(ui::with_opacity(settings.palette.sunken(30))),
                Style::full_size(),
            )],
        );
        let cell = new_cell_width();
        let new_cells = (0..)
            .zip(NEW_ROWS)
            .filter(|_| !options)
            .map(|(index, (label, _, _))| {
                UITree::leaf_styled(
                    ui::text(label, settings.palette.fg),
                    layout.style(OPTIONS_ROW, PAD + index as f32 * cell, cell),
                )
                .with_pointer()
            });
        std::iter::once(header).chain(new_cells).collect()
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
            self.follow_selection();
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
            // only the root and the status bar get these
            | StandardEvent::PinSideView(_)
            | StandardEvent::ActivateScreen(_)
            | StandardEvent::BarState(_)
            | StandardEvent::Deactivate
            | StandardEvent::SettingsChanged(_)
            | StandardEvent::DamageAck(_)
            | StandardEvent::WlSurfaceRegistered(_)
            | StandardEvent::Open(_)
            | StandardEvent::Pluck(_)
            | StandardEvent::Place(..)
            | StandardEvent::Focus(_)
            | StandardEvent::ToggleCollapsed(_)
            | StandardEvent::NodeAction(..)
            | StandardEvent::Command { .. }
            | StandardEvent::NodeUid(_) => {}
            // only the root gets these
            StandardEvent::Capturing(_)
            | StandardEvent::Screens(_)
            | StandardEvent::Brightness { .. } => {}
            StandardEvent::Swipe(view) => self.holder.tree.set_swipe(view),
            StandardEvent::ScreenMouse(screen, event) => {
                self.holder.tree.screen_mouse(screen, event);
            }
            StandardEvent::FocusChain { screen, chain } => {
                self.holder.tree.focus_chain(screen, &chain);
            }
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

    fn screen_focus_uids(&self) -> Vec<Vec<NodeUid>> {
        (0..self.holder.tree.screen_count())
            .map(|screen| self.holder.tree.screen_focus_uids(screen))
            .collect()
    }

    fn shown_at_once(&self) -> Vec<WorldTreePath> {
        if self.dissolved {
            return Vec::new();
        }
        self.holder.tree.shown_at_once()
    }

    fn treeview_colors(&self) -> Vec<(WorldTreePath, Color)> {
        if self.dissolved {
            return Vec::new();
        }
        self.holder.tree.treeview_colors()
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
        contents.color = self.color;
        Some(session::saved_with_inner(
            Self::SPAWNER,
            Vec::new(),
            contents,
        ))
    }

    fn awaiting_wl(&self) -> Vec<AwaitedWl> {
        if self.dissolved {
            return Vec::new();
        }
        self.holder.tree.awaiting_wl()
    }

    /// From a main of its contents
    fn handle_node_call(&mut self, call: NodeCall) {
        // its inner root and its node are one row, so the root's parent is its node's: asked of its own world, on
        // a thread, since that world may be busy
        let tree = &self.holder.tree;
        if call.name == "parent"
            && call.uid == Some(tree.uid(tree.root()))
            && let Some(own) = self.hook.uid()
        {
            std::thread::spawn(move || {
                let _ = call
                    .reply
                    .send(command::call(Some(own), "parent", Vec::new(), &|| false));
            });
            return;
        }
        if let Some(request) = self.holder.handle_node_call(call) {
            self.hook.request(request);
        }
    }

    fn command(&mut self, name: &str, _args: Vec<Value>) -> CommandResult {
        match name {
            // its tree's focus, for `command::focus`
            command::INNER_FOCUS => {
                let tree = &self.holder.tree;
                let id = tree.focused_node();
                Ok(Value::List(vec![
                    Value::Node(tree.uid(id)),
                    Value::Bool(tree.is_inner(id)),
                ]))
            }
            // for `n:inner()`
            command::INNER_ROOT => Ok(Value::Node(self.holder.tree.uid(self.holder.tree.root()))),
            _ => Err(command::unknown(name)),
        }
    }

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
        if self.holder.tree.zoom_animating() {
            self.hook.damage_window();
        }
        self.holder.update_wl_focus();
    }
}

#[cfg(test)]
mod tests {
    use super::{hue_color, hue_of};

    /// The slider's marker sits where it was clicked
    #[test]
    fn hue_round_trips() {
        for step in 0..100 {
            let hue = step as f32 / 100.0;
            let back = hue_of(hue_color(hue));
            let off = (back - hue).abs().min(1.0 - (back - hue).abs());
            assert!(off < 0.01, "{hue} came back as {back}");
        }
    }
}
