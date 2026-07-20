use serde::{Deserialize, Serialize};

use crate::hp::required_bits;
use crate::kinds::KindParam;
use crate::params::FractalParams;

/// How a segment's normalized progress (`t` in `0.0..=1.0`) is paced before
/// being fed into whatever's being interpolated across that segment —
/// applies uniformly to every animated property in the segment (zoom, a
/// kind's free parameter, ...), not just zoom.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Easing {
    /// Constant pace.
    Linear,
    /// The pace itself speeds up (`exponent > 1.0`) or slows down
    /// (`exponent < 1.0`) over the segment; `1.0` is identical to `Linear`.
    /// `exponent > 1.0` starts slow and builds speed toward the end — fixes
    /// the "zoom feels like it stalls" perception deep zooms get even under
    /// constant-ratio pacing, since self-similar fractal detail can make a
    /// mathematically-constant rate read as visually static.
    Accelerating { exponent: f64 },
}

impl Easing {
    fn remap(self, t: f64) -> f64 {
        match self {
            Easing::Linear => t,
            Easing::Accelerating { exponent } => t.powf(exponent),
        }
    }
}

/// One waypoint in an animation's timeline: `zoom` and the fractal kind's
/// free parameter (if it has one — see [`crate::FractalKind::animatable_param`])
/// at a specific frame index, plus the easing used for the segment leading
/// into it from the *previous* keyframe (ignored on a timeline's first
/// keyframe, since no segment precedes it).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub frame: u32,
    pub zoom: f64,
    pub kind_param: KindParam,
    pub easing: Easing,
}

/// A full animation: `base` holds everything that stays fixed for the whole
/// animation (center, kind, max_iterations, bailout_radius, palette), and
/// `keyframes` (sorted ascending by `frame`) describes how `zoom` and the
/// kind's free parameter move over the animation's frames.
///
/// `keyframes` is a `Vec` — not a fixed start/end pair — so multi-keyframe
/// animations (pausing at or passing through intermediate waypoints) can be
/// added later without another data-model change, even though today's only
/// constructor ([`Timeline::two_point`]) ever produces exactly two.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timeline {
    pub base: FractalParams,
    pub keyframes: Vec<Keyframe>,
}

impl Timeline {
    /// The current (only) shape a timeline is built in: a start keyframe at
    /// frame 0 and an end keyframe at `frame_count - 1`. `start`/`end` are
    /// each `(zoom, kind_param)` — pass `KindParam::None` for kinds with no
    /// free parameter, or to leave it unanimated.
    pub fn two_point(
        base: FractalParams,
        start: (f64, KindParam),
        end: (f64, KindParam),
        frame_count: u32,
        easing: Easing,
    ) -> Self {
        if frame_count == 0 {
            return Self { base, keyframes: Vec::new() };
        }
        let last_frame = frame_count - 1;
        Self {
            base,
            keyframes: vec![
                Keyframe { frame: 0, zoom: start.0, kind_param: start.1, easing: Easing::Linear },
                Keyframe { frame: last_frame, zoom: end.0, kind_param: end.1, easing },
            ],
        }
    }

    /// Generates one [`FractalParams`] per frame, walking the keyframe
    /// sequence and interpolating within whichever segment each frame index
    /// falls into.
    pub fn generate_frames(&self) -> Vec<FractalParams> {
        let Some(last) = self.keyframes.last() else { return Vec::new() };
        if self.keyframes.len() == 1 {
            return vec![frame_with_animatable(&self.base, last.zoom, last.kind_param)];
        }

        (0..=last.frame).map(|frame| self.params_at_frame(frame)).collect()
    }

    fn params_at_frame(&self, frame: u32) -> FractalParams {
        let (a, b) = self.segment_for_frame(frame);
        let span = f64::from(b.frame.saturating_sub(a.frame)).max(1.0);
        let t = (f64::from(frame.saturating_sub(a.frame)) / span).clamp(0.0, 1.0);
        let eased = b.easing.remap(t);

        let zoom = a.zoom * (b.zoom / a.zoom).powf(eased);
        let kind_param = lerp_kind_param(a.kind_param, b.kind_param, eased);
        frame_with_animatable(&self.base, zoom, kind_param)
    }

