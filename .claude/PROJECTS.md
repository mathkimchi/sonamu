# Projects as inner trees

The user's request (2026-10-16): a project node should hold everything in the project as its inner tree
(before, `project` was only a flag that narrowed the view). Plan agreed: 1. focus crossing into inner trees,
2. pull the tree logic out of `RootNodeApplet`, 3. make projects that. Decided by me, logged here; no commits.

## 1. Focus crossing into inner trees

- **The treeview state carries the focus**: `SharedState<Treeview>` with `Treeview { tree, focus }` (in `sap::packets`),
  instead of a separate state, so one publish/ack covers both. `ClientHandle::get_treeview` returns it;
  `DetachedClient::peek_treeview` still returns just the tree.
- `focus` is a path of `tree`; a trailing root path means "inside that element" (same convention the treeview already used).
- New `StandardApplet::get_focus_path` (default: inside the root) and `StandardApplet::focus(path)` (default: nothing).
  Applets damage their treeview when their focus path changes.
- New `StandardEvent::Focus(path)`: `NodeTree::focus_path` sends it to the applet when the path goes into it
  (eg clicking an inner row in the treeview).
- `NodeTree::get_focus_path` uses the applet's published focus for `Inner` nodes (was always the inner root).
- **Leaving an inner tree**: a traversal that passes a tree's root is kept in `NodeTree::escaped`; a project sends it to
  its node as `ChangeFocus`. `Inner` + `Parent` now focuses the node (like `PrevLayer`), since the inner root is the node's row.
- The treeview shows while traversing **by focus path** (`is_inside_element`), not by `is_inner` of the focused node,
  so traversing inside a project still shows it.
- File manager: focus path is inside the active document while the document pane has the keys; `focus` picks a document
  (or the file list for its root). Its treeview is damaged on pane/document changes.

## 2. `NodeHolder` (`nodular_applet/node_holder.rs`)

- Holds a `NodeTree` and does what the root and projects share: pluck/place (`pending`, `to_pluck`), node requests,
  wl surfaces, wl keyboard focus reports, Super+P. Generic over the request type it sends up
  (`StandardRequest` to sar for the root, `NodeRequest` to its node for a project).
