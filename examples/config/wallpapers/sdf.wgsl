struct Wallpaper {
    resolution: vec2f,
    time: f32,
    frame: u32,
    mouse: vec4f,
}
@group(0) @binding(0) var<uniform> wallpaper: Wallpaper;

fn min_w_material(a: vec2f, b: vec2f) -> vec2f {
    if a.x < b.x {
        return a;
    } else {
        return b;
    }
}

// ==========================================
// 1. YOUR SDF SCENE GOES HERE (REST IS BOILERPLATE FROM AI)
// ==========================================
fn map(p: vec3<f32>) -> vec2f {
    // Example: A sphere of radius 1.0 at the origin
    let sphere_dist = length(p) - 1.0;

    // Example: A ground plane at y = -1.0
    let plane_dist = p.y + 4.0;

    return min_w_material(vec2f(sphere_dist, 1), vec2f(plane_dist, 2));
}

// Assign colors based on Material ID
fn get_material(p: vec3<f32>, id: f32) -> vec3<f32> {
    if (id == 1.0) {
        return vec3<f32>(0.85, 0.25, 0.2); // Red sphere
    } else if (id == 2.0) {
        // Procedural checkerboard pattern for the ground
        let check = floor(p.x) + floor(p.z);
        if (abs(check % 2.0) < 0.5) {
            return vec3<f32>(0.2, 0.22, 0.25);
        }
        return vec3<f32>(0.45, 0.48, 0.52);
    }
    return vec3<f32>(1.0);
}

// ==========================================
// 2. RAYMARCHING, SHADOWS & MATERIALS
// ==========================================
fn calc_normal(p: vec3<f32>) -> vec3<f32> {
    let e = vec2<f32>(0.001, 0.0);
    return normalize(vec3<f32>(
        map(p + e.xyy).x - map(p - e.xyy).x,
        map(p + e.yxy).x - map(p - e.yxy).x,
        map(p + e.yyx).x - map(p - e.yyx).x
    ));
}

// Returns vec2f(hit_distance, material_id)
fn raymarch(ro: vec3<f32>, rd: vec3<f32>) -> vec2<f32> {
    var t = 0.0;
    for (var i = 0; i < 200; i++) {
        let p = ro + rd * t;
        let res = map(p);
        if (res.x < 0.001) { return vec2<f32>(t, res.y); }
        t += res.x;
        if (t > 1000.0) { break; }
    }
    return vec2<f32>(-1.0, 0.0); // Missed
}

// Soft shadow march from hit point toward the light
// k controls shadow sharpness (e.g., 8.0 = soft, 32.0 = sharp)
fn calc_shadow(ro: vec3<f32>, rd: vec3<f32>, k: f32) -> f32 {
    var res = 1.0;
    var t = 0.02; // Start slightly off the surface to prevent self-shadowing
    for (var i = 0; i < 64; i++) {
        let h = map(ro + rd * t).x;
        if (h < 0.001) { return 0.0; } // Full shadow
        res = min(res, k * h / t);
        t += h;
        if (t > 50.0) { break; }
    }
    return clamp(res, 0.0, 1.0);
}

// ==========================================
// 3. FRAGMENT SHADER
// ==========================================
fn main_image(position: vec2f) -> vec4f {
    let uv = (position - wallpaper.resolution * 0.5) / wallpaper.resolution.y;

    // Camera setup (Pulled closer so the radius-1 sphere is clearly visible)
    let ro = vec3<f32>(0.0, 1.0, 20.0);
    let rd = normalize(vec3<f32>(uv, -1.5));

    // Background color (sky)
    var col = vec3<f32>(0.1, 0.15, 0.2);

    // Run Raymarcher
    let hit = raymarch(ro, rd);
    let t = hit.x;
    let mat_id = hit.y;

    if (t > 0.0) {
        let p = ro + rd * t;
        let n = calc_normal(p);
        let base_color = get_material(p, mat_id);

        // Lighting & Shadows
        let light_dir = normalize(vec3<f32>(0.8, 1.0, 0.6));
        let diff = max(dot(n, light_dir), 0.0);

        // Offset origin slightly along the normal to avoid shadow acne
        let shadow = calc_shadow(p + n * 0.002, light_dir, 12.0);
        let ambient = 0.15;

        // Multiply diffuse light by shadow, but keep ambient light so shadows aren't pitch black
        col = base_color * (diff * shadow + ambient);
    }

    // Basic gamma correction
    col = pow(col, vec3<f32>(1.0 / 2.2));
    return vec4<f32>(col, 1.0);
}
