//! dmabufs (GPU buffers shared between processes or devices) as wgpu textures, through Vulkan:
//! clients' buffers to copy from (#136), and the monitors' scanout buffers to draw into (#60).
//! Only on the Vulkan backend, with `VK_EXT_image_drm_format_modifier`, and only single plane modifiers.

use ash::{khr, vk};
use smithay::backend::allocator::{Buffer as _, dmabuf::Dmabuf};
use std::os::fd::{AsRawFd, IntoRawFd};
use wgpu::hal::{api::Vulkan, vulkan};

/// What the dmabufs are, as Vulkan sees them (ie DRM's `Argb8888`/`Xrgb8888`)
pub const VK_FORMAT: vk::Format = vk::Format::B8G8R8A8_SRGB;
pub const WGPU_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const HANDLE_TYPE: vk::ExternalMemoryHandleTypeFlags =
    vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT;

/// The Vulkan usage for `usage` (only copying from and drawing into are supported),
/// and the format features it needs
fn vk_usage(usage: wgpu::TextureUsages) -> (vk::ImageUsageFlags, vk::FormatFeatureFlags) {
    let mut flags = (vk::ImageUsageFlags::empty(), vk::FormatFeatureFlags::empty());
    if usage.contains(wgpu::TextureUsages::COPY_SRC) {
        flags.0 |= vk::ImageUsageFlags::TRANSFER_SRC;
        flags.1 |= vk::FormatFeatureFlags::TRANSFER_SRC;
    }
    if usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT) {
        flags.0 |= vk::ImageUsageFlags::COLOR_ATTACHMENT;
        flags.1 |= vk::FormatFeatureFlags::COLOR_ATTACHMENT;
    }
    flags
}

/// The modifiers that import as one plane and can be used for `usage`
///
/// # Safety
/// `device` must be valid (eg wgpu's)
pub unsafe fn modifiers(device: &vulkan::Device, usage: wgpu::TextureUsages) -> Vec<u64> {
    let instance = device.shared_instance().raw_instance();
    let physical_device = device.raw_physical_device();
    let (vk_usage, features) = vk_usage(usage);

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
            .usage(vk_usage)
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
                    .contains(features)
                && importable(properties.drm_format_modifier)
        })
        .map(|properties| properties.drm_format_modifier)
        .collect()
}

/// The whole dmabuf as a texture for `usage` (`COPY_SRC` and/or `RENDER_ATTACHMENT`).
/// Whoever else uses it must be done with it when the GPU gets to it.
pub fn import(
    device: &wgpu::Device,
    dmabuf: &Dmabuf,
    usage: wgpu::TextureUsages,
) -> Option<wgpu::Texture> {
    let size = dmabuf.size();
    let size = wgpu::Extent3d {
        width: u32::try_from(size.w).ok()?,
        height: u32::try_from(size.h).ok()?,
        depth_or_array_layers: 1,
    };
    let hal_texture = {
        // SAFETY: the image and memory are made on this device and handed to wgpu-hal
        let hal_device = unsafe { device.as_hal::<Vulkan>() }?;
        let image = unsafe { create_image(&hal_device, dmabuf, size, vk_usage(usage).0) }?;
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
            usage: hal_usage(usage),
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
    // SAFETY: made on this device, for this descriptor
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
                usage,
                view_formats: &[],
            },
        )
    })
}

fn hal_usage(usage: wgpu::TextureUsages) -> wgpu::TextureUses {
    let mut uses = wgpu::TextureUses::empty();
    if usage.contains(wgpu::TextureUsages::COPY_SRC) {
        uses |= wgpu::TextureUses::COPY_SRC;
    }
    if usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT) {
        uses |= wgpu::TextureUses::COLOR_TARGET;
    }
    uses
}

unsafe fn create_image(
    device: &vulkan::Device,
    dmabuf: &Dmabuf,
    size: wgpu::Extent3d,
    usage: vk::ImageUsageFlags,
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
        .usage(usage)
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
