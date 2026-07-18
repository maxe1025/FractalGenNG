use bytemuck::{Pod, Zeroable};
use fractal_core::palette::MAX_GPU_STOPS;
use fractal_core::FractalParams;

/// Must stay byte-layout-compatible with the `Uniforms` struct in
/// `shaders/standard.wgsl`. Every field is a plain vec4 so there is no
/// sub-16-byte packing for WGSL's uniform-address-space rules to trip over.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Uniforms {
    /// (center.x, center.y, scale.x, scale.y)
    pub center_scale: [f32; 4],
    /// (julia_seed.x, julia_seed.y, bailout_sq, cycle_count)
    pub seed_bailout_cycle: [f32; 4],
    /// (max_iterations, fractal_kind, stop_count, multibrot_power — only
    /// meaningful when fractal_kind == 4 (Multibrot), else 0)
    pub iter_kind_stops: [u32; 4],
    pub interior_color: [f32; 4],
}

impl Uniforms {
    pub fn from_params(params: &FractalParams) -> Self {
        let half_extent = (2.0 / params.zoom) as f32;
        let seed = params.kind.julia_seed();
        let (_, _, stop_count) = params.palette.to_gpu_stops();
        let interior = params.palette.interior_color;
        let (center_x, center_y) = params.center.to_f64_pair();
        let power = match params.kind {
            fractal_core::FractalKind::Multibrot { power } => power.round().max(2.0) as u32,
            _ => 0,
        };
        Self {
            center_scale: [center_x as f32, center_y as f32, half_extent, half_extent],
            seed_bailout_cycle: [
                seed[0] as f32,
                seed[1] as f32,
                (params.bailout_radius * params.bailout_radius) as f32,
                params.palette.cycle_count.max(0.0001),
            ],
            iter_kind_stops: [params.max_iterations, params.kind.shader_id(), stop_count, power],
            interior_color: [
                interior[0] as f32 / 255.0,
                interior[1] as f32 / 255.0,
                interior[2] as f32 / 255.0,
                interior[3] as f32 / 255.0,
            ],
        }
    }
}

/// Palette gradient stops padded to `MAX_GPU_STOPS`, ready for upload to the
/// storage buffers `standard.wgsl` reads from.
pub fn gpu_stop_arrays(params: &FractalParams) -> ([f32; MAX_GPU_STOPS], [[f32; 4]; MAX_GPU_STOPS]) {
    let (positions, colors, _) = params.palette.to_gpu_stops();
    (positions, colors)
}

/// Must stay byte-layout-compatible with the `PerturbationUniforms` struct in
/// `shaders/perturbation.wgsl`. Every field is a `[f64;2]`/`[u32;4]`/`[f32;4]`
/// pair/quad so it maps directly to WGSL `vec2<f64>`/`vec4<u32>`/`vec4<f32>`
/// with no manual padding.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct PerturbationUniforms {
    /// (scale.x, scale.y)
    pub half_extent: [f64; 2],
    /// (bailout_sq, cycle_count)
    pub bailout_cycle: [f64; 2],
    /// (max_iterations, fractal_kind, stop_count, reference_orbit_len)
    pub iter_kind_stops_reflen: [u32; 4],
    pub interior_color: [f32; 4],
    /// (is_correction_pass, unused, unused, unused)
    pub correction: [u32; 4],
}

impl PerturbationUniforms {
    pub fn from_params(params: &FractalParams, ref_orbit_len: u32, is_correction_pass: bool) -> Self {
        let half_extent = 2.0 / params.zoom;
        let (_, _, stop_count) = params.palette.to_gpu_stops();
        let interior = params.palette.interior_color;
        Self {
            half_extent: [half_extent, half_extent],
            bailout_cycle: [
                params.bailout_radius * params.bailout_radius,
                f64::from(params.palette.cycle_count.max(0.0001)),
            ],
            iter_kind_stops_reflen: [
                params.max_iterations,
                params.kind.shader_id(),
                stop_count,
                ref_orbit_len,
            ],
            interior_color: [
                interior[0] as f32 / 255.0,
                interior[1] as f32 / 255.0,
                interior[2] as f32 / 255.0,
                interior[3] as f32 / 255.0,
            ],
            correction: [u32::from(is_correction_pass), 0, 0, 0],
        }
    }
}
