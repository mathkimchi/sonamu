//! Since this is a private module, I can just make all the structs and fields pub as needed

use super::{CapturedFrame, UIDisplay};
use crate::{
    display_units::DisplayAreaPx,
    ui_element::{
        CharGrid, Clip, Image, InternalCharCell, PrimitiveScene, RoundRect, UIPrimitiveElement,
        font_size,
    },
    winit_backend::WgpuData,
};
use glyphon::{AttrsOwned, Family, FontSystem, Metrics, TextRenderer, fontdb};
use msdfgen::{Bitmap, FillRule, FontExt, MsdfGeneratorConfig};
use std::{collections::HashMap, iter, sync::mpsc};
use wgpu::{
    BindGroup, BindGroupLayout, Device, MultisampleState, PipelineCompilationOptions,
    RenderPipeline, SurfaceConfiguration, TextureView, include_wgsl, util::DeviceExt as _,
};

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Vertex {
    position: [f32; 3],
}
impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];

    /// Large triangle trick to cover the whole screen
    /// https://webgpufundamentals.org/webgpu/lessons/webgpu-large-triangle-to-cover-clip-space.html (allegedly 5%)
    /// TODO: Doing two triangles for tight rectangles will probably boost speed much more, but I might end up only needing one render call so idk...
    pub(super) const VERTICES: &[Self] = &[
        // A
        Self {
            position: [3., -1., 0.0],
        },
        // B
        Self {
            position: [-1., 3., 0.0],
        },
        // C
        Self {
            position: [-1., -1., 0.0],
        },
    ];

    pub(super) const fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// A `Clip` as instance data; the shaders' `clip_coverage` reads it
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub(super) struct ClipInstance {
    origin: [f32; 2],
    size: [f32; 2],
    radius: f32,
}
impl From<Option<Clip>> for ClipInstance {
    fn from(clip: Option<Clip>) -> Self {
        match clip {
            Some(Clip { area, radius }) => Self {
                origin: [area.0[0][0] as f32, area.0[0][1] as f32],
                size: [area.size().width as f32, area.size().height as f32],
                radius,
            },
            // covers any screen
            None => Self {
                origin: [-1e6, -1e6],
                size: [2e6, 2e6],
                radius: 0.0,
            },
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub(super) struct RoundRectInstance {
    /// This is the top left, not the center
    /// Currently includes the border
    origin: [f32; 2],
    /// Currently includes the border
    size: [f32; 2],
    corner_radius: f32,
    border_dist: f32,
    main_color: [f32; 4],
    border_color: [f32; 4],
    clip: ClipInstance,
}
impl RoundRectInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
        1 => Float32x2,
        2 => Float32x2,
        3 => Float32,
        4 => Float32,
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x2,
        8 => Float32x2,
        9 => Float32,
    ];

    pub(super) const fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct RectangleRenderer {
    render_pipeline: RenderPipeline,
}
impl RectangleRenderer {
    pub fn new(device: &Device, surface_config: &SurfaceConfiguration) -> Self {
        let rectangle_render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[],
                immediate_size: 0,
            });
        let rectangle_shader = device.create_shader_module(include_wgsl!("rectangle_shader.wgsl"));
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rectangle Render Pipeline"),
            layout: Some(&rectangle_render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rectangle_shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc(), RoundRectInstance::desc()],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &rectangle_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::OVER,
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::POLYGON_MODE_LINE
                // or Features::POLYGON_MODE_POINT
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            // If the pipeline will be used with a multiview render pass, this
            // tells wgpu to render to just specific texture layers.
            multiview_mask: None,
            // Useful for optimizing shader compilation on Android
            cache: None,
        });

        Self { render_pipeline }
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub(super) struct ImageInstance {
    /// This is the top left, not the center
    /// Currently includes the border
    origin: [f32; 2],
    /// Currently includes the border
    size: [f32; 2],
    clip: ClipInstance,
}
impl ImageInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        1 => Float32x2,
        2 => Float32x2,
        3 => Float32x2,
        4 => Float32x2,
        5 => Float32,
    ];

    pub(super) const fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }

    fn set_instance_buffer(
        drawing_shared_data: &mut DrawingSharedData,
        area: DisplayAreaPx,
        clip: Option<Clip>,
    ) {
        let instances = vec![Self {
            // this currently takes in top left
            origin: [area.0[0][0] as f32, area.0[0][1] as f32],
            size: [
                (area.0[1][0] - area.0[0][0]) as f32,
                (area.0[1][1] - area.0[0][1]) as f32,
            ],
            clip: clip.into(),
        }];

        let instance_buffer =
            drawing_shared_data
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Instance Buffer"),
                    contents: bytemuck::cast_slice(&instances),
                    usage: wgpu::BufferUsages::VERTEX,
                });

        // vertex buffer slot 1 is actually the instance buffer
        drawing_shared_data
            .render_pass
            .set_vertex_buffer(1, instance_buffer.slice(..));
    }
}

