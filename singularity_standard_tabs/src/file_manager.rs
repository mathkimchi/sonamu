//! A file tree next to the open documents.
//! Opened files are code editors embedded in the file manager's inner tree, so they can be plucked out of it.

use crate::{code_editor::CodeEditorApplet, pdf_preview::PdfPreviewApplet};
use singularity_common::{
    sap::{
        client_handle::ClientHandle,
        detached::DetachedTree,
        packets::{
            AwaitedWl, NewWlSurface, NodeAction, SpawnPosition, StandardEvent, StandardRequest,
            WlSurfaceId,
        },
        session::{SavedApplet, SavedTree},
    },
    settings::{self, Palette, Settings},
    utils::tree::{
        recursive_tree::RecursiveTreeNode,
        tree_node_path::{TreeNodePath, TreeTraverseOperation},
        world_tree::{WorldTree, WorldTreePath, world_tree_traversal::WorldTreeTraversalOperation},
    },
};
use singularity_sttk::{
    command_hub::program_command,
    creatable_applet::CreatableNodularApplet,
    nodular_applet::{
        Answer, AppletContext, AppletSpawner, AppletSpawnerTrait, EmbeddedId, NodeRequest,
        NodularAppletInitializer, StandardApplet, with_opacity,
    },
    session,
    standard_keybinds::{handle_standard_keybinds, standard_keybind_request},
    wl_surface_applet::WlSurfaceApplet,
};
use sonamu_ui::{
    color::Color,
    display_units::DisplayContainerSize,
    ui_element::{Style, UIElement, UITree, font_size},
    ui_event::{KeyModifiers, KeySymbol, KeyTrait, MouseEvent, MouseEventKind, UIEvent},
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
};

fn row_height() -> f32 {
    font_size() + 10.0
}
const INDENT: f32 = 28.0;
const LEFT_PAD: f32 = 12.0;
/// Of the width, for the file list
const LIST_SPLIT: f32 = 0.3;
const BTN_LEFT: u32 = 0x110;

// Translucent like the treeview; text is from the settings' palette
const LIST_BG: Color = with_opacity(Color([0x16, 0x16, 0x1C, 0]));
const SELECTED_BG: Color = with_opacity(Color([0x1C, 0x3D, 0x2E, 0]));
const SELECTED_UNFOCUSED_BG: Color = with_opacity(Color([0x26, 0x2A, 0x32, 0]));
const HEADER_BG: Color = with_opacity(Color([0x0D, 0x0D, 0x16, 0]));
/// Behind the documents when none is shown; an editor's background
const DOCUMENT_BG: Color = HEADER_BG;
const DIR_TEXT: Color = Color([0x7A, 0xB8, 0xFF, 0xFF]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Files,
    Document,
}

/// An element of its inner tree, selected in its own treeview selector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Selected {
    /// The file manager itself (its inner root)
    Root,
    Document(usize),
}

/// A visible row of the file tree
struct Entry {
    path: PathBuf,
    depth: usize,
    is_dir: bool,
}

/// An embedded applet, shown next to the file list
struct Document {
    id: EmbeddedId,
    handle: ClientHandle,
    /// `None` if it was placed here rather than opened
    file: Option<PathBuf>,
    /// The wl surface it shows (eg Okular's for a PDF)
    wl: Option<WlSurfaceId>,
}

/// A file shown in place of the active document until another is (`open` keeps its editor)
struct Preview {
    file: PathBuf,
    /// `None` if it can't be read
    document: Option<Document>,
}

