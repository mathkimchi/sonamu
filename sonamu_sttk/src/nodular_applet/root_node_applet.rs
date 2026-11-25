use crate::{
    command_hub::repl::{self, Repl},
    man_page::ManPageApplet,
    nodular_applet::{
        AppletSpawner, NodeRequest, NodularAppletInitializer, history,
        history_view::{HistoryAction, HistoryPointer, HistoryResponse},
        node_holder::{NodeHolder, client_spawners},
        node_tree::{AttachClient, NodeId, NodeTree, SpawnClient},
        project_applet::ProjectApplet,
        swipe::{Settle, Strip, Swipe},
        traversal::{self, Traversal},
        treeview::{
            TreeviewAction, TreeviewPointer, TreeviewResponse, TreeviewRow, focused_row,
            treeview_rows,
        },
    },
    session,
    standard_keybinds::standard_keybind_request,
    ui,
};
use calloop::{
    EventLoop, LoopHandle,
    channel::{Channel, Sender},
    timer::{TimeoutAction, Timer},
};
use sonamu_common::{
    command::{self, NodeCall, NodeUid, Value, World},
    sap::{
        packets::{
            BarState, OpenApplet, SpawnPosition, StandardEvent, StandardRequest, StateKind,
            Treeview,
        },
        raw_client_initializer::RawClientInitializer,
        session::{Session, SideView},
        shared_state::{SharedState, StatePublisher},
    },
    settings::{self, BufferLine, ERROR, Palette, Settings, TreeviewFloating, TreeviewSide},
    utils::tree::{
        tree_node_path::TreeNodePath,
        world_tree::{Step, WorldTree, WorldTreePath},
    },
};
use sonamu_ui::{
    display_units::DisplayContainerSize,
    ui_element::{CharGrid, Style, UIElement, UITree, font_size},
    ui_event::{KeyModifiers, KeySymbol, MouseEvent, MouseEventKind, SwipePhase, UIEvent},
    winit_backend::ScreenArea,
};
use std::{
    cell::Cell,
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, RwLock, atomic::Ordering},
    time::{Duration, Instant},
};

/// Output rows the buffer line shows above its prompt at most (the last entry's)
const BUFFER_OUTPUT_ROWS: usize = 10;

/// evdev keycodes (`UIEvent::Key::raw_keycode`)
const KEY_LEFTALT: u32 = 56;
const KEY_RIGHTALT: u32 = 100;
const KEY_DELETE: u32 = 111;

/// Holds the tree of nodes, is held by a Basic Applet runner (applet runner).
///
/// Every applet in the tree is a client of this, on its own thread.
pub struct RootNodeApplet {
    holder: NodeHolder,
    surface: StatePublisher<UITree>,
    request_sender: Sender<StandardRequest>,

    latest_size: DisplayContainerSize,

    treeview_pointer: TreeviewPointer,
    /// Mouse state over the history view, shared by the screens (it acts on the active one)
    history_pointer: HistoryPointer,
    /// The last treeview drop and the node it was on.
    /// A drop can come before its drag's pluck finishes, which can shift the paths.
    last_drop: Option<(WorldTreePath, NodeId)>,
    /// The treeview is shown: pinned, or Alt held after an Alt+traversal key (see `update_treeview_visibility`)
    treeview_shown: bool,
    /// An Alt+traversal key moved the selector and Alt is still held
    alt_traversal: bool,
    /// Focus when a key capture started (`check_capture`)
    capture_focus: Option<WorldTreePath>,
    /// The view always shown in the column beside the panes (set by the status bar's arrow, Super+H; saved in the
    /// session; a clean session starts with the tree)
    pinned: Option<SideView>,
    /// `history::revision` when last drawn
    history_revision: u64,
    /// The view root when it is a row inside an applet (double click, right click › Set as view root, Alt+R);
    /// `None`: the `NodeTree`'s view root. Moves out with focus (see `update_view_path`).
    view_path: Option<WorldTreePath>,
    /// The focus path `update_view_path` last saw: it only moves the view out when focus moves, since right
    /// after switching screens an applet's published focus (eg a project's) is still the other screen's
    view_path_focus: Option<WorldTreePath>,
    /// The applets' area's left edge and width, as last sent to them
    applet_area: [u32; 2],
    /// Where the session is saved, if anywhere
    session_path: Option<PathBuf>,
    /// The last one written, so unchanged ones aren't written again
    saved_session: Option<Session>,
    /// Each monitor's brightness, saved in the session (#158; set by the runner)
    brightness: BTreeMap<String, f32>,
    /// The screen the pointer was last over, if any
    pointer_screen: Option<usize>,
    /// While a swipe acts on the screen under the pointer (#158): the screen keys stay on, made active again
    /// once the swipe (and its settle) ends
    lent_from: Option<usize>,
    /// Each monitor's focus (`NodeTree::screen_focus_uids`) as last saved, kept for monitors that went away (#158)
    monitor_focus: BTreeMap<String, Vec<NodeUid>>,
    /// Every screen (#60), each with its own focus (see `NodeTree::activate_screen`).
    /// The active one's view is the fields above (`latest_size`, `view_path`, `treeview_pointer`).
    screens: Vec<Screen>,

    /// The 3 finger swipe while the fingers are down (#141)
    swipe: Option<Swipe>,
    /// Easing onto a node after a swipe, a frame at a time (`tick_settle`)
    settle: Option<Settle>,
    /// Whether `tick_settle`'s timer is running
    settling: bool,
    /// How far a 4 finger swipe moved, until it acts (once a swipe)
    select_swipe: Option<[f64; 2]>,
    /// The child row each row last had focus under, for swipes into children (#142); not saved
    remembered: HashMap<WorldTreePath, WorldTreePath>,
    /// When Ctrl+Shift+Q was pressed, while it asks to press it again (#139)
    quit_asked: Option<Instant>,
    /// The focus path `remember_focus` last saw, to emit `focus` events when it moves
    last_focus: Option<WorldTreePath>,
    /// The buffer line's prompt (the command hub's, but the agent by default), at the bottom of the active screen
    repl: Repl,
    /// Keys go to the buffer line (Super+Esc), like vim's normal mode; else `INSERT`
    buffer_mode: bool,
    /// Lines the buffer line is scrolled up (PageUp) through its history; 0: the last entry's output
    buffer_scroll: usize,
    /// Mouse scrolling over the buffer line not yet a line
    buffer_scrolled: ui::ScrollSteps,
    /// The buffer line's height (px) on the active screen when last drawn in buffer mode, where scrolls scroll it
    buffer_line_height: Cell<f32>,
    /// The last `sonamu_script` message, shown over the buffer line for `ECHO_TIME` (Emacs' echo area)
    echo: Option<(sonamu_script::Message, Instant)>,
    /// `sonamu_script::last_message`'s count when last echoed
    echoed: u64,
    /// What the status bar was last told (#150; it runs beside the root, `status_bar::applet`)
    bar_state: BarState,
    loop_handle: LoopHandle<'static, Self>,
}

/// A screen, in the root's area. While inactive it keeps its view, and shows the treeview only if pinned.
#[derive(Default)]
struct Screen {
    /// Its monitor's (`ScreenArea::name`); empty until the runner tells
    name: String,
    /// `[x, y, width, height]`
    rect: [u32; 4],
    view_path: Option<WorldTreePath>,
    treeview_pointer: TreeviewPointer,
}

/// Where the treeview is
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TreeviewPlace {
    /// Pinned beside the panes
    Column(TreeviewSide),
    /// Over the panes
    Floating,
}

impl RootNodeApplet {
    /// How often the session is saved (if it changed), besides before quitting
    const SAVE_INTERVAL: Duration = Duration::from_secs(2);
    /// Between frames of a swipe settling
    const SETTLE_FRAME: Duration = Duration::from_millis(8);
    /// How far (px) a 4 finger swipe moves to act
    const SELECT_DISTANCE: f64 = 50.0;
    /// How long Ctrl+Shift+Q waits to be pressed again
    const QUIT_CONFIRM: Duration = Duration::from_secs(3);
    const QUIT_NOTICE: &str = "Press Ctrl+Shift+Q again to quit";

    /// Toggles buffer mode (Super+Esc), where every key goes to the buffer line
    fn set_buffer_mode(&mut self, on: bool) {
        self.buffer_mode = on;
        if on {
            self.repl.check_agent();
        }
        self.sync_key_grab();
        self.surface.damage();
    }

    /// The `buffer_mode` keybind, and keys while the buffer line takes them; whether it took the key (releases too,
    /// so nothing else sees half a key)
    fn handle_grabbed_key(&mut self, ui_event: &UIEvent) -> bool {
        let &UIEvent::Key {
            symbol,
            modifiers,
            pressed,
            ..
        } = ui_event
        else {
            return false;
        };
        if is_buffer_mode_key(ui_event) {
            self.set_buffer_mode(!self.buffer_mode);
            return true;
        }
        if self.buffer_mode {
            if let (true, Some(key)) = (pressed, symbol) {
                self.handle_buffer_key(key, modifiers);
            }
            return true;
        }
        false
    }

    /// Every key comes to the root in buffer mode
    fn sync_key_grab(&self) {
        sonamu_common::BUFFER_MODE.store(self.buffer_mode, Ordering::Relaxed);
    }

    /// A key in buffer mode: Esc clears the prompt, or leaves buffer mode if it's empty; others go to the prompt
    fn handle_buffer_key(&mut self, key: KeySymbol, modifiers: KeyModifiers) {
        if matches!(key, KeySymbol::Escape)
            && modifiers == KeyModifiers::NONE
            && self.repl.prompt_is_empty()
            // it refuses the agent's Luau
            && !self.repl.asking()
        {
            self.set_buffer_mode(false);
            return;
        }
        match key {
            KeySymbol::PageUp => self.scroll_buffer(ui::page(BUFFER_OUTPUT_ROWS) as isize),
            KeySymbol::PageDown => self.scroll_buffer(-(ui::page(BUFFER_OUTPUT_ROWS) as isize)),
            _ => {
                if matches!(key, KeySymbol::Enter) {
                    self.buffer_scroll = 0;
                }
                self.repl.handle_key(key, modifiers);
            }
        }
        self.surface.damage();
    }

    /// Scrolls the buffer line `by` lines up (negative: down) through the whole history (`buffer_line`)
    fn scroll_buffer(&mut self, by: isize) {
        let [_, _, width, height] = self.screens[self.holder.tree.active_screen()].rect;
        let (columns, _) =
            CharGrid::largest_fittable_size(DisplayContainerSize::new(width, height));
        let echo = self.echo.as_ref().map(|(message, _)| message);
        let (lines, last) = buffer_lines(&self.repl, echo, columns, &settings::get().palette);
        self.buffer_scroll = self
            .buffer_scroll
            .saturating_add_signed(by)
            .min(max_buffer_scroll(lines.len(), last));
    }

    /// Where the treeview and the history are drawn on the active screen (`side_areas`)
    fn side_areas(&self, settings: &Settings) -> SideAreas {
        side_areas(settings, self.latest_size, self.pinned, self.treeview_shown)
    }

    /// The applets' area's left edge and width: beside the pinned column, else everything
    fn applet_area(&self, settings: &Settings) -> [u32; 2] {
        applet_area(settings, self.latest_size, self.pinned)
    }

    fn inner_applet_size(&self) -> DisplayContainerSize {
        DisplayContainerSize::new(
            self.applet_area(&settings::get())[1],
            self.latest_size.height,
        )
    }

    /// Shows the treeview if pinned or Alt is held after an Alt+traversal key, else hides it, resizing the applets if their area changed
    /// (also when the treeview settings or pinning did)
    fn update_treeview_visibility(&mut self) {
        let settings = settings::get();
        let revision = history::revision();
        if revision != self.history_revision {
            self.history_revision = revision;
            self.surface.damage();
        }
        let shown = self.pinned == Some(SideView::Tree)
            || self.alt_traversal
            // eg a drag or menu that started there
            || self.treeview_pointer.is_pressed();
        if shown != self.treeview_shown {
            self.treeview_shown = shown;
            self.surface.damage();
        }
        // the buffer line's room may have changed with the settings
        let size = self
            .screens
            .get(self.holder.tree.active_screen())
            .map_or(self.latest_size, |screen| {
                content_size(&settings, screen.rect)
            });
        let applet_area = self.applet_area(&settings);
        if applet_area != self.applet_area || size != self.latest_size {
            self.latest_size = size;
            let applet_area = self.applet_area(&settings);
            self.applet_area = applet_area;
            self.surface.damage();
            self.holder
                .tree
                .handle_event(StandardEvent::UIEvent(UIEvent::WindowResized(
                    self.inner_applet_size(),
                )));
        }
        self.resize_inactive_screens();
    }