    /// The adjacent keyframe pair `frame` falls between. Assumes
    /// `keyframes` is sorted ascending by `frame` and has at least 2 entries.
    fn segment_for_frame(&self, frame: u32) -> (&Keyframe, &Keyframe) {
        let mut idx = 0;
        for (i, kf) in self.keyframes.iter().enumerate() {
            if kf.frame <= frame {
                idx = i;
            } else {
                break;
            }
        }
        let idx = idx.min(self.keyframes.len() - 2);
        (&self.keyframes[idx], &self.keyframes[idx + 1])
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn lerp_kind_param(a: KindParam, b: KindParam, t: f64) -> KindParam {
    match (a, b) {
        (KindParam::Seed(sa), KindParam::Seed(sb)) => KindParam::Seed([lerp(sa[0], sb[0], t), lerp(sa[1], sb[1], t)]),
        (KindParam::Power(pa), KindParam::Power(pb)) => KindParam::Power(lerp(pa, pb, t)),
        // KindParam::None, or a mismatched pair (shouldn't happen for a
        // correctly-built timeline, since both keyframes share `base.kind`)
        // — hold at the segment's starting value.
        _ => a,
    }
}

/// Clones `base` with `zoom`/the kind's free parameter set, upgrading
/// `center`'s precision if this frame's zoom is deeper than what `base` was
/// carrying (frames are typically generated from a `base` authored at the
/// *start* of the animation, whose precision was only ever tuned for that
/// starting zoom — deeper frames need more mantissa bits or their reference
/// orbit is computed with mistakenly low precision). Never downgrades:
/// `base`'s existing precision is a floor.
fn frame_with_animatable(base: &FractalParams, zoom: f64, kind_param: KindParam) -> FractalParams {
    let mut params = base.clone();
    params.zoom = zoom;
    params.kind = base.kind.with_animatable_param(kind_param);
    let needed = required_bits(zoom);
    if needed > params.center.precision_bits() {
        params.center = params.center.with_precision(needed);
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kinds::FractalKind;

    fn zooms(base: &FractalParams, start_zoom: f64, end_zoom: f64, frame_count: u32, easing: Easing) -> Vec<f64> {
        Timeline::two_point(base.clone(), (start_zoom, KindParam::None), (end_zoom, KindParam::None), frame_count, easing)
            .generate_frames()
            .iter()
            .map(|p| p.zoom)
            .collect()
    }

    #[test]
    fn frame_count_zero_yields_no_frames() {
        let base = FractalParams::default();
        assert!(zooms(&base, 1.0, 100.0, 0, Easing::Linear).is_empty());
    }

    #[test]
    fn frame_count_one_uses_start_zoom() {
        let base = FractalParams::default();
        let z = zooms(&base, 5.0, 500.0, 1, Easing::Linear);
        assert_eq!(z.len(), 1);
        assert_eq!(z[0], 5.0);
    }

    #[test]
    fn linear_easing_hits_endpoints_and_is_monotonic_and_geometric() {
        let base = FractalParams::default();
        let z = zooms(&base, 1.0, 1_000_000.0, 7, Easing::Linear);
        assert!((z[0] - 1.0).abs() < 1e-6);
        assert!((z.last().unwrap() - 1_000_000.0).abs() < 1.0);
        for pair in z.windows(2) {
            assert!(pair[1] > pair[0], "zoom must strictly increase frame over frame");
        }
        // Zoom is always geometrically interpolated: equal ratios between
        // consecutive frames under constant-pace (Linear) easing.
        let ratio = z[1] / z[0];
        for pair in z.windows(2) {
            let r = pair[1] / pair[0];
            assert!((r - ratio).abs() < 1e-6, "ratio should be constant across frames");
        }
    }

    #[test]
    fn accelerating_easing_hits_endpoints_and_is_monotonic() {
        let base = FractalParams::default();
        let z = zooms(&base, 1.0, 1_000_000.0, 7, Easing::Accelerating { exponent: 2.5 });
        assert!((z[0] - 1.0).abs() < 1e-6);
        assert!((z.last().unwrap() - 1_000_000.0).abs() < 1.0);
        for pair in z.windows(2) {
            assert!(pair[1] > pair[0], "zoom must strictly increase frame over frame");
        }
    }

    #[test]
    fn accelerating_exponent_one_matches_linear_easing() {
        let base = FractalParams::default();
        let linear = zooms(&base, 1.0, 1_000_000.0, 7, Easing::Linear);
        let accelerating = zooms(&base, 1.0, 1_000_000.0, 7, Easing::Accelerating { exponent: 1.0 });
        for (a, b) in linear.iter().zip(accelerating.iter()) {
            assert!((a - b).abs() < 1e-6, "exponent=1.0 should match Linear easing exactly");
        }
    }

    #[test]
    fn accelerating_exponent_above_one_starts_slower_than_linear_easing() {
        let base = FractalParams::default();
        let linear = zooms(&base, 1.0, 1_000_000.0, 10, Easing::Linear);
        let accelerating = zooms(&base, 1.0, 1_000_000.0, 10, Easing::Accelerating { exponent: 3.0 });
        let linear_first_ratio = linear[1] / linear[0];
        let acc_first_ratio = accelerating[1] / accelerating[0];
        assert!(
            acc_first_ratio < linear_first_ratio,
            "accelerating (exponent>1) should start slower: {acc_first_ratio} vs {linear_first_ratio}"
        );
    }

    #[test]
    fn center_kind_and_palette_are_preserved_from_base() {
        let mut base = FractalParams::julia_default();
        base.center = crate::hp::HpComplex::from_f64_pair(0.123, -0.456, 128);
        let timeline =
            Timeline::two_point(base.clone(), (1.0, KindParam::None), (2.0, KindParam::None), 4, Easing::Linear);
        for frame in timeline.generate_frames() {
            assert_eq!(frame.center, base.center);
            assert_eq!(frame.kind.shader_id(), base.kind.shader_id());
        }
    }

    #[test]
    fn julia_seed_interpolates_linearly_across_frames() {
        let base = FractalParams::julia_default();
        let timeline = Timeline::two_point(
            base,
            (1.0, KindParam::Seed([0.0, 0.0])),
            (1.0, KindParam::Seed([1.0, -2.0])),
            5,
            Easing::Linear,
        );
        let frames = timeline.generate_frames();
        assert_eq!(frames.len(), 5);
        for (i, frame) in frames.iter().enumerate() {
            let t = i as f64 / 4.0;
            let FractalKind::Julia { seed } = frame.kind else { panic!("expected Julia") };
            assert!((seed[0] - t).abs() < 1e-9);
            assert!((seed[1] - (-2.0 * t)).abs() < 1e-9);
        }
    }

    #[test]
    fn multibrot_power_interpolates_linearly_across_frames() {
        let base = FractalParams { kind: FractalKind::Multibrot { power: 3.0 }, ..FractalParams::default() };
        let timeline =
            Timeline::two_point(base, (1.0, KindParam::Power(2.0)), (1.0, KindParam::Power(6.0)), 5, Easing::Linear);
        let frames = timeline.generate_frames();
        let powers: Vec<f64> = frames
            .iter()
            .map(|p| match p.kind {
                FractalKind::Multibrot { power } => power,
                _ => panic!("expected Multibrot"),
            })
            .collect();
        assert_eq!(powers, vec![2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn mandelbrot_kind_param_is_a_no_op() {
        // Mandelbrot has no free parameter — feeding it a Seed/Power (as if
        // by caller mistake) shouldn't panic or change the kind.
        let base = FractalParams::default();
        let timeline = Timeline::two_point(
            base,
            (1.0, KindParam::Seed([1.0, 2.0])),
            (2.0, KindParam::Seed([3.0, 4.0])),
            3,
            Easing::Linear,
        );
        for frame in timeline.generate_frames() {
            assert_eq!(frame.kind, FractalKind::Mandelbrot);
        }
    }

    #[test]
    fn deep_end_zoom_upgrades_center_precision_beyond_base() {
        // `base` is authored at a shallow zoom (low precision); an animation
        // zooming out to 1e100 needs far more mantissa bits than that to
        // resolve its reference orbit correctly at the deepest frames.
        let base = FractalParams {
            center: crate::hp::HpComplex::from_f64_pair(-0.75, 0.0, crate::hp::MIN_PRECISION_BITS),
            ..FractalParams::default()
        };
        let base_precision = base.center.precision_bits();
        let timeline =
            Timeline::two_point(base, (1.0, KindParam::None), (1.0e100, KindParam::None), 5, Easing::Linear);
        let frames = timeline.generate_frames();

        assert_eq!(frames[0].center.precision_bits(), crate::hp::MIN_PRECISION_BITS);
        let last = frames.last().unwrap();
        assert!(last.center.precision_bits() >= required_bits(last.zoom));
        assert!(last.center.precision_bits() > base_precision);
        // Precision only ever grows across the frame sequence, never shrinks.
        for pair in frames.windows(2) {
            assert!(pair[1].center.precision_bits() >= pair[0].center.precision_bits());
        }
    }

    #[test]
    fn single_frame_animation_also_gets_precision_upgrade() {
        let base = FractalParams {
            center: crate::hp::HpComplex::from_f64_pair(-0.75, 0.0, crate::hp::MIN_PRECISION_BITS),
            ..FractalParams::default()
        };
        let timeline =
            Timeline::two_point(base, (1.0e50, KindParam::None), (1.0e50, KindParam::None), 1, Easing::Linear);
        let frames = timeline.generate_frames();
        assert_eq!(frames.len(), 1);
        assert!(frames[0].center.precision_bits() >= required_bits(1.0e50));
    }
}