pub struct ImageRenderer {
    render_pipeline: RenderPipeline,
    image_texture_bind_group_layout: BindGroupLayout,
}
impl ImageRenderer {
    pub fn new(device: &Device, surface_config: &SurfaceConfiguration) -> Self {
        let image_texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("image_texture_bind_group_layout"),
            });
        let image_render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    // Difference
                    Some(&image_texture_bind_group_layout),
                ],
                immediate_size: 0,
            });
        let image_shader = device.create_shader_module(include_wgsl!("image_shader.wgsl"));
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Image Render Pipeline"),
            layout: Some(&image_render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc(), ImageInstance::desc()],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &image_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::OVER,
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::POLYGON_MODE_LINE
                // or Features::POLYGON_MODE_POINT
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            // If the pipeline will be used with a multiview render pass, this
            // tells wgpu to render to just specific texture layers.
            multiview_mask: None,
            // Useful for optimizing shader compilation on Android
            cache: None,
        });

        Self {
            render_pipeline,
            image_texture_bind_group_layout,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable, Debug)]
pub(super) struct CharGridInstance {
    /// This is the top left, not the center
    /// Currently includes the border
    origin: [f32; 2],
    /// Currently includes the border
    size: [f32; 2],
    /// In terms of characters
    grid_size: [u32; 2],
    clip: ClipInstance,
}
impl CharGridInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        1 => Float32x2,
        2 => Float32x2,
        3 => Uint32x2,
        4 => Float32x2,
        5 => Float32x2,
        6 => Float32,
    ];

    pub(super) const fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct CharGridRenderer {
    render_pipeline: RenderPipeline,
    /// TODO: rename
    char_grid_texture_bind_group_layout: BindGroupLayout,
    atlas_bind_group: BindGroup,
}
impl CharGridRenderer {
    pub fn new(
        device: &Device,
        surface_config: &SurfaceConfiguration,
        queue: &wgpu::Queue,
    ) -> Self {
        let char_grid_texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::ReadOnly,
                        format: wgpu::TextureFormat::Rgba32Uint,
                        // Hmm... no option for `texture_storage_2d`, hopefully this works
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                }],
                label: Some("char_grid_texture_bind_group_layout"),
            });
        let (atlas_bind_group_layout, atlas_bind_group) = Self::atlas_bind_group(device, queue);

        let char_grid_render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    Some(&atlas_bind_group_layout),
                    Some(&char_grid_texture_bind_group_layout),
                ],
                immediate_size: 0,
            });
        let char_grid_shader = device.create_shader_module(include_wgsl!("char_grid_shader.wgsl"));
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Char Grid Render Pipeline"),
            layout: Some(&char_grid_render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &char_grid_shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::desc(), CharGridInstance::desc()],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &char_grid_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::OVER,
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                // Setting this to anything other than Fill requires Features::POLYGON_MODE_LINE
                // or Features::POLYGON_MODE_POINT
                polygon_mode: wgpu::PolygonMode::Fill,
                // Requires Features::DEPTH_CLIP_CONTROL
                unclipped_depth: false,
                // Requires Features::CONSERVATIVE_RASTERIZATION
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            // If the pipeline will be used with a multiview render pass, this
            // tells wgpu to render to just specific texture layers.
            multiview_mask: None,
            // Useful for optimizing shader compilation on Android
            cache: None,
        });

        Self {
            render_pipeline,
            char_grid_texture_bind_group_layout,
            atlas_bind_group,
        }
    }

    /// 4 channels at f32
    /// I don't want to use this much,
    /// TODO: use klyff_msdf crate bc it supports u8 and runs on wgpu
    // const SDF_PIXEL_BYTES: usize = 4 * 4;
    const SDF_PIXEL_BYTES: usize = 4;
    const SDF_WIDTH: usize = 32;
    const SDF_HEIGHT: usize = 64;
    /// Num bytes for each character's atlas
    const ATLAS_SIZE: usize = Self::SDF_PIXEL_BYTES * Self::SDF_WIDTH * Self::SDF_HEIGHT;

    fn generate_atlas_raw_data() -> Vec<u8> {
        let mut data = vec![0u8; Self::ATLAS_SIZE * (127 - 33)];

        let font = ttf_parser::Face::parse(dejavu::sans_mono::regular(), 0).unwrap();

        // let bound = shape.get_bound();
        // let framing = bound
        //     .autoframe(
        //         Self::SDF_WIDTH as _,
        //         Self::SDF_HEIGHT as _,
        //         msdfgen::Range::Px(4.0),
        //         None,
        //     )
        //     .unwrap();
        // log::debug!("Char: {}", ascii_code as char);
        // log::debug!("Bound: {bound:?}");
        // log::debug!("Autoframe: {framing:?}");
        // let framing = msdfgen::Framing::new(
        //     222.0,
        //     msdfgen::Vector2::new(0.01, 0.01),
        //     msdfgen::Vector2::new(400., 100.),
        // );
        // let bound = font.global_bounding_box();
        // log::debug!(
        //     "Bot: {}, top: {}, width: {}",
        //     font.descender(), // -483
        //     font.ascender(),  // 1901
        //     font.glyph_hor_advance(font.glyph_index(' ').unwrap())
        //         .unwrap(), // 1233
        //                       // so actual aspect ratio is 1.933
        // );
        let framing = msdfgen::Bound::new(
            0.,
            f64::from(font.descender()),
            f64::from(
                font.glyph_hor_advance(font.glyph_index(' ').unwrap())
                    .unwrap(),
            ),
            f64::from(font.ascender()),
        )
        .autoframe(
            Self::SDF_WIDTH as _,
            Self::SDF_HEIGHT as _,
            // NOTE: this should be the same number as PX_RANGE constant in the shader
            msdfgen::Range::Px(4.0),
            None,
        )
        .unwrap();

        for ascii_code in 33..127u8 {
            // Mostly just taken from msdf gen library: https://crates.io/crates/msdfgen
            // NOTE: Versions are weird, might need to downgrade ttf_parser
            let glyph = font.glyph_index(ascii_code as char).unwrap();

            let mut shape = font.glyph_shape(glyph).unwrap();

            let fill_rule = FillRule::default();

            let mut bitmap = Bitmap::new(Self::SDF_WIDTH as _, Self::SDF_HEIGHT as _);

            shape.edge_coloring_simple(3.0, 0);

            let config = MsdfGeneratorConfig::default();

            shape.generate_mtsdf(&mut bitmap, framing, config);

            // optionally
            shape.correct_sign(&mut bitmap, framing, fill_rule);
            shape.correct_msdf_error(&mut bitmap, framing, config);

            // let error = shape.estimate_error(&mut bitmap, framing, 5, FillRule::default());
            // log::debug!("Estimated error: {error}");

            bitmap.flip_y();

            let atlas_index = ascii_code as usize - 33;
            data[(atlas_index * Self::ATLAS_SIZE)..((atlas_index + 1) * Self::ATLAS_SIZE)]
                .copy_from_slice(bitmap.convert::<msdfgen::Rgba<u8>>().raw_pixels());
        }

        data
    }

    fn atlas_bind_group(device: &Device, queue: &wgpu::Queue) -> (BindGroupLayout, BindGroup) {
        let atlas_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            // idk what filterable or multisampled does, but the rest are straight-forward
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("atlas_bind_group_layout"),
            });

        let texture_size = wgpu::Extent3d {
            width: Self::SDF_WIDTH as _,
            height: Self::SDF_HEIGHT as _,
            // All textures are stored as 3D
            depth_or_array_layers: (127 - 33),
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1, // We'll talk about this a little later
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            // COPY_DST means that we want to copy data to this texture
            // I guess texture_2d_array is also storage binding, even though google says it uses texture binding (grrr)
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("atlas_texture"),
            // This is the same as with the SurfaceConfig. It
            // specifies what texture formats can be used to
            // create TextureViews for this texture. The base
            // texture format (Rgba8UnormSrgb in this case) is
            // always supported. Note that using a different
            // texture format is not supported on the WebGL2
            // backend.
            view_formats: &[],
        });

        // TODO: apparently texture is like vec and textureview is like slice
        // so I can just send over the textureview
        // (this goes for every time I have textureview, so chargrid and image as well)
        queue.write_texture(
            // Tells wgpu where to copy the pixel data
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            // The actual pixel data
            bytemuck::cast_slice(&Self::generate_atlas_raw_data()),
            // The layout of the texture
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((Self::SDF_PIXEL_BYTES * Self::SDF_WIDTH) as u32),
                rows_per_image: Some(Self::SDF_HEIGHT as u32),
            },
            texture_size,
        );

        // We don't need to configure the texture view much, so let's
        // let wgpu define it.
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &atlas_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    // TODO: use texture view array?
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("atlas_bind_group"),
        });

        (atlas_bind_group_layout, bind_group)
    }
}

