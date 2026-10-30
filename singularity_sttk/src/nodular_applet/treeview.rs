//! The root's treeview: an outline of the tree that can be clicked (focus) and dragged (pluck and place).
//!
//! Rows are placed at a fixed height, so the root can hit-test them itself.

use super::node_tree::INACTIVE_MARK;
use singularity_common::{
    sap::packets::{NewApplet, NodeAction, SpawnPosition},
    settings,
    utils::tree::{
        tree_node_path::{TraversableTree, TreeNodePath},
        world_tree::{WorldTree, WorldTreePath},
    },
};
use std::time::{Duration, Instant};

use sonamu_ui::{
    color::Color,
    ui_element::{Cursor, CursorIcon, Length, Style, UIElement, UITree, font_size},
    ui_event::{MouseEvent, MouseEventKind},
};

fn row_height() -> f32 {
    font_size() + 14.0
}
const TOP_PAD: f32 = 8.0;
const LEFT_PAD: f32 = 16.0;
const INDENT: f32 = 32.0;
const MARKER_WIDTH: f32 = 6.0;
/// The clickable width of a row's `▾`/`▸`, from the start of its label
fn triangle_width() -> f32 {
    font_size()
}
/// The width of a row's close button, at its right end
pub fn close_width() -> f32 {
    row_height()
}
/// How far the pointer moves while pressed before it's a drag
const DRAG_THRESHOLD: f64 = 12.0;
pub const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
/// Rows per scroll step
const SCROLL_STEP: usize = 3;

/// The alpha of everything in the treeview (about 80%), so what is behind it shows through
const OPACITY: u8 = 0xCC;

/// Also for the command hub's and project's backgrounds
pub const fn with_opacity(Color([r, g, b, _]): Color) -> Color {
    Color([r, g, b, OPACITY])
}

// Text and markers are from the settings' palette
const PANEL_BG: Color = with_opacity(Color([0x16, 0x16, 0x1C, 0]));
const HOVER_BG: Color = with_opacity(Color([0x26, 0x2A, 0x32, 0]));
const FOCUS_BG: Color = with_opacity(Color([0x1C, 0x3D, 0x2E, 0]));
const GUIDE: Color = with_opacity(Color([0x3B, 0x40, 0x49, 0]));
/// An expanded inner tree's box
const INNER_BOX_BG: Color = with_opacity(Color([0x1C, 0x1C, 0x26, 0]));
const INNER_BOX_BORDER: Color = GUIDE;
const DRAGGED_TEXT: Color = Color([0x55, 0x5A, 0x64, 0]);
const DROP_TARGET: Color = with_opacity(Color::CYAN);
/// The share of a row's height at its top and at its bottom that drops between siblings
const EDGE_ZONE: f32 = 0.25;
const INSERT_LINE_HEIGHT: f32 = 3.0;
const MENU_BG: Color = Color([0x20, 0x22, 0x2A, 0xF0]);
const MENU_PAD: f32 = 12.0;
/// Two clicks on a row within this are a double click (sets the view layer)
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// What the menu's `New` items list, and their labels
const NEW_APPLETS: [(NewApplet, &str); 5] = [
    (NewApplet::CommandHub, "Command hub"),
    (NewApplet::Terminal, "Terminal"),
    (NewApplet::Browser, "Browser"),
    (NewApplet::FileManager, "File manager"),
    (NewApplet::Project, "Project"),
];

pub struct TreeviewRow {
    pub path: WorldTreePath,
    depth: usize,
    label: String,
    /// Inside an applet's inner tree, rather than a node (or the view root's parent)
    inner: bool,
    /// Has a close button (shown while hovered): not the root, nor a `‹` row
    closable: bool,
    /// The holder's `‹` row of an inner tree: its name selects that tree's root (this path), its `‹` goes out to the holder
    inner_root: Option<WorldTreePath>,
}

/// One row per node of the tree, and per element of each applet's inner tree, in DFS order.
///
/// `treeview` is the view root's subtree and `base` its path, so rows get paths from the real root.
/// `up` is the view root's parent (path and label): a dim first row, to widen the view by focusing it.
pub fn treeview_rows(
    treeview: &WorldTree<String>,
    base: &TreeNodePath,
    up: Option<(TreeNodePath, String)>,
) -> Vec<TreeviewRow> {
    let mut rows = Vec::new();
    push_rows(treeview, &[], 0, false, 0, &mut rows);
    for row in &mut rows {
        row.path.0[0].0.splice(0..0, base.0.iter().copied());
    }
    if let Some((path, label)) = up {
        rows.insert(
            0,
            TreeviewRow {
                path: WorldTreePath(Box::new([path])),
                depth: 0,
                label: format!("‹ {label}"),
                inner: true,
                closable: false,
                inner_root: None,
            },
        );
    }
    rows
}

