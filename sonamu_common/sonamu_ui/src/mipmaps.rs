//! Smaller copies of textures, so they stay smooth when drawn small (eg windows in the zoom-out view)

use std::{
    collections::HashMap,
    sync::{
        Mutex, OnceLock, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

/// Set from the settings (`mipmaps`)
static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Usage a texture needs for `generate`
pub const USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT;

/// Levels for a texture of `size`: down to 1 px, or 1 when disabled
#[must_use]
pub fn level_count(size: wgpu::Extent3d) -> u32 {
    if ENABLED.load(Ordering::Relaxed) {
        size.max_mips(wgpu::TextureDimension::D2)
    } else {
        1
    }
}

/// A pipeline drawing one level into the next, by format (there's one device)
static PIPELINES: OnceLock<Mutex<HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>>> =
    OnceLock::new();

const SHADER: &str = "
struct Out { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };

// a triangle covering the target
@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> Out {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return Out(vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0), uv);
}

@group(0) @binding(0) var level: texture_2d<f32>;
@group(0) @binding(1) var linear: sampler;

// at half size, a linear sample averages 2 x 2 texels
@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    return textureSample(level, linear, in.uv);
}
";

fn pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let mut pipelines = PIPELINES
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    pipelines
        .entry(format)
        .or_insert_with(|| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("mipmap_shader"),
                source: wgpu::ShaderSource::Wgsl(SHADER.into()),
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mipmap_pipeline"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    targets: &[Some(format.into())],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        })
        .clone()
}

/// Fills `texture`'s levels after the first from it (made with `level_count` levels and `USAGE`)
pub fn generate(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) {
    if texture.mip_level_count() < 2 {
        return;
    }
    let pipeline = pipeline(device, texture.format());
    let layout = pipeline.get_bind_group_layout(0);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let level = |mip| {
        texture.create_view(&wgpu::TextureViewDescriptor {
            base_mip_level: mip,
            mip_level_count: Some(1),
            ..Default::default()
        })
    };
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    for mip in 1..texture.mip_level_count() {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&level(mip - 1)),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let target = level(mip);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mipmap_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit([encoder.finish()]);
}