/// Data needed for drawing
pub(super) struct DrawingSharedData<'a> {
    render_pass: wgpu::RenderPass<'a>,

    rectangle_renderer: &'a RectangleRenderer,
    image_renderer: &'a ImageRenderer,
    char_grid_renderer: &'a CharGridRenderer,

    /// Just one large triangle
    vertex_buffer: &'a wgpu::Buffer,

    font_system: &'a mut FontSystem,
    swash_cache: &'a mut glyphon::SwashCache,
    viewport: &'a mut glyphon::Viewport,
    atlas: &'a mut glyphon::TextAtlas,
    text_cache: &'a mut TextCache,
    image_cache: &'a mut ImageCache,
    // text_renderer: &'a mut glyphon::TextRenderer,
    // text_buffer: &'a mut glyphon::Buffer,
    device: &'a Device,
    queue: &'a wgpu::Queue,
    // surface: &'a wgpu::Surface<'static>,
    _surface_config: &'a SurfaceConfiguration,
}

/// Text and font size (bits)
type TextKey = (Vec<(String, AttrsOwned)>, u32);

/// Text shaped in earlier frames, since shaping every frame took most of the frame (#98)
pub(super) struct TextCache {
    /// The bool is whether this frame drew it
    buffers: HashMap<TextKey, (glyphon::Buffer, bool)>,
    /// The font `Family::Monospace` resolves to. Asked by name, since cosmic-text
    /// checks every system font for each monospace word otherwise.
    monospace: Option<String>,
}
impl TextCache {
    pub(super) fn new(font_system: &FontSystem) -> Self {
        let db = font_system.db();
        let monospace = db
            .query(&fontdb::Query {
                families: &[Family::Monospace],
                ..Default::default()
            })
            .and_then(|id| db.face(id))
            .and_then(|face| face.families.first())
            .map(|(name, _)| name.clone());
        log::info!("Monospace font: {monospace:?}");
        Self {
            buffers: HashMap::new(),
            monospace,
        }
    }

