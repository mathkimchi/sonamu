//! The root's treeview: an outline of the tree that can be clicked (focus) and dragged (pluck and place).
//!
//! Rows are placed at a fixed height, so the root can hit-test them itself.

use super::node_tree::INACTIVE_MARK;
use crate::ui::{self, Rows, rect, row_height};
use sonamu_common::{
    sap::packets::{NewApplet, NodeAction, SpawnPosition},
    settings::{self, Settings},
    utils::tree::{
        tree_node_path::TraversableTree,
        world_tree::{Step, WorldTree, WorldTreePath},
    },
};
use std::time::{Duration, Instant};

use sonamu_ui::{
    color::Color,
    ui_element::{Length, Style, UIElement, UITree, font_size},
    ui_event::{MouseEvent, MouseEventKind},
};

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
pub const BTN_RIGHT: u32 = 0x111;
/// Rows per scroll step
const SCROLL_STEP: usize = 3;

/// The alpha of everything in the treeview (about 80%), so what is behind it shows through
const OPACITY: u8 = 0xCC;

/// Also for the command hub's and project's backgrounds
pub const fn with_opacity(Color([r, g, b, _]): Color) -> Color {
    Color([r, g, b, OPACITY])
}

// Text and markers are the settings' palette; backgrounds are shades of its `bg`
fn panel_bg() -> Color {
    with_opacity(settings::get().palette.raised(5))
}
fn hover_bg() -> Color {
    with_opacity(settings::get().palette.raised(15))
}
fn focus_bg() -> Color {
    with_opacity(settings::get().palette.tinted(25))
}
pub(super) fn guide() -> Color {
    with_opacity(settings::get().palette.raised(25))
}
/// The share of a row's height at its top and at its bottom that drops between siblings
const EDGE_ZONE: f32 = 0.25;
const INSERT_LINE_HEIGHT: f32 = 3.0;
/// Nearly opaque, over the treeview
fn menu_bg() -> Color {
    let Color([r, g, b, _]) = settings::get().palette.raised(10);
    Color([r, g, b, 0xF0])
}
const MENU_PAD: f32 = 12.0;
/// Two clicks on a row within this are a double click (sets the view root)
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// What the menu's `New` items list, and their labels
const NEW_APPLETS: [(NewApplet, &str); 4] = [
    (NewApplet::Terminal, "Terminal"),
    (NewApplet::Browser, "Browser"),
    (NewApplet::FileManager, "File manager"),
    (NewApplet::Project, "Project"),
];

pub struct TreeviewRow {
    pub path: WorldTreePath,
    pub(crate) depth: usize,
    label: String,
    /// Has a close button (shown while hovered): not the root, nor a `‹` row
    closable: bool,
}

/// One row per node of the tree, and per element of each applet's inner tree, in DFS order.
///
/// `treeview` is the view root's subtree and `base` its path, so rows get paths from the real root.
/// `up` is the view root's parent (path and label): a dim first row, to widen the view by focusing it.
pub fn treeview_rows(
    treeview: &WorldTree<String>,
    base: &WorldTreePath,
    up: Option<(WorldTreePath, String)>,
) -> Vec<TreeviewRow> {
    let mut rows = Vec::new();
    push_rows(treeview, &WorldTreePath::root(), 0, false, &mut rows);
    for row in &mut rows {
        row.path.0.splice(0..0, base.0.iter().copied());
    }
    if let Some((path, label)) = up {
        rows.insert(
            0,
            TreeviewRow {
                path,
                depth: 0,
                label: format!("‹ {label}"),
                closable: false,
            },
        );
    }
    rows
}

/// `skip_root`: the world's root is already the row above (`prefix`; an applet's inner tree root is the applet itself).
fn push_rows(
    world: &WorldTree<String>,
    prefix: &WorldTreePath,
    depth: usize,
    skip_root: bool,
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
        let closable = !(prefix.0.is_empty() && path.is_root());
        let mut full_path = prefix.clone();
        if skip_root {
            full_path.push_inner(&WorldTreePath::from_tree_path(&path));
        } else {
            full_path.0.extend(WorldTreePath::from_tree_path(&path).0);
        }
        rows.push(TreeviewRow {
            path: full_path.clone(),
            depth: row_depth,
            label: value.get_root_value().clone(),
            closable,
        });
        push_rows(value, &full_path, row_depth, true, rows);
    }
}

