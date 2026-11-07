# Projects as inner trees

A project node holds everything in the project as its inner tree (2026-10-16; before, `project` was only a flag that
narrowed the view). How focus and traversal cross into it is in `.claude/WORLD_TREE.md` (#83: the root traverses by
shown rows; a project's row and its inner root are one row and one path).

## `NodeHolder` (`nodular_applet/node_holder.rs`)

- Holds a `NodeTree` and does what the root and projects share: pluck/place (`pending`, `to_pluck`), node requests,
  wl surfaces, wl keyboard focus reports, wrapping. It sends up through a `Box<dyn Fn(StandardRequest)>`.
- Paths it takes and sends are of its tree (`WorldTreePath`, see WORLD_TREE.md).
- `handle_node_request` / `handle_ui_event` return a request for the holder's own node when the holder can't do it
  itself (eg wrapping a project's row). The root logs and drops it.
- `client_spawners(loop, registry, fn)` makes the spawn/attach closures.
- Root-only stuff stays in `RootNodeApplet`: the treeview panel and mouse, traversal, `last_drop`, help, view root keys.

## `ProjectApplet` (`nodular_applet/project_applet.rs`)

- A plain `StandardApplet` run by `AppletClient`; the server (the holder of its node) doesn't know it is a project.
  Its contents' mains are embedded (`AppletContext::embed`/`embed_detached`), their `EmbeddedId`s map to its
  `NodeTree`'s `NodeId`s.
- **Its tree's root is a container** (no main, labeled with the project's name) and *is* the
  project's row: its treeview is its tree. Its panes are the container's children.
- **Making one**: `NodeRequest::Wrap(ProjectApplet::wrapper())` (Super+P, the menu's Make project, hub `project`):
  after the batch, the holder replaces the node's subtree with a project node holding it. Wrap on a project's own root
  is ignored.
- **Dissolving** = closing it: `CloseRequest` (Ctrl+W on its row) makes it detach everything and send
  `NodeRequest::Unwrap(tree)`; its node is replaced by the container's children (`NodeTree::unwrap`), the first focused
  if the project was. A project that is the root with several children keeps the container as the root (`(empty)`).
- **New children go inside** (`holds_children`, #83): keybinds, New child ›, drops onto its row, opened applets and wl
  windows for the project's node go to its root.
- **Pluck/place** answer later (`Answer::Later`), through its holder. Plucking its row plucks the whole project.
  `Pluck(None)`/`Place(None)` use the applet's focus path.
- **Wl surfaces**: one registered while focus is on or inside the project goes inside it. It reports its focused wl
  surface up with `WlKeyboardFocus` (`Node::inner_wl_focus`); `WlSurfaceEvent::Focus` goes straight to the surface's
  applet, which sends `NodeRequest::FocusSelf` up through the project. Destroyed surfaces not found in a tree are sent
  to every main.
- Projects can nest. The file manager refuses to place a project as a document.

## Other generic pieces this added (`nodular_applet/mod.rs`)

- `StandardApplet::close_requested` (default: close); `WlSurfaceApplet` asks its client, `ProjectApplet` dissolves.
- `StandardApplet::take_wl_surface` (default: give back, then `NodeRequest::AddWlSurface` makes it the node's child).
- `StandardApplet::update` (after each batch) and `is_traversing` (published as `Treeview::traversing`).
- Keybinds in the treeview selector act on the selected node (`NodeTree::keybind_request`), as if its applet asked.

## Issues / not done

- A project placed into the file manager as a document works, but if it dissolves there its contents are dropped
  (the file manager ignores `Unwrap`; it would need to hand them to its node).
- `ClientHandle`s of embedded clients that close are removed from the project's tree, but their `EmbeddedId` map entries
  stay (small leak per closed node).
- The project's `update` damages via `AppletContext::damage_*`, a channel message, so its redraw is one loop
  iteration later than the change. Each nesting level adds an ack round trip before inner changes show.
- Panes inside an unfocused project still draw focus-colored borders (the project's tree doesn't know it's unfocused).
- If a project dissolves while a pluck/place is pending inside it, the answer is lost and the holder above stays pending.
