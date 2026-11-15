# Settings Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A native settings editor for newcomers (#107), plus keybinds as named groups shown as help sections.

**Architecture:** Keybinds become an ordered object of groups. A `sonamu_common::settings_file` module edits the user's raw JSON (diff from defaults, atomic write). The editor applet (`sonamu_standard_tabs::settings_editor`) builds its form from the settings schema; it has hand-built Colors (pastel generator) and Keybinds (record via a global `KEY_CAPTURE` flag) sections. New form widgets go in `sonamu_sttk::ui::fields`.

**Tech Stack:** Rust, serde_json (`preserve_order`), indexmap 2, schemars 1 (`indexmap2`).

**Spec:** `.claude/specs/2026-11-15-settings-editor-design.md`

## Global Constraints

- Follow `.claude/CLAUDE.md`: small, idiomatic, terse docs; never edit `docs/`, `README.md`, `DEVLOG.md`.
- Every commit passes the pre-commit hook (`cargo fmt`, clippy `-D warnings`). Commit messages end with the session's attribution lines.
- Each test touching the filesystem uses its own dir under `std::env::temp_dir()` (no new dev-deps); never the real config.
- Widgets follow `sonamu_sttk/src/ui/CLAUDE.md` (shades, `row_height`, hover `raised_bg`, `ScrollSteps`, scrollbar).
- The button is named **Revert**.
- `font_size` range 8–64.
- Default keybind groups, in order: Global, Windows, Tree, Traversal, Swipe.

## Review Focus

- A hand-written `settings.json` with keys the editor doesn't know (`$schema`, `watch`) and custom order: a save keeps them, in order (Task 4 test `set_keeps_unknown_keys_and_order`).
- `settings.json` edited by hand while the editor is open: the next editor change keeps the hand edit (Task 10 test `commit_rereads_file`).
- A settings program (`settings` executable) present: the editor writes nothing (Task 10 test `script_source_is_read_only`).
- Recording stuck on (editor closed or unfocused mid-record): keybinds work again (Task 5 test `capture_ends_on_drop`, Task 10 unfocus handling).
- A typed number beyond the schema range (eg font size 200): clamped, never written (Task 10 test `number_commit_clamps`).

---

### Task 1: Keybind groups in settings

**Files:**
- Modify: `Cargo.toml` (workspace deps), `sonamu_common/Cargo.toml`
- Modify: `sonamu_common/src/settings.rs` (`Settings::keybinds`, `keybind_command`, `parse`, tests)
- Modify: `sonamu_sar/src/runner.rs:737` (`run_binding`)
- Modify: `sonamu_sttk/src/standard_keybinds.rs:176` (test)
- Modify: `assets/default_settings.json`, `assets/settings.schema.json` (regenerated), `examples/config/settings.json`
- Modify: `.claude/SETTINGS.md` (`keybinds` bullet)

**Interfaces:**
- Produces: `Settings::keybinds: IndexMap<String, Vec<Keybind>>`; `Settings::all_keybinds(&self) -> impl Iterator<Item = &Keybind>` (groups in order, binds in order).

- [ ] **Step 1: Write failing tests** in `settings.rs` tests:

```rust
#[test]
fn keybinds_replace_defaults_whole() {
    let loaded = parse(r#"{"keybinds": {"Mine": [{"key": "x", "shell": "true"}]}}"#).unwrap();
    assert_eq!(loaded.settings.keybinds.keys().collect::<Vec<_>>(), ["Mine"]);
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
```
Update `example_config_parses` to `all_keybinds().count() > 1`.

- [ ] **Step 2: Run** `cargo test -p sonamu_common settings` — Expected: compile failure (no `keys()` on `Vec`).

- [ ] **Step 3: Implement**
  - Workspace: `serde_json = { version = "^1.0", features = ["preserve_order"] }`, `indexmap = { version = "2", features = ["serde"] }`, schemars feature `indexmap2`.
  - `keybinds: IndexMap<String, Vec<Keybind>>`; doc: "Global shortcuts in named groups (shown as help sections): Sonamu commands or shell commands. Given, it replaces the defaults (list the groups to keep)".
  - `all_keybinds`; use it in `keybind_command`, `runner::run_binding`, `default_keybinds_are_known`.
  - `parse`: if the user JSON has `keybinds`, remove `keybinds` from the defaults value before `merge`.
  - `default_settings.json`: regroup (Global: quit, help, buffer_mode; Windows: spawn_terminal, spawn_terminal_sibling, spawn_browser, spawn_browser_sibling, close, wrap_project, toggle_half; Tree: swap_sibling −1/+1, pluck_place, view_root, view_parent; Traversal: every `traverse`, same order; Swipe: the four `swipe`s). One bind per line as today.
  - `examples/config/settings.json`: same five groups, plus `"Media"` (the volume and brightness shell binds) and `"Screen"` (`grim`, toggle-record), binds unchanged.

