# UI kit (`sonamu_sttk::ui`)

For applets building `UITree`s by hand (absolute `Style`s). `UITree::with_pointer()` for clickables.

## Consistency

Every native applet looks and behaves alike: same shades, metrics, input handling. Reuse what's here; logic two applets share belongs here, not copied or reached from another applet's module.

## Parts

- Metrics: `row_height()` (font + 14), `compact_row_height()` (font + 10), `PAD`.
- Backgrounds (from `&Palette`, translucent `with_opacity`): `bg` (applet), `panel_bg` (panel, list), `raised_bg` (bars, hover, unfocused selection), `selected_bg`, `guide` (lines, frames), `popup_bg` (menus, popups; opaque). Text is the palette's.
- Input: `BTN_LEFT`/`BTN_RIGHT`/`BTN_MIDDLE`, `dragged(from, at)`; `ScrollSteps` turns mouse scrolls into steps (use for every mouse scroll).
- Text: `text`, `spans`; place with `Rows::text`, `Rows::style` or `inset`. `rect`, `frame` (`Bordered`).
- `Rows { top, height, scroll }`: same indices for drawing (`row`, `style`, `top_of`, `text`) and clicking (`row_at`: may be past the last row, filter by count; `within`). Scrolling: `fitting`, `scroll_to`, `max_scroll`. Header: `Rows::with_height(0.0, h).row(0, ..)`.
- Char-grid UIs (hub, man page) use `command_hub::{fill_row, put}`; status bar popups use `status_bar/widgets.rs`.
