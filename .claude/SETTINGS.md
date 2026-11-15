# Settings (#63)

- Config dir `$SONAMU_CONFIG`, else `$XDG_CONFIG_HOME/sonamu`. Executable `settings` → its stdout as JSON (timeout), else `settings.json`. Reference: `examples/config` (the dev machine symlinks it).
- Defaults only in `assets/default_settings.json` (`include_str!`); user JSON deep-merged over. A field missing there panics in `Settings::default()` (tests catch it). Schema `assets/settings.schema.json` is rewritten by test `schema_is_current` (fails once, commit it).
- `"watch": [paths]`: extra files to watch. Watching is via parent dirs, debounced 100 ms. Loop detection is off (`Watcher::run`): a program changing its own watch files reloads forever.
- `sonamu_common::settings`: `get()`/`reload()`/`subscribe()`; applets get `StandardApplet::settings_changed`. `font_size` and `mipmaps` are mirrored into `sonamu_ui` (it can't depend on `sonamu_common`).
- Reloads notify (#143), retried 10 s; on failure previous settings are kept.
- Non-obvious behavior:
  - `palette.bg` is only for hub/buffer line/help backgrounds (an opaque root background hid wallpaper layers). Other backgrounds: `Palette::raised`/`tinted`, translucent via `with_opacity` (0.8).
  - `always_frame`: a lone pane still gets border/gap/radius.
  - `frames_itself` panes (projects showing panes) aren't framed and grow by `gap`.
  - `keybinds` (#114): checked in `AppletRunner::handle_ui_event`, exact mods, `sh -c`. One bad entry fails the whole parse.
  - `terminal`/`browser`/`pdf_viewer` unset: env, else first installed of `TERMINALS`/`BROWSERS`/`PDF_VIEWERS` (`standard_keybinds.rs`).
  - `startup`: run once at start (`run_startup`).
  - `treeview.side`/`floating*` are parsed but unused since the status bar (#150).
- Gap: a settings program's background child holding stdout blocks reading after it exits.
