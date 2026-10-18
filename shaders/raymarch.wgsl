// Ray-marched 3D fractals (Mandelbulb, Mandelbox, Menger/KIFS,
// pseudo-Kleinian, quaternion Julia; optionally sphere-inverted) via
// distance estimation.
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
    // Formula parameters (params_a | params_b):
    //   Mandelbulb:      (power, -, -, -)              | (julia seed.xyz, julia on = 1)
    //   Mandelbox:       (scale, min_radius, fixed_radius, -) | (julia seed.xyz, julia on = 1)
    //   KIFS:            (scale, offset.xyz)           | (rot_x, rot_z radians, -, -)
    //   Pseudo-Kleinian: (box_size.xyz, size)          | (thickness, -, -, -)
    //   Quaternion Julia: (c.xyzw)                     | (slice w, -, -, -)
    params_a: vec4<f32>,
    params_b: vec4<f32>,
    inversion: vec4<f32>,   // center.xyz, radius (0 = no inversion)
    // fog_distance (world units, 0 = off), cycle_count,
    // inversion free radius (see Inversion::free_radius), unused
    misc: vec4<f32>,
    background: vec4<f32>,
    // formula_id (0=Mandelbulb,1=Mandelbox,2=KIFS,3=Pseudo-Kleinian,4=Quaternion Julia),
    // max_iterations, max_steps, stop_count
    counts: vec4<u32>,
};

@group(0) @binding(0) var<uniform> u: RayUniforms;
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba8unorm, write>;

const KEY_LIGHT_DIR: vec3<f32> = vec3<f32>(0.48, 0.78, -0.40);

// KIFS per-iteration rotation (rows of Rz * Rx), built once per invocation
// in main() rather than once per DE evaluation.
var<private> kifs_rot0: vec3<f32>;
var<private> kifs_rot1: vec3<f32>;
var<private> kifs_rot2: vec3<f32>;

// Returns (distance estimate, orbit trap = min |z|^2).
fn de_mandelbulb(p: vec3<f32>, power: f32, c: vec3<f32>, dc: f32, max_iterations: u32) -> vec2<f32> {
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
        dr = pow(r, power - 1.0) * power * dr + dc;
        let zr = pow(r, power);
        z = zr * vec3<f32>(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta)) + c;
        trap = min(trap, dot(z, z));
    }
    return vec2<f32>(0.5 * log(max(r, 1e-12)) * r / dr, trap);
}

fn de_mandelbox(p: vec3<f32>, scale: f32, min_radius: f32, fixed_radius: f32, c: vec3<f32>, dc: f32, max_iterations: u32) -> vec2<f32> {
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
        dr = dr * k * abs(scale) + dc;
        z = z * (k * scale) + c;
        trap = min(trap, r2);
        if (dot(z, z) > 1e8) {
            break;
        }
    }
    return vec2<f32>(length(z) / abs(dr), trap);
}

