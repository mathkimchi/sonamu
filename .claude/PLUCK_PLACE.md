# Pluck and place

The user's spec (2026-10-15): the server asks the root to pluck at a world tree path. The structural node that owns the
requested element asks its applet first; if the applet has nothing pluckable, the node plucks itself (applet + children).
The server holds the plucked tree. Place goes down the same way; if the bottom applet can't place, its node adds the tree as a child.
Alt+P does both (first global keybind). The user then said: make the decisions, implement, and log them here.

## Decisions (mine, not yet reviewed)

### Protocol (`singularity_common::sap`)

- **`detached.rs`**: `DetachedClient` (a client with no holder: event sender, shared states, and its request channel with queued requests)
  and `DetachedTree { main, wl_surface, children }`.
  `ClientHandle::detach()` / `DetachedClient::attach(loop, on_request)`.
- **Request channel is type-erased** (`Box<dyn Any + Send>`, a `calloop::channel::Channel<Request>`), so sar can hold trees of
  `NodeRequest` clients without depending on sttk. `attach` with the wrong request type panics (a programming bug).
- **`ClientHandle` registers its request channel with `calloop::Dispatcher`**, so `detach` can take the channel back out.
  `ClientHandle` is no longer `Send` (it keeps a `LoopHandle`); nothing needed it to be.
- **Requests queued while detached are replayed on attach** (the channel keeps them; its ping fd stays readable).
- **Events** (holder to client): `Pluck(Option<WorldTreePath>)`, `Place(Option<WorldTreePath>, DetachedTree)`.
  `None` means "at your focus" (sar doesn't know the root's focus path). Paths are relative to the client's own treeview.
- **Requests** (client to holder): `Plucked(Option<DetachedTree>)` answers `Pluck`; `Placed(Option<DetachedTree>)` answers `Place`
  (`Some` gives the tree back). `RequestPluck(path)` / `RequestPlace(path)` ask the holder to start one (eg a treeview drag),
  like a wl client asking the compositor to start a drag: the holder stays in charge of the plucked tree.
- `StandardRequest` is no longer `Copy` (it now carries trees). Only `UIEvent` needed to stay `Copy`.

### Server (sar)

- Holds `plucked: Option<DetachedTree>`, at most one. Alt+P plucks at focus if empty, else places at focus.
- At most one pluck/place in flight (`pluck_place_pending`); Alt+P while pending is ignored.
  A `RequestPlace` that arrives while the pluck is still in flight is queued and sent once `Plucked` arrives
  (a fast drag can release before the pluck round trip ends).
- Shows the plucked tree's labels in a small box that follows the cursor (like a drag icon), reading the detached
  treeviews without acking.

### Root (`RootNodeApplet` / `NodeTree`)

- Path resolution: `path[0]` picks the structural node (`NodeTree::node_at`), `path[1..]` is inside its applet.
  An empty path means the root node.
- Pluck: if `path[1..]` isn't empty, the node's applet is asked (`Pluck(Some(path[1..]))`); otherwise, or if it answers `None`,
  the node plucks itself. Focusing a node (`Focusing`) gives a 1-long path, so Alt+P on it plucks the node directly;
  `Inner` focus gives `[node, root]`, so the applet is asked first (and every current applet says `None`).
