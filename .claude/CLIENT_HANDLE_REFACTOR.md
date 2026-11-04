# ClientHandle refactor

Replaces `BasicApplet`/`NodularApplet` (`StandardApplet` + hooks) with per-applet clients behind `ClientHandle`.

## Plan (agreed in grilling session, 2026-10-15)

### Architecture

- The sar server holds only the root `ClientHandle`. The tree lives in the root client thread (`RootNodeApplet`).
- ~~Node shape: `RecursiveNode { main: NodeMain, children: Vec<RecursiveNode>, focus }`,
  `enum NodeMain { Applet(ClientHandle), Wayland(WlSurfaceId) }`.~~
  **Revised 2026-10-16:** the tree is a slotmap, `NodeTree { root, nodes: SlotMap<NodeId, Node>, wl_surfaces: HashMap<WlSurfaceId, NodeId>, .. }`
  with `Node { main: ClientHandle, parent, children: Vec<NodeId>, focus }`.
  Every main is an applet: wl tiles are `WlSurfaceApplet`s again, and the tree records which node shows which surface.
  Why: lookups by `NodeId` and walking up via `parent` (instead of DFS and index paths), one kind of node to move around
  for ownership transfer, and wl tiles can later live in inner trees like any applet.
  Cost: wl input takes one more thread hop (root → wl applet → sar), and each wl client gets a mostly idle thread.
  `CachingApplet` is deleted. No `Option` main / layout field yet: splits aren't planned for now.
- Every applet runs on its own thread. It owns its inner tree directly (may embed its own `NodeTree`, later)
  and changes its children only through requests to its owning node.

### Protocol (`singularity_common::sap`)

- `ClientHandle` moves to common, generic over the event loop data, with an `on_request` closure.
- `RawClientInitializer<Req>` is generic: sar uses `StandardRequest`, sttk uses `NodeRequest`.
- `SharedState<T>` replaces the hand-rolled lock + damage + ack.
  `Damage(StateKind)` / `DamageAck(StateKind)` replace `DamageSurface`/`DamageTreeview`/`SurfaceDamageAck`/`TreeviewDamageAck`.
  Used for surface and treeview.

### sttk API

- `StandardApplet` trait (handle event, render, treeview) plus a generic initializer that runs the applet's loop and publishes `SharedState`s.
- `AppletContext`: clonable struct given at construction (`new(args, ctx)`), with request helpers and direct access to the shared spawner registry (`Arc<RwLock<BTreeMap<..>>>`).
- Requests from applets are tagged with their node's `NodeId`.
- Standard keybinds stay applet-side.

### Deferred (TODO comments)

- Focus path inside an applet's inner tree, and focus crossing into inner trees (forward in, bubble out).
- Wl surfaces inside inner trees.
- Ownership transfer (groundwork for pluck and place, popping apps out). Proposed, not approved yet:
  `ClientHandle::detach() -> DetachedClient` (`Send`, keeps queued requests, via calloop `Dispatcher::into_source_inner`)
  and `DetachedClient::attach(loop, on_request) -> ClientHandle`; on the tree, take/insert a subtree by `NodeId`,
  and detach/attach a subtree (new `NodeId`s on attach) for moves to another holder or thread.

### Scope

sttk + SDE + common + sar first, then the standard tabs that are currently compiled
(`code_editor`, `image_viewer`, `one_char_render_test`), until Sonamu runs with the existing applets and wl clients.

## Notes and decisions made during implementation

### Decisions I made on my own

