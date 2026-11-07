# World Tree and the hierarchy

How Sonamu's one big tree is structured, addressed, focused, and owned.
Background: DEVLOG 2026-02-03 to 2026-02-22 (the original design), `.claude/PROJECTS.md` (projects), and #83
(one tree to the user: canonical paths, the root traverses by shown rows, no layer keys).

## The data structure

`WorldTree<T>` (`sonamu_common/src/utils/tree/world_tree.rs`) is either:

- `Base(T)`: a raw value, or
- `World(RecursiveTreeNode<WorldTree<T>>)`: a tree whose nodes each have a **value** and a list of **children**.
  A node's value is itself a `WorldTree<T>`, so it is either a raw value or a whole **inner world**.

So every node has two directions to go "down":

- its **children**, in the *same* world (same layer), and
- its **value's inner world**, one layer *deeper*.

Example: a project node's value is the project's own world of applets (deeper layer); an applet's value can in turn be
the world of its elements (eg the file manager's documents). The layers are only the backend's: the treeview shows a
node's inner children and then its own children all as its children, alike (#83).

## Paths

`WorldTreePath` (#83) is a list of `Step`s from a world's root:

- `Child(i)`: child `i` of the current node, in the same world.
- `Inner(i)`: child `i` of the root of the current node's inner world (its value).

An inner world's root is the node's own row, so it has no path of its own: every row has exactly one path.

```text
[Child(2), Child(1)]                     node [2, 1] of the outer world (and its inner root)
[Child(2), Child(1), Inner(0)]           child 0 of its inner root
[Child(2), Child(1), Inner(0), Child(1)] child 1 of that, in the same inner world
```

`split_world` gives the node in the outermost world and the rest as a path of that node's own world
(`Inner(i)` becomes `Child(i)`); `join`/`push_inner` go the other way.
"Inside" a node (keys going to its main) isn't part of the path: it is the node's `FocusIndex::Inner`
(`NodeTree::screen_focus_inside` for the treeview's marker).
The saved session's `focus` falls back to the root if it can't be read (eg saved before #83).

## Traversal keys

The treeview selector shows when focus is on a node rather than inside an applet taking input.
Traversal is the root's (#83, `nodular_applet/traversal.rs`): it moves by the **shown rows** (`view_tree_rows`, the
view root's subtree without collapsed parts), so it crosses into and out of applets' inner trees like any other level,
then focuses the target with `Focus(path)` (`NodeTree::focus_path`, passed down to the applet owning the row).
Applets have no traversal of their own.

- While traversing (`NodeTree::is_traversing`), plain keys; otherwise with Alt (`is_keybind`, so wl clients don't get them):
  Tab / Shift+Tab: next / previous row (wrapping); `w` / `s`: previous / next sibling (shown rows at the same depth
  under the same parent: a node's inner children come before its own, so `s` on the last inner one goes to the first
  outer one); `a` / `0`: parent (past the view root: its parent, widening the view); `d`, `1`-`8`, `9`: first, nth, last child.
- Swipes (DRM backend only: `UIEvent::Swipe`, `Begin` / `Update(px)` / `End`), the root's (`handle_swipe`): 4 fingers up
  selects the focused row (out of its applet), 4 down is Enter; once a swipe (50 px). 3 fingers (#141, #142,
  `nodular_applet/swipe.rs`) move along a strip of shown rows, natural direction (content follows the fingers):
  - The axis locks after 10 px. Horizontal (#144): the focused row's siblings (rows under the same parent row, so inner
    then own children); fingers left bring in the next. Vertical: ancestors up to the first row, then down through each
    row's remembered child (`RootNodeApplet::remembered`, the child row focus was last under, by path, unsaved; else the
    first); fingers up bring in the child.
  - The view is a position in rows (`swipe.distance_x` / `distance_y` px each), rubber-banded past the ends (`swipe.rubber_band`). On
    lift it settles on one of the two rows shown, the one the speed projected 200 ms ahead is nearer (`settle_target`;
    it never skips rows), focuses it, then springs there (`Settle`: a damped harmonic oscillator, `swipe.stiffness` /
    `damping_ratio`, starting at the fingers' speed; ticked by a calloop timer). A new
    swipe during that carries on from where it was.
  - Rows whose children are shown at once (`Treeview::shown_at_once`; `NodeTree::shown_at_once` gathers nodes with
    several children side by side in its strips, and its mains' lists) are swiped `reversed`: the focus ring follows
    the fingers. It is the whole strip's, so past the shown ones the strip scrolls against the fingers.
  - Drawing: the root sends its tree `StandardEvent::Swipe(SwipeView { from, to, progress })` (the two rows the view is
    between). `NodeTree::set_swipe` draws it if both are its nodes, else passes it into the node whose inner world has
    both (a node and its inner root are one, so project ↔ its inner child is the project's), else drops it (ends in
    different worlds: inner ↔ own sibling, no animation). The holder lays out each end as if focused
    (`layout_swipe`, which also sizes and so shows their applets) and slides the two windows in a clipped box
    (`swipe::slide`); ends showing the same panes stay, and a focus ring stretches between their panes
    like a liquid drop instead (`swipe::ring`: leading edges ease out, trailing ones ease in). A project draws its
    overview for its root (`get_window_with`). The file manager slides its documents (not its root). Only the active
    screen's tree draws it.
- In the selector (the deepest holder's `NodeTree::handle_event`, or an applet's own, eg the file manager's documents):
  Space (or clicking its `▾`/`▸`) collapses/expands all its children, inner and own (the holder owning the row keeps
  the flag, unsaved; `NodeTree::applet_treeview`/`get_treeview_of` leave them out); Enter confirms (into the applet,
  resuming where it was; a container's first child). An applet whose row is its own root (`StandardApplet::selected_at_root`:
  a project, its overview) is sent `Focus([])` whenever its node is selected (`NodeTree::selected`), so the row shows,
  and Enter goes to, its overview (#123). Modifiers on their own don't leave the selector; other keys go into the applet.
- The standard keybinds (Ctrl+W, Ctrl+Shift++, Super+P) act on the selected node.
  Ctrl+W asks its main to close (`CloseRequest`, see below) rather than closing it.
  Something new for a node whose applet `holds_children` (projects) goes inside it, at its root (#83):
  Ctrl+Shift++ / Super+T / Super+N (the keybind is redone inside it), New child ›, a drop onto its row, opened
  applets and wl windows (`NodeHolder`). Other applets' nodes get it as a child.

An applet with an inner world can still be selecting in it (a project's `NodeTree`, the file manager over its root
and documents) when focus is on one of its rows; it publishes `traversing: true` then.

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
  by `NodeRequest`s (AddChild, Close, Wrap, ...) to the holder.
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
- Strips (#144, `NodeTree::push_panes`): a node's children sit side by side at their widths, full or half
  (`Super+F`, `NodeRequest::ToggleHalf`, saved as `SavedTree::half`; a project's inner root passes it to the project's
  node), `gap` apart. The strip is scrolled (`Node::scroll`, px) as little as can be to show the child focus is under
  whole; only the active screen's layout keeps the scroll. Siblings in sight are panes too (each showing its
  displayed node), cut to the strip (`Pane::clip`), their applets keeping their full size. Only the last strip toward
  the displayed node is on screen: focus on `[0, 0]` shows `[0, 0]`'s siblings across the whole area, not `[0]`'s. All full width, it shows one node, as before. A swipe between siblings slides the strip from one's
  scroll to the other's (`TreeSwipe::strip`, laid out with `Strips::fixed`), so the settle spring animates it; focus
  moved by keys jumps. Between siblings both in sight (same panes, in any order) the focus ring stretches between them
  instead (`ring_window`).
- Splits (`SplitApplet`, `Layout`) were removed, to be replaced by scrolling (#144). A saved split restores as a
  container of its panes (`NodeTree::restore_subtree`).

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
- An `Inner` node appends its main's published focus path (`push_inner`), so at its main's root it is just the node.

Moving focus goes down the same way: the root picks the target row (see Traversal keys) and sends `Focus(path)`;
each holder focuses its node (`Focusing` if the path ends there, else `Inner` with the rest sent to its main).
Enter makes a `Focusing` node `Inner`, and from then on keys go to its main.

## Screens (#60)

- The root's tree is shown on every screen (`StandardEvent::Screens`), each with its own focus, view root
  (`NodeTree`) and inner view root (`view_path`) and treeview state (`RootNodeApplet`). The **active** screen's are the usual fields
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
  `resize_screen`), so each screen has its own path inside it, nested projects too. Every screen shows what it would
  if active (its overview where that screen's focus is at its root, `at_root_on`), so switching screens changes
  nothing on them; `Treeview::screen_frames_itself` says which windows frame themselves, and
  `Treeview::screen_focus` where each screen's focus is inside it (for an inactive screen's treeview highlight).
- Known gaps: applets other than projects look and focus the same on every screen;
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
  `pinned` (always shown) or not (pinned by default in a new session; unpinned: only while `NodeTree::is_traversing`, or a treeview drag or menu is open).
  Floating is never pinned. See `treeview_area`, `applet_area` and `treeview_frame` in `root_node_applet.rs`.
- The treeview shows the **view root**'s subtree, any row (#83): the `NodeTree`'s view root for a node of the root's
  tree (it also picks the panes), else `RootNodeApplet::view_path` for a row inside an applet (its `NodeTree` view root
  is then the node holding it). Set by a double click on a row, its right click › Set as view root, Super+V (the focused
  row, or back to the whole tree) and Alt+R (the focused row's parent). Above it is a dim `‹ parent` row; focusing it
  (or focus leaving the view any other way) widens the view (`update_view_root`, `update_view_path`).
  Inner rows look like any other (#83: no box or shading); the panel scrolls with the wheel and follows focus.
- `is_traversing` follows `Inner` nodes down into
  their mains' `traversing`, since a path inside a project can still be a selection (eg `[Child(p), Inner(0)]` with the project selecting its child). A project at its own root (its overview) isn't traversing (#123): selecting it is its outer node's `Focusing`.

## Paths across holders

Paths sent between a client and its holder are always **relative to the client's own treeview**.
The holder converts them:

- Going up, a holder prefixes the node's path (`NodeHolder::full_path`). Applies to `RequestPluck`, `RequestPlace`
  and focus.
- Going down, it finds its node by `split_world` and passes the rest to that node's main
  as `Pluck`, `Place` or `Focus`. `Pluck(None)`/`Place(None)` mean "at your focus" (`AppletClient` uses the applet's
  `get_focus_path`).
- `NodeAction(path, action)` (`NodeHolder::node_action`) does something to the node at the path, passed down the same
  way to the holder owning it (`StandardApplet::node_action`; a project's holder, or the file manager for its
  documents). The treeview's `×` button, shown on the hovered row (not the root, not `‹` rows), sends `NodeAction::Close`,
  which asks the node's main to close like Ctrl+W.
- A right click in the treeview opens a menu (#103, `Menu` in `treeview.rs`). On a node's row: New child ›,
  New sibling below ›, Make project (Wrap, like Super+P), Set as view root, Deactivate, Close, Close with children (asks every node
  in the subtree, parents first). On the root's row only the first, Make project and Set as view root; on empty space or a `‹` row,
  the applet list for the shown world's root (the view root, or the inner root of the applet whose inner world is
  shown). New › lists command hub, terminal, browser, file manager (in the holder's directory) and an empty
  project, all as `NodeAction::Spawn`. Native ones are added there and focused inside; for a program, the holder
  keeps `wl_target` and the next new wl surface it gets goes there. A spawn passed into an applet focuses inside
  it, so that surface is passed in too. A click elsewhere closes the menu.
  - Deactivate (`NodeTree::deactivate`) puts a stand-in in the node's main, inactive and `hibernated` (its
    `SavedApplet`), so it starts again when focused. Focus on it moves to its parent, else it would wake right away.
    A live wl tile asks its client to close its toplevel (forgetting the surface, so the node stays). A native
    applet with a saved form gets `StandardEvent::Deactivate` (`StandardApplet::deactivate`) and is dropped; waking
    restores it from its saved form (`session::restore`, inner tree and all). Projects stop their whole
    inner tree (`NodeTree::deactivate_all`: wl tiles get `CloseRequest`, native mains `Deactivate`, recursively);
    the file manager closes its wl documents and deactivates the others. Restored wl clients inside come back
    hibernated, but the file manager's wl documents start right away. Starting placeholders and applets without a
    saved form (eg the help) can't be deactivated. The node's own (outer) children are left alone.
    Stand-ins are `session::hibernate`, labeled from the saved form. `NodeTree::focus_path` wakes the node before
    passing the rest of the path in, so the woken applet gets it.
- `Focus(path)` puts focus *at* the path: a path ending on an element selects it (the applet's selector),
  a longer one goes inside it.

## Directories (#124)

- A tree's directory is `NodeHolder::directory` (a project's, copied into new projects; the runner's if `None`).
- Applets ask with `NodeRequest::GetDirectory` (`AppletContext::get_directory`), answered with `StandardEvent::Directory`.
  `NodeHolder::set_directory` (a project's `d`) sends it to every applet in the tree; a nested project ignores it.
- The command hub starts `run` and `$` there; a file manager spawned without a path follows it.
- Known gap: an applet placed into another tree keeps the old directory until it asks again.