fn is_pdf(file: &Path) -> bool {
    file.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

pub struct FileManagerApplet {
    root: PathBuf,
    expanded: HashSet<PathBuf>,
    /// Visible rows, rebuilt when `expanded` changes
    entries: Vec<Entry>,
    selected: usize,
    /// First visible entry
    scroll: usize,

    documents: Vec<Document>,
    active: Option<usize>,
    pane: Pane,
    /// `Some` while selecting an element of its inner tree (traversing), rather than taking input in `pane`
    selecting: Option<Selected>,
    /// Shown instead of the header until the next key (eg a file that can't be opened)
    status: Option<String>,
    /// `root` is the directory of the tree it is in (eg a project's), rather than one it was given
    follows_directory: bool,
    palette: Palette,

    /// Of the file under the mouse, else of the one selected while the file list has the keys (`previewed`)
    preview: Option<Preview>,
    /// The file list row under the mouse
    hovered: Option<usize>,
    /// Okular processes started for these PDFs (or restored wl clients), whose windows become documents
    /// (`take_wl_surface`)
    starting: Vec<(u32, Option<PathBuf>)>,
    /// Pids of `starting` processes that ended, from the threads waiting on them
    exited_sender: mpsc::Sender<u32>,
    exited: mpsc::Receiver<u32>,

    size: DisplayContainerSize,
    hook: AppletContext,
}
impl CreatableNodularApplet<PathBuf> for FileManagerApplet {
    fn new(root: PathBuf, hook: AppletContext) -> Self {
        let (exited_sender, exited) = mpsc::channel();
        let mut file_manager = Self {
            root,
            expanded: HashSet::new(),
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            documents: Vec::new(),
            active: None,
            pane: Pane::Files,
            selecting: None,
            status: None,
            follows_directory: false,
            palette: Palette::default(),
            preview: None,
            hovered: None,
            starting: Vec::new(),
            exited_sender,
            exited,
            // until the node sends a size
            size: DisplayContainerSize::new(800, 600),
            hook,
        };
        file_manager.rebuild_entries();
        file_manager
    }
}
impl FileManagerApplet {
    /// Args: the root directory (default: the directory of the tree it is in, eg a project's).
    /// Restored with its documents as its saved inner tree's children (`save`).
    #[must_use]
    pub fn get_applet_spawner() -> AppletSpawner {
        struct Spawner;
        impl AppletSpawnerTrait for Spawner {
            fn create_initializer(&self, args: &[&str]) -> Option<NodularAppletInitializer> {
                self.restore_initializer(args, SavedTree::default())
            }

            fn restore_initializer(
                &self,
                args: &[&str],
                inner: SavedTree,
            ) -> Option<NodularAppletInitializer> {
                let root = args.first().map(PathBuf::from);
                let follows_directory = root.is_none();
                // until its node answers
                let root = root.or_else(|| std::env::current_dir().ok())?;
                Some(Box::new(move |hook: AppletContext| {
                    if follows_directory {
                        hook.get_directory();
                    }
                    let mut file_manager = FileManagerApplet::new(root, hook);
                    file_manager.follows_directory = follows_directory;
                    file_manager.restore_documents(inner.children);
                    Box::new(file_manager)
                }))
            }

            fn duplicate(&self) -> AppletSpawner {
                Box::new(Self)
            }
        }
        Box::new(Spawner)
    }

    fn title(&self) -> String {
        let name = self.root.file_name().map_or_else(
            || self.root.display().to_string(),
            |name| name.to_string_lossy().into(),
        );
        format!("{name}/")
    }

    fn rebuild_entries(&mut self) {
        let selected = self
            .entries
            .get(self.selected)
            .map(|entry| entry.path.clone());
        self.entries.clear();
        push_entries(&self.root, 0, &self.expanded, &mut self.entries);
        // keep the same file selected
        self.selected = selected
            .and_then(|path| self.entries.iter().position(|entry| entry.path == path))
            .unwrap_or(0)
            .min(self.entries.len().saturating_sub(1));
        self.clamp_scroll();
    }

    // sizes are non-negative
    #[allow(clippy::cast_sign_loss)]
    fn list_width(&self) -> u32 {
        (self.size.width as f32 * LIST_SPLIT) as u32
    }

    #[allow(clippy::cast_sign_loss)]
    fn document_size(&self) -> DisplayContainerSize {
        DisplayContainerSize::new(
            self.size.width.saturating_sub(self.list_width()),
            self.size.height.saturating_sub(row_height() as u32),
        )
    }

    /// Entries that fit under the header
    #[allow(clippy::cast_sign_loss)]
    fn visible_rows(&self) -> usize {
        ((self.size.height as f32 / row_height()) as usize)
            .saturating_sub(1)
            .max(1)
    }

    fn clamp_scroll(&mut self) {
        let rows = self.visible_rows();
        self.scroll = self
            .scroll
            .clamp((self.selected + 1).saturating_sub(rows), self.selected);
    }

    fn select(&mut self, index: usize) {
        self.selected = index.min(self.entries.len().saturating_sub(1));
        self.clamp_scroll();
    }

    /// A directory toggles, a file opens
    fn activate_selected(&mut self) {
        let Some(entry) = self.entries.get(self.selected) else {
            return;
        };
        if entry.is_dir {
            let path = entry.path.clone();
            if !self.expanded.remove(&path) {
                self.expanded.insert(path);
            }
            self.rebuild_entries();
        } else {
            self.open(entry.path.clone());
        }
    }

    /// Collapses the selected directory, or selects the parent directory
    fn collapse_selected(&mut self) {
        let Some(entry) = self.entries.get(self.selected) else {
            return;
        };
        if entry.is_dir && self.expanded.remove(&entry.path) {
            self.rebuild_entries();
        } else if let Some(parent) = entry.path.parent()
            && let Some(index) = self.entries.iter().position(|entry| entry.path == parent)
        {
            self.select(index);
        }
    }

    /// The open document showing the file
    fn document_index(&self, file: &Path) -> Option<usize> {
        self.documents
            .iter()
            .position(|document| document.file.as_deref() == Some(file))
    }

    /// Shows the file's document, making one if it isn't open: its preview's editor, a new editor,
    /// or for a PDF, Okular's window once it arrives
    fn open(&mut self, file: PathBuf) {
        if let Some(index) = self.document_index(&file) {
            self.set_active(Some(index));
            self.set_pane(Pane::Document);
            return;
        }
        if is_pdf(&file) {
            self.start_okular(file);
            return;
        }
        let document = match self.preview.take() {
            Some(Preview {
                file: previewed,
                document: Some(document),
            }) if previewed == file => document,
            preview => {
                self.preview = preview;
                // shown in the header rather than as an editor showing the error
                if let Err(err) = std::fs::read_to_string(&file) {
                    self.status = Some(format!("Can't open: {err}"));
                    return;
                }
                let (id, handle) = self
                    .hook
                    .embed(CodeEditorApplet::get_boxed_initiator(file.clone()));
                Document {
                    id,
                    handle,
                    file: Some(file),
                    wl: None,
                }
            }
        };
        self.add_document(document);
    }

    /// Its window becomes a document (`take_wl_surface`)
    fn start_okular(&mut self, file: PathBuf) {
        if self
            .starting
            .iter()
            .any(|(_, starting)| starting.as_ref() == Some(&file))
        {
            return;
        }
        let mut command = program_command("okular", &[]);
        command.arg(&file);
        self.start(command, Some(file));
    }

    /// Its window becomes a document (`take_wl_surface`)
    fn start(&mut self, mut command: Command, file: Option<PathBuf>) {
        match command.spawn() {
            Ok(mut child) => {
                let pid = child.id();
                self.starting.push((pid, file));
                // published as `awaiting_wl`, so its window comes here wherever focus is
                self.hook.damage_treeview();
                self.status = Some("Starting Okular...".to_string());
                let exited = self.exited_sender.clone();
                let waker = self.hook.waker();
                // reaps it
                std::thread::spawn(move || {
                    let _ = child.wait();
                    let _ = exited.send(pid);
                    waker.wake();
                });
            }
            Err(err) => {
                let program = command.get_program().to_string_lossy();
                self.status = Some(format!("Can't start {program}: {err}"));
            }
        }
    }

    /// Makes its saved documents again, none active; wl clients are started, their windows becoming documents
    fn restore_documents(&mut self, documents: Vec<SavedTree>) {
        for saved in documents.into_iter().filter_map(|document| document.main) {
            let file = saved_file(&saved);
            match saved {
                SavedApplet::Wl(wl) => {
                    if let Some(command) = session::restore_command(&wl) {
                        self.start(command, file);
                    }
                }
                saved => {
                    let (id, handle) = self.hook.embed(session::restore(saved));
                    self.add_document(Document {
                        id,
                        handle,
                        file,
                        wl: None,
                    });
                }
            }
        }
        self.set_active(None);
        self.set_pane(Pane::Files);
    }

    /// Closes the document; a wl client is asked first, and its document goes once its window does
    fn close_document(&mut self, index: usize) {
        if self.documents[index].wl.is_some() {
            self.documents[index]
                .handle
                .send_event(StandardEvent::CloseRequest);
        } else {
            self.remove_document(index);
        }
    }

    /// The file to show in place of the active document: under the mouse, else selected while the file list has
    /// the keys
    fn previewed(&self) -> Option<&Path> {
        let index = self.hovered.or_else(|| {
            (self.pane == Pane::Files && self.selecting.is_none()).then_some(self.selected)
        })?;
        let entry = self.entries.get(index)?;
        (!entry.is_dir).then_some(entry.path.as_path())
    }

    /// Embeds an editor (or for a PDF, its first page) for the previewed file, unless it is open
    fn update_preview(&mut self) {
        let file = self
            .previewed()
            .filter(|file| self.document_index(file).is_none())
            .map(Path::to_path_buf);
        if self.preview.as_ref().map(|preview| &preview.file) == file.as_ref() {
            return;
        }
        self.preview = file.map(|file| {
            // the PDF preview shows its own errors
            let readable = is_pdf(&file) || std::fs::read_to_string(&file).is_ok();
            let document = readable.then(|| {
                let initializer = if is_pdf(&file) {
                    PdfPreviewApplet::get_boxed_initiator(file.clone())
                } else {
                    CodeEditorApplet::get_boxed_initiator(file.clone())
                };
                let (id, handle) = self.hook.embed(initializer);
                handle.send_event(StandardEvent::UIEvent(UIEvent::WindowResized(
                    self.document_size(),
                )));
                // it doesn't take keys
                handle.send_event(StandardEvent::FocusChanged(false));
                Document {
                    id,
                    handle,
                    file: Some(file.clone()),
                    wl: None,
                }
            });
            Preview { file, document }
        });
        self.hook.damage_window();
    }

    /// What the document area shows: the previewed file's document, else the active one;
    /// `Err` shows a note instead
    fn shown_document(&self) -> Result<&Document, &'static str> {
        let Some(file) = self.previewed() else {
            return self.active_document().ok_or("");
        };
        if let Some(index) = self.document_index(file) {
            return Ok(&self.documents[index]);
        }
        match &self.preview {
            Some(Preview {
                file: previewed,
                document: Some(document),
            }) if previewed == file => Ok(document),
            _ => Err("Can't preview it"),
        }
    }

    fn add_document(&mut self, document: Document) {
        document
            .handle
            .send_event(StandardEvent::UIEvent(UIEvent::WindowResized(
                self.document_size(),
            )));
        self.documents.push(document);
        self.set_active(Some(self.documents.len() - 1));
        self.set_pane(Pane::Document);
        self.hook.damage_treeview();
    }

    /// Takes the document out, keeping the others' order
    fn remove_document(&mut self, index: usize) -> Document {
        let document = self.documents.remove(index);
        self.active = match self.active {
            Some(active) if active > index => Some(active - 1),
            // the next one (or the new last one) takes its place
            Some(active) if active == index => {
                (!self.documents.is_empty()).then(|| index.min(self.documents.len() - 1))
            }
            active => active,
        };
        if self.active.is_none() {
            self.pane = Pane::Files;
        }
        self.send_document_focus();
        self.hook.damage_treeview();
        document
    }

    fn set_active(&mut self, active: Option<usize>) {
        if let Some(document) = self.active_document() {
            document
                .handle
                .send_event(StandardEvent::FocusChanged(false));
        }
        self.active = active;
        self.send_document_focus();
    }

    /// Takes input in the pane (leaving its selector)
    fn set_pane(&mut self, pane: Pane) {
        self.selecting = None;
        self.pane = if self.active.is_some() {
            pane
        } else {
            Pane::Files
        };
        self.send_document_focus();
    }

    /// Tells the active document whether it has the keys, and the node which wl surface does
    fn send_document_focus(&self) {
        let has_keys = self.pane == Pane::Document && self.selecting.is_none();
        if let Some(document) = self.active_document() {
            document
                .handle
                .send_event(StandardEvent::FocusChanged(has_keys));
        }
        let wl = self
            .active_document()
            .filter(|_| has_keys)
            .and_then(|document| document.wl);
        self.hook
            .request(StandardRequest::WlKeyboardFocus(wl).into());
        // the focus path changed
        self.hook.damage_treeview();
    }

    /// Starts or stops selecting in its own selector
    fn set_selecting(&mut self, selecting: Option<Selected>) {
        if let Some(Selected::Document(index)) = selecting {
            self.set_active(Some(index));
        }
        self.selecting = selecting;
        self.send_document_focus();
        self.hook.damage_window();
    }

    /// Enter: takes input in the selected element (the file list, or a document)
    fn confirm(&mut self, selected: Selected) {
        self.selecting = None;
        match selected {
            Selected::Root => self.set_pane(Pane::Files),
            Selected::Document(index) => {
                self.set_active(Some(index));
                self.set_pane(Pane::Document);
            }
        }
    }

    /// A key in its own selector: traversal of its inner tree (the root, then one document per child)
    fn handle_selector_key(&mut self, selected: Selected, key: KeySymbol, ui_event: UIEvent) {
        // on the selected element: a document closes here, the rest is the file manager's node's
        if let Some(request) = standard_keybind_request(&ui_event) {
            match (selected, request) {
                (Selected::Document(index), NodeRequest::Close) => {
                    self.close_document(index);
                    self.set_selecting(Some(Selected::Root));
                }
                (_, request) => self.hook.request(request),
            }
            return;
        }
        if matches!(key, KeySymbol::Enter) {
            self.confirm(selected);
            return;
        }
        // documents have no inner trees to collapse
        if matches!(key, KeySymbol::Char(' ')) {
            return;
        }
        let Some(operation) = key
            .to_char()
            .and_then(WorldTreeTraversalOperation::from_char)
        else {
            // any other key goes into the selected element
            self.confirm(selected);
            self.handle_key(ui_event);
            return;
        };
        self.traverse_selector(selected, operation);
    }

    /// Moves its own selector, or out of it to its node
    fn traverse_selector(&mut self, selected: Selected, operation: WorldTreeTraversalOperation) {
        let last = self.documents.len().checked_sub(1);
        match (selected, operation) {
            // out of its inner tree, to its node
            (
                Selected::Root,
                WorldTreeTraversalOperation::Layerwise(TreeTraverseOperation::Parent),
            )
            | (
                _,
                WorldTreeTraversalOperation::PrevLayer | WorldTreeTraversalOperation::GlobalRoot,
            ) => {
                self.selecting = None;
                self.hook.change_focus(operation);
            }
            (
                Selected::Document(_),
                WorldTreeTraversalOperation::Layerwise(TreeTraverseOperation::Parent),
            ) => self.set_selecting(Some(Selected::Root)),
            (Selected::Root, WorldTreeTraversalOperation::NextLayer) => self.confirm(selected),
            // into the document's inner tree, at its root
            (Selected::Document(index), WorldTreeTraversalOperation::NextLayer) => {
                self.confirm(selected);
                self.documents[index]
                    .handle
                    .send_event(StandardEvent::Focus(WorldTreePath::new_into()));
            }
            (
                Selected::Root,
                WorldTreeTraversalOperation::Layerwise(TreeTraverseOperation::Child(index)),
            ) if index < self.documents.len() => {
                self.set_selecting(Some(Selected::Document(index)));
            }
            (
                Selected::Root,
                WorldTreeTraversalOperation::Layerwise(TreeTraverseOperation::LastChild),
            ) => {
                if let Some(last) = last {
                    self.set_selecting(Some(Selected::Document(last)));
                }
            }
            (
                Selected::Document(index),
                WorldTreeTraversalOperation::Layerwise(TreeTraverseOperation::RelShiftSibling(
                    shift,
                )),
            ) => {
                let index = index.saturating_add_signed(shift).min(last.unwrap_or(0));
                self.set_selecting(Some(Selected::Document(index)));
            }
            _ => {}
        }
    }

    fn active_document(&self) -> Option<&Document> {
        self.documents.get(self.active?)
    }

    /// A standard keybind in the file list; returns whether it was one
    fn handle_files_keybind(&mut self, ui_event: &UIEvent) -> bool {
        // eg Alt+q: out of the file list, to its own selector
        if let Some(NodeRequest::ChangeFocus(WorldTreeTraversalOperation::PrevLayer)) =
            standard_keybind_request(ui_event)
        {
            self.set_selecting(Some(Selected::Root));
            return true;
        }
        handle_standard_keybinds(ui_event, &self.hook)
    }

    fn handle_key(&mut self, ui_event: UIEvent) {
        if let Some(selected) = self.selecting {
            match ui_event {
                UIEvent::Key {
                    symbol: Some(key),
                    pressed: true,
                    ..
                } => self.handle_selector_key(selected, key, ui_event),
                // eg 4 fingers up / down like Alt+q / `e`
                UIEvent::Swipe { .. } => {
                    if let Some(NodeRequest::ChangeFocus(operation)) =
                        standard_keybind_request(&ui_event)
                    {
                        self.traverse_selector(selected, operation);
                    }
                }
                // releases, and modifiers on their own, stay in the selector
                _ => {}
            }
            return;
        }
        let UIEvent::Key {
            symbol: Some(key),
            modifiers,
            pressed: true,
            ..
        } = ui_event
        else {
            match self.pane {
                Pane::Document => {
                    if let Some(document) = self.active_document() {
                        document.handle.send_event(StandardEvent::UIEvent(ui_event));
                    }
                }
                // eg swipes
                Pane::Files => {
                    self.handle_files_keybind(&ui_event);
                }
            }
            return;
        };
        self.status = None;
        // the selected file is previewed again
        self.hovered = None;

        // Ctrl+Tab switches between the file list and the document
        if modifiers == KeyModifiers::CTRL && matches!(key, KeySymbol::Char('\t')) {
            self.set_pane(match self.pane {
                Pane::Files => Pane::Document,
                Pane::Document => Pane::Files,
            });
            return;
        }

        match self.pane {
            // the document handles its own standard keybinds (eg Ctrl+W closes it)
            Pane::Document => {
                if let Some(document) = self.active_document() {
                    document.handle.send_event(StandardEvent::UIEvent(ui_event));
                }
            }
            Pane::Files => {
                if self.handle_files_keybind(&ui_event) {
                    return;
                }
                match (key, modifiers) {
                    (KeySymbol::ArrowKeyUp | KeySymbol::Char('k'), KeyModifiers::NONE) => {
                        self.select(self.selected.saturating_sub(1));
                    }
                    (KeySymbol::ArrowKeyDown | KeySymbol::Char('j'), KeyModifiers::NONE) => {
                        self.select(self.selected + 1);
                    }
                    (
                        KeySymbol::Enter | KeySymbol::ArrowKeyRight | KeySymbol::Char('l'),
                        KeyModifiers::NONE,
                    ) => self.activate_selected(),
                    (KeySymbol::ArrowKeyLeft | KeySymbol::Char('h'), KeyModifiers::NONE) => {
                        self.collapse_selected();
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_mouse(&mut self, mut mouse_event: MouseEvent) {
        let list_width = f64::from(self.list_width());
        let [x, y] = mouse_event.position;

        // non-negative, since it's past the header
        #[allow(clippy::cast_sign_loss)]
        let row = (x < list_width && y >= f64::from(row_height()))
            .then(|| ((y / f64::from(row_height())) as usize - 1) + self.scroll)
            .filter(|&index| index < self.entries.len());
        self.hovered = row;

        if x >= list_width {
            if let MouseEventKind::Button { pressed: true, .. } = mouse_event.kind {
                self.set_pane(Pane::Document);
            }
            if let Some(document) = self.active_document() {
                mouse_event.position = [x - list_width, y - f64::from(row_height())];
                document
                    .handle
                    .send_event(StandardEvent::UIEvent(UIEvent::Mouse(mouse_event)));
            }
            return;
        }

        match mouse_event.kind {
            MouseEventKind::Button {
                button: BTN_LEFT,
                pressed: true,
            } => {
                self.set_pane(Pane::Files);
                if let Some(index) = row {
                    self.select(index);
                    self.activate_selected();
                }
            }
            MouseEventKind::Scroll { delta, .. } => {
                let max_scroll = self.entries.len().saturating_sub(self.visible_rows());
                self.scroll = if delta[1] > 0.0 {
                    (self.scroll + 1).min(max_scroll)
                } else if delta[1] < 0.0 {
                    self.scroll.saturating_sub(1)
                } else {
                    self.scroll
                };
            }
            MouseEventKind::Button { .. } | MouseEventKind::Motion => {}
        }
    }

    fn render_list(&self, width: f32) -> UITree {
        let mut rows = vec![row(
            0,
            width,
            HEADER_BG,
            text(LEFT_PAD, self.title(), self.palette.inactive, width),
        )];
        for (row_index, (index, entry)) in self
            .entries
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(self.visible_rows())
            .enumerate()
        {
            let background = if index != self.selected {
                Color::TRANSPARENT
            } else if self.pane == Pane::Files {
                SELECTED_BG
            } else {
                SELECTED_UNFOCUSED_BG
            };
            let name = entry
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (label, color) = if entry.is_dir {
                let arrow = if self.expanded.contains(&entry.path) {
                    '▾'
                } else {
                    '▸'
                };
                (format!("{arrow} {name}"), DIR_TEXT)
            } else {
                let is_open = self
                    .documents
                    .iter()
                    .any(|document| document.file.as_ref() == Some(&entry.path));
                (
                    format!("  {name}"),
                    if is_open {
                        self.palette.accent
                    } else {
                        self.palette.fg
                    },
                )
            };
            rows.push(row(
                row_index + 1,
                width,
                background,
                text(LEFT_PAD + entry.depth as f32 * INDENT, label, color, width),
            ));
        }
        UITree::new_styled(UIElement::Backgrounded(LIST_BG), Style::full_size(), rows)
    }

    /// The open documents' names, then the active document
    fn render_documents(&self, width: f32) -> UITree {
        let header = match &self.status {
            Some(status) => vec![(status.clone(), UIElement::glyphon_attr(Color::RED))],
            None if self.documents.is_empty() => vec![(
                "Open a file (Enter or click)".to_string(),
                UIElement::glyphon_attr(self.palette.inactive),
            )],
            None => self
                .documents
                .iter()
                .enumerate()
                .map(|(index, document)| {
                    let name = document.handle.get_treeview().tree.get_root_value().clone();
                    let color = if Some(index) == self.active {
                        self.palette.accent
                    } else {
                        self.palette.inactive
                    };
                    (format!(" {name} "), UIElement::glyphon_attr(color))
                })
                .collect(),
        };
        let header = UITree::new_styled(
            UIElement::Backgrounded(HEADER_BG),
            Style::absolute([0.0, 0.0], [width, row_height()]),
            vec![UITree::leaf_styled(
                UIElement::Text(header),
                Style::absolute([LEFT_PAD, text_top()], [width, row_height() - text_top()]),
            )],
        );

        let size = self.document_size();
        let body = match self.shown_document() {
            Ok(document) => document.handle.get_surface(),
            Err(note) => UITree::new_styled(
                UIElement::Backgrounded(DOCUMENT_BG),
                Style::full_size(),
                vec![text(
                    LEFT_PAD,
                    note.to_string(),
                    self.palette.inactive,
                    size.width as f32,
                )],
            ),
        };
        let body = UITree::new_styled(
            UIElement::Nothing,
            Style::absolute([0.0, row_height()], [size.width as f32, size.height as f32]),
            vec![body],
        );
        UITree::new_styled(UIElement::Nothing, Style::full_size(), vec![header, body])
    }
}
impl StandardApplet for FileManagerApplet {
    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        match standard_event {
            StandardEvent::UIEvent(UIEvent::WindowResized(size)) => {
                self.size = size;
                self.clamp_scroll();
                let document_size = self.document_size();
                let preview = self
                    .preview
                    .as_ref()
                    .and_then(|preview| preview.document.as_ref());
                for document in self.documents.iter().chain(preview) {
                    document
                        .handle
                        .send_event(StandardEvent::UIEvent(UIEvent::WindowResized(
                            document_size,
                        )));
                }
            }
            StandardEvent::UIEvent(UIEvent::Mouse(mouse_event)) => self.handle_mouse(mouse_event),
            StandardEvent::UIEvent(ui_event @ UIEvent::Key { .. }) => self.handle_key(ui_event),
            StandardEvent::Directory(directory) if self.follows_directory => {
                let Some(root) = directory.or_else(|| std::env::current_dir().ok()) else {
                    return;
                };
                self.root = root;
                self.expanded.clear();
                self.rebuild_entries();
                self.hook.damage_treeview();
            }
            _ => return,
        }
        self.hook.damage_window();
    }

    fn settings_changed(&mut self, settings: &Settings) {
        self.palette = settings.palette.clone();
    }

    fn update(&mut self) {
        self.update_preview();
    }

    /// An Okular process ended
    fn wake(&mut self) {
        while let Ok(pid) = self.exited.try_recv() {
            self.starting.retain(|&(starting, _)| starting != pid);
        }
        self.hook.damage_treeview();
    }

    /// Okular's windows, for the PDFs it was started for
    fn awaiting_wl(&self) -> Vec<AwaitedWl> {
        self.starting
            .iter()
            .map(|&(pid, _)| AwaitedWl { pid, app_id: None })
            .collect()
    }

    /// Okular's window for a PDF becomes its document
    fn take_wl_surface(&mut self, surface: NewWlSurface) -> Result<(), NewWlSurface> {
        let Some(index) = self
            .starting
            .iter()
            .position(|&(pid, _)| surface.pid == Some(pid))
        else {
            return Err(surface);
        };
        let (_, file) = self.starting.remove(index);
        self.status = None;
        let wl = Some(surface.surface_id);
        let (id, handle) = self
            .hook
            .embed(WlSurfaceApplet::get_boxed_initiator(surface));
        self.add_document(Document {
            id,
            handle,
            file,
            wl,
        });
        self.hook.damage_window();
        Ok(())
    }

    /// Asks its wl clients (eg Okular) to close too, since nothing else shows their windows
    fn close_requested(&mut self) -> bool {
        for document in self
            .documents
            .iter()
            .filter(|document| document.wl.is_some())
        {
            document.handle.send_event(StandardEvent::CloseRequest);
        }
        true
    }

    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree {
        let list_width = self.list_width() as f32;
        let documents_width = container_size.width as f32 - list_width;
        UITree::new_container(vec![
            UITree::new_styled(
                UIElement::Nothing,
                Style::absolute([0.0, 0.0], [list_width, container_size.height as f32]),
                vec![self.render_list(list_width)],
            ),
            UITree::new_styled(
                UIElement::Nothing,
                Style::absolute(
                    [list_width, 0.0],
                    [documents_width, container_size.height as f32],
                ),
                vec![self.render_documents(documents_width)],
            ),
        ])
    }

    /// The root is the file manager; each open document is a child
    fn get_treeview(&self) -> WorldTree<String> {
        WorldTree::World(Box::new(RecursiveTreeNode::new(
            WorldTree::Base(self.title()),
            self.documents
                .iter()
                .map(|document| RecursiveTreeNode::from_value(document.handle.get_treeview().tree))
                .collect(),
        )))
    }

    /// Its root (none if it follows its tree's directory), and its documents as its inner tree's children,
    /// but not which is active
    fn save(&self) -> Option<SavedApplet> {
        let args = if self.follows_directory {
            Vec::new()
        } else {
            let root = std::path::absolute(&self.root).ok()?;
            vec![root.to_string_lossy().into_owned()]
        };
        let children = self
            .documents
            .iter()
            .filter_map(|document| {
                Some(SavedTree {
                    main: Some(document.handle.get_treeview().saved?),
                    ..SavedTree::default()
                })
            })
            .collect();
        let inner = SavedTree {
            children,
            ..SavedTree::default()
        };
        Some(session::saved_with_inner("file_manager", args, inner))
    }

    /// On the selected element while selecting; otherwise inside the active document while the document pane has
    /// the keys, or inside the file manager itself (the file list)
    fn get_focus_path(&self) -> WorldTreePath {
        match (self.selecting, self.active_document()) {
            (Some(Selected::Document(index)), _) => {
                WorldTreePath(Box::new([TreeNodePath(vec![index])]))
            }
            (None, Some(document)) if self.pane == Pane::Document => document_path(
                self.active.expect("has an active document"),
                document.handle.get_treeview().focus,
            ),
            _ => WorldTreePath::new_into(),
        }
    }

    fn is_traversing(&self) -> bool {
        self.selecting.is_some()
    }

    /// Selects the root (`[[]]`, eg `e` into it) or a document (`[[i]]`), or goes inside a document for longer paths
    fn focus(&mut self, path: &WorldTreePath) {
        match path.0.split_first() {
            Some((node_path, inner))
                if node_path.0.len() == 1 && node_path.0[0] < self.documents.len() =>
            {
                let index = node_path.0[0];
                if inner.is_empty() {
                    self.set_selecting(Some(Selected::Document(index)));
                } else {
                    self.confirm(Selected::Document(index));
                    self.documents[index]
                        .handle
                        .send_event(StandardEvent::Focus(WorldTreePath(inner.into())));
                }
            }
            _ => self.set_selecting(Some(Selected::Root)),
        }
        self.hook.damage_window();
        self.hook.damage_treeview();
    }

    /// Closes a document (`[[i]]`), like Ctrl+W on it in its selector
    fn node_action(&mut self, path: &WorldTreePath, action: NodeAction) {
        if action == NodeAction::Close
            && let [TreeNodePath(index)] = &*path.0
            && let &[index] = index.as_slice()
            && index < self.documents.len()
        {
            self.close_document(index);
            if self.selecting.is_some() {
                self.set_selecting(Some(Selected::Root));
            }
            self.hook.damage_window();
        }
    }

    /// A document (with everything inside it); the root plucks the file manager itself
    fn pluck(&mut self, path: &WorldTreePath) -> Answer<Option<DetachedTree>> {
        let index = match path.0.first().map(|node_path| node_path.0.as_slice()) {
            Some([index, ..]) if *index < self.documents.len() => *index,
            _ => return Answer::Now(None),
        };
        let document = self.remove_document(index);
        self.hook.damage_window();
        let mut tree = DetachedTree::leaf(document.handle.detach());
        tree.wl_surface = document.wl;
        Answer::Now(Some(tree))
    }

    /// Becomes a document; trees with children or wl surfaces are given back
    fn place(
        &mut self,
        _path: &WorldTreePath,
        _position: SpawnPosition,
        tree: DetachedTree,
    ) -> Answer<Result<(), DetachedTree>> {
        // TODO: wl surfaces inside inner trees; children of documents
        if tree.main.is_none() || tree.wl_surface.is_some() || !tree.children.is_empty() {
            return Answer::Now(Err(tree));
        }
        let (id, handle) = self.hook.embed_detached(tree.main.expect("checked above"));
        self.add_document(Document {
            id,
            handle,
            file: None,
            wl: None,
        });
        self.hook.damage_window();
        Answer::Now(Ok(()))
    }

    fn handle_embedded_request(&mut self, id: EmbeddedId, request: NodeRequest) {
        let Some(index) = self.documents.iter().position(|document| document.id == id) else {
            // the preview's surface may have changed
            if self
                .preview
                .as_ref()
                .and_then(|preview| preview.document.as_ref())
                .is_some_and(|document| document.id == id)
            {
                self.hook.damage_window();
            }
            return;
        };
        match request {
            NodeRequest::Standard(StandardRequest::Damage(_)) => {}
            NodeRequest::Standard(StandardRequest::Quit) => self.hook.quit(),
            // its path is inside its own inner tree, which is child `index` of this one
            NodeRequest::Standard(StandardRequest::RequestPluck(path)) => self
                .hook
                .request(StandardRequest::RequestPluck(document_path(index, path)).into()),
            NodeRequest::Standard(StandardRequest::RequestPlace(path, position)) => {
                self.hook.request(
                    StandardRequest::RequestPlace(document_path(index, path), position).into(),
                )
            }
            // documents are in the file manager's directory
            NodeRequest::GetDirectory => {
                self.documents[index]
                    .handle
                    .send_event(StandardEvent::Directory(Some(self.root.clone())));
                return;
            }
            NodeRequest::Standard(StandardRequest::GetSettings) => {
                self.documents[index]
                    .handle
                    .send_event(StandardEvent::SettingsChanged(settings::get()));
                return;
            }
            NodeRequest::Standard(_) => return,
            // documents can't hold children, split, be wrapped or hold wl surfaces or opened applets here, so this node does
            request @ (NodeRequest::AddChild(_)
            | NodeRequest::Split { .. }
            | NodeRequest::Wrap(_)
            | NodeRequest::Run(_)
            | NodeRequest::AddWlSurface(_)
            | NodeRequest::Open(_)
            | NodeRequest::FocusSelf) => {
                self.hook.request(request);
            }
            // eg Alt+q: out of the document, to it in the selector
            NodeRequest::ChangeFocus(
                WorldTreeTraversalOperation::PrevLayer
                | WorldTreeTraversalOperation::Layerwise(TreeTraverseOperation::Parent),
            ) => self.set_selecting(Some(Selected::Document(index))),
            // eg 3 finger swipes: prev / next node from this one
            request @ NodeRequest::ChangeFocus(WorldTreeTraversalOperation::Layerwise(
                TreeTraverseOperation::BfsPrev | TreeTraverseOperation::BfsNext,
            )) => self.hook.request(request),
            // TODO: a placed project dissolving here loses its contents (they'd need to be the node's children)
            NodeRequest::ChangeFocus(_) | NodeRequest::SwapSibling(_) | NodeRequest::Unwrap(_) => {
                return;
            }
            NodeRequest::Close => {
                // dropping the handle ends it
                self.remove_document(index);
            }
        }
        // a document's surface or title may have changed
        self.hook.damage_window();
        self.hook.damage_treeview();
    }
}

/// The file a saved document shows: a code editor's, or a PDF its wl client (eg Okular) was started with
fn saved_file(saved: &SavedApplet) -> Option<PathBuf> {
    match saved {
        SavedApplet::Spawner { name, args, .. } if name == "code_editor" => {
            args.first().map(PathBuf::from)
        }
        SavedApplet::Wl(wl) => wl
            .argv
            .last()
            .map(PathBuf::from)
            .filter(|file| is_pdf(file)),
        SavedApplet::Spawner { .. } => None,
    }
}

/// A path inside document `index`, as a path of the file manager's inner tree
fn document_path(index: usize, path: WorldTreePath) -> WorldTreePath {
    let mut full = vec![TreeNodePath(vec![index])];
    full.extend(path.0);
    WorldTreePath(full.into_boxed_slice())
}

/// Directories first, then files, each sorted by name; hidden files are skipped
fn push_entries(dir: &Path, depth: usize, expanded: &HashSet<PathBuf>, entries: &mut Vec<Entry>) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<(bool, PathBuf)> = read_dir
        .filter_map(Result::ok)
        .filter(|child| !child.file_name().to_string_lossy().starts_with('.'))
        .map(|child| (child.path().is_dir(), child.path()))
        .collect();
    children.sort_by(|(a_dir, a), (b_dir, b)| b_dir.cmp(a_dir).then_with(|| a.cmp(b)));

    for (is_dir, path) in children {
        let expand = is_dir && expanded.contains(&path);
        entries.push(Entry {
            path: path.clone(),
            depth,
            is_dir,
        });
        if expand {
            push_entries(&path, depth + 1, expanded, entries);
        }
    }
}

fn text_top() -> f32 {
    (row_height() - font_size()) / 2.0
}

fn text(left: f32, label: String, color: Color, width: f32) -> UITree {
    UITree::leaf_styled(
        UIElement::Text(vec![(label, UIElement::glyphon_attr(color))]),
        Style::absolute(
            [left, text_top()],
            [(width - left).max(0.0), row_height() - text_top()],
        ),
    )
}

fn row(index: usize, width: f32, background: Color, content: UITree) -> UITree {
    let background = if background == Color::TRANSPARENT {
        UIElement::Nothing
    } else {
        UIElement::Backgrounded(background)
    };
    UITree::new_styled(
        background,
        Style::absolute([0.0, index as f32 * row_height()], [width, row_height()]),
        vec![content],
    )
}
