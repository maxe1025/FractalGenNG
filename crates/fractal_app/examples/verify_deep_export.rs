//! Throwaway diagnostic for Phase 3e verification: does `render_to_rgba`
//! actually use the perturbation pipeline at deep zoom and produce a correct
//! image, or does it still silently fall back to the f32 pipeline? Renders
//! both a shallow and a deep-zoom PNG at the seahorse-valley coordinate used
//! throughout Phase 3 verification, plus a simulated multi-frame animation
//! export where only the last frame is deep — exercising the same
//! `frame_with_zoom` precision-upgrade path `export_panel.rs` relies on.

use fractal_app::gpu::renderer::FractalRenderer;
use fractal_core::export::animation::{generate_zoom_frames, ZoomCurve};
use fractal_core::hp::HpComplex;
use fractal_core::{FractalKind, FractalParams, Palette};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    pollster::block_on(run());
}

async fn run() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapters = instance.enumerate_adapters(wgpu::Backends::VULKAN).await;
    let adapter = adapters
        .iter()
        .find(|a| a.features().contains(wgpu::Features::SHADER_F64))
        .or_else(|| adapters.first())
        .expect("no adapter")
        .clone();
    println!("adapter: {} SHADER_F64={}", adapter.get_info().name, adapter.features().contains(wgpu::Features::SHADER_F64));

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("verify device"),
            required_features: wgpu::Features::SHADER_F64,
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        })
        .await
        .expect("no device");

    let mut egui_renderer = egui_wgpu::Renderer::new(&device, wgpu::TextureFormat::Rgba8Unorm, egui_wgpu::RendererOptions::default());
    let mut renderer = FractalRenderer::new(&device, &mut egui_renderer, 64, 64);
    println!("supports_deep_zoom: {}", renderer.supports_deep_zoom());

    let seahorse = HpComplex::from_f64_pair(-0.743643887037151, 0.131825904205330, 128);
    let mut params = FractalParams {
        kind: FractalKind::Mandelbrot,
        center: seahorse,
        zoom: 1.0,
        max_iterations: 5000,
        bailout_radius: 4.0,
        palette: Palette::fire(),
    };

    // Shallow export (should use the standard pipeline).
    params.zoom = 1.0;
    let shallow = renderer.render_to_rgba(&device, &queue, &params, 256, 256);
    save(&shallow, 256, 256, "verify_shallow.png");

    // Deep export (should now use the perturbation pipeline, not the old
    // always-f32 path).
    params.zoom = 1.0e9;
    let deep = renderer.render_to_rgba(&device, &queue, &params, 256, 256);
    save(&deep, 256, 256, "verify_deep.png");
    report_non_black_fraction("verify_deep.png (1e9)", &deep);

    // Even deeper, to confirm it's not just coincidentally matching the
    // shallow render (f32 breaks down completely well before this).
    params.zoom = 1.0e15;
    let very_deep = renderer.render_to_rgba(&device, &queue, &params, 256, 256);
    save(&very_deep, 256, 256, "verify_very_deep.png");
    report_non_black_fraction("verify_very_deep.png (1e15)", &very_deep);

    // Animation export path: base authored at shallow zoom (low precision),
    // end zoom deep — exercises frame_with_zoom's precision upgrade + the
    // renderer's orbit cache across frames with a fixed center.
    let base = FractalParams {
        kind: FractalKind::Mandelbrot,
        center: HpComplex::from_f64_pair(-0.743643887037151, 0.131825904205330, 64),
        zoom: 1.0,
        max_iterations: 5000,
        bailout_radius: 4.0,
        palette: Palette::fire(),
    };
    let frames = generate_zoom_frames(&base, 1.0, 1.0e12, 4, ZoomCurve::Exponential);
    for (i, frame) in frames.iter().enumerate() {
        println!(
            "frame {i}: zoom={:e} center.precision_bits={}",
            frame.zoom,
            frame.center.precision_bits()
        );
        let rgba = renderer.render_to_rgba(&device, &queue, frame, 128, 128);
        save(&rgba, 128, 128, &format!("verify_anim_frame_{i}.png"));
        report_non_black_fraction(&format!("anim frame {i} (zoom={:e})", frame.zoom), &rgba);
    }

    println!("done");
}

fn save(rgba: &[u8], width: u32, height: u32, name: &str) {
    let path = std::env::temp_dir().join(name);
    fractal_core::export::image_export::save_png(width, height, rgba, &path).expect("save png");
    println!("saved {}", path.display());
}

fn report_non_black_fraction(label: &str, rgba: &[u8]) {
    let pixels = rgba.len() / 4;
    let non_black = rgba.chunks_exact(4).filter(|p| p[0] > 4 || p[1] > 4 || p[2] > 4).count();
    println!("{label}: {non_black}/{pixels} non-black pixels ({:.1}%)", 100.0 * non_black as f64 / pixels as f64);
}