- **Kept old names** to keep the tab diffs small: `StandardApplet`, `NodularAppletInitializer`, `AppletSpawner(Trait)`,
  `CreatableNodularApplet`, the `hook` field name in applets, and the hook method names on `AppletContext`
  (`damage_window`, `damage_treeview`, `close`, `add_child`, `quit`, spawner methods; `change_focus` went with #83).
  `RecursiveNodeApplet` became `RecursiveNode` since it's no longer an applet; after the revision it's `NodeTree`,
  in `nodular_applet/node_tree.rs` (was `recursive_node_applet.rs`).
- **`StandardApplet` lost** `get_focus_path`, `contains_wl_surface` and `focused_wl_surface`. The node works these out itself;
  the focus path inside an applet is a TODO placeholder (`WorldTreePath::new_into()`, same as every applet returned before).
- ~~**`NodeId` is an `AtomicU64` counter**~~ Revised: `NodeId` is the `NodeTree` slotmap key.
- **`SharedState<T>` + `StatePublisher<T>`** live in `singularity_common::sap::shared_state`.
  The publisher renders lazily and keeps at most one unread publish; damage while waiting for the ack is merged into the next one.
  This also fixes an old `RootNodeApplet::try_redraw` bug: damage that arrived while a publish was unacked was dropped.
- **`ClientHandle::get_surface`/`get_treeview` always ack**, as before. Acks are idempotent now, so extra ones are harmless.
  Applets that aren't shown are never read, so they stay waiting for an ack: when revealed, their last published frame shows first
  and a fresh one arrives one round trip later.
- **`AppletContext::damage_*` goes through a channel to the applet's own loop**, so it works from any thread and wakes the loop.
- **Applets render at their last `WindowResized` size** (800x600 until their node sends one); a resize damages the surface automatically.
  Before, the parent passed the size at render time.
- **Requests from applets** reach the root through each `ClientHandle`'s closure, tagged with the `NodeId`.
  ~~**Requests nodes make themselves** go into `NodeEnv::pending_requests`~~ Revised: `NodeEnv` and the pending queue are gone.
  `NodeTree` sees the whole tree, so it handles traversal keys and wl destruction directly,
  and wl tiles send their keybind requests like any applet.
- **No `CloseWithChildren` request**: applets can't hand over children. `Close` closes the applet's node and the parent adopts its children.
- ~~**Wl tiles are `NodeMain::Wayland`**~~ Revised: wl tiles are `WlSurfaceApplet`s, each holding its own `wl_request_queue` clone.
  It uses `standard_keybind_request`, except that Ctrl+W sends `WlSurfaceRequest::Close` instead of closing the tile.
  `AppletContext::request` is now public, and `handle_standard_keybinds` just forwards `standard_keybind_request`'s result.
  `NodeTree::wl_surfaces` maps each surface to its node for `WlKeyboardFocus` and restoring.
  Revised again: focus, destroy and title come straight from the compositor as `WlSurfaceEvent`s on the surface's own
  `wl_event_queue` (`AppletContext::listen_wl_surface`), not as `StandardEvent`s routed down the tree.
  New wl tiles are added as children of the top node, as before.
- **Every node is displayed at the tree's size** (`NodeTree::latest_size`), not a per-node size (they were always equal).
- **`RawClientInitializer::init`** takes a new `treeview` argument (separate arguments, not a bundled struct).
  The root ignores its own treeview, since it draws the treeview into its surface; sar ignores `Damage(Treeview)`.
- **Generic `ClientHandle` ignores a closed request channel** (calloop removes the source).
  Before, sar panicked when the root closed; now the root thread's own panic message is what you'd see.
- **`find_applet_spawner` takes `&str`** (clippy `needless_pass_by_value`; also resolves the old TODO there).
- **Deleted** `basic_applet.rs`, `caching_applet.rs`, sar's `client_handle.rs` (moved to common),
  and the commented-out `NodularHolderApplet` block in `root_node_applet.rs`, since it referenced removed types.
- **Scope:** `terminal`, `browser` and `pdf_viewer` were already commented out, so they weren't ported (`pdf_viewer` was deleted since).

### Issues found

- An applet panic now kills only that applet's thread. Its tile keeps its last surface, and nothing cleans it up.
  `ImageViewerApplet::get_window_standard_applet` is `todo!()`, so spawning `image_viewer` does exactly this.
- ~~Closing a wl tile (Ctrl+W) doesn't close the wl client~~ Fixed: Ctrl+W on a wl tile sends `WlSurfaceEvent::Close`
  (xdg `close`), and the tile closes when the client destroys its toplevel (`WlSurfaceDestroyed`).
  So a client can refuse or delay, and a client that ignores `close` keeps its tile.
- `AppletRunner::event_loop` is never read (warning existed before this refactor).
- `.claude/TODO.md`'s "Later: replace nested `StandardApplet`s" section and `.claude/CONTEXT.md`'s "Outer Tree"
  (owned by the server) describe a server-owned tree. We chose a tree owned by the root client instead.

### CLAUDE.md known issues affected

- Gone: the `SharedResource::add_child` deadlock, and the unreachable `WindowResized` case in `RecursiveNodeApplet`.
- Still true but harmless now: `ClientHandle::get_surface` acks on every read.
- Possibly fixed: "closing a wl client leaves its tile". `WlSurfaceDestroyed` closes the tile in the headless test,
  but this isn't checked visually.

### Verification

- `cargo check --workspace --all-targets` passes; clippy has no warnings in the touched crates.
- Ran `singularity_sde` for about 10 s with alacritty connecting to its socket: no panics, the toplevel registered, and frames were drawn.
- A throwaway example (since deleted) drove the root headlessly. The following all behaved as expected:
  typing into the command hub, `add_child command_hub`, Alt+q, `d` into the child, Ctrl+W in the child,
  wl surface registration (resize sent), `FocusWlSurface` (`WlKeyboardFocus(Some)` sent, keys forwarded to the surface),
  `WlSurfaceDestroyed` (tile closed, `WlKeyboardFocus(None)` sent), and Ctrl+Shift+Q (`Quit` sent).
- Not checked: actually looking at it and using it interactively.
- After the `NodeTree` revision: workspace builds, clippy clean in sttk, the same headless run passes
  (plus Ctrl+W on a wl tile sends `Close` and the tile stays until `WlSurfaceDestroyed`), and the live alacritty run has no panics.
