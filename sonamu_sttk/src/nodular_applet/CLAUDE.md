# World tree

## Structure

- `WorldTree<T>` (`sonamu_common/src/utils/tree/world_tree.rs`): `Base(T)` or `World(RecursiveTreeNode<WorldTree<T>>)`. A node's value is a raw value or a whole **inner world**: down is either its children (same world) or its inner world (one layer deeper). The treeview shows inner children, then own children, alike.
- `WorldTreePath` = `Vec<Step>`: `Child(i)` (same world), `Inner(i)` (child of the current node's inner root). An inner root is its node's row: no path of its own, every row has exactly one path. `split_world` / `join` / `push_inner` convert.
- Paths between client and holder are relative to the client's own treeview. Up: holder prefixes (`NodeHolder::full_path`). Down: `split_world`, rest to the main as `Pluck`/`Place`/`Focus`. `None` = "at your focus".

## Ownership

- **Node** (`nodular_applet/node_tree.rs`): optional main (applet behind `ClientHandle`) + children. Its world-tree value is the main's published treeview.
- **`NodeTree`**: slotmap of nodes, one world, held by a **`NodeHolder`** (`node_holder.rs`): the root's (`RootNodeApplet`) and each project's. Shared logic: pluck/place, node requests, wl surfaces, wrapping, directory. Requests it can't do itself are returned for its own node.
- Applets own their inner world directly (eg file manager documents), embedding applets (`AppletContext::embed`) on their own loop.
- Pluck/place moves ownership: `ClientHandle::detach` → `DetachedTree { main, wl_surface, children, uid, awaiting, hibernated }` → `DetachedClient::attach` (queued requests replay). The structural node plucks itself if its applet has nothing pluckable; place adds as last child if the applet can't. Structural plucks run after the event batch (`to_pluck`): calloop panics detaching the dispatcher currently dispatching. A fast drag can drop before its pluck ends: `RootNodeApplet::last_drop` re-resolves the drop by node.
- Generic `StandardApplet` hooks: `pluck`/`place` (`Answer::Later` to answer afterwards), `close_requested` (default close; wl asks client; project dissolves), `take_wl_surface` (default gives it back → node's child), `update` (after each batch), `is_traversing`, `node_action`.

## Projects (`nodular_applet/project_applet.rs`)

- A plain `StandardApplet` holding a `NodeHolder`; nothing above knows it's a project. Its tree's root is a container (no main, the project's name) and *is* the project's row.
- Make: `NodeRequest::Wrap(ProjectApplet::wrapper())` (Super+P, menu, hub). Dissolve: close it → `NodeRequest::Unwrap(contents)` replaces its node.
- `holds_children`: new things for its node (keybinds, New child, drops, opened applets, wl windows) go inside, at its root. Flag lives on the initializer (`NodularAppletInitializer::holding_children`) to avoid a publish race. `NodeTree::child_target` puts a normal child right after it.
- Wl surfaces registered while focus is inside go inside; it reports `WlKeyboardFocus` up (`Node::inner_wl_focus`).
- Overview (focus at its inner root, `focused_node() == root()`): name (`r`), directory (`d`, #93, completion), icon (`i`), color (`c`, #76: a square of it; `c` or a click shows plain + hue slider until a click pick, Enter, another click or Esc (gives the old color back); Left/Right step the hue, Space toggles plain; `Treeview::colors`, `NodeTree::title_color`), new rows (`p` empty project, `t` terminal, `b` browser), zoom-out view below. Saved as `SavedTree::{label, icon, color}` on the inner root.
- `frames_itself` (published): a project showing panes isn't framed by its holder; its tree frames its panes.
- Gaps: dissolving inside the file manager drops contents; panes in an unfocused project still draw focused borders; dissolving with a pending pluck/place inside loses the answer; each nesting level adds an ack round trip.

## Focus

- Each node: `FocusIndex::Here` (focus ends at it: inside its main, else the node's selector) or `Child(i)`. Path built bottom-up; `Here` appends the main's published focus. No separate "selected" state (#145).
- Moving: the root picks the target row, sends `Focus(path)`; each holder focuses its node and passes the rest (`Focus([])` if it ends there). `NodeTree::focus_path` wakes hibernating nodes on the way.
- Traversing (`is_traversing`): focus at a node without a main, or in an applet's own selector (published `traversing`).

## Traversal (`nodular_applet/traversal.rs`)

Root-only, by **shown rows** (`view_tree_rows`: view root's subtree minus collapsed). Plain keys while traversing, else with Alt (`is_keybind`):
Tab/Shift+Tab next/prev; `w`/`s` sibling; `a`/`0` parent (widens past the view root); `d` remembered child (`RootNodeApplet::remembered`), else first; `1`–`8`, `9` nth/last; Alt+Enter = 4 down. Selector: Space collapses, Enter focuses zoom center.

Swipes (DRM only, `UIEvent::Swipe`, `handle_swipe`, `swipe.rs`):
- 4 up: leave innermost world (path up to last `Inner`, else `[]`). 4 down: `NodeTree::zoom_target` (recurses via `Treeview::zoom_focus`).
- 3 fingers: axis locks after 10 px; horizontal = siblings, vertical = ancestors/remembered children; natural direction. Settles on one of the two shown rows (`settle_target`), spring `Settle` (settings `swipe.*`). Ctrl+Super+arrow = one step (`Settle::step`).
- Drawing: `StandardEvent::Swipe(SwipeView)` to the tree; `NodeTree::set_swipe` draws if both ends are its nodes, else passes in; different worlds: no animation. Same panes at both ends: focus ring stretches (`swipe::ring`).

## Strips (#144, `NodeTree::push_panes`)

Children side by side, full or half width (`Super+F`, `ToggleHalf`, saved `SavedTree::half`), `gap` apart, scrolled minimally to show focus (`Node::scroll`, active screen only). Only the innermost strip toward the displayed node is on screen. Key moves jump; swipes slide.

## Zoom-out view (#145, `NodeTree::zoom_window`)

- Drawn while focus ends at a node without a main. One row per depth down the center's chain (`zoom_rows`, `Strips::whole`), all in one `Scaled` (fits 4 × 3 views × pinch scale `ZOOM_PINCH`, ¼–4×). Center = `last_focus` if below focus, else remembered child.
- Arrows/scroll move the center, not focus; click centers then focuses; drag to a pane edge moves the node (`zoom_drop`, `move_node`); right press opens the treeview menu. Shift+C resets view.
- `zoom_out.view: free`: scrolls move the view (`zoom_free`, bounded, momentum `zoom_glide`); pinch/Ctrl+scroll scale around the pointer. Arrows slide the center back to the middle (`center_zoom_free`), keeping the scale.
- Animations: `ZoomAnim` (in/out spring), `zoom_pan` (its lateral part, to a sibling, slides only the center's row and below, #166), `move_zoom_panes`; `zoom_animating` drives holders' redraws (paced by `DamageAck`).
- Gotcha: a UI node with an explicit size clips its children, so the `Scaled` box covers `size` and panes sit in an equally big container.

## Treeview

- Each holder stitches nodes' published trees in as values (labels with Nerd Font icons from `saved`, `icon` in `node_tree.rs`). Left column, toggled by the status bar's arrow (`treeview_pinned`); while hidden, floats while Alt is held after Alt+traversal (`alt_traversal`; Alt release detected by evdev keycode, modifiers are from before the key).
- View root (any row): `NodeTree` view root for root-tree nodes, else `RootNodeApplet::view_path`. Double click / menu / Super+V / Alt+R; `‹ parent` row widens it.
- Menu (#103, `treeview::Menu`): New child/sibling ›, Make project, Set as view root, Deactivate, Close, Close with children; actions are `NodeAction`s routed to the owning holder. New program windows land via `wl_target`.
- Deactivate (`NodeTree::deactivate`): stand-in `hibernated` main, woken on focus; wl tiles close their toplevel; native get `StandardEvent::Deactivate`; projects deactivate all inside.
- Drop on a row's top/bottom edge inserts a sibling (`drop_target`); `InsertInner` → `InsertChild` only by the holder owning the path's last node.

## Screens (#60)

- DRM: monitors placed by setting `displays` (#158, `.claude/SETTINGS.md`) in one scene, main first; window: `SONAMU_SCREENS=N` side by side, named `window-<i>`. Root gets `StandardEvent::Screens`; each screen has its own focus, view root, treeview state; inactive ones live in `screens`, swapped by `activate_screen`. Last clicked screen is active (keys). The pointer acts on the screen under it without activating it (#158): hover/scroll go to its treeview and panes (`inactive_screen_mouse`, `NodeTree::screen_mouse`; `StandardEvent::ScreenMouse` for `screen_windows` mains); a swipe makes it active until the swipe settles (`lent_from`), the wl keyboard staying put (no `update_wl_focus` meanwhile). Gap: focus rings show the lent screen as focused during the swipe.
- Each screen is a wl output (`sonamu_sar/CLAUDE.md`): layer surfaces, capture.
- Applets get `ScreenViews { sizes, active }`; may publish `screen_windows` (one per screen). Projects mirror screens in their tree (`set_screen_count`, `at_root_on`, `Treeview::screen_focus`).
- Each monitor's focus is saved (`.claude/SESSIONS.md`).
- Gaps: scale 1 (#164); preferred modes only (#165).

## Directories (#124)

`NodeHolder::directory` (project's, else runner cwd). `NodeRequest::GetDirectory` → `StandardEvent::Directory`; `set_directory` broadcasts. Gap: a placed applet keeps its old directory until it asks.