/// The rows of the inner tree of the node at `holder` (eg `[[2]]`),
/// under a first row for that node (focusing it goes back out).
/// `treeview` is the whole tree.
pub fn inner_layer_rows(
    treeview: &WorldTree<String>,
    holder: &WorldTreePath,
) -> Option<Vec<TreeviewRow>> {
    let world = treeview.safe_get(holder)?;
    if !matches!(world, WorldTree::World(_)) {
        return None;
    }
    let label = world.get_root_value().trim_start_matches("▾ ");
    let mut inner_root = holder.0.to_vec();
    inner_root.push(TreeNodePath::new_root());
    let mut rows = vec![TreeviewRow {
        label: format!("‹ {label}"),
        path: holder.clone(),
        depth: 0,
        inner: true,
        closable: false,
        inner_root: Some(WorldTreePath(inner_root.into_boxed_slice())),
    }];
    push_rows(world, &holder.0, 0, true, holder.0.len(), &mut rows);
    Some(rows)
}

/// `skip_root`: the world's root is already the row above (an applet's inner tree root is the applet itself).
/// `layer`: how many worlds deep the view is; rows deeper than it are `inner`.
fn push_rows(
    world: &WorldTree<String>,
    prefix: &[TreeNodePath],
    depth: usize,
    skip_root: bool,
    layer: usize,
    rows: &mut Vec<TreeviewRow>,
) {
    let WorldTree::World(root) = world else {
        return;
    };
    for path in root.iter_paths_dfs() {
        if skip_root && path.is_root() {
            continue;
        }
        let value = &root[&path];
        let row_depth = depth + path.depth();
        let closable = !(prefix.is_empty() && path.is_root());
        let mut full_path = prefix.to_vec();
        full_path.push(path);
        rows.push(TreeviewRow {
            path: WorldTreePath(full_path.clone().into_boxed_slice()),
            depth: row_depth,
            label: value.get_root_value().clone(),
            inner: prefix.len() > layer,
            closable,
            inner_root: None,
        });
        push_rows(value, &full_path, row_depth, true, layer, rows);
    }
}

/// The path of the row showing what `path` points at (an applet's inner root is the applet's row)
fn row_path(path: &WorldTreePath) -> WorldTreePath {
    let mut path = path.0.to_vec();
    while path.len() > 1 && path.last().is_some_and(TreeNodePath::is_root) {
        path.pop();
    }
    if path.is_empty() {
        path.push(TreeNodePath::new_root());
    }
    WorldTreePath(path.into_boxed_slice())
}

/// The row showing the focus: its own row, or if that is hidden (inside a collapsed node), the row it is hidden in
fn focused_row(rows: &[TreeviewRow], focus_path: &WorldTreePath) -> WorldTreePath {
    let own = row_path(focus_path);
    if rows.iter().any(|row| row.path == own) {
        return own;
    }
    rows.iter()
        .filter(|row| focus_path.0.starts_with(&row.path.0))
        .max_by_key(|row| row.path.0.len())
        .map_or(own, |row| row.path.clone())
}

/// Whether the focus path goes past its row, into the element's inner world (eg `[[2, 1], []]` is inside `[[2, 1]]`)
fn is_inside_element(focus_path: &WorldTreePath) -> bool {
    focus_path.0.len() > row_path(focus_path).0.len()
}

fn rect(color: Color, position: [f32; 2], size: [f32; 2]) -> UITree {
    UITree::leaf_styled(
        UIElement::Backgrounded(color),
        Style::absolute(position, size),
    )
}

/// Mouse state over the treeview
#[derive(Default)]
pub struct TreeviewPointer {
    hovered: Option<usize>,
    /// The pointer is on the hovered row's close button
    hovered_close: bool,
    /// Where in the hovered row the pointer is
    zone: DropZone,
    press: Option<Press>,
    /// The first row shown
    scroll: usize,
    /// The focused row last scrolled to, so it is scrolled to again only when focus moves
    followed: Option<WorldTreePath>,
    /// Opened by a right click
    menu: Option<Menu>,
    /// The last click on a row's name, to tell a double click
    last_click: Option<(WorldTreePath, Instant)>,
}

