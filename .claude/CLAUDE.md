# Sonamu

A Rust compositor that runs native Sonamu applets and Wayland clients.
Organizes applets and even elements of applets into a large recursive tree hierarchy.

# Repo Structure

- `singularity_common` contains things used by both the compositor and native applets
  - `singularity_common/singularity_ui` contains custom UI protocol
- `singularity_sar` is general server-side framework
- `singularity_sde` is containst main runner
- `singularity_sttk` contains tools for clients and necessary clients
- `singularity_standard_tabs`

Try not to look beyond these crates.
The rest are mostly deprecated.

DEVLOG.md is 10k+ lines, read it with tail.

# Architecture

- The sar runner holds only the root client's `ClientHandle` (`singularity_common::sap::client_handle`). The applet tree (`NodeTree`, a slotmap of nodes with parent/children ids, in `nodular_applet/node_tree.rs`) lives in the root client's thread (`RootNodeApplet`).
- Every node's main is an applet on its own thread, behind a `ClientHandle`; wl tiles are `WlSurfaceApplet`s, and `NodeTree::wl_surfaces` maps each surface to its node. Keys for the focused wl tile go straight from the runner to its client unless `root_node_applet::is_keybind` (#56); a tile sends other `WlSurfaceRequest`s to the compositor on `wl_request_queue` and gets `WlSurfaceEvent`s (focus, destroyed, title) on its own `wl_event_queue`. Applets talk to their node only through `NodeRequest`s, sent via `AppletContext`; their requests are tagged with the node's `NodeId`.
- Client state the holder reads (surface, treeview) uses `SharedState`/`StatePublisher`: the client publishes and sends `Damage(kind)`, the holder reads and sends `DamageAck(kind)`. At most one unread publish at a time.
- An applet owns its inner tree directly; it changes its children only by requests to its node. See `.claude/CLIENT_HANDLE_REFACTOR.md`.
- Sessions: applets publish `StandardApplet::save` with their treeview; the root saves the tree every 2 s and restores it on start (wl clients by command line, matched back by pid/app id). See `.claude/SESSIONS.md`.
- Settings (#63): JSON, or JSON printed by an executable settings program, live-reloaded. See `.claude/SETTINGS.md`.
- Other processes reach the runner over `$SONAMU_SOCKET` (`singularity_common::msg`, `singularity_sar/src/msg_socket.rs`): `sonamu-msg open <applet> [args]` becomes `StandardEvent::Open` to the root, which adds that registered applet after the focus (`NodeHolder::open`). `assets/sonamu-file-manager.desktop` makes the file manager the default for folders.
- World tree, paths ("inside" a node), focus, selectors, and who owns what (incl. projects, which are plain applets holding a `NodeTree`): `.claude/WORLD_TREE.md`.

# Rules

- Do not change more than asked.
- Keep changes small and simple.
- Report issues we missed.
- Follow general Rust idioms.
- Use minimal tokens.
- Update the files in .claude as things are discovered and changed.
