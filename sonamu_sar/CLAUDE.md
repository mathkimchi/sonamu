# Runner (sar)

Runner-only unless noted; the tree doesn't know about these.

## Wayland

- Scale: only wl clients are scaled; Sonamu's UI is physical px. Logical space = physical / scale. Display info comes from the UI thread as `DisplayNotification::Info`.
- Keyboard focus follows Sonamu focus: the root sends `StandardRequest::WlKeyboardFocus(Option<WlSurfaceId>)` after each batch (`StandardApplet::focused_wl_surface`). Clipboard, IME, pointer locks follow. Only the focused toplevel is `Activated`.
- Toplevels register on map (first buffer, `register_toplevel`), so app id and fixed size are known.
- Window geometry is the tile (#55, like niri): drawn at `-window_geometry.loc`, `Tiled*` states, clipped to geometry.
- Event times are CLOCK_MONOTONIC ms (`now_input_time`; libinput's own for its events): Firefox's pan gestures rely on it. Touchpad scroll stop → `wl_pointer.axis_stop`.
- IME text and relative motion are `DisplayNotification`s, not `UIEvent`s (`UIEvent` must stay `Copy`).
- Fullscreen (#121, `SmithayState::fullscreen`): covers its screen (overlay layers above), gets the mouse, until it stops or focus leaves its tile (`focus_wl_keyboard`).
- Floating (#148, `smithay/floating.rs`): toplevels with a parent or min == max size. Centered on the pointer's screen, over layers, focused like a layer surface (`layer_focus`). Super+left drag moves (`floating_drag_mouse`, `super_held`).
- Pinch (#152, DRM): to the wl pointer focus (`AppletRunner::pinch`, `zwp_pointer_gestures_v1`); with none, to the root as `UIEvent::Pinch` (`NodeTree::zoom_pinch`). Swipes/holds aren't forwarded.
- Magnifier: Ctrl+Super+scroll sets `magnification`; `redraw_ui` wraps the scene in `UIElement::Scaled` around the pointer (input not remapped). DRM cursor scales (`cursor_scale`). Huge text overflowing glyphon's atlas is skipped.
- No Xwayland: sets `XDG_SESSION_TYPE=wayland`, `ELECTRON_OZONE_PLATFORM_HINT=auto`, `NIXOS_OZONE_WL=1` at start (exported to D-Bus on tty). `WAYLAND_DISPLAY` can't be set that way (`AppletRunner::run`): each launch sets it from `SONAMU_WAYLAND_DISPLAY`.
- Gotcha: a client whose buffer fills (eg resize flood) is dropped with only `ConnectionClosed` in the log.

## Wallpaper (#104, `wallpaper.rs`)

- Setting `wallpaper` (`battery_wallpaper` while `battery::on_battery`; `sync_wallpaper` on settings changes and when it flips), drawn first in `redraw_ui` (under background layers). The runner subscribes to settings; a timer at a shader's `fps` asks for its next frame (`next_frame`; other redraws reuse the last) and redraws (not under fullscreen), and checks its file for changes (1 s otherwise).
- Shaders: Shadertoy-style GLSL (wgpu `glsl` feature) between `PREAMBLE` (uniforms as `iTime`, `iResolution`, `iFrame`, `iMouse` = 0) and `EPILOGUE` (`main` flips to Shadertoy's bottom-left origin). `.wgsl` files (`Language`): `WGSL_PREAMBLE` (the same buffer as uniform `wallpaper`, `frame` a `u32`: GLSL bitcasts) and `WGSL_EPILOGUE` (calls `main_image`); a file declaring `wallpaper` itself (for editors) gets no preamble, but must match it (`declared_layout`). Checked with naga first (errors are a notification, at the user's lines; the last wallpaper stays). Rendered into an `Rgba8Unorm` texture at `scale`, sampled through an sRGB view (Shadertoy colors are display values).
- A file is loaded once per change (`Watched`), so a broken one isn't retried every frame.

## Socket (`$SONAMU_SOCKET`, `sonamu_common::msg`, `msg_socket.rs`)

- `$XDG_RUNTIME_DIR/sonamu-<wl socket>.sock`. `sonamu-msg open <applet> [args]` → `StandardEvent::Open` to the root, placed like a new wl surface: into the focused applet (`take_open`) or as the focused node's child (`NodeHolder::open`). Relative path args are made absolute. Exit 1 if unreachable (xdg-open falls back).
- `eval`, `subscribe`: `.claude/COMMANDS.md`. `assets/sonamu-file-manager.desktop`: default folder handler.

## Clipboard

Native applets use `sonamu_common::clipboard` (not `arboard`: wrong compositor). `paste`: runner pipe, the selection owner writes (`SmithayState::write_clipboard`, 1 s timeout). `copy`: runner owns the selection (`set_clipboard`). `Target::Primary` likewise. Luau `clipboard.copy/paste`. Prompts copy last output with Ctrl+Shift+C.

## Notifications

`sonamu_common::notification::Replacing` via `busctl`, served by the status bar. Used by settings reloads and battery (`sonamu_sar::battery`: 20%, critical 10%, polled 5 s, also `on_battery`). Keep tests from sending them.
