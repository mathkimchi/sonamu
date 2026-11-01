# World Tree and the hierarchy

How Sonamu's one big tree is structured, addressed, focused, and owned.
Background: DEVLOG 2026-02-03 to 2026-02-22 (the original design), and `.claude/PROJECTS.md` (projects; its
rounds 1-4 record how the current design was reached, later rounds superseding earlier ones).

## The data structure

`WorldTree<T>` (`singularity_common/src/utils/tree/world_tree.rs`) is either:

- `Base(T)`: a raw value, or
- `World(RecursiveTreeNode<WorldTree<T>>)`: a tree whose nodes each have a **value** and a list of **children**.
  A node's value is itself a `WorldTree<T>`, so it is either a raw value or a whole **inner world**.

So every node has two directions to go "down":

- its **children**, in the *same* world (same layer), and
- its **value's inner world**, one layer *deeper*.

Example (from the DEVLOG): a project node's children are other projects (same layer).
Its value is the project's own world of applets (deeper layer). An applet's value can in turn be the world of its elements.

## Paths

`WorldTreePath` is a list of `TreeNodePath`s, **one per world**:

- The first picks a node in the outermost world (`[]` is that world's root, `[2, 1]` is child 1 of child 2).
- Each later one picks a node in the **inner world of the node the previous one ended at**.

```text
[[2, 1]]          on node [2, 1] of the outer world
[[2, 1], []]      inside node [2, 1]: at the root of its inner world
[[2, 1], [0]]     inside node [2, 1]: at child 0 of its inner root
[[2, 1], [0], []] inside that one in turn
```

**"Inside" a node** means the path goes past it: it has more components than the path ending at that node.
`[[2, 1]]` is *on* the node; `[[2, 1], []]` is *inside* it.
The treeview draws an inner world's root on the node's own row, so a path ending in `[]` shows on the same row as the node,
but marked as inside (`row_path`, `is_inside_element` in `nodular_applet/treeview.rs`).

## Traversal keys

The treeview selector shows when focus is on a node rather than inside an applet taking input.
Keys (`WorldTreeTraversalOperation`):

- `e`: **into** a layer (NextLayer): from a node into its inner world, at its root
  (the node becomes `Inner` and its main gets `Focus([[]])`).
- `q`: **out of** a layer (PrevLayer): from anywhere in an inner world back to the node holding that world
  (from the outermost world: up to its root).
- `a` / `0`: parent; `d`, `1`-`8`, `9`: first, nth, last child; `w` / `s`: previous/next sibling.
  These all stay in the same world.
- Space (or clicking its `▾`/`▸`, outer-world nodes only for now) collapses/expands the selected node's inner world in the treeview; Enter confirms (into the applet, resuming
  where it was, without `Focus`). Modifiers on their own don't leave the selector; other keys go into the applet.
- The standard keybinds (Ctrl+W, Ctrl+Shift++, splits, Super+P) act on the selected node.
  Ctrl+W asks its main to close (`CloseRequest`, see below) rather than closing it.
  A selected project is a node like any other, so Ctrl+Shift++ on it gives it a child. Inside it at its inner root,
  Ctrl+Shift++ adds to the project's contents instead.