/// A right-click menu: actions on a row's node, or what can be added
struct Menu {
    /// Where its actions go
    target: WorldTreePath,
    /// Where it was opened; it is moved to fit (`rect`)
    position: [f32; 2],
    items: Vec<MenuItem>,
    hovered: Option<usize>,
}
impl Menu {
    /// Actions on the row's node, or if it is a `‹` row (or none), what can be added to `general`
    fn new(row: Option<&TreeviewRow>, general: &WorldTreePath, position: [f32; 2]) -> Self {
        let (target, items) = match row.filter(|row| !row.label.starts_with('‹')) {
            Some(row) => {
                let mut items = vec![MenuItem::New { sibling: false }];
                if row.closable {
                    items.push(MenuItem::New { sibling: true });
                }
                items.push(MenuItem::Action(NodeAction::MakeProject));
                items.push(MenuItem::ViewLayer);
                if row.closable {
                    items.extend(
                        [
                            NodeAction::Deactivate,
                            NodeAction::Close,
                            NodeAction::CloseWithChildren,
                        ]
                        .map(MenuItem::Action),
                    );
                }
                (row.path.clone(), items)
            }
            None => (general.clone(), new_items(false)),
        };
        Self {
            target,
            position,
            items,
            hovered: None,
        }
    }

    /// `[x, y, width, height]` in a treeview of this size, moved left and up to fit
    fn rect(&self, [width, height]: [f32; 2]) -> [f32; 4] {
        let menu_width = (font_size() * 12.0).min(width);
        let menu_height = self.items.len() as f32 * row_height();
        let [x, y] = self.position;
        [
            x.min(width - menu_width).max(0.0),
            y.min(height - menu_height).max(0.0),
            menu_width,
            menu_height,
        ]
    }

    fn item_at(&self, [x, y]: [f64; 2], size: [f32; 2]) -> Option<usize> {
        let [left, top, width, height] = self.rect(size).map(f64::from);
        // non-negative, since `y >= top`
        #[allow(clippy::cast_sign_loss)]
        ((left..left + width).contains(&x) && (top..top + height).contains(&y))
            .then(|| ((y - top) / f64::from(row_height())) as usize)
    }

    fn render(&self, size: [f32; 2]) -> UITree {
        let [x, y, width, height] = self.rect(size);
        let text = with_opacity(settings::get().palette.fg);
        let items = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let background = if self.hovered == Some(index) {
                    UIElement::Backgrounded(HOVER_BG)
                } else {
                    UIElement::Nothing
                };
                let text_top = (row_height() - font_size()) / 2.0;
                UITree::new_styled(
                    background,
                    Style::absolute([0.0, index as f32 * row_height()], [width, row_height()]),
                    vec![UITree::leaf_styled(
                        UIElement::Text(vec![(item.label(), UIElement::glyphon_attr(text))]),
                        Style::absolute(
                            [MENU_PAD, text_top],
                            [(width - MENU_PAD).max(0.0), row_height() - text_top],
                        ),
                    )],
                )
                .with_cursor(Some(Cursor::Named(CursorIcon::Pointer)))
            })
            .collect();
        UITree::new_styled(
            UIElement::Backgrounded(MENU_BG),
            Style::absolute([x, y], [width, height]),
            vec![UITree::new_styled(
                UIElement::border(GUIDE),
                Style {
                    border: Length::uniform_rect(Length::Px(1.0)),
                    ..Style::full_size()
                },
                items,
            )],
        )
    }
}

#[derive(Clone, Copy)]
enum MenuItem {
    /// Lists what can be added, as a child or as the next sibling
    New {
        sibling: bool,
    },
    Action(NodeAction),
    /// Shows the tree the row is in
    ViewLayer,
}
impl MenuItem {
    fn label(self) -> String {
        match self {
            Self::New { sibling: false } => "New child ›",
            Self::New { sibling: true } => "New sibling below ›",
            Self::Action(NodeAction::Close) => "Close",
            Self::Action(NodeAction::CloseWithChildren) => "Close with children",
            Self::Action(NodeAction::Deactivate) => "Deactivate",
            Self::Action(NodeAction::MakeProject) => "Make project",
            Self::ViewLayer => "Set as view layer",
            Self::Action(NodeAction::Spawn { applet, .. }) => NEW_APPLETS
                .iter()
                .find(|(new, _)| *new == applet)
                .map_or("?", |(_, label)| label),
        }
        .to_string()
    }
}

