//! The settings editor (#107): every setting as a form, written to `settings.json` as it changes

pub mod editor;
pub mod form;
pub mod keybinds;
pub mod pastel;

use editor::{Editor, hex};
use form::{Field, FieldKind};
use keybinds::Keybinds;
use serde_json::Value;
use sonamu_common::{
    Capture,
    command::{self, NodeUid},
    sap::packets::{StandardEvent, SwipeView},
    settings::{self, ERROR, Keybind, Palette, Settings},
    utils::tree::{
        recursive_tree::RecursiveTreeNode,
        tree_node_path::TreeNodePath,
        world_tree::{WorldTree, WorldTreePath},
    },
};
use sonamu_sttk::{
    nodular_applet::{
        AppletContext, AppletSpawner, AppletSpawnerTrait, NodularAppletInitializer, StandardApplet,
        swipe,
    },
    standard_keybinds::handle_standard_keybinds,
    ui::{
        self, Rows,
        color_picker::{ColorPicker, PickerKey},
        fields::{self, Edit, TextInput},
        file_chooser::FileChooser,
    },
};
use sonamu_ui::{
    color::Color,
    display_units::DisplayContainerSize,
    ui_element::{Style, UIElement, UITree, font_size},
    ui_event::{KeySymbol, MouseEventKind, UIEvent},
};
use std::{
    cell::Cell,
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::Instant,
};

/// The open editor's node: opening Settings again focuses it instead of making another
static OPEN: Mutex<Option<NodeUid>> = Mutex::new(None);

/// One line of the form
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Row {
    /// A section's title
    Title(usize),
    /// "Generate pastel", in Colors
    Generate,
    /// A setting: section and field index
    Field(usize, usize),
    /// An item of a list setting
    Item(usize, usize, usize),
    /// "+ add" below a list setting's items
    AddItem(usize, usize),
    /// A line of the open color picker, below its field
    Picker(usize),
    /// "type a path", first below a path field with its file chooser open
    TypePath,
    /// A line of the open file chooser
    Chooser(usize),
    /// The Keybinds section's title
    KeybindsTitle,
    /// A keybind group's name
    Group(usize),
    /// A keybind: group and bind index
    Bind(usize, usize),
    /// "+ keybind" at a group's end
    AddBind(usize),
    AddGroup,
    /// Puts back missing default groups
    AddDefaults,
}
impl Row {
    const fn is_title(self) -> bool {
        matches!(self, Self::Title(_) | Self::KeybindsTitle)
    }
}

/// Rows the color picker takes
const PICKER_LINES: usize = 6;

/// The setting the Keybinds section edits
const KEYBINDS: &str = "keybinds";

/// Header rows above the form: title with Revert, then the selected setting's description
const HEADER_ROWS: usize = 2;
const REVERT: &str = "Revert";
/// In the header while a section is shown alone: back to every section
const BACK: &str = "‹ Settings";

pub struct SettingsEditorApplet {
    editor: Editor,
    palette: Palette,
    /// As last drawn (resizes don't always come as events), for hit-testing
    size: Cell<DisplayContainerSize>,
    selected: Option<usize>,
    hovered: Option<usize>,
    scroll: usize,
    scrolled: ui::ScrollSteps,
    /// The selected row's text being typed
    editing: Option<TextInput>,
    /// The last write's error
    error: Option<String>,
    keybinds: Keybinds,
    /// The file's keybinds when `keybinds` last matched it, to see changes made elsewhere
    keybinds_seen: Value,
    /// Folded sections: their index, the Keybinds section's is after the form's
    collapsed: HashSet<usize>,
    /// The color field being picked, and its picker
    picker: Option<((usize, usize), ColorPicker)>,
    /// A number field's slider being dragged, and the value it shows (written on release)
    sliding: Option<((usize, usize), f64)>,
    /// The bind whose keys are being recorded; every key comes here meanwhile
    recording: Option<((usize, usize), Capture)>,
    /// The path field choosing a file, and its chooser
    chooser: Option<((usize, usize), FileChooser)>,
    /// The section shown alone (its inner child focused), else every section (its inner root)
    view: Option<usize>,
    /// A swipe between sections or to the inner root, drawn instead of the view
    swipe: Option<SwipeView>,
    /// The last click on a section's title, to tell a double click (shows it alone)
    title_click: Option<(usize, Instant)>,
    /// The config dir: path settings are relative to it
    dir: PathBuf,
    hook: AppletContext,
}
impl SettingsEditorApplet {
    fn new(dir: &Path, hook: AppletContext) -> Self {
        let editor = Editor::open(dir);
        Self {
            keybinds: Keybinds::from_json(&editor.value(&[KEYBINDS.to_owned()])),
            keybinds_seen: editor.value(&[KEYBINDS.to_owned()]),
            recording: None,
            collapsed: HashSet::new(),
            picker: None,
            sliding: None,
            chooser: None,
            view: None,
            swipe: None,
            title_click: None,
            dir: dir.to_owned(),
            editor,
            palette: settings::get().palette,
            size: Cell::new(DisplayContainerSize::new(800, 600)),
            selected: None,
            hovered: None,
            scroll: 0,
            scrolled: ui::ScrollSteps::default(),
            editing: None,
            error: None,
            hook,
        }
    }

    #[must_use]
    pub fn get_applet_spawner() -> AppletSpawner {
        struct Spawner;
        impl AppletSpawnerTrait for Spawner {
            fn create_initializer(&self, _args: &[&str]) -> Option<NodularAppletInitializer> {
                if let Some(uid) = *OPEN.lock().unwrap_or_else(PoisonError::into_inner) {
                    command::send(uid, "focus", Vec::new());
                    return None;
                }
                let dir = settings::config_dir()?;
                Some(NodularAppletInitializer::new(move |hook: AppletContext| {
                    Box::new(SettingsEditorApplet::new(&dir, hook))
                }))
            }

            fn duplicate(&self) -> AppletSpawner {
                Box::new(Self)
            }
        }
        Box::new(Spawner)
    }

    fn rows(&self) -> Vec<Row> {
        self.rows_of(self.view, true)
    }

    /// The rows `view` shows; with what's open in it if `live` (the view, not one a swipe brings in)
    fn rows_of(&self, view: Option<usize>, live: bool) -> Vec<Row> {
        let picker = self.picker.as_ref().map(|(at, _)| *at);
        let chooser = self
            .chooser
            .as_ref()
            .map(|(at, chooser)| (*at, chooser.lines()));
        let open = Open {
            picker: picker.filter(|_| live),
            chooser: chooser.filter(|_| live),
        };
        form_rows(&self.editor, &self.keybinds, &self.collapsed, open, view)
    }

    /// Shows `view` (`None`: every section); the treeview's focus follows
    fn show(&mut self, view: Option<usize>) {
        if view == self.view {
            return;
        }
        self.commit_editing();
        self.recording = None;
        self.picker = None;
        self.chooser = None;
        self.sliding = None;
        let from = std::mem::replace(&mut self.view, view);
        self.selected = None;
        self.hovered = None;
        self.scroll = 0;
        // back on every section: at the one just left
        if let (None, Some(from)) = (view, from) {
            let title = if from == self.editor.sections.len() {
                Row::KeybindsTitle
            } else {
                Row::Title(from)
            };
            if let Some(index) = self.rows().iter().position(|row| *row == title) {
                self.selected = Some(index);
                self.scroll = index.min(ui::max_scroll(self.rows().len(), self.shown()));
            }
        }
        self.hook.damage_window();
        self.hook.damage_treeview();
    }