    /// Publishes the window (every screen) if damaged and the runner read the last one
    fn flush_surface(&mut self) {
        let settings = settings::get();
        let shown = self.treeview_shown;
        let pinned = self.pinned;
        let quit_asked = self.quit_asked.is_some();
        let buffer_mode = self.buffer_mode;
        let buffer_scroll = self.buffer_scroll;
        let echo = self.echo.as_ref().map(|(message, _)| message);
        let Self {
            holder,
            treeview_pointer,
            history_pointer,
            view_path,
            screens,
            repl,
            buffer_line_height,
            ..
        } = self;
        let active = holder.tree.active_screen();
        self.surface.flush(
            || {
                let screens = screens.iter_mut().enumerate().map(|(index, screen)| {
                    let [x, y, _, height] = screen.rect;
                    // above the buffer line, when it always shows
                    let size = content_size(&settings, screen.rect);
                    // the active one's view is the root's own
                    let (shown, view_path, treeview_pointer) = if index == active {
                        (shown, &*view_path, &mut *treeview_pointer)
                    } else {
                        (
                            pinned == Some(SideView::Tree),
                            &screen.view_path,
                            &mut screen.treeview_pointer,
                        )
                    };
                    let areas = side_areas(&settings, size, pinned, shown);
                    let [left, width] =
                        applet_area(&settings, size, pinned).map(|length| length as f32);
                    let mut content = vec![UITree::new_container_styled(
                        Style::absolute([left, 0.0], [width, size.height as f32]),
                        vec![holder.tree.get_screen_window(index)],
                    )];
                    if let Some(area) = areas.history {
                        content.push(Self::get_history(&area, history_pointer));
                    }
                    if let Some(area) = areas.tree {
                        content.push(Self::get_treeview(
                            &holder.tree,
                            index,
                            view_path.as_ref(),
                            treeview_pointer,
                            area,
                        ));
                    }
                    if quit_asked && index == active {
                        content.push(notice(&settings, Self::QUIT_NOTICE, size));
                    }
                    let mut window = vec![UITree::new_container_styled(
                        Style::absolute([0.0, 0.0], [size.width as f32, size.height as f32]),
                        content,
                    )];
                    let full = DisplayContainerSize::new(size.width, height);
                    let echo = echo.filter(|_| index == active);
                    if index == active && buffer_mode {
                        let (line, height) = buffer_line(
                            repl,
                            echo,
                            full,
                            Bar::Prompt(buffer_scroll),
                            &settings.palette,
                        );
                        buffer_line_height.set(height);
                        window.push(line);
                    } else if settings.buffer_line == BufferLine::Always {
                        window
                            .push(buffer_line(repl, echo, full, Bar::Insert, &settings.palette).0);
                    } else if echo.is_some() {
                        window.push(buffer_line(repl, echo, full, Bar::None, &settings.palette).0);
                    }
                    UITree::new_container_styled(
                        Style::absolute([x as f32, y as f32], [size.width as f32, height as f32]),
                        window,
                    )
                });
                UITree::new_container(screens.collect())
            },
            &self.request_sender,
        );
    }

    /// The screens' areas from the runner
    fn set_screens(&mut self, areas: Vec<ScreenArea>) {
        if areas.is_empty() {
            return;
        }
        // a monitor going away (or moving to another screen) keeps its focus for when it's back
        self.remember_screen_focus();
        let old_names: Vec<String> = self
            .screens
            .iter()
            .map(|screen| screen.name.clone())
            .collect();
        // the active one may be gone
        let active = self.holder.tree.active_screen().min(areas.len() - 1);
        self.activate_screen(active);
        self.holder.tree.set_screen_count(areas.len());
        if self.screens.len() != areas.len() {
            sonamu_script::emit("screens", vec![Value::Number(areas.len() as f64)]);
        }
        self.screens.resize_with(areas.len(), Screen::default);
        for (screen, area) in self.screens.iter_mut().zip(areas) {
            screen.rect = area.rect;
            screen.name = area.name;
        }
        self.latest_size = content_size(&settings::get(), self.screens[active].rect);
        self.surface.damage();
        self.holder
            .tree
            .handle_event(StandardEvent::UIEvent(UIEvent::WindowResized(
                self.inner_applet_size(),
            )));
        self.resize_inactive_screens();
        // a screen showing another monitor (eg at startup) gets that monitor's focus
        for (index, screen) in self.screens.iter().enumerate() {
            if old_names.get(index) != Some(&screen.name)
                && let Some(chain) = self.monitor_focus.get(&screen.name)
            {
                self.holder.tree.focus_chain(index, chain);
            }
        }
    }

    /// Keeps each monitor's focus in `monitor_focus`
    fn remember_screen_focus(&mut self) {
        for (index, screen) in self.screens.iter().enumerate() {
            // not before the runner names it
            if !screen.name.is_empty() && index < self.holder.tree.screen_count() {
                self.monitor_focus.insert(
                    screen.name.clone(),
                    self.holder.tree.screen_focus_uids(index),
                );
            }
        }
    }

    /// Keys and the treeview go to the screen from now on (eg the pointer moved onto it)
    fn activate_screen(&mut self, index: usize) {
        let active = self.holder.tree.active_screen();
        if index == active || index >= self.screens.len() {
            return;
        }
        let left = &mut self.screens[active];
        left.view_path = self.view_path.take();
        left.treeview_pointer = std::mem::take(&mut self.treeview_pointer);
        let screen = &mut self.screens[index];
        self.view_path = screen.view_path.take();
        self.treeview_pointer = std::mem::take(&mut screen.treeview_pointer);
        self.latest_size = content_size(&settings::get(), screen.rect);
        self.holder.tree.activate_screen(index);
        self.surface.damage();
    }

    /// Sends the inactive screens' applets' areas' sizes to the tree (the treeview shows on them only if pinned)
    fn resize_inactive_screens(&mut self) {
        let settings = settings::get();
        let active = self.holder.tree.active_screen();
        for (index, screen) in self.screens.iter().enumerate() {
            if index == active {
                continue;
            }
            let size = content_size(&settings, screen.rect);
            self.holder.tree.resize_screen(
                index,
                Some(DisplayContainerSize::new(
                    applet_area(&settings, size, self.pinned)[1],
                    size.height,
                )),
            );
        }
    }

    fn send_request(&self, request: StandardRequest) {
        self.request_sender.send(request).unwrap();
    }

    /// Writes the session if it changed since the last write
    fn save_session(&mut self) {
        self.complete_history();
        history::save();
        if self.session_path.is_none() {
            return;
        }
        self.remember_screen_focus();
        let session = Session {
            tree: self.holder.tree.save(),
            monitor_focus: self.monitor_focus.clone(),
            brightness: self.brightness.clone(),
            screen_focus: Vec::new(),
            focus: WorldTreePath::root(),
            side_view: self.pinned,
            treeview_pinned: false,
        };
        if self.saved_session.as_ref() == Some(&session) {
            return;
        }
        let Some(path) = &self.session_path else {
            return;
        };
        match session::write(path, &session) {
            Ok(()) => self.saved_session = Some(session),
            Err(err) => log::warn!("Couldn't save the session to {}: {err}", path.display()),
        }
    }

    /// Lets the history's ancestors reach the root across projects
    fn complete_history(&self) {
        let tree = &self.holder.tree;
        history::complete_ancestors(tree.uid(tree.root()), || tree.save());
    }

    /// What a click on the history view asked for
    fn apply_history_action(&mut self, action: HistoryAction) {
        match action {
            HistoryAction::SetMode(mode) => {
                if let Err(err) = sonamu_script::set_setting("history.mode", mode.into()) {
                    log::warn!("Couldn't set history.mode: {err}");
                }
            }
            HistoryAction::Restore(id) => self.restore(history::take(id)),
            HistoryAction::Remove(id) => history::remove(id),
            HistoryAction::Clear => history::clear(),
        }
    }

    /// Restores the entries (newest first) each under its nearest ancestor that's still there, else the root; that
    /// node's world does it (also this one, on its own thread's next turn)
    fn restore(&mut self, entries: Vec<history::Closed>) {
        self.complete_history();
        for entry in entries {
            let tree = &self.holder.tree;
            let target = entry
                .ancestors
                .iter()
                .copied()
                .find(|&uid| command::is_alive(uid))
                .unwrap_or_else(|| tree.uid(tree.root()));
            let id = entry.id as f64;
            history::hold(entry);
            command::send(target, command::RESTORE_CLOSED, vec![Value::Number(id)]);
        }
    }

    /// Shows `sonamu_script`'s last message if it's new, until `ECHO_TIME` passes
    fn echo_message(&mut self) {
        const ECHO_TIME: Duration = Duration::from_secs(5);
        let (Some(message), count) = sonamu_script::last_message() else {
            return;
        };
        if count == self.echoed {
            return;
        }
        self.echoed = count;
        self.echo = Some((message, Instant::now()));
        self.loop_handle
            .insert_source(
                Timer::from_duration(ECHO_TIME),
                |_, &mut (), root: &mut Self| {
                    // not a newer one meanwhile
                    if root
                        .echo
                        .as_ref()
                        .is_some_and(|(_, shown)| shown.elapsed() >= ECHO_TIME)
                    {
                        root.echo = None;
                        root.surface.damage();
                    }
                    TimeoutAction::Drop
                },
            )
            .unwrap();
    }

    /// Ctrl+Shift+Q: quits if pressed again within `QUIT_CONFIRM`, else shows a notice asking for that (#139)
    fn ask_quit(&mut self) {
        if self
            .quit_asked
            .is_some_and(|asked| asked.elapsed() < Self::QUIT_CONFIRM)
        {
            self.quit();
            return;
        }
        self.quit_asked = Some(Instant::now());
        self.surface.damage();
        self.loop_handle
            .insert_source(
                Timer::from_duration(Self::QUIT_CONFIRM),
                |_, &mut (), root: &mut Self| {
                    // not asked again meanwhile
                    if root
                        .quit_asked
                        .is_some_and(|asked| asked.elapsed() >= Self::QUIT_CONFIRM)
                    {
                        root.quit_asked = None;
                        root.surface.damage();
                    }
                    TimeoutAction::Drop
                },
            )
            .unwrap();
    }

    /// Saves first, since the runner may end the process right after
    fn quit(&mut self) {
        self.save_session();
        self.send_request(StandardRequest::Quit);
    }

    /// The saved session if there is one, else `root_initializer` at the root
    #[must_use]
    fn new(
        root_initializer: NodularAppletInitializer,
        mut applet_spawner_registry: BTreeMap<String, AppletSpawner>,
        session_path: Option<PathBuf>,
        surface: SharedState<UITree>,
        request_sender: Sender<StandardRequest>,
        event_loop: &LoopHandle<'static, Self>,
        world: World,
    ) -> Self {
        // the buffer line's evals and completions
        let (repl_wake, repl_woken) = calloop::channel::channel();
        event_loop
            .insert_source(repl_woken, |event, &mut (), root: &mut Self| {
                if let calloop::channel::Event::Msg(()) = event {
                    root.repl.wake();
                    root.echo_message();
                    root.surface.damage();
                }
            })
            .unwrap();
        let repl_waker: repl::Waker = Arc::new(move || {
            let _ = repl_wake.send(());
        });
        let echo_waker = Arc::clone(&repl_waker);
        sonamu_script::on_message(move || echo_waker());
        // saved projects are made again by these
        applet_spawner_registry.insert(
            ProjectApplet::SPAWNER.to_string(),
            ProjectApplet::get_applet_spawner(),
        );
        // Super+? opens it
        applet_spawner_registry.insert("help".to_string(), ManPageApplet::get_applet_spawner());
        for (name, spawner) in &applet_spawner_registry {
            command::declare(name, spawner.commands());
        }
        command::declare("wl_surface", &crate::wl_surface_applet::COMMANDS);
        let applet_spawners = Arc::new(RwLock::new(applet_spawner_registry));
        let (spawn_client, attach_client) = client_spawners(
            event_loop,
            Arc::clone(&applet_spawners),
            |root: &mut Self, id, request| {
                root.handle_node_request(id, request);
            },
        );
        let saved_session = session_path.as_deref().and_then(session::load);
        history::load();
        let tree = match &saved_session {
            Some(saved) => restore_tree(saved, spawn_client, attach_client, world),
            None => NodeTree::new(root_initializer, spawn_client, attach_client, world),
        };
        let monitor_focus = saved_session
            .as_ref()
            .map(|saved| saved.monitor_focus.clone())
            .unwrap_or_default();
        let brightness = saved_session
            .as_ref()
            .map(|saved| saved.brightness.clone())
            .unwrap_or_default();
        // the runner sets it as monitors connect
        let _ = request_sender.send(StandardRequest::SavedBrightness(brightness.clone()));
        let send = {
            let request_sender = request_sender.clone();
            // the runner may already be gone while shutting down
            Box::new(move |request| {
                let _ = request_sender.send(request);
            })
        };

        let mut root = Self {
            holder: NodeHolder::new(tree, send, false, applet_spawners),
            surface: StatePublisher::new(surface),
            request_sender,
            // Just making stuff up
            latest_size: DisplayContainerSize::new(800, 600),
            treeview_pointer: TreeviewPointer::default(),
            history_pointer: HistoryPointer::default(),
            last_drop: None,
            // the root node starts focused
            treeview_shown: true,
            alt_traversal: false,
            capture_focus: None,
            pinned: initial_pin(saved_session.as_ref()),
            history_revision: history::revision(),
            view_path: None,
            view_path_focus: None,
            applet_area: [0, 800],
            session_path,
            saved_session,
            brightness,
            pointer_screen: None,
            lent_from: None,
            monitor_focus,
            screens: vec![Screen {
                rect: [0, 0, 800, 600],
                ..Screen::default()
            }],
            swipe: None,
            settle: None,
            settling: false,
            select_swipe: None,
            remembered: HashMap::new(),
            quit_asked: None,
            last_focus: None,
            repl: Repl::new(repl::Kind::Agent, repl_waker),
            buffer_mode: false,
            echo: None,
            buffer_scroll: 0,
            buffer_scrolled: ui::ScrollSteps::default(),
            buffer_line_height: Cell::new(0.0),
            echoed: 0,
            bar_state: BarState::default(),
            loop_handle: event_loop.clone(),
        };
        // a clean session starts with the help shown
        if root.saved_session.is_none() {
            root.open_help();
        }
        root
    }