    /// Shapes `text`, or reuses it from an earlier frame
    fn get(
        &mut self,
        font_system: &mut FontSystem,
        text: &[(String, AttrsOwned)],
    ) -> &glyphon::Buffer {
        let size = font_size();
        let monospace = self.monospace.as_deref();
        fn with_monospace<'a>(
            attrs: glyphon::Attrs<'a>,
            monospace: Option<&'a str>,
        ) -> glyphon::Attrs<'a> {
            match (attrs.family, monospace) {
                (Family::Monospace, Some(name)) => attrs.family(Family::Name(name)),
                _ => attrs,
            }
        }
        let (buffer, drawn) = self
            .buffers
            .entry((text.to_vec(), size.to_bits()))
            .or_insert_with(|| {
                let mut buffer = glyphon::Buffer::new(font_system, Metrics::new(size, size));
                buffer.set_rich_text(
                    font_system,
                    text.iter()
                        .map(|(s, attr)| (s.as_str(), with_monospace(attr.as_attrs(), monospace))),
                    &with_monospace(glyphon::Attrs::new().family(Family::Monospace), monospace),
                    glyphon::Shaping::Advanced,
                    None,
                );
                buffer.shape_until_scroll(font_system, false);
                (buffer, false)
            });
        *drawn = true;
        buffer
    }

    /// Forgets the text this frame didn't draw
    fn end_frame(&mut self) {
        self.buffers.retain(|_, (_, drawn)| std::mem::take(drawn));
    }
}
/// Textures of `UIElement::Image`s, uploaded when first drawn
#[derive(Default)]
pub(super) struct ImageCache {
    /// By image id; the bool is whether this frame drew it
    textures: HashMap<u64, (TextureView, bool)>,
}
impl ImageCache {
    fn get(&mut self, device: &Device, queue: &wgpu::Queue, image: &Image) -> &TextureView {
        let (view, drawn) = self.textures.entry(image.id()).or_insert_with(|| {
            let [width, height] = image.size();
            let size = wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: Some("image_texture"),
                view_formats: &[],
            });
            queue.write_texture(
                texture.as_image_copy(),
                image.rgba(),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                size,
            );
            (
                texture.create_view(&wgpu::TextureViewDescriptor::default()),
                false,
            )
        });
        *drawn = true;
        view
    }

    /// Frees the images this frame didn't draw
    fn end_frame(&mut self) {
        self.textures.retain(|_, (_, drawn)| std::mem::take(drawn));
    }
}

