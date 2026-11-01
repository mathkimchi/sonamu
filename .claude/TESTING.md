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
