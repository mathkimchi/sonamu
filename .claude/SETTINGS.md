# Settings (issue #63)

Scope for now: cosmetic settings only: palette, border (width, focused/unfocused color), gap, radius, font size. Cursor dropped for now.

## Source

- Config dir: `$XDG_CONFIG_HOME/sonamu` (default `~/.config/sonamu`). The repo's `examples/config` is the reference config; the dev machine symlinks `~/.config/sonamu -> examples/config`.
- If `settings` in the config dir is executable, sonamu runs it and parses its stdout as JSON. Otherwise it reads `settings.json`.
- `examples/config/settings` is a python script that builds the palette and border colors from DankMaterialShell (`~/.cache/DankMaterialShell/dms-colors.json`, current `mode`) and radius from `cornerRadius` in `~/.config/DankMaterialShell/settings.json`, and watches both. Font size is 24 px times the first bar's `barConfigs[0].fontScale` (the scale the user changes in DMS; the global top-level `fontScale` isn't in the file, so it stays the 1.0 default). Border width is DMS's "Focused Border" (workspace appearance): `workspaceFocusedBorderThickness` (default 2) if `workspaceFocusedBorderEnabled`, else 0. Gap is fixed in the script (DMS only has per-compositor ones, eg `niriLayoutGapsOverride`, hidden outside niri).
- The program can be anything; the intended Rust form is a small binary depending on a `sonamu_settings` crate that builds `Settings` and prints it as JSON.
- Not a dylib: JSON over a pipe has no ABI coupling and a crashing/hanging program can't take down the compositor.

## Format

- `Settings` derives `serde` + `schemars::JsonSchema`; the schema is written out so `"$schema"` gives editor autocomplete.
- Every field is `#[serde(default)]`; partial files are fine.
- Optional top-level `"watch": [paths]`: extra files to watch (eg the file colors are generated from). Not part of the stored settings.

## Reload

- Watch the settings file/program and every `watch` path (`notify`). On change: re-run/re-read.
- Program gets a timeout. On spawn failure, timeout, non-zero exit or parse error: keep the previous settings and log.
- Also the command hub's `reload_settings` (`settings::reload()` on a thread).
- Settings live in a global in `singularity_common` readable from every applet thread; a settings-changed event is broadcast so applets redraw.

## Implemented