    /// Restores the session at `session_path` if there is one (else starts with `root_initializer` at the root),
    /// and keeps it saved there
    pub fn get_initializer(
        root_initializer: NodularAppletInitializer,
        applet_spawner_registry: BTreeMap<String, AppletSpawner>,
        session_path: Option<PathBuf>,
    ) -> Box<dyn RawClientInitializer> {
        struct Initializer {
            root_initializer: NodularAppletInitializer,
            applet_spawner_registry: BTreeMap<String, AppletSpawner>,
            session_path: Option<PathBuf>,
        }
        impl RawClientInitializer for Initializer {
            fn init(
                // smth smth box needs to know size
                self: Box<Self>,
                surface: SharedState<UITree>,
                // the treeview is drawn into the surface
                _treeview: SharedState<Treeview>,
                event_queue: Channel<StandardEvent>,
                // yeah, ik the naming is inconsistent bc I'm not saying "event_receiver" or "event_rx", but it's calm
                // (I am really trying to convince myself this is fine, I am the strawman)
                request_sender: Sender<StandardRequest>,
            ) {
                let mut event_loop = EventLoop::try_new().unwrap();

                event_loop
                    .handle()
                    .insert_source(event_queue, |event, &mut (), root: &mut RootNodeApplet| {
                        let calloop::channel::Event::Msg(event) = event else {
                            return;
                        };
                        root.handle_event(event);
                    })
                    .unwrap();

                event_loop
                    .handle()
                    .insert_source(
                        Timer::from_duration(RootNodeApplet::SAVE_INTERVAL),
                        |_, &mut (), root: &mut RootNodeApplet| {
                            root.save_session();
                            TimeoutAction::ToDuration(RootNodeApplet::SAVE_INTERVAL)
                        },
                    )
                    .unwrap();

                let (settings_tx, settings_rx) = calloop::channel::channel();
                settings::subscribe(settings_tx);
                event_loop
                    .handle()
                    .insert_source(settings_rx, |event, &mut (), root: &mut RootNodeApplet| {
                        if let calloop::channel::Event::Msg(()) = event {
                            root.surface.damage();
                        }
                    })
                    .unwrap();

                // commands for the root's tree (`sonamu_common::command`)
                let (world, calls) = calloop::channel::channel();
                event_loop
                    .handle()
                    .insert_source(calls, |event, &mut (), root: &mut RootNodeApplet| {
                        if let calloop::channel::Event::Msg(call) = event {
                            root.handle_node_call(call);
                        }
                    })
                    .unwrap();
                command::set_root_world(world.clone());

                let mut root = RootNodeApplet::new(
                    self.root_initializer,
                    self.applet_spawner_registry,
                    self.session_path,
                    surface,
                    request_sender,
                    &event_loop.handle(),
                    world,
                );

                // the user's `init.luau`, now that the tree's commands work
                sonamu_script::start();

                event_loop
                    .run(None, &mut root, RootNodeApplet::after_batch)
                    .unwrap();
            }
        }
        Box::new(Initializer {
            root_initializer,
            applet_spawner_registry,
            session_path,
        })
    }

    /// After each batch of events, since any of them may move focus or damage
    fn after_batch(&mut self) {
        self.holder.finish();
        if std::mem::take(&mut self.holder.quit) {
            self.ask_quit();
        }
        self.holder.tree.update_view_root();
        self.update_view_path();
        self.remember_focus();
        self.damage_if_tree_changed();
        self.update_treeview_visibility();
        self.holder.tree.update_layout();
        // a frame each time the runner reads the last one
        if self.holder.tree.zoom_animating() {
            self.surface.damage();
        }
        // the wl keyboard stays on the screen lent from
        if self.lent_from.is_none() {
            self.holder.update_wl_focus();
        }
        self.flush_surface();
        self.send_bar_state();
    }

    /// From the runner
    fn run_keybind(&mut self, keybind: RootKeybind) {
        match keybind {
            RootKeybind::Help => self.open_help(),
            RootKeybind::ViewRoot => self.toggle_view_root(),
            RootKeybind::ViewParent => {
                let mut parent = self.holder.tree.get_focus_path();
                parent.0.pop();
                self.set_view_root(&parent);
            }
            RootKeybind::Swipe { vertical, forward } => self.swipe_key(vertical, forward),
            RootKeybind::RestoreClosed => self.restore(history::pop_newest()),
            // pins it with the keys, then gives them, then unpins
            RootKeybind::History => {
                let unpin = self.pinned == Some(SideView::History) && self.history_pointer.keys;
                self.pin((!unpin).then_some(SideView::History));
                self.set_history_keys(!unpin);
            }
        }
    }

    fn handle_event(&mut self, event: StandardEvent) {
        self.check_capture();
        // an applet recording a keybind gets the key, whatever it is
        let capturing = sonamu_common::capturing()
            && matches!(event, StandardEvent::UIEvent(UIEvent::Key { .. }));
        if let StandardEvent::UIEvent(ui_event) = &event
            && !capturing
            && self.handle_grabbed_key(ui_event)
        {
            return;
        }
        if let StandardEvent::UIEvent(UIEvent::Swipe { fingers, phase }) = event {
            self.handle_fingers(fingers, phase);
            return;
        }
        // a key's modifiers are from before it, so an Alt release still has Alt
        if let StandardEvent::UIEvent(UIEvent::Key {
            modifiers,
            raw_keycode,
            pressed,
            ..
        }) = event
            && (!modifiers.alt || (!pressed && matches!(raw_keycode, KEY_LEFTALT | KEY_RIGHTALT)))
        {
            self.alt_traversal = false;
        }
        if let StandardEvent::UIEvent(ui_event) = &event
            && !capturing
            && let Some(keybind) = RootKeybind::of(ui_event)
        {
            self.run_keybind(keybind);
            self.damage_if_tree_changed();
            return;
        }
        if let StandardEvent::UIEvent(ui_event) = &event
            && !capturing
            && let Some(traversal) = Traversal::of(ui_event, self.holder.tree.is_traversing())
        {
            if let UIEvent::Key { modifiers, .. } = ui_event {
                self.alt_traversal = modifiers.alt;
            }
            self.traverse(traversal);
            self.damage_if_tree_changed();
            return;
        }
        if !capturing && self.take_history_key(&event) {
            return;
        }
        match event {
            // the screens' areas follow
            StandardEvent::UIEvent(UIEvent::WindowResized(_)) => {}
            StandardEvent::UIEvent(UIEvent::Mouse(mouse_event)) => self.handle_mouse(mouse_event),
            StandardEvent::DamageAck(StateKind::Surface) => self.surface.ack(),
            // the treeview is drawn into the surface
            StandardEvent::DamageAck(StateKind::Treeview) => {}
            // the root can't close without ending everything
            StandardEvent::CloseRequest => self.quit(),
            // the root is never deactivated
            StandardEvent::Deactivate => {}
            StandardEvent::WlSurfaceRegistered(surface) => {
                // a restored tile, else where the user is working (eg under the command hub that ran it)
                let app_id = surface.app_id.clone().map_or(Value::Nil, Value::String);
                self.holder.add_wl_surface(surface);
                sonamu_script::emit("window", vec![app_id]);
            }
            StandardEvent::Pluck(path) => self.holder.start_pluck(path.as_ref()),
            StandardEvent::Place(path, position, tree) => {
                let path = path.map(|path| self.redirect_drop(path));
                self.holder.start_place(path.as_ref(), position, tree);
            }
            StandardEvent::Focus(path) => self.holder.focus(&path),
            StandardEvent::ToggleCollapsed(path) => self.holder.toggle_collapsed(&path),
            StandardEvent::NodeAction(path, action) => self.holder.node_action(&path, action),
            StandardEvent::Capturing(_) => {}
            StandardEvent::Screens(rects) => self.set_screens(rects),
            StandardEvent::Brightness { screen, level, .. } => {
                self.brightness.insert(screen, level);
            }
            // it has the screens itself
            StandardEvent::ScreenViews { .. } => {}
            StandardEvent::PinSideView(view) => self.pin(view),
            StandardEvent::ActivateScreen(index) => {
                self.lent_from = None;
                self.activate_screen(index);
                self.surface.damage();
            }
            // the status bar's
            StandardEvent::BarState(_) => {}
            // it reads the settings when it draws
            StandardEvent::SettingsChanged(_) => {
                self.surface.damage();
                sonamu_script::emit("settings", Vec::new());
            }
            // its tree is in the runner's directory
            StandardEvent::Directory(_) => {}
            StandardEvent::Open(open) => self.holder.open(open),
            // it starts them
            StandardEvent::Swipe(_) => {}
            // the root's node isn't in a world
            StandardEvent::Command { .. }
            | StandardEvent::NodeUid(_)
            | StandardEvent::FocusChain { .. }
            | StandardEvent::ScreenMouse(..) => {}
            StandardEvent::UIEvent(_)
            | StandardEvent::FocusChanged(..)
            | StandardEvent::Highlighted(..) => {
                // the root is never a project, so nothing is passed up
                let _ = self.holder.handle_ui_event(event);
            }
        }

        self.damage_if_tree_changed();
    }

    /// Ends a key capture once focus leaves where it was when it started (eg a click on another tile), so keys
    /// reach the new focus
    fn check_capture(&mut self) {
        if !sonamu_common::capturing() {
            self.capture_focus = None;
            return;
        }
        let focus = self.holder.tree.get_focus_path();
        match &self.capture_focus {
            None => self.capture_focus = Some(focus),
            Some(at) if *at != focus => {
                sonamu_common::end_capture();
                self.capture_focus = None;
            }
            Some(_) => {}
        }
    }

    /// Where a new wl surface would go, focused
    fn open_help(&mut self) {
        self.holder.open(OpenApplet {
            applet: "help".to_string(),
            args: Vec::new(),
        });
    }

    fn handle_mouse(&mut self, mut mouse_event: MouseEvent) {
        let [pointer_x, pointer_y] = mouse_event.position;
        let under = self.screens.iter().position(|screen| {
            let [x, y, width, height] = screen.rect.map(f64::from);
            (x..x + width).contains(&pointer_x) && (y..y + height).contains(&pointer_y)
        });
        // the screen it left clears its treeview's hover
        if let Some(left) = self.pointer_screen
            && Some(left) != under
        {
            self.treeview_mouse(left, &mouse_event);
        }
        self.pointer_screen = under;
        // another screen becomes active when clicked (not during a drag); until then the pointer acts there
        // without it (#158)
        if !self.treeview_pointer.is_pressed()
            && !self.holder.tree.has_pointer_grab()
            && let Some(index) = under
            && index != self.holder.tree.active_screen()
        {
            if !matches!(
                mouse_event.kind,
                MouseEventKind::Button { pressed: true, .. }
            ) {
                self.inactive_screen_mouse(index, mouse_event);
                return;
            }
            // chosen, so it stays active after a swipe
            self.lent_from = None;
            self.activate_screen(index);
        }
        // in the active screen's coords
        let [x, y, _, screen_height] = self.screens[self.holder.tree.active_screen()].rect;
        mouse_event.position = [pointer_x - f64::from(x), pointer_y - f64::from(y)];
        if let MouseEventKind::Scroll { delta, v120, .. } = mouse_event.kind
            && self.buffer_mode
            && mouse_event.position[1]
                >= f64::from(screen_height) - f64::from(self.buffer_line_height.get())
        {
            // positive scrolls down, toward the newest
            let steps = self.buffer_scrolled.add(delta, v120) as isize;
            self.scroll_buffer(-steps);
            self.surface.damage();
            return;
        }
        let settings = settings::get();
        let areas = self.side_areas(&settings);
        let response = treeview_mouse(
            &self.holder.tree,
            &settings,
            areas.tree,
            (self.holder.tree.active_screen(), self.view_path.as_ref()),
            &mut self.treeview_pointer,
            &mouse_event,
        );
        if response.damaged {
            self.surface.damage();
        }
        if let Some(action) = response.action {
            self.apply_treeview_action(action);
        }
        let history = history_mouse(
            &settings,
            areas.history,
            &mut self.history_pointer,
            &mouse_event,
        );
        if history.damaged {
            self.surface.damage();
        }
        if let Some(action) = history.action {
            self.apply_history_action(action);
        }
        // a click gives the history the keys, or takes them away
        if let MouseEventKind::Button { pressed: true, .. } = mouse_event.kind {
            self.set_history_keys(history.consumed);
        }
        if response.consumed || history.consumed {
            return;
        }

        // remap window coords -> tree coords
        mouse_event.position[0] -= f64::from(self.applet_area(&settings)[0]);
        self.holder
            .tree
            .handle_event(StandardEvent::UIEvent(UIEvent::Mouse(mouse_event)));
        self.holder.do_zoom_action();
    }

    /// A pointer event over an inactive screen (not a press, which activates it): to its treeview and panes,
    /// keys staying on the active screen (#158)
    fn inactive_screen_mouse(&mut self, index: usize, mut mouse_event: MouseEvent) {
        let settings = settings::get();
        let [x, y, ..] = self.screens[index].rect;
        mouse_event.position[0] -= f64::from(x);
        mouse_event.position[1] -= f64::from(y);
        let size = content_size(&settings, self.screens[index].rect);
        let areas = side_areas(
            &settings,
            size,
            self.pinned,
            self.pinned == Some(SideView::Tree),
        );
        let screen = &mut self.screens[index];
        let response = treeview_mouse(
            &self.holder.tree,
            &settings,
            areas.tree,
            (index, screen.view_path.as_ref()),
            &mut screen.treeview_pointer,
            &mouse_event,
        );
        if response.damaged {
            self.surface.damage();
        }
        // its actions come from presses, which activate the screen first
        let history = history_mouse(
            &settings,
            areas.history,
            &mut self.history_pointer,
            &mouse_event,
        );
        if history.damaged {
            self.surface.damage();
        }
        if response.consumed || history.consumed {
            return;
        }
        mouse_event.position[0] -= f64::from(applet_area(&settings, size, self.pinned)[0]);
        self.holder.tree.screen_mouse(index, mouse_event);
    }

