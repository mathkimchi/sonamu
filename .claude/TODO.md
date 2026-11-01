# TODO

## File manager / splits / TTY session (2026-10-16)

Decisions and details: `.claude/FM_SPLITS_UDEV.md`. Delete a line when done.

- [x] File manager with embedded editors in its inner tree (`AppletContext::embed`)
- [x] 2-way splits (`Layout::Split`, Ctrl+Shift+| / Ctrl+Shift+_)
- [x] TTY backend (`run_drm_display`), untested on a real TTY
- [x] Bug squashing and resolving TODOs (fresh context); see "Bug fixes and TODOs" in that file
- [ ] Improve the three features
- [x] Redo the command hub (looks, history recall, Tab completion, `$` on a worker thread, no Ctrl+letters typed)
- [x] Wl clients open under the focused node
- [x] Help applet on Super+?
- [x] Treeview only while traversing (now the `treeview.pinned` setting, #106)
- [x] Projects and a movable view root (Super+P, Super+V, hub `project`)
- [x] Projects hold their contents as an inner tree (`ProjectApplet`); see `.claude/PROJECTS.md`
- [ ] Make the treeview look nice
- [ ] Delete the throwaway harness `singularity_sde/examples/headless.rs` and its dev-dependencies in `singularity_sde/Cargo.toml`

## Sessions (2026-10-17)

Done: save and restore the tree (`.claude/SESSIONS.md`). Left, from its "Issues found":

- [ ] Keep `awaiting` when a "Starting ..." tile is plucked or wrapped
- [ ] Skip or nest dialogs (toplevels with a parent)
- [ ] Try qutebrowser, firefox and foot

## Wayland

Non-major Wayland features. Delete a line when done.

- IME: place the candidate window with winit `set_ime_cursor_area` using the client's text-input cursor rectangle. Blocked: smithay only stores that rectangle privately (for `input_method_v2` popups), with no public getter.

## Buttons

- Native applets have no buttons: they draw a node and hit-test clicks in `handle_mouse` with their own geometry (eg the file manager's list toggle, the treeview's pin and close), so the two can drift apart. Clicks are routed by hand (runner → `NodeTree::handle_mouse` → applet → its embedded documents), while only the cursor comes from the laid out tree (`CursorAreas`). Idea: `UITree::clickable(id)` (also setting the pointer cursor), with the applet laying out its own surface to hit-test it like `CursorAreas`. The id is explicit since buttons appear conditionally; `ClickId(u64)`, or type-erased if most buttons carry data (eg a row index). The old `singularity_sttk/src/components/button.rs` is dead code.

## Done (differently): replace nested `StandardApplet`s with per-client `ClientHandle`s

Done in the ClientHandle refactor, but the tree lives in the root client, not the runner; see `.claude/CLIENT_HANDLE_REFACTOR.md`.
Kept below for history; what's left is in that file's "Deferred" section.

Original plan: the runner would own the node tree; clients talk to it only through queues.

Why:
- Hooks call into parents mid-dispatch, which causes the `add_child` deadlock and forces deferred close (`ChildSlot.closed` / `remove_closed_children`).
- Wl surfaces become server nodes, removing `WlSurfaceApplet`, `WlSurfaceRegistered`, `FocusWlSurface`, `WlSurfaceDestroyed` and the `WlKeyboardFocus` round-trip.
- Each client gets its own event loop (native applets currently can't wake themselves).

Steps:
1. Server-side node tree in `AppletRunner` (node = native `ClientHandle` or wl surface, plus children and focus); port close/reparent/focus logic there.
2. Make wl surfaces server nodes directly; drop `WlSurfaceApplet`.
3. sttk adapter running an existing `StandardApplet` as a client (own calloop loop, hook backed by `request_sender`) so tabs migrate gradually.
4. Delete `RootNodeApplet`, `RecursiveNodeApplet`, `CachingApplet`.

Open questions:
- Synchronous hook calls with return values (`get_applet_spawners`, `find_applet_spawner`) need query-response, or the spawner registry moves to the server.
- Traversal keys handled in the `Focusing` state move to the server; client keybinds become requests (`Close`, `Quit`, `ChangeFocus`, `AddChild`).
- Granularity: one client per tab; finer structure stays inside the client's treeview.
- Can native tabs host native child tabs internally, or does the server own all nesting?
- Thread per client vs a shared loop for native clients.