/// What can be added, as a child or as the next sibling
fn new_items(sibling: bool) -> Vec<MenuItem> {
    NEW_APPLETS
        .iter()
        .map(|&(applet, _)| MenuItem::Action(NodeAction::Spawn { applet, sibling }))
        .collect()
}

/// Where in a row the pointer is: the middle appends a child, the edges insert a sibling
#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum DropZone {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// Where a drop on `row` goes: the path of the parent, and the position among its children.
/// Roots (incl. inner roots) and `‹` rows have no siblings, so they are appended to.
/// An applet that can't insert there (eg one without a `NodeTree`) appends or gives it back.
fn drop_target(row: &TreeviewRow, zone: DropZone) -> (WorldTreePath, SpawnPosition) {
    let append = || (row.path.clone(), SpawnPosition::AppendChild);
    let Some((node, outer)) = row.path.0.split_last().filter(|_| row.closable) else {
        return append();
    };
    let Some((&index, parent)) = node.0.split_last() else {
        return append();
    };
    let child_index = match zone {
        DropZone::Middle => return append(),
        DropZone::Top => index,
        DropZone::Bottom => index + 1,
    };
    let mut path = outer.to_vec();
    path.push(TreeNodePath(parent.to_vec()));
    (
        WorldTreePath(path.into_boxed_slice()),
        SpawnPosition::InsertChild { child_index },
    )
}

struct Press {
    path: WorldTreePath,
    position: [f64; 2],
    dragging: bool,
    /// On the row's `▾`/`▸` (a click collapses or expands it instead of focusing it), or a `‹` row's `‹`
    on_triangle: bool,
    /// On the row's close button, so a click closes it
    on_close: bool,
}

pub enum TreeviewAction {
    Focus(WorldTreePath),
    /// Its `▾`/`▸` was clicked
    ToggleCollapsed(WorldTreePath),
    /// Pluck at this path
    StartDrag(WorldTreePath),
    /// Place at this path, or at the focus if dropped outside the treeview
    Drop(Option<(WorldTreePath, SpawnPosition)>),
    /// Do this to the node at this path (eg its close button was clicked)
    Node(WorldTreePath, NodeAction),
    /// Show the tree the node at this path is in
    SetViewLayer(WorldTreePath),
}

pub struct TreeviewResponse {
    pub action: Option<TreeviewAction>,
    /// The treeview used the event, so the applets shouldn't get it
    pub consumed: bool,
    /// The treeview looks different now
    pub damaged: bool,
}

impl TreeviewPointer {
    /// A press that started on a row hasn't been released yet, or its menu is open
    pub const fn is_pressed(&self) -> bool {
        self.press.is_some() || self.menu.is_some()
    }

    /// The menu's part of `handle_mouse`; `None` if the event is the rows'
    fn handle_menu_mouse(
        &mut self,
        event: &MouseEvent,
        in_treeview: bool,
        row: Option<&TreeviewRow>,
        general: &WorldTreePath,
        size: [f32; 2],
    ) -> Option<TreeviewResponse> {
        let response = |action, damaged| {
            Some(TreeviewResponse {
                action,
                consumed: in_treeview,
                damaged,
            })
        };
        if let MouseEventKind::Button {
            button: BTN_RIGHT,
            pressed: true,
        } = event.kind
            && in_treeview
            && self.press.is_none()
        {
            let position = event.position.map(|coord| coord as f32);
            self.menu = Some(Menu::new(row, general, position));
            return response(None, true);
        }
        let menu = self.menu.as_mut()?;
        let item = menu.item_at(event.position, size);
        match event.kind {
            MouseEventKind::Motion => {
                let damaged = std::mem::replace(&mut menu.hovered, item) != item;
                response(None, damaged)
            }
            MouseEventKind::Button {
                button: BTN_LEFT,
                pressed: true,
            } => {
                let action = match item.map(|index| menu.items[index]) {
                    Some(MenuItem::New { sibling }) => {
                        menu.items = new_items(sibling);
                        menu.hovered = None;
                        return response(None, true);
                    }
                    Some(MenuItem::Action(action)) => {
                        Some(TreeviewAction::Node(menu.target.clone(), action))
                    }
                    Some(MenuItem::ViewLayer) => {
                        Some(TreeviewAction::SetViewLayer(menu.target.clone()))
                    }
                    // a click elsewhere closes it
                    None => None,
                };
                self.menu = None;
                response(action, true)
            }
            MouseEventKind::Button {
                button: BTN_LEFT,
                pressed: false,
            } => response(None, false),
            MouseEventKind::Button { pressed: true, .. } => {
                self.menu = None;
                response(None, true)
            }
            MouseEventKind::Button { .. } | MouseEventKind::Scroll { .. } => None,
        }
    }

