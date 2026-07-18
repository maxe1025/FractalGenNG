/// Escape-time rule: `z_{n+1} = z_n^2 + c`, iterated from `z_0 = 0`, where `c` is the
/// pixel's complex-plane coordinate. Mirrored in `shaders/standard.wgsl`.
pub const DEFAULT_CENTER: [f64; 2] = [-0.5, 0.0];
