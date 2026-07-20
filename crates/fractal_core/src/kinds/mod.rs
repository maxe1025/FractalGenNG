mod burning_ship;
mod julia;
mod mandelbrot;
mod multibrot;
mod tricorn;

pub use burning_ship::DEFAULT_CENTER as BURNING_SHIP_DEFAULT_CENTER;
pub use julia::{DEFAULT_CENTER as JULIA_DEFAULT_CENTER, DEFAULT_SEED as JULIA_DEFAULT_SEED};
pub use mandelbrot::DEFAULT_CENTER as MANDELBROT_DEFAULT_CENTER;
pub use multibrot::{DEFAULT_CENTER as MULTIBROT_DEFAULT_CENTER, DEFAULT_POWER as MULTIBROT_DEFAULT_POWER};
pub use tricorn::DEFAULT_CENTER as TRICORN_DEFAULT_CENTER;

use serde::{Deserialize, Serialize};

/// Which escape-time formula to render. The GPU shader dispatches on `shader_id()`,
/// so adding a new kind here means adding a matching branch in `shaders/standard.wgsl`.
///
/// Burning Ship, Tricorn, and Multibrot are standard-shader (f32) only —
/// [`FractalKind::supports_perturbation`] gates the deep-zoom perturbation
/// path to Mandelbrot/Julia, the two kinds `orbit::compute_reference_orbit`
/// knows how to compute a reference orbit for. Extending perturbation theory
/// to the others is future work (Burning Ship's `abs()` in particular breaks
/// the smooth derivative the delta-iteration recurrence relies on at axis
/// crossings, and needs dedicated handling rather than a quick copy-paste).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FractalKind {
    Mandelbrot,
    Julia { seed: [f64; 2] },
    BurningShip,
    Tricorn,
    Multibrot { power: f64 },
}

impl FractalKind {
    pub fn shader_id(self) -> u32 {
        match self {
            FractalKind::Mandelbrot => 0,
            FractalKind::Julia { .. } => 1,
            FractalKind::BurningShip => 2,
            FractalKind::Tricorn => 3,
            FractalKind::Multibrot { .. } => 4,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            FractalKind::Mandelbrot => "Mandelbrot",
            FractalKind::Julia { .. } => "Julia",
            FractalKind::BurningShip => "Burning Ship",
            FractalKind::Tricorn => "Tricorn",
            FractalKind::Multibrot { .. } => "Multibrot",
        }
    }

    pub fn julia_seed(self) -> [f64; 2] {
        match self {
            FractalKind::Julia { seed } => seed,
            _ => [0.0, 0.0],
        }
    }

    pub fn default_center(self) -> [f64; 2] {
        match self {
            FractalKind::Mandelbrot => MANDELBROT_DEFAULT_CENTER,
            FractalKind::Julia { .. } => JULIA_DEFAULT_CENTER,
            FractalKind::BurningShip => BURNING_SHIP_DEFAULT_CENTER,
            FractalKind::Tricorn => TRICORN_DEFAULT_CENTER,
            FractalKind::Multibrot { .. } => MULTIBROT_DEFAULT_CENTER,
        }
    }

    /// Whether the deep-zoom perturbation-theory pipeline can render this
    /// kind. See the type-level doc comment for why Burning Ship/Tricorn/
    /// Multibrot aren't included yet.
    pub fn supports_perturbation(self) -> bool {
        matches!(self, FractalKind::Mandelbrot | FractalKind::Julia { .. })
    }

    /// Whether `shaders/standard.wgsl` tracks the derivative
    /// (`dz/dc` for Mandelbrot, `dz/dz_0` for Julia) that distance-estimator
    /// shading needs. Coincides with [`Self::supports_perturbation`] today
    /// (same two kinds, for unrelated reasons — the shader just hasn't
    /// grown a derivative recurrence for the others yet), but is a distinct
    /// concept: this gates a standard-shader coloring feature, that gates
    /// the perturbation deep-zoom pipeline. Kept separate so the two can
    /// diverge without silently changing the other's behavior.
    pub fn supports_distance_shading(self) -> bool {
        matches!(self, FractalKind::Mandelbrot | FractalKind::Julia { .. })
    }