- ~~Paths are `[0] ++ tree path` for a project~~ (superseded by "Round 2": paths are of its tree).
- `handle_node_request` / `handle_ui_event` return a request for the holder's own node when the holder can't do it
  itself (a split of a project's row, see Round 2). The root logs and drops it.
- `client_spawners(loop, fn)` makes the spawn/attach closures; `NodeTree` now spawns `RawClientInitializer<NodeRequest>`s
  (so it can spawn a project) and wraps applet initializers in `AppletClient` itself (it got the spawner registry).
- Root-only stuff stays in `RootNodeApplet`: treeview panel and mouse, `last_drop`, help, Super+V, Super+?.

## 3. Projects (`nodular_applet/project_applet.rs`)

- `ProjectApplet` is a raw client (`RawClientInitializer<NodeRequest>`, own loop, like the root), not a `StandardApplet`:
  its pluck/place must wait on inner applets, which the sync `StandardApplet::pluck`/`place` can't.
- ~~**Treeview**: root `Project`, with the inner tree's root as its only child~~ (superseded by Round 2).
  Its rows show dimmer, like other inner elements.
- **Making one** (Super+P, hub `project`, `SetProject(true)`): after the batch (detach panics in a request callback),
  the node's subtree is detached and becomes the inner tree of a new project node in its place, focused `Inner`.
  Inner focus starts at the project's root, `Focusing`. Focus states inside the moved subtree reset (as with place).
- **Dissolving** = closing the project: `CloseRequest` makes it detach everything and send `NodeRequest::Unwrap(tree)`,
  and its node is replaced by the tree (`NodeTree::unwrap`), then its thread ends.
  Triggers: Super+P on the project row, or on its root (Super+P / hub `project off` / `SetProject(false)` from the root).
  `SetProject(true)` on a project's root does nothing (no project wrapping just a project).
- Super+P with focus inside a project is passed to the project, so it acts on the innermost focused node.
- **Display**: the project shows its tree's panes (no treeview of its own). The root still narrows its view to a project
  focus moves into (`Node::project` now means "its main is a `ProjectApplet`").
- **Place onto a project row** asks the project, which places under its root (before: it became the node's child).
  Plucking the project row, or its root, plucks the whole project.
- ~~Closing a project's root with nothing under it closes the project~~ (Round 2: closing its row dissolves it).
- **Wl surfaces**: a new surface goes inside the focused project (passed down). The project reports its focused wl surface
  up with `WlKeyboardFocus` (`Node::inner_wl_focus`), so keyboard focus works. `WlSurfaceDestroyed` not found in a tree
  is sent to all its projects.
- Projects can nest. The file manager refuses to place a project as a document.

## Round 2 (2026-10-16): collapsible nodes, projects get children

The user: all nodes with inner trees should be collapsible; project nodes couldn't get children
(the `Project` row only ever had its single root under it).

### Projects get children

- **A project's tree root is a container** (no main, `Layout::Single`) and *is* the project's row: its treeview is its
  tree, with the root labeled `Project`. Making a project puts the node's subtree under that container.
  Paths of a project are plain tree paths again (no `[0]` prefix).
- **A project node is never `Focusing` itself**: `NodeTree::focus_into_project` (after each batch) turns it into `Inner`
  and sends `Focus([])`, focusing the container. The project publishes an empty focus path then, so the root draws it as
  the project's row being focused (yellow marker and border, treeview shown), not as inside it.
- **Traversal past the container** comes up as `ChangeFocus`, which the holder applies as if from the project node
  being `Focusing` (`NodeTree::leave_project`; ignored if focus isn't inside the project anymore).
- **Nodes with no main (containers and splits) take the standard keybinds** (`NodeTree::keybind_request`, done by the
  holder as if the node's applet asked; checked before traversal keys, since Ctrl+W would otherwise be `w`).
  So on a project's row: Ctrl+Shift++ adds a child to the project, Ctrl+W closes (dissolves) it,
  Ctrl+Shift+| / _ split the project node (passed up to its node). Splits got this too (eg Ctrl+Shift++ adds a pane).
- A container shows the child focus was last on (`Node::last_child`) while focused itself, instead of nothing.
- **Dissolving** drops the container: its children take the project's place (`NodeTree::unwrap`), and the first one is
  focused if the project was. A project that is the root with several children keeps the container as the root
  (shown as `(empty)`), since the tree needs one root.
- Wl surfaces registered while a project is focused go inside it even when its row is focused (to its root).

### Collapsing

- **Super+C** collapses or expands the focused node's inner tree in the treeview (`Node::collapsed`). Only nodes whose
  main has inner elements (or are collapsed) toggle. With focus inside a project below its row, the project gets the key
  (it collapses its own focused node), like Super+P.
- Labels: `▾` before a node with inner elements, `▸` before a collapsed one (whose inner rows are hidden).
  A project's row is `▾ Project`.
- If the focused row is hidden in a collapsed node, that node's row shows the focus (longest visible row prefix).
- Not done: collapsing with the mouse (no click target on the arrow), keeping collapsed state through pluck/place,
  collapsing a node's child nodes (only its main's inner tree collapses).

### Verified (headless)

Ctrl+Shift++ twice on a project row adds two top-level children (plucking `0|1` takes the second); Super+C collapses
and expands the project row; dropping on a collapsed project's row places inside; Ctrl+Shift+| on the row splits the
project node; Ctrl+W on the row dissolves it into its parent (both children spliced in); Alt+q on the row leaves the
project; the round 1 wl and root-project scenarios still work; Super+C on a file manager collapses its documents.

## Round 3 (2026-10-16): world tree semantics, selector keys

The user: I misread the world tree. A node has a value (raw, or an inner world) and children; `e`/`q` go into/out of
a layer. Round 2 gave projects an inner tree but no children. Path "inside" must be documented. Space collapses,
Enter only confirms, modifier keys don't leave the selector.

Superseding round 2:
- (Undone by #83: the two are one row and one path, `[Child(p)]`.) **On the project (`[[p]]`) and inside it at its inner root (`[[p], []]`) are distinct again.** Removed
  `focus_into_project`, `leave_project`, the empty published focus and the project-row border special case.
  The inner world's root is still a container labeled `Project` (shown on the project's row, as an inner root is).
  Making a project focuses the project node itself.
- **Keybinds in the treeview selector act on the selected node** (`NodeTree::keybind_request`, for every focused node
  now, not just ones with no main): so Ctrl+Shift++ on a project (not inside) adds a child to the project node in the
  tree holding it; inside at its root, it adds to the project's contents. Same for splits.
  Ctrl+W on a project or wl tile sends it `CloseRequest` (a project dissolves, a wl client is asked to close;
  `WlSurfaceApplet` now handles `CloseRequest`); others close directly.
- Dropping on the project's row places as its child again; registering a wl surface while on (not inside) the project
  adds it as the project's child.

Selector keys (`NodeTree::handle_event`, `Focusing` or no main):
- Space collapses/expands the node's inner tree (replaces Super+C). Enter confirms: into the main (`Inner`), the key
  isn't forwarded; nothing for nodes with no main. Enter is no longer `PrevLayer` here (`from_char` still maps `\n`,
  but Enter is handled first; `singularity_common` untouched).
- Key releases and keys with no symbol (modifiers on their own) are ignored instead of entering the applet.
- **Treeview visibility** uses `NodeTree::is_traversing` instead of the focus path: inside a project at its root is a
  path inside the project but still the selector. Holders publish it as `Treeview::traversing` (applets: `false`).
- "Inside" is documented on `WorldTreePath` and `Treeview::focus`.

Verified headless: Ctrl+Shift++ on the project vs inside it (plucked back from `0,0` and `0|1` respectively); drop on
the project row becomes its child; Space collapses only the inner tree (the project's child stays); Shift alone keeps
the selector; Enter on a hub hides the treeview, on a project goes inside at its root (treeview stays); `q` from a
project's child goes out to the project; Ctrl+W on the project dissolves it; Ctrl+W on a selected wl tile keeps the tile
(waits for the client); a wl surface opened on the project becomes its child.

## Round 4 (2026-10-16): projects are plain applets; file manager selector

The user: the file manager should support traversing its inner tree; a project should just be a `StandardApplet`,
since the server (the holder of its node) doesn't know or care that it is a project.

Superseding earlier rounds:
- **`ProjectApplet` is a `StandardApplet`** run by `AppletClient`. Its contents' mains are embedded
  (`AppletContext::embed`/`embed_detached`), and their `EmbeddedId`s map to its `NodeTree`'s `NodeId`s.
- **Removed every project special case outside it**: `Node::project`, `DetachedTree::project`, `NodeRequest::SetProject`,
  `NodeTree::make_project`/`is_project`/`projects`, the holder's Super+P handling and Close/wl special cases, and the
  view root narrowing into projects (Super+V still narrows by hand). `NodeTree` spawns `NodularAppletInitializer`s again
  (no raw clients, no spawner registry in it); `client_spawners` takes the registry.
- **Generic replacements** (`nodular_applet/mod.rs`):
  - `NodeRequest::Wrap(WrapInitializer)`: the holder replaces the node's subtree with a node whose applet is made from it
    (after the batch). `ProjectApplet::wrapper()` makes a project; Super+P is now a standard keybind sending it (so it
    acts on the selected node in the selector, or the applet's node inside one); hub `project` sends it too.
    `project off` is gone: Ctrl+W on the project (or its inner root) undoes it. Wrap on a project's own inner root is ignored.
  - `Unwrap` stays (generic: replace my node with this tree).
  - `StandardApplet::close_requested` (default: close). Ctrl+W in a selector sends `CloseRequest` to a node with a main;
    `WlSurfaceApplet` asks its client, `ProjectApplet` dissolves.
  - `pluck`/`place` return `Answer<_>`; `Later` means the applet answers itself (the project, through its holder).
    `Pluck(None)`/`Place(None)` now use the applet's focus path instead of its inner root.
  - `StandardApplet::take_wl_surface` (default: give back, then `NodeRequest::AddWlSurface` makes it the node's child).
    Holders pass a new surface to the focused node's main if focus is inside it; destroyed surfaces not found in a tree
    are sent to every main.
  - `StandardApplet::update` (after each batch) and `is_traversing` (published as `Treeview::traversing`).
- `NodeHolder` is no longer generic; it sends through a `Box<dyn Fn(StandardRequest)>`.
- `e` (NextLayer) on a node now sends its main `Focus([[]])`: into its inner world, at the root. Enter doesn't.

File manager selector (`Selected::{Root, Document(i)}`, `selecting: Option<Selected>`):
- Entered by `e` on its node (`Focus([[]])`), Alt+q in the file list (selects its root), Alt+q in a document (selects it),
  or clicking its rows in the treeview (`Focus([[i]])`). Published as `traversing` and focus `[[]]`/`[[i]]`.
- Keys: `d`/digits/`9` from the root pick a document; `w`/`s` move between documents; `a` from a document goes to the
  root, from the root out to its node; `q` out to its node; `e` on the root goes into the file list, on a document into it
  (`Focus([[]])`); Enter confirms (list or document input); Space does nothing (documents have no inner trees); other keys
  confirm and are handled there. Standard keybinds: Ctrl+W on a document closes it; others go to the file manager's node.
- Clicking a pane leaves the selector (`set_pane`). Its `pluck` at the root now means the file manager itself (the old
  "active document" case is covered by plucking at the focus path).
- Not done: highlighting the selected document in the file manager's own UI (only the root's treeview shows it).

Verified headless: Super+P in the selector wraps the node; Ctrl+Shift++ on vs inside the project (plucked back from `0,0`
and `0|1`); Space collapses the project; `e d Enter` into a hub inside the project hides the treeview, Alt+q `q` back to the
project, Ctrl+W dissolves it; wl surfaces inside a project (keyboard focus, destroy). File manager: two documents,
Alt+q selects the active one, `w` the previous one, Enter into it, Alt+q `a` `q` `e` back; plucking at the focus while
`w` selected the first document plucks `b.txt`.

Issues:
- A project placed into the file manager as a document works, but if it dissolves there its contents are dropped
  (the file manager ignores `Unwrap`; it would need to hand them to its node).
- `ClientHandle`s of embedded clients that close are removed from the project's tree, but their `EmbeddedId` map entries
  stay (small leak per closed node).
- The project's `update` damages via `AppletContext::damage_*`, which is a channel message, so its redraw is one loop
  iteration later than the change.

## Verification

- `cargo check --workspace --all-targets` passes; clippy has nothing in the touched files; `cargo test --workspace`
  passes except the existing `no bin target named editor` error (also on the clean tree).
- Headless harness (`singularity_sde/examples/headless.rs`, got `wlx` and `focus:` steps) showed all as expected:
  make the root a project and add a child in it; traverse out of it with keys; dissolve it with Super+P; a wl surface
  added inside a project gets keyboard focus, loses it on Alt+q, and its tile closes on destroy; pluck from inside a
  nested project and place at the root; focus into a project by path, then Super+P dissolves it from its root;
  drop onto a project row lands inside; plucking a project's root takes the whole project; clicking a file manager
  document by path hides the treeview (focus is inside).
- **Not checked**: running it live and looking at it (pane borders, treeview indentation, dragging with the mouse).

## Issues / not done

- ~~`FocusWlSurface` inside a project is ignored~~ Fixed: `WlSurfaceEvent::Focus` goes straight to the surface's applet, which sends `NodeRequest::FocusSelf` up through the project.
- ~~A wl surface destroyed while its project is plucked stays as a dead tile~~ Probably fixed (untested): `WlSurfaceEvent::Destroyed` reaches the applet inside the plucked project, which closes itself.
- Panes inside an unfocused project still draw focus-colored borders (the project's tree doesn't know it's unfocused).
- If a project dissolves while a pluck/place is pending inside it, the answer is lost and the holder above stays pending.
- Alt+q in `Focusing` state is treated as plain `q` (modifiers aren't checked for traversal keys). Existing, but more
  visible now since it crosses out of projects.
- Each nesting level adds an ack round trip before inner changes show (existing, deeper now).