fn unit_box_distance(p: vec3<f32>) -> f32 {
    let q = abs(p) - vec3<f32>(1.0);
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

fn de_kifs(p: vec3<f32>, scale: f32, offset: vec3<f32>, max_iterations: u32) -> vec2<f32> {
    var z = p;
    var stretch = 1.0;
    var trap = 1e20;
    for (var i = 0u; i < max_iterations; i = i + 1u) {
        // Fold into the octant x >= y >= z >= 0.
        z = abs(z);
        if (z.x < z.y) { z = z.yxz; }
        if (z.x < z.z) { z = z.zyx; }
        if (z.y < z.z) { z = z.xzy; }
        z = vec3<f32>(dot(kifs_rot0, z), dot(kifs_rot1, z), dot(kifs_rot2, z));
        z = z * scale - offset * (scale - 1.0);
        // Keep the middle slab (the Menger sponge's face-center hole).
        if (z.z < -0.5 * offset.z * (scale - 1.0)) {
            z.z = z.z + offset.z * (scale - 1.0);
        }
        stretch = stretch * scale;
        trap = min(trap, dot(z, z));
    }
    return vec2<f32>(unit_box_distance(z) / stretch, trap);
}

fn de_pseudo_kleinian(p: vec3<f32>, box_size: vec3<f32>, size: f32, thickness: f32, max_iterations: u32) -> vec2<f32> {
    var z = p;
    var stretch = 1.0;
    var trap = 1e20;
    for (var i = 0u; i < max_iterations; i = i + 1u) {
        let previous = z;
        z = 2.0 * clamp(z, -box_size, box_size) - z;
        let r2 = max(dot(z, z), 1e-30);
        trap = min(trap, r2);
        let k = max(size / r2, 1.0);
        z = z * k;
        stretch = stretch * k;
        // Once a fold changes nothing, every later one won't either.
        if (all(z == previous)) {
            break;
        }
    }
    let rxy = length(z.xy);
    let de = max(rxy - thickness, abs(rxy * z.z) / max(length(z), 1e-30));
    return vec2<f32>(de / stretch, trap);
}

fn de_quaternion_julia(p: vec3<f32>, c: vec4<f32>, slice: f32, max_iterations: u32) -> vec2<f32> {
    // z = (real, i, j, k)
    var z = vec4<f32>(p, slice);
    var dr = 1.0;
    var r2 = dot(z, z);
    var trap = 1e20;
    for (var i = 0u; i < max_iterations; i = i + 1u) {
        // |d(z²)| = 2|z|·|dz| for quaternions, as for complex numbers.
        dr = dr * 2.0 * sqrt(r2);
        // (a + v)² = a² − |v|² + 2a·v
        z = vec4<f32>(z.x * z.x - dot(z.yzw, z.yzw), 2.0 * z.x * z.yzw) + c;
        r2 = dot(z, z);
        trap = min(trap, r2);
        if (r2 > 256.0) {
            break;
        }
    }
    let r = max(sqrt(r2), 1e-30);
    return vec2<f32>(0.5 * r * log(r) / max(dr, 1e-30), trap);
}

// Mandelbulb/Mandelbox `c`: the point itself, or the fixed Julia seed when
// Julia mode is on (mirrors fractal3d::julia_addend). Its derivative with
// respect to the point is 1 or 0 — the `+ dc` of the DE recurrence.
fn julia_on() -> bool {
    return u.params_b.w > 0.5;
}

fn julia_c(p: vec3<f32>) -> vec3<f32> {
    return select(p, u.params_b.xyz, julia_on());
}

fn julia_dc() -> f32 {
    return select(1.0, 0.0, julia_on());
}

fn formula_estimate(p: vec3<f32>) -> vec2<f32> {
    let a = u.params_a;
    let iterations = u.counts.y;
    switch u.counts.x {
        case 0u: { return de_mandelbulb(p, a.x, julia_c(p), julia_dc(), iterations); }
        case 1u: { return de_mandelbox(p, a.x, a.y, a.z, julia_c(p), julia_dc(), iterations); }
        case 2u: { return de_kifs(p, a.x, a.yzw, iterations); }
        case 3u: { return de_pseudo_kleinian(p, a.xyz, a.w, u.params_b.x, iterations); }
        default: { return de_quaternion_julia(p, a, u.params_b.x, iterations); }
    }
}

// The formula's estimate, in sphere-inverted space if an inversion is set
// (mirrors Inversion::estimate).
fn estimate(p: vec3<f32>) -> vec2<f32> {
    let radius = u.inversion.w;
    if (radius <= 0.0) {
        return formula_estimate(p);
    }
    let center = u.inversion.xyz;
    let d = p - center;
    let r2 = max(dot(d, d), 1e-30);
    let r = sqrt(r2);
    let rr = radius * radius;
    let e = formula_estimate(center + d * (rr / r2));
    let de = max(r2 * e.x / (rr + r * abs(e.x)), u.misc.z - r);
    return vec2<f32>(de, e.y);
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

    // Rz(b) * Rx(a), as rows (mirrors fractal3d::kifs_rotation).
    let sa = sin(u.params_b.x);
    let ca = cos(u.params_b.x);
    let sb = sin(u.params_b.y);
    let cb = cos(u.params_b.y);
    kifs_rot0 = vec3<f32>(cb, -sb * ca, sb * sa);
    kifs_rot1 = vec3<f32>(sb, cb * ca, -cb * sa);
    kifs_rot2 = vec3<f32>(0.0, sa, ca);

    let ro = u.cam_pos.xyz;
    let tan_half_fov = u.cam_pos.w;
    let detail = u.cam_right.w;
    let max_distance = u.cam_up.w;
    let step_factor = u.cam_forward.w;
    let fog_distance = u.misc.x;
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
        // Mirrored lookup (0 → 1 → 0 ...): a plain wrap would jump from the
        // palette's last color straight back to its first, drawing hard,
        // aliased seams across smooth surfaces.
        let color_t = log2(1.0 + trap) * u.misc.y * 0.25;
        let mirrored = 1.0 - abs(1.0 - 2.0 * fract(color_t * 0.5));
        let base = sample_palette(min(mirrored, 0.9999), u.counts.w).rgb;
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
