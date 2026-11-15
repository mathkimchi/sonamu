# Headless testing

- Host: `WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 sway -c <conf>` (GPU renderer). Short `XDG_RUNTIME_DIR` (eg `/run/user/1000/sonamu-test`); socket `wayland-1`; size via `swaymsg output HEADLESS-1 resolution WxH`. No sway: `nix-shell -p sway --run 'which sway'`. Xvfb doesn't work.
- Sonamu: `dbus-run-session -- env SONAMU_SESSION=<test json> SONAMU_CONFIG=<empty dir> SONAMU_SCREENS=2 SONAMU_BACKEND=winit target/debug/sonamu_sde`. Never use the real session or config; kill test processes by PID only. Rebuild `--bin sonamu_sde`.
- Screenshots: `grim` on the sway socket. `cmp` two PNGs to check a refactor.
- Pointer: `wlrctl` drops its pointer between calls. Use a tiny client (`wayland-client` 0.31 + `wayland-protocols-wlr` 0.3, `--offline`) binding `zwlr_virtual_pointer_v1`, reading `move x y`/`click [btn]`/`scroll dy`/`sleep ms` from stdin. Move twice before clicking; coords include sway's title bar.
- Keys: `wtype` is unreliable (keys arrive as Escape or not at all). Prefer `sonamu-msg eval` and unit tests; check what the root got.
- Gestures can't be injected: use a temporary env-gated timer calling `handle_swipe` / `traverse`.
- Session JSON can stand in for input (inner children, `Wl` placeholders, deep `focus`).
- Clipboard: `wayland-info | grep data_control`, `wl-copy`/`wl-paste` against Sonamu's socket.
- Sonamu strips the dev shell's `LD_LIBRARY_PATH` from clients (`connect_wl_client`, #153), so test apps the way Sonamu launches them: via `startup`.
- Tray: a small zbus program serving `StatusNotifierItem` + `dbusmenu` in the same `dbus-run-session`.
- Real session log: `~/.local/state/sonamu.log`. Fuzz: `root_node_applet::tests::swipes_keep_every_tab`.