    /// Drops a picker or chooser whose field changed (eg another wallpaper kind)
    fn close_stale(&mut self) {
        let is = |at: (usize, usize), kind: &FieldKind| {
            self.editor
                .sections
                .get(at.0)
                .and_then(|section| section.fields.get(at.1))
                .is_some_and(|field| field.kind == *kind)
        };
        if self
            .picker
            .as_ref()
            .is_some_and(|(at, _)| !is(*at, &FieldKind::Color))
        {
            self.picker = None;
        }
        if self
            .chooser
            .as_ref()
            .is_some_and(|(at, _)| !is(*at, &FieldKind::Path))
        {
            self.chooser = None;
        }
        self.selected = self.selected.filter(|&index| index < self.rows().len());
    }

    /// Selects `row`, if it's shown
    fn select_row(&mut self, row: Row) {
        let index = self.rows().iter().position(|&other| other == row);
        self.select(index);
    }

    /// Writes the Keybinds section's binds (unfinished ones left out)
    fn commit_keybinds(&mut self) {
        let json = self.keybinds.to_json();
        let result = self.editor.commit(&[vec![KEYBINDS.to_owned()]], &[json]);
        self.report(result);
        self.keybinds_seen = self.editor.value(&[KEYBINDS.to_owned()]);
    }

    /// Takes the keybinds from the file again if they changed there (eg Revert, a hand edit)
    fn sync_keybinds(&mut self) {
        let current = self.editor.value(&[KEYBINDS.to_owned()]);
        if keybinds_changed(&self.keybinds_seen, &current) {
            self.rebuild_keybinds();
        }
    }

    /// The keybinds as the file has them now
    fn rebuild_keybinds(&mut self) {
        let current = self.editor.value(&[KEYBINDS.to_owned()]);
        self.keybinds = Keybinds::from_json(&current);
        self.keybinds_seen = current;
        self.recording = None;
        self.selected = None;
        self.editing = None;
    }

    /// Records the keys of a bind the editor can edit
    fn start_recording(&mut self, at: (usize, usize)) {
        if self.keybinds.groups[at.0].1[at.1].editable() {
            self.recording = Some((at, Capture::start()));
        }
    }

    /// While recording: the pressed keys become the bind's (Esc alone cancels)
    fn record(&mut self, ui_event: UIEvent) {
        let (
            Some(((group, bind), _)),
            UIEvent::Key {
                symbol,
                modifiers,
                raw_keycode,
                pressed: true,
            },
        ) = (&self.recording, ui_event)
        else {
            return;
        };
        let (group, bind) = (*group, *bind);
        let bare = !(modifiers.ctrl || modifiers.alt || modifiers.shift || modifiers.logo);
        if bare && matches!(symbol, Some(KeySymbol::Escape)) {
            self.recording = None;
            return;
        }
        if let Some((mods, key)) = Keybind::recorded(symbol, raw_keycode, modifiers) {
            self.recording = None;
            let draft = &mut self.keybinds.groups[group].1[bind];
            draft.mods = mods;
            draft.key = Some(key);
            self.commit_keybinds();
        }
    }

    fn field(&self, section: usize, field: usize) -> &Field {
        &self.editor.sections[section].fields[field]
    }

    /// The field a row belongs to
    fn field_of(&self, row: Row) -> Option<&Field> {
        match row {
            Row::Field(section, field)
            | Row::Item(section, field, _)
            | Row::AddItem(section, field) => Some(self.field(section, field)),
            Row::TypePath | Row::Chooser(_) => {
                let ((section, field), _) = self.chooser.as_ref()?;
                Some(self.field(*section, *field))
            }
            _ => None,
        }
    }

    fn items(&self, field: &Field) -> Vec<String> {
        items(&self.editor, field)
    }

    fn list_rows(&self) -> Rows {
        Rows::new(HEADER_ROWS as f32 * ui::row_height()).scrolled(self.scroll)
    }

    fn shown(&self) -> usize {
        self.list_rows().fitting(self.size.get().height as f32)
    }

    fn report(&mut self, result: Result<(), String>) {
        self.error = result.err();
    }

    /// Enter or a click on `row`
    fn activate(&mut self, row: Row) {
        match row {
            Row::Title(section) => return self.fold(section),
            Row::KeybindsTitle => return self.fold(self.editor.sections.len()),
            _ => {}
        }
        if self.editor.read_only().is_some() {
            return;
        }
        match row {
            Row::Title(_) | Row::KeybindsTitle | Row::Picker(_) => {}
            Row::TypePath => {
                let value = self
                    .field_of(row)
                    .map(|field| self.editor.field_value(field));
                self.editing = Some(TextInput::new(
                    value.as_ref().map(plain).unwrap_or_default(),
                ));
            }
            Row::Chooser(line) => self.choose(line),
            Row::Generate => {
                let result = self.editor.generate(pastel::random_hue());
                self.report(result);
            }
            Row::Field(section, field) => {
                let field = self.field(section, field).clone();
                let value = self.editor.field_value(&field);
                match &field.kind {
                    FieldKind::Toggle => {
                        let on = !value.as_bool().unwrap_or_default();
                        let result = self.editor.commit(&[field.path], &[Value::from(on)]);
                        self.report(result);
                    }
                    FieldKind::Choice(_) | FieldKind::Variant(_) => self.cycle(&field, true),
                    FieldKind::Color => {
                        self.toggle_picker((section_index_of(row), field_index_of(row)), &value)
                    }
                    FieldKind::Path => {
                        self.toggle_chooser((section_index_of(row), field_index_of(row)), &value)
                    }
                    FieldKind::Number { .. } | FieldKind::Text { .. } => {
                        self.editing = Some(TextInput::new(plain(&value)));
                    }
                    FieldKind::TextList | FieldKind::ReadOnly => {}
                }
            }
            Row::Item(section, field, item) => {
                let text = self.items(self.field(section, field)).swap_remove(item);
                self.editing = Some(TextInput::new(text));
            }
            Row::AddItem(..) => self.editing = Some(TextInput::new(String::new())),
            Row::Group(group) => {
                self.editing = Some(TextInput::new(self.keybinds.groups[group].0.clone()));
            }
            Row::Bind(group, bind) => self.start_recording((group, bind)),
            Row::AddBind(group) => {
                let bind = self.keybinds.add_bind(group);
                self.select_row(Row::Bind(group, bind));
                self.start_recording((group, bind));
            }
            Row::AddGroup => {
                let group = self.keybinds.add_group();
                self.commit_keybinds();
                self.select_row(Row::Group(group));
                self.editing = Some(TextInput::new(self.keybinds.groups[group].0.clone()));
            }
            Row::AddDefaults => {
                self.keybinds.add_defaults();
                self.commit_keybinds();
            }
        }
    }