impl DrawingSharedData<'_> {
    fn draw_rect(
        &mut self,
        round_rect: &RoundRect,
        display_area_px: DisplayAreaPx,
        clip: Option<Clip>,
    ) {
        self.render_pass
            .set_pipeline(&self.rectangle_renderer.render_pipeline);
        // these buffers are how we pass data to the gpu
        self.render_pass
            .set_vertex_buffer(0, self.vertex_buffer.slice(..));

        let instances = vec![RoundRectInstance {
            // this currently takes in top left
            origin: [
                display_area_px.0[0][0] as f32,
                display_area_px.0[0][1] as f32,
            ],
            size: [
                display_area_px.size().width as f32,
                display_area_px.size().height as f32,
            ],
            corner_radius: round_rect.corner_radius,
            border_dist: round_rect.border_width,
            // main_color: inner_color.0.map(|c| f32::from(c) / f32::from(u8::MAX)),
            main_color: round_rect.main_color,
            border_color: round_rect.border_color,
            clip: clip.into(),
        }];

        let instance_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Instance Buffer"),
                contents: bytemuck::cast_slice(&instances),
                usage: wgpu::BufferUsages::VERTEX,
            });

        self.render_pass
            .set_vertex_buffer(1, instance_buffer.slice(..));
        // render_pass
        //     .set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        // render_pass.draw_indexed(0..(INDICES.len() as u32), 0, 0..1);
        self.render_pass.draw(0..Vertex::VERTICES.len() as _, 0..1); // 1 bc we only draw 1 rect at a time (which I am not happy about)

        // let mut pb = raqote::PathBuilder::new();
        // pb.rect(
        //     area.0.x.pixels(dt.width()) as f32,
        //     area.0.y.pixels(dt.height()) as f32,
        //     area.size().width.pixels(dt.width()) as f32,
        //     area.size().height.pixels(dt.height()) as f32,
        // );
        // let path = pb.finish();
        // dt.fill(&path, &Source::Solid(color.into()), &DrawOptions::new());
    }

    fn display_area_to_text_bounds(display_area_px: DisplayAreaPx) -> glyphon::TextBounds {
        glyphon::TextBounds {
            left: display_area_px.0[0][0] as i32,
            top: display_area_px.0[0][1] as i32,
            right: display_area_px.0[1][0] as i32,
            bottom: display_area_px.0[1][1] as i32,
        }
    }
    fn draw_text_with_glyphon(
        &mut self,
        text: &[(String, AttrsOwned)],
        display_area_px: DisplayAreaPx,
        clip: Option<Clip>,
        scale: f32,
    ) {
        // glyphon clips to rectangular bounds only, so rounded clip corners aren't followed
        let bounds = clip.map_or(display_area_px, |clip| {
            clip.area.intersection(display_area_px)
        });
        let text_buffer = self.text_cache.get(self.font_system, text);

        let mut text_renderer =
            TextRenderer::new(self.atlas, self.device, MultisampleState::default(), None);

        text_renderer
            .prepare(
                self.device,
                self.queue,
                self.font_system,
                self.atlas,
                self.viewport,
                [glyphon::TextArea {
                    buffer: text_buffer,
                    left: display_area_px.0[0][0] as f32,
                    top: display_area_px.0[0][1] as f32,
                    scale,
                    bounds: Self::display_area_to_text_bounds(bounds),
                    default_color: glyphon::Color::rgb(255, 255, 255),
                    custom_glyphs: &[],
                }],
                self.swash_cache,
            )
            .unwrap();

        text_renderer
            .render(self.atlas, self.viewport, &mut self.render_pass)
            .unwrap();

        // // FIXME: doesn't work with space
        // dt.draw_text(
        //     font,
        //     FONT_SIZE as f32,
        //     text,
        //     DisplayCoord::new(
        //         container_area.0.x,
        //         container_area.0.y + FONT_SIZE.into(),
        //     )
        //     .into_raqote_point(dt),
        //     &Source::Solid(SolidSource {
        //         r: 0,
        //         g: 0xFF,
        //         b: 0xFF,
        //         a: 0xFF,
        //     }),
        //     &DrawOptions::new(),
        // );
    }

    /// load the actual char grid info as if it was a texture where each pixel is a char
    fn load_char_grid_to_tex(&mut self, char_grid: &CharGrid, width: u32, height: u32) {
        self.render_pass
            .set_bind_group(0, Some(&self.char_grid_renderer.atlas_bind_group), &[]);

        let texture_size = wgpu::Extent3d {
            width,
            height,
            // All textures are stored as 3D, we represent our 2D texture
            // by setting depth to 1.
            depth_or_array_layers: 1,
        };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1, // We'll talk about this a little later
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Uint,
            // STORAGE_BINDING instead of TEXTURE_BINDING because we are using a storage_texture instead of normal texture
            // COPY_DST means that we want to copy data to this texture
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("char_grid_storage"),
            // This is the same as with the SurfaceConfig. It
            // specifies what texture formats can be used to
            // create TextureViews for this texture. The base
            // texture format (Rgba8UnormSrgb in this case) is
            // always supported. Note that using a different
            // texture format is not supported on the WebGL2
            // backend.
            view_formats: &[],
        });

        self.queue.write_texture(
            // Tells wgpu where to copy the pixel data
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            // The actual pixel data
            bytemuck::cast_slice(char_grid.content()),
            // The layout of the texture
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(InternalCharCell::BYTES as u32 * width),
                rows_per_image: Some(height),
            },
            texture_size,
        );

        // We don't need to configure the texture view much, so let's
        // let wgpu define it.
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.char_grid_renderer.char_grid_texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&texture_view),
            }],
            label: Some("bind_group"),
        });
        self.render_pass.set_bind_group(1, Some(&bind_group), &[]);
    }
    fn draw_char_grid(
        &mut self,
        char_grid: &CharGrid,
        display_area_px: DisplayAreaPx,
        clip: Option<Clip>,
    ) {
        // NOTE: rn, the area is just auto-computed from the bounds,
        // TODO: let apps customize bounds

        self.render_pass
            .set_pipeline(&self.char_grid_renderer.render_pipeline);

        let width = char_grid.width() as u32;
        let height = char_grid.height() as u32;

        // load the actual char grid info as if it was a texture where each pixel is a char
        self.load_char_grid_to_tex(char_grid, width, height);

        // these buffers are how we pass data to the gpu
        // pass in the large triangle
        self.render_pass
            .set_vertex_buffer(0, self.vertex_buffer.slice(..));

        // set instance buffer
        {
            // log::debug!(
            //     "Char Grid Area: {} {} px",
            //     area.size()
            //         .width
            //         .pixels(drawing_shared_data.surface_config.width as _),
            //     area.size()
            //         .height
            //         .pixels(drawing_shared_data.surface_config.height as _),
            // );
            let instances = vec![CharGridInstance {
                // this currently takes in top left
                origin: [
                    display_area_px.0[0][0] as f32,
                    display_area_px.0[0][1] as f32,
                ],
                size: [
                    display_area_px.size().width as f32,
                    display_area_px.size().height as f32,
                ],
                grid_size: [width, height],
                clip: clip.into(),
            }];

            let instance_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Instance Buffer"),
                        contents: bytemuck::cast_slice(&instances),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

            // vertex buffer slot 1 is actually the instance buffer
            self.render_pass
                .set_vertex_buffer(1, instance_buffer.slice(..));
        }

        self.render_pass.draw(0..Vertex::VERTICES.len() as _, 0..1); // 1 bc we only draw 1 image at a time (which I am not happy about)

        /*
        let mut text_buffer = glyphon::Buffer::new(
            drawing_shared_data.font_system,
            Metrics::new(FONT_SIZE_F, FONT_SIZE_F),
        );

        let attrs = &glyphon::Attrs::new().family(glyphon::Family::Monospace);
        let shaping = glyphon::Shaping::Advanced;

        // // text_buffer.set_size(
        // //     &mut drawing_shared_data.font_system,
        // //     Some(physical_width),
        // //     Some(physical_height),
        // // );

        // // text_buffer.set_text(
        // //     drawing_shared_data.font_system,
        // //     character.to_string().as_str(),
        // //     &glyphon::Attrs::new().family(glyphon::Family::Monospace),
        // //     glyphon::Shaping::Advanced,
        // //     None,
        // // );
        // text_buffer.shape_until_scroll(drawing_shared_data.font_system, false);

        // let mut text_renderer = TextRenderer::new(
        //     drawing_shared_data.atlas,
        //     drawing_shared_data.device,
        //     MultisampleState::default(),
        //     None,
        // );

        for row in 0..char_grid.height() {
            for col in 0..char_grid.width() {
                // log::debug!(
                //     "Row: {row}, col: {col}, w: {}, h: {}",
                //     char_grid.width(),
                //     char_grid.height()
                // );
                let CharCell { character, fg, bg } = char_grid.get_char(row, col);

                let top_left = DisplayCoord::new(
                    container_area.0.x + DisplayUnits::Pixels(FONT_SIZE / 2 * (col as i32)),
                    container_area.0.y + DisplayUnits::Pixels(FONT_SIZE * (row as i32) + 1),
                );

                if !container_area.contains(
                    top_left,
                    [
                        drawing_shared_data.viewport.resolution().width as i32,
                        drawing_shared_data.viewport.resolution().height as i32,
                    ],
                ) {
                    // FIXME: not completely foolproof -- main purpose is just optimization
                    continue;
                }

                // let bot_right = DisplayCoord::new(
                //     container_area.0.x
                //         + DisplayUnits::Pixels(
                //             FONT_SIZE / 2 * ((col_index + 1) as i32),
                //         ),
                //     container_area.0.y
                //         + DisplayUnits::Pixels(FONT_SIZE * (line_index + 1) as i32),
                // );

                if bg != Color::TRANSPARENT {
                    Self::fill_rect(
                        drawing_shared_data,
                        DisplayArea::from_corner_size(
                            top_left,
                            DisplaySize::new(
                                (FONT_SIZE / 2 + 1).into(),
                                (FONT_SIZE + 2).into(),
                            ),
                        ),
                        0.,
                        // Set to 1 for dbg boxes
                        0.,
                        bg,
                        Color::TRANSPARENT,
                    );
                }

                if character == ' ' {
                    continue;
                }

                // let mut text_buffer = glyphon::Buffer::new(
                //     drawing_shared_data.font_system,
                //     Metrics::new(FONT_SIZE_F, FONT_SIZE_F),
                // );

                // text_buffer.set_size(
                //     &mut drawing_shared_data.font_system,
                //     Some(physical_width),
                //     Some(physical_height),
                // );

                text_buffer.set_text(
                    drawing_shared_data.font_system,
                    character.to_string().as_str(),
                    // &glyphon::Attrs::new().family(glyphon::Family::Monospace),
                    attrs,
                    // glyphon::Shaping::Advanced,
                    shaping,
                    None,
                );
                text_buffer.shape_until_scroll(drawing_shared_data.font_system, false);

                let mut text_renderer = TextRenderer::new(
                    drawing_shared_data.atlas,
                    drawing_shared_data.device,
                    MultisampleState::default(),
                    None,
                );

                text_renderer
                    .prepare(
                        drawing_shared_data.device,
                        drawing_shared_data.queue,
                        drawing_shared_data.font_system,
                        drawing_shared_data.atlas,
                        drawing_shared_data.viewport,
                        [glyphon::TextArea {
                            buffer: &text_buffer,
                            left: top_left
                                .x
                                .pixels(drawing_shared_data.surface_config.width as _)
                                as _,
                            top: top_left
                                .y
                                .pixels(drawing_shared_data.surface_config.height as _)
                                as _,
                            scale: 1.0,
                            bounds: Self::display_area_to_text_bounds(
                                container_area,
                                drawing_shared_data.surface_config,
                            ),
                            default_color: fg.into(),
                            custom_glyphs: &[],
                        }],
                        drawing_shared_data.swash_cache,
                    )
                    .unwrap();

                text_renderer
                    .render(
                        drawing_shared_data.atlas,
                        drawing_shared_data.viewport,
                        &mut drawing_shared_data.render_pass,
                    )
                    .unwrap();

                // drawing_shared_data.queue.submit(Some(encoder.finish()));
                // drawing_shared_data.frame.present();

                // dt.draw_text(
                //     font,
                //     FONT_SIZE as f32,
                //     &character.to_string(),
                //     // `start` is actually bottom left corner
                //     bot_left.into_raqote_point(dt),
                //     &raqote::Source::Solid((*fg).into()),
                //     &DrawOptions::new(),
                // );
            }
        }
        */
    }

    fn draw_texture(
        &mut self,
        texture_view: &TextureView,
        display_area_px: DisplayAreaPx,
        clip: Option<Clip>,
    ) {
        self.render_pass
            .set_pipeline(&self.image_renderer.render_pipeline);

        // set bind group (which holds the image texture)
        {
            let diffuse_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            });

            let diffuse_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.image_renderer.image_texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                    },
                ],
                label: Some("diffuse_bind_group"),
            });
            self.render_pass
                .set_bind_group(0, Some(&diffuse_bind_group), &[]);
        }

        // these buffers are how we pass data to the gpu
        // pass in the large triangle
        self.render_pass
            .set_vertex_buffer(0, self.vertex_buffer.slice(..));

        ImageInstance::set_instance_buffer(self, display_area_px, clip);

        self.render_pass.draw(0..Vertex::VERTICES.len() as _, 0..1); // 1 bc we only draw 1 image at a time (which I am not happy about)

        /*
        self.render_pass
            .set_pipeline(&self.image_renderer.render_pipeline);

        // set bind group (which holds the image texture)
        {
            let width = image_buffer.width();
            let height = image_buffer.height();

            let texture_size = wgpu::Extent3d {
                width,
                height,
                // All textures are stored as 3D, we represent our 2D texture
                // by setting depth to 1.
                depth_or_array_layers: 1,
            };

            let diffuse_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                size: texture_size,
                mip_level_count: 1, // We'll talk about this a little later
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // Most images are stored using sRGB, so we need to reflect that here.
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                // TEXTURE_BINDING tells wgpu that we want to use this texture in shaders
                // COPY_DST means that we want to copy data to this texture
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: Some("diffuse_texture"),
                // This is the same as with the SurfaceConfig. It
                // specifies what texture formats can be used to
                // create TextureViews for this texture. The base
                // texture format (Rgba8UnormSrgb in this case) is
                // always supported. Note that using a different
                // texture format is not supported on the WebGL2
                // backend.
                view_formats: &[],
            });

            self.queue.write_texture(
                // Tells wgpu where to copy the pixel data
                wgpu::TexelCopyTextureInfo {
                    texture: &diffuse_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                // The actual pixel data
                image_buffer,
                // The layout of the texture
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * width),
                    rows_per_image: Some(height),
                },
                texture_size,
            );

            // We don't need to configure the texture view much, so let's
            // let wgpu define it.
            let diffuse_texture_view =
                diffuse_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let diffuse_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            });

            let diffuse_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.image_renderer.image_texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                    },
                ],
                label: Some("diffuse_bind_group"),
            });
            self.render_pass
                .set_bind_group(0, Some(&diffuse_bind_group), &[]);
        }

        // these buffers are how we pass data to the gpu
        // pass in the large triangle
        self.render_pass
            .set_vertex_buffer(0, self.vertex_buffer.slice(..));

        ImageInstance::set_instance_buffer(self, area);

        self.render_pass.draw(0..Vertex::VERTICES.len() as _, 0..1); // 1 bc we only draw 1 image at a time (which I am not happy about)
        */
    }

    fn draw_primitive_element(
        &mut self,
        element: &UIPrimitiveElement,
        display_area_px: DisplayAreaPx,
        clip: Option<Clip>,
    ) {
        match element {
            UIPrimitiveElement::RoundRect(round_rect) => {
                self.draw_rect(round_rect, display_area_px, clip);
            }
            UIPrimitiveElement::Text(items, scale) => {
                self.draw_text_with_glyphon(items, display_area_px, clip, *scale);
            }
            UIPrimitiveElement::CharGrid(char_grid) => {
                self.draw_char_grid(char_grid, display_area_px, clip);
            }
            UIPrimitiveElement::Texture(texture_view) => {
                self.draw_texture(texture_view, display_area_px, clip);
            }
            // an empty texture is invalid
            UIPrimitiveElement::Image(image) if image.size().contains(&0) => {}
            UIPrimitiveElement::Image(image) => {
                let view = self.image_cache.get(self.device, self.queue, image).clone();
                self.draw_texture(&view, display_area_px, clip);
            }
        }
    }

    /// TODO: chunk consecutive elements of same type
    fn draw_primitive_scene(&mut self, scene: &PrimitiveScene) {
        for (element, display_area_px, clip) in &scene.elements {
            self.draw_primitive_element(element, *display_area_px, *clip);
        }
    }
}

