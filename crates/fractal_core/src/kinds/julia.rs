/// Escape-time rule: `z_{n+1} = z_n^2 + seed`, iterated from `z_0` = the pixel's
/// complex-plane coordinate, where `seed` is a fixed constant. Mirrored in `shaders/standard.wgsl`.
pub const DEFAULT_SEED: [f64; 2] = [-0.4, 0.6];
pub const DEFAULT_CENTER: [f64; 2] = [0.0, 0.0];
