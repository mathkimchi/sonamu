# Project UI (issue #68)

## Decisions

- The overview shows while the project's own tree is focused at its inner root (`focused_node() == root()`), i.e.
  on the project itself rather than on something in it. That is also the state right after Super+P, so a freshly
  focused project node (not inside it) shows it too. The project can't tell whether its outer node is `Focusing`
  (`FocusChanged` is never sent), so it doesn't key off that.
- It replaces the panes there (they were the last child's pane before). Rows, from the top: name, directory, key hint,
  `1  <label>` per child of the inner root, then `NEW_ROWS` (command hub `n`, project `p`, terminal `t`, browser `b`). Rows are a fixed height so the project hit-tests clicks itself.
- `p` (#81) adds an empty project (`ProjectApplet::empty`, no contents, this project's directory) to the inner
  root and focuses inside it, so its own overview shows, ready to rename and fill.
- Icon: a row after the directory (so the hint is row 3, contents from `CONTENTS_ROW` 4). `i` or a click on it opens a
  picker listing `ICONS` (Nerd Font) where the contents were: Up/Down + Enter or a click picks one, any typed
  character (eg an emoji) is used as is, Backspace goes back to the default (`PROJECT_ICON`), Esc cancels. Also shown
  before the name. Saved as `SavedTree::icon` on the inner root (like `label`), and the treeview's `icon` reads it
  from the published `project` spawner's `inner`.
- Previews (#82): below the rows, a card per child of the inner root (`CARD_WIDTH` wide, the project's own shape, wrapping
  into columns), each showing what that child shows (`NodeTree::preview`: its displayed node's surface, a split's first
  pane) scaled down to fit with `UIElement::Scaled`, over its number and label. They update live, since a child's
  `Damage` damages the project's window. A click on one selects that child, like its row. Hidden while setting the
  directory (its options take those rows). The children keep their last pane size, or the tree's size if never shown.
- `UIElement::Scaled(f)` (sonamu_ui): its children are laid out at full size, then drawn `f` times as big from its
  top-left (rects incl. border and radius, text via glyphon's `TextArea::scale`, char grids, textures and wl
  subsurfaces). Text is clipped only to a rectangle (glyphon bounds), so the preview is fitted inside its card instead of cut to it.
- Keys there, taken before the project's selector: `r` (no modifiers) starts renaming; `n` adds a command hub to the
  inner root and focuses inside it (unlike Ctrl+Shift++, which doesn't focus it); `t`/`b` start `TERMINAL`/`BROWSER`
  (`standard_keybinds`, shared with Super+T/N) with `NodeRequest::Run`, and since focus is at the inner root, the
  window lands there. Digits/`d`/`e`/`q` stay the selector's.
  While renaming: text, Backspace, Enter (saves; empty keeps the old name), Esc (cancels); Ctrl/Alt/Super keys still
  go to the selector. Mouse events are all taken (no panes are shown).
- Directory (#93): `d` or a click on its row starts editing, prefilled with the current directory (the runner's
  cwd if none) as shown, with home as `~` and no trailing `/`, so `~/x` + typing `/y` gives `~/x/y`. Only absolute
  or `~` paths; relative ones get no options and are rejected. While editing, the rows below the hint list the
  subdirectories of the typed parent matching the last part (hidden ones once `.` is typed, max 20); a click fills
  one in; Up/Down selects one (wrapping, shown with `›` in the accent color) and Tab fills it in, else Tab
  completes to the longest common prefix (or adds `/` once it is the only match). Typing clears the selection. Enter saves only an
  existing directory (canonicalized) or clears it if empty; otherwise the hint turns red ("Not a directory") and
  editing stays open.
- A project made inside a project (`NodeRequest::Wrap` to its holder) starts with that holder's directory:
  `WrapInitializer` takes the directory along with the subtree.
- Renaming starts from an empty name (starting from the old one made typing append to `Project`).
- The name is the inner root's treeview label, saved as the new `SavedTree::label` on the project's inner root
  (`#[serde(default, skip_serializing_if)]`, so old sessions load and other nodes don't change in the JSON).
  Chose this over changing `SavedApplet::Project(Box<SavedTree>)`, whose JSON form would break old sessions.

- The overview's background is the command hub's `BG`. `BG` and `BAR_BG` use the treeview's `with_opacity` (0.8 alpha),
  so the help applet, which shares them, is translucent too.

## Questions for the user

- Color: not done, tracked in #76. Treeview labels are `WorldTree<String>`, so a color needs a protocol
  change (eg a label struct with an optional color, through `Treeview`, `NodeTree::applet_treeview` and the treeview
  renderer).
- Should the overview also show while focus is on a child of the project (eg beside the panes), or only at its root?

## Issues found

- `StandardEvent::FocusChanged`/`Highlighted` are never sent by anything.
- `1-9: select` in the hint: `9` is the last child and digits only reach 8 children; beyond that only clicks/`w`/`s`.
- No hover/pointer cursor on the clickable rows.

## Verified (headless, `singularity_sde/examples/headless.rs hub ...`)

Super+P shows the overview; click on the name / `r`, type, Enter renames (treeview and overview); Esc cancels;
click `+ command hub` / `n` adds a focused hub; a wl window arriving at the overview lands in the project; Alt+q `a` back to the overview lists both children; clicking one shows its pane;
the name survives a save and restore (`"label"` in the session JSON). Not checked live.