    /// Keys on a keybind or group row that aren't Enter; whether it was one
    fn handle_keybind_key(
        &mut self,
        row: Row,
        symbol: KeySymbol,
        modifiers: sonamu_ui::ui_event::KeyModifiers,
    ) -> bool {
        let vertical = matches!(symbol, KeySymbol::ArrowKeyUp | KeySymbol::ArrowKeyDown);
        let up = matches!(symbol, KeySymbol::ArrowKeyUp);
        match (row, symbol) {
            (Row::Bind(group, bind), KeySymbol::ArrowKeyLeft | KeySymbol::ArrowKeyRight) => {
                let forward = matches!(symbol, KeySymbol::ArrowKeyRight);
                self.keybinds.groups[group].1[bind].cycle_action(forward);
                self.commit_keybinds();
            }
            (Row::Bind(group, bind), KeySymbol::Char('\t')) => {
                let draft = &self.keybinds.groups[group].1[bind];
                if draft.takes_args() {
                    self.editing = Some(TextInput::new(draft.args_text()));
                }
            }
            (Row::Bind(group, bind), KeySymbol::Backspace) => {
                self.keybinds.delete_bind((group, bind));
                self.commit_keybinds();
            }
            (Row::Bind(group, bind), _) if vertical && modifiers.shift => {
                if let Some((group, bind)) = self.keybinds.move_bind((group, bind), up) {
                    self.commit_keybinds();
                    self.select_row(Row::Bind(group, bind));
                }
            }
            (Row::Group(group), KeySymbol::Backspace) => {
                self.keybinds.delete_group(group);
                self.commit_keybinds();
            }
            (Row::Group(group), _) if vertical && modifiers.shift => {
                let group = self.keybinds.move_group(group, up);
                self.commit_keybinds();
                self.select_row(Row::Group(group));
            }
            _ => return false,
        }
        true
    }

    /// Folds or unfolds a section
    fn fold(&mut self, section: usize) {
        if !self.collapsed.remove(&section) {
            self.collapsed.insert(section);
        }
        self.selected = self.selected.min(Some(self.rows().len().saturating_sub(1)));
    }

    /// Opens the color picker under a color field, or closes it if it's open there
    fn toggle_picker(&mut self, at: (usize, usize), value: &Value) {
        if self.picker.as_ref().is_some_and(|(open, _)| *open == at) {
            self.picker = None;
            return;
        }
        let color = value
            .as_str()
            .and_then(|text| text.parse().ok())
            .unwrap_or(Color([0, 0, 0, 0xFF]));
        self.picker = Some((at, ColorPicker::new(color)));
        if let Some(index) = self
            .rows()
            .iter()
            .position(|row| *row == Row::Field(at.0, at.1))
        {
            self.scroll = ui::scroll_to(self.scroll, index + PICKER_LINES, self.shown());
        }
    }

    /// Opens the file chooser under a path field (in the current file's folder), or closes it if it's open there
    fn toggle_chooser(&mut self, at: (usize, usize), value: &Value) {
        if self.chooser.as_ref().is_some_and(|(open, _)| *open == at) {
            return self.close_chooser();
        }
        let current = plain(value);
        let start = if current.is_empty() {
            self.dir.clone()
        } else {
            settings::config_path(Path::new(&current), &self.dir)
        };
        self.chooser = Some((at, FileChooser::new(&start)));
        self.select_row(Row::TypePath);
    }

    /// Closes the file chooser, selecting its field
    fn close_chooser(&mut self) {
        if let Some((at, _)) = self.chooser.take() {
            self.selected = None;
            self.select_row(Row::Field(at.0, at.1));
        }
    }

    /// Enter on a chooser line: goes into a folder, or writes the file as the setting
    fn choose(&mut self, line: usize) {
        let Some(((section, field), chooser)) = &mut self.chooser else {
            return;
        };
        let (section, field) = (*section, *field);
        let Some(file) = chooser.choose(line) else {
            return self.select_row(Row::Chooser(0));
        };
        let path = settings::config_relative(&file, &self.dir);
        let field = self.field(section, field).clone();
        let result = self.editor.commit_text(&field, &path.to_string_lossy());
        self.report(result);
        self.close_chooser();
    }

    /// Writes the picker's color (`original` to undo it)
    fn commit_picker(&mut self, original: bool) {
        let Some(((section, field), picker)) = &self.picker else {
            return;
        };
        let color = if original {
            picker.original
        } else {
            picker.color()
        };
        let path = self.field(*section, *field).path.clone();
        let result = self.editor.commit(&[path], &[Value::from(hex(color))]);
        self.report(result);
    }

    /// The picker's top left and height in the window, while its first line is shown
    fn picker_area(&self) -> Option<([f32; 2], f32)> {
        let index = self.rows().iter().position(|row| *row == Row::Picker(0))?;
        let rows = self.list_rows();
        (index >= self.scroll).then(|| {
            let top = rows.top_of(index) + ui::PAD / 2.0;
            let left = value_left(self.size.get().width as f32);
            ([left, top], PICKER_LINES as f32 * rows.height - ui::PAD)
        })
    }

    /// Steps a number field one step up (`up`) or down
    fn step_number(&mut self, field: &Field, up: bool) {
        let FieldKind::Number { min, max, integer } = field.kind else {
            return;
        };
        let current = self.editor.field_value(field).as_f64().unwrap_or_default();
        let next = fields::stepped(
            current,
            fields::step_size(min, max, integer),
            up,
            (min, max),
        );
        let result = self.editor.commit_text(field, &next.to_string());
        self.report(result);
    }

    /// A number field's value at `x` on its slider, if it has one
    fn slider_value_at(&self, field: &Field, x: f32) -> Option<f64> {
        let FieldKind::Number {
            min: Some(min),
            max: Some(max),
            integer,
        } = field.kind
        else {
            return None;
        };
        let (left, width) = number_layout(self.size.get().width as f32).slider?;
        let step = fields::step_size(Some(min), Some(max), integer);
        Some(fields::slider_value(
            f64::from((x - left) / width),
            min,
            max,
            step,
        ))
    }

    /// Moves a choice to its next (`forward`) or previous option
    fn cycle(&mut self, field: &Field, forward: bool) {
        if let FieldKind::Variant(variants) = &field.kind {
            let current = variant_name(&self.editor.value(&field.path));
            let at = variants
                .iter()
                .position(|(name, _)| *name == current)
                .unwrap_or(0);
            let (next, _) = &variants[fields::cycle(variants.len(), at, forward)];
            let result = self.editor.commit_variant(field, next);
            self.report(result);
            return self.close_stale();
        }
        let FieldKind::Choice(options) = &field.kind else {
            return;
        };
        let value = self.editor.value(&field.path);
        let current = options
            .iter()
            .position(|option| Some(option.as_str()) == value.as_str())
            .unwrap_or(0);
        let next = &options[fields::cycle(options.len(), current, forward)];
        let result = self.editor.commit(
            std::slice::from_ref(&field.path),
            &[Value::from(next.as_str())],
        );
        self.report(result);
    }

    /// Writes what's being typed into the selected row
    fn commit_editing(&mut self) {
        let (Some(input), Some(row)) = (
            self.editing.take(),
            self.selected
                .and_then(|index| self.rows().get(index).copied()),
        ) else {
            return;
        };
        match row {
            Row::Group(group) => {
                if self.keybinds.rename_group(group, &input.text) {
                    self.commit_keybinds();
                } else {
                    self.error = Some(format!("{:?} is empty or another group's name", input.text));
                }
                return;
            }
            Row::Bind(group, bind) => {
                let draft = &mut self.keybinds.groups[group].1[bind];
                if draft.set_args_text(&input.text) {
                    self.commit_keybinds();
                } else {
                    self.error = Some(format!(
                        "{:?} isn't right for {}",
                        input.text,
                        draft.action_name()
                    ));
                }
                return;
            }
            _ => {}
        }
        let Some(field) = self.field_of(row).cloned() else {
            return;
        };
        let result = match row {
            Row::Item(_, _, index) => {
                let mut items = self.items(&field);
                if input.text.trim().is_empty() {
                    items.remove(index);
                } else {
                    items[index] = input.text;
                }
                self.editor.commit(&[field.path], &[Value::from(items)])
            }
            Row::AddItem(..) if input.text.trim().is_empty() => Ok(()),
            Row::AddItem(..) => {
                let mut items = self.items(&field);
                items.push(input.text);
                self.editor.commit(&[field.path], &[Value::from(items)])
            }
            _ => self.editor.commit_text(&field, &input.text),
        };
        self.report(result);
    }

