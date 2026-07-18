/// Escape-time rule: `z_{n+1} = (|Re(z_n)| + i|Im(z_n)|)^2 + c`, iterated from
/// `z_0 = 0`, where `c` is the pixel's complex-plane coordinate. Taking the
/// absolute value of each component before squaring is what gives the set
/// its asymmetric, ship-like silhouette. Mirrored in `shaders/standard.wgsl`.
pub const DEFAULT_CENTER: [f64; 2] = [-0.4, -0.5];
