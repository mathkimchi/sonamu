# Status bar (#150), design

Replaces DMS completely (bar, notifications, tray, OSD). Out of scope: DMS's colors/wallpaper (the example `settings`
script still reads DMS's files), screenshot key (`dms screenshot`), lock screen. Mockups:
`.superpowers/brainstorm/*/content/bar-layout-v6.html` (not committed).

## Look

Top row of every screen, own row (separate from the buffer line). Nerd Font icons (Symbols Nerd Font, via glyphon's
fallback). Left to right:

- Left: tree arrow `▾`/`▴` (toggles the treeview), app launcher `󰀻`, time `2026-12-25 12:00:00PM` (no icon).
- Right: tray icons, bell `󰂚` with the count as a small superscript in its top right corner (fixed room: the bell
  never moves), resources (battery icon + % colored by level: >50 green, 20–50 yellow, <20 red [my call]; cpu %,
  mem %), hub (wifi, bluetooth, volume, brightness icons).

Popups (one at a time, under their segment, close on outside click / Esc / clicking the segment again):

- Time: calendar placeholder (current month grid).
- Launcher: search field (typing filters), `.desktop` apps with icons (`Icon=` via icon theme), Enter/click launches.
- Notifications: list with app icons, `󰅖` dismiss each, clicking one runs its default action and dismisses it,
  "clear all", Do not disturb toggle.
- Resources: % + charging state, time left, rate in W (negative while discharging), power profile picker
  (saver/balanced/performance), process list (name, cpu, mem), sorted by the clicked column (like `top`).
- Hub: wifi (current network › list), bluetooth (on/off › devices), volume slider + output device, brightness
  slider, placeholder Settings row.

Transient:

- New notification pops up under the bar's right end for `notifications.timeout` s (setting, default 5) [my call: the
  sender's own timeout wins; critical stays until dismissed]; not while Do not disturb.
- Volume/brightness changes show a small OSD popup (like DMS).
- Power saver is set once at Sonamu start.

## Treeview

Left only. Shown/hidden by the bar's arrow (replaces the pin; saved as `treeview_pinned` in the session). Comment out
the pin button, floating treeview, and `side: right` (settings stay parseable).

## Architecture (my calls)

- Bar and popups are drawn by `RootNodeApplet`, like the buffer line: `content_size` also subtracts the bar row and
  the content starts under it; mouse in the bar row/popups goes to the bar first.
- Components are Rust structs (`Segment`-like) in `sonamu_sttk/src/nodular_applet/status_bar/`, not nodes.
- System state comes from `sonamu_sttk::shell` services on their own threads, waking the root (calloop ping) on change:
  - notifications: `org.freedesktop.Notifications` server (`zbus`), replaces DMS's.
  - tray: StatusNotifierWatcher + host (`zbus`), icons from pixmaps or theme names, left click `Activate`, right click
    its DBusMenu as a popup.
  - battery: `/sys/class/power_supply` (rate from `power_now`/`current_now`·`voltage_now`), profiles via
    `powerprofilesctl`.
  - cpu/mem/processes: `/proc` (`sysinfo` crate).
  - audio `wpctl`, brightness `brightnessctl`, wifi `nmcli`, bluetooth `bluetoothctl` (CLI, polled while needed + on
    change; OSD triggered by detected changes).
- Icons: freedesktop icon theme lookup; SVG via `resvg`, drawn as `UIElement::Image`.
- Luau: `bar.layout({left = {...}, right = {...}})` with component names (default as above), set from `init.luau`.

## Status

Follow-ups after the 8 steps (fixes first, then improvements) are logged at the end of this list.

- Done: 1 (`nodular_applet/status_bar.rs`: `layout` places segments for drawing and clicks; `content_size` subtracts
  `status_bar::height()`; the root draws content at `[0, height]` and shifts mouse y).
- Done: 2. `StatusBar` (in the root) holds the open popup; `handle_mouse` gets screen coords first (popup, then bar;
  a click outside a popup only closes it). Clock: root timer every 200 ms, redraws when the text changes. Calendar:
  month grid, today `accent`, `‹`/`›` change month.
- Widths: `char_width()` = 0.6 em (Noto Sans Mono's advance; text isn't measured).
- Popups are opaque: with 0xF0 alpha the panes' text showed through (linear blending), so the treeview's menu
  (`menu_bg`) likely shows a little too.
