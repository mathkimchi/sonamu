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

- Done: 1 (`nodular_applet/status_bar.rs`: `layout` places segments for drawing and clicks; `content_size` subtracts
  `status_bar::height()`; the root draws content at `[0, height]` and shifts mouse y).
- Done: 2. `StatusBar` (in the root) holds the open popup; `handle_mouse` gets screen coords first (popup, then bar;
  a click outside a popup only closes it). Clock: root timer every 200 ms, redraws when the text changes. Calendar:
  month grid, today `accent`, `‹`/`›` change month.
- Widths: `char_width()` = 0.6 em (Noto Sans Mono's advance; text isn't measured).
- Popups are opaque: with 0xF0 alpha the panes' text showed through (linear blending), so the treeview's menu
  (`menu_bg`) likely shows a little too.
- Done: 3. `status_bar/system.rs`: a thread reads every 2 s (battery from sysfs, profile via `powerprofilesctl get`,
  cpu/mem from `/proc`; processes from `/proc/*/stat`+`statm` only while the popup is open) into
  `Arc<Mutex<Snapshot>>`, then pings the root (calloop ping → damage). Sets `power-saver` once at start.
  `resources.rs`: segment + popup; `Lines` (mod.rs) places monospace text by column for drawing and clicks.
- Widths: Nerd Font icons count as 2 columns (`columns`), they render ~1.5 wide.
- Don't `match` on `snapshot.lock()` directly when an arm locks again (the guard lives through the match: deadlock).
- Process names are `comm` (15 chars, eg `.quickshell-wra`).
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
