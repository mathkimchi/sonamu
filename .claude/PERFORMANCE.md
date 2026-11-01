# Performance (#98)

## Text shaping on the display thread (found 2026-10-18)

- The DRM display thread (`run_drm_display`) used ~91% of a core, drawing ~60 fps at ~15 ms CPU per frame (debug build).
- perf (userspace, `perf_event_paranoid=2` allows it without root): most of that time is cosmic-text's
  `FontFallbackIter` (fontdb id binary searches, codepoint support counts).
- Cause: `draw_text_with_glyphon` (`sonamu_ui/src/winit_backend/rendering.rs`) makes a new `glyphon::Buffer`
  and re-shapes every `UIPrimitiveElement::Text` on every frame. With `Family::Monospace`, cosmic-text scans
  every system font (578 here) per word, even ASCII words.
- Measured with the 41 real node labels from the session file:

  | build   | `Family::Monospace` | `Family::Name("Hack")` |
  |---------|---------------------|------------------------|
  | debug   | ~150 ms/frame       |                        |
  | release | ~15 ms/frame        | ~2.3 ms/frame          |

- Cost grows with (Text elements on screen, mostly treeview labels) × (redraws/s). Any damage redraws everything,
  and animated titles (eg Claude Code's spinner) damage constantly. So it gets worse as windows are opened.
- `FontSystem` itself doesn't slow down over time (flat over 80 frames of changing text).
- A late frame delays libinput too ("your system is too slow" in the log), which likely explains doubled keys (#56).

Fixes:
- `TextCache` (`rendering.rs`, in `WgpuData`): shaped buffers by (text, font size), kept while drawn each frame.
- `TextCache::monospace`: `Family::Monospace` is resolved to a font name once (cosmic-text sets it to
  Noto Sans Mono) and asked for by name. Same font as before; 41 labels shape in 2.8 ms instead of 15.6 (release).
- `scripts/runner.sh` runs the release build.

Not done: skipping off-screen rows.

Other notes: `atlas.trim()` is never called (glyphs never evicted, atlas only grows); wgpu-core's debug lock
tracing captures backtraces each frame (~4% in debug only).

## Videos (#136)

- Hidden tiles (not in the last redraw's `tile_areas`) get no frame callbacks and are set `Suspended`, so
  clients in inactive nodes stop rendering.
- Each surface keeps its last texture (`TextureCache` in its data map), keyed by buffer, smithay commit counter
  and crop, so a redraw re-uploads only surfaces that committed. Still a full upload per commit (no damage).
- linux-dmabuf (`smithay/dmabuf.rs`, importing through `sonamu_ui::dmabuf`, shared with the DRM backend's scanout buffers): on Vulkan, the device is opened with `VK_EXT_image_drm_format_modifier`
  (`UIHandle::request_device`), and ARGB/XRGB8888 with single plane modifiers are advertised (with feedback when
  `VK_EXT_physical_device_drm` gives the render node). A dmabuf is imported as a `VkImage` on its fd and copied
  (cropped) on the GPU into its own texture. Commits with a dmabuf wait for its fence (`new_surface` blocker).
  No queue family ownership transfer: fine for uncompressed single plane modifiers on Mesa.
