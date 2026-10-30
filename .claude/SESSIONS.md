# Sessions: remembering and restoring the tree (2026-10-17)

Sonamu saves the whole tree (layout, native applets, projects, wl clients, focus, whether the treeview is pinned) and rebuilds it on the next start.
Sonamu owns the session; apps' own session restore should be off.

## How it works

- **Saved form** (`singularity_common/src/sap/session.rs`): `Session { tree: SavedTree, focus, treeview_pinned }` (`treeview_pinned`: the root treeview's pin button).
  `SavedTree { main: Option<SavedApplet>, layout, children, label }` (`label`: only a project's inner root, its name).
  `SavedApplet` is one of `Spawner { name, args }` (made again by the registered applet spawner, like the command hub does)
  or `Wl(SavedWl { argv, cwd, app_id, title })`.
- **Applets with an inner tree** (#130) save it as `Spawner::inner` (`session::saved_with_inner`), restored by
  `AppletSpawnerTrait::restore_initializer(args, inner)` (default: `create_initializer`, so other spawners never see it).
  JSON is only used to read and write the file; published saved forms stay structured.
  - `project`: no args, inner = its `NodeTree::save()` with label/directory/icon on the inner root (the treeview's
    icon reads `inner.icon`). `split`: no args, inner = its `NodeTree::save()`. Both spawners are registered by
    `RootNodeApplet` itself; `project` with no args (not restoring) is an empty project, `split` can't be made.
  - `file_manager`: args = its root (none when it follows its tree's directory), inner's children = its documents,
    each `main` saved by the document itself. Restored with `session::restore`, none active; a wl document (eg Okular)
    is started with `session::restore_command` and its window claimed by pid like a newly opened PDF (`starting`).
    A document's file is read back from its saved form (`saved_file`: a code editor's arg, a PDF in the wl argv).
- **Publishing**: every applet publishes `StandardApplet::save()` as `Treeview::saved`, so a holder reads it
  synchronously like the treeview. `None` (the default, eg the help) leaves the node out, its children taking its place.
  A split left with one pane is left out too. A project saves its whole `NodeTree` (`NodeTree::save`).
- **Writing** (`RootNodeApplet::save_session`): every 2 s if changed, and before quitting. Atomic (temp + rename).
  Path: `$SONAMU_SESSION` (set but empty: no session), else `$XDG_STATE_HOME/sonamu/session.json`
  (`~/.local/state/sonamu/session.json`). An unparsable file is moved to `.json.bad`.
- **Restoring** (`NodeTree::restore`, `session::restore`): on start, if there's a session, `root_initializer` is ignored.
  Spawners are looked up on the applet's own thread (`AppletContext::find_applet_spawner`).
  Missing ones become a `RestoringApplet` placeholder ("Couldn't restore ...") that stays saved as it was until Ctrl+W.
- **Wl clients**:
  - Saved from `/proc/<pid>`: `cmdline`, and the cwd of its first child (eg a terminal's shell), else its own.
    The pid comes from the client's socket credentials.
  - The compositor now tells the root about a toplevel on its **first commit** (`register_toplevel`), not in
    `new_toplevel`, so its app id is set. Its title is sent right after, since `title_changed` came before the root knew.
  - Restoring spawns the command (on the holder's thread). The node gets `awaiting: AwaitedWl { pid, app_id }`
    and a `RestoringApplet` ("Starting ..."). The child is reaped on a thread.
  - `NodeHolder::add_wl_surface` first tries `NodeTree::claim_wl_surface`: a tile awaiting the pid, else the app id
    (for single-instance apps whose window comes from the running instance). Tiles inside a main (projects) are found
    through `Treeview::awaiting_wl`, and the surface is sent to that main, whose holder claims it.
    A claimed tile's main is replaced by a `WlSurfaceApplet`.
  - Restore command (`restore_command`):
    - qutebrowser (app id `org.qutebrowser.qutebrowser`): `qutebrowser --target window [url]`.
    - firefox with a url: `firefox --new-window url`.
    - Chromium web apps / PWAs (#96, app id `chrome-<id>-<profile dir, spaces as _>`, `SavedWl::web_app`): the saved
      browser (argv[0]'s first word: Chromium rewrites its cmdline as one space-separated string) with
      `--app-id=<id> --profile-directory=<profile>`. Claimed by app id, since every window is the
      browser's pid. Not tested in Sonamu yet. No PWA icons yet.
    - Anything else: the saved argv. A `/nix/store/...` program is replaced by its name, minus a `.foo-wrapped` wrapper
      prefix/suffix, since the store path may be garbage collected.
  - The url comes from the title (`SavedWl::url`): before its first ` | ` (firefox, via `extras/firefox`), else after
    its last ` | ` (qutebrowser), with no spaces; `about:` pages count, so they're hidden and restored too.
    So the browser has to put it there.
    Tiles and "Starting …" placeholders show the title without it (`title_without_url`).
  - Quitting Sonamu kills Firefox uncleanly, so with `browser.sessionstore.resume_from_crash` on, Firefox restores
    its own windows on top of Sonamu's: duplicates, which then get saved as new tiles.

## Decisions

- No `SavedApplet` variant per applet (projects and splits had them): every native applet is a `Spawner`, with its
  inner tree in `inner` if it has one (#130). Not JSON in an arg: that would serialize on every treeview publish. Sessions from before that don't parse (moved to `.json.bad`).
- A restored split asks its node for its directory (`get_directory`), like a new one gets it from `wrapper` (#129).

- Sonamu owns the session, not the apps: the alternative means guessing which restored window goes in which tile.
- Saved state is published with the treeview, not collected by request and answer: synchronous, no new messages,
  and projects nest for free.
- Matching by pid first, then by app id. No `xdg-activation` tokens yet (apps don't all pass them on;
  pid covers one-process-per-window apps, app id covers the rest). `xx-session-management` isn't used yet (almost no apps support it).
- Restores happen at start only. There's no "restore now" command.
- `NodeRequest::Standard(Quit)` now sets `NodeHolder::quit`, handled after the batch (projects pass it on, the root saves then
  quits), because the runner may end the process right after `Quit`.
- `StandardEvent::WlSurfaceRegistered` and `NodeRequest::AddWlSurface` now carry a `NewWlSurface`
  (`surface_id`, `wl_request_queue`, `wl_event_queue`, `pid`, `app_id`), and `take_wl_surface` takes and gives back one.
- Saved paths (code editor, image viewer, file manager root) are made absolute.
- Spawner names are hardcoded in each applet's `save` and must match `singularity_sde/src/main.rs`.
- `headless` example reads `SONAMU_SESSION` too (unset: no session).

## Recommended apps / settings (not installed)

User-facing setup (incl. NixOS / Home Manager) is in `docs/persistent_apps.md`.

- **Browser: Firefox** (#73, replaced qutebrowser as `BROWSER`), with the extension in `extras/firefox`
  (WebExtension MV3): moves every new tab next to others into its own window (`windows.create({ tabId })`), and sets
  the window's `titlePreface` to `<url> | ` (Firefox only allows a prefix). `userChrome.css` hides the tab bar.
  Prefs: `browser.link.open_newwindow = 2`, `.restriction = 0`, `browser.startup.page = 0`,
  `browser.sessionstore.resume_from_crash = false`, `toolkit.legacyUserProfileCustomizations.stylesheets = true`.
  Signed as unlisted on AMO by the user and attached to a GitHub release `firefox-ext-v<version>` (not committed,
  it's a zip); users install it by hand
  (incl. Nix users). Signing steps in `extras/firefox/README.md`.
  qutebrowser still restores (`tabs_are_windows`, `title_format = '{current_title} | {current_url}'`), not documented.
- **Terminal: foot**, one process per window (`foot`, not `foot --server`/`footclient`), so pid matching works and the
  shell's cwd is known. Have the prompt put `$PWD` in the title so a `cd` republishes the cwd. To keep processes alive
  across restarts, run the shell in `shpool`/`dtach`/`abduco` (eg `foot -e shpool attach main`): the argv is saved as is.
  Tested here with alacritty and `kitty -1` too.

## Testing done

- Nested (winit in niri): alacritty saved with argv/cwd/app id, restored into the same tile in the same cwd.
- Two `kitty -1` windows (one instance): both restored into their tiles (second matched by app id).
- Alacritty inside a project (hand-written session): restored and claimed inside the project.
- Headless: Super+P on a wl tile, saved as a project, restored as a project with its placeholder kept saved.
- `singularity_common/tests/session_test.rs`: `SavedWl::url`.
- Not tested: qutebrowser, foot, the TTY backend, the firefox extension (not loaded yet).

## Issues found / limits

- Single-instance apps (kitty `-1`, qutebrowser, firefox): every window has the instance's pid, so all share the
  first window's cwd, and two awaiting tiles of the same app may get each other's windows (urls swapped).
- Plucking or wrapping a tile while it is still "Starting ..." drops its `awaiting` (`DetachedTree` has no field for it):
  the placeholder stays, and the window opens at the focus instead.
- Fixed (#77): Firefox's add-on install prompt didn't show. Every toplevel was always `Activated`, and Firefox only
  shows doorhangers in its active window, so it never did; now only the keyboard focus's toplevel is (`focus_changed`).
  The prompt is a subsurface, which was drawn under the page: surface trees were walked top to bottom
  (`surface_tree_layers` now uses `with_surface_tree_upward`). `surface_under` now respects input regions, since
  Firefox's content subsurfaces have empty ones and its root takes the clicks.
- Every toplevel is a tile, so dialogs are saved and restored as windows too (no `xdg_toplevel.parent` check).
- A wl tile's saved cwd only updates when it publishes (eg a title change).
- A placeholder for a client that never opens a window stays "Starting ..." forever (Ctrl+W drops it). No timeout.
- Not saved: the view root (Super+V), collapsed nodes, the file manager's active document, unsaved editor buffers.
- Killed with SIGKILL / crash: up to 2 s of changes are lost.
- `NodeRequest::Run` and the command hub's `$` still leave zombies when their child exits (pre-existing).
- On restore, every wl client is spawned while the root starts, before the compositor loop runs; they connect once it does.

## Questions for the user

- Should restoring be opt-out per start (eg `sonamu --fresh`), besides `SONAMU_SESSION=`?
- Placeholders that never get a window: drop after a timeout, or keep (current)?
- Save dialogs/transient toplevels or skip them (needs the parent from smithay)?
- Is the ` | url` title convention fine, or do you prefer qutebrowser IPC / a userscript writing urls to a file?

## Suggested CLAUDE.md updates

- Architecture: "Every applet publishes `StandardApplet::save` with its treeview; the root saves the tree and restores it
  on start. See `.claude/SESSIONS.md`." (added)