    /// `position` is relative to the treeview's top-left; `in_treeview` is whether it's over the treeview.
    /// `general` is where the menu of the empty space (or a `‹` row) adds things.
    pub fn handle_mouse(
        &mut self,
        event: &MouseEvent,
        in_treeview: bool,
        rows: &[TreeviewRow],
        general: &WorldTreePath,
        [width, height]: [f32; 2],
    ) -> TreeviewResponse {
        let [x, y] = event.position;
        // non-negative, since `y >= TOP_PAD`
        #[allow(clippy::cast_sign_loss)]
        let row = (in_treeview && y >= f64::from(TOP_PAD))
            .then(|| self.scroll + ((y - f64::from(TOP_PAD)) / f64::from(row_height())) as usize)
            .filter(|&index| index < rows.len());

        let within = ((y - f64::from(TOP_PAD)) / f64::from(row_height())).fract() as f32;
        let zone = if row.is_none() || (EDGE_ZONE..=1.0 - EDGE_ZONE).contains(&within) {
            DropZone::Middle
        } else if within < EDGE_ZONE {
            DropZone::Top
        } else {
            DropZone::Bottom
        };
        if let Some(response) = self.handle_menu_mouse(
            event,
            in_treeview,
            row.map(|index| &rows[index]),
            general,
            [width, height],
        ) {
            return response;
        }
        let on_close =
            row.is_some_and(|index| rows[index].closable && x >= f64::from(width - close_width()));
        let mut damaged = std::mem::replace(&mut self.hovered, row) != row;
        damaged |= std::mem::replace(&mut self.hovered_close, on_close) != on_close;
        damaged |= std::mem::replace(&mut self.zone, zone) != zone;
        let mut action = None;
        let consumed = in_treeview || self.press.is_some();

        match event.kind {
            MouseEventKind::Motion => {
                if let Some(press) = &mut self.press
                    && !press.dragging
                    && distance(press.position, event.position) > DRAG_THRESHOLD
                {
                    press.dragging = true;
                    action = Some(TreeviewAction::StartDrag(press.path.clone()));
                    damaged = true;
                }
            }
            MouseEventKind::Button {
                button: BTN_LEFT,
                pressed: true,
            } => {
                if let Some(index) = row {
                    let row = &rows[index];
                    let text_left = f64::from(LEFT_PAD + row.depth as f32 * INDENT);
                    self.press = Some(Press {
                        path: row.path.clone(),
                        position: event.position,
                        dragging: false,
                        on_triangle: (has_triangle(row) || row.inner_root.is_some())
                            && (text_left..text_left + f64::from(triangle_width()))
                                .contains(&event.position[0]),
                        on_close,
                    });
                }
            }
            MouseEventKind::Button {
                button: BTN_LEFT,
                pressed: false,
            } => {
                if let Some(press) = self.press.take() {
                    action = if press.dragging {
                        Some(TreeviewAction::Drop(
                            row.map(|index| drop_target(&rows[index], zone)),
                        ))
                    } else {
                        // a click: pressed and released on the same row
                        let row = row.map(|index| &rows[index]);
                        let last_click = self.last_click.take();
                        row.filter(|row| row.path == press.path).map(|row| {
                            let target = row.path.clone();
                            let double = last_click.is_some_and(|(path, time)| {
                                path == target && time.elapsed() < DOUBLE_CLICK
                            });
                            if !press.on_close && !press.on_triangle && !double {
                                self.last_click = Some((target.clone(), Instant::now()));
                            }
                            match &row.inner_root {
                                _ if press.on_close => {
                                    TreeviewAction::Node(target, NodeAction::Close)
                                }
                                // its `‹`: out to the holder
                                Some(_) if press.on_triangle => TreeviewAction::Focus(target),
                                None if double && !press.on_triangle => {
                                    TreeviewAction::SetViewLayer(target)
                                }
                                Some(inner_root) => TreeviewAction::Focus(inner_root.clone()),
                                None if press.on_triangle => {
                                    TreeviewAction::ToggleCollapsed(target)
                                }
                                None => TreeviewAction::Focus(target),
                            }
                        })
                    };
                    damaged = true;
                }
            }
            MouseEventKind::Scroll { delta, .. } if in_treeview => {
                let scroll = if delta[1] > 0.0 {
                    self.scroll + SCROLL_STEP
                } else {
                    self.scroll.saturating_sub(SCROLL_STEP)
                }
                .min(max_scroll(rows.len(), height));
                damaged |= std::mem::replace(&mut self.scroll, scroll) != scroll;
            }
            MouseEventKind::Button { .. } | MouseEventKind::Scroll { .. } => {}
        }

        TreeviewResponse {
            action,
            consumed,
            damaged,
        }
    }

