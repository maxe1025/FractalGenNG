// Deep-zoom escape-time renderer via perturbation theory. One arbitrary-
// precision reference orbit (computed CPU-side, see fractal_core::orbit) is
// uploaded as `ref_orbit`; each invocation iterates a small per-pixel *delta*
// against it in native f64 (requires wgpu::Features::SHADER_F64). The delta
// stays small by construction, so f64 remains accurate far beyond what f64
// could resolve for an absolute coordinate at this zoom depth.
//
// Recurrence (matching fractal_core::orbit's conventions):
//  - Mandelbrot (z_0=0, c=pixel): delta_{n+1} = 2*Z_ref_n*delta_n + delta_n^2 + delta_c,
//    delta_0 = 0.
//  - Julia (z_0=pixel, c=seed):   delta_{n+1} = 2*Z_ref_n*delta_n + delta_n^2,
//    delta_0 = pixel_offset (injected once).
//
// Glitch rebasing: perturbation theory assumes the pixel's true orbit stays
// close to the reference orbit at every step. For some pixels it doesn't —
// the true value can end up close to the reference's *starting* point
// instead of its current position, which would make continuing to track the
// current reference index numerically unstable ("glitchy": visibly wrong,
// usually black or noisy speckling). Each iteration, if the true value's
// magnitude is smaller than the current delta's own magnitude, that's a
// sign the perturbation has drifted from a good linearization point — we
// "rebase": treat the true value as a fresh delta relative to the
// reference's start (`ref_orbit[0]`) and restart the reference index at 0,
// while the true escape-time iteration count keeps counting normally (it
// reflects the real orbit, independent of which reference index we're
// borrowing `Z_ref` from this step).
//
// Secondary-reference correction: rebasing alone doesn't fix every pixel —
// some pixels exhaust the entire reference orbit (`ref_index` reaches its
// length) without escaping *or* reaching `max_iterations`. That's the
// signature of a residual glitch: this pixel's true orbit needed more
// reference points than a reference anchored elsewhere could provide. Such
// pixels are flagged in `glitch_flags`; the CPU (see `gpu/renderer.rs`)
// reads that buffer back, computes a new reference orbit anchored near the
// glitched region, and re-dispatches this same shader as a "correction
// pass" (`u.correction.x == 1`) that only recomputes flagged pixels,
// leaving everything else's color from the previous pass untouched.
//
// shaders/palette.wgsl is concatenated before this file at shader-module
// creation time and provides the `stop_positions`/`stop_colors` bindings
// (1/2) and the `sample_palette` function.

struct PerturbationUniforms {
    half_extent: vec2<f64>,            // scale.x, scale.y
    bailout_cycle: vec2<f64>,          // bailout_sq, cycle_count
    iter_kind_stops_reflen: vec4<u32>, // max_iterations, fractal_kind (0=Mandelbrot,1=Julia), stop_count, ref_orbit_len
    interior_color: vec4<f32>,
    correction: vec4<u32>,             // is_correction_pass, unused, unused, unused
};

@group(0) @binding(0) var<uniform> u: PerturbationUniforms;
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(4) var<storage, read> ref_orbit: array<vec2<f64>>;
@group(0) @binding(5) var<storage, read_write> glitch_flags: array<u32>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = textureDimensions(output_tex);
    if (gid.x >= dims.x || gid.y >= dims.y) {
        return;
    }
    let pixel_index = gid.y * dims.x + gid.x;
    if (u.correction.x == 1u && glitch_flags[pixel_index] == 0u) {
        // Not flagged glitched by the previous pass — leave its color alone.
        return;
    }

    let width = f64(dims.x);
    let height = f64(dims.y);
    let aspect = width / height;

    let max_iterations = u.iter_kind_stops_reflen.x;
    let fractal_kind = u.iter_kind_stops_reflen.y;
    let stop_count = u.iter_kind_stops_reflen.z;
    let ref_len = u.iter_kind_stops_reflen.w;
    let bailout_sq = u.bailout_cycle.x;
    let cycle_count = f32(u.bailout_cycle.y);

    // Screen space -> small f64 offset from the reference orbit's anchor point.
    let px = (f64(gid.x) + 0.5) / width * 2.0 - 1.0;
    let py = (f64(gid.y) + 0.5) / height * 2.0 - 1.0;
    let offset_re = px * u.half_extent.x * aspect;
    let offset_im = -py * u.half_extent.y;

    var delta_re: f64 = 0.0;
    var delta_im: f64 = 0.0;
    if (fractal_kind == 1u) {
        // Julia: the perturbation is injected once, at n=0.
        delta_re = offset_re;
        delta_im = offset_im;
    }

    var true_iter: u32 = 0u;  // real escape-time iteration count, for coloring
    var ref_index: u32 = 0u;  // position within ref_orbit — can reset on rebase
    var true_mag_sq: f64 = delta_re * delta_re + delta_im * delta_im;
    var glitched: bool = false;
    let ref0 = ref_orbit[0];

    loop {
        if (true_iter >= max_iterations) {
            break;
        }
        if (ref_index >= ref_len) {
            // Reference orbit exhausted before this pixel escaped or reached
            // max_iterations — this reference can't resolve it.
            glitched = true;
            break;
        }
        var zr = ref_orbit[ref_index];
        let true_re = zr.x + delta_re;
        let true_im = zr.y + delta_im;
        true_mag_sq = true_re * true_re + true_im * true_im;
        if (true_mag_sq > bailout_sq) {
            break;
        }

        let delta_mag_sq = delta_re * delta_re + delta_im * delta_im;
        if (true_mag_sq < delta_mag_sq) {
            // Rebase: the true value is a better delta relative to the
            // reference's start than continuing from the current index.
            // Fall straight through into the step below using the rebased
            // (delta, ref_index) — never just re-check the same true value
            // again next iteration, or a pixel that keeps satisfying the
            // rebase condition could get stuck re-triggering it forever
            // without ever actually advancing.
            delta_re = true_re - ref0.x;
            delta_im = true_im - ref0.y;
            ref_index = 0u;
            zr = ref0;
        }

        // delta_{n+1} = 2*Z_ref_n*delta_n + delta_n^2 [+ delta_c for Mandelbrot]
        let two_zr_re = 2.0 * zr.x;
        let two_zr_im = 2.0 * zr.y;
        let term1_re = two_zr_re * delta_re - two_zr_im * delta_im;
        let term1_im = two_zr_re * delta_im + two_zr_im * delta_re;
        let term2_re = delta_re * delta_re - delta_im * delta_im;
        let term2_im = 2.0 * delta_re * delta_im;

        var new_delta_re = term1_re + term2_re;
        var new_delta_im = term1_im + term2_im;
        if (fractal_kind == 0u) {
            new_delta_re = new_delta_re + offset_re;
            new_delta_im = new_delta_im + offset_im;
        }
        delta_re = new_delta_re;
        delta_im = new_delta_im;
        ref_index = ref_index + 1u;
        true_iter = true_iter + 1u;
    }

    var color: vec4<f32>;
    if (true_iter >= max_iterations) {
        color = u.interior_color;
    } else {
        let log_zn = log(f32(true_mag_sq)) * 0.5;
        let nu = log(log_zn / log(2.0)) / log(2.0);
        let smooth_iter = f32(true_iter) + 1.0 - nu;
        let t = smooth_iter / cycle_count;
        color = sample_palette(t, stop_count);
    }

    glitch_flags[pixel_index] = select(0u, 1u, glitched);
    textureStore(output_tex, vec2<i32>(i32(gid.x), i32(gid.y)), color);
}