/// `UIDisplay::draw` didn't draw this time (eg the surface needed reconfiguring)
pub(super) struct Skipped;

/// A drawn frame being copied into a buffer
struct FrameCopy {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    /// Rows are padded to `wgpu::COPY_BYTES_PER_ROW_ALIGNMENT`
    padded_row: u32,
    /// The frame is R,G,B,A rather than B,G,R,A
    swap_red_blue: bool,
}
impl FrameCopy {
    /// Copies `texture` once `encoder` is submitted; `None` if it can't be copied
    fn start(
        device: &Device,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
    ) -> Option<Self> {
        if !texture.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return None;
        }
        let swap_red_blue = match texture.format() {
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => false,
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => true,
            _ => return None,
        };
        let (width, height) = (texture.width(), texture.height());
        let padded_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture_buffer"),
            size: u64::from(padded_row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row),
                    rows_per_image: None,
                },
            },
            texture.size(),
        );
        Some(Self {
            buffer,
            width,
            height,
            padded_row,
            swap_red_blue,
        })
    }

    /// Blocks until the copy is done
    fn finish(self, device: &Device) -> Option<CapturedFrame> {
        let (tx, rx) = mpsc::channel();
        self.buffer
            .map_async(wgpu::MapMode::Read, .., move |result| {
                let _ = tx.send(result);
            });
        device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        rx.recv().ok()?.ok()?;

        let mapped = self.buffer.get_mapped_range(..);
        let row_len = self.width as usize * 4;
        let mut bgra = Vec::with_capacity(row_len * self.height as usize);
        for row in mapped.chunks_exact(self.padded_row as usize) {
            bgra.extend_from_slice(&row[..row_len]);
        }
        if self.swap_red_blue {
            for pixel in bgra.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        Some(CapturedFrame {
            width: self.width,
            height: self.height,
            bgra,
        })
    }
}

