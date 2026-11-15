# Status bar (#150)

Replaces DMS (bar, notifications, tray, OSD). Code: `sonamu_sttk/src/nodular_applet/status_bar/`.

## Architecture

- Own client (`StatusBarApplet`, `ClientHandle<BarRequest>`) held by the runner beside the root, like a layer surface. Publishes `BarInput` (height, input areas, `popup`, `keys`); the runner gives the root screens minus its row and routes to it: mouse in its areas, presses while `popup`, keys while `keys`, bare Esc while `popup`.
- Root ↔ bar via the runner: `BarState`, `ToggleTreeview`, `ActivateScreen`, `BarCommand` (`sonamu.bar_layout`, effect-checked by the root).
- Segments: tree arrow, launcher, time | tray, bell, resources, hub. `layout` drops segments on narrow screens (`LEFT_OUT`); wide popups drawn `Scaled` (`fit`).
- Text is monospace by column (`Lines`, `char_width()` = 0.6 em; Nerd Font icons = 2 columns). `widgets.rs`: `Button` (draws and hit-tests together), `ScrollList`.
- Services on threads, pinging the bar: `system.rs` (sysfs battery, power-profiles D-Bus, `/proc`; processes only while open; sets power-saver once on tty), `notifications.rs` (zbus 6 server), `tray.rs` (StatusNotifierWatcher + host, polls items every 2 s, DBusMenu), `controls.rs` (brightness sysfs, `pactl subscribe`, `nmcli`, `bluetoothctl`, `wpctl`; `command_within` timeouts), `desktop.rs`/`launcher.rs`, `icons.rs` (icon theme, resvg).
- Bus names are requested queued (`serve_queued`): with DMS running, Sonamu gets them once it leaves.

## Gotchas

- Don't `match` on `snapshot.lock()` when an arm locks again (deadlock).
- Popups and menus are opaque (`ui::popup_bg`): translucent ones showed text through (linear blending).
- Headless audio: pass `PIPEWIRE_RUNTIME_DIR=/run/user/1000 PULSE_RUNTIME_PATH=/run/user/1000/pulse`.
- Tests touch real volume/wifi/power profile: run in `dbus-run-session`.

## To do

- Calendar is a placeholder; hub Settings row placeholder; wifi can't prompt for a password.
- Super+? help still mentions the pin button (`man_page.rs`).
- `pactl subscribe` isn't killed on exit. Tray: no tooltips, no unregistered signal, `IconThemePath` ignored. Notifications: no inline images, store never trimmed.
- Example `settings` still reads DMS colors.
