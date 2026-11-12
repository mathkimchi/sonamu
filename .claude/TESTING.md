# Driving Sonamu without a monitor (for agents)

Found while debugging #60; nothing here is in the repo's code.

- Host: headless sway (`WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 sway -c <conf>`, GPU renderer; pixman can't
  take Sonamu's Vulkan surface). `XDG_RUNTIME_DIR` must be short (eg `/run/user/1000/sonamu-test`), else the socket
  path is too long. Its socket is `wayland-1` there. Set the size with `swaymsg output HEADLESS-1 resolution WxH`.
- Sonamu in it: `SONAMU_SESSION=<test session json> SONAMU_SCREENS=2 SONAMU_BACKEND=winit target/debug/sonamu_sde`.
  Never point it at the real session, and kill only the test processes by PID (the user's Sonamu may be running).
- Screenshots: `grim` against the sway socket (Sonamu's own capture is only the main screen).
- Input: `wlrctl pointer move/click` and `wtype` (eg `wtype -M alt -k r -m alt`). Each `wlrctl` call makes and drops
  its own virtual pointer, so the seat has no pointer between calls and winit misses the events: keep one open with a
  tiny wayland client that creates a `zwlr_virtual_pointer_v1` and blocks (builds offline from the cached
  `wayland-client`/`wayland-protocols-wlr`), and a keyboard with `wtype -s 3000000 a &`.
  Move absolutely with `wlrctl pointer move -5000 -5000` then the target.
- X11/Xvfb doesn't work: winit is built for Wayland only here.
- sway may not be installed: `nix-shell -p sway --run 'which sway'` gives a store path to run directly.
- Rebuild the binary (`cargo build --bin sonamu_sde`), not just `-p sonamu_sar`, or the old one runs.
- Clipboard: `wayland-info | grep data_control`, then `wl-copy x; wl-paste` against Sonamu's socket; "Handling new
  toplevel surface" in the log means wl-copy fell back to a window (#113).
- It reads the real config (`~/.config/sonamu`), so `startup` (eg `dms run`) starts in the test too; point
  `SONAMU_CONFIG` at an empty dir to avoid that.
- `wtype -M alt -M shift -k b` arrived as Escape (raw keycode 1) in one run (#141): check what the root actually got
  before trusting injected keys. Gestures can't be injected at all (DRM only); a temporary timer in the root calling
  `handle_swipe` with scripted `SwipePhase`s worked.
- Session state stands in for input when only static looks matter: a file manager's `inner` children open documents,
  a `Wl` main restores as a placeholder. In one run (2026-11-05) `wtype` keys never reached Sonamu and `wlrctl`
  wasn't installed; same-binary screenshots were byte-identical, so `cmp` on two `grim` PNGs checks a refactor.
- Focus moves without input (#145): a temporary env-gated hook in `RootNodeApplet::remember_focus` (runs each loop)
  calling `self.traverse(Traversal::Select / Enter)` every 2 s and `eprintln!`ing `get_focus_path()` checked 4 finger
  round trips through nested projects. Start focus deep via the session's `focus` (`[{"Inner":1},{"Child":0}]`).
- 2026-11-10: in a fresh headless sway, `wtype` Escape never reached Sonamu (plain or with Super) and `6` was dropped
  from typed text, while letters did arrive; drive Luau features over `sonamu-msg eval` instead (`SONAMU_SOCKET` is
  `$XDG_RUNTIME_DIR/sonamu-<its wl socket>.sock`; stale ones from old runs stay in the dir).
- Clients launched from a Sonamu run in the dev shell (`IN_NIX_SHELL`) don't get its `LD_LIBRARY_PATH`
  (`connect_wl_client`, #153): it beat Signal's RUNPATH (BoringSSL), so test apps from a shell without it too.
  Run test apps through the settings' `startup` (with `SONAMU_CONFIG`) to launch them as Sonamu does.
- 2026-11-12 (#150): clicks work with a ~60-line virtual pointer client (`wayland-client` 0.31.14 +
  `wayland-protocols-wlr` 0.3.12 `client`, `cargo build --offline`): binds `zwlr_virtual_pointer_manager_v1` + seat,
  reads `move x y` (`motion_absolute` in the output's WxH), `click`, `scroll dy`, `sleep ms` lines from stdin; pipe a
  subshell into it with `grim` calls between (the pointer stays while stdin is open). Coords include sway's title bar.
- Test runs share the machine: the status bar sets the real power profile (`power-saver` at start, and on clicks).
- 2026-11-12: `wtype f` (with a held `wtype -s` keyboard too) reached the root as `Escape`; key handling is
  unit-tested instead (eg `status_bar::launcher` tests).
- Tray (#150): a ~40-line scratch zbus 6 program serving `org.kde.StatusNotifierItem` + `com.canonical.dbusmenu`
  (`GetLayout` children as `zbus::Structure::from((id, props, children))` variants) and calling the watcher's
  `RegisterStatusNotifierItem` stands in for an app (`cargo build --offline` once zbus is cached); run it inside the
  same `dbus-run-session` as Sonamu. Right click: `click 273` (BTN_RIGHT) in the pointer client.