/// The row showing the focus: its own row, or if that is hidden (inside a collapsed node), the row it is hidden in
pub(crate) fn focused_row(rows: &[TreeviewRow], focus_path: &WorldTreePath) -> WorldTreePath {
    if rows.iter().any(|row| &row.path == focus_path) {
        return focus_path.clone();
    }
    rows.iter()
        .filter(|row| focus_path.starts_with(&row.path))
        .max_by_key(|row| row.path.0.len())
        .map_or_else(|| focus_path.clone(), |row| row.path.clone())
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
    menu: Option<Menu<WorldTreePath>>,
    /// The last click on a row's name, to tell a double click
    last_click: Option<(WorldTreePath, Instant)>,
}

/// A right-click menu: actions on a node (a row's path, or a zoom-out view pane's node), or what can be added
pub(super) struct Menu<T> {
    /// Where its actions go
    pub target: T,
    /// Where it was opened; it is moved to fit (`rect`)
    position: [f32; 2],
    items: Vec<MenuItem>,
    hovered: Option<usize>,
}

/// What a mouse event did to an open menu (`Menu::handle_mouse`)
pub(super) enum MenuEvent {
    Stay {
        damaged: bool,
    },
    /// It closes, with the item chosen if any
    Close(Option<MenuItem>),
}

/// The items of a node's menu; `view_root` adds "Set as view root"
pub(super) fn node_items(closable: bool, view_root: bool) -> Vec<MenuItem> {
    let mut items = vec![MenuItem::New { sibling: false }];
    if closable {
        items.push(MenuItem::New { sibling: true });
    }
    items.push(MenuItem::Action(NodeAction::MakeProject));
    if view_root {
        items.push(MenuItem::ViewRoot);
    }
    if closable {
        items.extend(
            [
                NodeAction::Deactivate,
                NodeAction::Close,
                NodeAction::CloseWithChildren,
            ]
            .map(MenuItem::Action),
        );
    }
    items
}

impl<T> Menu<T> {
    pub(super) const fn new(target: T, items: Vec<MenuItem>, position: [f32; 2]) -> Self {
        Self {
            target,
            position,
            items,
            hovered: None,
        }
    }

    /// `event` is relative to where it is drawn, at `size`; `None` if the event isn't the menu's
    pub(super) fn handle_mouse(&mut self, event: &MouseEvent, size: [f32; 2]) -> Option<MenuEvent> {
        let item = self.item_at(event.position, size);
        Some(match event.kind {
            MouseEventKind::Motion => MenuEvent::Stay {
                damaged: std::mem::replace(&mut self.hovered, item) != item,
            },
            MouseEventKind::Button {
                button: BTN_LEFT,
                pressed: true,
            } => match item.map(|index| self.items[index]) {
                Some(MenuItem::New { sibling }) => {
                    self.items = new_items(sibling);
                    self.hovered = None;
                    MenuEvent::Stay { damaged: true }
                }
                // a click elsewhere closes it
                item => MenuEvent::Close(item),
            },
            MouseEventKind::Button {
                button: BTN_LEFT,
                pressed: false,
            } => MenuEvent::Stay { damaged: false },
            MouseEventKind::Button { pressed: true, .. } => MenuEvent::Close(None),
            MouseEventKind::Button { .. } | MouseEventKind::Scroll { .. } => return None,
        })
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
        ((left..left + width).contains(&x) && (top..top + height).contains(&y))
            .then(|| Rows::new(0.0).row_at(y - top))
            .flatten()
    }

    pub(super) fn render(&self, size: [f32; 2]) -> UITree {
        let [x, y, width, height] = self.rect(size);
        let text = with_opacity(settings::get().palette.fg);
        let rows = Rows::new(0.0);
        let items = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let background = (self.hovered == Some(index)).then_some(hover_bg());
                let label = rows.text(ui::text(item.label(), text), MENU_PAD, width);
                rows.row(index, width, background, vec![label])
                    .with_pointer()
            })
            .collect();
        UITree::new_styled(
            UIElement::Backgrounded(menu_bg()),
            Style::absolute([x, y], [width, height]),
            vec![ui::frame(guide(), 0.0, 1.0, Style::full_size(), items)],
        )
    }
}