    /// Touchpad swipes, on the screen under the pointer
    fn handle_fingers(&mut self, fingers: u32, phase: SwipePhase) {
        if phase == SwipePhase::Begin {
            self.lend_pointer_screen();
        }
        match fingers {
            3 => self.handle_swipe(phase),
            4 => self.handle_select_swipe(phase),
            _ => {}
        }
        // a 3 finger one ends once it settles
        if matches!(phase, SwipePhase::End { .. }) && self.settle.is_none() {
            self.return_lent_screen();
        }
        self.damage_if_tree_changed();
    }

    /// A swipe starting over an inactive screen acts there: it's active until the swipe ends, keys staying where
    /// they were (`lent_from`)
    fn lend_pointer_screen(&mut self) {
        let active = self.holder.tree.active_screen();
        if let Some(screen) = self.pointer_screen
            && screen != active
            && self.lent_from.is_none()
        {
            self.lent_from = Some(active);
            self.activate_screen(screen);
        }
    }

    /// After a swipe on another screen, keys' screen is active again
    fn return_lent_screen(&mut self) {
        if let Some(screen) = self.lent_from.take() {
            self.activate_screen(screen);
        }
    }

    /// The pointer left the screen: its treeview clears its hover
    fn treeview_mouse(&mut self, index: usize, mouse_event: &MouseEvent) {
        let settings = settings::get();
        let outside = MouseEvent {
            kind: MouseEventKind::Motion,
            position: [-1.0; 2],
            ..*mouse_event
        };
        let active = index == self.holder.tree.active_screen();
        let size = match self.screens.get(index) {
            Some(screen) if !active => content_size(&settings, screen.rect),
            Some(_) => self.latest_size,
            None => return,
        };
        let shown = if active {
            self.treeview_shown
        } else {
            self.pinned == Some(SideView::Tree)
        };
        let area = side_areas(&settings, size, self.pinned, shown).tree;
        let (view_path, pointer) = if active {
            (self.view_path.as_ref(), &mut self.treeview_pointer)
        } else {
            let screen = &mut self.screens[index];
            (screen.view_path.as_ref(), &mut screen.treeview_pointer)
        };
        let response = treeview_mouse(
            &self.holder.tree,
            &settings,
            area,
            (index, view_path),
            pointer,
            &outside,
        );
        if response.damaged {
            self.surface.damage();
        }
        if history_mouse(&settings, None, &mut self.history_pointer, &outside).damaged {
            self.surface.damage();
        }
    }

    /// Gives the history view the keyboard, or takes it away
    fn set_history_keys(&mut self, on: bool) {
        if std::mem::replace(&mut self.history_pointer.keys, on) != on {
            sonamu_common::HISTORY_KEYS.store(on, Ordering::Relaxed);
            self.surface.damage();
        }
    }

    /// Whether the history view, if it has the keyboard, took the event: every key, so nothing else sees half a key
    fn take_history_key(&mut self, event: &StandardEvent) -> bool {
        let &StandardEvent::UIEvent(UIEvent::Key {
            symbol,
            modifiers,
            raw_keycode,
            pressed,
        }) = event
        else {
            return false;
        };
        if !self.history_pointer.keys {
            return false;
        }
        if pressed && modifiers == KeyModifiers::NONE {
            self.history_key(symbol, raw_keycode);
        }
        true
    }

    /// A key while the history view has the keyboard: Up/Down select, Enter restores, Delete removes, Esc gives it
    /// back
    fn history_key(&mut self, symbol: Option<KeySymbol>, raw_keycode: u32) {
        if matches!(symbol, Some(KeySymbol::Escape)) {
            self.set_history_keys(false);
            return;
        }
        let action = self.history_pointer.key(symbol, raw_keycode == KEY_DELETE);
        self.surface.damage();
        if let Some(action) = action {
            self.apply_history_action(action);
        }
    }

    /// Pins the view in the side column, or unpins (the status bar's arrow, Super+H)
    fn pin(&mut self, view: Option<SideView>) {
        self.pinned = view;
        if view != Some(SideView::History) {
            self.set_history_keys(false);
        }
        self.surface.damage();
        self.update_treeview_visibility();
    }

    /// Tells the status bar (through the runner) what it shows of the root, if it changed
    fn send_bar_state(&mut self) {
        let state = BarState {
            pinned: self.pinned,
            active_screen: self.holder.tree.active_screen(),
        };
        if state != self.bar_state {
            self.bar_state = state;
            self.send_request(StandardRequest::BarState(state));
        }
    }

    fn apply_treeview_action(&mut self, action: TreeviewAction) {
        match action {
            TreeviewAction::Focus(path) => self.holder.focus(&path),
            TreeviewAction::ToggleCollapsed(path) => self.holder.toggle_collapsed(&path),
            TreeviewAction::Node(path, action) => self.holder.node_action(&path, action),
            TreeviewAction::SetViewRoot(path) => self.set_view_root(&path),
            TreeviewAction::StartDrag(path) => {
                self.send_request(StandardRequest::RequestPluck(path));
            }
            TreeviewAction::Drop(target) => {
                let (path, position) = target.unwrap_or_else(|| {
                    (
                        self.holder.tree.get_focus_path(),
                        SpawnPosition::AppendChild,
                    )
                });
                self.last_drop = self
                    .holder
                    .tree
                    .node_at(&path.split_world().0)
                    .map(|id| (path.clone(), id));
                self.send_request(StandardRequest::RequestPlace(path, position));
            }
        }
    }

    /// If this is the last drop, the path to the node it was on now.
    /// That node was plucked too if it was dropped onto the dragged part, so it goes to the focus instead.
    fn redirect_drop(&mut self, path: WorldTreePath) -> WorldTreePath {
        match self.last_drop.take() {
            Some((dropped, id)) if dropped == path => self
                .holder
                .full_path(id, &path.split_world().1.unwrap_or_default())
                .unwrap_or_else(|| self.holder.tree.get_focus_path()),
            _ => path,
        }
    }

    /// A command for the root's tree or a node in it
    fn handle_node_call(&mut self, call: NodeCall) {
        // the status bar's (`sonamu.bar_layout`), so its effect limit is checked here, as the holder does for others;
        // the bar answers
        if call.uid.is_none() && call.name == "bar_layout" {
            if command::tree_effect(&call.name) > call.max {
                let _ = call.reply.send(Err(format!(
                    "bar_layout is above this eval's limit ({:?})",
                    call.max
                )));
            } else {
                self.send_request(StandardRequest::BarCommand {
                    name: call.name,
                    args: call.args,
                    reply: call.reply,
                });
            }
            return;
        }
        // like a `traverse` keybind (`sonamu.traverse`)
        if call.uid.is_none() && call.name == "traverse" {
            // Luau's numbers are floats; `from_args` wants integers
            let args: Vec<serde_json::Value> = call
                .args
                .iter()
                .map(|arg| match arg {
                    &Value::Number(number) if number.fract() == 0.0 => (number as i64).into(),
                    arg => serde_json::to_value(arg).unwrap_or_default(),
                })
                .collect();
            let result = if command::tree_effect(&call.name) > call.max {
                Err(format!(
                    "traverse is above this eval's limit ({:?})",
                    call.max
                ))
            } else if let Some(traversal) = Traversal::from_args(&args) {
                self.traverse(traversal);
                Ok(Value::Nil)
            } else {
                Err(format!(
                    "no traversal {}: next, prev, parent, child n, recent_child, last_child, \
                     sibling offset, select, enter",
                    serde_json::Value::from(args)
                ))
            };
            let _ = call.reply.send(result);
            return;
        }
        match self.holder.handle_node_call(call) {
            None | Some(NodeRequest::FocusSelf) => {}
            Some(_) => log::warn!("The root holder has no node to pass a request to"),
        }
        self.damage_if_tree_changed();
    }

    /// From an applet in the tree
    fn handle_node_request(&mut self, id: NodeId, request: NodeRequest) {
        // nothing is above the root
        match self.holder.handle_node_request(id, request) {
            None | Some(NodeRequest::FocusSelf) => {}
            Some(_) => log::warn!("The root holder has no node to pass a request to"),
        }
        self.damage_if_tree_changed();
    }

    /// Moves focus by the shown rows (#83), across applets' inner trees
    fn traverse(&mut self, traversal: Traversal) {
        let focus = self.holder.tree.get_focus_path();
        let target = match traversal {
            // out of its world, to the node holding it (`[[2], [1, 2]]` to `[[2]]`), or to the root
            Traversal::Select => {
                let world = focus
                    .0
                    .iter()
                    .rposition(|step| matches!(step, Step::Inner(_)));
                (!focus.0.is_empty()).then(|| WorldTreePath(focus.0[..world.unwrap_or(0)].to_vec()))
            }
            // to the zoom-out view's center; nothing without one
            Traversal::Enter => self.holder.tree.zoom_target(),
            _ => self.target(&focus, traversal),
        };
        if let Some(target) = target {
            self.holder.focus(&target);
            self.surface.damage();
        }
    }

    /// Where `traversal` goes from `focus` among the shown rows
    fn target(&self, focus: &WorldTreePath, traversal: Traversal) -> Option<WorldTreePath> {
        let rows = view_tree_rows(
            &self.holder.tree,
            self.view_path.as_ref(),
            self.holder.tree.active_screen(),
            false,
        );
        let from = rows
            .iter()
            .position(|row| row.path == focused_row(&rows, focus));
        traversal::target(&rows, from, traversal, &self.remembered)
    }

    /// 3 fingers: the view follows them between nodes, and settles on one when they lift (`swipe`, #141)
    fn handle_swipe(&mut self, phase: SwipePhase) {
        let settings = settings::get().swipe;
        match phase {
            SwipePhase::Begin => {
                // it carries on from where a settle it interrupts was
                let carried = self
                    .settle
                    .take()
                    .map(|settle| settle.carried(Instant::now()));
                self.swipe = Some(Swipe::new(carried));
            }
            SwipePhase::Update(delta) => {
                let Self {
                    swipe: Some(swipe),
                    holder,
                    view_path,
                    remembered,
                    ..
                } = self
                else {
                    return;
                };
                let view = swipe.update(delta, &settings, |vertical| {
                    swipe_strip(&holder.tree, view_path.as_ref(), remembered, vertical)
                });
                if let Some(view) = view {
                    self.holder.tree.set_swipe(Some(view));
                }
            }
            SwipePhase::End { cancelled } => {
                let Some(settle) = self
                    .swipe
                    .take()
                    .and_then(|swipe| swipe.end(cancelled, &settings))
                else {
                    // it never moved far enough (but may have stopped a settle)
                    self.holder.tree.set_swipe(None);
                    return;
                };
                self.start_settle(settle);
            }
        }
    }

    /// Ctrl+Super+arrow: a 3 finger swipe by one row (`forward`: right / down)
    fn swipe_key(&mut self, vertical: bool, forward: bool) {
        // it carries on from where a settle it interrupts was
        let offset = match self
            .settle
            .as_ref()
            .map(|settle| settle.carried(Instant::now()))
        {
            Some((carried_vertical, offset)) if carried_vertical == vertical => offset,
            _ => 0.0,
        };
        let Some(settle) = swipe_strip(
            &self.holder.tree,
            self.view_path.as_ref(),
            &self.remembered,
            vertical,
        )
        .and_then(|strip| Settle::step(strip, offset, forward, &settings::get().swipe)) else {
            return;
        };
        self.start_settle(settle);
    }

    /// Focuses where `settle` goes, and springs the view there a frame at a time
    fn start_settle(&mut self, settle: Settle) {
        if settle.target != settle.strip.start {
            self.holder.focus(&settle.strip.paths[settle.target]);
            self.surface.damage();
        }
        self.settle = Some(settle);
        if !std::mem::replace(&mut self.settling, true) {
            self.loop_handle
                .insert_source(Timer::immediate(), |_, &mut (), root: &mut Self| {
                    root.tick_settle()
                })
                .unwrap();
        }
    }

    /// A frame of the settle, until it arrives (or a swipe interrupts it)
    fn tick_settle(&mut self) -> TimeoutAction {
        let Some(settle) = &self.settle else {
            self.settling = false;
            return TimeoutAction::Drop;
        };
        let view = settle.view(Instant::now());
        let arrived = view.is_none();
        self.holder.tree.set_swipe(view);
        self.damage_if_tree_changed();
        if arrived {
            self.settle = None;
            self.settling = false;
            // unless the fingers are down again
            if self.swipe.is_none() {
                self.return_lent_screen();
            }
            return TimeoutAction::Drop;
        }
        TimeoutAction::ToDuration(Self::SETTLE_FRAME)
    }

