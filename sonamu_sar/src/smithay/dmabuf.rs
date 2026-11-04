//! Clients' dmabufs (GPU buffers, eg Firefox's), imported into wgpu through Vulkan (`sonamu_ui::dmabuf`),
//! so they don't have to render on the CPU into shm (#136).

use crate::runner::AppletRunner;
use ash::{ext, khr, vk};
use smithay::{
    backend::allocator::{Buffer as _, Format, Fourcc, dmabuf::Dmabuf},
    reexports::wayland_server::DisplayHandle,
    wayland::dmabuf::{
        DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier,
    },
};
use std::ffi::CStr;
use wgpu::hal::{api::Vulkan, vulkan};

/// Same as wl_shm's (see `SmithayState::import_buffer`)
const FOURCCS: [Fourcc; 2] = [Fourcc::Argb8888, Fourcc::Xrgb8888];
pub use sonamu_ui::dmabuf::WGPU_FORMAT;

/// The device extensions to add to wgpu's (which enables the external memory ones when supported),
/// or `None` if the adapter can't import dmabufs
pub fn device_extensions(adapter: &vulkan::Adapter) -> Option<Vec<&'static CStr>> {
    let capabilities = adapter.physical_device_capabilities();
    [
        khr::external_memory_fd::NAME,
        ext::external_memory_dma_buf::NAME,
        khr::image_format_list::NAME,
        ext::image_drm_format_modifier::NAME,
    ]
    .iter()
    .all(|name| capabilities.supports_extension(name))
    .then(|| vec![ext::image_drm_format_modifier::NAME])
}

#[derive(Debug)]
pub struct DmabufImport {
    state: DmabufState,
    global: Option<DmabufGlobal>,
    formats: Vec<Format>,
}

impl Default for DmabufImport {
    fn default() -> Self {
        Self {
            state: DmabufState::new(),
            global: None,
            formats: Vec::new(),
        }
    }
}

impl DmabufImport {
    /// Advertises dmabufs if `device` can import them
    pub fn init(&mut self, display_handle: &DisplayHandle, device: &wgpu::Device) {
        // SAFETY: only queries
        let Some(hal_device) = (unsafe { device.as_hal::<Vulkan>() }) else {
            return;
        };
        if !hal_device
            .enabled_device_extensions()
            .contains(&ext::image_drm_format_modifier::NAME)
        {
            return;
        }
        // SAFETY: the instance and physical device are wgpu's, and outlive this
        let (formats, main_device) = unsafe { (formats(&hal_device), main_device(&hal_device)) };
        if formats.is_empty() {
            return;
        }

        // with the render node, clients know which GPU to allocate on
        let feedback = main_device.and_then(|main_device| {
            DmabufFeedbackBuilder::new(main_device, formats.clone())
                .build()
                .ok()
        });
        self.global = Some(match feedback {
            Some(feedback) => self
                .state
                .create_global_with_default_feedback::<AppletRunner>(display_handle, &feedback),
            None => self
                .state
                .create_global::<AppletRunner>(display_handle, formats.clone()),
        });
        self.formats = formats;
    }
}

/// The formats and modifiers that import as one plane, can be copied from, and are importable
unsafe fn formats(device: &vulkan::Device) -> Vec<Format> {
    unsafe { sonamu_ui::dmabuf::modifiers(device, wgpu::TextureUsages::COPY_SRC) }
        .into_iter()
        .flat_map(|modifier| {
            FOURCCS.map(|code| Format {
                code,
                modifier: modifier.into(),
            })
        })
        .collect()
}

/// The GPU's render node, if the driver tells
unsafe fn main_device(device: &vulkan::Device) -> Option<libc::dev_t> {
    let instance = device.shared_instance().raw_instance();
    let physical_device = device.raw_physical_device();
    let extensions =
        unsafe { instance.enumerate_device_extension_properties(physical_device) }.ok()?;
    extensions
        .iter()
        .any(|extension| extension.extension_name_as_c_str() == Ok(ext::physical_device_drm::NAME))
        .then_some(())?;

    let mut drm = vk::PhysicalDeviceDrmPropertiesEXT::default();
    unsafe {
        instance.get_physical_device_properties2(
            physical_device,
            &mut vk::PhysicalDeviceProperties2::default().push_next(&mut drm),
        );
    }
    (drm.has_render == vk::TRUE).then_some(())?;
    Some(libc::makedev(
        u32::try_from(drm.render_major).ok()?,
        u32::try_from(drm.render_minor).ok()?,
    ))
}

/// The whole dmabuf as a texture that can only be copied from.
/// Its contents are read when the copy runs, so the client must be done rendering it
/// (commits wait on that, see `CompositorHandler::new_surface`).
pub fn import(device: &wgpu::Device, dmabuf: &Dmabuf) -> Option<wgpu::Texture> {
    sonamu_ui::dmabuf::import(device, dmabuf, wgpu::TextureUsages::COPY_SRC)
}

impl DmabufHandler for AppletRunner {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.smithay_state.dmabuf.state
    }

    /// Imported when drawn, so only checked here
    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        if dmabuf.num_planes() == 1 && self.smithay_state.dmabuf.formats.contains(&dmabuf.format())
        {
            let _ = notifier.successful::<AppletRunner>();
        } else {
            notifier.failed();
        }
    }
}
