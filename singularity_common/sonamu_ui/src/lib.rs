pub mod color;
pub mod display_units;
pub mod dmabuf;
pub mod task_logger;
pub mod ui_element;

#[cfg(feature = "wayland_backend")]
mod wayland_backend;
#[cfg(feature = "wayland_backend")]
pub use wayland_backend::UIDisplay;
#[cfg(feature = "wayland_backend")]
pub use wayland_backend::ui_event;

#[cfg(feature = "winit_backend")]
pub mod winit_backend;
#[cfg(feature = "winit_backend")]
pub use winit_backend::ui_event;
#[cfg(feature = "winit_backend")]
pub use winit_backend::{
    CapturedFrame, DisplayInfo, DisplayNotification, DisplayProxy, DisplayRequest, Gpu, ImeEvent,
    PointerGrab, UIDisplay, drm::run_drm_display,
};

// #[cfg(test)]