    /// Scrolls to the focused row if focus moved there since the last render
    pub fn render(
        &mut self,
        rows: &[TreeviewRow],
        [width, height]: [f32; 2],
        focus_path: &WorldTreePath,
    ) -> UITree {
        let focused_row = focused_row(rows, focus_path);
        let focus_inner = is_inside_element(focus_path);

        if self.followed.as_ref() != Some(&focused_row) {
            if let Some(index) = rows.iter().position(|row| row.path == focused_row) {
                let shown = shown_rows(height);
                self.scroll = self.scroll.clamp((index + 1).saturating_sub(shown), index);
            }
            self.followed = Some(focused_row.clone());
        }
        self.scroll = self.scroll.min(max_scroll(rows.len(), height));
        let scroll = self.scroll;
        let row_top = |index: usize| TOP_PAD + (index as f32 - scroll as f32) * row_height();
        let settings = settings::get();

        let dragging = self.press.as_ref().filter(|press| press.dragging);

        let mut children = inner_box_trees(rows, width, row_top);

        children.extend(rows.iter().enumerate().skip(scroll).map(|(index, row)| {
            let is_focused = row.path == focused_row;
            let is_hovered = self.hovered == Some(index);
            let is_dragged = dragging.is_some_and(|press| press.path == row.path);

            let mut parts = Vec::new();
            for level in 0..row.depth {
                parts.push(rect(
                    GUIDE,
                    [LEFT_PAD + level as f32 * INDENT + INDENT / 2.0 - 1.0, 0.0],
                    [2.0, row_height()],
                ));
            }
            if is_focused {
                let marker = if focus_inner {
                    settings.border.focused
                } else {
                    settings.palette.accent
                };
                parts.push(rect(
                    with_opacity(marker),
                    [0.0, 0.0],
                    [MARKER_WIDTH, row_height()],
                ));
            }

            let text_left = LEFT_PAD + row.depth as f32 * INDENT;
            let text_top = (row_height() - font_size()) / 2.0;
            // like a browser tab's, only on the hovered row
            let shows_close = row.closable && is_hovered && dragging.is_none();
            let text_color = with_opacity(if is_dragged {
                DRAGGED_TEXT
            } else if is_inactive(row) {
                settings.palette.inactive
            } else {
                settings.palette.fg
            });
            let label = if row.label.is_empty() {
                "…".to_string()
            } else {
                row.label.clone()
            };
            parts.push(UITree::leaf_styled(
                UIElement::Text(vec![(label, UIElement::glyphon_attr(text_color))]),
                Style::absolute(
                    [text_left, text_top],
                    [
                        (width - text_left - if shows_close { close_width() } else { 0.0 })
                            .max(0.0),
                        row_height() - text_top,
                    ],
                ),
            ));
            if shows_close {
                let color = if self.hovered_close {
                    settings.palette.fg
                } else {
                    settings.palette.inactive
                };
                parts.push(
                    UITree::leaf_styled(
                        UIElement::Text(vec![(
                            "×".to_string(),
                            UIElement::glyphon_attr(with_opacity(color)),
                        )]),
                        Style::absolute(
                            [
                                width - close_width() + (close_width() - font_size()) / 2.0,
                                text_top,
                            ],
                            [close_width(), row_height() - text_top],
                        ),
                    )
                    .with_cursor(Some(Cursor::Named(CursorIcon::Pointer))),
                );
            }
            if has_triangle(row) || row.inner_root.is_some() {
                parts.push(
                    UITree::leaf_styled(
                        UIElement::Nothing,
                        Style::absolute([text_left, 0.0], [triangle_width(), row_height()]),
                    )
                    .with_cursor(Some(Cursor::Named(CursorIcon::Pointer))),
                );
            }

            let hovered_drop = dragging.is_some() && is_hovered && !is_dragged;
            if hovered_drop && drop_target(row, self.zone).1 != SpawnPosition::AppendChild {
                // a line between siblings
                let line_left = LEFT_PAD + row.depth as f32 * INDENT;
                let line_top = if self.zone == DropZone::Top {
                    0.0
                } else {
                    row_height() - INSERT_LINE_HEIGHT
                };
                parts.push(rect(
                    DROP_TARGET,
                    [line_left, line_top],
                    [(width - line_left - 4.0).max(0.0), INSERT_LINE_HEIGHT],
                ));
            } else if hovered_drop {
                parts.push(UITree::leaf_styled(
                    UIElement::border(DROP_TARGET),
                    Style {
                        border: Length::uniform_rect(Length::Px(2.0)),
                        ..Style::absolute([2.0, 0.0], [(width - 4.0).max(0.0), row_height()])
                    },
                ));
            }

            let background = if is_focused {
                UIElement::Backgrounded(FOCUS_BG)
            } else if is_hovered && dragging.is_none() {
                UIElement::Backgrounded(HOVER_BG)
            } else {
                UIElement::Nothing
            };
            UITree::new_styled(
                background,
                Style::absolute([0.0, row_top(index)], [width, row_height()]),
                parts,
            )
        }));

        if let Some(menu) = &self.menu {
            children.push(menu.render([width, height]));
        }
        UITree::new_styled(
            UIElement::Backgrounded(PANEL_BG),
            Style::full_size(),
            children,
        )
    }
}

