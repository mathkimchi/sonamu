//! The setting `wallpaper` (#104): drawn under everything, even wallpaper programs' layers

use sonamu_common::{
    notification::Replacing,
    settings::{self, ShaderWallpaper, Wallpaper as Setting},
};
use sonamu_ui::{
    color::Color,
    ui_element::{Image, Style, UIElement, UITree},
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};
use wgpu::naga;

/// Declares Shadertoy's inputs before the user's shader
const PREAMBLE: &str = "#version 450
layout(set = 0, binding = 0) uniform SonamuGlobals {
    vec4 resolution_time_frame;
    vec4 mouse;
} sonamu_globals;
#define iResolution vec3(sonamu_globals.resolution_time_frame.xy, 1.0)
#define iTime sonamu_globals.resolution_time_frame.z
#define iFrame int(floatBitsToUint(sonamu_globals.resolution_time_frame.w))
#define iMouse sonamu_globals.mouse
layout(location = 0) out vec4 sonamu_color;
";

/// Calls the user's `mainImage` with Shadertoy's origin (bottom left)
const EPILOGUE: &str = "
void main() {
    vec4 color = vec4(0.0);
    mainImage(color, vec2(gl_FragCoord.x, iResolution.y - gl_FragCoord.y));
    sonamu_color = vec4(color.rgb, 1.0);
}
";

/// A WGSL shader's inputs: `PREAMBLE`'s buffer as `wallpaper`. A shader may declare it itself (eg so an editor knows
/// its type), exactly so (`declared_layout`)
const WGSL_PREAMBLE: &str = "struct Wallpaper {
    resolution: vec2f,
    time: f32,
    frame: u32,
    mouse: vec4f,
}
@group(0) @binding(0) var<uniform> wallpaper: Wallpaper;
";

/// Calls the user's WGSL `main_image`, like `EPILOGUE`
const WGSL_EPILOGUE: &str = "
@fragment
fn main(@builtin(position) position: vec4f) -> @location(0) vec4f {
    let color = main_image(vec2f(position.x, wallpaper.resolution.y - position.y));
    return vec4f(color.rgb, 1.0);
}
";

/// Whether a WGSL shader declares `wallpaper` itself (then `WGSL_PREAMBLE` isn't added)
fn declares_wallpaper(user: &str) -> bool {
    user.contains("var<uniform> wallpaper")
}

/// The `wallpaper` uniform's fields (name, offset, type), if it's at group 0, binding 0
fn uniform_layout(module: &naga::Module) -> Option<Vec<(Option<String>, u32, String)>> {
    let (_, global) = module
        .global_variables
        .iter()
        .find(|(_, global)| global.name.as_deref() == Some("wallpaper"))?;
    let binding = global.binding.as_ref()?;
    if global.space != naga::AddressSpace::Uniform || (binding.group, binding.binding) != (0, 0) {
        return None;
    }
    let naga::TypeInner::Struct { members, .. } = &module.types[global.ty].inner else {
        return None;
    };
    Some(
        members
            .iter()
            .map(|member| {
                let ty = format!("{:?}", module.types[member.ty].inner);
                (member.name.clone(), member.offset, ty)
            })
            .collect(),
    )
}

/// Whether a shader's own `wallpaper` declaration is `WGSL_PREAMBLE`'s
fn declared_layout(module: &naga::Module) -> Result<(), String> {
    let expected = naga::front::wgsl::parse_str(WGSL_PREAMBLE)
        .ok()
        .and_then(|preamble| uniform_layout(&preamble));
    if uniform_layout(module).is_some_and(|layout| Some(layout) == expected) {
        Ok(())
    } else {
        Err(format!(
            "`wallpaper` is declared differently from Sonamu's (or leave it out):\n{WGSL_PREAMBLE}"
        ))
    }
}

/// A wallpaper shader's language, by its file's extension: `.wgsl`, else GLSL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
    Glsl,
    Wgsl,
}
impl Language {
    fn of(path: &Path) -> Self {
        if path
            .extension()
            .is_some_and(|extension| extension == "wgsl")
        {
            Self::Wgsl
        } else {
            Self::Glsl
        }
    }
}

/// A triangle covering the target
const VERTEX: &str = "
@vertex
fn main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}
";

/// Written as is (Shadertoy's colors are already sRGB), read through an sRGB view like client buffers
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Errors loading the wallpaper, replaced by the next
static NOTIFICATION: Replacing = Replacing::new();

/// The user's Shadertoy-style shader as a whole fragment shader
fn fragment_source(user: &str, language: Language) -> String {
    match language {
        Language::Glsl => format!("{PREAMBLE}{user}\n{EPILOGUE}"),
        Language::Wgsl if declares_wallpaper(user) => format!("{user}\n{WGSL_EPILOGUE}"),
        Language::Wgsl => format!("{WGSL_PREAMBLE}{user}\n{WGSL_EPILOGUE}"),
    }
}

