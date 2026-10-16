// Ray-marched 3D fractals (Mandelbulb, Mandelbox) via distance estimation.
// One compute invocation per output pixel: march the pixel's ray by the
// distance estimate (DE) until it is within one pixel footprint of the
// surface, then shade the hit with a headlight, a key light and ambient
// occlusion, colored by orbit trap.
//
// The DE functions mirror crates/fractal_core/src/fractal3d.rs — keep them
// in sync. shaders/palette.wgsl is concatenated before this file and
// provides bindings 1/2 and `sample_palette`.

struct RayUniforms {
    cam_pos: vec4<f32>,     // xyz, tan(fov_y / 2)
    cam_right: vec4<f32>,   // xyz, detail (pixels)
    cam_up: vec4<f32>,      // xyz, max ray distance
    cam_forward: vec4<f32>, // xyz, step factor
    // Mandelbulb: (power, -, -, fog_distance)
    // Mandelbox:  (scale, min_radius, fixed_radius, fog_distance)
    // fog_distance is in world units; 0 disables fog.
    formula: vec4<f32>,
    background: vec4<f32>,
    counts: vec4<u32>,      // formula_id (0=Mandelbulb,1=Mandelbox), max_iterations, max_steps, stop_count
    coloring: vec4<f32>,    // cycle_count, unused, unused, unused
};

@group(0) @binding(0) var<uniform> u: RayUniforms;
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba8unorm, write>;

const KEY_LIGHT_DIR: vec3<f32> = vec3<f32>(0.48, 0.78, -0.40);

// Returns (distance estimate, orbit trap = min |z|^2).
fn de_mandelbulb(p: vec3<f32>, power: f32, max_iterations: u32) -> vec2<f32> {
    var z = p;
    var dr = 1.0;
    var r = length(z);
    var trap = 1e20;
    for (var i = 0u; i < max_iterations; i = i + 1u) {
        r = length(z);
        if (r > 2.0 || r < 1e-12) {
            break;
        }
        let theta = acos(clamp(z.z / r, -1.0, 1.0)) * power;
        let phi = atan2(z.y, z.x) * power;
        dr = pow(r, power - 1.0) * power * dr + 1.0;
        let zr = pow(r, power);
        z = zr * vec3<f32>(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta)) + p;
        trap = min(trap, dot(z, z));
    }
    return vec2<f32>(0.5 * log(max(r, 1e-12)) * r / dr, trap);
}

fn de_mandelbox(p: vec3<f32>, scale: f32, min_radius: f32, fixed_radius: f32, max_iterations: u32) -> vec2<f32> {
    let min_r2 = min_radius * min_radius;
    let fixed_r2 = fixed_radius * fixed_radius;
    var z = p;
    var dr = 1.0;
    var trap = 1e20;
    for (var i = 0u; i < max_iterations; i = i + 1u) {
        // Box fold: reflect each axis back into [-1, 1].
        z = clamp(z, vec3<f32>(-1.0), vec3<f32>(1.0)) * 2.0 - z;
        // Sphere fold: invert the shell between min and fixed radius.
        let r2 = dot(z, z);
        var k = 1.0;
        if (r2 < min_r2) {
            k = fixed_r2 / min_r2;
        } else if (r2 < fixed_r2) {
            k = fixed_r2 / r2;
        }
        dr = dr * k * abs(scale) + 1.0;
        z = z * (k * scale) + p;
        trap = min(trap, r2);
        if (dot(z, z) > 1e8) {
            break;
        }
    }
    return vec2<f32>(length(z) / abs(dr), trap);
}

fn estimate(p: vec3<f32>) -> vec2<f32> {
    if (u.counts.x == 0u) {
        return de_mandelbulb(p, u.formula.x, u.counts.y);
    }
    return de_mandelbox(p, u.formula.x, u.formula.y, u.formula.z, u.counts.y);
}

fn de(p: vec3<f32>) -> f32 {
    return estimate(p).x;
}

// Tetrahedral gradient of the DE: 4 samples instead of 6.
fn normal_at(p: vec3<f32>, h: f32) -> vec3<f32> {
    let k = vec2<f32>(1.0, -1.0);
    return normalize(
        k.xyy * de(p + k.xyy * h) +
        k.yyx * de(p + k.yyx * h) +
        k.yxy * de(p + k.yxy * h) +
        k.xxx * de(p + k.xxx * h)
    );
}

// How much free space surrounds `p` along its normal, sampled at 5 growing
// distances: 1 = fully open, 0 = buried in a crevice.
fn ambient_occlusion(p: vec3<f32>, n: vec3<f32>, scale: f32) -> f32 {
    var occlusion = 0.0;
    var weight = 1.0;
    for (var i = 1u; i <= 5u; i = i + 1u) {
        let h = scale * f32(i);
        occlusion = occlusion + weight * clamp((h - de(p + n * h)) / h, 0.0, 1.0);
        weight = weight * 0.6;
    }
    return clamp(1.0 - occlusion * 0.25, 0.0, 1.0);
}

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(output_tex);
    if (gid.x >= dims.x || gid.y >= dims.y) {
        return;
    }
    let width = f32(dims.x);
    let height = f32(dims.y);
    let aspect = width / height;

    let ro = u.cam_pos.xyz;
    let tan_half_fov = u.cam_pos.w;
    let detail = u.cam_right.w;
    let max_distance = u.cam_up.w;
    let step_factor = u.cam_forward.w;
    let fog_distance = u.formula.w;
    let max_steps = u.counts.z;

    let px = (f32(gid.x) + 0.5) / width * 2.0 - 1.0;
    let py = (f32(gid.y) + 0.5) / height * 2.0 - 1.0;
    let dir = normalize(
        u.cam_forward.xyz + u.cam_right.xyz * (px * tan_half_fov * aspect) - u.cam_up.xyz * (py * tan_half_fov)
    );
    // Angular size of one pixel: a ray at distance t covers t * pixel_angle.
    let pixel_angle = 2.0 * tan_half_fov / height;

    var t = 0.0;
    var hit = false;
    var trap = 0.0;
    var eps = 0.0;
    for (var i = 0u; i < max_steps; i = i + 1u) {
        let e = estimate(ro + dir * t);
        eps = max(detail * pixel_angle * t, 1e-6);
        if (e.x < eps) {
            hit = true;
            trap = e.y;
            break;
        }
        t = t + e.x * step_factor;
        if (t > max_distance) {
            break;
        }
    }

    var color = u.background.rgb;
    if (hit) {
        let p = ro + dir * t;
        let n = normal_at(p, max(eps * 2.0, 1e-6));
        let base = sample_palette(log2(1.0 + trap) * u.coloring.x * 0.25, u.counts.w).rgb;
        let headlight = max(dot(n, -dir), 0.0);
        let key = max(dot(n, normalize(KEY_LIGHT_DIR)), 0.0);
        let ao = ambient_occlusion(p, n, max(t * 0.015, eps * 2.0));
        color = base * (0.2 + 0.9 * headlight + 0.3 * key) * ao;
        if (fog_distance > 0.0) {
            let fog = 1.0 - exp(-t / fog_distance);
            color = mix(color, u.background.rgb, fog);
        }
    }

    textureStore(output_tex, vec2<i32>(i32(gid.x), i32(gid.y)), vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0));
}
