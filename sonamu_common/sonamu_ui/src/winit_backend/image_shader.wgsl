// From:
// https://github.com/sotrh/learn-wgpu/blob/master/code/beginner/tutorial3-pipeline/src/shader.wgsl

// Vertex shader

struct VertexInput {
    @location(0) position: vec3<f32>,
};

// Just assumes the image is at the corners
struct InstanceInput {
    // top left corner
    @location(1) origin: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) clip_origin: vec2<f32>,
    @location(4) clip_size: vec2<f32>,
    @location(5) clip_radius: f32,
}

// is also the input of the fragment shader
struct VertexOutput {
    // builtin(position) is in clip space (-1 to 1) when outputted by vertex shader,
    // then the rasterizer transforms it to pixel space and gives it to frag shader
    @builtin(position) clip_position: vec4<f32>,
    // // the uv coordinates (in terms of the texture)
    // // goes from Top left (0, 0) to bottom right (1, 1)
    // @location(0) tex_coords: vec2<f32>,
    // top left corner
    @location(0) origin: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) clip_origin: vec2<f32>,
    @location(3) clip_size: vec2<f32>,
    @location(4) clip_radius: f32,
};

fn remap_range(x: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    return out_min + (x - in_min) / (in_max - in_min) * (out_max - out_min);
}

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(model.position, 1.0);
    // out.tex_coords = vec2<f32>(
    //     remap_range(model.position.x, instance.origin.x, instance.origin.x + instance.size.x, 0., 1.),
    //     remap_range(model.position.y, instance.origin.y, instance.origin.y + instance.size.y, 0., 1.),
    // );
    out.origin = instance.origin;
    out.size = instance.size;
    out.clip_origin = instance.clip_origin;
    out.clip_size = instance.clip_size;
    out.clip_radius = instance.clip_radius;
    return out;
}

/// How much of the pixel centered at `p` is inside the rounded box (see `rectangle_shader.wgsl`)
fn coverage(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    let half = max(size, vec2<f32>(0.0)) * 0.5;
    let r = min(radius, min(half.x, half.y));
    let q = abs(p - origin - half) - half + vec2<f32>(r);
    let sdf = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    return clamp(0.5 - sdf, 0.0, 1.0);
}

// Fragment shader

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_coords = vec2<f32>(
        remap_range(in.clip_position.x, in.origin.x, in.origin.x + in.size.x, 0., 1.),
        remap_range(in.clip_position.y, in.origin.y, in.origin.y + in.size.y, 0., 1.),
    );

    // There's gotta be a way to check all at once
    if (0. <= tex_coords.x && tex_coords.x <= 1.) &&
        (0. <= tex_coords.y && tex_coords.y <= 1.) {
        // // turn ARGB => RGBA for wl stuff
        // return textureSample(t_diffuse, s_diffuse, tex_coords).yzwx;
        return textureSample(t_diffuse, s_diffuse, tex_coords)
            * coverage(in.clip_position.xy, in.clip_origin, in.clip_size, in.clip_radius);
    } else {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
}