    fn selected_row(&self) -> Option<Row> {
        self.selected
            .and_then(|index| self.rows().get(index).copied())
    }

    fn select(&mut self, index: Option<usize>) {
        if index != self.selected {
            self.commit_editing();
        }
        self.selected = index;
        if let Some(index) = index {
            self.scroll = ui::scroll_to(self.scroll, index, self.shown());
        }
    }

    fn handle_key(&mut self, symbol: KeySymbol, modifiers: sonamu_ui::ui_event::KeyModifiers) {
        if let Some(input) = &mut self.editing {
            match input.handle_key(symbol, modifiers) {
                Edit::Commit => {
                    let typed_path = self.selected_row() == Some(Row::TypePath);
                    self.commit_editing();
                    if typed_path && self.error.is_none() {
                        self.close_chooser();
                    }
                }
                Edit::Cancel => self.editing = None,
                Edit::Changed | Edit::Ignored => {}
            }
            return;
        }
        if let Some((_, picker)) = &mut self.picker {
            match picker.handle_key(symbol, modifiers) {
                PickerKey::Changed => return self.commit_picker(false),
                PickerKey::Done => {
                    self.picker = None;
                    return;
                }
                PickerKey::Cancel => {
                    self.commit_picker(true);
                    self.picker = None;
                    return;
                }
                PickerKey::Ignored => {}
            }
        }
        let rows = self.rows();
        let row = self.selected.and_then(|index| rows.get(index).copied());
        match (row, symbol) {
            (_, KeySymbol::Escape) if self.chooser.is_some() => return self.close_chooser(),
            (_, KeySymbol::Escape) => return self.show(None),
            (Some(Row::Title(section)), KeySymbol::ArrowKeyRight) => {
                return self.show(Some(section));
            }
            (Some(Row::KeybindsTitle), KeySymbol::ArrowKeyRight) => {
                return self.show(Some(self.editor.sections.len()));
            }
            _ => {}
        }
        if let (
            Some(Row::Field(section, field)),
            KeySymbol::ArrowKeyLeft | KeySymbol::ArrowKeyRight,
        ) = (row, symbol)
            && let field = self.field(section, field).clone()
            && matches!(field.kind, FieldKind::Number { .. })
            && self.editor.read_only().is_none()
        {
            return self.step_number(&field, matches!(symbol, KeySymbol::ArrowKeyRight));
        }
        if let Some(row) = row
            && self.editor.read_only().is_none()
            && self.handle_keybind_key(row, symbol, modifiers)
        {
            return;
        }
        match symbol {
            KeySymbol::ArrowKeyDown | KeySymbol::ArrowKeyUp => {
                let down = matches!(symbol, KeySymbol::ArrowKeyDown);
                let mut next = ui::step(self.selected, down, rows.len());
                // titles are skipped, except at the ends
                while let Some(index) = next
                    && rows[index].is_title()
                    && ui::step(next, down, rows.len()) != next
                {
                    next = ui::step(next, down, rows.len());
                }
                self.select(next);
            }
            KeySymbol::PageDown | KeySymbol::PageUp => {
                let page = ui::page(self.shown());
                let index = self.selected.unwrap_or(0);
                let index = if matches!(symbol, KeySymbol::PageDown) {
                    (index + page).min(rows.len().saturating_sub(1))
                } else {
                    index.saturating_sub(page)
                };
                self.select(Some(index));
            }
            KeySymbol::Enter | KeySymbol::Char(' ') => {
                if let Some(row) = row {
                    self.activate(row);
                }
            }
            KeySymbol::ArrowKeyLeft | KeySymbol::ArrowKeyRight => {
                if let Some(field) = row.and_then(|row| self.field_of(row)).cloned() {
                    self.cycle(&field, matches!(symbol, KeySymbol::ArrowKeyRight));
                }
            }
            _ => {}
        }
    }

    /// The Revert button's left edge in the header
    fn revert_left(&self) -> f32 {
        self.size.get().width as f32 - ui::PAD - text_width(REVERT)
    }

    fn handle_mouse(&mut self, position: [f64; 2], kind: MouseEventKind) {
        let [x, y] = position;
        let list = self.list_rows();
        let count = self.rows().len();
        let row_at = list.row_at(y).filter(|&index| index < count);
        let point = [x as f32, y as f32];
        match kind {
            MouseEventKind::Motion => {
                self.hovered = row_at;
                if let (Some(([left, top], height)), Some((_, picker))) =
                    (self.picker_area(), &mut self.picker)
                {
                    picker.drag([point[0] - left, point[1] - top], height);
                }
                if let Some((at, _)) = self.sliding {
                    let field = self.field(at.0, at.1).clone();
                    if let Some(value) = self.slider_value_at(&field, point[0]) {
                        self.sliding = Some((at, value));
                    }
                }
            }
            MouseEventKind::Button {
                button: ui::BTN_LEFT,
                pressed: false,
            } => {
                if let Some((_, picker)) = &mut self.picker
                    && picker.release()
                {
                    self.commit_picker(false);
                }
                if let Some(((section, field), value)) = self.sliding.take() {
                    let field = self.field(section, field).clone();
                    let result = self.editor.commit_text(&field, &value.to_string());
                    self.report(result);
                }
            }
            MouseEventKind::Button {
                button: ui::BTN_LEFT,
                pressed: true,
            } => {
                if let (Some(([left, top], height)), Some((_, picker))) =
                    (self.picker_area(), &mut self.picker)
                    && self.editor.read_only().is_none()
                    && picker.press([point[0] - left, point[1] - top], height)
                {
                    return;
                }
                if let Some(index) = row_at
                    && let Row::Field(section, field_index) = self.rows()[index]
                    && let field = self.field(section, field_index).clone()
                    && matches!(field.kind, FieldKind::Number { .. })
                    && self.editor.read_only().is_none()
                {
                    self.select(Some(index));
                    let layout = number_layout(self.size.get().width as f32);
                    let within = |(from, to): (f32, f32)| (from..to).contains(&point[0]);
                    if within(layout.minus) || within(layout.plus) {
                        self.step_number(&field, within(layout.plus));
                    } else if let Some(value) = self.slider_value_at(&field, point[0])
                        && point[0]
                            >= layout
                                .slider
                                .map_or(f32::MAX, |(left, _)| left - font_size() / 2.0)
                    {
                        self.sliding = Some(((section, field_index), value));
                    } else if self.editing.is_none() {
                        self.activate(Row::Field(section, field_index));
                    }
                    return;
                }
                if y < f64::from(ui::row_height())
                    && self.view.is_some()
                    && x < f64::from(back_end())
                {
                    self.show(None);
                } else if let Some(index) = row_at
                    && let Some(section) = self.title_section(self.rows()[index])
                    && self.double_clicked(section)
                {
                    // the first click folded it
                    self.collapsed.remove(&section);
                    self.show(Some(section));
                } else if y < f64::from(ui::row_height()) && x >= f64::from(self.revert_left()) {
                    self.commit_editing();
                    let result = self.editor.revert();
                    self.report(result);
                    self.rebuild_keybinds();
                } else if let Some(index) = row_at {
                    self.select(Some(index));
                    if self.editing.is_none() {
                        self.activate(self.rows()[index]);
                    }
                }
            }
            MouseEventKind::Scroll { delta, v120, .. } => {
                let steps = self.scrolled.add(delta, v120);
                let max = ui::max_scroll(count, self.shown());
                self.scroll = self.scroll.saturating_add_signed(steps as isize).min(max);
            }
            MouseEventKind::Button { .. } => {}
        }
    }