An applet with an inner world can have its own selector over it, with the same keys (the project via its `NodeTree`,
the file manager by hand over its root and documents). While it does, it publishes `traversing: true`.
Alt+Q inside an applet goes to its own selector if it has one (eg the file manager's list or a document), else to its node.

Touchpad swipes (DRM backend only: `UIEvent::Swipe`, a step each time the fingers move `SWIPE_DISTANCE`, like
scrolling; 3 finger swipes act on every step, 4 finger ones only on the first) map to
keybinds in `standard_keybind_request`: 4 fingers up / down is Alt+Q / `e` (out of / into a layer), 3 fingers up / down
is Alt+Shift+Tab / Alt+Tab (previous / next node in depth-first order, landing on the node, not inside its applet).
3 finger swipes skip the applet: its loop (`AppletClient`) sends them straight to its node, unless it
`handles_dfs_swipes` (a project, to move through its own tree). The file manager passes its documents' ones to its node.

## Who owns what

There is no single global tree object. Each part is owned by whoever manages it, and parts are stitched together through
published treeviews.

- **Node** (`nodular_applet/node_tree.rs`): has an optional **main** (an applet client behind a `ClientHandle`)
  and **children** (other nodes). The node's *value* in the world tree is its main's published treeview.
  That treeview is the main's inner world, or just a label.
- **`NodeTree`**: a slotmap of nodes forming one world. It is held by a **`NodeHolder`** (`node_holder.rs`):
  - The root's holder (`RootNodeApplet`, a raw sar client) holds the outermost world. sar only holds the root's
    `ClientHandle`.
  - A `ProjectApplet` holds a `NodeHolder` too, for the project's inner world.
- **Applets own their inner world directly** (eg the file manager's documents, a project's `NodeTree`).
  Applets they hold are embedded (`AppletContext::embed`) on their own loop. They change their own node's children only
  by `NodeRequest`s (AddChild, Close, Split, Wrap, ...) to the holder.
- A **project** is an ordinary `StandardApplet` (`ProjectApplet`). **Nothing above it knows it is a project**: its node
  just sees an applet with an inner world. Its inner world is a full `NodeTree`, whose root is a container node
  (no main, labeled with the project's name, `Project` until renamed), and the project node still has its own children
  in the outer world.
  - While focus is at its inner root, it shows an overview instead of its panes (`.claude/PROJECT_UI.md`): its name
    (`r` or click renames it), its contents (click selects one; with live scaled previews below) and rows adding a command hub (`n`), empty project (`p`), terminal (`t`) or browser (`b`) to it.
  - Made by `NodeRequest::Wrap(ProjectApplet::wrapper())` (Super+P, hub `project`): the holder replaces the node's
    subtree with a node whose applet is made from it.
  - Undone by closing it: on `CloseRequest` (Ctrl+W on its node, or on its inner root) it detaches its contents and
    sends `NodeRequest::Unwrap(contents)`, so they take its place.
- A **split** is a `SplitApplet` the same way: its inner tree's root is the split (`Layout::Split`, no main) and its
  children are the panes. Ctrl+Shift+| / _ wraps the node into one (`SplitApplet::wrapper`) with a new pane; closing it
  gives its panes back, and one left with a single pane is replaced by it. See `.claude/FM_SPLITS_UDEV.md`.

Generic hooks that make this possible (`StandardApplet`, run by `AppletClient`):

- `pluck`/`place` return `Answer::Later` to answer by themselves afterwards (a project asks the applets inside it).
- `close_requested` decides what `CloseRequest` does (default: close; wl tiles ask the client; projects dissolve).
- `take_wl_surface` takes a wl surface registered while focus is inside it (or one a restored tile inside it awaits, `awaiting_wl`); by default it is given back with
  `NodeRequest::AddWlSurface` and becomes the node's child. Its wl keyboard focus goes up as `WlKeyboardFocus`.
- `update` runs after each batch of events (a project finishes plucks and wraps there, outside request callbacks).
- `is_traversing` and `focus` are its own selector, above.

Ownership is what pluck/place moves between: a holder detaches clients from its loop and another attaches them
(`ClientHandle::detach`, `DetachedClient::attach`).

## Focus

Each node stores one `FocusIndex`:

- `Focusing`: the node itself is selected (the treeview selector, yellow marker).
- `Inner`: focus is inside its main (keys go to it, green marker). Where exactly inside is up to the main.
- `Child(i)`: focus is down in child `i` (same world).

The full focus path is built bottom-up:

- A `Child(i)` node prefixes `i` to its child's node path.
- An `Inner` node appends its main's published focus path, which adds more `TreeNodePath`s, one per deeper world.

Moving focus is recursive too:

- A traversal starts at the focused node and is passed up to parents as needed (`calculate_new_focus`).
- If it passes the root of a world, it **escapes** (`NodeTree::escaped`). A project sends it to its own node
  as `ChangeFocus`, which continues in the outer world. That is how `q` from inside a project lands on the project.
  Other applets do the same by hand (`AppletContext::change_focus`).
- Going into a deeper world is done by the node holding it. `NextLayer` makes the node `Inner` and sends `Focus([[]])`,
  and from then on keys go to its main, which moves focus inside its own world.
- `Inner` + `Parent`/`PrevLayer` (from the main) makes the node `Focusing` again: the inner root is the node's own row.

## Screens (#60)

- The root's tree is shown on every screen (`StandardEvent::Screens`), each with its own focus, view root
  (`NodeTree`) and view layer and treeview state (`RootNodeApplet`). The **active** screen's are the usual fields
  (nodes' `FocusIndex`, `view_root`, `panes`, ...); the others are kept in `screens` and swapped in by
  `activate_screen`, which puts focus back on the node it was left on (`NodeTree::activate_screen`).
- An inactive screen shows the panes toward its focus (`displayed_node_toward`), with no focus border, and the
  treeview only if pinned. A click on a screen makes it active (not during a drag); until then the pointer is ignored there (no hover or scrolling), and a fullscreen toplevel there lets the click through to the root.
- An applet shown on several screens is sized to the active screen's pane (else the first), and drawn as is on the others.
- A fullscreen toplevel covers only its screen, and stays while focus goes to another screen by a click there.
- The one wl output is the **main (first) screen** only: layer surfaces (dms' bar and wallpaper) are laid out there
  and only shrink its area; the root covers the whole window. Screen capture is of the main screen.
- **Inside applets**: every `NodeTree` sends its mains `StandardEvent::ScreenViews { sizes, active }` (their size on
  each screen, `None` where not shown, and the active screen) when it changes, right away on `activate_screen` so the
  click that activated a screen reaches the right one. An applet may then publish one window per screen (a container,
  `Treeview::screen_windows`, `StandardApplet::screen_windows`), and holders draw window *i* on screen *i*
  (`screen_surface`). A project does: its tree mirrors the screens (`set_screen_count`, `activate_screen`,
  `resize_screen`), so each screen has its own path inside it, nested projects too. Other screens show its panes,
  never its overview.
- Known gaps: splits (and anything inside them) and other applets look and focus the same on every screen;
  only the active screen's focus is saved in the session; no wallpaper or bar on the other screens (one wl output);
  no keybind to move between screens; monitors are always side by side, top aligned, at scale 1.

## What a client publishes (`Treeview`, `sap::packets`)

Every client publishes, through its treeview `SharedState`:

- `tree`: its inner world (`WorldTree<String>`, labels only),
- `focus`: where focus is in it (a `WorldTreePath` of that tree), and
- `traversing`: whether it is showing a selector itself (`StandardApplet::is_traversing`).

The holder above stitches these in:

- Its treeview puts each node's published tree in as that node's value, its label prefixed with a Nerd Font icon
  picked from the published `saved` (applet kind, or a wl client's app id; `icon` in `node_tree.rs`).
  Applets can't draw images, so there are no real app icons or favicons.
- Its focus path appends the published focus.
- The root shows the treeview per the `treeview` setting (#106): `position` left / right (a column, the panes get the
  rest of the width) or floating (default: centered over the panes, which keep their size; always framed), and
  `pinned` (always shown) or not (default: only while `NodeTree::is_traversing`, or a treeview drag or menu is open).
  Floating is never pinned. See `treeview_area`, `applet_area` and `treeview_frame` in `root_node_applet.rs`.
- The treeview shows the **view layer**'s world (`RootNodeApplet::view_layer`, the holder's path; `None` is the
  outermost), chosen by the user: Alt+R sets it to the world focus is in, a row's right click › Set as view layer to
  the world that row is in, as does a double click on it. Focus going out of it (or its holder going away) moves it out to the next world focus is
  inside (`update_view_layer`). An inner view layer is shown under a `‹ holder` row (`inner_layer_rows`): a click on its `‹` focuses the holding node (out), on its name the inner
  world's root (eg a project's overview; `TreeviewRow::inner_root`). Else it shows the view root's subtree (Super+V).
  Expanded inner worlds are drawn in a box; the panel scrolls with the wheel and follows focus.
- `is_traversing` follows `Inner` nodes down into
  their mains' `traversing`, since a path inside a project can still be a selection (eg `[[p], []]`).

## Paths across holders

Paths sent between a client and its holder are always **relative to the client's own treeview**.
The holder converts them:

- Going up, a holder prefixes the node's path (`NodeHolder::full_path`). Applies to `RequestPluck`, `RequestPlace`
  and focus.
- Going down, it takes the first component to find its node and passes the rest (`inner_path`) to that node's main
  as `Pluck`, `Place` or `Focus`. `Pluck(None)`/`Place(None)` mean "at your focus" (`AppletClient` uses the applet's
  `get_focus_path`).
- `NodeAction(path, action)` (`NodeHolder::node_action`) does something to the node at the path, passed down the same
  way to the holder owning it (`StandardApplet::node_action`; a project's holder, or the file manager for its
  documents). The treeview's `×` button, shown on the hovered row (not the root, not `‹` rows), sends `NodeAction::Close`,
  which asks the node's main to close like Ctrl+W.
- A right click in the treeview opens a menu (#103, `Menu` in `treeview.rs`). On a node's row: New child ›,
  New sibling below ›, Make project (Wrap, like Super+P), Set as view layer, Deactivate, Close, Close with children (asks every node
  in the subtree, parents first). On the root's row only the first, Make project and Set as view layer; on empty space or a `‹` row,
  the applet list for the shown world's root (the view root, or the inner root of the applet whose inner world is
  shown). New › lists command hub, terminal, browser, file manager (in the holder's directory) and an empty
  project, all as `NodeAction::Spawn`. Native ones are added there and focused inside; for a program, the holder
  keeps `wl_target` and the next new wl surface it gets goes there. A spawn passed into an applet focuses inside
  it, so that surface is passed in too. A click elsewhere closes the menu.
  - Deactivate (`NodeTree::deactivate`) puts a stand-in in the node's main, inactive and `hibernated` (its
    `SavedApplet`), so it starts again when focused. Focus on it moves to its parent, else it would wake right away.
    A live wl tile asks its client to close its toplevel (forgetting the surface, so the node stays). A native
    applet with a saved form gets `StandardEvent::Deactivate` (`StandardApplet::deactivate`) and is dropped; waking
    restores it from its saved form (`session::restore`, inner tree and all). Projects and splits stop their whole
    inner tree (`NodeTree::deactivate_all`: wl tiles get `CloseRequest`, native mains `Deactivate`, recursively);
    the file manager closes its wl documents and deactivates the others. Restored wl clients inside come back
    hibernated, but the file manager's wl documents start right away. Starting placeholders and applets without a
    saved form (eg the help) can't be deactivated. The node's own (outer) children are left alone.
    Stand-ins are `session::hibernate`, labeled from the saved form. `NodeTree::focus_path` wakes the node before
    passing the rest of the path in, so the woken applet gets it.
- `Focus(path)` puts focus *at* the path: a path ending on an element selects it (the applet's selector),
  a longer one goes inside it.

## Directories (#124)

- A tree's directory is `NodeHolder::directory` (a project's, copied into splits and new projects; the runner's if `None`).
- Applets ask with `NodeRequest::GetDirectory` (`AppletContext::get_directory`), answered with `StandardEvent::Directory`.
  `NodeHolder::set_directory` (a project's `d`) sends it to every applet in the tree; a split passes it on to its panes, a nested project ignores it.
- The command hub starts `run` and `$` there; a file manager spawned without a path follows it.
- Known gap: an applet placed into another tree keeps the old directory until it asks again.
