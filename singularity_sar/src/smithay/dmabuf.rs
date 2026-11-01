//! Clients' dmabufs (GPU buffers, eg Firefox's), imported into wgpu through Vulkan, so they
//! don't have to render on the CPU into shm (#136).
//! Only on the Vulkan backend, and only single plane modifiers.

use crate::runner::AppletRunner;
use ash::{ext, khr, vk};
use smithay::{
    backend::allocator::{Buffer as _, Format, Fourcc, dmabuf::Dmabuf},
    reexports::wayland_server::DisplayHandle,
    wayland::dmabuf::{
        DmabufFeedbackBuilder, DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier,
    },
};
use std::{
    ffi::CStr,
    os::fd::{AsRawFd, IntoRawFd},
};
use wgpu::hal::{api::Vulkan, vulkan};

/// Same as wl_shm's (see `SmithayState::import_buffer`)
const FOURCCS: [Fourcc; 2] = [Fourcc::Argb8888, Fourcc::Xrgb8888];
const VK_FORMAT: vk::Format = vk::Format::B8G8R8A8_SRGB;
pub const WGPU_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const HANDLE_TYPE: vk::ExternalMemoryHandleTypeFlags =
    vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT;
/// Sampled after it's copied out (cropped) on the GPU
const USAGE: vk::ImageUsageFlags = vk::ImageUsageFlags::TRANSFER_SRC;

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
    let instance = device.shared_instance().raw_instance();
    let physical_device = device.raw_physical_device();

    let mut list = vk::DrmFormatModifierPropertiesListEXT::default();
    unsafe {
        instance.get_physical_device_format_properties2(
            physical_device,
            VK_FORMAT,
            &mut vk::FormatProperties2::default().push_next(&mut list),
        );
    }
    let mut modifiers = vec![
        vk::DrmFormatModifierPropertiesEXT::default();
        list.drm_format_modifier_count as usize
    ];
    let mut list = vk::DrmFormatModifierPropertiesListEXT::default()
        .drm_format_modifier_properties(&mut modifiers);
    unsafe {
        instance.get_physical_device_format_properties2(
            physical_device,
            VK_FORMAT,
            &mut vk::FormatProperties2::default().push_next(&mut list),
        );
    }

    let importable = |modifier: u64| {
        let mut modifier_info = vk::PhysicalDeviceImageDrmFormatModifierInfoEXT::default()
            .drm_format_modifier(modifier)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let mut external_info =
            vk::PhysicalDeviceExternalImageFormatInfo::default().handle_type(HANDLE_TYPE);
        let info = vk::PhysicalDeviceImageFormatInfo2::default()
            .format(VK_FORMAT)
            .ty(vk::ImageType::TYPE_2D)
            .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
            .usage(USAGE)
            .push_next(&mut modifier_info)
            .push_next(&mut external_info);
        let mut external_properties = vk::ExternalImageFormatProperties::default();
        let mut properties =
            vk::ImageFormatProperties2::default().push_next(&mut external_properties);
        unsafe {
            instance.get_physical_device_image_format_properties2(
                physical_device,
                &info,
                &mut properties,
            )
        }
        .is_ok()
            && external_properties
                .external_memory_properties
                .external_memory_features
                .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
    };

    modifiers
        .iter()
        .filter(|properties| {
            properties.drm_format_modifier_plane_count == 1
                && properties
                    .drm_format_modifier_tiling_features
                    .contains(vk::FormatFeatureFlags::TRANSFER_SRC)
                && importable(properties.drm_format_modifier)
        })
        .flat_map(|properties| {
            FOURCCS.map(|code| Format {
                code,
                modifier: properties.drm_format_modifier.into(),
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
    let size = dmabuf.size();
    let size = wgpu::Extent3d {
        width: u32::try_from(size.w).ok()?,
        height: u32::try_from(size.h).ok()?,
        depth_or_array_layers: 1,
    };
    let hal_texture = {
        // SAFETY: the image and memory are made on this device and handed to wgpu-hal
        let hal_device = unsafe { device.as_hal::<Vulkan>() }?;
        let image = unsafe { create_image(&hal_device, dmabuf, size) }?;
        let Some(memory) = (unsafe { bind_memory(&hal_device, image, dmabuf) }) else {
            unsafe { hal_device.raw_device().destroy_image(image, None) };
            return None;
        };
        let desc = wgpu::hal::TextureDescriptor {
            label: Some("dmabuf"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: WGPU_FORMAT,
            usage: wgpu::TextureUses::COPY_SRC,
            memory_flags: wgpu::hal::MemoryFlags::empty(),
            view_formats: Vec::new(),
        };
        unsafe {
            hal_device.texture_from_raw(
                image,
                &desc,
                None,
                vulkan::TextureMemory::Dedicated(memory),
            )
        }
    };
    // SAFETY: made on this device, for this descriptor, and the client initialized it
    Some(unsafe {
        device.create_texture_from_hal::<Vulkan>(
            hal_texture,
            &wgpu::TextureDescriptor {
                label: Some("dmabuf"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: WGPU_FORMAT,
                usage: wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
        )
    })
}

unsafe fn create_image(
    device: &vulkan::Device,
    dmabuf: &Dmabuf,
    size: wgpu::Extent3d,
) -> Option<vk::Image> {
    let plane_layouts = [vk::SubresourceLayout {
        offset: u64::from(dmabuf.offsets().next()?),
        row_pitch: u64::from(dmabuf.strides().next()?),
        ..Default::default()
    }];
    let mut modifier_info = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
        .drm_format_modifier(dmabuf.format().modifier.into())
        .plane_layouts(&plane_layouts);
    let mut external_info = vk::ExternalMemoryImageCreateInfo::default().handle_types(HANDLE_TYPE);
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(VK_FORMAT)
        .extent(vk::Extent3D {
            width: size.width,
            height: size.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(USAGE)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut external_info)
        .push_next(&mut modifier_info);
    unsafe { device.raw_device().create_image(&info, None) }.ok()
}

/// Imports the dmabuf's fd as `image`'s memory
unsafe fn bind_memory(
    device: &vulkan::Device,
    image: vk::Image,
    dmabuf: &Dmabuf,
) -> Option<vk::DeviceMemory> {
    let raw = device.raw_device();
    let fd = dmabuf.handles().next()?.try_clone_to_owned().ok()?;

    let external_memory_fd =
        khr::external_memory_fd::Device::new(device.shared_instance().raw_instance(), raw);
    let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
    unsafe {
        external_memory_fd.get_memory_fd_properties(HANDLE_TYPE, fd.as_raw_fd(), &mut fd_properties)
    }
    .ok()?;
    let requirements = unsafe { raw.get_image_memory_requirements(image) };
    let type_bits = requirements.memory_type_bits & fd_properties.memory_type_bits;
    if type_bits == 0 {
        return None;
    }

    let mut import_info = vk::ImportMemoryFdInfoKHR::default()
        .handle_type(HANDLE_TYPE)
        .fd(fd.as_raw_fd());
    let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let info = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(type_bits.trailing_zeros())
        .push_next(&mut import_info)
        .push_next(&mut dedicated_info);
    let memory = unsafe { raw.allocate_memory(&info, None) }.ok()?;
    // the memory owns the fd now
    let _ = fd.into_raw_fd();

    if unsafe { raw.bind_image_memory(image, memory, 0) }.is_err() {
        unsafe { raw.free_memory(memory, None) };
        return None;
    }
    Some(memory)
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
