# TODO

## File manager / splits / TTY session (2026-10-16)

Delete a line when done.

- [x] File manager with embedded editors in its inner tree (`AppletContext::embed`)
- [x] 2-way splits (`Layout::Split`, Super+Shift+| / Super+Shift+_)
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

## Sessions (2026-10-17)

Done: save and restore the tree (`.claude/SESSIONS.md`). Left, from its "Issues found":

- [ ] Keep `awaiting` when a "Starting ..." tile is plucked or wrapped
- [ ] Skip or nest dialogs (toplevels with a parent)
- [ ] Try qutebrowser, firefox and foot

## Wayland

Non-major Wayland features. Delete a line when done.

- IME: place the candidate window with winit `set_ime_cursor_area` using the client's text-input cursor rectangle. Blocked: smithay only stores that rectangle privately (for `input_method_v2` popups), with no public getter.

## Buttons

- Native applets have no buttons: they draw a node and hit-test clicks in `handle_mouse` with their own geometry (eg the file manager's list toggle, the treeview's pin and close), so the two can drift apart. Clicks are routed by hand (runner → `NodeTree::handle_mouse` → applet → its embedded documents), while only the cursor comes from the laid out tree (`CursorAreas`). Idea: `UITree::clickable(id)` (also setting the pointer cursor), with the applet laying out its own surface to hit-test it like `CursorAreas`. The id is explicit since buttons appear conditionally; `ClickId(u64)`, or type-erased if most buttons carry data (eg a row index).
