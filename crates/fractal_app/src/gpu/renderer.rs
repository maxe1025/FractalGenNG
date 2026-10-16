use bytemuck::cast_slice;
use fractal_core::fractal3d::Params3D;
use fractal_core::hp::ZOOM_PERTURBATION_THRESHOLD;
use fractal_core::palette::MAX_GPU_STOPS;
use fractal_core::{orbit, FractalKind, FractalParams, HpComplex};
use rayon::prelude::*;
use wgpu::util::DeviceExt;

use super::buffers::{gpu_stop_arrays, PerturbationUniforms, RayUniforms, Uniforms};

const STANDARD_SRC: &str = concat!(
    include_str!("../../../../shaders/palette.wgsl"),
    include_str!("../../../../shaders/standard.wgsl"),
);
const PERTURBATION_SRC: &str = concat!(
    include_str!("../../../../shaders/palette.wgsl"),
    include_str!("../../../../shaders/perturbation.wgsl"),
);
const RAYMARCH_SRC: &str = concat!(
    include_str!("../../../../shaders/palette.wgsl"),
    include_str!("../../../../shaders/raymarch.wgsl"),
);
const WORKGROUP_SIZE: u32 = 8;
/// Bounded retry count for secondary reference orbits — if residual glitches
/// still remain after this many additional references, we give up and leave
/// whatever the last pass produced (a graceful, if imperfect, degradation).
const MAX_GLITCH_CORRECTION_PASSES: u32 = 4;

/// A compute pipeline with the basic binding set — uniforms (0), palette
/// stops (1, 2), output texture (3) — shared by the standard 2D shader and
/// the 3D ray-march shader.
struct BasicPipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

struct PerturbationPipeline {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    orbit_buffer: wgpu::Buffer,
    orbit_len: u32,
    /// One `u32` per pixel (0/1), written by the shader every pass, read back
    /// on the CPU after the primary dispatch to drive secondary-reference
    /// glitch correction.
    glitch_buffer: wgpu::Buffer,
}

/// The inputs that determine a reference orbit. Panning/zooming that doesn't
/// change the anchor point (or the other fields here) can reuse the last
/// computed orbit instead of recomputing an arbitrary-precision recurrence
/// every frame.
#[derive(Clone, PartialEq)]
struct OrbitKey {
    center: HpComplex,
    kind: FractalKind,
    max_iterations: u32,
    bailout_radius: f64,
}

/// Owns the compute pipelines (standard f32 2D, deep-zoom perturbation 2D,
/// 3D ray-march) and the
/// live viewport's output texture (registered with egui so it can be
/// displayed via `ui.image`). Also used to render arbitrary off-screen
/// resolutions for still-image and animation-frame export via
/// `render_to_rgba`, which picks the same pipeline `render` would for the
/// frame's zoom level.
pub struct FractalRenderer {
    width: u32,
    height: u32,
    output_view: wgpu::TextureView,
    stop_pos_buffer: wgpu::Buffer,
    stop_color_buffer: wgpu::Buffer,
    texture_id: egui::TextureId,
    /// Whether the active GPU/driver granted `wgpu::Features::SHADER_F64`,
    /// needed for the perturbation-theory deep-zoom shader's delta iteration.
    supports_f64: bool,
    standard: BasicPipeline,
    raymarch: BasicPipeline,
    /// `None` when `!supports_f64` — deep zoom simply isn't available.
    perturbation: Option<PerturbationPipeline>,
    orbit_cache: Option<(OrbitKey, Vec<[f64; 2]>)>,
}

