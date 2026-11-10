# Sonamu

A Rust compositor that runs native Sonamu applets and Wayland clients.
Organizes applets and even elements of applets into a large recursive tree hierarchy.

# Repo Structure

- `sonamu_common` contains things used by both the compositor and native applets
  - `sonamu_common/sonamu_ui` contains custom UI protocol
  - `sonamu_common/sonamu_sync` contains sync primitives
  - `sonamu_common/sonamu_command` contains node commands (`NodeUid`, `Value`, registry), see `.claude/COMMANDS.md`
- `sonamu_sar` is general server-side framework
- `sonamu_sde` contains the main runner
- `sonamu_script` contains the Luau script thread (`eval`)
- `sonamu_sttk` contains tools for clients and necessary clients
- `sonamu_standard_tabs` contains the native applets (code editor, file manager, image viewer, pdf preview)
- Floats to px: `display_units::ToPx` (`.to_px()`, clamps to 0) instead of `as u32`; settings' gap/border: `Settings::gap_and_border_px`
- Check for unused dependencies with `cargo machete` (sttk's `regex` is a build.rs dep it can't see; ignored in its `Cargo.toml`)

DEVLOG.md is 10k+ lines, read it with tail.

# Architecture

- The sar runner holds only the root client's `ClientHandle` (`sonamu_common::sap::client_handle`). The applet tree (`NodeTree`, a slotmap of nodes with parent/children ids, in `nodular_applet/node_tree.rs`) lives in the root client's thread (`RootNodeApplet`).
- Every node's main is an applet on its own thread, behind a `ClientHandle`; wl tiles are `WlSurfaceApplet`s, and `NodeTree::wl_surfaces` maps each surface to its node. Keys for the focused wl tile go straight from the runner to its client unless `root_node_applet::is_keybind` (#56); a tile sends other `WlSurfaceRequest`s to the compositor on `wl_request_queue` and gets `WlSurfaceEvent`s (focus, destroyed, title) on its own `wl_event_queue`. Applets talk to their node only through `NodeRequest`s, sent via `AppletContext`; their requests are tagged with the node's `NodeId`.
- Fullscreen (#121) is runner-only (`SmithayState::fullscreen`): a wl toplevel that asks is drawn over the whole window (only overlay layers above it) and gets the mouse, until it asks to stop or Sonamu focus leaves its tile (`focus_wl_keyboard`); the tree doesn't know about it.
- Multiple screens (#60): DRM shows each connected monitor (side by side, one scene); in a window `SONAMU_SCREENS=N` splits it. The runner sends the root `StandardEvent::Screens` (rects in its area); each screen shows the one tree with its own focus and view root/layer, and the last clicked screen is active (keys, treeview); the one wl output (layer surfaces, capture) is the main screen. Holders send applets `ScreenViews`, and projects keep their own per-screen focus and draw a window per screen. See `.claude/WORLD_TREE.md` (Screens).
- Client state the holder reads (surface, treeview) uses `SharedState`/`StatePublisher`: the client publishes and sends `Damage(kind)`, the holder reads and sends `DamageAck(kind)`. At most one unread publish at a time.
- An applet owns its inner tree directly; it changes its children only by requests to its node.
- Sessions: applets publish `StandardApplet::save` with their treeview; the root saves the tree every 2 s and restores it on start (wl clients by command line, matched back by pid/app id). See `.claude/SESSIONS.md`.
- Settings (#63): JSON, or JSON printed by an executable settings program, live-reloaded. See `.claude/SETTINGS.md`.
- Other processes reach the runner over `$SONAMU_SOCKET` (`sonamu_common::msg`, `sonamu_sar/src/msg_socket.rs`): `sonamu-msg open <applet> [args]` becomes `StandardEvent::Open` to the root, which places that registered applet like a new wl surface: into the focused applet if focus is inside it (`StandardApplet::take_open`, else `NodeRequest::Open` back), otherwise as the focused node's child (`NodeHolder::open`). `assets/sonamu-file-manager.desktop` makes the file manager the default for folders. `sonamu-msg eval '<luau>'` runs Luau like the buffer line, `sonamu-msg subscribe` streams events (`.claude/COMMANDS.md`).
- No Xwayland: the runner sets `XDG_SESSION_TYPE=wayland`, `ELECTRON_OZONE_PLATFORM_HINT=auto` and `NIXOS_OZONE_WL=1` in its own env at start, so clients inherit them (and on the tty exports them to D-Bus with `WAYLAND_DISPLAY`). `WAYLAND_DISPLAY` can't be set that way (see `AppletRunner::run`), so each launch sets it from `SONAMU_WAYLAND_DISPLAY`.
- Native applets paste with `sonamu_common::clipboard::paste` (not `arboard`, which reaches the host's or no compositor): it sends the runner a pipe, and `SmithayState::write_clipboard` has the selection's owner (a wl client, incl. data-control like `wl-copy`) write into it; 1 s timeout. `clipboard::copy(text)` makes the runner the selection's owner (`SmithayState::set_clipboard`, `SelectionUserData` = the text, written from a thread in `send_selection`); pasting that back reads it directly. `clipboard::Target::Primary` (`copy_primary`/`paste_primary`): the same for the primary selection. Luau: `clipboard.copy`/`paste` (`{primary = true}`). Prompts (hub, buffer line) copy their last output with Ctrl+Shift+C; the code editor has no selection to copy yet.
- Desktop notifications (freedesktop D-Bus, eg DMS shows them): `sonamu_common::notification::Replacing`, sent with `busctl`; used by settings reloads and low battery warnings (`sonamu_sar::battery`, 20% then a critical one at 10%, polled every 30 s). Keep tests from sending them.
- Testing with input and screenshots without a monitor (headless sway): `.claude/TESTING.md`.
- Native applets' hand-built UI (rows with hit-testing, text, frames, metrics): `sonamu_sttk::ui`, see `.claude/UI_KIT.md`.
- Commands and Luau scripting (in progress; the hub runs Luau): `.claude/COMMANDS.md`.
- World tree, paths ("inside" a node), focus, selectors, and who owns what (incl. projects, which are plain applets holding a `NodeTree`): `.claude/WORLD_TREE.md`.

# Rules

- Do not change more than asked.
- Keep changes small and simple.
- Report issues we missed.
- Follow general Rust idioms.
- Use minimal tokens.
- When explaining and writing to .claude, avoid verbosity. Every word must serve a purpose. Prefer clear shorthand over full sentence when sensible.
- Update the files in .claude as things are discovered and changed.
- Never edit `docs/`, `README.md` or `DEVLOG.md`.
- Commits may wait for a gpg passphrase: if `git commit` hasn't finished after 20 s, commit again with `--no-gpg-sign` (eg `timeout 20 git commit ... || git commit --no-gpg-sign ...`).
