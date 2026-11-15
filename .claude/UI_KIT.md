# UI kit (`sonamu_sttk::ui`)

For applets building `UITree`s by hand (absolute `Style`s). `UITree::with_pointer()` for clickables.

- Metrics: `row_height()` (font + 14), `compact_row_height()` (font + 10), `PAD`.
- Text: `text`, `spans`; place with `Rows::text`, `Rows::style` or `inset`. `rect`, `frame` (`Bordered`).
- `Rows { top, height, scroll }`: same indices for drawing (`row`, `style`, `top_of`, `text`) and clicking (`row_at`: may be past the last row, filter by count; `within`). Scrolling: `fitting`, `scroll_to`, `max_scroll`; mouse scrolls to steps: `ScrollSteps` (bar, treeview), `scroll_rows` (fixed rows per event). Header: `Rows::with_height(0.0, h).row(0, ..)`.
- Char-grid UIs (hub, man page) use `command_hub::{fill_row, put}`; status bar popups use `status_bar/widgets.rs`.