- Done: 3. `status_bar/system.rs`: a thread reads every 2 s (battery from sysfs, profile via power-profiles-daemon's D-Bus `ActiveProfile` (was `powerprofilesctl`, a Python script started every 2 s),
  cpu/mem from `/proc`; processes from `/proc/*/stat`+`statm` only while the popup is open) into
  `Arc<Mutex<Snapshot>>`, then pings the root (calloop ping → damage). Sets `power-saver` once at start.
  `resources.rs`: segment + popup; `Lines` (mod.rs) places monospace text by column for drawing and clicks.
- Widths: Nerd Font icons count as 2 columns (`columns`), they render ~1.5 wide.
- Don't `match` on `snapshot.lock()` directly when an arm locks again (the guard lives through the match: deadlock).
- Process names are `comm` (15 chars, eg `.quickshell-wra`).
- Done: 4. `notifications.rs`: zbus 6 (no blocking API: `zbus::block_on(Builder::session().name(..).serve_at(..)
  .build())`, then zbus's helper thread serves it); the `Server` interface fills `Arc<Mutex<Store>>`; signals
  (`NotificationClosed`, `ActionInvoked`) go out from the serving thread via an mpsc channel. Name taken (eg DMS
  running) → logged, nothing served. `notification_list.rs`: bell (count as a `UIElement::Scaled(0.55)`
  superscript; 󰂛 while Do not disturb), list popup (dismiss ✕, click = `default` action + dismiss, clear all, Do
  not disturb), popping up ones (≤ 3, under the bar's right end, `popups()`: until `popup_until`; critical ones
  until dismissed; none while Do not disturb). `StatusBar::tick` redraws when the clock text or the popping up set
  changes. Icons not drawn yet (`icon`, `desktop_entry` kept).
- Tested in a private bus: `dbus-run-session -- <script>` running Sonamu, `busctl --user call -- ... Notify
  "susssasa{sv}i" app 0 "" summary body 0 0 -1` (`--`, else `-1` reads as an option).
- Done: 5. `desktop.rs` (`.desktop` apps from the data dirs: `Exec` split, field codes dropped; `score`: prefix >
  word start > inside > letters in order), `launcher.rs` (popup; Enter/click start via `program_command`, in
  `terminal() -e` for `Terminal=true`; windows placed like any new one), `icons.rs` (thread; index of the GTK
  theme (`gtk-icon-theme-name` in gtk-4.0/3.0 `settings.ini`), its `Inherits`, hicolor, pixmaps; nearest 48 px, SVG
  via resvg; `Icons::get` asks and returns `None` until loaded). Notifications show `app_icon`/image path, else the
  `desktop-entry` hint's or the app name's icon. Keys: while the launcher is open the root sets
  `sonamu_common::BUFFER_MODE` (`sync_key_grab`) so every key comes to it (`handle_grabbed_key`).
- Done: 6. `controls.rs` (threads: brightness from sysfs every 150 ms, volume re-read on `pactl subscribe` sink /
  server events; either change sets `State::osd`; wifi (`nmcli`), bluetooth (`bluetoothctl`), sinks (`wpctl
  status`, `Audio` › `Sinks:`) read at start and every 5 s while the hub is open; actions run on a thread, then
  everything is read again). `hub.rs`: segment icons, popup (sections expand with `›`; on/off at a header's end;
  slider click sets the level, the icon mutes; outputs list; Settings placeholder), OSD (middle, under the bar,
  1.5 s; not while the hub is open). Wifi connects only to networks `nmcli` can join without asking (no
  password prompt yet).
- Headless tests: the test `XDG_RUNTIME_DIR` hides PipeWire; give Sonamu `PIPEWIRE_RUNTIME_DIR=/run/user/1000
  PULSE_RUNTIME_PATH=/run/user/1000/pulse` (OSD checked that way).
- Done: 7. `tray.rs`: serves `org.kde.StatusNotifierWatcher` (`/StatusNotifierWatcher`; items register a bus
  name, or an object path on the sender as Ayatana's do), emits `StatusNotifierHostRegistered` at start; a thread
  reads every item's properties every 2 s (no proxy property cache: items signal `NewIcon`, not
  `PropertiesChanged`), drops ones that can't be read; `Passive` ones aren't drawn. Icons: `IconPixmap` (ARGB →
  RGBA, nearest 48 px), else `IconName` via `Icons`. Left: `Activate` (menu if `ItemIsMenu`), middle:
  `SecondaryActivate`, right: its DBusMenu (`AboutToShow`, `GetLayout` parsed from `Variant`s; submenus indented;
  `_` mnemonics dropped; click → `Event(id, "clicked")`). Tested with a scratch zbus item (TESTING.md).
- Done: 8. `sonamu.bar_layout` (COMMANDS.md); `StatusBar::layout`, `Segment::NAMES`; empty segments (a tray
  without items) take no room. `init.luau` re-runs on `sonamu.reset()`, which is the live reload.
- Fix: hub commands have timeouts (`command_within`: 5 s, 30 s to connect; killed after), since `bluetoothctl`
  waits forever without bluetoothd and would stall the lists' thread.
- Fix: Esc closes any popup (design asked it): `BAR_POPUP` (root_node_applet.rs) makes `is_keybind` take a bare
  Esc while one is open; `handle_grabbed_key` closes it. Untested live: injected keys didn't reach Sonamu.
- Improvement: scrolling over the hub's icons changes volume 5% a step, over the calendar its month
  (`StatusBar::scroll`; a wheel notch or `SCROLL_STEP` touchpad px per step); any scroll over the bar or a popup
  stays there.
- Improvement: the hovered segment, and the one whose popup is open, get a lighter background (`hovered`,
  `segment_at`; active screen only).
- Improvement: a popping up notification shows its actions other than `default` as `[label]` buttons (while they
  fit one line); a click sends `ActionInvoked(id, key)`, then dismisses it.
- Improvement: `status_bar/widgets.rs`: `Button` (columns on a popup line; draws and hit-tests, so they agree;
  lit while on, lighter under the pointer: `StatusBar::hovered_cell`) and `ScrollList` (wheel scroll, `show` keeps a
  selection in view, `item_at`, a thin bar in the right pad while not all fit). Buttons: launcher apps, power
  profiles, sort columns, Do not disturb, Clear all, hub headers' name and on/off, networks/devices/outputs, mute,
  `out ›` (now at the volume line's end). Scrolling: launcher (all matches, 12 shown), processes (all, 15 shown;
  `Popup::Resources(sort, list)`). Also buttons: calendar `‹`/`›`, tray menu entries (enabled ones), notifications
  (itself over title + body (`Button::tall`), ✕, actions `[label]`; list and popping up, these hovered by
  `hovered_notification`). A popping up one's click off its buttons (gaps on the actions line) does nothing now (was
  activate). Untested live (no `wlrctl`); hub, calendar and popup notification clicks unit-tested.
