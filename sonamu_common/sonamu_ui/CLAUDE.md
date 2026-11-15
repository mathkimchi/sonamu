# Rendering (`sonamu_ui`)

Used by the runner (sar).

- `TextCache` (`rendering.rs`): shaped buffers by (text, size); `Family::Monospace` resolved to a name once (scanning all fonts per word was the main CPU cost).
- Elements are scissored to their clip (`draw_primitive_scene`); `Clip` keeps the pane's rounded rect apart from the cut `area`. `DisplayAreaPx` is signed. Text is only cut to rects.
- Colors are sRGB everywhere (shaders decode).
- Hidden tiles get no frame callbacks (`Suspended`). Surface textures cached per commit (`TextureCache`). linux-dmabuf imported as `VkImage` (`smithay/dmabuf.rs`, `sonamu_ui::dmabuf`).
- `atlas.trim()` is never called (glyph atlas only grows).
