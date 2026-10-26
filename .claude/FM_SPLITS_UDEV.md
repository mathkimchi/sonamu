# File manager, splits, udev backend

The user's request (2026-10-16): a file manager whose opened files are code editors embedded in its inner tree
(to test plucking elements), 2-way split panes, running Sonamu as a real compositor (udev), then bug fixing,
then polishing these features and the treeview. Decide, implement, and log here. No commits.

## Embedding applets in applets (sttk)

- **`AppletContext::embed(initializer) -> (EmbeddedId, ClientHandle)`** runs an applet the caller holds itself,
  on the caller's own loop (its requests are handled there). `embed_detached(DetachedClient)` does the same for a placed client.
- Requests from embedded applets go to the new **`StandardApplet::handle_embedded_request(id, request)`** (default: ignore).
- **`AppletContext` is no longer `Send`**: it holds the applet's `LoopHandle` (needed to spawn embedded clients).
  Nothing sent it to another thread. If an applet needs to damage from a worker later, split off a `Send` part then.
- The applet loop's state struct moved out of `AppletClient::init` as `AppletState` (public type, private fields),
  so `AppletContext` can name the loop's type.
- `EmbeddedId` is a per-applet counter, not a slotmap key: the applet owns its own storage.

## File manager (`singularity_standard_tabs/src/file_manager.rs`)