/// Parses and checks the user's shader (so a mistake is a message, not a GPU error); parse errors are at lines of
/// the user's file
fn fragment_module(user: &str, language: Language) -> Result<naga::Module, String> {
    let source = fragment_source(user, language);
    let module = match language {
        Language::Glsl => glsl_module(user, &source)?,
        Language::Wgsl => naga::front::wgsl::parse_str(&source).map_err(|err| {
            let preamble_lines = if declares_wallpaper(user) {
                0
            } else {
                WGSL_PREAMBLE.lines().count()
            };
            match err.location(&source) {
                Some(at) => format!(
                    "wgsl:{}:{}: {}",
                    (at.line_number as usize).saturating_sub(preamble_lines),
                    at.line_position,
                    err.message()
                ),
                None => err.emit_to_string(&source),
            }
        })?,
    };
    if language == Language::Wgsl && declares_wallpaper(user) {
        declared_layout(&module)?;
    }
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .map_err(|err| err.emit_to_string(&source))?;
    Ok(module)
}

/// Parses a whole GLSL fragment shader (`source`, with `user` in it); errors at `user`'s lines
fn glsl_module(user: &str, source: &str) -> Result<naga::Module, String> {
    naga::front::glsl::Frontend::default()
        .parse(
            &naga::front::glsl::Options::from(naga::ShaderStage::Fragment),
            source,
        )
        .map_err(|err| {
            let lines: Vec<String> = err
                .errors
                .iter()
                .map(|error| {
                    let start = error.meta.to_range().map_or(0, |range| range.start);
                    let at = &user[..start.saturating_sub(PREAMBLE.len()).min(user.len())];
                    let line = at.matches('\n').count() + 1;
                    let column = at.len() - at.rfind('\n').map_or(0, |newline| newline + 1) + 1;
                    format!("glsl:{line}:{column}: {}", error.kind)
                })
                .collect();
            lines.join("\n")
        })
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

/// The wallpaper's file, as it was when last loaded (or tried)
struct Watched {
    path: PathBuf,
    modified: Option<SystemTime>,
}
impl Watched {
    fn now(path: PathBuf) -> Self {
        Self {
            modified: modified(&path),
            path,
        }
    }

    fn changed(&self) -> bool {
        modified(&self.path) != self.modified
    }
}

/// What's drawn now
enum State {
    None,
    Color(Color),
    Image(Image),
    Shader(Box<Shader>),
}

/// A shader wallpaper: rendered into a texture at most `fps` times a second
struct Shader {
    setting: ShaderWallpaper,
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// The texture's size, and its sRGB view
    target: Option<([u32; 2], wgpu::Texture, wgpu::TextureView)>,
    start: Instant,
    /// A new frame is to be drawn: `next_frame` was called, or the target is new
    due: bool,
    frame: u32,
}
impl Shader {
    fn new(setting: &ShaderWallpaper, path: &Path, device: &wgpu::Device) -> Result<Self, String> {
        let user =
            std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
        let language = Language::of(path);
        fragment_module(&user, language).map_err(|err| format!("{}:\n{err}", path.display()))?;
        let source = fragment_source(&user, language).into();
        let fragment = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wallpaper fragment"),
            source: match language {
                Language::Glsl => wgpu::ShaderSource::Glsl {
                    shader: source,
                    stage: naga::ShaderStage::Fragment,
                    defines: &[],
                },
                Language::Wgsl => wgpu::ShaderSource::Wgsl(source),
            },
        });
        let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("wallpaper vertex"),
            source: wgpu::ShaderSource::Wgsl(VERTEX.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("wallpaper globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wallpaper globals"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("wallpaper globals"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("wallpaper"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wallpaper"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &fragment,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(FORMAT.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            setting: setting.clone(),
            pipeline,
            globals,
            bind_group,
            target: None,
            start: Instant::now(),
            due: true,
            frame: 0,
        })
    }

    /// Its texture's view, drawing a frame first if one is due; `window` is the window's size
    fn view(
        &mut self,
        window: [u32; 2],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> wgpu::TextureView {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a fraction of a size"
        )]
        let size = window.map(|side| ((side as f32 * self.setting.scale).round() as u32).max(1));
        if self.target.as_ref().is_none_or(|(at, ..)| *at != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("wallpaper"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[FORMAT.add_srgb_suffix()],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(FORMAT.add_srgb_suffix()),
                ..Default::default()
            });
            self.target = Some((size, texture, view));
            self.due = true;
        }
        let (_, texture, view) = self.target.as_ref().expect("just made");
        let now = Instant::now();
        if std::mem::take(&mut self.due) {
            #[expect(
                clippy::cast_precision_loss,
                reason = "sizes and frame counts are small"
            )]
            let globals: [f32; 8] = [
                size[0] as f32,
                size[1] as f32,
                now.duration_since(self.start).as_secs_f32(),
                // a `u32` to WGSL (`int(floatBitsToUint(..))` in GLSL)
                f32::from_bits(self.frame),
                0.0,
                0.0,
                0.0,
                0.0,
            ];
            self.frame += 1;
            let bytes: Vec<u8> = globals
                .iter()
                .flat_map(|value| value.to_ne_bytes())
                .collect();
            queue.write_buffer(&self.globals, 0, &bytes);
            let target = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("wallpaper"),
            });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("wallpaper"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            queue.submit([encoder.finish()]);
        }
        view.clone()
    }
}