    /// The section whose title `row` is
    fn title_section(&self, row: Row) -> Option<usize> {
        match row {
            Row::Title(section) => Some(section),
            Row::KeybindsTitle => Some(self.editor.sections.len()),
            _ => None,
        }
    }

    /// Whether this click on `section`'s title follows one on it
    fn double_clicked(&mut self, section: usize) -> bool {
        let double = self
            .title_click
            .is_some_and(|(last, time)| last == section && time.elapsed() < ui::DOUBLE_CLICK);
        self.title_click = (!double).then(|| (section, Instant::now()));
        double
    }

    fn section_title(&self, section: usize) -> &str {
        self.editor
            .sections
            .get(section)
            .map_or("Keybinds", |section| section.title.as_str())
    }

    /// The selected setting's description, else a hint
    fn help(&self) -> String {
        let row = self
            .selected
            .and_then(|index| self.rows().get(index).copied());
        if self.recording.is_some() {
            return "Press the keys (Esc alone: cancel)".to_owned();
        }
        match row {
            Some(Row::Bind(..)) => {
                return "Enter: record keys, Left/Right: command, Tab: its args, Shift+Up/Down: move, Backspace: \
                        delete"
                    .to_owned();
            }
            Some(Row::Group(_)) => {
                return "Enter: rename, Shift+Up/Down: move, Backspace: delete (with its keybinds)"
                    .to_owned();
            }
            Some(Row::KeybindsTitle) => {
                return "Shortcuts in named groups, shown as sections in help (Super+?). Enter or click: fold, \
                        Right or double click: show alone"
                    .to_owned();
            }
            Some(Row::Title(_)) => {
                return "Enter or click: fold, Right or double click: show alone".to_owned();
            }
            Some(Row::TypePath | Row::Chooser(_)) => {
                return "Enter or click: open a folder or pick a file, Esc: close".to_owned();
            }
            Some(Row::Field(section, field))
                if self.field(section, field).kind == FieldKind::Path =>
            {
                let help = &self.field(section, field).help;
                return format!("{help} (Enter or click: choose a file)");
            }
            Some(Row::Field(section, field))
                if matches!(self.field(section, field).kind, FieldKind::Number { .. }) =>
            {
                let help = &self.field(section, field).help;
                return format!("{help} (−/+ or Left/Right: step, drag the slider, Enter: type)");
            }
            _ => {}
        }
        match row.and_then(|row| self.field_of(row)) {
            Some(field) if !field.help.is_empty() => field.help.replace('\n', " "),
            _ if self.view.is_some() => {
                "Up/Down: pick, Enter or click: change, Left/Right: options, Esc: every section"
                    .to_owned()
            }
            _ => "Up/Down: pick, Enter or click: change, Left/Right: options".to_owned(),
        }
    }

    /// Title (with the back button in a section), status and Revert; then the selected row's help if `live`
    fn header(&self, view: Option<usize>, live: bool, width: f32) -> Vec<UITree> {
        let header = Rows::new(0.0);
        let palette = &self.palette;
        let status = match (&self.error, self.editor.read_only()) {
            (Some(error), _) => ui::text(error.clone(), ERROR),
            (None, Some(program)) => ui::text(
                format!(
                    "Settings come from a program ({}): change it there",
                    program.display()
                ),
                palette.inactive,
            ),
            (None, None) => ui::text("Changes apply as you make them", palette.inactive),
        };
        let revert_left = self.revert_left();
        let mut parts = Vec::new();
        let title_left = match view {
            Some(_) => {
                parts.push(
                    header
                        .text(ui::text(BACK, palette.accent), ui::PAD, back_end())
                        .with_pointer(),
                );
                back_end()
            }
            None => ui::PAD,
        };
        let title = view.map_or("Settings", |section| self.section_title(section));
        let title_end = title_left + text_width(title) + ui::PAD;
        parts.extend([
            header.text(ui::text(title, palette.fg), title_left, title_end),
            header.text(status, title_end, revert_left - ui::PAD),
        ]);
        if self.editor.read_only().is_none() {
            parts.push(
                header
                    .text(ui::text(REVERT, palette.accent), revert_left, width)
                    .with_pointer(),
            );
        }
        vec![
            header.row(0, width, Some(ui::raised_bg(palette)), parts),
            header.row(
                1,
                width,
                None,
                vec![header.text(
                    ui::text(
                        if live { self.help() } else { String::new() },
                        palette.inactive,
                    ),
                    ui::PAD,
                    width - ui::PAD,
                )],
            ),
        ]
    }

    /// A keybind row's keys, command and args
    fn bind_parts(
        &self,
        rows: &Rows,
        (group, bind): (usize, usize),
        value_left: f32,
        width: f32,
    ) -> Vec<UITree> {
        let palette = &self.palette;
        let editing = self.editing.as_ref().filter(|_| {
            self.selected
                .and_then(|index| self.rows().get(index).copied())
                == Some(Row::Bind(group, bind))
        });
        let draft = &self.keybinds.groups[group].1[bind];
        let recording = self
            .recording
            .as_ref()
            .is_some_and(|(at, _)| *at == (group, bind));
        let keys = if recording {
            ui::text("press keys…", palette.accent)
        } else {
            let color = if draft.key.is_some() {
                palette.fg
            } else {
                ERROR
            };
            ui::text(draft.keys_label(), color)
        };
        let args = editing.map_or_else(
            || ui::text(draft.args_text(), palette.inactive),
            |input| input.element(true, palette),
        );
        let complete = draft.to_json().is_some();
        let action_left = value_left;
        let args_left = action_left + text_width(draft.action_name()) + 4.0 * font_size();
        vec![
            rows.text(keys, ui::PAD + ui::INDENT, value_left),
            rows.text(
                fields::choice(draft.action_name(), palette),
                action_left,
                args_left,
            ),
            rows.text(args, args_left, width - ui::PAD),
        ]
        .into_iter()
        .chain((!complete && !recording).then(|| {
            rows.text(
                ui::text("unfinished", ERROR),
                width - ui::PAD - text_width("unfinished"),
                width,
            )
        }))
        .collect()
    }

    /// A number field's −, value, + and slider
    fn number_parts(
        &self,
        rows: &Rows,
        value: f64,
        (min, max): (Option<f64>, Option<f64>),
        width: f32,
    ) -> Vec<UITree> {
        let palette = &self.palette;
        let layout = number_layout(width);
        let mut parts = vec![
            rows.text(
                ui::text("−", palette.accent),
                layout.minus.0,
                layout.minus.1,
            ),
            rows.text(
                ui::text(format_number(value), palette.fg),
                layout.text.0,
                layout.text.1,
            ),
            rows.text(ui::text("+", palette.accent), layout.plus.0, layout.plus.1),
        ];
        if let (Some(min), Some(max), Some((left, slider_width))) = (min, max, layout.slider) {
            let fraction = fields::slider_fraction(value, min, max);
            parts.push(fields::slider(rows, left, slider_width, fraction, palette));
        }
        parts
    }

