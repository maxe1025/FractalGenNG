/// Escape-time rule: `z_{n+1} = z_n^power + c`, iterated from `z_0 = 0`,
/// where `c` is the pixel's complex-plane coordinate — the generalization of
/// the Mandelbrot set (`power = 2`) to arbitrary exponents. Computed in the
/// shader via polar form (`r^power`, `angle * power`) rather than repeated
/// multiplication, so any power works without a variable-length loop.
/// Mirrored in `shaders/standard.wgsl`.
pub const DEFAULT_CENTER: [f64; 2] = [0.0, 0.0];
pub const DEFAULT_POWER: f64 = 3.0;