- [ ] **Step 4: Run** `cargo test -p sonamu_common` twice (first run rewrites the schema and fails once), then `cargo test -p sonamu_sttk standard_keybinds` — Expected: PASS.

- [ ] **Step 5: Update `.claude/SETTINGS.md`** `keybinds` bullet: groups object, order kept (`preserve_order`), given replaces whole (`parse`), first match across groups in order.

- [ ] **Step 6: Commit** "Keybinds in named groups (#107)".

### Task 2: Help page sections per group

**Files:**
- Modify: `sonamu_sttk/src/man_page.rs` (`keybind_rows`, `rows`, `ManPageApplet::keybinds`)

**Interfaces:**
- Consumes: `Settings::keybinds` (Task 1).
- Produces: `fn keybind_sections(settings: &Settings) -> Vec<(String, Vec<(String, String)>)>` replacing `keybind_rows`.

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn sections_follow_keybind_groups() {
    let titles: Vec<_> = super::keybind_sections(&Settings::default()).into_iter().map(|(t, _)| t).collect();
    assert_eq!(titles, ["Global", "Windows", "Tree", "Traversal", "Swipe"]);
}
```
Adapt `default_keybinds_described` to iterate every section's entries.

- [ ] **Step 2: Run** `cargo test -p sonamu_sttk man_page` — Expected: FAIL (no `keybind_sections`).
- [ ] **Step 3: Implement**: the keybind sections come first, then `MAN_PAGE`; section titles become `String` in `rows`.
- [ ] **Step 4: Run** the same — Expected: PASS.
- [ ] **Step 5: Commit** "Help page: a section per keybind group (#107)".

### Task 3: Schema ranges and `settings::schema()`

**Files:**
- Modify: `sonamu_common/src/settings.rs`, `assets/settings.schema.json`

**Interfaces:**
- Produces: `pub fn schema() -> serde_json::Value` (`schema_for!(Loaded)` as a value; `schema_is_current` uses it).

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn font_size_range_in_schema() {
    let font = &schema()["properties"]["font_size"];
    assert_eq!((font["minimum"].as_f64(), font["maximum"].as_f64()), (Some(8.0), Some(64.0)));
}
```
- [ ] **Step 2: Run** `cargo test -p sonamu_common font_size_range` — Expected: FAIL.
- [ ] **Step 3: Add `#[schemars(range(min, max))]`**: `font_size` 8–64, `gap` 0–64, `radius` 0–64, `border.width` 0–16, `treeview.width` 10–60, `swipe.distance_x`/`distance_y` 100–5000, `swipe.rubber_band` 0–1, `swipe.stiffness` 100–5000, `swipe.damping_ratio` 0.1–2, `notifications.timeout` 0–60. Check each default is inside its range (add `defaults_within_ranges` test walking the schema's `minimum`/`maximum` against `DEFAULTS`).
- [ ] **Step 4: Run** `cargo test -p sonamu_common` twice — Expected: PASS.
- [ ] **Step 5: Commit** "Settings schema ranges (#107)".

### Task 4: Editing the settings file

**Files:**
- Create: `sonamu_common/src/settings_file.rs`
- Modify: `sonamu_common/src/lib.rs` (`pub mod settings_file;`), `sonamu_common/src/settings.rs` (make `parse` `pub(crate)`)

**Interfaces:**
- Produces:
  - `pub enum Source { Json(PathBuf), Program(PathBuf) }`, `pub fn source(dir: &Path) -> Source` (program if `settings` is executable, like `load`).
  - `pub fn read(path: &Path) -> Result<Option<Value>, LoadError>` (`None`: no file).
  - `pub fn defaults() -> Value`.
  - `pub fn value_at(user: &Value, path: &[&str]) -> Value` — user's, else default's, else `Null`.
  - `pub fn set(user: &mut Value, path: &[&str], value: Value)` — equal to the default: removed, and empty parent objects too; else inserted (creating objects), keeping key order.
  - `pub fn check(user: &Value) -> Result<(), LoadError>` — `parse` of it.
  - `pub fn write(path: &Path, user: Option<&Value>) -> io::Result<()>` — `None` deletes; else `format` to `<path>.tmp` then rename; creates the parent dir.
  - `pub fn format(user: &Value) -> String` — pretty, 2 spaces, trailing newline; each object directly inside a keybind group's array on one line (`{"mods": ["logo"], "key": "t", "command": "spawn_terminal"}`).

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn set_default_removes_key_and_empty_parent() {
    let mut user = json!({"border": {"width": 3.0}});
    set(&mut user, &["border", "width"], defaults()["border"]["width"].clone());
    assert_eq!(user, json!({}));
}

#[test]
fn set_keeps_unknown_keys_and_order() {
    let mut user = json!({"$schema": "x", "watch": ["/a"], "gap": 4.0});
    set(&mut user, &["font_size"], json!(30.0));
    let keys: Vec<_> = user.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["$schema", "watch", "gap", "font_size"]);
}

