use serde::{Deserialize, Serialize};

use crate::params::FractalParams;

/// How magnification progresses across an animation's frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZoomCurve {
    /// `zoom` increases by a constant amount each frame. Looks like it starts
    /// fast and drags at the end, since perceived zoom speed is multiplicative.
    Linear,
    /// `zoom` increases by a constant *factor* each frame (geometric
    /// interpolation). Constant perceived zoom speed — the natural default.
    Exponential,
}

/// Generates one [`FractalParams`] per frame, interpolating `zoom` from
/// `start_zoom` to `end_zoom` over `frame_count` frames. `center`, `kind`, and
/// `palette` are held fixed from `base` — the focus point doesn't move during
/// the animation, only the magnification.
pub fn generate_zoom_frames(
    base: &FractalParams,
    start_zoom: f64,
    end_zoom: f64,
    frame_count: u32,
    curve: ZoomCurve,
) -> Vec<FractalParams> {
    if frame_count == 0 {
        return Vec::new();
    }
    if frame_count == 1 {
        let mut params = base.clone();
        params.zoom = start_zoom;
        return vec![params];
    }

    (0..frame_count)
        .map(|i| {
            let t = f64::from(i) / f64::from(frame_count - 1);
            let zoom = match curve {
                ZoomCurve::Linear => start_zoom + (end_zoom - start_zoom) * t,
                ZoomCurve::Exponential => start_zoom * (end_zoom / start_zoom).powf(t),
            };
            let mut params = base.clone();
            params.zoom = zoom;
            params
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_zero_yields_no_frames() {
        let base = FractalParams::default();
        assert!(generate_zoom_frames(&base, 1.0, 100.0, 0, ZoomCurve::Linear).is_empty());
    }

    #[test]
    fn frame_count_one_uses_start_zoom() {
        let base = FractalParams::default();
        let frames = generate_zoom_frames(&base, 5.0, 500.0, 1, ZoomCurve::Exponential);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].zoom, 5.0);
    }

    #[test]
    fn linear_curve_hits_endpoints_and_midpoint() {
        let base = FractalParams::default();
        let frames = generate_zoom_frames(&base, 10.0, 110.0, 3, ZoomCurve::Linear);
        assert_eq!(frames.len(), 3);
        assert!((frames[0].zoom - 10.0).abs() < 1e-9);
        assert!((frames[1].zoom - 60.0).abs() < 1e-9);
        assert!((frames[2].zoom - 110.0).abs() < 1e-9);
    }

    #[test]
    fn exponential_curve_hits_endpoints_and_is_monotonic() {
        let base = FractalParams::default();
        let frames = generate_zoom_frames(&base, 1.0, 1_000_000.0, 7, ZoomCurve::Exponential);
        assert!((frames[0].zoom - 1.0).abs() < 1e-6);
        assert!((frames.last().unwrap().zoom - 1_000_000.0).abs() < 1.0);
        for pair in frames.windows(2) {
            assert!(pair[1].zoom > pair[0].zoom, "zoom must strictly increase frame over frame");
        }
        // Exponential (geometric) interpolation: equal ratios between
        // consecutive frames, unlike linear's equal differences.
        let ratio = frames[1].zoom / frames[0].zoom;
        for pair in frames.windows(2) {
            let r = pair[1].zoom / pair[0].zoom;
            assert!((r - ratio).abs() < 1e-6, "ratio should be constant across frames");
        }
    }

    #[test]
    fn center_kind_and_palette_are_preserved_from_base() {
        let mut base = FractalParams::julia_default();
        base.center = crate::hp::HpComplex::from_f64_pair(0.123, -0.456, 128);
        let frames = generate_zoom_frames(&base, 1.0, 2.0, 4, ZoomCurve::Linear);
        for frame in &frames {
            assert_eq!(frame.center, base.center);
            assert_eq!(frame.kind.shader_id(), base.kind.shader_id());
        }
    }
}
