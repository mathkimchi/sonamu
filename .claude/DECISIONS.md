# Decisions

Choices made while finishing the non-major Wayland features (and small bug fixes), for review.

## Fractional scale / refresh rate

- Scale and refresh rate are display properties only the runner (smithay) needs, so they are sent from the UI thread as `DisplayNotification::Info`, not as a `UIEvent`. Applets are untouched. The present channel became this `DisplayNotification` enum (`Presented` / `Info`) instead of adding another `run_display` parameter.
- Display info is (re)sent on start, `ScaleFactorChanged` and `Moved` (monitor may change), only when it differs.
- Only wl clients are scaled; Sonamu's own UI keeps drawing in physical px.
- The compositor's logical space is physical px / scale. Wl layers are drawn at logical * scale, pointer positions are divided by scale, and tile sizes sent in configures are tile px / scale (rounded).
- Preferred scale sent to every surface on every commit (smithay dedupes): fractional scale = the winit scale factor, integer `preferred_buffer_scale` = ceil(scale), as anvil does.
- When the scale changes, every toplevel is re-configured with its size converted to the new scale.
- Refresh rate comes from winit's current monitor. If unknown: `wl_output` mode says 60 Hz (it needs a number), presentation feedback says `Refresh::Unknown`; otherwise `Refresh::Fixed`.
- Viewporter crop now converts `src` to buffer px using the surface's buffer scale (needed once clients use buffer scale > 1). Buffer transforms are still ignored.
- Presentation arguments were bundled into a `Presentation` struct (was 5 loose args).

## xdg-activation