#[test]
fn keybinds_one_per_line() {
    let user = json!({"keybinds": {"G": [{"key": "x", "shell": "true"}]}});
    assert!(format(&user).contains("\n      {\"key\": \"x\", \"shell\": \"true\"}\n"));
    assert_eq!(serde_json::from_str::<Value>(&format(&user)).unwrap(), user);
}

#[test]
fn check_rejects_bad_keybind() {
    assert!(check(&json!({"keybinds": {"G": [{"key": "x"}]}})).is_err());
}

#[test]
fn write_none_deletes() { /* write Some then None in a temp dir; file gone */ }
```
- [ ] **Step 2: Run** `cargo test -p sonamu_common settings_file` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** per Interfaces.
- [ ] **Step 4: Run** the same — Expected: PASS.
- [ ] **Step 5: Commit** "settings_file: edit the user's settings JSON (#107)".

### Task 5: Key capture and recording

**Files:**
- Modify: `sonamu_common/src/lib.rs` (static), `sonamu_common/src/settings.rs` (`NamedKey::of`, `Keybind::recorded`)
- Modify: `sonamu_sar/src/runner.rs` (`run_binding`)
- Modify: `sonamu_sttk/src/nodular_applet/root_node_applet.rs` (`is_keybind`, `handle_event`)

**Interfaces:**
- Produces:
  - `pub static KEY_CAPTURE: AtomicBool` beside `BUFFER_MODE` (doc: "an applet is recording a keybind: every key goes to the focused applet").
  - `pub struct Capture;` in `sonamu_common` — `Capture::start() -> Capture` sets it, `Drop` clears it.
  - `NamedKey::of(KeySymbol) -> Option<NamedKey>`.
  - `Keybind::recorded(symbol: Option<KeySymbol>, raw_keycode: u32, modifiers: KeyModifiers) -> Option<(Vec<String>, Key)>` — `None` for modifier keys (evdev 29, 42, 54, 56, 97, 100, 125, 126); key: named, else lowercase char, else `Key::Code`; mods in `ctrl, alt, shift, logo` order.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn recorded_keys() {
    let logo = KeyModifiers { logo: true, ..KeyModifiers::NONE };
    assert_eq!(Keybind::recorded(Some(KeySymbol::Char('T')), 20, logo), Some((vec!["logo".into()], Key::Char('t'))));
    assert_eq!(Keybind::recorded(Some(KeySymbol::Escape), 1, logo), Some((vec!["logo".into()], Key::Named(NamedKey::Escape))));
    assert_eq!(Keybind::recorded(None, 115, KeyModifiers::NONE), Some((vec![], Key::Code(115))));
    assert_eq!(Keybind::recorded(None, 125, logo), None);
}

#[test]
fn capture_ends_on_drop() {
    drop(Capture::start());
    assert!(!KEY_CAPTURE.load(Relaxed));
}
```
And in `root_node_applet` tests: `is_keybind` is true for a plain `a` while a `Capture` lives.
- [ ] **Step 2: Run** `cargo test -p sonamu_common recorded_keys capture_ends` — Expected: FAIL.
- [ ] **Step 3: Implement**: `run_binding` returns `false` while capturing (before Luau and settings binds); `is_keybind` returns `true`; `handle_event` skips buffer-mode, `RootKeybind` and `Traversal` checks while capturing (the key falls through to the focused applet).
- [ ] **Step 4: Run** `cargo test -p sonamu_common && cargo test -p sonamu_sttk root_node_applet` — Expected: PASS.
- [ ] **Step 5: Commit** "KEY_CAPTURE: record a keybind's keys (#107)".