- Spawner `file_manager [dir]` (default: the directory of the tree it is in, which it follows), registered in sde.
- PDFs open in Okular (#95), embedded as a document: the file manager publishes the started pid as `awaiting_wl`, so the window
  is routed to it wherever focus is, and `take_wl_surface` embeds a `WlSurfaceApplet` for it. While it has the keys the file manager
  reports its surface with `WlKeyboardFocus`. Closing such a document (or the file manager) asks Okular to close; it goes once its window does.
- Preview: the file under the mouse, else the selected one while the file list has the keys, is shown in the document area by
  an embedded editor that doesn't take keys (`Preview`, synced in `update`). Enter or a click opens it, keeping that editor.
  PDFs are previewed by `PdfPreviewApplet` (`pdf_preview.rs`): page 1 rendered with `hayro` on a worker thread to fit its size,
  re-rendered on resize (newest job wins). ~40 ms a page in release; hayro and its heavy deps are `opt-level = 3` in dev
  (root `Cargo.toml`), else it's ~1.5 s. With nothing to show, the area has an editor's background.
- `UIElement::Image(Image)` (`sonamu_ui`): RGBA8 pixels from an applet, which has no GPU device for a `Texture`.
  `Image` is an `Arc` with a unique id; the renderer's `ImageCache` uploads each id once and frees it after a frame that
  didn't draw it (like `TextCache`). Stretched over its node, so size the node to the image. The image viewer could use it too.
- Known gap: there's no pointer-leave event, so the hover preview stays after the pointer leaves the pane until it moves in it again.
  The old file manager (old packet API) was already deleted in the working tree; this is new.
- Layout: file list (30% of the width) | header (open documents, active one yellow) over the active document.
- Opened files are `CodeEditorApplet`s embedded with `AppletContext::embed`. Several documents can be open;
  opening an open file switches to it. Called "documents" since a placed applet of any kind can be one.
- **Inner tree** (treeview): root `dirname/`, one child per document (its own treeview). The file list isn't in the
  inner tree: it would add a row per file to the root treeview.
- Input: `Pane::Files` or `Pane::Document`. **Ctrl+Tab** toggles. In the list: Up/Down or j/k move, Enter/Right/l
  opens a file or toggles a dir, Left/h collapses or goes to the parent. A click on a row selects and opens it.
  Wheel scrolls the list. Clicking the document area focuses it; mouse events there are forwarded (shifted).
  In the document pane every key goes to the document, so its standard keybinds apply to it:
  Ctrl+W closes the document (not the file manager), Alt+q goes back to the file list.
  In the file list, standard keybinds apply to the file manager (Alt+q leaves it, Ctrl+W closes it).
- The active document gets `FocusChanged(bool)` when the pane changes (the editor greys its cursor).
- Documents' requests: `Damage` redraws, `Quit` forwards, `AddChild` adds to the file manager's node,
  `RequestPluck/Place` are prefixed with the document's index and forwarded, `Close` removes the document.
- **Pluck**: a path into document `i` plucks the whole document (no deeper plucking). The inner root path
  (what Alt+P with `Inner` focus sends) plucks the active document if the document pane is focused,
  otherwise `None` (the node plucks itself).
- **Place**: anything without children or a wl surface becomes a new active document (no file, so opening its
  file again makes a second editor). Others are given back and become children of the node.
- Files that aren't UTF-8 text show "Can't open: ..." in the header instead of spawning an editor (it would panic).
- Hidden files are skipped; dirs first, then by name.
- Fixed while here: `CommandHubApplet` and `OneCharRenderTestApplet` `todo!()`ed on `FocusChanged`/`Highlighted`,
  which the file manager sends to documents; they now ignore them.

## Splits

**Now (2026-10-22): a split is an applet**, `SplitApplet` (`nodular_applet/split_applet.rs`), holding its panes as its
inner tree like a project holds its contents: its inner root is the split node described below, its children the
panes. `NodeRequest::Split` wraps the node (`NodeHolder::to_wrap`, its subtree becomes the first pane) and focuses the
new pane inside. So its row gets `▾`/`▸`, `×`/Ctrl+W close it (its panes take its place), and left with one pane it
is replaced by that pane (it sends `Unwrap`). Saved as the `split` spawner with its inner tree (see `.claude/SESSIONS.md`). The notes below are from before, and still describe its inner tree.

- **A split is a node with no main** (`Node::main: Option<ClientHandle>`) and `layout: Layout::Split(Axis)`,
  as the ClientHandle refactor notes anticipated ("No `Option` main / layout field yet").
  `Layout` and `Axis` live in `singularity_common::sap::detached`, since `DetachedTree` carries them
  (`DetachedTree { main: Option<DetachedClient>, layout, .. }`) so plucked splits are placed back as splits.
  `DetachedTree::leaf(client)` added for the common case.
- **Display rule** (generalizes the old "show the focused node's main"): a node shows its focused child if it has one,
  otherwise its main; a split shows all its children side by side (or stacked), each by the same rule.
  So each pane remembers what it shows. `NodeTree::update_layout` (run by the root after each event batch) works out
  the panes (`NodeTree::panes`: node + rect) and resizes each shown applet to its pane.
  Hidden applets keep their last size and get the right one when shown.
- Splits lay out **any number of children** equally. Creating one always makes a new 2-pane split (nested, never
  appending to a parent split of the same axis), so in practice they're 2-way. Closing a pane that has children
  moves them into the split (N-way). **A split left with one pane is replaced by that pane** (`collapse_split`,
  after `close` and `pluck`).
- Keybinds (standard, applet-side): **Ctrl+Shift+|** splits side by side, **Ctrl+Shift+_** stacked. New request
  `NodeRequest::Split { axis, new_pane }`; the keybind's new pane is a CommandHub, which gets focus (`Inner`).
  Splitting the root makes the split the new root. The file manager forwards documents' split requests to its node.
- Every pane has a 1px border: green if focus is inside its applet, yellow if on the node itself, dim otherwise.
  This replaces the root's outer `LIGHT_GREEN` border, and fixes the old "1px border" mouse offset TODO.
- **Mouse** goes to the pane under the pointer (relative to it); a press focuses that pane's applet (`Inner`) and
  grabs the pointer until release. Before, all mouse events went to the focused node.
- A focused split (`Focusing`) takes only traversal keys; NextLayer goes into its first pane.
  Pluck/place paths into a split never ask an applet (`NodeTree::has_main`), since it has none (the root would wait forever).
- Treeview label of a split: `Split ⇆` / `Split ⇅`.
- Not done: resizing panes (always equal), moving focus between panes by direction (use traversal keys / click),
  closing a split as a whole except by plucking it.

## Running as the actual compositor (TTY)

- **Approach: present the existing wgpu renderer directly to a monitor** via wgpu's `SurfaceTargetUnsafe::Drm`
  (Vulkan `VK_EXT_acquire_drm_display`), instead of smithay's DRM/GBM/GLES path. The whole renderer stays as is;
  the Vulkan driver does the modesetting and page flips. Cost: needs a Vulkan driver with that extension
  (Mesa has it; NVIDIA's proprietary driver probably not), no hardware cursor, no multi-monitor, and VT switching
  is best effort (below). The smithay-renderer route would mean rewriting the renderer, so I didn't.
- New `sonamu_ui::winit_backend::drm::run_drm_display` (exported as `sonamu_ui::run_drm_display`), same arguments as
  `UIDisplay::run_display`. It lives under `winit_backend` because it reuses that module's private `WgpuData` and draw
  code; the module name is now a bit wrong (`winit_backend` is really "the wgpu display").
- **Selection**: `AppletRunner::run` runs on the TTY if neither `WAYLAND_DISPLAY` nor `DISPLAY` is set (like niri/anvil),
  or if `SONAMU_BACKEND=tty`; `SONAMU_BACKEND=winit` forces a window. On the TTY, wgpu is limited to Vulkan.
- Session: smithay's `LibSeatSession` (logind or seatd); GPU: `udev::primary_gpu`, else the first GPU.
  Monitor: the first connected connector at its preferred mode. Vulkan display plane index 0
  (Mesa numbers planes by its connector list, which only has ours), overridable with `SONAMU_VK_DISPLAY_PLANE`.
  wgpu matches the mode by exact refresh rate, so it's computed the way Mesa does (`vulkan_refresh_millihertz`).
- Input: libinput (smithay's `LibinputInputBackend`) on the display thread, translated to the same `UIEvent`s as winit:
  xkb keymap from `XKB_DEFAULT_*`, key symbols from the keysym (so Ctrl+W is still `w`, not a control char),
  modifiers from before the key (like winit), **key repeat** done here (600 ms delay, 25/s; libinput doesn't repeat,
  winit did), pointer accumulated and clamped to the screen, `RelativeMotion` from unaccelerated deltas,
  scroll as 15 px per notch (like winit). A pointer lock just stops moving the pointer.
- **Software cursor**: a small white box drawn on top of the scene (hidden where the scene asks for `Cursor::Hidden`).
  Pointer motion redraws the last scene, at most once per event batch. Named/custom cursor shapes are ignored.
- **Ctrl+Alt+Backspace quits** (the TTY's own keyboard is off while this runs, so this is the escape hatch).
  Ctrl+Alt+F1..F12 switch VTs through the session; on return, libinput resumes and the surface is reconfigured,
  but whether Mesa restores the mode afterwards is untested.
- If the session, GPU, monitor or Vulkan surface can't be set up, it logs why and returns, which ends Sonamu
  (the proxy is sent first, so the runner isn't left waiting).
- `DisplayProxy` (`Box<dyn Fn(DisplayRequest) + Send>`) replaces `EventLoopProxy<DisplayRequest>` in `UIHandle`,
  so either display can be woken. `Gpu { device, queue, instance, adapter }` bundles what the display gets
  (it needs the adapter for the surface's capabilities). `UIHandle::instance` (never read) is gone.
- `WgpuData::new` now takes the surface and picks the format/alpha from its capabilities, preferring the old
  `Bgra8UnormSrgb` + `PreMultiplied` (so the window is unchanged); a Vulkan display usually only has opaque alpha.
- **`WAYLAND_DISPLAY` is never set in-process.** I first set it to Sonamu's socket on the TTY, which hung at startup:
  Mesa's Vulkan device-select layer connects to `WAYLAND_DISPLAY` while wgpu creates the instance, before Sonamu's
  loop runs. Instead sar sets `SONAMU_WAYLAND_DISPLAY` (before any thread starts), and the command hub passes it as
  `WAYLAND_DISPLAY` to what it launches. `SmithayState::socket_name` is kept for this.
- Command hub: new `run <program> [args]` starts a program without waiting (so wl clients open inside Sonamu);
  `test wl` uses it instead of the hardcoded `wayland-2`. Fixed: `$` with no program panicked (`args[0]`).
- **Tested**: builds; window mode still runs; on this machine (niri running) TTY mode fails cleanly with
  "can't open /dev/dri/card0: ... Resource temporarily unavailable" and exits. **Not tested on a real TTY**
  (I can't switch VTs without taking over your session). To try: from a free TTY, `cargo run -p singularity_sde`
  (or `SONAMU_BACKEND=tty`); quit with Ctrl+Shift+Q, or Ctrl+Alt+Backspace if stuck.

## Bug fixes and TODOs

Crashes:
- **Code editor on a missing/unreadable file panicked** (`File::open(..).unwrap()`; eg `add_child code_editor /nonexistent`).
  Now a missing file opens empty (created on save, like vim); other read errors (eg not UTF-8) show "Can't open: ..." as
  the text and set `load_error`, which disables saving so the file isn't overwritten.
- **Code editor `save_buffer` unwrapped** IO errors (eg permission denied killed the editor). Now logs them.
- **Code editor Up on an empty buffer underflowed** (`len_chars() - 1`). Up/Down now share `cursor_at(line, column)`,
  which clamps to the line's end before its line break. This also fixes Down onto a shorter line landing at the start of
  the line after it, and Up onto a last line without a trailing newline stopping one char short.
- **Code editor paste**: `Clipboard::new().unwrap()` panics where there's no clipboard (TTY); the cursor moved by bytes
  (`text.len()`), so non-ASCII pastes put it past the end and the next render panicked. Now chars, and no clipboard = no paste.
- **Code editor in a pane shorter than one row**: `ViewOffset::clamp_to_cursor` called `clamp(min, max)` with min > max (panics).
  Treats it as one row.
- Code editor / image viewer titles unwrapped `file_name()`/`to_str()` (paths like `/`, non-UTF-8 names). Now lossy.
- **`image_viewer` render was `todo!()`** (and `new` unwrapped open/decode): spawning it killed its applet. It now loads into
  `Result<RgbaImage, String>` and shows a line describing the image or the error. Drawing it still needs textures for applets (TODO).
- `NodeTree::calculate_new_focus` `todo!()` arms are no-ops now (`Inner` + `GlobalRoot` leaves like `Child` does).
  None were reachable by keys (`q`/`e` are layer keys, so `Bfs*` never come; only `PrevLayer` is sent by applets),
  only by stale `ChangeFocus` requests. Didn't touch the `RelShiftSibling` clamp: `Child(_)` focus implies at least one child.
- `CloseRequest` (`todo!()` in root, command hub, char test; never sent today): applets close themselves, the root asks sar to quit.
  `WlSurfaceRegistered` (`todo!()` in command hub, char test; only the root gets it) is ignored.
- `NodeTree::handle_mouse` indexed `nodes[pane.node]` from the last layout; a node closed earlier in the same event batch
  would panic there. Now `get`.

Pluck/place:
- **Root/sar stuck pending forever when the asked applet has ended** (eg panicked): `ClientHandle::try_send_event`
  (gives the event back if the client's queue is closed) and `NodeTree::try_send_event` (also `Err` for splits) replace
  `has_main`. If the `Pluck`/`Place` can't be sent, the node plucks itself / takes the tree as a child, as for splits.
  Still stuck if the applet dies after the event was queued (would need a "client ended" callback on `ClientHandle`).
- `Placed(Some(tree))` from an applet whose node closed meanwhile would index a removed node in `NodeTree::place`.
  `place` now falls back to the root.
- **A wl surface destroyed while plucked** was placed back dead. sar now removes it from `plucked` first
  (`DetachedTree::remove_wl_surface`: its children take its place; at the plucked root, a single child replaces it,
  otherwise the root keeps its children with no client, shown as `(empty)`); if nothing is left, `plucked` is cleared.
  Only otherwise is `WlSurfaceDestroyed` sent to the root. Not covered: destroyed while the pluck is in flight.

Cleanup:
- Removed `AppletRunner::event_loop` (never read; the old warning). `AppletRunner::new` takes `&LoopHandle` (clippy).

Skipped (design-level or not worth it now):
- `RootNodeApplet` `WlSurfaceRegistered` "TODO: change behavior based on the focus": new wl tiles still go under the root
  node. With a split root that means a new pane (visible right away); under the focused node they'd be hidden behind it.
- Focus inside inner trees, wl surfaces in inner trees, wl titles, damage acks after present, keybind system TODOs.
- A panicked applet's tile is still never cleaned up (needs the same "client ended" callback as above).

Issues found, not fixed:
- Every pointer event over the root rebuilds the treeview (`treeview_rows(&tree.get_treeview())`), which acks every
  applet's treeview each time. Harmless (acks are idempotent) but wasteful; cache rows per render instead.
- ~~The command hub's `$` runs the program synchronously~~ and ~~Ctrl+letters are typed into its prompt~~: fixed in "Command hub redo".
- A plucked document keeps the last `FocusChanged` it got, so an editor plucked while focused shows a focused cursor wherever it's placed.
- sar's `plucked()` on an unexpected `Plucked` overwrites (drops) a tree it already holds.
- Code editor: in CRLF files the cursor can sit between `\r` and `\n`.

Verification: `cargo clippy --workspace --all-targets` has no warnings in touched code (the remaining ones are older:
sonamu_ui `ui_element`/`winit_backend`, sar `smithay/mod.rs`, the harness). Headless harness: missing-file editor and arrow
keys on its empty buffer (panicked before), splits close/pluck/place incl. placing into a split by exact path, file manager
open/close/pluck/place of documents, clicks. `singularity_sde` ran 8 s windowed with no panics; `SONAMU_BACKEND=tty` still fails cleanly.
The dead-applet guard and `remove_wl_surface` weren't exercised (no panicking applet left; no live wl client was plucked).

## Command hub redo (`singularity_sttk/src/command_hub/mod.rs`)

- **Drawn as a `CharGrid`** (like the code editor), not glyphon text: exact cell metrics make wrapping, scrolling
  and a block cursor simple, and cells have backgrounds. Cost: the grid's shader only draws ASCII; anything else
  (eg unicode in `$ ls` output) shows as its red box.
- Layout: title bar (title; right side `help | Tab | Up/Down`, or "N lines below" while scrolled), scrollback anchored
  to the bottom, prompt bar `> ` with a block cursor, scrolled sideways to keep the cursor in view.
  Commands are on a slightly lighter row, outputs grey, errors red, running `$` commands amber `running...`, hints dim.
  Colors pre-darkened (grid colors are linear too); the help applet reuses them (`pub(crate)`).
- Kept: `add_child`, `set_title` (now also damages the treeview, so the title updates), `test`, `help`, `addition`,
  `run`, `$`, standard keybinds, the treeview title. New: `clear`, `project [off]` (task 5). Empty Enter does nothing.
- Keys: Left/Right move the cursor, Backspace deletes before it, Up/Down recall entered commands (the typed prompt comes
  back past the newest), Tab completes the verb, or applet names after `add_child`, tests after `test`, verbs after `help`
  (unique: completes with a space; several: the common prefix, candidates shown as a hint), PageUp/PageDown and the wheel
  scroll, Esc clears the prompt.
- **Fixed: Ctrl/Alt/Super + letter no longer typed** (only unmodified or Shift chars are text).
- **Fixed: `$` froze the hub**: it runs on a worker thread; the result comes back through an mpsc channel plus a wake.
  New generic piece: **`AppletContext::waker() -> AppletWaker`** (`Send`, `Clone`) and **`StandardApplet::wake()`**,
  called on the applet's thread (a calloop channel in the applet loop). Chose this over handing out a typed channel:
  the loop only knows `dyn StandardApplet`, so it can't deliver a typed payload to the concrete applet.
  `$` now also gets stderr, and a failing exit status shows as an error. It gets `WAYLAND_DISPLAY` like `run`.
- Trimming: 200 entries and 200 recalled commands per hub; outputs over 500 lines are cut with "... (N more lines)".
- `$` jobs can't be cancelled; a hub closed while one runs leaves the thread until the process exits (the result is dropped).

## Wl clients open under the focused node

- The root adds a new toplevel's tile as the **last child of `NodeTree::focused_node()`** (the deepest node focus is on
  or in), and focuses inside it (`Inner`), so it's shown in place of that node and gets the keyboard right away.
  The user's "propagate down, then `add_child` up" is just `focused_node()` here, since the root owns the whole node tree;
  wl surfaces can't live in applets' inner trees yet, so an applet with embedded documents (file manager) gets it as a node child.
- Focusing a split (`Focusing`) adds it to the split, so it becomes a new pane (the split goes N-way). Kept: it's visible.
- It steals focus even if the client was slow and the user moved on (like most compositors). It opens inside the view root (task 5).

## Help (Super+?)

- `singularity_sttk/src/help.rs`: `HelpApplet`, a cheat sheet in sections (anywhere, inside an applet, traversal, mouse,
  command hub, file manager, the help itself), keys in amber beside wrapped descriptions (stacked when narrow).
  Also registered as the `help` applet in sde (`add_child help`).
- **Super+? is handled by the root** (before the tree), so it works whatever is focused, including wl clients.
  It opens the help as a **child of the focused node**, focused inside, so it shows in that pane, like any node
  (can be plucked, etc). Super+? again, Esc or q (or Ctrl+W) closes it. If focus was inside the applet it was opened
  from, it goes back inside it (the root remembers `OpenHelp { node, parent, parent_inner }`); otherwise the parent is focused.
- Scrolls with Up/Down, j/k, PageUp/PageDown, Space, the wheel.
- The list is hand-written, so it can drift from the real bindings (no keybind registry yet).
- Nested in niri, the host compositor takes Super+Shift+/ (its own hotkey overlay); `add_child help` works there.

## Treeview only while traversing

- `RootNodeApplet::TREEVIEW_ONLY_WHILE_TRAVERSING` (true): the treeview shows only while the focused node itself is
  focused (`Focusing`), or while a press that started on it is held (so a drag can't lose it). Otherwise the
  applets get the whole window.
- Toggled after each event batch (`update_treeview_visibility`), which resizes the tree (so every shown applet gets
  `WindowResized`). The treeview's width is now in px (`TREEVIEW_PERCENT` of the width) for both the grid and
  the applets' sizes, and the mouse offset is that width (0 when hidden); a hidden treeview gets no rows (so it's not
  hit, and isn't rebuilt on every pointer event).
- Chose resizing over drawing the treeview on top of the applets (which would avoid the resizes):
  that's what the request described. Cost: entering/leaving traversal reconfigures wl clients, and a click on a pane
  shifts the layout under the pointer while the button is held.

## Projects and the view root

- **View root** (`NodeTree::view_root`): only its subtree is shown, panes and treeview, as if it were the root.
  Focus always stays inside it. Paths stay world paths from the real root: treeview rows get the view root's path as
  a prefix (`treeview_rows(.., base, ..)`); focus/pluck/place paths were already from the real root.
- **A project is a node flag** (`Node::project`, carried by `DetachedTree::project`, so plucked projects stay projects).
  Made by **Super+P** (toggles the focused node) or the hub's **`project` / `project off`** (its own node;
  `NodeRequest::SetProject`, which the file manager forwards for documents). Marking a node makes it the view root
  (you're working in it).
- **Narrowing**: when focus moves into a project inside the view (from outside it), the view root becomes the outermost such project.
  **Widening**: when focus leaves the view root (eg `a` on it goes to its parent, `w`/`s` to a sibling, `q` to the root),
  it widens to the closest ancestor holding the focus. Both in `NodeTree::update_view_root`, after each event batch
  (`last_focused` tells focus *moved* into a project, so an explicit widen doesn't snap back).
- **Super+V** sets the view root to the focused node (any node, no project needed), or back to the real root if it already is.
- When the view root isn't the real root, the treeview's first row is a dim `‹ <parent>` row (the parent's world path):
  clicking it focuses the parent, which widens the view; dropping on it places into the parent.
- Kept valid: closing the view root moves it to the parent; plucking it (or an ancestor) to the plucked node's parent;
  splitting it makes the new split the view root, and **a split takes over its node's project flag** (it now stands
  where the project was) and gives it back if it collapses.
- Not done: no project indicator while the treeview is hidden; projects placed into the file manager (as documents) lose the flag.

## Suggested CLAUDE.md updates

- Known issues: the `image_viewer` `todo!()` example is gone (it renders a placeholder now); a panicked applet's tile
  still stays. Mention `ClientHandle::try_send_event` for events that need an answer (pluck/place).
- `AppletRunner` no longer has an `event_loop` field.
- Architecture: applets that need to hear back from another thread use `AppletContext::waker()` + `StandardApplet::wake`
  (`AppletContext` itself isn't `Send`).
- Architecture: `NodeTree` has a view root (projects, Super+V); panes and the treeview start there, but every path
  (treeview rows, focus, pluck/place) is still from the real root. The root applet handles the Super keybinds
  (Super+? help, Super+P project, Super+V view root); sar handles Alt+P.
- The treeview only shows while traversing (`RootNodeApplet::TREEVIEW_ONLY_WHILE_TRAVERSING`).
- Known issue: while a node is focused (traversal), traversal keys ignore modifiers, so eg Ctrl+W there moves to the
  previous sibling instead of closing (`NodeTree::handle_event`). Alt+<key> traversal relies on this.
- `CharGrid` only draws ASCII (other chars are red boxes).