/// The wallpaper, kept in sync with the setting
pub struct Wallpaper {
    setting: Setting,
    state: State,
    /// The setting's file, if it has one
    watched: Option<Watched>,
}
impl Wallpaper {
    pub fn new() -> Self {
        Self {
            setting: Setting::None,
            state: State::None,
            watched: None,
        }
    }

    /// Follows the setting if it changed; on an error it keeps what it had and notifies
    pub fn update(&mut self, setting: &Setting, device: &wgpu::Device) {
        if *setting != self.setting {
            self.setting = setting.clone();
            self.load(device);
        }
    }

    /// Loads the setting's file again if it changed since the last try (eg while writing a shader)
    pub fn reload_changed(&mut self, device: &wgpu::Device) {
        if self.watched.as_ref().is_some_and(Watched::changed) {
            self.load(device);
        }
    }

    fn load(&mut self, device: &wgpu::Device) {
        let dir = settings::config_dir().unwrap_or_default();
        let file = match &self.setting {
            Setting::Image(path) => Some(settings::config_path(path, &dir)),
            Setting::Shader(shader) => Some(settings::config_path(&shader.path, &dir)),
            Setting::None | Setting::Color(_) => None,
        };
        // tried once per change of the file, so a broken one isn't retried (and reported) every frame
        self.watched = file.clone().map(Watched::now);
        let loaded = match (&self.setting, file) {
            (Setting::Image(_), Some(path)) => load_image(&path).map(State::Image),
            (Setting::Shader(shader), Some(path)) => {
                Shader::new(shader, &path, device).map(|shader| State::Shader(Box::new(shader)))
            }
            (Setting::Color(color), _) => Ok(State::Color(*color)),
            _ => Ok(State::None),
        };
        match loaded {
            Ok(state) => self.state = state,
            Err(err) => {
                eprintln!("Wallpaper: {err}");
                NOTIFICATION.send("Wallpaper not loaded".to_owned(), err, false);
            }
        }
    }

    /// A shader draws its next frame when next drawn (the runner's timer, at `frame_interval`; other redraws show
    /// the last frame)
    pub fn next_frame(&mut self) {
        if let State::Shader(shader) = &mut self.state {
            shader.due = true;
        }
    }

    /// How often it needs redrawing (a shader's frame rate); `None`: only when it changes
    pub fn frame_interval(&self) -> Option<Duration> {
        match &self.state {
            State::Shader(shader) => Some(Duration::from_secs(1) / shader.setting.fps.max(1)),
            _ => None,
        }
    }