### Task 6: Keybind command catalog

**Files:**
- Modify: `sonamu_sttk/src/standard_keybinds.rs`

**Interfaces:**
- Produces:
  - `pub enum ArgKind { None, Program, Offset, Direction, Traversal }`; `ArgKind::accepts(self, args: &[serde_json::Value]) -> bool` (Traversal via `Traversal::from_args`; Direction: left/right/up/down; Offset: one integer; Program: one string).
  - `pub struct CommandSpec { pub name: &'static str, pub args: ArgKind }`; `pub const KEYBIND_COMMANDS: &[CommandSpec]` — every command `default_keybinds_are_known` lists, plus `spawn`/`spawn_sibling` (Program).

- [ ] **Step 1: Rewrite `default_keybinds_are_known`** to require each default command be in `KEYBIND_COMMANDS` with `accepts(&args)`; add `catalog_commands_are_handled` asserting every catalog name is one `standard_keybind`, `RootKeybind`, `traverse`, `pluck_place` or `buffer_mode` handles (move the existing name list into the catalog).
- [ ] **Step 2: Run** `cargo test -p sonamu_sttk standard_keybinds` — Expected: FAIL (no catalog).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** "Keybind command catalog (#107)".

### Task 7: Form widgets in the UI kit

**Files:**
- Create: `sonamu_sttk/src/ui/fields.rs`; Modify: `sonamu_sttk/src/ui/mod.rs` (`pub mod fields;`), `sonamu_sttk/src/ui/CLAUDE.md` (Parts)

**Interfaces:**
- Produces:
  - `pub struct TextInput { pub text: String, cursor: usize }`; `TextInput::new(text) -> Self`; `handle_key(&mut self, symbol: KeySymbol, modifiers: KeyModifiers) -> Edit`; `pub enum Edit { Ignored, Changed, Commit, Cancel }` (Left/Right move, Backspace, chars insert, Enter commit, Esc cancel); `draw(&self, focused: bool, palette: &Palette, width: f32) -> UITree` (cursor shown when focused).
  - `pub fn toggle(on: bool, palette: &Palette) -> UITree`.
  - `pub fn choice(value: &str, palette: &Palette) -> UITree` (drawn `‹ value ›`; clicks/Left/Right cycle, handled by the caller with `cycle(options, current, forward) -> usize`).
  - `pub fn swatch(color: Color, palette: &Palette) -> UITree` (`row_height` square, `guide` frame).

- [ ] **Step 1: Write failing tests** for `TextInput`: insert at cursor after Left, Backspace at 0 does nothing, Enter → `Commit`, Esc → `Cancel`; `cycle` wraps both ways.
- [ ] **Step 2: Run** `cargo test -p sonamu_sttk fields` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Add a Parts bullet** in `ui/CLAUDE.md`; **commit** "UI kit form widgets (#107)".

### Task 8: Form from the schema

**Files:**
- Create: `sonamu_standard_tabs/src/settings_editor/mod.rs` (module only for now), `sonamu_standard_tabs/src/settings_editor/form.rs`
- Modify: `sonamu_standard_tabs/src/lib.rs`

**Interfaces:**
- Consumes: `settings::schema()` (Task 3), `settings_file::defaults()` (Task 4).
- Produces: `pub struct Section { pub title: String, pub fields: Vec<Field> }`; `pub struct Field { pub path: Vec<String>, pub label: String, pub help: String, pub kind: FieldKind }`; `pub enum FieldKind { Toggle, Number { min: Option<f64>, max: Option<f64>, integer: bool }, Choice(Vec<String>), Color, Text { nullable: bool }, TextList, ReadOnly }`; `pub fn sections(schema: &Value) -> Vec<Section>`.
- Rules: resolve `$ref` into `$defs`; enums from `enum` or `oneOf` of `const`s; `Color` def → `Color`; `["string","null"]` → `Text { nullable: true }`; array of strings → `TextList`; anything else (tuples, `watch`, `restore_programs`) → `ReadOnly`. Label: path's last key, `_` → space, capitalized. Help: the property's `description`, else its def's. Sections: "Colors" first (every `Color` field), then one per top-level key in schema order (objects become one section; plain top-level fields gather in "General"); `keybinds` skipped (Task 11).