    /// The single free parameter this kind's iteration formula exposes for
    /// timeline animation, if any — Julia's seed, Multibrot's power. Kinds
    /// with no free parameter (Mandelbrot, Burning Ship, Tricorn) have
    /// `KindParam::None`. `zoom` is always animatable regardless of kind and
    /// isn't part of this — see `export::animation::Timeline`.
    pub fn animatable_param(self) -> KindParam {
        match self {
            FractalKind::Julia { seed } => KindParam::Seed(seed),
            FractalKind::Multibrot { power } => KindParam::Power(power),
            FractalKind::Mandelbrot | FractalKind::BurningShip | FractalKind::Tricorn => KindParam::None,
        }
    }

    /// Rebuilds this kind with `param` applied. If `param`'s variant doesn't
    /// match what this kind actually animates (e.g. a `Seed` handed to a
    /// `Mandelbrot`), returns `self` unchanged rather than panicking — a
    /// mismatch here means caller-side inconsistency (a keyframe built
    /// against a different kind than the timeline's `base`), not something
    /// a render should crash over.
    pub fn with_animatable_param(self, param: KindParam) -> Self {
        match (self, param) {
            (FractalKind::Julia { .. }, KindParam::Seed(seed)) => FractalKind::Julia { seed },
            (FractalKind::Multibrot { .. }, KindParam::Power(power)) => FractalKind::Multibrot { power },
            _ => self,
        }
    }
}

/// A kind's animatable free parameter — see [`FractalKind::animatable_param`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum KindParam {
    None,
    Seed([f64; 2]),
    Power(f64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_ids_are_distinct() {
        let kinds = [
            FractalKind::Mandelbrot,
            FractalKind::Julia { seed: [0.0, 0.0] },
            FractalKind::BurningShip,
            FractalKind::Tricorn,
            FractalKind::Multibrot { power: 3.0 },
        ];
        let ids: Vec<u32> = kinds.iter().map(|k| k.shader_id()).collect();
        for (i, a) in ids.iter().enumerate() {
            for (j, b) in ids.iter().enumerate() {
                assert!(i == j || a != b, "duplicate shader_id {a} for kinds {i} and {j}");
            }
        }
    }

    #[test]
    fn only_mandelbrot_and_julia_support_perturbation() {
        assert!(FractalKind::Mandelbrot.supports_perturbation());
        assert!(FractalKind::Julia { seed: [0.0, 0.0] }.supports_perturbation());
        assert!(!FractalKind::BurningShip.supports_perturbation());
        assert!(!FractalKind::Tricorn.supports_perturbation());
        assert!(!FractalKind::Multibrot { power: 3.0 }.supports_perturbation());
    }

    #[test]
    fn kinds_without_a_free_parameter_report_none() {
        assert_eq!(FractalKind::Mandelbrot.animatable_param(), KindParam::None);
        assert_eq!(FractalKind::BurningShip.animatable_param(), KindParam::None);
        assert_eq!(FractalKind::Tricorn.animatable_param(), KindParam::None);
    }

    #[test]
    fn julia_and_multibrot_animatable_param_round_trips() {
        let julia = FractalKind::Julia { seed: [0.1, 0.2] };
        assert_eq!(julia.animatable_param(), KindParam::Seed([0.1, 0.2]));
        assert_eq!(julia.with_animatable_param(KindParam::Seed([0.3, 0.4])), FractalKind::Julia { seed: [0.3, 0.4] });

        let multibrot = FractalKind::Multibrot { power: 3.0 };
        assert_eq!(multibrot.animatable_param(), KindParam::Power(3.0));
        assert_eq!(multibrot.with_animatable_param(KindParam::Power(5.0)), FractalKind::Multibrot { power: 5.0 });
    }

    #[test]
    fn with_animatable_param_ignores_mismatched_variant() {
        // A Seed handed to a kind with no seed just leaves it unchanged,
        // rather than panicking — defensive against caller-side mismatches.
        assert_eq!(FractalKind::Mandelbrot.with_animatable_param(KindParam::Seed([1.0, 2.0])), FractalKind::Mandelbrot);
    }
}
