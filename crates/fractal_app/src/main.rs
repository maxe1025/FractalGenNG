use std::sync::Arc;

use fractal_app::app;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    // Deep zoom needs a GPU/driver that supports 64-bit shader floats for the
    // perturbation-theory delta iteration. This is currently only implemented
    // by wgpu on the Vulkan backend, and only some adapters expose it at all
    // (many consumer GPUs reserve it for professional/compute cards). Search
    // every adapter wgpu can see (across whichever backends are enabled) and
    // prefer one that supports it; fall back to the first adapter otherwise
    // so the app still runs (without deep zoom) on hardware that lacks it.
    let mut wgpu_setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    wgpu_setup.native_adapter_selector = Some(Arc::new(|adapters, _surface| {
        adapters
            .iter()
            .find(|a| a.features().contains(wgpu::Features::SHADER_F64))
            .or_else(|| adapters.first())
            .cloned()
            .ok_or_else(|| "no wgpu adapters available".to_string())
    }));
    wgpu_setup.device_descriptor = Arc::new(|adapter| {
        let mut required_features = wgpu::Features::empty();
        if adapter.features().contains(wgpu::Features::SHADER_F64) {
            required_features |= wgpu::Features::SHADER_F64;
        }
        wgpu::DeviceDescriptor {
            label: Some("fractalgen device"),
            required_features,
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }
    });

    let native_options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(wgpu_setup),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "FractalGen",
        native_options,
        Box::new(|cc| Ok(Box::new(app::FractalApp::new(cc)))),
    )
}