- Clicks tested headless (TESTING.md's pointer client): calendar month, treeview arrow, resources sort and profile.

## Order of work

1. Bar row + layout shift + treeview arrow (treeview left-only).
2. Time + calendar placeholder.
3. Resources segment + popup (battery, profiles, processes).
4. Notifications daemon + popups + DnD.
5. Launcher (desktop entries, icons).
6. Hub (audio, brightness, wifi, bluetooth) + OSD.
7. Tray.
8. Luau `bar.layout`.

## To do

- Calendar is a placeholder (asked for a good one: events? week numbers? undecided).
- Hub: Settings row is a placeholder; wifi can't ask for a password (only networks `nmcli` joins alone).
- DMS leftovers: example `settings` still reads DMS's colors/radius/font scale (`colors_path`, `dms_settings_path`;
  out of scope, user's "setting setter"); wallpaper came from DMS too.
- Super+? help (`man_page.rs:72`) still says "Click top right circle: pin/unpin tree view" (the pin is gone: the bar's
  arrow).
- Treeview's own menu (`treeview::menu_bg`, alpha 0xF0) likely lets text show through, as the popups did.
- `pactl subscribe` (controls.rs) isn't killed when Sonamu exits.
- Tray: no tooltips (`ToolTip`/`Title` unread), no `StatusNotifierItemUnregistered`, `IconThemePath` ignored.
- Notifications: no inline images (`image-data` hint), no history beyond the list's 8, unread count = all.
- `cargo machete` wasn't run (not installed).

## Notes

- Untested live: Esc closing popups, launcher typing (injected keys don't reach Sonamu headless; launcher keys
  unit-tested), DRM multi-monitor.
- Bus names are requested queued, without allowing replacement (`serve_queued`): if another server (eg DMS) has
  one, Sonamu gets it once that leaves. Before (fixed), a DMS still running at start kept them and Sonamu never got
  them, even after DMS quit: no tray or notifications.
- Test runs share the machine: the bar sets the real power profile; hub actions change real volume/wifi.
- Beeper can't start hidden; the example config closes its first window instead (SETTINGS.md, `startup`).
