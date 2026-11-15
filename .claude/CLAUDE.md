# Sonamu

Rust Wayland compositor: native applets and wl clients in one recursive tree.

# Crates

- `sonamu_common`: shared by compositor and applets; `sonamu_ui` (UI protocol), `sonamu_sync` (sync primitives), `sonamu_command` (node commands)
- `sonamu_sar`: server-side framework (runner); `sonamu_sde`: main binary
- `sonamu_script`: Luau thread (`eval`)
- `sonamu_sttk`: client tools and core clients; `sonamu_standard_tabs`: native applets
- Floats to px: `display_units::ToPx` (`.to_px()`); gap/border: `Settings::gap_and_border_px`
- `cargo machete` for unused deps (sttk's `regex` is a build.rs dep, ignored)
- DEVLOG.md is 10k+ lines: read with tail

# Architecture

- Runner holds only the root's `ClientHandle`. The tree (`NodeTree`, slotmap, `nodular_applet/node_tree.rs`) lives in the root's thread (`RootNodeApplet`).
- Each node's main is an applet on its own thread behind a `ClientHandle`. Applets reach their node only via `NodeRequest`s (`AppletContext`), tagged with its `NodeId`; an applet owns its inner tree and changes children only by those requests.
- wl tiles are `WlSurfaceApplet`s (`NodeTree::wl_surfaces`). Focused tile's keys go runner → client unless `root_node_applet::is_keybind`; other traffic: `WlSurfaceRequest` on `wl_request_queue`, `WlSurfaceEvent` on `wl_event_queue`.
- Holder-read client state (surface, treeview): `SharedState`/`StatePublisher`, `Damage(kind)` → `DamageAck(kind)`, at most one unread publish.

# Docs

- Runner (Wayland, socket, clipboard, notifications, rendering): `RUNNER.md`
- World tree, ownership, pluck/place, projects, focus, traversal, zoom-out, treeview, screens: `WORLD_TREE.md`
- Glossary: `CONTEXT.md`
- Sessions: `SESSIONS.md`; settings: `SETTINGS.md`; status bar: `STATUS_BAR.md`
- Commands, Luau: `COMMANDS.md`
- Native applet UI kit: `UI_KIT.md`
- Headless testing: `TESTING.md`

# Rules

- Change only what's asked; keep it small, simple, idiomatic Rust, self-documenting.
- Report issues we missed.
- Terse everywhere, incl. .claude; keep .claude current.
- Never edit `docs/`, `README.md` or `DEVLOG.md`.
- Commit frequently. Pre-commit hook (`.githooks/`, `core.hooksPath`) runs `cargo fmt` and clippy `-D warnings`.
