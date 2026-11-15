//! The settings editor (#107): every setting as a form, written to `settings.json` as it changes

pub mod editor;
pub mod form;
pub mod pastel;

use editor::{Editor, hex};
use form::{Field, FieldKind};
use serde_json::Value;
use sonamu_common::{
    sap::packets::StandardEvent,
    settings::{self, ERROR, Palette, Settings},
    utils::tree::world_tree::WorldTree,
};
use sonamu_sttk::{
    nodular_applet::{
        AppletContext, AppletSpawner, AppletSpawnerTrait, NodularAppletInitializer, StandardApplet,
    },
    standard_keybinds::handle_standard_keybinds,
    ui::{
        self, Rows,
        fields::{self, Edit, TextInput},
    },
};
use sonamu_ui::{
    display_units::DisplayContainerSize,
    ui_element::{Style, UIElement, UITree, font_size},
    ui_event::{KeySymbol, MouseEventKind, UIEvent},
};
use std::path::Path;

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
}

/// Header rows above the form: title with Revert, then the selected setting's description
const HEADER_ROWS: usize = 2;
const REVERT: &str = "Revert";

pub struct SettingsEditorApplet {
    editor: Editor,
    palette: Palette,
    size: DisplayContainerSize,
    selected: Option<usize>,
    hovered: Option<usize>,
    scroll: usize,
    scrolled: ui::ScrollSteps,
    /// The selected row's text being typed
    editing: Option<TextInput>,
    /// The last write's error
    error: Option<String>,
    hook: AppletContext,
}
impl SettingsEditorApplet {
    fn new(dir: &Path, hook: AppletContext) -> Self {
        Self {
            editor: Editor::open(dir),
            palette: settings::get().palette,
            size: DisplayContainerSize::new(800, 600),
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
        let mut rows = Vec::new();
        for (section_index, section) in self.editor.sections.iter().enumerate() {
            rows.push(Row::Title(section_index));
            if section.title == "Colors" && self.editor.read_only().is_none() {
                rows.push(Row::Generate);
            }
            for (field_index, field) in section.fields.iter().enumerate() {
                rows.push(Row::Field(section_index, field_index));
                if field.kind == FieldKind::TextList {
                    let count = self.items(field).len();
                    rows.extend((0..count).map(|item| Row::Item(section_index, field_index, item)));
                    rows.push(Row::AddItem(section_index, field_index));
                }
            }
        }
        rows
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
            Row::Title(_) | Row::Generate => None,
        }
    }