impl FractalRenderer {
    pub fn new(
        device: &wgpu::Device,
        egui_renderer: &mut egui_wgpu::Renderer,
        width: u32,
        height: u32,
    ) -> Self {
        let supports_f64 = device.features().contains(wgpu::Features::SHADER_F64);

        let stop_pos_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stop_pos_buffer"),
            size: (MAX_GPU_STOPS * std::mem::size_of::<f32>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let stop_color_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("stop_color_buffer"),
            size: (MAX_GPU_STOPS * std::mem::size_of::<[f32; 4]>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let (_texture, output_view) = create_output_texture(device, width, height);

        let standard = create_basic_pipeline(
            device,
            "standard",
            STANDARD_SRC,
            std::mem::size_of::<Uniforms>() as u64,
            &stop_pos_buffer,
            &stop_color_buffer,
            &output_view,
        );
        let raymarch = create_basic_pipeline(
            device,
            "raymarch",
            RAYMARCH_SRC,
            std::mem::size_of::<RayUniforms>() as u64,
            &stop_pos_buffer,
            &stop_color_buffer,
            &output_view,
        );
        let perturbation = supports_f64.then(|| {
            create_perturbation_pipeline(device, &stop_pos_buffer, &stop_color_buffer, &output_view, width, height, 1)
        });

        let texture_id =
            egui_renderer.register_native_texture(device, &output_view, wgpu::FilterMode::Nearest);

        Self {
            width,
            height,
            output_view,
            stop_pos_buffer,
            stop_color_buffer,
            texture_id,
            supports_f64,
            standard,
            raymarch,
            perturbation,
            orbit_cache: None,
        }
    }

    pub fn texture_id(&self) -> egui::TextureId {
        self.texture_id
    }

    /// Whether the perturbation-theory deep-zoom shader can run on this GPU.
    pub fn supports_deep_zoom(&self) -> bool {
        self.supports_f64
    }

    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        egui_renderer: &mut egui_wgpu::Renderer,
        width: u32,
        height: u32,
    ) {
        if width == self.width && height == self.height || width == 0 || height == 0 {
            return;
        }
        let (_texture, output_view) = create_output_texture(device, width, height);
        self.standard.bind_group = create_basic_bind_group(
            device,
            &self.standard.bind_group_layout,
            &self.standard.uniform_buffer,
            &self.stop_pos_buffer,
            &self.stop_color_buffer,
            &output_view,
        );
        self.raymarch.bind_group = create_basic_bind_group(
            device,
            &self.raymarch.bind_group_layout,
            &self.raymarch.uniform_buffer,
            &self.stop_pos_buffer,
            &self.stop_color_buffer,
            &output_view,
        );
        if let Some(perturbation) = &mut self.perturbation {
            perturbation.glitch_buffer = create_glitch_buffer(device, width, height);
            perturbation.bind_group = create_perturbation_bind_group(
                device,
                &perturbation.bind_group_layout,
                &perturbation.uniform_buffer,
                &self.stop_pos_buffer,
                &self.stop_color_buffer,
                &output_view,
                &perturbation.orbit_buffer,
                &perturbation.glitch_buffer,
            );
        }
        egui_renderer.free_texture(&self.texture_id);
        self.texture_id =
            egui_renderer.register_native_texture(device, &output_view, wgpu::FilterMode::Nearest);
        self.output_view = output_view;
        self.width = width;
        self.height = height;
    }

    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, params: &FractalParams) {
        if params.zoom >= ZOOM_PERTURBATION_THRESHOLD
            && params.kind.supports_perturbation()
            && self.render_perturbation(device, queue, params)
        {
            return;
        }
        self.render_standard(device, queue, params);
    }