- `singularity_common::settings`: `Settings` (palette, border, gap, radius, font_size), `load(dir)`, `config_dir()` (`$SONAMU_CONFIG` overrides), global `get()`/`reload()`/`subscribe()`. `load_and_watch()` (called in `singularity_sde` main) loads once, then a `notify` thread reloads on changes. Applets get them through `StandardApplet::settings_changed` (#94); the root and the node tree still read `settings::get()` when they draw.
- Watching goes through parent dirs (non-recursive), so atomically replaced files are seen. Watched: `settings`, `settings.json` (only while there is no program), and `watch` paths (relative ones resolve against the config dir). Changes are debounced 100 ms.
- A change during a program run (+100 ms) causes another reload.
- Loop detection is commented out in `Watcher::run`: it ignored a `watch` path that changed during program runs (first after 1 run, then after 3 in a row), but DMS writes its `settings.json` on theme changes and continuously while a slider is dragged, so it kept ignoring DMS's settings. A program that changes its own `watch` files now reloads forever.
- Broadcast: on an actual change (`!=`), each `subscribe`d calloop sender is pinged; dead ones are dropped. `AppletClient` and the root applet subscribe. `AppletClient` calls `StandardApplet::settings_changed` (default: nothing) once before the first draw and on each change, then damages the surface. Always delivered, no subscribing per applet.
- `StandardRequest::GetSettings` is answered with `StandardEvent::SettingsChanged(Settings)` by the holder (`NodeHolder`, the runner for the root, the file manager for its documents); `AppletClient` turns that event into `settings_changed` too. Nothing sends it yet, since every applet gets the settings anyway.
- `Color` parses/deserializes `#rrggbb`/`#rrggbbaa`.
- Defaults reproduce the old hardcoded look (focus-inside border green, traversal yellow, unfocused #3b4049, black bg, 46 px font).
- Known gap: if the program leaves a background child holding stdout open, reading blocks after it exits (timeout only covers the program itself).

## What each setting drives

- `palette.bg`: unused. It was the root's opaque background, but that hid background layer surfaces (DMS's wallpaper) behind transparent windows and gaps.
- `palette.fg`: treeview and project overview text.
- `palette.accent`: border and treeview marker when focus is on a node itself (traversing).
- `palette.inactive`: treeview text of inactive nodes.
- `border.focused`: border and treeview marker when focus is inside the pane's applet. `border.unfocused`: other panes. `border.width`: pane borders.
- `gap`: around the tiled area and between split siblings (`NodeTree::push_panes`). Nested trees (projects) add their own.
- `radius`: pane corners; the pane's content (rects, char grids, wl textures) is clipped to the rounded inside; glyphon text only to its bounding rect.
- `font_size`: physical px (Sonamu's UI isn't scaled). Mirrored into `sonamu_ui::ui_element::font_size()` on load, since `sonamu_ui` can't depend on `singularity_common`; used by glyphon text, char grid cell size (width = size / 1.933, ceil), and row heights in treeview, project overview, file manager and the pluck ghost.
- `always_frame` (default true): a pane alone in its tree also gets border, gap and radius, and the treeview is framed like a pane (gap left/top/bottom, the panes' gap on its right; `border.unfocused`; mouse coords shifted by gap + border in `RootNodeApplet::handle_mouse`). False: a lone pane fills its tree and the treeview is flush. The treeview's border is `border.treeview` (DMS `secondary` in the example config).
- `treeview.side` (`left`, `right`; default `left`): where a pinned treeview's column is (20% of the width; the panes
  get the rest). `treeview.floating` (`center`, `side`; default `center`): where an unpinned one floats over the panes
  (only while traversing): 40% x 70% of the window in the middle, or the column's place at `side`, inset by `gap`.
  Whether it's pinned is not a setting but part of the session, toggled by the ●/○ pin button in its top right corner
  (`on_pin_button`, handled in `RootNodeApplet::handle_mouse` before the treeview's own handling).
- A pane whose applet `frames_itself` (published in `Treeview`; a split, or a project showing its panes, not its overview) is never framed (no border, no radius) and grows by `gap` on every side: its own tree frames its panes and adds its own outer gap, so leaf content gets exactly one frame and one gap at any nesting depth. `NodeTree::update_layout` reads only that flag, never the applet's type. Such a node's focus then only shows in the treeview.
- `keybinds` (#114): `[{"mods": ["logo"], "key": "t", "command": "foot"}]`; `key` is a char (case-insensitive) or an evdev code. Checked in `AppletRunner::handle_ui_event` after layer surfaces and Alt+P; mods must match exactly; runs `sh -c`. Only in the settings the compositor actually loads (with an executable `settings`, `settings.json` is ignored), and one bad entry fails the whole parse (previous settings kept, error logged). The example config binds volume and mute (wpctl), brightness (brightnessctl) and Print (`dms screenshot`, grim in a comment).
- File manager and code editor (#126): text from `palette` (`fg`, `inactive` for dim text, `accent` for the active/open file and the normal mode cursor), backgrounds made translucent with the treeview's `with_opacity` (a constant 0.8, not a setting). The file manager's directory names stay blue (no palette color for them).
- Not themed yet: command hub/help and the other applets keep their own constants.

## Rendering changes

- Colors are sRGB everywhere now. Rects and char grids used to write `Color`s straight to the sRGB surface (so they acted linear and showed lighter); their shaders now decode sRGB. Hand-tuned "drawn as linear" constants (treeview, command hub, file manager, pluck ghost) were converted to the sRGB they showed as, so they look the same. Plain named colors like `Color::DARK_GRAY` used as rect/char grid colors (eg in editors) now show darker, as named.
- The rect shader now honors `corner_radius` (antialiased SDF). `UIElement::Bordered { color, radius }` (`UIElement::border(color)` for square); radius > 0 clips its children.
- `PrimitiveScene::elements` carry an `Option<Clip>`; rect, image and char grid shaders multiply by clip coverage.

## Issues found

- Couldn't screenshot: a nested winit Sonamu inside a running Sonamu fails `Surface::configure` ("Surface does not support the adapter's queue family"); the previous commit fails the same way, so it's environmental.
- The file manager sizes its preview from row height only on resize, so a font change applies there after the next resize.

## Later

- Commands/keybinds by name from the same program. Only things that must run code in-process would need a dylib.
- Settings editor: deferred; schema autocomplete covers most of it.