#[derive(Clone, Copy)]
pub(super) enum MenuItem {
    /// Lists what can be added, as a child or as the next sibling
    New {
        sibling: bool,
    },
    Action(NodeAction),
    /// Shows the tree the row is in
    ViewRoot,
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
            Self::ViewRoot => "Set as view root",
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
    let Some((&last, parent)) = row.path.0.split_last().filter(|_| row.closable) else {
        return append();
    };
    let (Step::Child(index) | Step::Inner(index)) = last;
    let child_index = match zone {
        DropZone::Middle => return append(),
        DropZone::Top => index,
        DropZone::Bottom => index + 1,
    };
    let position = match last {
        Step::Child(_) => SpawnPosition::InsertChild { child_index },
        Step::Inner(_) => SpawnPosition::InsertInner { child_index },
    };
    (WorldTreePath(parent.to_vec()), position)
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
    SetViewRoot(WorldTreePath),
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
            let (target, items) = match row.filter(|row| !row.label.starts_with('‹')) {
                Some(row) => (row.path.clone(), node_items(row.closable, true)),
                None => (general.clone(), new_items(false)),
            };
            self.menu = Some(Menu::new(target, items, position));
            return response(None, true);
        }
        let menu = self.menu.as_mut()?;
        let action = match menu.handle_mouse(event, size)? {
            MenuEvent::Stay { damaged } => return response(None, damaged),
            MenuEvent::Close(Some(MenuItem::Action(action))) => {
                Some(TreeviewAction::Node(menu.target.clone(), action))
            }
            MenuEvent::Close(Some(MenuItem::ViewRoot)) => {
                Some(TreeviewAction::SetViewRoot(menu.target.clone()))
            }
            MenuEvent::Close(_) => None,
        };
        self.menu = None;
        response(action, true)
    }

    /// A click: pressed and released on the same row
    fn click_action(&mut self, press: &Press, row: Option<&TreeviewRow>) -> Option<TreeviewAction> {
        let last_click = self.last_click.take();
        row.filter(|row| row.path == press.path).map(|row| {
            let target = row.path.clone();
            let double = last_click
                .is_some_and(|(path, time)| path == target && time.elapsed() < DOUBLE_CLICK);
            if !press.on_close && !press.on_triangle && !double {
                self.last_click = Some((target.clone(), Instant::now()));
            }
            if press.on_close {
                TreeviewAction::Node(target, NodeAction::Close)
            } else if press.on_triangle {
                TreeviewAction::ToggleCollapsed(target)
            } else if double {
                TreeviewAction::SetViewRoot(target)
            } else {
                TreeviewAction::Focus(target)
            }
        })
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
        let layout = self.rows();
        let row = in_treeview
            .then(|| layout.row_at(y))
            .flatten()
            .filter(|&index| index < rows.len());

        let within = layout.within(y);
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
                        on_triangle: has_triangle(row)
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
                        self.click_action(&press, row.map(|index| &rows[index]))
                    };
                    damaged = true;
                }
            }
            MouseEventKind::Scroll { delta, .. } if in_treeview => {
                let scroll = self
                    .scroll
                    .saturating_add_signed(ui::scroll_rows(delta, SCROLL_STEP))
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

    /// The `×` that closes a hovered row
    fn close_button(&self, width: f32, settings: &Settings) -> UITree {
        let color = if self.hovered_close {
            settings.palette.fg
        } else {
            settings.palette.inactive
        };
        let left = width - close_width() + (close_width() - font_size()) / 2.0;
        self.rows()
            .text(
                ui::text("×", with_opacity(color)),
                left,
                left + close_width(),
            )
            .with_pointer()
    }

    /// Where a drag over the row drops: a line between siblings, else a border around the row (into it)
    fn drop_marker(&self, row: &TreeviewRow, width: f32) -> UITree {
        let color = with_opacity(settings::get().palette.accent);
        if drop_target(row, self.zone).1 == SpawnPosition::AppendChild {
            UITree::leaf_styled(
                UIElement::border(color),
                Style {
                    border: Length::uniform_rect(Length::Px(2.0)),
                    ..Style::absolute([2.0, 0.0], [(width - 4.0).max(0.0), row_height()])
                },
            )
        } else {
            let line_left = LEFT_PAD + row.depth as f32 * INDENT;
            let line_top = if self.zone == DropZone::Top {
                0.0
            } else {
                row_height() - INSERT_LINE_HEIGHT
            };
            rect(
                color,
                [line_left, line_top],
                [(width - line_left - 4.0).max(0.0), INSERT_LINE_HEIGHT],
            )
        }
    }

    /// The row at `index` of the rows, placed for the current scroll
    fn render_row(
        &self,
        index: usize,
        row: &TreeviewRow,
        focused_row: &WorldTreePath,
        focus_inner: bool,
        width: f32,
        settings: &Settings,
    ) -> UITree {
        let dragging = self.press.as_ref().filter(|press| press.dragging);
        let is_focused = row.path == *focused_row;
        let is_hovered = self.hovered == Some(index);
        let is_dragged = dragging.is_some_and(|press| press.path == row.path);

        let mut parts = Vec::new();
        for level in 0..row.depth {
            parts.push(rect(
                guide(),
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
        let rows = self.rows();
        // like a browser tab's, only on the hovered row
        let shows_close = row.closable && is_hovered && dragging.is_none();
        let text_color = with_opacity(if is_dragged || is_inactive(row) {
            settings.palette.inactive
        } else {
            settings.palette.fg
        });
        let label = if row.label.is_empty() {
            "…".to_string()
        } else {
            row.label.clone()
        };
        parts.push(rows.text(
            ui::text(label, text_color),
            text_left,
            width - if shows_close { close_width() } else { 0.0 },
        ));
        if shows_close {
            parts.push(self.close_button(width, settings));
        }
        if has_triangle(row) {
            parts.push(
                UITree::leaf_styled(
                    UIElement::Nothing,
                    Style::absolute([text_left, 0.0], [triangle_width(), row_height()]),
                )
                .with_pointer(),
            );
        }

        if dragging.is_some() && is_hovered && !is_dragged {
            parts.push(self.drop_marker(row, width));
        }

        let background = if is_focused {
            Some(focus_bg())
        } else if is_hovered && dragging.is_none() {
            Some(hover_bg())
        } else {
            None
        };
        rows.row(index, width, background, parts)
    }

    /// Where the rows are, for the current scroll
    fn rows(&self) -> Rows {
        Rows::new(TOP_PAD).scrolled(self.scroll)
    }

    /// Scrolls to the focused row if focus moved there since the last render
    pub fn render(
        &mut self,
        rows: &[TreeviewRow],
        [width, height]: [f32; 2],
        focus_path: &WorldTreePath,
        focus_inner: bool,
    ) -> UITree {
        let focused_row = focused_row(rows, focus_path);

        if self.followed.as_ref() != Some(&focused_row) {
            if let Some(index) = rows.iter().position(|row| row.path == focused_row) {
                self.scroll = ui::scroll_to(self.scroll, index, shown_rows(height));
            }
            self.followed = Some(focused_row.clone());
        }
        self.scroll = self.scroll.min(max_scroll(rows.len(), height));
        let settings = settings::get();

        let mut children: Vec<UITree> = rows
            .iter()
            .enumerate()
            .skip(self.scroll)
            .map(|(index, row)| {
                self.render_row(index, row, &focused_row, focus_inner, width, &settings)
            })
            .collect();

        if let Some(menu) = &self.menu {
            children.push(menu.render([width, height]));
        }
        UITree::new_styled(
            UIElement::Backgrounded(panel_bg()),
            Style::full_size(),
            children,
        )
    }
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
fn shown_rows(height: f32) -> usize {
    Rows::new(TOP_PAD).fitting(height)
}

/// The furthest the rows can scroll: until the last one is at the bottom
fn max_scroll(row_count: usize, height: f32) -> usize {
    ui::max_scroll(row_count, shown_rows(height))
}

fn distance([x1, y1]: [f64; 2], [x2, y2]: [f64; 2]) -> f64 {
    (x2 - x1).hypot(y2 - y1)
}