    /// Ray-marches a 3D fractal into the viewport texture.
    pub fn render_3d(&self, device: &wgpu::Device, queue: &wgpu::Queue, params: &Params3D) {
        let uniforms = RayUniforms::from_params(params);
        queue.write_buffer(&self.raymarch.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        let (positions, colors, _) = params.palette.to_gpu_stops();
        queue.write_buffer(&self.stop_pos_buffer, 0, cast_slice(&positions));
        queue.write_buffer(&self.stop_color_buffer, 0, cast_slice(&colors));
        dispatch(device, queue, &self.raymarch.pipeline, &self.raymarch.bind_group, self.width, self.height);
    }

    /// Ray-marches a 3D fractal into a fresh off-screen texture and reads it
    /// back as tightly-packed RGBA8, for export.
    pub fn render_3d_to_rgba(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &Params3D,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let uniforms = RayUniforms::from_params(params);
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("export_ray_uniform_buffer"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let (positions, colors, _) = params.palette.to_gpu_stops();
        let stop_pos_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("export_ray_stop_pos_buffer"),
            contents: cast_slice(&positions),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let stop_color_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("export_ray_stop_color_buffer"),
            contents: cast_slice(&colors),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let (texture, view) = create_output_texture(device, width, height);
        let bind_group = create_basic_bind_group(
            device,
            &self.raymarch.bind_group_layout,
            &uniform_buffer,
            &stop_pos_buffer,
            &stop_color_buffer,
            &view,
        );

        dispatch(device, queue, &self.raymarch.pipeline, &bind_group, width, height);
        read_texture_to_rgba(device, queue, &texture, width, height)
    }

    fn render_standard(&self, device: &wgpu::Device, queue: &wgpu::Queue, params: &FractalParams) {
        let uniforms = Uniforms::from_params(params);
        queue.write_buffer(&self.standard.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        let (positions, colors) = gpu_stop_arrays(params);
        queue.write_buffer(&self.stop_pos_buffer, 0, cast_slice(&positions));
        queue.write_buffer(&self.stop_color_buffer, 0, cast_slice(&colors));
        dispatch(device, queue, &self.standard.pipeline, &self.standard.bind_group, self.width, self.height);
    }

    /// Returns `false` (having rendered nothing) if the perturbation pipeline
    /// isn't available — the caller falls back to the standard pipeline.
    fn render_perturbation(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, params: &FractalParams) -> bool {
        let key = OrbitKey {
            center: params.center.clone(),
            kind: params.kind,
            max_iterations: params.max_iterations,
            bailout_radius: params.bailout_radius,
        };
        let needs_recompute = !matches!(&self.orbit_cache, Some((cached, _)) if *cached == key);
        if needs_recompute {
            let computed = orbit::compute_reference_orbit(&key.center, &key.kind, key.max_iterations, key.bailout_radius);
            self.orbit_cache = Some((key.clone(), computed));
        }
        let Some((_, orbit_points)) = &self.orbit_cache else { return false };
        let orbit_len = orbit_points.len() as u32;

        let Some(perturbation) = &mut self.perturbation else { return false };

        if needs_recompute {
            perturbation.orbit_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ref_orbit_buffer"),
                contents: cast_slice(orbit_points.as_slice()),
                usage: wgpu::BufferUsages::STORAGE,
            });
            perturbation.orbit_len = orbit_len;
            perturbation.bind_group = create_perturbation_bind_group(
                device,
                &perturbation.bind_group_layout,
                &perturbation.uniform_buffer,
                &self.stop_pos_buffer,
                &self.stop_color_buffer,
                &self.output_view,
                &perturbation.orbit_buffer,
                &perturbation.glitch_buffer,
            );
        }

        let uniforms = PerturbationUniforms::from_params(params, perturbation.orbit_len, false);
        queue.write_buffer(&perturbation.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        let (positions, colors) = gpu_stop_arrays(params);
        queue.write_buffer(&self.stop_pos_buffer, 0, cast_slice(&positions));
        queue.write_buffer(&self.stop_color_buffer, 0, cast_slice(&colors));
        dispatch(device, queue, &perturbation.pipeline, &perturbation.bind_group, self.width, self.height);

        // Secondary-reference glitch correction only needs to run right after
        // the primary orbit changed — panning/zooming without moving the
        // anchor point reuses both the orbit and last correction result, so
        // this doesn't add a GPU-sync stall to every interactive frame.
        if needs_recompute {
            self.correct_glitches(device, queue, params, &key);
        }
        true
    }

    /// Reads back the glitch-flag buffer after the primary perturbation
    /// dispatch; for any flagged pixels, computes a new reference orbit
    /// anchored near their centroid and re-dispatches a correction pass that
    /// only touches those pixels. Repeats up to [`MAX_GLITCH_CORRECTION_PASSES`]
    /// times, since one secondary reference may not resolve every pixel.
    fn correct_glitches(&self, device: &wgpu::Device, queue: &wgpu::Queue, params: &FractalParams, key: &OrbitKey) {
        let Some(perturbation) = &self.perturbation else { return };
        run_glitch_correction(
            device,
            queue,
            &perturbation.pipeline,
            &perturbation.bind_group_layout,
            &perturbation.uniform_buffer,
            &self.stop_pos_buffer,
            &self.stop_color_buffer,
            &self.output_view,
            &perturbation.glitch_buffer,
            params,
            &key.center,
            &key.kind,
            key.max_iterations,
            key.bailout_radius,
            self.width,
            self.height,
        );
    }

    /// Renders `params` at an arbitrary resolution (independent of the live
    /// viewport texture) and reads the result back as tightly-packed RGBA8, for
    /// still-image and animation-frame export. Uses the perturbation pipeline
    /// when `params.zoom` is deep enough and the GPU supports it (mirroring
    /// `render`'s pipeline choice), falling back to the standard f32 pipeline
    /// otherwise.
    pub fn render_to_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &FractalParams,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        if params.zoom >= ZOOM_PERTURBATION_THRESHOLD
            && params.kind.supports_perturbation()
            && let Some(rgba) = self.render_perturbation_to_rgba(device, queue, params, width, height)
        {
            return rgba;
        }
        self.render_standard_to_rgba(device, queue, params, width, height)
    }

    /// Returns `None` (having rendered nothing) if the perturbation pipeline
    /// isn't available — the caller falls back to the standard pipeline.
    fn render_perturbation_to_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &FractalParams,
        width: u32,
        height: u32,
    ) -> Option<Vec<u8>> {
        let key = OrbitKey {
            center: params.center.clone(),
            kind: params.kind,
            max_iterations: params.max_iterations,
            bailout_radius: params.bailout_radius,
        };
        let needs_recompute = !matches!(&self.orbit_cache, Some((cached, _)) if *cached == key);
        if needs_recompute {
            let computed = orbit::compute_reference_orbit(&key.center, &key.kind, key.max_iterations, key.bailout_radius);
            self.orbit_cache = Some((key.clone(), computed));
        }
        let (_, orbit_points) = self.orbit_cache.as_ref()?;
        let orbit_len = orbit_points.len() as u32;
        let perturbation = self.perturbation.as_ref()?;

        let uniforms = PerturbationUniforms::from_params(params, orbit_len, false);
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("export_perturbation_uniform_buffer"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let (positions, colors) = gpu_stop_arrays(params);
        let stop_pos_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("export_perturbation_stop_pos_buffer"),
            contents: cast_slice(&positions),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let stop_color_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("export_perturbation_stop_color_buffer"),
            contents: cast_slice(&colors),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let orbit_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("export_ref_orbit_buffer"),
            contents: cast_slice(orbit_points.as_slice()),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let glitch_buffer = create_glitch_buffer(device, width, height);

        let (texture, view) = create_output_texture(device, width, height);
        let bind_group = create_perturbation_bind_group(
            device,
            &perturbation.bind_group_layout,
            &uniform_buffer,
            &stop_pos_buffer,
            &stop_color_buffer,
            &view,
            &orbit_buffer,
            &glitch_buffer,
        );

        dispatch(device, queue, &perturbation.pipeline, &bind_group, width, height);

        run_glitch_correction(
            device,
            queue,
            &perturbation.pipeline,
            &perturbation.bind_group_layout,
            &uniform_buffer,
            &stop_pos_buffer,
            &stop_color_buffer,
            &view,
            &glitch_buffer,
            params,
            &key.center,
            &key.kind,
            key.max_iterations,
            key.bailout_radius,
            width,
            height,
        );

        Some(read_texture_to_rgba(device, queue, &texture, width, height))
    }

    fn render_standard_to_rgba(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        params: &FractalParams,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let uniforms = Uniforms::from_params(params);
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("export_uniform_buffer"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let (positions, colors) = gpu_stop_arrays(params);
        let stop_pos_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("export_stop_pos_buffer"),
            contents: cast_slice(&positions),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let stop_color_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("export_stop_color_buffer"),
            contents: cast_slice(&colors),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let (texture, view) = create_output_texture(device, width, height);
        let bind_group = create_basic_bind_group(
            device,
            &self.standard.bind_group_layout,
            &uniform_buffer,
            &stop_pos_buffer,
            &stop_color_buffer,
            &view,
        );

        dispatch(device, queue, &self.standard.pipeline, &bind_group, width, height);
        read_texture_to_rgba(device, queue, &texture, width, height)
    }
}

/// Mirrors the screen-space-to-world-offset math in `ui/viewport.rs::pixel_offset`
/// and `shaders/perturbation.wgsl` — must stay in sync with both.
fn pixel_offset(px_x: u32, px_y: u32, width: u32, height: u32, zoom: f64) -> (f64, f64) {
    let px = (f64::from(px_x) + 0.5) / f64::from(width) * 2.0 - 1.0;
    let py = (f64::from(px_y) + 0.5) / f64::from(height) * 2.0 - 1.0;
    let half_extent = 2.0 / zoom;
    let aspect = f64::from(width) / f64::from(height);
    (px * half_extent * aspect, -py * half_extent)
}

/// Shared secondary-reference-orbit glitch-correction loop, used both by the
/// live viewport (`FractalRenderer::correct_glitches`, against the shared
/// interactive buffers) and by off-screen export
/// (`FractalRenderer::render_perturbation_to_rgba`, against export-scoped
/// buffers sized to the export resolution). Reads back `glitch_buffer`,
/// recenters a new reference orbit on the flagged pixels' centroid, and
/// re-dispatches a correction pass — up to [`MAX_GLITCH_CORRECTION_PASSES`]
/// times, since one secondary reference may not resolve every pixel.
#[allow(clippy::too_many_arguments)]
fn run_glitch_correction(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    stop_pos_buffer: &wgpu::Buffer,
    stop_color_buffer: &wgpu::Buffer,
    view: &wgpu::TextureView,
    glitch_buffer: &wgpu::Buffer,
    params: &FractalParams,
    center: &HpComplex,
    kind: &FractalKind,
    max_iterations: u32,
    bailout_radius: f64,
    width: u32,
    height: u32,
) {
    let pixel_count = (width * height) as usize;

    for _ in 0..MAX_GLITCH_CORRECTION_PASSES {
        let flags = read_glitch_flags(device, queue, glitch_buffer, pixel_count);
        let Some((cx, cy, count)) = glitched_centroid(&flags, width) else { break };
        log::debug!("perturbation glitch correction: {count} glitched pixel(s), recentering on ({cx}, {cy})");

        let (dx, dy) = pixel_offset(cx, cy, width, height, params.zoom);
        let new_center = center.add_f64_delta(dx, dy);
        let secondary_orbit = orbit::compute_reference_orbit(&new_center, kind, max_iterations, bailout_radius);
        let secondary_len = secondary_orbit.len() as u32;

        let secondary_orbit_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("secondary_ref_orbit_buffer"),
            contents: cast_slice(secondary_orbit.as_slice()),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let correction_bind_group = create_perturbation_bind_group(
            device,
            layout,
            uniform_buffer,
            stop_pos_buffer,
            stop_color_buffer,
            view,
            &secondary_orbit_buffer,
            glitch_buffer,
        );

        let uniforms = PerturbationUniforms::from_params(params, secondary_len, true);
        queue.write_buffer(uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        dispatch(device, queue, pipeline, &correction_bind_group, width, height);
    }
}

/// Centroid (in pixel coordinates) of all flagged pixels, plus the count.
/// `None` if nothing is flagged.
fn glitched_centroid(flags: &[u32], width: u32) -> Option<(u32, u32, usize)> {
    let (sum_x, sum_y, count) = flags
        .par_iter()
        .enumerate()
        .filter(|&(_, &flag)| flag != 0)
        .map(|(i, _)| {
            let i = i as u64;
            let w = u64::from(width);
            (i % w, i / w)
        })
        .fold(|| (0u64, 0u64, 0u64), |(sx, sy, c), (x, y)| (sx + x, sy + y, c + 1))
        .reduce(|| (0, 0, 0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2));

    if count == 0 {
        return None;
    }
    Some(((sum_x / count) as u32, (sum_y / count) as u32, count as usize))
}

fn create_output_texture(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fractal_output_texture"),
        size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_glitch_buffer(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Buffer {
    let pixel_count = (width.max(1) * height.max(1)) as u64;
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("glitch_flags_buffer"),
        size: pixel_count * std::mem::size_of::<u32>() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

fn storage_texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::Rgba8Unorm,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn read_only_storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn read_write_storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Builds a [`BasicPipeline`] for `source` (a full WGSL module whose
/// uniform struct is `uniform_size` bytes). `name` prefixes the debug labels.
fn create_basic_pipeline(
    device: &wgpu::Device,
    name: &str,
    source: &str,
    uniform_size: u64,
    stop_pos_buffer: &wgpu::Buffer,
    stop_color_buffer: &wgpu::Buffer,
    output_view: &wgpu::TextureView,
) -> BasicPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(&format!("{name}_shader")),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(&format!("{name}_bind_group_layout")),
        entries: &[
            uniform_entry(0),
            read_only_storage_entry(1),
            read_only_storage_entry(2),
            storage_texture_entry(3),
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(&format!("{name}_pipeline_layout")),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(&format!("{name}_pipeline")),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(&format!("{name}_uniform_buffer")),
        size: uniform_size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let bind_group = create_basic_bind_group(
        device,
        &bind_group_layout,
        &uniform_buffer,
        stop_pos_buffer,
        stop_color_buffer,
        output_view,
    );

    BasicPipeline { pipeline, bind_group_layout, uniform_buffer, bind_group }
}

fn create_basic_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    stop_pos_buffer: &wgpu::Buffer,
    stop_color_buffer: &wgpu::Buffer,
    output_view: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("basic_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: stop_pos_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: stop_color_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(output_view) },
        ],
    })
}

#[allow(clippy::too_many_arguments)]
fn create_perturbation_pipeline(
    device: &wgpu::Device,
    stop_pos_buffer: &wgpu::Buffer,
    stop_color_buffer: &wgpu::Buffer,
    output_view: &wgpu::TextureView,
    width: u32,
    height: u32,
    initial_orbit_len: u32,
) -> PerturbationPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("perturbation_shader"),
        source: wgpu::ShaderSource::Wgsl(PERTURBATION_SRC.into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("perturbation_bind_group_layout"),
        entries: &[
            uniform_entry(0),
            read_only_storage_entry(1),
            read_only_storage_entry(2),
            storage_texture_entry(3),
            read_only_storage_entry(4),
            read_write_storage_entry(5),
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("perturbation_pipeline_layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("perturbation_pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    });

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("perturbation_uniform_buffer"),
        size: std::mem::size_of::<PerturbationUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let orbit_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ref_orbit_buffer"),
        size: (initial_orbit_len.max(1) as u64) * std::mem::size_of::<[f64; 2]>() as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let glitch_buffer = create_glitch_buffer(device, width, height);

    let bind_group = create_perturbation_bind_group(
        device,
        &bind_group_layout,
        &uniform_buffer,
        stop_pos_buffer,
        stop_color_buffer,
        output_view,
        &orbit_buffer,
        &glitch_buffer,
    );

    PerturbationPipeline {
        pipeline,
        bind_group_layout,
        uniform_buffer,
        bind_group,
        orbit_buffer,
        orbit_len: 0,
        glitch_buffer,
    }
}

#[allow(clippy::too_many_arguments)]
fn create_perturbation_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    stop_pos_buffer: &wgpu::Buffer,
    stop_color_buffer: &wgpu::Buffer,
    output_view: &wgpu::TextureView,
    orbit_buffer: &wgpu::Buffer,
    glitch_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("perturbation_bind_group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: stop_pos_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: stop_color_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(output_view) },
            wgpu::BindGroupEntry { binding: 4, resource: orbit_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 5, resource: glitch_buffer.as_entire_binding() },
        ],
    })
}

fn dispatch(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    bind_group: &wgpu::BindGroup,
    width: u32,
    height: u32,
) {
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("fractal_encoder") });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("fractal_pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        let groups_x = width.div_ceil(WORKGROUP_SIZE);
        let groups_y = height.div_ceil(WORKGROUP_SIZE);
        pass.dispatch_workgroups(groups_x, groups_y, 1);
    }
    queue.submit(Some(encoder.finish()));
}