- Activation is granted if the token is under 10 s old, like anvil. No other checks (e.g. requesting client's focus).
- New `StandardEvent::FocusWlSurface(WlSurfaceId)`, sent to the root applet and passed down. Each `RecursiveNodeApplet` focuses the main applet (`FocusIndex::Inner`) or the child that contains the surface, then forwards the event to it.
- New `StandardApplet::contains_wl_surface` (default `false`), implemented by `WlSurfaceApplet`, `RecursiveNodeApplet`, `CachingApplet` and `Box<dyn StandardApplet>`. Chosen over searching treeviews since treeviews only have titles.
- Applets that can't show wl surfaces (`command_hub`, `one_char_render_test`) ignore the event instead of `todo!()`.
- Smithay keyboard focus is moved to the surface right away, so the clipboard follows too.
- Not done: clearing smithay keyboard focus when Sonamu focus leaves (see TODO.md).

## Text input (IME)

- `UIEvent` is `Copy` and applets rely on that, so IME text isn't a `UIEvent`. It's `DisplayNotification::Ime(ImeEvent)` to the runner, which hands it to smithay's text input. (`DisplayNotification` lost `Copy` for this; nothing needed it.)
- So only wl clients get IME text; native applets don't (none handle IME yet).
- It goes to the smithay keyboard focus (the last wl surface typed into), which smithay also uses for text-input focus. Same staleness as the clipboard, until keyboard focus clearing is done.
- IME is always allowed on the Sonamu window (`set_ime_allowed(true)`), instead of only while a client has text input enabled, since smithay gives no enable callback.
- Preedit cursor uses winit's byte range; `None` becomes `-1, -1` (hidden). Commit clears the preedit first.
- Not done: candidate window position (`set_ime_cursor_area` from the client's cursor rectangle).

## Pointer constraints / relative pointer

- The redraw proxy's `()` became `DisplayRequest { Redraw, SetPointerGrab(PointerGrab) }`, so the runner can ask the UI thread to grab the pointer. `PointerGrab` is Sonamu's own enum (`None`/`Confined`/`Locked`) so applets/runner don't depend on winit's type.
- The runner checks once per event loop iteration (`AppletRunner::update_pointer_grab`) whether the pointer-focused wl surface has an active constraint, and only sends a request when that changes. This also releases the grab when a constraint is destroyed, without needing a callback.
- Locked uses winit `CursorGrabMode::Locked`, falling back to `Confined` where locking is unsupported (X11). Confined confines to the whole Sonamu window, not the client's region.
- Constraints activate when the pointer moves onto the surface or the constraint is created there, if the pointer is inside the constraint's region (like anvil).
- Raw mouse motion is winit's `DeviceEvent::MouseMotion`, sent as `DisplayNotification::RelativeMotion` (not a `UIEvent`, same reasoning as IME). It goes to the last surface the pointer was sent to (`SmithayState::pointer_focus`), scaled to logical units. Accelerated and unaccelerated deltas are the same since winit only gives raw.
- If Sonamu focus moves away while a client has the pointer locked, the lock stays until the pointer focus changes or the host compositor takes focus away. Depends on the keyboard/pointer focus clearing TODO.
- `pointer_focus` is checked for liveness before use, since the client may destroy the surface.
- Not done: warping the host cursor to the client's `cursor_position_hint` when a lock ends.

## Drag and drop

- `dnd_requested` starts smithay's `DnDGrab` like anvil. Touch drags are cancelled since there's no touch input.
- The drag icon is drawn at the pointer in every wl tile's tree, since the pointer has no focus during a drag (only the visible tile matters). Its attach offset is tracked on commit, like anvil.
- The icon is cleared on drop and on cancel, and gets frame callbacks while dragging.
- Drops only reach surfaces in the visible wl tile, since other tiles get no pointer events.
- Cursor and drag icon surfaces are checked for liveness before use.

## Keyboard focus follows Sonamu focus

- Instead of `StandardEvent::FocusChanged` (never sent, and `command_hub` `todo!()`s on it), the root applet asks its tree which wl surface is at the end of the focus path after each batch of events, and sends `StandardRequest::WlKeyboardFocus(Option<WlSurfaceId>)` when it changes.
- New `StandardApplet::focused_wl_surface` (default `None`), implemented by `WlSurfaceApplet`, `RecursiveNodeApplet`, `CachingApplet`, `Box<dyn StandardApplet>`. A `RecursiveNodeApplet` in `Focusing` returns `None`, since it takes the keys itself then.
- The runner focuses that toplevel (unless it or one of its popups already has focus), or clears keyboard focus for `None`. Clipboard, primary selection and text input follow, and clearing also ends popup grabs.
- On `None` the pointer also leaves the wl surface (`pointer_focus` cleared), which ends pointer locks and so the window's pointer grab. Mouse events forwarded to the tile later bring it back.
- The old "set focus on every key press" path is kept as a fallback, now sharing `focus_wl_keyboard_on`.
- This supersedes the staleness notes in the clipboard/IME/pointer constraint sections above.

## Popups outside the tile

- The scene now records where each subsurface (wl tile) was laid out (`PrimitiveScene::subsurface_areas`), and the runner passes that to smithay after each redraw (`SmithayState::set_tile_areas`).
- Popups are now kept inside the output (the whole Sonamu window), like anvil, using that tile position. Falls back to the tile before the first layout. They may draw over other Sonamu UI.
- Over the part of a popup outside its tile, Sonamu's cursor shows instead of the client's (the client cursor region is the tile).
- "Clicking outside the tile doesn't dismiss popups" needed no change: all mouse events reach the displayed wl applet, a click with no wl surface under it has no pointer focus, and smithay's `PopupPointerGrab` dismisses on that. Moving Sonamu focus away also clears keyboard focus, which ends the grab.

## Window geometry is the tile (#55)

- Like niri, a toplevel is drawn at `-window_geometry.loc`, so its xdg window geometry (not the surface, which may include CSD shadows) starts at the tile's top-left. Layer locations, pointer focus origins and the popup unconstrain target are all relative to the window geometry.
- Like niri for tiled windows, toplevels get `TiledLeft/Right/Top/Bottom`, so GTK etc. drop shadows and rounded corners.
- The root's surface tree is clipped to the window geometry (niri's opt-in `clip-to-geometry`, always on here, since tiles have no gaps). Popups aren't clipped. Meant to fix a 1px line left of Firefox (unverified guess: GTK3 Adwaita draws tiled windows with a 1px border box-shadow just outside the geometry). Hit testing isn't clipped.

## Touchpad scroll stop (#58)

- Like niri: on udev, a libinput finger scroll with an explicit `0` on an axis is the fingers lifting; it's sent as `MouseEventKind::Scroll { stop }` and becomes `wl_pointer.axis_stop`. Firefox treats touchpad scrolls as pan gestures that need it.
- That alone didn't fix Firefox. A WAYLAND_DEBUG log showed axis events bunched 2-3 per identical timestamp, every ~15-20 ms: the drm loop draws synchronously between event batches, and the compositor stamped events when it handled them. Firefox derives pan velocity from those times.
- Like niri: `MouseEvent::time_msec` carries libinput's event time; the compositor's own times (keys, synthesized motion, relative motion, winit events) are now CLOCK_MONOTONIC (`SmithayState::now_msec`) so they share libinput's clock. Previously keys used UNIX time and the pointer used time since start.
- Likely the real cause of "touchpad doesn't move at all": Firefox's `nsWindow::GetEventTimeStamp` assumes Wayland event times are CLOCK_MONOTONIC ms, and touchpad scrolls become APZ pan gestures stamped with it (wheel scrolls take a path that doesn't). Our old time-since-start made every pan look about as old as the machine's uptime.
- Also like niri: a `wl_pointer.motion` is only sent for real motion (moved, or focus changed), not before every button/scroll.
- Key events used the constant serial 42; they now take `SERIAL_COUNTER.next_serial()`.
- Still open: the drm loop batching input between synchronous draws adds up to a frame of latency.
- Not done: the winit backend always sends `stop: [false; 2]` (winit's `TouchPhase::Ended` could map to it). The axis source is still guessed from `v120` instead of libinput's `source()`.

## Pointer lock end

- New `DisplayRequest::WarpPointer` (winit `set_cursor_position`). `cursor_position_hint` is converted to window px when received (using the tile position and the surface's origin), and the pointer is warped there when a `Locked` grab ends.

## IME candidate window

- Not done: smithay keeps the client's cursor rectangle private (only forwarded to `input_method_v2` popups). Left in TODO.md.

## Bug fixes

Fixed:

- `resumed` sent `WindowResized(800, 600)`, but that is the logical size; it now sends the window's actual physical inner size.
- Ctrl+Shift+Q ("Quit") panicked: the root applet's `StandardRequest::Quit => todo!()`. The root now forwards `Quit` to the runner.
- Quitting or closing the window never ended the process: the runner only set `is_running` (the display only checked it on window events), and the runner's calloop loop never stopped. Now `UIHandle::quit` also wakes the display, the display checks `is_running` in `user_event`, and when the display's channel closes the runner stops its loop (`AppletRunner::loop_signal`), so `AppletRunner::run` returns.
- Runner `StandardRequest::DamageTreeview => todo!()` panicked; now it marks a redraw (the treeview is part of the surface).
- `import_buffer` `todo!()`ed on dmabuf/single-pixel/unknown buffers; now draws nothing (those protocols aren't advertised anyway).
- A zero-sized window resize (eg minimizing on X11/Windows) would make wgpu panic in `surface.configure`; zero sizes are now ignored.
- `new_toplevel` gave every new toplevel keyboard focus, even though Sonamu focus wasn't on it. Removed, since focus now follows `StandardRequest::WlKeyboardFocus`.

- (Regression from sending frame callbacks after present) wl clients only got their frame callbacks on the next input event, because clients were only flushed after client dispatch and after input. The runner now flushes clients once per event loop iteration, which also covers configures, IME, relative motion and focus changes sent from other sources.

Found, not fixed (not small, or the intended behavior is unclear):

- Closing a wl client leaves its tile in the tree, drawn red forever (`toplevel_destroyed` only forgets the surface). Needs a way to remove a child applet. (Addressed by the ClientHandle refactor: `WlSurfaceDestroyed` closes the tile.)
- Root mouse remapping ignores the tiles' 1px border (existing TODO in `root_node_applet.rs`), so wl pointer positions are off by ~1px.
- Mouse events over the treeview are still forwarded to the displayed applet (root forwards every mouse event to its inner applet).
- `command_hub` and `one_char_render_test` `todo!()` on several `StandardEvent`s (`FocusChanged`, `SurfaceDamageAck`, ...). Currently unreachable, but they'd panic if those events start being forwarded.
- `ClientHandle` panics if the root applet's request channel closes (eg the applet thread panicked), turning one panic into two. (Fixed by the ClientHandle refactor: a closed channel is ignored.)

- dms (quickshell) was killed when closing one of two open popouts: smithay's layer-shell pre-commit hook still ran after the layer surface was destroyed, saw the reset (0-size, unanchored) state and posted `InvalidSize`. Fixed upstream (smithay `ed563d9`), which needed smithay master and so winit 0.31: the display's `DisplayRequest`s now go through an mpsc channel, and the proxy only wakes the loop (`proxy_wake_up`, replacing `user_event`).
- SDE spawns `dms run` itself (`AppletRunner::spawn_shell`). It currently sets `WAYLAND_DEBUG=1` and writes stderr to `dms.log` for debugging. Client disconnect reasons are printed (`ClientState::disconnected`).

## Opening applets from other processes (default file manager)

- A launcher (`sonamu-msg`) asks the runner over a Unix socket; the applet still runs as a thread in the root's tree, so it's saved, focused and themed like any other. Running native applets out of process was rejected: their surface/treeview are in-memory `SharedState`.
- Socket: `$XDG_RUNTIME_DIR/sonamu-<wl socket>.sock`, path in `SONAMU_SOCKET` (also exported to D-Bus on tty). One message per connection, NUL-terminated fields; no reply. A read blocks the runner's loop for at most 100 ms.
- The message names an applet by its spawner's registry name (eg `file_manager`), not `NewApplet`, so any registered applet can be opened.
- It's placed like a new wl surface: passed into the focused applet while focus is inside it (projects and splits take it into their tree), else added as the focused node's last child, focused. So without a path arg the file manager follows the directory of the tree it lands in (eg a project's); in the root's tree that's the runner's cwd.
- `sonamu-msg` makes relative args that exist as paths absolute, since the runner's cwd differs. It exits 1 if Sonamu isn't reachable, so xdg-open can fall back.
- Not done: `org.freedesktop.FileManager1` over D-Bus (browsers' "Show in folder"), `file://` URI args, and selecting a file (`ShowItems`) rather than opening a folder.