- **The root node can't be plucked**: the answer is `Plucked(None)`. An invalid path also answers `None`.
- **Structural plucks run after the event batch** (`to_pluck`), never inside a request callback:
  calloop panics in `into_source_inner` if the dispatcher being detached is the one currently dispatching
  (the applet's `Plucked(None)` arrives in exactly that callback).
- After a pluck, the parent becomes focused (`Focusing`) if the plucked node was its focused child.
- Place: goes to the deepest existing node along `path[0]` (so an outdated path still places somewhere close).
  If the node was found exactly and `path[1..]` isn't empty, its applet is asked first; `Placed(Some(tree))` or a
  1-long path adds the tree as the node's **last child**. The placed node gets focus (`Focusing`, so you see it and can move it).
- Placed nodes get new `NodeId`s (the slotmap key); `on_request` re-tags requests with the new id.
  Wl tiles keep their surface (`DetachedTree::wl_surface`), re-added to `NodeTree::wl_surfaces`.
- Inner focus of placed nodes is reset to `Focusing`.
- One pluck/place in flight per root (`pending`); a reply from a different node is logged and dropped
  (dropping a tree ends its clients).
- `StandardApplet` gets `pluck`/`place` with defaults (`None` / `Err(tree)`), handled in `AppletClient`'s loop.
  No current applet overrides them.
- Inner applets' `RequestPluck`/`RequestPlace` are prefixed with their node's path and forwarded to sar.
- **Drop paths are re-resolved by node** (`RootNodeApplet::last_drop`): a fast drag can drop before its pluck finishes,
  so the dropped path was computed on the tree before the pluck, and the pluck can shift indices
  (eg drag `[0]` onto `[1]`: after the pluck that node is `[0]`). The root remembers the node of its last drop and,
  when `Place` comes back with that path, uses the node's current path. If that node was plucked too (dropped onto
  the dragged part), it places at the focus (the plucked node's parent) instead.
- `NodeTree::focus_node` was pulled out of `focus_wl_surface` (shared with click-to-focus and place).
- Removed the root's `println!("Focused path: ...")` that printed on every render (it was in the replaced treeview code).

### Treeview UI

- Replaced the per-layer columns with one outline: a row per node (and per inner element of an applet, dimmer),
  indented by depth with guide lines. Rows are placed absolutely at a fixed height, so the root can hit-test clicks itself
  (sar does the layout, so the root can't ask it where things ended up).
- Colors: focused row filled, hovered row lighter, drag source dimmed, drop target outlined.
- Mouse: click a row focuses it (a node row gets `Focusing`; an inner-element row gets `Inner`, since focus inside applets is TODO).
  Press and drag past a threshold sends `RequestPluck(row)`; releasing sends `RequestPlace(row under cursor)`,
  or `RequestPlace` at the focused node if released outside the treeview.
- A drop on a row's top/bottom edge inserts a sibling (`drop_target`), in the root's tree and in inner trees alike
  (projects and splits insert via their `NodeHolder`); roots, inner roots and `‹` rows are appended to.
  Applets without a `NodeTree` ignore the position.
- Keyboard focus paths still work the same.
- Marker bar on the focused row: yellow when the node itself is focused (traversal keys), green when its applet is (keys go in).
- Mouse events over the treeview, or during a press that started there, don't reach applets.
- Wheel scrolling over the treeview is eaten (no scrolling yet, so long trees run off the bottom).
- Labels clip at the panel's width (text is always 46px, so at 20% of a narrow window most labels clip).

## Verification

- `cargo check --workspace --all-targets` passes; clippy has nothing new in the touched files.
- A throwaway headless example (deleted) drove the root with a fake server, and all of these behaved as expected:
  build a 3-level tree with clicks and keys; pluck with `Inner` focus (applet asked, says `None`, node plucked after the batch);
  place at focus; drag-pluck a node with a child (`RequestPluck`, tree comes with its child) and drop on the root row;
  re-attached clients still work (retitle shows up, Ctrl+W closes the right node, so requests carry the new `NodeId`);
  a fast drag whose drop arrives before the pluck lands on the right node; plucking the root answers `None`.
- Ran `singularity_sde` live twice (~12 s): no panics, and screenshots show the new treeview.
- **Not checked**: Alt+P in sar, the drag ghost, hover and drop-target looks, and plucking wl tiles, since I can't send
  input to the live window. sar's pluck/place state machine only ran through the fake server's copy of it.

## Questions for the user

- Should place insert as a sibling (after the target) instead of the last child? Dragging onto a row currently makes it a child.
- Should the plucked tree survive Alt+P while a pluck is in flight, or should Alt+P cancel?
- A wl surface destroyed while plucked: the server still holds its tile, and it's placed back dead (`WlSurfaceDestroyed` only reaches the root's tree).
- If an applet never answers `Pluck`/`Place` (eg it panicked), the root and server stay pending forever.
- Should the plucked tree keep its inner focus (`FocusIndex`) instead of resetting to `Focusing`?

## Issues found

- **Rect colors are drawn as linear, text colors as sRGB**: `Backgrounded(#121418)` shows as about `#4b5059`.
  The treeview's and ghost's rect colors are pre-darkened to compensate (commented). Worth fixing in the renderer
  (probably the surface format is sRGB while rect colors aren't converted), then the constants need updating.
- A 1px `Backgrounded` rect drew as a bright line (likely the known "weird border lines" FIXME), so I dropped the separator.
- Each tree level adds one ack round trip before a nested applet's treeview change shows (applet publishes, root reads on its
  next render, which waits for the server's ack). Fine with sar acking every redraw, but it's why headless tests need settling.
- `StandardEvent::FocusChanged` hits `todo!()` in `CommandHubApplet` (and others), so the root can never send it.
- The previous ClientHandle refactor says `ClientHandle` was `Send`; it isn't anymore (keeps a `LoopHandle`).

## Suggested CLAUDE.md updates

- "Not done yet" should drop "moving nodes between owners (pluck and place)" and mention what's left: wl surface destroyed
  while plucked, applets that never answer, inner-applet pluck/place (no applet overrides `pluck`/`place` yet).
- Architecture: `ClientHandle::detach`/`DetachedClient::attach`, sar holds `plucked`, Alt+P is the first global keybind (in sar).
- Rect colors are linear (see above).
