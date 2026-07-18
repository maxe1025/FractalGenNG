// Standard-precision (f32) escape-time renderer for Mandelbrot and Julia sets.
// One compute invocation per output pixel. Valid for zoom levels well within
// f32 precision range; deep zoom uses perturbation.wgsl instead.
//
// shaders/palette.wgsl is concatenated before this file at shader-module
// creation time and provides the `stop_positions`/`stop_colors` bindings
// (1/2) and the `sample_palette` function.

struct Uniforms {
    center_scale: vec4<f32>,       // center.x, center.y, scale.x, scale.y
    seed_bailout_cycle: vec4<f32>, // julia_seed.x, julia_seed.y, bailout_sq, cycle_count
    iter_kind_stops: vec4<u32>,    // max_iterations, fractal_kind (0=Mandelbrot,1=Julia), stop_count, unused
    interior_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(output_tex);
    if (gid.x >= dims.x || gid.y >= dims.y) {
        return;
    }

    let width = f32(dims.x);
    let height = f32(dims.y);
    let aspect = width / height;

    // Screen space -> complex plane, centered at u.center_scale.xy, +y is up.
    let center = u.center_scale.xy;
    let scale = u.center_scale.zw;
    let max_iterations = u.iter_kind_stops.x;
    let fractal_kind = u.iter_kind_stops.y;
    let julia_seed = u.seed_bailout_cycle.xy;
    let bailout_sq = u.seed_bailout_cycle.z;
    let cycle_count = u.seed_bailout_cycle.w;

    let px = (f32(gid.x) + 0.5) / width * 2.0 - 1.0;
    let py = (f32(gid.y) + 0.5) / height * 2.0 - 1.0;
    let cx = center.x + px * scale.x * aspect;
    let cy = center.y - py * scale.y;

    var zx: f32;
    var zy: f32;
    var cxi: f32;
    var cyi: f32;
    if (fractal_kind == 0u) {
        // Mandelbrot: z_0 = 0, c = pixel coordinate.
        zx = 0.0;
        zy = 0.0;
        cxi = cx;
        cyi = cy;
    } else {
        // Julia: z_0 = pixel coordinate, c = fixed seed.
        zx = cx;
        zy = cy;
        cxi = julia_seed.x;
        cyi = julia_seed.y;
    }

    var iter: u32 = 0u;
    var mag_sq: f32 = zx * zx + zy * zy;
    loop {
        if (iter >= max_iterations || mag_sq > bailout_sq) {
            break;
        }
        let new_zx = zx * zx - zy * zy + cxi;
        let new_zy = 2.0 * zx * zy + cyi;
        zx = new_zx;
        zy = new_zy;
        mag_sq = zx * zx + zy * zy;
        iter = iter + 1u;
    }

    var color: vec4<f32>;
    if (iter >= max_iterations) {
        color = u.interior_color;
    } else {
        // Smooth (continuous) iteration count for band-free coloring.
        let log_zn = log(mag_sq) * 0.5;
        let nu = log(log_zn / log(2.0)) / log(2.0);
        let smooth_iter = f32(iter) + 1.0 - nu;
        let t = smooth_iter / cycle_count;
        color = sample_palette(t, u.iter_kind_stops.z);
    }

    textureStore(output_tex, vec2<i32>(i32(gid.x), i32(gid.y)), color);
}
