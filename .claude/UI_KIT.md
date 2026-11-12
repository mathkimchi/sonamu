# UI kit (`sonamu_sttk::ui`)

Helpers for applets that build `UITree`s by hand. Everything is placed absolutely (`Style::absolute`), as before;
the kit only removes the copies of the same numbers and builders. It is in sttk (not `sonamu_ui`) since it is for
applets; the runner doesn't use it. `UITree::with_pointer()` (in `sonamu_ui`) is the pointing-hand cursor for
anything clickable.

- Metrics: `row_height()` (font + 14: treeview, its menu, project overview), `compact_row_height()` (font + 10: file
  manager), `PAD` (16). Applets keep their own other constants (indents, `TOP_PAD`, `MENU_PAD`).
- Text: `text(str, color)` and `spans([(String, Color)])` make the `UIElement::Text`; place it with `Rows::text`
  (centered in a row, height down to the row's bottom so descenders fit), `Rows::style`, or `inset(element, size,
  pad)` (a message filling an applet).
- `rect(color, position, size)`; `frame(color, radius, border_width, style, children)` (a `Bordered` node; children
  are laid out inside the border).
- `Rows { top, height, scroll }`: a list of equal rows. Indices are the list's, for both drawing and clicking, so they
  can't disagree:
  - drawing: `row(index, width, Option<background>, parts)`, `style(index, left, width)`, `top_of(index)`,
    `text(element, left, right)` (row-local)
  - clicking: `row_at(y)` (`None` above `top`; may be past the last row, so filter by the count), `within(y)` (0..1
    in the row, eg the treeview's drop zones)
  - scrolling: `fitting(height)` (rows that fit, at least 1), with free fns `scroll_to(scroll, index, shown)`,
    `max_scroll(count, shown)` and `scroll_rows(delta, step)` (a mouse scroll's signed rows; 0 without vertical delta,
    eg a touchpad's stop: use it for every row scroll)
  - a header is just `Rows::with_height(0.0, h).row(0, ...)`; a list under it starts at `top: h`.

Users: treeview rows and menu, the root's treeview frame / pin button / `notice`, the file manager, the project
overview (its rows draw text at the row's top, via `style`), the session placeholders, pane frames and the swipe
highlight. The command hub and man page draw `CharGrid`s (`command_hub::{fill_row, put}`), not this.

Status bar popups (monospace `Lines`) have their own `Button` and `ScrollList` (`status_bar/widgets.rs`, STATUS_BAR.md).