    /// 4 fingers up select the focused row (out of its applet), down are Enter; once a swipe
    fn handle_select_swipe(&mut self, phase: SwipePhase) {
        match phase {
            SwipePhase::Begin => self.select_swipe = Some([0.0; 2]),
            SwipePhase::Update([x, y]) => {
                let Some(moved) = &mut self.select_swipe else {
                    return;
                };
                *moved = [moved[0] + x, moved[1] + y];
                let [x, y] = *moved;
                if x.hypot(y) < Self::SELECT_DISTANCE {
                    return;
                }
                self.select_swipe = None;
                if x.abs() > y.abs() {
                    return;
                }
                self.traverse(if y < 0.0 {
                    Traversal::Select
                } else {
                    Traversal::Enter
                });
            }
            SwipePhase::End { .. } => self.select_swipe = None,
        }
    }

    /// Remembers the child each row on the focus path has focus under (#142)
    fn remember_focus(&mut self) {
        let focus = self.holder.tree.get_focus_path();
        // `events.on("focus", ..)`
        if self.last_focus.as_ref() != Some(&focus) {
            self.last_focus = Some(focus.clone());
            sonamu_script::emit("focus", Vec::new());
        }
        for end in 1..=focus.0.len() {
            self.remembered.insert(
                WorldTreePath(focus.0[..end - 1].to_vec()),
                WorldTreePath(focus.0[..end].to_vec()),
            );
        }
    }

    /// Super+V: the focused row as the view root, or back to the whole tree
    fn toggle_view_root(&mut self) {
        let tree = &self.holder.tree;
        let path = if self.view_path.is_some()
            || tree.view_root_path(tree.active_screen()) != TreeNodePath::new_root()
        {
            WorldTreePath::root()
        } else {
            tree.get_focus_path()
        };
        self.set_view_root(&path);
    }

    /// Shows the row's subtree: the `NodeTree`'s view root for a node of it, else `view_path`
    fn set_view_root(&mut self, path: &WorldTreePath) {
        let (node_path, inner) = path.split_world();
        self.holder.tree.set_view_root(&node_path);
        self.view_path = inner.map(|_| path.clone());
        self.surface.damage();
    }

    /// Moves the view out until focus is inside it (eg focus went out of it), and out of gone rows
    fn update_view_path(&mut self) {
        let Some(mut view) = self.view_path.clone() else {
            return;
        };
        let focus = self.holder.tree.get_focus_path();
        let focus_moved = self.view_path_focus.as_ref() != Some(&focus);
        self.view_path_focus = Some(focus.clone());
        let treeview = self.holder.tree.get_treeview();
        while !((!focus_moved || focus.starts_with(&view)) && treeview.safe_get(&view).is_some()) {
            view.0.pop();
            if !view.is_inner() {
                self.view_path = None;
                self.holder.tree.set_view_root(&view.split_world().0);
                self.surface.damage();
                return;
            }
        }
        if self.view_path.as_ref() != Some(&view) {
            self.view_path = Some(view);
            self.surface.damage();
        }
    }

    fn damage_if_tree_changed(&mut self) {
        if self.holder.tree.take_damage() {
            self.surface.damage();
        }
    }

    /// The history view at `area`, framed like the treeview
    fn get_history(
        &(area, place): &([u32; 4], TreeviewPlace),
        pointer: &mut HistoryPointer,
    ) -> UITree {
        let settings = settings::get();
        let ([x, y, width, height], border) = treeview_frame(&settings, area, place);
        let inner = [width, height].map(|length| length.saturating_sub(2 * border) as f32);
        ui::frame(
            if pointer.keys {
                settings.border.focused
            } else {
                settings.border.treeview
            },
            if border > 0 { settings.radius } else { 0.0 },
            border as f32,
            Style::absolute([x as f32, y as f32], [width as f32, height as f32]),
            vec![pointer.view(&settings, inner)],
        )
    }

    /// The treeview at `area` with its pin button, framed like a pane with `always_frame` (a floating one always is)
    fn get_treeview(
        tree: &NodeTree,
        screen: usize,
        view_path: Option<&WorldTreePath>,
        treeview_pointer: &mut TreeviewPointer,
        (area, place): ([u32; 4], TreeviewPlace),
    ) -> UITree {
        let (rows, _) = view_rows(tree, view_path, screen);
        let settings = settings::get();
        let ([x, y, width, height], border) = treeview_frame(&settings, area, place);
        let inner_width = width.saturating_sub(2 * border) as f32;
        let treeview = treeview_pointer.render(
            &rows,
            [inner_width, height.saturating_sub(2 * border) as f32],
            &tree.screen_focus_path(screen),
            tree.screen_focus_inside(screen),
        );
        // the pin button is off: the status bar's arrow shows and hides the treeview (#150)
        // // the top row's close button's place
        // let size = close_width();
        // let left = inner_width - size + (size - font_size()) / 2.0;
        // let pin_button = Rows::new(0.0)
        //     .text(
        //         ui::text(
        //             if pinned { "●" } else { "○" },
        //             with_opacity(settings.palette.fg),
        //         ),
        //         left,
        //         left + size,
        //     )
        //     .with_pointer();
        ui::frame(
            settings.border.treeview,
            if border > 0 { settings.radius } else { 0.0 },
            border as f32,
            Style::absolute([x as f32, y as f32], [width as f32, height as f32]),
            vec![treeview],
        )
    }
}

/// The side view pinned at start: the saved one, else (a clean session) the tree
fn initial_pin(saved: Option<&Session>) -> Option<SideView> {
    saved.map_or(Some(SideView::Tree), Session::pinned)
}

/// The history view's take on a pointer event in the screen's coords (`area`: where it is, if shown)
fn history_mouse(
    settings: &Settings,
    area: Option<([u32; 4], TreeviewPlace)>,
    pointer: &mut HistoryPointer,
    mouse_event: &MouseEvent,
) -> HistoryResponse {
    let ([x, y, width, height], border) = area.map_or(([0; 4], 0), |(area, place)| {
        treeview_frame(settings, area, place)
    });
    let [x, y, width, height] = [x, y, width, height].map(f64::from);
    let [pointer_x, pointer_y] = mouse_event.position;
    let inside = area.is_some()
        && (x..x + width).contains(&pointer_x)
        && (y..y + height).contains(&pointer_y);
    let mut event = *mouse_event;
    event.position = [
        pointer_x - x - f64::from(border),
        pointer_y - y - f64::from(border),
    ];
    pointer.handle_mouse(
        &event,
        inside,
        [width, height].map(|length| (length - f64::from(2 * border)).max(0.0) as f32),
    )
}

/// Where the treeview and the history are drawn (`[x, y, width, height]`, frame included, and how); `None`
/// while hidden
struct SideAreas {
    tree: Option<([u32; 4], TreeviewPlace)>,
    history: Option<([u32; 4], TreeviewPlace)>,
}

/// The pinned column on a screen of `size` (`[x, y, width, height]`, frame included) and its side
fn column_area(settings: &Settings, size: DisplayContainerSize) -> ([u32; 4], TreeviewSide) {
    let side = settings.treeview.side;
    let column = size.width * settings.treeview.width.min(100) / 100;
    let column_x = match side {
        TreeviewSide::Left => 0,
        TreeviewSide::Right => size.width - column,
    };
    ([column_x, 0, column, size.height], side)
}

/// Where the treeview and the history are drawn on a screen of `size`. `shown`: the treeview is (pinned, or Alt
/// held after Alt+traversal). Shown without being pinned it floats per `treeview.floating`; over a pinned view,
/// `side` takes the pinned column's place and hides the history meanwhile.
fn side_areas(
    settings: &Settings,
    size: DisplayContainerSize,
    pinned: Option<SideView>,
    shown: bool,
) -> SideAreas {
    let (column, side) = column_area(settings, size);
    let column = (column, TreeviewPlace::Column(side));
    let tree = shown.then(|| {
        let takes_column = pinned == Some(SideView::Tree)
            || (pinned.is_some() && settings.treeview.floating == TreeviewFloating::Side);
        if takes_column {
            column
        } else {
            (floating_area(settings, size), TreeviewPlace::Floating)
        }
    });
    let history = (pinned == Some(SideView::History)
        && !matches!(tree, Some((_, TreeviewPlace::Column(_)))))
    .then_some(column);
    SideAreas { tree, history }
}

/// The floating treeview's area, per `treeview.floating`
fn floating_area(settings: &Settings, size: DisplayContainerSize) -> [u32; 4] {
    let DisplayContainerSize { width, height } = size;
    match settings.treeview.floating {
        TreeviewFloating::Center => {
            let [w, h] = settings.treeview.floating_size;
            let [w, h] = [width * w.min(100) / 100, height * h.min(100) / 100];
            [(width - w) / 2, (height - h) / 2, w, h]
        }
        // in the column's place, with the gap all around
        TreeviewFloating::Side => {
            let ([column_x, _, column, _], _) = column_area(settings, size);
            let (gap, _) = settings.gap_and_border_px();
            [
                column_x + gap,
                gap,
                column.saturating_sub(2 * gap),
                height.saturating_sub(2 * gap),
            ]
        }
    }
}

/// The applets' area's left edge and width on a screen of `size`: beside the column if a view is pinned, else
/// everything
fn applet_area(
    settings: &Settings,
    size: DisplayContainerSize,
    pinned: Option<SideView>,
) -> [u32; 2] {
    if pinned.is_none() {
        return [0, size.width];
    }
    match column_area(settings, size) {
        ([_, _, width, _], TreeviewSide::Left) => [width, size.width - width],
        ([_, _, width, _], TreeviewSide::Right) => [0, size.width - width],
    }
}

// the pin button is off (#150)
// /// Whether `[x, y]` (in the treeview's coords, inside its frame) is on its pin button, in its top right corner
// fn on_pin_button([x, y]: [f64; 2], inner_width: f64) -> bool {
//     let size = f64::from(close_width());
//     (inner_width - size..inner_width).contains(&x) && (0.0..size).contains(&y)
// }

/// The treeview's frame (`[x, y, width, height]`, border included) in its `area`, and its border width.
/// Framed like a pane with `always_frame`: in a column the panes' gap is on the side facing them, so it gets the
/// other gaps. A floating one is always framed, as it is over the panes.
fn treeview_frame(
    settings: &Settings,
    [x, y, width, height]: [u32; 4],
    place: TreeviewPlace,
) -> ([u32; 4], u32) {
    let (gap, border) = settings.gap_and_border_px();
    match place {
        TreeviewPlace::Floating => ([x, y, width, height], border.max(1)),
        _ if !settings.always_frame => ([x, y, width, height], 0),
        TreeviewPlace::Column(TreeviewSide::Left) => (
            [
                x + gap,
                y + gap,
                width.saturating_sub(gap),
                height.saturating_sub(2 * gap),
            ],
            border,
        ),
        TreeviewPlace::Column(TreeviewSide::Right) => (
            [
                x,
                y + gap,
                width.saturating_sub(gap),
                height.saturating_sub(2 * gap),
            ],
            border,
        ),
    }
}

/// The treeview's rows: the view root's subtree (`view_path`, else the `NodeTree`'s),
/// under a `‹` row for its parent with `up`.
/// A line of text framed in the middle of `size` (eg asking to confirm quitting)
fn notice(settings: &Settings, text: &str, size: DisplayContainerSize) -> UITree {
    let pad = font_size();
    // monospace: about this wide a character
    let width = text.chars().count() as f32 * font_size() * 0.6 + 2.0 * pad;
    // room for descenders
    let line = font_size() * 1.4;
    let height = line + 2.0 * pad;
    let top = (size.height as f32 - height) / 2.0;
    let left = (size.width as f32 - width) / 2.0;
    ui::frame(
        settings.palette.accent,
        settings.radius,
        settings.border.width,
        Style::absolute([left, top], [width, height]),
        vec![UITree::new_styled(
            UIElement::Backgrounded(ui::bg(&settings.palette)),
            Style::full_size(),
            vec![UITree::leaf_styled(
                ui::text(text, settings.palette.fg),
                Style::absolute([pad, pad], [width - 2.0 * pad, line]),
            )],
        )],
    )
}

/// The view's rows, in their colors (`NodeTree::treeview_colors`)
fn view_tree_rows(
    tree: &NodeTree,
    view_path: Option<&WorldTreePath>,
    screen: usize,
    up: bool,
) -> Vec<TreeviewRow> {
    let mut rows = uncolored_rows(tree, view_path, screen, up);
    let colors: HashMap<_, _> = tree.treeview_colors().into_iter().collect();
    for row in &mut rows {
        row.color = colors.get(&row.path).copied();
    }
    rows
}

fn uncolored_rows(
    tree: &NodeTree,
    view_path: Option<&WorldTreePath>,
    screen: usize,
    up: bool,
) -> Vec<TreeviewRow> {
    if let Some(view) = view_path {
        let treeview = tree.get_treeview();
        if let Some(node) = treeview.safe_get(view) {
            let mut parent = view.clone();
            parent.0.pop();
            let up = up
                .then(|| treeview.safe_get(&parent))
                .flatten()
                .map(|parent_node| (parent, parent_node.get_value().get_root_value().clone()));
            return treeview_rows(&WorldTree::World(Box::new(node.clone())), view, up);
        }
    }
    let base = WorldTreePath::from_tree_path(&tree.view_root_path(screen));
    let up = up
        .then(|| tree.view_parent(screen))
        .flatten()
        .map(|(path, label)| (WorldTreePath::from_tree_path(&path), label));
    treeview_rows(&tree.get_view_treeview(screen), &base, up)
}