    fn items(&self, field: &Field) -> Vec<String> {
        self.editor
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

    fn list_rows(&self) -> Rows {
        Rows::new(HEADER_ROWS as f32 * ui::row_height()).scrolled(self.scroll)
    }

    fn shown(&self) -> usize {
        self.list_rows().fitting(self.size.height as f32)
    }

    fn report(&mut self, result: Result<(), String>) {
        self.error = result.err();
    }

    /// Enter or a click on `row`
    fn activate(&mut self, row: Row) {
        if self.editor.read_only().is_some() {
            return;
        }
        match row {
            Row::Title(_) => {}
            Row::Generate => {
                let result = self.editor.generate(pastel::random_hue());
                self.report(result);
            }
            Row::Field(section, field) => {
                let field = self.field(section, field).clone();
                let value = self.editor.value(&field.path);
                match &field.kind {
                    FieldKind::Toggle => {
                        let on = !value.as_bool().unwrap_or_default();
                        let result = self.editor.commit(&[field.path], &[Value::from(on)]);
                        self.report(result);
                    }
                    FieldKind::Choice(_) => self.cycle(&field, true),
                    FieldKind::Number { .. } | FieldKind::Color | FieldKind::Text { .. } => {
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
        }
    }

    /// Moves a choice to its next (`forward`) or previous option
    fn cycle(&mut self, field: &Field, forward: bool) {
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
                Edit::Commit => self.commit_editing(),
                Edit::Cancel => self.editing = None,
                Edit::Changed | Edit::Ignored => {}
            }
            return;
        }
        let rows = self.rows();
        let row = self.selected.and_then(|index| rows.get(index).copied());
        match symbol {
            KeySymbol::ArrowKeyDown | KeySymbol::ArrowKeyUp => {
                let down = matches!(symbol, KeySymbol::ArrowKeyDown);
                let mut next = ui::step(self.selected, down, rows.len());
                // titles are skipped, except at the ends
                while let Some(index) = next
                    && matches!(rows[index], Row::Title(_))
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
        self.size.width as f32 - ui::PAD - text_width(REVERT)
    }

    fn handle_mouse(&mut self, position: [f64; 2], kind: MouseEventKind) {
        let [x, y] = position;
        let list = self.list_rows();
        let count = self.rows().len();
        let row_at = list.row_at(y).filter(|&index| index < count);
        match kind {
            MouseEventKind::Motion => self.hovered = row_at,
            MouseEventKind::Button {
                button: ui::BTN_LEFT,
                pressed: true,
            } => {
                if y < f64::from(ui::row_height()) && x >= f64::from(self.revert_left()) {
                    self.commit_editing();
                    let result = self.editor.revert();
                    self.report(result);
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

    /// The selected setting's description, else a hint
    fn help(&self) -> String {
        let row = self
            .selected
            .and_then(|index| self.rows().get(index).copied());
        match row.and_then(|row| self.field_of(row)) {
            Some(field) if !field.help.is_empty() => field.help.replace('\n', " "),
            _ => "Up/Down: pick, Enter or click: change, Left/Right: options".to_owned(),
        }
    }

    fn header(&self, width: f32) -> Vec<UITree> {
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
        let title_end = ui::PAD + text_width("Settings") + ui::PAD;
        let mut parts = vec![
            header.text(ui::text("Settings", palette.fg), ui::PAD, title_end),
            header.text(status, title_end, revert_left - ui::PAD),
        ];
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
                    ui::text(self.help(), palette.inactive),
                    ui::PAD,
                    width - ui::PAD,
                )],
            ),
        ]
    }

    fn draw_row(&self, rows: &Rows, index: usize, row: Row, width: f32) -> UITree {
        let palette = &self.palette;
        let value_left = (width * 0.4).max(ui::PAD + 8.0 * font_size());
        let selected = self.selected == Some(index);
        let background = if selected {
            Some(ui::selected_bg(palette))
        } else if self.hovered == Some(index) && !matches!(row, Row::Title(_)) {
            Some(ui::raised_bg(palette))
        } else {
            None
        };
        let editing = self.editing.as_ref().filter(|_| selected);
        let label = |text: &str, color| rows.text(ui::text(text, color), ui::PAD, value_left);
        let value = |element| rows.text(element, value_left, width - ui::PAD);
        let parts = match row {
            Row::Title(section) => {
                vec![label(&self.editor.sections[section].title, palette.accent)]
            }
            Row::Generate => vec![label("✨ Generate pastel", palette.fg)],
            Row::Field(section, field) => {
                let field = self.field(section, field);
                let current = self.editor.value(&field.path);
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
                    (FieldKind::Color, None) => {
                        let color = current.as_str().and_then(|text| text.parse().ok());
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
                    (_, None) if current.is_null() => {
                        parts.push(value(ui::text("unset", palette.inactive)))
                    }
                    (_, None) => parts.push(value(ui::text(plain(&current), palette.fg))),
                }
                parts
            }
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
        };
        let tree = rows.row(index, width, background, parts);
        if matches!(row, Row::Title(_)) {
            tree
        } else {
            tree.with_pointer()
        }
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

impl StandardApplet for SettingsEditorApplet {
    fn settings_changed(&mut self, settings: &Settings) {
        self.palette = settings.palette.clone();
        self.editor.reload();
        self.hook.damage_window();
    }

    fn handle_standard_event(&mut self, standard_event: StandardEvent) {
        match standard_event {
            StandardEvent::UIEvent(ui_event) => {
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
                        self.size = size;
                        let max = ui::max_scroll(self.rows().len(), self.shown());
                        self.scroll = self.scroll.min(max);
                    }
                    _ => return,
                }
                self.hook.damage_window();
            }
            StandardEvent::FocusChanged(false) => {
                self.commit_editing();
                self.hook.damage_window();
            }
            _ => {}
        }
    }

    fn get_window_standard_applet(&self, container_size: DisplayContainerSize) -> UITree {
        let [width, height] = [container_size.width, container_size.height].map(|px| px as f32);
        let rows = self.list_rows();
        let all = self.rows();
        let shown = rows.fitting(height);
        let mut parts = self.header(width);
        for (index, &row) in all.iter().enumerate().skip(self.scroll).take(shown) {
            parts.push(self.draw_row(&rows, index, row, width));
        }
        parts.extend(ui::scrollbar(
            &self.palette,
            ui::scrollbar_x(width),
            [rows.top, height - rows.top],
            self.scroll,
            shown,
            all.len(),
        ));
        UITree::new_styled(
            UIElement::Backgrounded(ui::bg(&self.palette)),
            Style::absolute([0.0, 0.0], [width, height]),
            parts,
        )
    }

    fn get_treeview(&self) -> WorldTree<String> {
        WorldTree::Base("Settings".to_owned())
    }
}