    /// What to draw under everything in a window `size` big, if anything
    pub fn element(
        &mut self,
        size: [u32; 2],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Option<UITree> {
        #[expect(clippy::cast_precision_loss, reason = "window sizes are small")]
        let window = size.map(|side| side as f32);
        let full = Style::absolute([0.0, 0.0], window);
        match &mut self.state {
            State::None => None,
            State::Color(color) => Some(UITree::leaf_styled(UIElement::Backgrounded(*color), full)),
            State::Image(image) => Some(UITree::leaf_styled(
                UIElement::Image(image.clone()),
                cover(image.size(), window),
            )),
            State::Shader(shader) => Some(UITree::leaf_styled(
                UIElement::Texture(shader.view(size, device, queue)),
                full,
            )),
        }
    }
}

/// An image `[width, height]` scaled to cover `window`, centered (its edges may be cut off)
#[expect(clippy::cast_precision_loss, reason = "image sizes are small")]
fn cover(image: [u32; 2], window: [f32; 2]) -> Style {
    let [width, height] = image.map(|side| side.max(1) as f32);
    let scale = (window[0] / width).max(window[1] / height);
    let size = [width * scale, height * scale];
    Style::absolute(
        [(window[0] - size[0]) / 2.0, (window[1] - size[1]) / 2.0],
        size,
    )
}

fn load_image(path: &Path) -> Result<Image, String> {
    let image = image::ImageReader::open(path)
        .and_then(image::ImageReader::with_guessed_format)
        .map_err(|err| format!("{}: {err}", path.display()))?
        .decode()
        .map_err(|err| format!("{}: {err}", path.display()))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    Image::new(width, height, image.into_raw())
        .ok_or_else(|| format!("{}: bad image", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINE: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
        vec2 uv = fragCoord / iResolution.xy;
        float wave = 0.5 + 0.4 * sin(uv.x * 12.0 + iTime);
        fragColor = vec4(0.0, smoothstep(0.02, 0.0, abs(uv.y - wave)), 0.0, 1.0);
    }";

    #[test]
    fn shadertoy_shader_compiles() {
        assert!(fragment_module(SINE, Language::Glsl).is_ok());
        let frames = "void mainImage(out vec4 c, in vec2 p) { c = vec4(float(iFrame % 2)); }";
        let result = fragment_module(frames, Language::Glsl);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    /// The example config's wallpaper
    #[test]
    fn example_shader_compiles() {
        let shader = include_str!("../../examples/config/wallpapers/sine.glsl");
        assert!(
            fragment_module(shader, Language::Glsl).is_ok(),
            "{:?}",
            fragment_module(shader, Language::Glsl).err()
        );
    }

    #[test]
    fn broken_shader_says_why() {
        let err = fragment_module(
            "void mainImage(out vec4 c, in vec2 p) {\n    c = nope;\n}",
            Language::Glsl,
        )
        .unwrap_err();
        assert!(err.contains("nope"), "{err}");
        // lines are the user's, not counting the preamble
        assert!(err.contains("glsl:2:"), "{err}");
    }

    #[test]
    fn wgsl_shader_compiles() {
        let shader = "fn main_image(position: vec2f) -> vec4f {
            let uv = position / wallpaper.resolution;
            let wave = 0.5 + 0.4 * sin(uv.x * 12.0 + wallpaper.time + f32(wallpaper.frame % 2u));
            return vec4f(0.0, smoothstep(0.02, 0.0, abs(uv.y - wave)), 0.0, 1.0);
        }";
        let result = fragment_module(shader, Language::Wgsl);
        assert!(result.is_ok(), "{:?}", result.err());
        // declared in the file, as Sonamu's
        let declared = format!("{WGSL_PREAMBLE}{shader}");
        let result = fragment_module(&declared, Language::Wgsl);
        assert!(result.is_ok(), "{:?}", result.err());
        assert_eq!(Language::of(Path::new("w/a.wgsl")), Language::Wgsl);
        assert_eq!(Language::of(Path::new("w/a.frag")), Language::Glsl);
    }

    #[test]
    fn broken_wgsl_says_where() {
        let err = fragment_module(
            "fn main_image(p: vec2f) -> vec4f {\n    return nope;\n}",
            Language::Wgsl,
        )
        .unwrap_err();
        assert!(err.contains("nope"), "{err}");
        assert!(err.contains("wgsl:2:"), "{err}");
    }

    /// A declaration in the file must match the buffer
    #[test]
    fn declared_wallpaper_must_match() {
        let shader = "struct Wallpaper { time: f32, resolution: vec2f, frame: u32, mouse: vec4f }
@group(0) @binding(0) var<uniform> wallpaper: Wallpaper;
fn main_image(p: vec2f) -> vec4f { return vec4f(wallpaper.time); }";
        let err = fragment_module(shader, Language::Wgsl).unwrap_err();
        assert!(err.contains("declared differently"), "{err}");
        // lines count from the file's first when it declares it
        let err = fragment_module(
            &format!("{WGSL_PREAMBLE}fn main_image(p: vec2f) -> vec4f {{\n    return nope;\n}}"),
            Language::Wgsl,
        )
        .unwrap_err();
        assert!(err.contains("wgsl:9:"), "{err}");
    }

    #[test]
    fn images_cover_the_window() {
        let style = cover([100, 50], [400.0, 400.0]);
        assert_eq!(style, Style::absolute([-200.0, 0.0], [800.0, 400.0]));
    }

    /// A file is loaded again only once it changes after the last try, so a broken one isn't retried every frame
    #[test]
    fn retried_only_when_changed() {
        let path =
            std::env::temp_dir().join(format!("sonamu-wallpaper-{}.glsl", std::process::id()));
        std::fs::write(&path, "a").unwrap();
        let watched = Watched::now(path.clone());
        assert!(!watched.changed());
        let later = SystemTime::now() + Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(watched.changed());
        assert!(!Watched::now(path).changed());
    }
}
