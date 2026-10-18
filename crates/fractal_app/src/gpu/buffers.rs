use bytemuck::{Pod, Zeroable};
use fractal_core::fractal3d::{Formula3D, Params3D};
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
    /// (distance_shading_enabled, unused, unused, unused)
    pub flags: [u32; 4],
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
            flags: [u32::from(params.palette.distance_shading), 0, 0, 0],
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

/// Must stay byte-layout-compatible with the `RayUniforms` struct in
/// `shaders/raymarch.wgsl`. All vec4s, like `Uniforms`.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct RayUniforms {
    /// (position.xyz, tan(fov_y / 2))
    pub cam_pos: [f32; 4],
    /// (right.xyz, detail in pixels)
    pub cam_right: [f32; 4],
    /// (up.xyz, max ray distance)
    pub cam_up: [f32; 4],
    /// (forward.xyz, step factor)
    pub cam_forward: [f32; 4],
    /// Formula parameters; the per-formula layout is documented on the
    /// shader's `RayUniforms`.
    pub params_a: [f32; 4],
    pub params_b: [f32; 4],
    /// (center.xyz, radius); radius 0 = no inversion.
    pub inversion: [f32; 4],
    /// (fog distance in world units — 0 = off, cycle_count, inversion free
    /// radius, unused)
    pub misc: [f32; 4],
    pub background: [f32; 4],
    /// (formula_id, max_iterations, max_steps, stop_count)
    pub counts: [u32; 4],
}

impl RayUniforms {
    pub fn from_params(params: &Params3D) -> Self {
        let cam = &params.camera;
        let tan_half_fov = (cam.fov_y_degrees.to_radians() * 0.5).tan();
        let vec = |v: [f64; 3], w: f64| [v[0] as f32, v[1] as f32, v[2] as f32, w as f32];
        // `fog_distance` is relative to the camera's clearance (see
        // `Params3D::fog_distance`); the shader wants world units.
        let fog = if params.fog_distance > 0.0 { params.fog_distance * params.camera_clearance().max(1e-9) } else { 0.0 };
        let (params_a, params_b) = match params.formula {
            Formula3D::Mandelbulb { power, julia } => ([power, 0.0, 0.0, 0.0], julia_params(julia)),
            Formula3D::Mandelbox { scale, min_radius, fixed_radius, julia } => {
                ([scale, min_radius, fixed_radius, 0.0], julia_params(julia))
            }
            Formula3D::Kifs { scale, offset, rotation_degrees } => (
                [scale, offset[0], offset[1], offset[2]],
                [rotation_degrees[0].to_radians(), rotation_degrees[1].to_radians(), 0.0, 0.0],
            ),
            Formula3D::PseudoKleinian { box_size, size, thickness } => {
                ([box_size[0], box_size[1], box_size[2], size], [thickness, 0.0, 0.0, 0.0])
            }
            Formula3D::QuaternionJulia { c, slice } => (c, [slice, 0.0, 0.0, 0.0]),
        };
        let (inversion, free_radius) = match params.inversion {
            Some(inv) => (vec(inv.center, inv.radius), inv.free_radius(params.formula)),
            None => ([0.0; 4], 0.0),
        };
        let (_, _, stop_count) = params.palette.to_gpu_stops();
        let bg = params.palette.interior_color;
        Self {
            cam_pos: vec(cam.position, tan_half_fov),
            cam_right: vec(cam.right(), params.detail),
            cam_up: vec(cam.up(), params.max_distance()),
            cam_forward: vec(cam.forward(), params.step_factor),
            params_a: params_a.map(|x| x as f32),
            params_b: params_b.map(|x| x as f32),
            inversion,
            misc: [fog as f32, params.palette.cycle_count.max(0.0001), free_radius as f32, 0.0],
            background: [bg[0] as f32 / 255.0, bg[1] as f32 / 255.0, bg[2] as f32 / 255.0, 1.0],
            counts: [params.formula.shader_id(), params.max_iterations, params.max_steps, stop_count],
        }
    }
}

/// `params_b` for the Mandelbulb/Mandelbox: the Julia seed and an on-flag.
fn julia_params(julia: Option<[f64; 3]>) -> [f64; 4] {
    match julia {
        Some([x, y, z]) => [x, y, z, 1.0],
        None => [0.0; 4],
    }
}