    /// The open picker, its color and hints beside it
    fn picker_parts(&self) -> Vec<UITree> {
        let (Some(((section, field), picker)), Some((origin, height))) =
            (&self.picker, self.picker_area())
        else {
            return Vec::new();
        };
        let palette = &self.palette;
        let mut parts = picker.draw(origin, height, palette);
        let left = origin[0]
            + height
            + ui::color_picker::STRIP_GAP
            + ui::color_picker::STRIP_WIDTH
            + ui::PAD;
        let right = self.size.get().width as f32 - ui::PAD;
        let lines = [
            (
                format!(
                    "{}: {}",
                    self.field(*section, *field).label,
                    hex(picker.color())
                ),
                palette.fg,
            ),
            (
                "Drag; arrows: saturation, value".to_owned(),
                palette.inactive,
            ),
            ("Shift+Up/Down: hue".to_owned(), palette.inactive),
            ("Enter: done, Esc: undo".to_owned(), palette.inactive),
        ];
        let line = ui::row_height();
        for (index, (text, color)) in lines.into_iter().enumerate() {
            parts.push(UITree::leaf_styled(
                ui::text(text, color),
                Style::absolute(
                    [left, origin[1] + index as f32 * line],
                    [(right - left).max(0.0), line],
                ),
            ));
        }
        parts
    }

    /// A setting's label and value
    fn field_parts(
        &self,
        rows: &Rows,
        (section, field_index): (usize, usize),
        editing: Option<&TextInput>,
        width: f32,
    ) -> Vec<UITree> {
        let palette = &self.palette;
        let value_left = value_left(width);
        let label = |text: &str, color| rows.text(ui::text(text, color), ui::PAD, value_left);
        let value = |element| rows.text(element, value_left, width - ui::PAD);
        let field = self.field(section, field_index);
        let current = self.editor.field_value(field);
        let mut parts = vec![label(&field.label, palette.fg)];
        match (&field.kind, editing) {
            (_, Some(input)) => parts.push(value(input.element(true, palette))),
            (FieldKind::Toggle, None) => {
                parts.push(value(fields::toggle(
                    current.as_bool().unwrap_or_default(),
                    palette,
                )));
            }
            (FieldKind::Choice(_), None) => {
                parts.push(value(fields::choice(
                    current.as_str().unwrap_or_default(),
                    palette,
                )));
            }
            (FieldKind::Variant(_), None) => {
                parts.push(value(fields::choice(&variant_name(&current), palette)));
            }
            (FieldKind::Number { min, max, .. }, None) => {
                let shown = self
                    .sliding
                    .filter(|(at, _)| *at == (section, field_index))
                    .map_or_else(|| current.as_f64().unwrap_or_default(), |(_, value)| value);
                parts.extend(self.number_parts(rows, shown, (*min, *max), width));
            }
            (FieldKind::Color, None) => {
                let picked = self
                    .picker
                    .as_ref()
                    .filter(|(at, _)| *at == (section, field_index))
                    .map(|(_, picker)| picker.color());
                let color = picked.or_else(|| current.as_str().and_then(|text| text.parse().ok()));
                if let Some(color) = color {
                    parts.push(fields::swatch(rows, value_left, color, palette));
                }
                let left = value_left + font_size() + 8.0;
                let text = color.map_or_else(|| plain(&current), hex);
                parts.push(rows.text(ui::text(text, palette.fg), left, width - ui::PAD));
            }
            (FieldKind::TextList, None) => {}
            (FieldKind::ReadOnly, None) => parts.push(value(ui::text(
                format!("{current} (edit in settings.json)"),
                palette.inactive,
            ))),
            (FieldKind::Path, None) if plain(&current).is_empty() => {
                parts.push(value(ui::text("choose a file…", palette.inactive)));
            }
            (_, None) if current.is_null() => {
                parts.push(value(ui::text("unset", palette.inactive)))
            }
            (_, None) => parts.push(value(ui::text(plain(&current), palette.fg))),
        }
        parts
    }

    /// Row `index` of the view, or (not `live`) of one a swipe brings in
    fn draw_row(&self, rows: &Rows, (index, row): (usize, Row), live: bool, width: f32) -> UITree {
        let palette = &self.palette;
        let value_left = value_left(width);
        let selected = live && self.selected == Some(index);
        let background = if selected {
            Some(ui::selected_bg(palette))
        } else if live && self.hovered == Some(index) && !row.is_title() {
            Some(ui::raised_bg(palette))
        } else {
            None
        };
        let editing = self.editing.as_ref().filter(|_| selected);
        let label = |text: &str, color| rows.text(ui::text(text, color), ui::PAD, value_left);
        let value = |element| rows.text(element, value_left, width - ui::PAD);
        let parts = match row {
            Row::Title(section) => {
                let arrow = if self.collapsed.contains(&section) {
                    "▸"
                } else {
                    "▾"
                };
                let title = format!("{arrow} {}", self.editor.sections[section].title);
                vec![label(&title, palette.accent)]
            }
            Row::Generate => vec![label("Generate pastel", palette.fg)],
            Row::Picker(_) => Vec::new(),
            Row::TypePath => {
                let element = editing.map_or_else(
                    || ui::text("✎ type a path", palette.inactive),
                    |input| input.element(true, palette),
                );
                vec![value(element)]
            }
            Row::Chooser(line) => {
                let label = self
                    .chooser
                    .as_ref()
                    .map(|(_, chooser)| chooser.label(line))
                    .unwrap_or_default();
                let color = match line {
                    0 => palette.inactive,
                    _ if label.ends_with('/') => palette.accent,
                    _ => palette.fg,
                };
                vec![value(ui::text(label, color))]
            }
            Row::Field(section, field) => self.field_parts(rows, (section, field), editing, width),
            Row::Item(section, field, item) => {
                let text = self.items(self.field(section, field)).swap_remove(item);
                let element = editing.map_or_else(
                    || ui::text(text, palette.fg),
                    |input| input.element(true, palette),
                );
                vec![label("  •", palette.inactive), value(element)]
            }
            Row::AddItem(..) => {
                let element = editing.map_or_else(
                    || ui::text("+ add", palette.inactive),
                    |input| input.element(true, palette),
                );
                vec![value(element)]
            }
            Row::KeybindsTitle => {
                let folded = self.collapsed.contains(&self.editor.sections.len());
                let arrow = if folded { "▸" } else { "▾" };
                vec![label(&format!("{arrow} Keybinds"), palette.accent)]
            }
            Row::Group(group) => {
                let name = &self.keybinds.groups[group].0;
                let element = editing.map_or_else(
                    || ui::text(name.clone(), palette.fg),
                    |input| input.element(true, palette),
                );
                vec![rows.text(element, ui::PAD, value_left)]
            }
            Row::Bind(group, bind) => self.bind_parts(rows, (group, bind), value_left, width),
            Row::AddBind(_) => vec![rows.text(
                ui::text("+ keybind", palette.inactive),
                ui::PAD + ui::INDENT,
                value_left,
            )],
            Row::AddGroup => vec![label("+ group", palette.inactive)],
            Row::AddDefaults => vec![label("+ missing default groups", palette.inactive)],
        };
        let tree = rows.row(index, width, background, parts);
        if row.is_title() {
            tree
        } else {
            tree.with_pointer()
        }
    }
}

impl Drop for SettingsEditorApplet {
    fn drop(&mut self) {
        let mut open = OPEN.lock().unwrap_or_else(PoisonError::into_inner);
        if *open == self.hook.uid() {
            *open = None;
        }
    }
}