/// A box behind each shown inner tree's rows. `row_top` is where a row is drawn.
fn inner_box_trees(
    rows: &[TreeviewRow],
    width: f32,
    row_top: impl Fn(usize) -> f32,
) -> Vec<UITree> {
    inner_boxes(rows)
        .flat_map(|(holder, end)| {
            let left = LEFT_PAD + rows[holder].depth as f32 * INDENT + INDENT / 2.0 - 1.0;
            let position = [left, row_top(holder + 1)];
            let size = [
                (width - left - 4.0).max(0.0),
                (end - holder) as f32 * row_height(),
            ];
            [
                rect(INNER_BOX_BG, position, size),
                UITree::leaf_styled(
                    UIElement::border(INNER_BOX_BORDER),
                    Style {
                        border: Length::uniform_rect(Length::Px(2.0)),
                        ..Style::absolute(position, size)
                    },
                ),
            ]
        })
        .collect()
}

/// Its node is inactive (see `INACTIVE_MARK`)
fn is_inactive(row: &TreeviewRow) -> bool {
    row.label
        .trim_start_matches(['▾', '▸', ' '])
        .starts_with(INACTIVE_MARK.trim_end())
}

/// Its label starts with a `▾`/`▸` that collapses or expands it when clicked
fn has_triangle(row: &TreeviewRow) -> bool {
    row.label.starts_with(['▾', '▸'])
}

/// How many rows fit
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn shown_rows(height: f32) -> usize {
    (((height - TOP_PAD) / row_height()).floor() as usize).max(1)
}

/// The furthest the rows can scroll: until the last one is at the bottom
fn max_scroll(row_count: usize, height: f32) -> usize {
    row_count.saturating_sub(shown_rows(height))
}

/// Each shown inner tree: the row holding it, and the last of the rows of it (which directly follow)
fn inner_boxes(rows: &[TreeviewRow]) -> impl Iterator<Item = (usize, usize)> {
    rows.iter().enumerate().filter_map(|(holder, row)| {
        let len = rows[holder + 1..]
            .iter()
            .take_while(|inner| {
                inner.inner
                    && inner.path.0.len() > row.path.0.len()
                    && inner.path.0.starts_with(&row.path.0)
            })
            .count();
        (len > 0).then_some((holder, holder + len))
    })
}

fn distance([x1, y1]: [f64; 2], [x2, y2]: [f64; 2]) -> f64 {
    (x2 - x1).hypot(y2 - y1)
}