/// The treeview's rows, and where its menu adds things when opened on no row (the view root)
/// The screen's treeview's take on a pointer event in the screen's coords (`area`: where the treeview is, if shown)
fn treeview_mouse(
    tree: &NodeTree,
    settings: &Settings,
    area: Option<([u32; 4], TreeviewPlace)>,
    (screen, view_path): (usize, Option<&WorldTreePath>),
    pointer: &mut TreeviewPointer,
    mouse_event: &MouseEvent,
) -> TreeviewResponse {
    // a hidden treeview has no rows to hit
    let (rows, general) = if area.is_some() {
        view_rows(tree, view_path, screen)
    } else {
        (Vec::new(), WorldTreePath::root())
    };
    // in the treeview's coords, inside its frame
    let ([x, y, width, height], border) = area.map_or(([0; 4], 0), |(area, place)| {
        treeview_frame(settings, area, place)
    });
    let [x, y, width, height] = [x, y, width, height].map(f64::from);
    let [pointer_x, pointer_y] = mouse_event.position;
    let in_treeview = area.is_some()
        && (x..x + width).contains(&pointer_x)
        && (y..y + height).contains(&pointer_y);
    let mut treeview_event = *mouse_event;
    treeview_event.position = [
        pointer_x - x - f64::from(border),
        pointer_y - y - f64::from(border),
    ];
    // the pin button is off (#150)
    // let inner_width = (width - f64::from(2 * border)).max(0.0);
    // if in_treeview
    //     && !self.treeview_pointer.is_pressed()
    //     && on_pin_button(treeview_event.position, inner_width)
    //     && matches!(
    //         mouse_event.kind,
    //         MouseEventKind::Button {
    //             button: BTN_LEFT,
    //             pressed: true
    //         }
    //     )
    // {
    //     self.treeview_pinned = !self.treeview_pinned;
    //     self.surface.damage();
    //     self.update_treeview_visibility();
    //     return;
    // }
    pointer.handle_mouse(
        &treeview_event,
        in_treeview,
        &rows,
        &general,
        [width, height].map(|length| (length - f64::from(2 * border)).max(0.0) as f32),
    )
}

fn view_rows(
    tree: &NodeTree,
    view_path: Option<&WorldTreePath>,
    screen: usize,
) -> (Vec<TreeviewRow>, WorldTreePath) {
    let general = view_path
        .cloned()
        .unwrap_or_else(|| WorldTreePath::from_tree_path(&tree.view_root_path(screen)));
    (view_tree_rows(tree, view_path, screen, true), general)
}

/// The rows a 3 finger swipe (or Ctrl+Super+arrow) moves along from focus: parent and child (`vertical`), else
/// siblings; `None` if focus isn't shown
fn swipe_strip(
    tree: &NodeTree,
    view_path: Option<&WorldTreePath>,
    remembered: &HashMap<WorldTreePath, WorldTreePath>,
    vertical: bool,
) -> Option<Strip> {
    let rows = view_tree_rows(tree, view_path, tree.active_screen(), false);
    let focus = focused_row(&rows, &tree.get_focus_path());
    let from = rows.iter().position(|row| row.path == focus)?;
    Some(if vertical {
        Strip::lineage(&rows, from, remembered)
    } else {
        let mut strip = Strip::siblings(&rows, from);
        let mut parent = focus;
        parent.0.pop();
        strip.reversed = tree.shown_at_once().contains(&parent);
        strip
    })
}

/// The root's own keybinds
#[derive(Clone, Copy)]
enum RootKeybind {
    Help,
    /// Toggles the focused node as the view root
    ViewRoot,
    /// The focus's parent as the view root
    ViewParent,
    /// Restores the newest closed node (#162)
    RestoreClosed,
    /// Toggles the history as the pinned side view (#162)
    History,
    /// Like a 3 finger swipe, by one row
    Swipe {
        vertical: bool,
        forward: bool,
    },
}
impl RootKeybind {
    /// From the keybinds (setting `keybinds`): `help`, `view_root`, `view_parent`, `restore_closed`, `history`, `swipe` and a direction
    fn of(ui_event: &UIEvent) -> Option<Self> {
        let &UIEvent::Key { modifiers, .. } = ui_event else {
            return None;
        };
        let (command, args) = settings::keybind_command(ui_event, modifiers)?;
        Some(match command.as_str() {
            "help" => Self::Help,
            "view_root" => Self::ViewRoot,
            "view_parent" => Self::ViewParent,
            "restore_closed" => Self::RestoreClosed,
            "history" => Self::History,
            "swipe" => {
                let (vertical, forward) = match args.first()?.as_str()? {
                    "left" => (false, false),
                    "right" => (false, true),
                    "up" => (true, false),
                    "down" => (true, true),
                    _ => return None,
                };
                Self::Swipe { vertical, forward }
            }
            _ => return None,
        })
    }
}

/// Whether it's the `buffer_mode` keybind (setting `keybinds`), which toggles the buffer line
fn is_buffer_mode_key(ui_event: &UIEvent) -> bool {
    let &UIEvent::Key { modifiers, .. } = ui_event else {
        return false;
    };
    settings::keybind_command(ui_event, modifiers)
        .is_some_and(|(command, _)| command == "buffer_mode")
}

/// Whether the root or a node acts on this key, rather than a focused wl client.
/// The runner sends other keys straight to the client (#56).
pub fn is_keybind(ui_event: &UIEvent) -> bool {
    sonamu_common::capturing()
        || sonamu_common::BUFFER_MODE.load(Ordering::Relaxed)
        || sonamu_common::HISTORY_KEYS.load(Ordering::Relaxed)
        || is_buffer_mode_key(ui_event)
        || RootKeybind::of(ui_event).is_some()
        || standard_keybind_request(ui_event).is_some()
        || Traversal::of(ui_event, false).is_some()
}

/// Rows the buffer line takes below the panes: one when it always shows, else none (it shows over them)
fn buffer_line_height(settings: &Settings) -> u32 {
    match settings.buffer_line {
        BufferLine::Always => CharGrid::new_empty(1, 1).display_size().height,
        BufferLine::BufferMode => 0,
    }
}

/// A screen's (`[x, y, width, height]`, under the status bar: the runner keeps its row free) room for the treeview
/// and panes, above the buffer line
fn content_size(settings: &Settings, [_, _, width, height]: [u32; 4]) -> DisplayContainerSize {
    DisplayContainerSize::new(width, height.saturating_sub(buffer_line_height(settings)))
}

/// The buffer line's bottom row
#[derive(Clone, Copy, PartialEq)]
enum Bar {
    /// In buffer mode: the prompt, under the last entry's output and Tab's candidates, scrolled up this many lines
    /// through the whole history (clamped; `buffer_window`)
    Prompt(usize),
    /// `-- INSERT --` (setting `buffer_line: always`)
    Insert,
    /// Only the echoed message
    None,
}

/// The echoed message's lines
fn echo_lines(
    echo: Option<&sonamu_script::Message>,
    columns: usize,
    palette: &Palette,
) -> Vec<repl::Line> {
    echo.map(|message| {
        let fg = if message.error { ERROR } else { palette.fg };
        repl::text_lines(&message.text, fg, columns, palette)
    })
    .unwrap_or_default()
}

/// The buffer line's lines in buffer mode, the whole history then the echoed message; and how many of them from the
/// last entry on (shown unscrolled)
fn buffer_lines(
    repl: &Repl,
    echo: Option<&sonamu_script::Message>,
    columns: usize,
    palette: &Palette,
) -> (Vec<repl::Line>, usize) {
    let echo = echo_lines(echo, columns, palette);
    let last = repl
        .lines(columns, repl.history.len().saturating_sub(1), palette)
        .len()
        + echo.len();
    let mut lines = repl.lines(columns, 0, palette);
    lines.extend(echo);
    (lines, last)
}

/// The most `buffer_window` scrolls up through `count` lines, the `last` of them shown unscrolled
fn max_buffer_scroll(count: usize, last: usize) -> usize {
    count - last.min(BUFFER_OUTPUT_ROWS)
}

/// Which of `count` lines the buffer line shows, scrolled up `scroll` lines from the `last` ones: first it grows
/// upward to `BUFFER_OUTPUT_ROWS`, then it moves up, so it never jumps
fn buffer_window(count: usize, last: usize, scroll: usize) -> std::ops::Range<usize> {
    let scroll = scroll.min(max_buffer_scroll(count, last));
    let rows = (last + scroll).min(BUFFER_OUTPUT_ROWS);
    let end = count - scroll.saturating_sub(BUFFER_OUTPUT_ROWS - last.min(BUFFER_OUTPUT_ROWS));
    end - rows..end
}

/// The buffer line at the bottom of a screen of `size`: `bar`, under the echoed message; and its height (px)
fn buffer_line(
    repl: &Repl,
    echo: Option<&sonamu_script::Message>,
    size: DisplayContainerSize,
    bar: Bar,
    palette: &Palette,
) -> (UITree, f32) {
    let (columns, _) = CharGrid::largest_fittable_size(size);
    let (lines, last, scroll) = match bar {
        Bar::Prompt(scroll) => {
            let (lines, last) = buffer_lines(repl, echo, columns, palette);
            (lines, last, scroll)
        }
        Bar::Insert | Bar::None => {
            let lines = echo_lines(echo, columns, palette);
            let last = lines.len();
            (lines, last, 0)
        }
    };
    let shown = &lines[buffer_window(lines.len(), last, scroll)];
    let bar_rows = usize::from(bar != Bar::None);
    let mut grid = CharGrid::new_empty(columns, shown.len() + bar_rows);
    repl::draw_lines(&mut grid, 0, shown, palette.accent);
    let prompt_row = shown.len();
    match bar {
        Bar::Prompt(_) => {
            ui::fill_row(&mut grid, prompt_row, ui::raised_bg(palette));
            repl.draw_prompt(&mut grid, prompt_row, palette);
        }
        Bar::Insert => {
            ui::fill_row(&mut grid, prompt_row, ui::raised_bg(palette));
            ui::put(&mut grid, prompt_row, 1, "-- INSERT --", palette.accent);
        }
        Bar::None => {}
    }
    let height = grid.display_size().height as f32;
    let top = size.height as f32 - height;
    let mut children = vec![
        // the grid is whole cells; the bar spans the screen
        ui::rect(
            ui::raised_bg(palette),
            [0.0, 0.0],
            [size.width as f32, height],
        ),
        grid.element().into(),
    ];
    // only through the whole history: unscrolled, it shows just the last entry
    if scroll > 0 {
        children.extend(ui::grid_scrollbar(
            palette,
            columns,
            0,
            buffer_window(lines.len(), last, scroll).start,
            shown.len(),
            lines.len(),
        ));
    }
    let line = UITree::new_container_styled(
        Style::absolute([0.0, top], [size.width as f32, height]),
        children,
    );
    (line, height)
}