/// Whether the file's keybinds changed from `last`
fn keybinds_changed(last: &Value, now: &Value) -> bool {
    // as text: maps compare without their order, which is the groups' order
    serde_json::to_vec(last).ok() != serde_json::to_vec(now).ok()
}

fn section_index_of(row: Row) -> usize {
    match row {
        Row::Field(section, _) => section,
        _ => 0,
    }
}

fn field_index_of(row: Row) -> usize {
    match row {
        Row::Field(_, field) => field,
        _ => 0,
    }
}

/// The back button's right edge
fn back_end() -> f32 {
    ui::PAD + text_width(BACK) + ui::PAD
}

/// Where values start in a row of an applet `width` wide
fn value_left(width: f32) -> f32 {
    (width * 0.4).max(ui::PAD + 8.0 * font_size())
}

/// Where a number field's parts are in a row: −, the value, +, and the slider (left and width) if there's room
struct NumberLayout {
    minus: (f32, f32),
    text: (f32, f32),
    plus: (f32, f32),
    slider: Option<(f32, f32)>,
}

fn number_layout(width: f32) -> NumberLayout {
    let em = font_size();
    let left = value_left(width);
    let slider_left = left + 9.0 * em;
    let slider_width = width - ui::PAD - em - slider_left;
    NumberLayout {
        minus: (left, left + 1.5 * em),
        text: (left + 1.5 * em, left + 6.5 * em),
        plus: (left + 6.5 * em, left + 8.0 * em),
        slider: (slider_width >= 4.0 * em).then_some((slider_left, slider_width)),
    }
}

/// The editor's treeview: "Settings", with each section (Keybinds last) as an inner child
fn section_tree(editor: &Editor) -> WorldTree<String> {
    let titles = editor
        .sections
        .iter()
        .map(|section| section.title.clone())
        .chain(["Keybinds".to_owned()]);
    WorldTree::World(Box::new(RecursiveTreeNode::new(
        WorldTree::Base("Settings".to_owned()),
        titles
            .map(|title| RecursiveTreeNode::from_value(WorldTree::Base(title)))
            .collect(),
    )))
}

/// What's open under a field: the color picker, the file chooser (with its line count)
#[derive(Debug, Clone, Copy, Default)]
struct Open {
    picker: Option<(usize, usize)>,
    chooser: Option<((usize, usize), usize)>,
}

/// The rows of `view` (a section alone, without its title; else every section's title, then its rows unless
/// folded): fields (and list items, what's `open`), the keybinds last
fn form_rows(
    editor: &Editor,
    keybinds: &Keybinds,
    collapsed: &HashSet<usize>,
    open: Open,
    view: Option<usize>,
) -> Vec<Row> {
    let shows = |section| view.is_none_or(|view| view == section);
    let folded = |section| view.is_none() && collapsed.contains(&section);
    let mut rows = Vec::new();
    for (section_index, section) in editor.sections.iter().enumerate() {
        if !shows(section_index) {
            continue;
        }
        if view.is_none() {
            rows.push(Row::Title(section_index));
        }
        if folded(section_index) {
            continue;
        }
        if section.title == "Colors" && editor.read_only().is_none() {
            rows.push(Row::Generate);
        }
        for (field_index, field) in section.fields.iter().enumerate() {
            let at = (section_index, field_index);
            rows.push(Row::Field(section_index, field_index));
            if field.kind == FieldKind::TextList {
                let count = items(editor, field).len();
                rows.extend((0..count).map(|item| Row::Item(section_index, field_index, item)));
                rows.push(Row::AddItem(section_index, field_index));
            }
            if open.picker == Some(at) {
                rows.extend((0..PICKER_LINES).map(Row::Picker));
            }
            if let Some((chooser, lines)) = open.chooser
                && chooser == at
            {
                rows.push(Row::TypePath);
                rows.extend((0..lines).map(Row::Chooser));
            }
        }
    }
    let keybinds_section = editor.sections.len();
    if !shows(keybinds_section) {
        return rows;
    }
    if view.is_none() {
        rows.push(Row::KeybindsTitle);
    }
    if !folded(keybinds_section) {
        for (group, (_, binds)) in keybinds.groups.iter().enumerate() {
            rows.push(Row::Group(group));
            rows.extend((0..binds.len()).map(|bind| Row::Bind(group, bind)));
            rows.push(Row::AddBind(group));
        }
        rows.extend([Row::AddGroup, Row::AddDefaults]);
    }
    rows
}

/// The view an inner path is: the inner root (every section) or a section's child; `None` if neither
#[expect(
    clippy::option_option,
    reason = "not a view, or the view (itself an option)"
)]
fn view_at(path: &WorldTreePath, sections: usize) -> Option<Option<usize>> {
    let (TreeNodePath(steps), _) = path.split_world();
    match *steps.as_slice() {
        [] => Some(None),
        [section] if section <= sections => Some(Some(section)),
        _ => None,
    }
}

/// A list setting's items
fn items(editor: &Editor, field: &Field) -> Vec<String> {
    editor
        .value(&field.path)
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// A variant setting's kind: its name, or its object's key
fn variant_name(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_object().and_then(|map| map.keys().next().cloned()))
        .unwrap_or_default()
}

/// A number as shown: whole ones without `.0`, others to 2 places
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}").trim_end_matches('0').to_owned()
    }
}

/// A setting as text to edit: strings without quotes
fn plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        _ => value.to_string(),
    }
}

/// About how wide `text` is drawn (0.6 em per char)
fn text_width(text: &str) -> f32 {
    text.chars().count() as f32 * font_size() * 0.6
}

impl SettingsEditorApplet {
    /// What `view` shows: as it is if it's the view, else from its top (eg brought in by a swipe)
    fn page(&self, view: Option<usize>, size: DisplayContainerSize) -> UITree {
        let live = view == self.view;
        let [width, height] = [size.width, size.height].map(|px| px as f32);
        let scroll = if live { self.scroll } else { 0 };
        let rows = Rows::new(HEADER_ROWS as f32 * ui::row_height()).scrolled(scroll);
        let all = self.rows_of(view, live);
        let shown = rows.fitting(height);
        let mut parts = self.header(view, live, width);
        for (index, &row) in all.iter().enumerate().skip(scroll).take(shown) {
            parts.push(self.draw_row(&rows, (index, row), live, width));
        }
        if live {
            parts.extend(self.picker_parts());
        }
        parts.extend(ui::scrollbar(
            &self.palette,
            ui::scrollbar_x(width),
            [rows.top, height - rows.top],
            scroll,
            shown,
            all.len(),
        ));
        UITree::new_styled(
            UIElement::Backgrounded(ui::bg(&self.palette)),
            Style::absolute([0.0, 0.0], [width, height]),
            parts,
        )
    }
}