impl UIDisplay {
    /// With `capture`, also copies the frame out (`Ok(None)` if it couldn't be)
    ///
    /// REVIEW: move somewhere else?
    pub(super) fn draw(
        wgpu_data: &mut WgpuData,
        content: &PrimitiveScene,
        capture: bool,
    ) -> Result<Option<CapturedFrame>, Skipped> {
        let WgpuData {
            // window,
            device,
            queue,
            surface,
            surface_config,
            font_system,
            swash_cache,
            viewport,
            atlas,
            text_cache,
            image_cache,
            // text_renderer,
            // text_buffer,
            rectangle_renderer,
            image_renderer,
            char_grid_renderer,
            vertex_buffer,
            // index_buffer,
            // instance_buffer,
            // instances,
            ..
        } = wgpu_data;

        let output = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output) => output,
            wgpu::CurrentSurfaceTexture::Suboptimal(output) => {
                // still usable this frame; reconfiguring must wait until `output` is
                // presented/dropped, so just render as-is and let the next acquire
                // (which will report `Outdated` if still mismatched) trigger reconfigure
                output
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                // skip this frame and try again later
                return Err(Skipped);
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(device, surface_config);
                return Err(Skipped);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("wgpu surface validation error while acquiring frame");
                return Err(Skipped);
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // I don't really understand the other junk here,
                        // but this is the background
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            // if this isn't opaque, weird artifacts appear
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });

            // tell it the resolution, which shouldn't change for the rest of the draw calls
            viewport.update(
                queue,
                glyphon::Resolution {
                    width: surface_config.width,
                    height: surface_config.height,
                },
            );

            let mut drawing_shared_data = DrawingSharedData {
                render_pass,
                rectangle_renderer,
                image_renderer,
                char_grid_renderer,
                vertex_buffer,
                font_system,
                swash_cache,
                // surface,
                viewport,
                atlas,
                text_cache,
                image_cache,
                device,
                queue,
                _surface_config: surface_config,
                // text_renderer,
                // text_buffer,
            };

            drawing_shared_data.draw_primitive_scene(content);
        }
        text_cache.end_frame();
        image_cache.end_frame();
        // lets glyphon evict glyphs this frame didn't use, else the atlas fills up
        atlas.trim();

        let copy = capture
            .then(|| FrameCopy::start(device, &mut encoder, &output.texture))
            .flatten();
        queue.submit(iter::once(encoder.finish()));
        output.present();
        let captured = copy.and_then(|copy| copy.finish(device));

        // let stride = self.width as i32 * 4;

        // let buffer = self.buffer.get_or_insert_with(|| {
        //     self.pool
        //         .create_buffer(
        //             self.width as i32,
        //             self.height as i32,
        //             stride,
        //             wl_shm::Format::Argb8888,
        //         )
        //         .expect("create buffer")
        //         .0
        // });

        // let canvas = match self.pool.canvas(buffer) {
        //     Some(canvas) => canvas,
        //     None => {
        //         // This should be rare, but if the compositor has not released the previous
        //         // buffer, we need double-buffering.
        //         let (second_buffer, canvas) = self
        //             .pool
        //             .create_buffer(
        //                 self.width as i32,
        //                 self.height as i32,
        //                 stride,
        //                 wl_shm::Format::Argb8888,
        //             )
        //             .expect("create buffer");
        //         *buffer = second_buffer;
        //         canvas
        //     }
        // };

        // // Draw to the window:
        // // FIXME find an actual fix to the height difference
        // if canvas.len() as u32 == 4 * self.width * self.height {
        //     let mut dt = DrawTarget::new(self.width as i32, self.height as i32);
        //     self.root_element
        //         .lock()
        //         .unwrap()
        //         .draw(&mut dt, DisplayArea::FULL, &self.font);
        //     canvas.copy_from_slice(dt.get_data_u8());
        // }

        // // Damage the entire window
        // self.window
        //     .wl_surface()
        //     .damage_buffer(0, 0, self.width as i32, self.height as i32);

        // // Request our next frame
        // self.window
        //     .wl_surface()
        //     .frame(qh, self.window.wl_surface().clone());

        // // Attach and commit to present.
        // buffer
        //     .attach_to(self.window.wl_surface())
        //     .expect("buffer attach");
        // self.window.commit();

        Ok(captured)
    }
}

