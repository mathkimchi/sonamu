# Settings editor (#107)

Native applet `settings_editor` for newcomers: every setting as a form, applied as it changes. Opened by the hub's Settings row (`BarAction::OpenSettings` → `BarRequest::Open`), `sonamu-msg open settings_editor`, `sonamu.open`.

- `form.rs`: fields from `settings::schema()` (bool → toggle, number → clamped to `minimum`/`maximum`, enum → choice, `Color` → swatch, `string|null` → text (empty = null), string list → item rows, else read-only). Colors first; object settings get sections; the rest is "General". New settings show up without editing it.
- `editor.rs` (`Editor`): edits the user's raw JSON via `sonamu_common::settings_file` (only what differs from the defaults is kept; unknown keys and order stay). Re-reads the file before each write, so hand edits survive. Revert writes the file as it was at open (deletes it if there was none; refused if it couldn't be read). An executable `settings` makes it read-only.
- `keybinds.rs`: the Keybinds section's groups and `Draft`s. Unfinished binds (no keys, bad args) stay in the editor but aren't written. Records keys by holding a `sonamu_common::Capture` (`KEY_CAPTURE`: runner and root skip their bindings); dropped on Esc, the recorded key, unfocus, or close.
- `pastel.rs`: seven colors from one random hue.
- One editor: `OPEN` holds its `NodeUid`; opening again sends it `focus`.
- Writes reach Sonamu through the settings watcher (`settings_changed` → `Editor::reload`, `sync_keybinds`).