/// The saved tree, with focus from before monitors had names; else each monitor's comes once they're known
/// (`set_screens`)
fn restore_tree(
    saved: &Session,
    spawn_client: SpawnClient,
    attach_client: AttachClient,
    world: World,
) -> NodeTree {
    saved.tree.reserve_ids();
    let mut tree = NodeTree::restore(saved.tree.clone(), spawn_client, attach_client, world);
    if saved.monitor_focus.is_empty() {
        match saved.screen_focus.first() {
            Some(chain) => tree.focus_chain(0, chain),
            None => tree.focus_path(&saved.focus),
        }
    }
    tree
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodular_applet::{
        AppletContext, AppletSpawner, AppletSpawnerTrait, NodularAppletInitializer, StandardApplet,
    };
    use calloop::EventLoop;
    use sonamu_common::sap::session::{SavedApplet, SavedTree};
    use sonamu_ui::ui_element::PrimitiveScene;

    #[test]
    fn capture_takes_every_key() {
        let key = UIEvent::Key {
            symbol: Some(KeySymbol::Char('a')),
            modifiers: KeyModifiers::NONE,
            raw_keycode: 30,
            pressed: true,
        };
        assert!(!is_keybind(&key));
        let capture = sonamu_common::Capture::start();
        assert!(is_keybind(&key));
        drop(capture);

        // focus moving away (eg a click on a terminal) ends it, so keys reach the new focus
        let mut event_loop = EventLoop::<RootNodeApplet>::try_new().unwrap();
        let (world, _calls) = calloop::channel::channel();
        let (request_sender, _requests) = calloop::channel::channel();
        let surface = SharedState::new(StateKind::Surface, UITree::leaf(UIElement::Nothing));
        let mut root = RootNodeApplet::new(
            ProjectApplet::restorer(container(
                Some("p"),
                vec![tab("0", Vec::new()), tab("1", Vec::new())],
            )),
            BTreeMap::from([("tab".to_string(), Box::new(TabSpawner) as AppletSpawner)]),
            None,
            surface,
            request_sender,
            &event_loop.handle(),
            world,
        );
        pump(&mut event_loop, &mut root, 100);
        let rows = view_tree_rows(&root.holder.tree, None, 0, false);
        let paths: Vec<_> = rows.iter().map(|row| row.path.clone()).collect();
        send(
            &mut event_loop,
            &mut root,
            StandardEvent::Focus(paths[1].clone()),
        );
        let capture = sonamu_common::Capture::start();
        send(&mut event_loop, &mut root, StandardEvent::UIEvent(key));
        assert!(sonamu_common::capturing(), "same focus keeps it");
        send(
            &mut event_loop,
            &mut root,
            StandardEvent::Focus(paths[2].clone()),
        );
        send(&mut event_loop, &mut root, StandardEvent::UIEvent(key));
        assert!(!sonamu_common::capturing(), "focus moved");
        drop(capture);
    }

    /// A tab that saves itself, like a code editor
    struct Tab {
        name: String,
        /// Its last two sizes, and how many times in a row it went back to the one before
        sizes: [Option<DisplayContainerSize>; 2],
        flips: u32,
    }
    /// Tabs that got a pointer event, by name
    static POINTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

    impl StandardApplet for Tab {
        /// A wl client sized like this crashes or is flooded with configures and dropped, closing its tab
        fn handle_standard_event(&mut self, event: StandardEvent) {
            if let StandardEvent::UIEvent(UIEvent::Mouse(_)) = event {
                POINTED.lock().unwrap().push(self.name.clone());
            }
            if let StandardEvent::UIEvent(UIEvent::WindowResized(size)) = event {
                // no bigger than a screen, nor collapsed
                assert!(
                    (20..=2560).contains(&size.width) && (20..=1600).contains(&size.height),
                    "{} resized to {size:?}",
                    self.name
                );
                // switching screens back and forth flips it too, but not this often in a row
                self.flips = if self.sizes[0] == Some(size) {
                    self.flips + 1
                } else {
                    0
                };
                assert!(
                    self.flips < 8,
                    "{} resized back and forth: {:?}",
                    self.name,
                    self.sizes
                );
                self.sizes = [self.sizes[1], Some(size)];
            }
        }

        fn get_window_standard_applet(&self, _size: DisplayContainerSize) -> UITree {
            ui::text(self.name.clone(), settings::get().palette.inactive).into()
        }

        fn get_treeview(&self) -> WorldTree<String> {
            WorldTree::new_base(self.name.clone())
        }

        fn save(&self) -> Option<SavedApplet> {
            Some(SavedApplet::Spawner {
                name: "tab".to_string(),
                args: vec![self.name.clone()],
                inner: None,
            })
        }
    }

    struct TabSpawner;
    impl AppletSpawnerTrait for TabSpawner {
        fn create_initializer(&self, args: &[&str]) -> Option<NodularAppletInitializer> {
            let name = (*args.first()?).to_string();
            Some(NodularAppletInitializer::new(move |_: AppletContext| {
                Box::new(Tab {
                    name,
                    sizes: [None; 2],
                    flips: 0,
                }) as Box<dyn StandardApplet>
            }))
        }

        fn duplicate(&self) -> AppletSpawner {
            Box::new(Self)
        }
    }

    fn tab(name: &str, children: Vec<SavedTree>) -> SavedTree {
        SavedTree {
            main: Some(SavedApplet::Spawner {
                name: "tab".to_string(),
                args: vec![name.to_string()],
                inner: None,
            }),
            children,
            ..SavedTree::default()
        }
    }

    fn container(label: Option<&str>, children: Vec<SavedTree>) -> SavedTree {
        SavedTree {
            main: None,
            children,
            label: label.map(str::to_string),
            ..SavedTree::default()
        }
    }

    fn project(name: &str, children: Vec<SavedTree>) -> SavedTree {
        SavedTree {
            main: Some(SavedApplet::Spawner {
                name: ProjectApplet::SPAWNER.to_string(),
                args: Vec::new(),
                inner: Some(Box::new(container(Some(name), children))),
            }),
            ..container(None, Vec::new())
        }
    }

    /// Random children, like a real session: projects nested deep, tabs with layers of children, half width
    /// ones, containers; tabs are numbered from `tabs`
    fn random_children(rng: &mut Rng, depth: u32, tabs: &mut usize) -> Vec<SavedTree> {
        let count = if depth == 0 { 0 } else { 1 + rng.below(3) };
        (0..count)
            .map(|_| {
                let children = random_children(rng, depth - 1, tabs);
                let mut tree = match rng.below(6) {
                    0 => project(&format!("p{}", rng.below(100)), children),
                    1 => container(None, children),
                    _ => {
                        *tabs += 1;
                        tab(&(*tabs - 1).to_string(), children)
                    }
                };
                tree.half = rng.below(4) == 0;
                tree
            })
            .collect()
    }

    /// How many times each tab is in the saved tree (running or not, also inside projects)
    fn tab_counts(tree: &SavedTree, counts: &mut [usize]) {
        match &tree.main {
            Some(SavedApplet::Spawner { name, args, .. }) if name == "tab" => {
                counts[args[0].parse::<usize>().unwrap()] += 1;
            }
            Some(SavedApplet::Spawner {
                inner: Some(inner), ..
            }) => tab_counts(inner, counts),
            _ => {}
        }
        for child in &tree.children {
            tab_counts(child, counts);
        }
    }

    /// xorshift, so a failing seed can be run again
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }

        fn px(&mut self) -> f64 {
            self.below(240) as f64 - 120.0
        }
    }

    /// Two screens of different sizes, side by side
    fn screens(rng: &mut Rng) -> Vec<ScreenArea> {
        let mut size = || [800 + rng.below(1760) as u32, 600 + rng.below(1000) as u32];
        let [[width, height], [width2, height2]] = [size(), size()];
        named(&[
            ("A", [0, 0, width, height]),
            ("B", [width, 0, width2, height2]),
        ])
    }

    fn named(screens: &[(&str, [u32; 4])]) -> Vec<ScreenArea> {
        screens
            .iter()
            .map(|&(name, rect)| ScreenArea {
                name: name.to_string(),
                rect,
            })
            .collect()
    }

    /// Runs the root's loop (and its applets) for `ms`, as the runner's thread would
    fn pump(
        event_loop: &mut EventLoop<'static, RootNodeApplet>,
        root: &mut RootNodeApplet,
        ms: u64,
    ) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            event_loop
                .dispatch(Some(Duration::from_millis(1)), root)
                .unwrap();
            root.after_batch();
        }
    }

    fn send(
        event_loop: &mut EventLoop<'static, RootNodeApplet>,
        root: &mut RootNodeApplet,
        event: StandardEvent,
    ) {
        root.handle_event(event);
        pump(event_loop, root, 3);
    }

    fn swipe(fingers: u32, phase: SwipePhase) -> StandardEvent {
        StandardEvent::UIEvent(UIEvent::Swipe { fingers, phase })
    }

    /// A swipe with `fingers` through `deltas`
    fn swipe_through(
        event_loop: &mut EventLoop<'static, RootNodeApplet>,
        root: &mut RootNodeApplet,
        fingers: u32,
        deltas: impl IntoIterator<Item = [f64; 2]>,
        cancelled: bool,
    ) {
        send(event_loop, root, swipe(fingers, SwipePhase::Begin));
        for delta in deltas {
            send(event_loop, root, swipe(fingers, SwipePhase::Update(delta)));
        }
        send(
            event_loop,
            root,
            swipe(fingers, SwipePhase::End { cancelled }),
        );
    }

    /// One random step of `swipes_keep_every_tab`, described in `log` first
    fn random_action(
        rng: &mut Rng,
        event_loop: &mut EventLoop<'static, RootNodeApplet>,
        root: &mut RootNodeApplet,
        log: &mut Vec<String>,
    ) {
        match rng.below(10) {
            // like clicking a row of the treeview, to get deep quickly
            8 | 9 => {
                let rows = view_tree_rows(
                    &root.holder.tree,
                    None,
                    root.holder.tree.active_screen(),
                    false,
                );
                let path = rows[rng.below(rows.len() as u64) as usize].path.clone();
                log.push(format!("focus {:?}", path.0));
                send(event_loop, root, StandardEvent::Focus(path));
            }
            0 => {
                let screen = rng.below(2) as usize;
                log.push(format!("activate {screen}"));
                send(event_loop, root, StandardEvent::ActivateScreen(screen));
            }
            1 => {
                let down = rng.below(2) == 0;
                log.push(format!("4 fingers {}", if down { "down" } else { "up" }));
                let y = if down { 80.0 } else { -80.0 };
                swipe_through(event_loop, root, 4, [[0.0, y]], false);
            }
            // what lost tabs: switching screens, then from a child to its parent
            2 | 3 => {
                let screen = 1 - root.holder.tree.active_screen();
                let updates = 1 + rng.below(6);
                log.push(format!(
                    "activate {screen}, 3 fingers to parent, {updates} updates"
                ));
                send(event_loop, root, StandardEvent::ActivateScreen(screen));
                let deltas: Vec<_> = (0..updates)
                    .map(|_| [rng.px() / 8.0, rng.px() + 180.0])
                    .collect();
                swipe_through(event_loop, root, 3, deltas, false);
            }
            _ => {
                let vertical = rng.below(2) == 0;
                let updates = 1 + rng.below(8);
                let cancelled = rng.below(10) == 0;
                let sign = if rng.below(2) == 0 { 1.0 } else { -1.0 };
                log.push(format!(
                    "3 fingers vertical {vertical}, {updates} updates, cancelled {cancelled}"
                ));
                let deltas: Vec<_> = (0..updates)
                    .map(|_| {
                        // mostly one way, as fingers do
                        let main = sign * (rng.px() + 60.0);
                        let cross = rng.px() / 8.0;
                        if vertical {
                            [cross, main]
                        } else {
                            [main, cross]
                        }
                    })
                    .collect();
                swipe_through(event_loop, root, 3, deltas, cancelled);
            }
        }
    }

    /// Random swipes, screen switches and focus jumps on two screens never lose a tab: no applet panics, and no
    /// tab is sized past a screen or flooded with resizes (a wl client is dropped when it can't keep up)
    #[test]
    fn buffer_window_grows_then_moves() {
        // a 3-line last entry under 30 lines: it grows up to 10 rows, then moves a line a step
        assert_eq!(buffer_window(30, 3, 0), 27..30);
        assert_eq!(buffer_window(30, 3, 1), 26..30);
        assert_eq!(buffer_window(30, 3, 7), 20..30);
        assert_eq!(buffer_window(30, 3, 8), 19..29);
        assert_eq!(buffer_window(30, 3, 99), 0..10);
        // a long last entry already fills it
        assert_eq!(buffer_window(30, 20, 0), 20..30);
        assert_eq!(buffer_window(30, 20, 1), 19..29);
        // all of it fits
        assert_eq!(buffer_window(5, 2, 99), 0..5);
        assert_eq!(buffer_window(0, 0, 0), 0..0);
    }

    #[test]
    fn wheel_scrolls_the_buffer_line() {
        let mut event_loop = EventLoop::<RootNodeApplet>::try_new().unwrap();
        let (world, _calls) = calloop::channel::channel();
        let (request_sender, _requests) = calloop::channel::channel();
        let surface = SharedState::new(StateKind::Surface, UITree::leaf(UIElement::Nothing));
        let mut root = RootNodeApplet::new(
            ProjectApplet::restorer(container(Some("p"), vec![tab("0", Vec::new())])),
            BTreeMap::from([("tab".to_string(), Box::new(TabSpawner) as AppletSpawner)]),
            None,
            surface,
            request_sender,
            &event_loop.handle(),
            world,
        );
        root.repl.history.push(repl::Entry {
            kind: repl::Kind::Luau,
            command: "!x".to_string(),
            printed: String::new(),
            output: repl::Output::Text("line\n".repeat(30)),
            messages: Vec::new(),
        });
        root.buffer_mode = true;
        root.buffer_line_height.set(100.0);
        let scroll = |y| {
            StandardEvent::UIEvent(UIEvent::Mouse(MouseEvent {
                position: [10.0, y],
                kind: MouseEventKind::Scroll {
                    delta: [0.0, -15.0],
                    v120: Some([0, -360]),
                    stop: [false; 2],
                    ctrl: false,
                    logo: false,
                },
                time_msec: None,
            }))
        };
        send(&mut event_loop, &mut root, scroll(590.0));
        assert_eq!(root.buffer_scroll, 3, "three notches up, over it");
        send(&mut event_loop, &mut root, scroll(100.0));
        assert_eq!(root.buffer_scroll, 3, "not over it");
    }

    #[test]
    fn swipes_keep_every_tab() {
        // an applet's thread panicking (eg a project's) freezes it, its tabs gone from the screen
        let panicked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hook = std::panic::take_hook();
        std::panic::set_hook({
            let panicked = Arc::clone(&panicked);
            Box::new(move |info| {
                panicked.store(true, Ordering::Relaxed);
                hook(info);
            })
        });
        for seed in 1..=3_u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut tabs = 0;
            let children = random_children(&mut rng, 6, &mut tabs);
            let mut event_loop = EventLoop::<RootNodeApplet>::try_new().unwrap();
            let (world, _calls) = calloop::channel::channel();
            let (request_sender, _requests) = calloop::channel::channel();
            let surface = SharedState::new(StateKind::Surface, UITree::leaf(UIElement::Nothing));
            let mut root = RootNodeApplet::new(
                ProjectApplet::restorer(container(Some("p"), children)),
                BTreeMap::from([("tab".to_string(), Box::new(TabSpawner) as AppletSpawner)]),
                None,
                surface.clone(),
                request_sender,
                &event_loop.handle(),
                world,
            );
            send(
                &mut event_loop,
                &mut root,
                StandardEvent::Screens(screens(&mut rng)),
            );
            pump(&mut event_loop, &mut root, 100);
            let mut counts = vec![0; tabs];
            tab_counts(&root.holder.tree.save(), &mut counts);
            assert_eq!(counts, vec![1; tabs], "seed {seed}: before swiping");
            let mut log = Vec::new();
            for _ in 0..40 {
                random_action(&mut rng, &mut event_loop, &mut root, &mut log);
                // sometimes let the settle finish, else interrupt it
                pump(&mut event_loop, &mut root, rng.below(2) * 100);
                // as the runner draws it
                PrimitiveScene::from_ui_tree(
                    surface.get(),
                    DisplayContainerSize::new(5120, 1600),
                    |_| UITree::leaf(UIElement::Nothing),
                );
                assert!(
                    !panicked.load(Ordering::Relaxed),
                    "seed {seed}: an applet panicked after\n{}",
                    log.join("\n")
                );
            }
            pump(&mut event_loop, &mut root, 200);
            assert!(
                !panicked.load(Ordering::Relaxed),
                "seed {seed}: an applet panicked after\n{}",
                log.join("\n")
            );
            let mut counts = vec![0; tabs];
            tab_counts(&root.holder.tree.save(), &mut counts);
            assert_eq!(
                counts,
                vec![1; tabs],
                "seed {seed}: tabs (by number) after\n{}",
                log.join("\n")
            );
        }
    }

    /// Each screen's focus, also inside a project, comes back on the next start (#158)
    #[test]
    fn session_keeps_each_screens_focus() {
        let session = std::env::temp_dir().join(format!(
            "sonamu-test-screen-focus-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&session);
        let start = || {
            let event_loop = EventLoop::<RootNodeApplet>::try_new().unwrap();
            let (world, calls) = calloop::channel::channel();
            let (request_sender, requests) = calloop::channel::channel();
            let root = RootNodeApplet::new(
                ProjectApplet::restorer(container(
                    Some("p"),
                    vec![
                        project("a", vec![tab("1", Vec::new()), tab("2", Vec::new())]),
                        tab("0", Vec::new()),
                    ],
                )),
                BTreeMap::from([("tab".to_string(), Box::new(TabSpawner) as AppletSpawner)]),
                Some(session.clone()),
                SharedState::new(StateKind::Surface, UITree::leaf(UIElement::Nothing)),
                request_sender,
                &event_loop.handle(),
                world,
            );
            // kept, so sending to them doesn't fail
            (event_loop, root, (calls, requests))
        };
        let (a, b) = (("A", [0_u32, 0, 800, 600]), ("B", [800_u32, 0, 1000, 700]));
        let screens = || StandardEvent::Screens(named(&[a, b]));
        let inner =
            |steps: &[usize]| WorldTreePath(steps.iter().map(|&i| Step::Inner(i)).collect());
        let focus = |root: &RootNodeApplet| {
            (0..2)
                .map(|screen| {
                    let tree = &root.holder.tree;
                    (
                        tree.screen_focus_path(screen),
                        tree.screen_focus_uids(screen),
                    )
                })
                .collect::<Vec<_>>()
        };

        let (mut event_loop, mut root, _channels) = start();
        send(&mut event_loop, &mut root, screens());
        // screen 0: tab 2, in project a (woken first); screen 1: tab 0
        for path in [inner(&[0]), inner(&[0, 1])] {
            send(&mut event_loop, &mut root, StandardEvent::Focus(path));
            pump(&mut event_loop, &mut root, 100);
        }
        send(&mut event_loop, &mut root, StandardEvent::ActivateScreen(1));
        send(
            &mut event_loop,
            &mut root,
            StandardEvent::Focus(inner(&[1])),
        );
        send(&mut event_loop, &mut root, StandardEvent::ActivateScreen(0));
        pump(&mut event_loop, &mut root, 100);
        let before = focus(&root);
        assert_eq!(before[0].0, inner(&[0, 1]));
        assert_eq!(before[1].0, inner(&[1]));
        root.save_session();

        let (mut event_loop, mut root, _channels) = start();
        send(&mut event_loop, &mut root, screens());
        pump(&mut event_loop, &mut root, 300);
        assert_eq!(focus(&root), before);

        // tab 2 is gone: its project gets focus
        let mut saved = session::load(&session).unwrap();
        let Some(SavedApplet::Spawner {
            inner: Some(root_inner),
            ..
        }) = &mut saved.tree.main
        else {
            panic!("the root's project isn't saved: {saved:?}");
        };
        let Some(SavedApplet::Spawner {
            inner: Some(project_a),
            ..
        }) = &mut root_inner.children[0].main
        else {
            panic!("project a isn't saved: {saved:?}");
        };
        project_a.children.pop();
        session::write(&session, &saved).unwrap();
        let (mut event_loop, mut root, _channels) = start();
        send(&mut event_loop, &mut root, screens());
        pump(&mut event_loop, &mut root, 300);
        let after = focus(&root);
        assert_eq!(after[0].0, inner(&[0]));
        assert_eq!(after[1], before[1]);

        // the monitors swapped screens: focus follows them
        let (mut event_loop, mut root, _channels) = start();
        send(
            &mut event_loop,
            &mut root,
            StandardEvent::Screens(named(&[b, a])),
        );
        pump(&mut event_loop, &mut root, 300);
        let _ = std::fs::remove_file(&session);
        let swapped = focus(&root);
        assert_eq!(swapped[0], after[1]);
        assert_eq!(swapped[1].0, after[0].0);
    }

    /// The pointer over an inactive screen reaches what it shows there, inside a project too, without making it
    /// active; a press does (#158)
    #[test]
    fn pointer_acts_on_inactive_screen() {
        let mut event_loop = EventLoop::<RootNodeApplet>::try_new().unwrap();
        let (world, _calls) = calloop::channel::channel();
        let (request_sender, _requests) = calloop::channel::channel();
        let mut root = RootNodeApplet::new(
            ProjectApplet::restorer(container(
                Some("p"),
                vec![tab("pointed 0", Vec::new()), tab("pointed 1", Vec::new())],
            )),
            BTreeMap::from([("tab".to_string(), Box::new(TabSpawner) as AppletSpawner)]),
            None,
            SharedState::new(StateKind::Surface, UITree::leaf(UIElement::Nothing)),
            request_sender,
            &event_loop.handle(),
            world,
        );
        let screens = named(&[("A", [0, 0, 800, 600]), ("B", [800, 0, 800, 600])]);
        send(&mut event_loop, &mut root, StandardEvent::Screens(screens));
        let inner = |i| WorldTreePath(vec![Step::Inner(i)]);
        send(&mut event_loop, &mut root, StandardEvent::Focus(inner(0)));
        send(&mut event_loop, &mut root, StandardEvent::ActivateScreen(1));
        send(&mut event_loop, &mut root, StandardEvent::Focus(inner(1)));
        send(&mut event_loop, &mut root, StandardEvent::ActivateScreen(0));
        pump(&mut event_loop, &mut root, 200);
        let pointed = || {
            let mut pointed = POINTED.lock().unwrap();
            let names: Vec<String> = pointed
                .drain(..)
                .filter(|name| name.starts_with("pointed"))
                .collect();
            names
        };
        pointed();

        let mouse = |kind, x| {
            StandardEvent::UIEvent(UIEvent::Mouse(MouseEvent {
                kind,
                position: [x, 300.0],
                time_msec: None,
            }))
        };
        send(
            &mut event_loop,
            &mut root,
            mouse(MouseEventKind::Motion, 1500.0),
        );
        pump(&mut event_loop, &mut root, 50);
        assert!(pointed().contains(&"pointed 1".to_string()));
        assert_eq!(root.holder.tree.active_screen(), 0);

        let press = MouseEventKind::Button {
            button: ui::BTN_LEFT,
            pressed: true,
        };
        send(&mut event_loop, &mut root, mouse(press, 1500.0));
        pump(&mut event_loop, &mut root, 50);
        assert_eq!(root.holder.tree.active_screen(), 1);
    }

    /// A swipe over an inactive screen moves its focus; the other screen stays active (#158)
    #[test]
    fn swipe_acts_on_inactive_screen() {
        let mut event_loop = EventLoop::<RootNodeApplet>::try_new().unwrap();
        let (world, _calls) = calloop::channel::channel();
        let (request_sender, _requests) = calloop::channel::channel();
        let mut root = RootNodeApplet::new(
            ProjectApplet::restorer(container(
                Some("p"),
                (0..3).map(|i| tab(&i.to_string(), Vec::new())).collect(),
            )),
            BTreeMap::from([("tab".to_string(), Box::new(TabSpawner) as AppletSpawner)]),
            None,
            SharedState::new(StateKind::Surface, UITree::leaf(UIElement::Nothing)),
            request_sender,
            &event_loop.handle(),
            world,
        );
        let screens = named(&[("A", [0, 0, 800, 600]), ("B", [800, 0, 800, 600])]);
        send(&mut event_loop, &mut root, StandardEvent::Screens(screens));
        let inner = |i| WorldTreePath(vec![Step::Inner(i)]);
        send(&mut event_loop, &mut root, StandardEvent::Focus(inner(0)));
        send(&mut event_loop, &mut root, StandardEvent::ActivateScreen(1));
        send(&mut event_loop, &mut root, StandardEvent::Focus(inner(1)));
        send(&mut event_loop, &mut root, StandardEvent::ActivateScreen(0));
        pump(&mut event_loop, &mut root, 200);
        let motion = StandardEvent::UIEvent(UIEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Motion,
            position: [1200.0, 300.0],
            time_msec: None,
        }));
        send(&mut event_loop, &mut root, motion);
        // fingers left by one tab (`swipe.distance_x`): the next one
        swipe_through(&mut event_loop, &mut root, 3, [[-200.0, 0.0]; 3], false);
        pump(&mut event_loop, &mut root, 1500);
        let tree = &root.holder.tree;
        assert_eq!(tree.active_screen(), 0);
        assert_eq!(tree.screen_focus_path(0), inner(0));
        assert_eq!(tree.screen_focus_path(1), inner(2));
    }

    /// Projects' colors reach the root's treeview rows through the projects holding them (#76)
    #[test]
    fn project_colors_in_treeview() {
        use sonamu_ui::color::Color;
        let red = Color([0xFF, 0, 0, 0xFF]);
        let colored = |name, color, children| {
            let mut tree = project(name, children);
            if let Some(SavedApplet::Spawner {
                inner: Some(inner), ..
            }) = &mut tree.main
            {
                inner.color = Some(color);
            }
            tree
        };
        let inner = colored("b", Color([0, 0, 0xFF, 0xFF]), vec![tab("1", Vec::new())]);
        let outer = colored("a", red, vec![inner]);
        let mut event_loop = EventLoop::<RootNodeApplet>::try_new().unwrap();
        let (world, _calls) = calloop::channel::channel();
        let (request_sender, _requests) = calloop::channel::channel();
        let surface = SharedState::new(StateKind::Surface, UITree::leaf(UIElement::Nothing));
        let mut root = RootNodeApplet::new(
            ProjectApplet::restorer(container(Some("p"), vec![outer, tab("0", Vec::new())])),
            BTreeMap::from([("tab".to_string(), Box::new(TabSpawner) as AppletSpawner)]),
            None,
            surface,
            request_sender,
            &event_loop.handle(),
            world,
        );
        pump(&mut event_loop, &mut root, 200);
        let rows = view_tree_rows(&root.holder.tree, None, 0, false);
        // the root's tree is a project too, so `a`'s color crossed one; `b` is inside `a`, hibernated
        assert_eq!(rows[1].color, Some(red));
    }

    #[test]
    fn treeview_area_follows_side_and_floating() {
        let mut settings = Settings::default();
        settings.treeview.width = 20;
        let size = DisplayContainerSize::new(1000, 500);
        assert_eq!(
            side_areas(&settings, size, Some(SideView::Tree), false).tree,
            None
        );

        settings.treeview.side = TreeviewSide::Left;
        assert_eq!(
            side_areas(&settings, size, Some(SideView::Tree), true).tree,
            Some(([0, 0, 200, 500], TreeviewPlace::Column(TreeviewSide::Left)))
        );
        settings.treeview.side = TreeviewSide::Right;
        assert_eq!(
            side_areas(&settings, size, Some(SideView::Tree), true).tree,
            Some((
                [800, 0, 200, 500],
                TreeviewPlace::Column(TreeviewSide::Right)
            ))
        );

        settings.treeview.floating = TreeviewFloating::Center;
        settings.treeview.floating_size = [40, 70];
        assert_eq!(
            side_areas(&settings, size, None, true).tree,
            Some(([300, 75, 400, 350], TreeviewPlace::Floating))
        );

        settings.treeview.floating = TreeviewFloating::Side;
        let (gap, _) = settings.gap_and_border_px();
        assert_eq!(
            side_areas(&settings, size, None, true).tree,
            Some((
                [800 + gap, gap, 200 - 2 * gap, 500 - 2 * gap],
                TreeviewPlace::Floating
            ))
        );
    }

    #[test]
    fn side_areas_share_the_column() {
        let mut settings = Settings::default();
        settings.treeview.width = 20;
        settings.treeview.side = TreeviewSide::Left;
        let size = DisplayContainerSize::new(1000, 500);
        let column = [0, 0, 200, 500];
        let pinned = Some(SideView::History);

        let areas = side_areas(&settings, size, pinned, false);
        assert_eq!(areas.tree, None);
        assert_eq!(
            areas.history,
            Some((column, TreeviewPlace::Column(TreeviewSide::Left)))
        );
        assert_eq!(applet_area(&settings, size, pinned), [200, 800]);

        settings.treeview.floating = TreeviewFloating::Side;
        let areas = side_areas(&settings, size, pinned, true);
        assert_eq!(
            areas.tree,
            Some((column, TreeviewPlace::Column(TreeviewSide::Left)))
        );
        assert_eq!(areas.history, None);

        settings.treeview.floating = TreeviewFloating::Center;
        let areas = side_areas(&settings, size, pinned, true);
        assert!(matches!(areas.tree, Some((_, TreeviewPlace::Floating))));
        assert_eq!(
            areas.history,
            Some((column, TreeviewPlace::Column(TreeviewSide::Left)))
        );

        assert_eq!(applet_area(&settings, size, None), [0, 1000]);
    }
}