- [ ] **Step 1: Write failing tests**: `every_setting_has_a_field` (each leaf path of `defaults()` except under `keybinds` is some field's path); kinds: `always_frame` Toggle, `font_size` Number 8–64, `buffer_line` Choice containing `"always"`, `palette.bg` Color in "Colors", `terminal` Text nullable, `startup` TextList, `restore_programs` ReadOnly.
- [ ] **Step 2: Run** `cargo test -p sonamu_standard_tabs form` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** "Settings editor form from the schema (#107)".

### Task 9: Pastel palette

**Files:**
- Create: `sonamu_standard_tabs/src/settings_editor/pastel.rs`

**Interfaces:**
- Produces: `pub fn pastel(hue: f32) -> [(&'static [&'static str], Color); 7]` (paths into settings), `pub fn random_hue() -> f32` (from `SystemTime` nanos; no new dep).
- HSL (hue degrees, s, l): `palette.bg` (h, .25, .08), `palette.fg` (h, .20, .92), `palette.accent` (h, .60, .80), `border.focused` (h+30, .60, .78), `palette.inactive` (h, .08, .55), `border.unfocused` (h, .10, .28), `border.treeview` (h, .15, .45); alpha FF.

- [ ] **Step 1: Write failing tests**: for hues 0, 120, 359.9: fg/bg contrast ratio ≥ 7 (WCAG relative luminance), accent lighter than inactive, all seven paths distinct and matching the settings' color fields (`form::sections` Colors section paths).
- [ ] **Step 2: Run** `cargo test -p sonamu_standard_tabs pastel` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** "Pastel palette generator (#107)".

### Task 10: Editor applet

**Files:**
- Modify: `sonamu_standard_tabs/src/settings_editor/mod.rs`
- Modify: `sonamu_sde/src/main.rs` (register `"settings_editor"`)

**Interfaces:**
- Consumes: Tasks 4, 7, 8, 9.
- Produces: `SettingsEditorApplet::get_applet_spawner() -> AppletSpawner`; `SettingsEditorApplet::new(dir: PathBuf, hook)`; state logic separate from drawing in `struct Editor { dir, baseline: Option<Value>, user: Value, read_only: bool, sections, .. }` with `Editor::open(dir: &Path) -> Editor`, `Editor::commit(&mut self, path: &[&str], value: Value) -> Result<(), String>` (re-reads the file, `set`, `check`, `write`), `Editor::revert(&mut self) -> io::Result<()>` (writes `baseline`), `Editor::generate(&mut self, hue: f32)` (one write for all seven).
- Behavior: header row pinned at top: title, notice when read-only ("Settings come from a script: <path>"), last error, **Revert** button. Below, scrolled sections (`ui::Rows`, `ScrollSteps`, scrollbar). Up/Down select a field, Enter/click edits; Toggle and Choice commit at once; Number/Text commit on Enter (or selection moving away); Number clamps to `min`/`max` and rounds when `integer`. Colors section has a "Generate pastel" button. `settings_changed` refreshes `user` from disk. Standard keybinds handled except while a text field or recording is active. Title "Settings"; `save` → `None`.

- [ ] **Step 1: Write failing tests** (each in its own temp dir):
  - `commit_writes_diff`: commit `font_size` 30 → file is `{"font_size": 30.0}`.
  - `commit_rereads_file`: write `{"gap": 4.0}` by hand after `open`, commit `font_size` → file has both.
  - `revert_restores_baseline`: open with no file, commit, revert → no file.
  - `script_source_is_read_only`: executable `settings` in dir → `commit` errors, nothing written.
  - `number_commit_clamps`: committing `font_size` text `"200"` writes 64.
  - `generate_writes_seven_colors`.
- [ ] **Step 2: Run** `cargo test -p sonamu_standard_tabs settings_editor` — Expected: FAIL.
- [ ] **Step 3: Implement** `Editor`, then the applet drawing and input.
- [ ] **Step 4: Run** — Expected: PASS. Then headless (`.claude/TESTING.md`): `sonamu-msg open settings_editor`, screenshot, change font size and a toggle, check the temp config's `settings.json`.
- [ ] **Step 5: Commit** "Settings editor applet (#107)".

### Task 11: Keybinds section

**Files:**
- Create: `sonamu_standard_tabs/src/settings_editor/keybinds.rs`; Modify: `settings_editor/mod.rs`

**Interfaces:**
- Consumes: `KEYBIND_COMMANDS`, `ArgKind` (Task 6), `Capture`, `Keybind::recorded` (Task 5), `Editor::commit` (Task 10).
- Produces: `pub struct Keybinds { groups: Vec<(String, Vec<Draft>)> }`; `Draft { mods: Vec<String>, key: Option<Key>, action: DraftAction }`; `DraftAction { Command { name: String, args: Vec<Value> }, Shell(String) }`; methods `from_json(&Value)`, `to_json(&self) -> Value` (only valid drafts: key set, command known and `accepts` args, shell non-empty), `add_group`, `rename_group`, `move_group`, `delete_group`, `add_bind(group)`, `delete_bind`, `move_bind(from: (usize, usize), to: (usize, usize))` (within and between groups), `add_defaults()` (appends default groups whose names are missing).
- Behavior: rows per group (name, ▲▼, delete) and bind (keys label, action picker, args fields per `ArgKind`, Record, delete). Record: `Capture::start()` held in the applet; next key → `Keybind::recorded`; Esc cancels; dropped on `FocusChanged(false)`. Every valid change commits `["keybinds"]` with `to_json()` (equal to defaults → removed by `set`). Starts from the defaults when the user has no `keybinds`.

- [ ] **Step 1: Write failing tests**: `to_json` round-trips defaults; an incomplete draft (no key) is left out of `to_json`; `move_bind` across groups; `add_defaults` restores a deleted default group at the end and leaves present ones alone; `rename_group` keeps position.
- [ ] **Step 2: Run** `cargo test -p sonamu_standard_tabs keybinds` — Expected: FAIL.
- [ ] **Step 3: Implement** model, then rows and input.
- [ ] **Step 4: Run** — Expected: PASS. Headless: record a bind, check the file and that Super keys work after Esc.
- [ ] **Step 5: Commit** "Settings editor: keybinds (#107)".

### Task 12: Opening the editor

**Files:**
- Modify: `sonamu_common/src/sap/packets.rs` (`BarRequest::Open(OpenApplet)`), `sonamu_sar/src/runner.rs` (forward as `StandardEvent::Open`), `sonamu_sttk/src/nodular_applet/status_bar/hub.rs` (Settings row clickable, `palette.fg`), status bar applet click handling, `sonamu_sttk/src/nodular_applet/root_node_applet.rs` (on `Open` of `settings_editor`: focus an existing one in the root tree instead)

- [ ] **Step 1: Write failing test** in `root_node_applet` tests: two `Open { applet: "settings_editor" }` events leave one such node, focused.
- [ ] **Step 2: Run** `cargo test -p sonamu_sttk root_node_applet` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS. Headless: click the hub's Settings row twice.
- [ ] **Step 5: Commit** "Hub Settings row opens the settings editor (#107)".

### Task 13: Docs and cleanup

**Files:**
- Create: `sonamu_standard_tabs/src/settings_editor/CLAUDE.md` (writes diff via `settings_file`, re-read before write, Revert baseline, read-only for scripts, `KEY_CAPTURE`, form rules)
- Modify: `.claude/CLAUDE.md` (Docs list), `.claude/SETTINGS.md` (ranges, `settings_file`), `.claude/COMMANDS.md` (`buffer_mode`/`KEY_CAPTURE` note), `sonamu_sttk/src/nodular_applet/status_bar/CLAUDE.md` (drop "hub Settings row placeholder")
- Delete: `.claude/specs/2026-11-15-settings-editor-design.md`, `.claude/specs/2026-11-15-settings-editor-plan.md`

- [ ] **Step 1: Write the docs** (terse).
- [ ] **Step 2: Run** `cargo test --workspace` and `cargo machete` — Expected: PASS, no unused deps.
- [ ] **Step 3: Commit** "Settings editor docs; drop spec and plan (#107)".
