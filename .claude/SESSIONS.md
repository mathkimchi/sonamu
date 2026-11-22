# Sessions

Sonamu owns the session (apps' own restore should be off).

- Form (`sap/session.rs`): `Session { tree: SavedTree, monitor_focus, brightness, treeview_pinned }` (`brightness`: by monitor name, #158, `sonamu_sar/CLAUDE.md`); `SavedApplet` = `Spawner { name, args, inner }` or `Wl(SavedWl { argv, cwd, app_id, title, running })`. Inner trees (projects, file manager documents) go in `inner` (`saved_with_inner`, `AppletSpawnerTrait::restore_initializer`).
- Publishing: every applet publishes `StandardApplet::save()` as `Treeview::saved` (`None` leaves the node out). Spawner names in each `save` must match `sonamu_sde/src/main.rs`. Saved paths are absolute.
- Writing (`RootNodeApplet::save_session`): every 2 s if changed, and before quit; atomic. `$SONAMU_SESSION` (empty: none), else `$XDG_STATE_HOME/sonamu/session.json`. Unparsable → `.json.bad`. No session: an empty project with the help.
- Node ids (#158): `SavedTree::id` is the node's `NodeUid`, kept on restore (`SavedTree::reserve_ids` first, so new ones don't collide); none for applet-owned inner nodes (eg file manager documents).
- Focus (#158): `monitor_focus[name]` (`ScreenArea::name`) = focused node's uid, then its ancestors' (`NodeTree::screen_focus_uids`; inside projects via `Treeview::screen_focus_uids`). Restore: `NodeTree::focus_chain` focuses the first uid in its world and sends the rest in as `StandardEvent::FocusChain`. Applied in `set_screens` to each screen whose monitor changed (startup, replug, reorder); monitors that went away keep their entry. Old sessions (read only): `screen_focus` (by index, first used), else `focus` path.
- Restoring: every node starts hibernating (`session::hibernate`), woken when focused; only the focus chain starts. Missing spawners → `RestoringApplet` placeholder.
- Wl clients: saved from `/proc/<pid>` (cmdline, first child's cwd; pid from socket credentials). Restore spawns `restore_command` and sets `awaiting: AwaitedWl { pid, app_id }`; `NodeTree::claim_wl_surface` matches pid, then app id (inside mains via `Treeview::awaiting_wl`).
  - `restore_command`: qutebrowser/firefox by url, Chromium PWAs (`SavedWl::web_app`), `/nix/store` paths → program name.
  - Url from title (`SavedWl::url`): before first ` | ` (firefox extension `extras/firefox`) or after last (qutebrowser); `title_without_url` for labels.
  - Programs in a terminal (`session::running`: tty foreground job): `restore_programs` setting re-runs them via `sh -c '"$@"; exec "$0"' <shell> ...` (wrapper stripped on re-save, `wrapper_start`); others get a `RestorePrompt` → `NodeRequest::StartWl`.
  - Wl tiles re-read `/proc` on title change and every 2 s.
- `Quit` sets `NodeHolder::quit`, handled after the batch so the root saves first.
- Gaps: single-instance apps share one pid (cwds/urls may swap); placeholders never time out; not saved: view root, collapsed nodes, unsaved buffers; Firefox crash-restore duplicates windows (`resume_from_crash = false`).
- User setup: `docs/persistent_apps.md`.
