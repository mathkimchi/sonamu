# UI kit (`sonamu_sttk::ui`)

For applets building `UITree`s by hand (absolute `Style`s). `UITree::with_pointer()` for clickables.

## Consistency

Every native applet looks and behaves alike: same shades, metrics, input handling. Reuse what's here; logic two applets share belongs here, not copied or reached from another applet's module.

## Parts

- Metrics: `row_height()` (font + 14; every list), `compact_row_height()` (font + 10; bar popup lines), `PAD` (also lists' left inset), `INDENT` (per tree level), `POPUP_PAD` (inside menus and popups).
- Backgrounds (from `&Palette`, translucent `with_opacity`: setting `opacity`, mirrored as `settings::alpha()`): `bg` (applet), `panel_bg` (panel, list), `raised_bg` (bars, hover, unfocused selection), `selected_bg`, `guide` (lines, frames), `popup_bg` (menus, popups; opaque). Text is the palette's.
- Clickable rows and cells light up under the pointer (`raised_bg`).
- Keys: `step` (Up/Down in a list: stops at the ends), `page`.
- Input: `BTN_LEFT`/`BTN_RIGHT`/`BTN_MIDDLE` (from `sonamu_ui::ui_event`, also the runner's), `dragged(from, at)`; `ScrollSteps` turns mouse scrolls into steps, one item or line each (use for every mouse scroll).
- Every scrolled list shows a scrollbar: `scrollbar` (`scrollbar_x` for the right pad), `grid_scrollbar` for char grids; bar popups via `ScrollList::bar`.
- Text: `text`, `spans`; place with `Rows::text`, `Rows::style` or `inset`. `rect`, `frame` (`Bordered`).
- Scroll state stays clamped (`max_scroll`), also on resize and content changes; PageUp/PageDown move `page(shown)`.
- `Rows { top, height, scroll }`: same indices for drawing (`row`, `style`, `top_of`, `text`) and clicking (`row_at`: may be past the last row, filter by count; `within`). Scrolling: `fitting`, `scroll_to`, `max_scroll`. Header: `Rows::with_height(0.0, h).row(0, ..)`.
- Form fields (`fields`): `TextInput` (`handle_key` → `Edit`, `element`), `toggle`, `choice` + `cycle`, `swatch`. Text ones are `UIElement`s placed with `Rows::text`.
  - Numbers: `step_size`, `stepped`, `slider` (`slider_fraction` / `slider_value`). Colors: `color_picker::ColorPicker` (HSV square image + hue strip; `press`/`drag`/`release`, `handle_key` → `PickerKey`), `hsv_color` / `color_hsv`.
- Char grids (hub, man page, buffer line): `fill_row`, `put`. Status bar popups: `status_bar/widgets.rs`, text helpers (`columns`, `cut`, `justified`) in `status_bar/mod.rs`.