fn read_texture_to_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let bytes_per_pixel = 4u32;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;

    let buffer_size = (padded_bytes_per_row * height) as u64;
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("export_readback_buffer"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("export_copy_encoder") });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit(Some(encoder.finish()));

    let buffer_slice = output_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).expect("device poll failed");
    rx.recv().expect("map_async channel closed").expect("buffer map failed");

    let data = buffer_slice.get_mapped_range();
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded_bytes_per_row) as usize;
        let end = start + unpadded_bytes_per_row as usize;
        rgba.extend_from_slice(&data[start..end]);
    }
    drop(data);
    output_buffer.unmap();
    rgba
}

fn read_glitch_flags(device: &wgpu::Device, queue: &wgpu::Queue, glitch_buffer: &wgpu::Buffer, pixel_count: usize) -> Vec<u32> {
    let size = (pixel_count * std::mem::size_of::<u32>()) as u64;
    let readback_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("glitch_flags_readback_buffer"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("glitch_flags_copy_encoder") });
    encoder.copy_buffer_to_buffer(glitch_buffer, 0, &readback_buffer, 0, size);
    queue.submit(Some(encoder.finish()));

    let buffer_slice = readback_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).expect("device poll failed");
    rx.recv().expect("map_async channel closed").expect("buffer map failed");

    let data = buffer_slice.get_mapped_range();
    let flags = cast_slice::<u8, u32>(&data).to_vec();
    drop(data);
    readback_buffer.unmap();
    flags
}
