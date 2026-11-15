// Inspired by: The Universe Within https://www.shadertoy.com/view/lscczl

struct Wallpaper {
    resolution: vec2f,
    time: f32,
    frame: u32,
    mouse: vec4f,
}
@group(0) @binding(0) var<uniform> wallpaper: Wallpaper;

// Simple 2D-to-2D pseudo-random hash function
fn hash22(p: vec2<f32>) -> vec2<f32> {
    // if these numbers are too big, I think there's an overflow
    let a = fract(dot(p*p*p, vec2(186.917, 736.278)));
    let b = a * vec2(823789.732, 318046.427);
    let out1 = fract(b);

    // out1 stops working at larger p
    let c = fract(dot(1/p, vec2(673.316, 843.6478)));
    let d = c * vec2(62729.242, 3673436.137);
    let out2 = fract(d);
    
    return fract(out1 + out2);
}

/// Distance from point to a line segment from a to b
fn line_dist(p: vec2f, a: vec2f, b: vec2f) -> f32 {
    return 0.0;
}

// each grid cell gets a star that moves around
fn star_uv(grid_coord: vec2f, time: f32) -> vec2f {
    // I'm thinking something like 4 to 8
    let periods = 4 * hash22(grid_coord) + 4;

    let shifted_time = time + 100;

    // 6.28 is tau, 0.8 an attempt to avoid clipping at borders
    return vec2f(sin(shifted_time * 6.28 / periods.x), sin(shifted_time * 6.28 / periods.y)) * 0.8;
}

fn main_image(position: vec2f) -> vec4f {
    // -1 to 1
    let uv = (position - wallpaper.resolution * 0.5) / wallpaper.resolution.y;

    let grid_scale = 10.0;
    // -1 to 1
    let grid_uv = 2 * fract(uv * grid_scale) - 1;
    // integer
    let grid_coord = floor(uv * grid_scale);

    let highlight_color = vec3(0.1, 1, 0.1);

    let wave = 0.4 * sin(uv.x * 12.0 + wallpaper.time);
    // return vec4<f32>(0.0, 0.005 / abs(uv.y - wave), 0.0, 1.0);
    // return vec4f(hash22(grid_coord), 0, 1);
    // return vec4f(grid_uv, 0,1);
    return vec4f(highlight_color * smoothstep(0, 1, 0.05/distance(grid_uv, star_uv(grid_coord, wallpaper.time))), 1);
}
