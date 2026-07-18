//! Throwaway diagnostic: which backends/adapters on this machine support
//! `wgpu::Features::SHADER_F64`? Informs whether Phase 3's perturbation
//! shader can use native f64 or needs a software double-double fallback.

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    pollster::block_on(run());
}

async fn run() {
    for backends in [wgpu::Backends::DX12, wgpu::Backends::VULKAN, wgpu::Backends::GL] {
        println!("=== Backend: {backends:?} ===");
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapters = instance.enumerate_adapters(backends).await;
        if adapters.is_empty() {
            println!("  (no adapters)");
            continue;
        }
        for adapter in adapters {
            let info = adapter.get_info();
            let features = adapter.features();
            println!("  Adapter: {} ({:?}, backend={:?})", info.name, info.device_type, info.backend);
            println!("    SHADER_F64: {}", features.contains(wgpu::Features::SHADER_F64));
        }
    }
}
