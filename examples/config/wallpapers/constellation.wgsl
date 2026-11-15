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

    // 6.28 is tau, 1.2 to allow more flexibility
    return vec2f(sin(shifted_time * 6.28 / periods.x), sin(shifted_time * 6.28 / periods.y)) * 1.2;
}

fn main_image(position: vec2f) -> vec4f {
    // -1 to 1
    let uv = (position - wallpaper.resolution * 0.5) / wallpaper.resolution.y;

    let grid_scale = 5.0;
    let drift_vel = vec2f(0, -0.5);
    let pos = grid_scale * uv - drift_vel * wallpaper.time;
    // -1 to 1
    let grid_uv = 2 * fract(pos) - 1;
    // integer
    let grid_coord = floor(pos);

    let highlight_color = vec3(0.4, 1, 0.4);

    var intensity = 0.;

    // add this cell's and neighboring stars
    for (var neighbor_x = -1; neighbor_x <= 1; neighbor_x++) {
        for (var neighbor_y = -1; neighbor_y <= 1; neighbor_y++) {
            let neighbor_offset = vec2f(vec2i(neighbor_x, neighbor_y));
            let neighbor_coord = grid_coord + neighbor_offset;

            // relative to our grid cell's uv
            // Need to + 2*neighbor offset because cells are 2 apart 
            let star_pos = star_uv(neighbor_coord, wallpaper.time) + 2 * neighbor_offset;

            let dist_to_star = distance(grid_uv, star_pos);

            // reciprocal is good for making it look glowy
            intensity += smoothstep(0, 1, 0.05/dist_to_star);
        }
    }

    // add lines
    // for 

    return vec4f(highlight_color * intensity, 1);
}
