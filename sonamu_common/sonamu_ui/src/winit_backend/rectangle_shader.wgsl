// From:
// https://github.com/sotrh/learn-wgpu/blob/master/code/beginner/tutorial3-pipeline/src/shader.wgsl

// Vertex shader

struct VertexInput {
    @location(0) position: vec3<f32>,
    // @location(1) color: vec3<f32>,
};
/// RoundRectInstance:
/// origin: [f32; 2],
/// size: [f32; 2],
/// corner_radius: f32,
/// border_dist: f32,
/// main_color: [f32; 4],
/// border_color: [f32; 4],
struct InstanceInput {
    // Should be top left
    @location(1) origin: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) corner_radius: f32,
    @location(4) border_dist: f32,
    @location(5) main_color: vec4<f32>,
    @location(6) border_color: vec4<f32>,
    @location(7) clip_origin: vec2<f32>,
    @location(8) clip_size: vec2<f32>,
    @location(9) clip_radius: f32,
}

// is also the input of the fragment shader
struct VertexOutput {
    // builtin(position) is in clip space (-1 to 1) when outputted by vertex shader,
    // then the rasterizer transforms it to pixel space and gives it to frag shader
    @builtin(position) clip_position: vec4<f32>,
    @location(0) main_color: vec4<f32>,
    @location(1) origin: vec2<f32>,
    @location(2) size: vec2<f32>,
    @location(3) corner_radius: f32,
    @location(4) border_dist: f32,
    @location(5) border_color: vec4<f32>,
    @location(6) clip_origin: vec2<f32>,
    @location(7) clip_size: vec2<f32>,
    @location(8) clip_radius: f32,
};

// @vertex
// fn vs_main(
//     model: VertexInput,
//     @builtin(vertex_index) index: u32,
// ) -> VertexOutput {
@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(model.position, 1.0);
    // out.color = model.color;
    out.main_color = instance.main_color;
    out.origin = instance.origin;
    out.size = instance.size;
    out.corner_radius = instance.corner_radius;
    out.border_dist = instance.border_dist;
    out.border_color = instance.border_color;
    out.clip_origin = instance.clip_origin;
    out.clip_size = instance.clip_size;
    out.clip_radius = instance.clip_radius;
    return out;
}

// Fragment shader

/// Signed distance from `p` to the rounded box (negative inside)
fn rounded_box_sdf(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    let half = max(size, vec2<f32>(0.0)) * 0.5;
    let r = min(radius, min(half.x, half.y));
    let q = abs(p - origin - half) - half + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

/// How much of the pixel centered at `p` is inside the rounded box
fn coverage(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    return clamp(0.5 - rounded_box_sdf(p, origin, size, radius), 0.0, 1.0);
}

/// Colors are given as sRGB, but the surface wants linear
fn srgb_to_linear(c: vec4<f32>) -> vec4<f32> {
    let low = c.rgb / 12.92;
    let high = pow((c.rgb + 0.055) / 1.055, vec3<f32>(2.4));
    return vec4<f32>(select(high, low, c.rgb <= vec3<f32>(0.04045)), c.a);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = in.clip_position.xy;
    let outer = coverage(p, in.origin, in.size, in.corner_radius);
    var inner = 1.0;
    if in.border_dist > 0.0 {
        let border = vec2<f32>(in.border_dist);
        inner = coverage(p, in.origin + border, in.size - 2.0 * border, max(in.corner_radius - in.border_dist, 0.0));
    }
    let color = mix(srgb_to_linear(in.border_color), srgb_to_linear(in.main_color), inner);
    return color * outer * coverage(p, in.clip_origin, in.clip_size, in.clip_radius);
}
