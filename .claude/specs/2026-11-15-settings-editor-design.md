# Settings editor (#107)

## Intent

Newcomers change every setting without seeing JSON. The editor writes `settings.json`; the existing watcher reloads it, so changes apply as you edit.

Also: keybinds get named groups, shown as sections in the help page.

## Goals

- Native applet `settings_editor` covering every setting, opened from the status bar hub's Settings row (today a placeholder) and by `sonamu-msg open settings_editor` / `sonamu.open`.
- Fields generated from the settings schema; hand-built sections for Keybinds and Colors.
- Live: each committed change writes `settings.json`. An always-visible **Revert** button puts the file back as it was when the editor opened (deletes it if there was none).
- Pastel palette generator in Colors.
- Keybinds as named groups (object, ordered); help page shows one section per group.
- Regroup `examples/config/settings.json`.

## Non-goals

- Dragging the treeview width (dropped).
- Editing an executable `settings`: the editor shows the settings come from a script and is read-only.
- Editing `init.luau`.
- Backward compatibility with a flat `keybinds` list: it fails to parse and the reload notification (#143) says so.

## Anti-goals

- Losing what the user wrote: keys the editor doesn't know (`$schema`, `watch`), key order, and values left untouched must survive a save.
- A newcomer stuck with an unusable UI (eg font size 200): Revert is always on screen.
- An editor that falls behind the settings: a new setting must show up without editing the editor.

## Approaches

- **A. Schema-driven form (picked).** Walk the schema (`schemars`, already derived on `Settings`); a widget per type; doc comments are the help text. Hand-built sections only for Keybinds and Colors. New settings appear for free.
- B. Hand-built form per setting. Friendlier ordering and labels, but must be edited with every new setting and drifts.
- Rejected: JSON text editor with validation (still raw JSON for newcomers).

## Decided

### Keybind groups

```json
"keybinds": {
  "Windows": [
    {"mods": ["logo"], "key": "t", "command": "spawn_terminal"}
  ],
  "Traversal": [...]
}
```

- Group names are free text. Order is the file's: enable `serde_json`'s `preserve_order` (workspace-wide).
- `keybinds` given replaces the defaults whole: `merge` special-cases it (objects are otherwise deep-merged).
- Matching still walks every bind in order, groups in order, first match wins.
- Default groups (from `docs/usage.md`):
  - Global: quit, help, buffer_mode
  - Windows: spawn_terminal(_sibling), spawn_browser(_sibling), close, wrap_project, toggle_half
  - Tree: swap_sibling ±1, pluck_place, view_root, view_parent
  - Traversal: every `traverse`
  - Swipe: the four `swipe`s
- Help page (`man_page.rs`): one section per group, in order, replacing the single "Keybinds" section.

### Editor behavior

- Lives in `sonamu_standard_tabs` beside the file manager; registered in `sonamu_sde/src/main.rs`.
- Edits the user's JSON as a `serde_json::Value` (`Settings` is deserialize-only), never the merged settings.
- A field set back to its default is removed from the file (and empty parent objects with it); other keys stay as they are. Keybinds: the whole object is written once edited; removed if equal to the defaults.
- The Keybinds section starts from the defaults when the user has none, so editing never drops them. Groups: add, rename, reorder, delete; "Add defaults" restores missing default groups.
- Script settings (`settings` executable present): notice, fields read-only.
- Reads `settings.json` again before each write, so hand edits made meanwhile survive. Revert's baseline: the file when this editor opened. Opening Settings again focuses the open editor instead of making a second one.
- Number fields clamp to the schema's `minimum`/`maximum`, set beside each field with `#[schemars(range(min, max))]` (eg `font_size` 8–64). Loading doesn't enforce them. Revert stays pinned at the top.

### Editing a bind

- Keys: "Record", then press the combo. A global `KEY_CAPTURE` flag (like `BUFFER_MODE`) makes `run_binding` (settings shell binds and Luau `keymap.set`) and the root's keybinds (`is_keybind` true, root keybinds skipped) pass every key to the focused editor. Capture ends on the first non-modifier key, Esc (cancels), or focus leaving the editor, so it can't stay stuck.
- Action: picker of the known commands, each with its own argument fields (eg `swipe` direction, `traverse` kind and n, `spawn` program), or "Shell command" with a text field.
- Binds can be added, deleted, and moved within and between groups.
- A half-made bind isn't written until it's valid (one bad entry fails the whole parse).

### Colors

- Seven colors: `palette.{bg, fg, accent, inactive}`, `border.{focused, unfocused, treeview}`. Each is a swatch plus hex field.
- "Generate pastel" fills all seven from a random base hue: dark tinted `bg`, near-white `fg`, pastel `accent` and `border.focused`, muted greys for `inactive`/`unfocused`/`treeview`. Writes like any other change (Revert undoes it).

### UI kit

New widgets in `sonamu_sttk::ui` (for any applet): text field (single line, commits on Enter or focus loss), number field, toggle, picker, color field. They follow `ui/CLAUDE.md` (shades, `row_height`, hover).

## Left to the builder

- Schema → widget mapping beyond the obvious (bool → toggle, number → number field, enum → picker, `Color` → color field, `string | null` → text field where empty is null, list of strings → rows). An unhandled shape shows its JSON read-only with "edit in settings.json".
- Writing: atomic (temp file + rename) into the config dir, creating it if needed.
- Output formatting: pretty JSON, but a keybind object stays on one line.
- How the hub row opens the applet (through the root, like other bar commands).
- Validating against the real parser before writing; read-only display for shapes not mapped (`watch`, `restore_programs`, `treeview.floating*`); alpha in color fields; where the applet opens.