// #[cfg(test)]
// mod tests {
//     use wgpu::{DeviceDescriptor, TextureViewDescriptor};

//     use crate::{UIDisplay, ui_element::UIElement};

//     /// https://sotrh.github.io/learn-wgpu/showcase/windowless/#a-triangle-without-a-window
//     /// For using GPU without making a window
//     #[test]
//     fn draw_char_grid() {
//         let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
//         let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
//             power_preference: wgpu::PowerPreference::default(),
//             compatible_surface: None,
//             force_fallback_adapter: false,
//         }))
//         .unwrap();
//         let (device, queue) =
//             pollster::block_on(adapter.request_device(&DeviceDescriptor::default())).unwrap();

//         let texture_size = 256u32;

//         let texture_desc = wgpu::TextureDescriptor {
//             size: wgpu::Extent3d {
//                 width: texture_size,
//                 height: texture_size,
//                 depth_or_array_layers: 1,
//             },
//             mip_level_count: 1,
//             sample_count: 1,
//             dimension: wgpu::TextureDimension::D2,
//             format: wgpu::TextureFormat::Rgba8UnormSrgb,
//             usage: wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::RENDER_ATTACHMENT,
//             label: None,
//             view_formats: &[],
//         };
//         let texture = device.create_texture(&texture_desc);
//         let texture_view = texture.create_view(&TextureViewDescriptor::default());

//         UIElement::draw(drawing_shared_data, char_grid, area);
//     }
// }