impl StandardApplet for SettingsEditorApplet {
    fn settings_changed(&mut self, settings: &Settings) {
        self.palette = settings.palette.clone();
        self.editor.reload();
        self.close_stale();
        self.sync_keybinds();
        self.hook.damage_window();
        self.hook.damage_treeview();
    }

    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        // known once the node has its uid
        if let Some(uid) = self.hook.uid() {
            *OPEN.lock().unwrap_or_else(PoisonError::into_inner) = Some(uid);
        }
        match standard_event {
            StandardEvent::UIEvent(ui_event) => {
                // the root ends a capture when focus leaves
                if self.recording.is_some() && !sonamu_common::capturing() {
                    self.recording = None;
                }
                if self.recording.is_some() && matches!(ui_event, UIEvent::Key { .. }) {
                    self.record(ui_event);
                    self.hook.damage_window();
                    self.hook.damage_treeview();
                    return;
                }
                if self.editing.is_none() && handle_standard_keybinds(&ui_event, &self.hook) {
                    return;
                }
                match ui_event {
                    UIEvent::Key {
                        symbol: Some(symbol),
                        modifiers,
                        pressed: true,
                        ..
                    } => self.handle_key(symbol, modifiers),
                    UIEvent::Mouse(mouse) => self.handle_mouse(mouse.position, mouse.kind),
                    UIEvent::WindowResized(size) => {
                        self.size.set(size);
                        let max = ui::max_scroll(self.rows().len(), self.shown());
                        self.scroll = self.scroll.min(max);
                    }
                    _ => return,
                }
                self.hook.damage_window();
                self.hook.damage_treeview();
            }
            StandardEvent::Swipe(view) => {
                self.swipe = view;
                self.hook.damage_window();
            }
            StandardEvent::FocusChanged(false) => {
                self.recording = None;
                self.commit_editing();
                self.hook.damage_window();
                self.hook.damage_treeview();
            }
            _ => {}
        }
    }

    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree {
        self.size.set(container_size);
        let sections = self.editor.sections.len();
        let swiped = self.swipe.as_ref().and_then(|swipe| {
            let from = view_at(&swipe.from, sections)?;
            let to = match &swipe.to {
                Some(to) => Some(view_at(to, sections)?),
                None => None,
            };
            Some((swipe, from, to))
        });
        match swiped {
            Some((swipe, from, to)) => swipe::slide(
                container_size,
                swipe.vertical,
                swipe.progress,
                self.page(from, container_size),
                to.map(|to| self.page(to, container_size)),
            ),
            None => self.page(self.view, container_size),
        }
    }

    fn get_treeview(&self) -> WorldTree<String> {
        section_tree(&self.editor)
    }

    /// The section shown alone, else the inner root
    fn get_focus_path(&self) -> WorldTreePath {
        self.view.map_or_else(WorldTreePath::root, |section| {
            WorldTreePath::from_tree_path(&TreeNodePath(vec![section]))
        })
    }

    /// A section's row in the treeview shows it alone; the inner root, every section
    fn focus(&mut self, path: &WorldTreePath) {
        if let Some(view) = view_at(path, self.editor.sections.len()) {
            self.show(view);
        }
    }

    /// Space on a section's row in the treeview folds it
    fn toggle_collapsed(&mut self, path: &WorldTreePath) {
        let (TreeNodePath(steps), _) = path.split_world();
        if let [section] = *steps.as_slice()
            && section <= self.editor.sections.len()
        {
            self.fold(section);
            self.hook.damage_window();
            self.hook.damage_treeview();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sonamu_common::utils::tree::tree_node_path::TreeNodePath;

    fn editor(name: &str) -> (Editor, Keybinds) {
        let dir = std::env::temp_dir().join(format!("sonamu-rows-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let editor = Editor::open(&dir);
        let keybinds = Keybinds::from_json(&editor.value(&[KEYBINDS.to_owned()]));
        (editor, keybinds)
    }

    #[test]
    fn collapsed_sections_hide_their_rows() {
        let (editor, keybinds) = editor("collapse");
        let open = form_rows(&editor, &keybinds, &HashSet::new(), Open::default(), None);
        let keybinds_section = editor.sections.len();
        let collapsed = form_rows(
            &editor,
            &keybinds,
            &HashSet::from([0, keybinds_section]),
            Open::default(),
            None,
        );
        assert_eq!(collapsed[0], Row::Title(0));
        assert_eq!(collapsed[1], Row::Title(1));
        assert!(!collapsed.iter().any(|row| matches!(row, Row::Group(_))));
        assert!(collapsed.contains(&Row::KeybindsTitle));
        assert!(collapsed.len() < open.len());
    }

    #[test]
    fn picker_rows_follow_its_field() {
        let (editor, keybinds) = editor("picker");
        let open = Open {
            picker: Some((0, 0)),
            chooser: None,
        };
        let rows = form_rows(&editor, &keybinds, &HashSet::new(), open, None);
        let at = rows
            .iter()
            .position(|row| *row == Row::Field(0, 0))
            .unwrap();
        let lines: Vec<Row> = (0..PICKER_LINES).map(Row::Picker).collect();
        assert_eq!(rows[at + 1..=at + PICKER_LINES], lines[..]);
    }

    #[test]
    fn chooser_rows_follow_its_field() {
        let (editor, keybinds) = editor("chooser");
        let open = Open {
            picker: None,
            chooser: Some(((1, 0), 2)),
        };
        let rows = form_rows(&editor, &keybinds, &HashSet::new(), open, None);
        let at = rows
            .iter()
            .position(|row| *row == Row::Field(1, 0))
            .unwrap();
        assert_eq!(
            rows[at + 1..=at + 3],
            [Row::TypePath, Row::Chooser(0), Row::Chooser(1)]
        );
    }

    /// A section shown alone has its rows only, unfolded and without its title
    #[test]
    fn a_view_shows_its_section() {
        let (editor, keybinds) = editor("view");
        let keybinds_section = editor.sections.len();
        let collapsed = HashSet::from([1]);
        let section = form_rows(&editor, &keybinds, &collapsed, Open::default(), Some(1));
        assert_ne!(section.len(), 0);
        assert!(section.iter().all(|row| matches!(
            row,
            Row::Field(1, _) | Row::Item(1, ..) | Row::AddItem(1, _)
        )));
        let binds = form_rows(
            &editor,
            &keybinds,
            &HashSet::new(),
            Open::default(),
            Some(keybinds_section),
        );
        assert_eq!(binds.first(), Some(&Row::Group(0)));
        assert_eq!(binds.last(), Some(&Row::AddDefaults));
    }

    #[test]
    fn inner_paths_are_views() {
        let path = |steps: Vec<usize>| WorldTreePath::from_tree_path(&TreeNodePath(steps));
        assert_eq!(view_at(&path(vec![]), 5), Some(None));
        assert_eq!(view_at(&path(vec![5]), 5), Some(Some(5)));
        assert_eq!(view_at(&path(vec![6]), 5), None);
        assert_eq!(view_at(&path(vec![1, 0]), 5), None);
    }

    /// The treeview shows each section, Keybinds last, as an inner child
    #[test]
    fn treeview_lists_sections() {
        let (editor, _) = editor("treeview");
        let WorldTree::World(tree) = section_tree(&editor) else {
            panic!("no inner tree");
        };
        let labels: Vec<String> = (0..)
            .map_while(|index| tree.safe_get(&TreeNodePath(vec![index])))
            .map(|child| child.get_value().get_root_value().clone())
            .collect();
        let mut titles: Vec<String> = editor
            .sections
            .iter()
            .map(|section| section.title.clone())
            .collect();
        titles.push("Keybinds".to_owned());
        assert_eq!(labels, titles);
    }

    #[test]
    fn keybinds_changed_sees_order() {
        let a = serde_json::json!({"A": [], "B": []});
        let b = serde_json::json!({"B": [], "A": []});
        assert!(keybinds_changed(&a, &b));
        assert!(!keybinds_changed(&a, &a.clone()));
    }

    /// Opening Settings while one is open focuses it instead of making another
    #[test]
    fn second_open_focuses_the_first() {
        *OPEN.lock().unwrap() = Some(NodeUid::new());
        let initializer = SettingsEditorApplet::get_applet_spawner().create_initializer(&[]);
        *OPEN.lock().unwrap() = None;
        assert!(initializer.is_none());
    }
}
