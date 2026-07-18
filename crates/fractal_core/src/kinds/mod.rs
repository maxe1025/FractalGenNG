mod julia;
mod mandelbrot;

pub use julia::{DEFAULT_CENTER as JULIA_DEFAULT_CENTER, DEFAULT_SEED as JULIA_DEFAULT_SEED};
pub use mandelbrot::DEFAULT_CENTER as MANDELBROT_DEFAULT_CENTER;

use serde::{Deserialize, Serialize};

/// Which escape-time formula to render. The GPU shader dispatches on `shader_id()`,
/// so adding a new kind here (e.g. Burning Ship, Tricorn, Multibrot) means adding a
/// matching branch in `shaders/standard.wgsl`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FractalKind {
    Mandelbrot,
    Julia { seed: [f64; 2] },
}

impl FractalKind {
    pub fn shader_id(self) -> u32 {
        match self {
            FractalKind::Mandelbrot => 0,
            FractalKind::Julia { .. } => 1,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            FractalKind::Mandelbrot => "Mandelbrot",
            FractalKind::Julia { .. } => "Julia",
        }
    }

    pub fn julia_seed(self) -> [f64; 2] {
        match self {
            FractalKind::Julia { seed } => seed,
            FractalKind::Mandelbrot => [0.0, 0.0],
        }
    }

    pub fn default_center(self) -> [f64; 2] {
        match self {
            FractalKind::Mandelbrot => MANDELBROT_DEFAULT_CENTER,
            FractalKind::Julia { .. } => JULIA_DEFAULT_CENTER,
        }
    }
}
