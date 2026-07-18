use serde::{Deserialize, Serialize};

use crate::hp::{required_bits, HpComplex};
use crate::kinds::FractalKind;
use crate::palette::Palette;

/// A complete, serializable description of one fractal render. The same
/// `FractalParams` value drives the interactive viewport, a still-image export,
/// and (in a later phase) each frame of a zoom animation, so exports always
/// match what was seen on screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FractalParams {
    pub kind: FractalKind,
    /// Focus point: the complex-plane coordinate at the center of the view.
    /// Arbitrary-precision so it stays resolvable far beyond what `f64` can
    /// represent at extreme zoom.
    pub center: HpComplex,
    /// Magnification factor. The half-*height* of the visible complex-plane region
    /// is `2.0 / zoom`; the half-width is scaled further by the viewport aspect ratio.
    pub zoom: f64,
    pub max_iterations: u32,
    pub bailout_radius: f64,
    pub palette: Palette,
}

impl Default for FractalParams {
    fn default() -> Self {
        let kind = FractalKind::Mandelbrot;
        let zoom = 1.0;
        let [cx, cy] = kind.default_center();
        Self {
            center: HpComplex::from_f64_pair(cx, cy, required_bits(zoom)),
            kind,
            zoom,
            max_iterations: 500,
            bailout_radius: 4.0,
            palette: Palette::default(),
        }
    }
}

impl FractalParams {
    pub fn julia_default() -> Self {
        let kind = FractalKind::Julia { seed: crate::kinds::JULIA_DEFAULT_SEED };
        let zoom = 1.0;
        let [cx, cy] = kind.default_center();
        Self {
            center: HpComplex::from_f64_pair(cx, cy, required_bits(zoom)),
            kind,
            zoom,
            max_iterations: 500,
            bailout_radius: 4.0,
            palette: Palette::default(),
        }
    }
}
