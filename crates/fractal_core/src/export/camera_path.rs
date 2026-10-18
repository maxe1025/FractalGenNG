//! 3D camera-path animation: keyframed cameras joined by smooth splines.
//!
//! Positions follow a centripetal Catmull-Rom spline. Unlike the uniform
//! variant it never overshoots or loops when neighbouring keyframes are
//! spaced very differently — the normal case when a flight goes from an
//! overview of the whole fractal down into a crevice a thousand times
//! smaller, where an overshoot would fly straight into a wall. Yaw, pitch,
//! roll and field of view use a monotone cubic spline, so the camera never swings
//! past a keyframe's angle or drifts while two keyframes agree, with yaw
//! and roll unwrapped so they always turn the short way around.

use serde::{Deserialize, Serialize};

use super::animation::Easing;
use crate::fractal3d::{Camera3D, Formula3D, Params3D, MAX_PITCH};

/// One waypoint of a 3D animation: the camera at `frame` — and the
/// formula's parameters there, if keyed — plus the easing of the segment
/// leading into it from the previous keyframe (ignored on the first
/// keyframe).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraKeyframe {
    pub frame: u32,
    pub camera: Camera3D,
    /// The formula when this key was set. Formula parameters animate only
    /// when every key carries the same formula as the path's base (keys
    /// from before formulas were keyed, or from a different formula, leave
    /// the base formula unanimated).
    #[serde(default)]
    pub formula: Option<Formula3D>,
    pub easing: Easing,
}

/// A full 3D animation: `base` holds everything that stays fixed
/// (iterations, palette, ...), `keyframes` (sorted ascending by `frame`)
/// where the camera is when, and the formula's parameters along the way.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraPath {
    pub base: Params3D,
    pub keyframes: Vec<CameraKeyframe>,
}

impl CameraPath {
    /// The segment `keys[i] → keys[i + 1]` containing `frame` and the eased
    /// position `t ∈ [0, 1]` within it; `None` with fewer than 2 keys.
    fn segment(&self, frame: u32) -> Option<(usize, f64)> {
        let keys = &self.keyframes;
        if keys.len() < 2 {
            return None;
        }
        let i = segment_index(keys, frame);
        let (a, b) = (&keys[i], &keys[i + 1]);
        let span = f64::from(b.frame.saturating_sub(a.frame)).max(1.0);
        let t = (f64::from(frame.saturating_sub(a.frame)) / span).clamp(0.0, 1.0);
        Some((i, b.easing.remap(t)))
    }

    fn key_frames(&self) -> Vec<f64> {
        self.keyframes.iter().map(|k| f64::from(k.frame)).collect()
    }

    /// The camera at an arbitrary frame (clamped to the path's range).
    pub fn camera_at_frame(&self, frame: u32) -> Camera3D {
        let keys = &self.keyframes;
        let Some((i, t)) = self.segment(frame) else {
            return keys.first().map_or(self.base.camera, |k| k.camera);
        };

        // Neighbours for the spline; the ends are mirrored so the path
        // leaves the first and reaches the last keyframe without a kink.
        let p1 = keys[i].camera.position;
        let p2 = keys[i + 1].camera.position;
        let p0 = if i > 0 { keys[i - 1].camera.position } else { mirror(p2, p1) };
        let p3 = if i + 2 < keys.len() { keys[i + 2].camera.position } else { mirror(p1, p2) };
        let position = centripetal_catmull_rom(p0, p1, p2, p3, t);

        let frames = self.key_frames();
        let scalar = |values: Vec<f64>| monotone_cubic(&frames, &values, i, t);
        let yaw = scalar(unwrapped(keys.iter().map(|k| k.camera.yaw)));
        let pitch = scalar(keys.iter().map(|k| k.camera.pitch).collect());
        let roll = scalar(unwrapped(keys.iter().map(|k| k.camera.roll)));
        let fov = scalar(keys.iter().map(|k| k.camera.fov_y_degrees).collect());

        Camera3D {
            position,
            yaw: yaw.rem_euclid(std::f64::consts::TAU),
            pitch: pitch.clamp(-MAX_PITCH, MAX_PITCH),
            roll: (roll + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI,
            fov_y_degrees: fov.clamp(1.0, 179.0),
        }
    }

    /// Whether the keys animate the formula: all of them carry the base's
    /// formula kind with the same parameter layout (e.g. all in Julia mode
    /// or none), any values.
    pub fn animates_formula(&self) -> bool {
        let base = self.base.formula;
        let same_layout = |f: Formula3D| f.shader_id() == base.shader_id() && f.parameters().len() == base.parameters().len();
        !self.keyframes.is_empty() && self.keyframes.iter().all(|k| k.formula.is_some_and(same_layout))
    }

    /// The formula at an arbitrary frame: each parameter follows a monotone
    /// spline through the keys, so it never overshoots a keyed value.
    pub fn formula_at_frame(&self, frame: u32) -> Formula3D {
        if !self.animates_formula() {
            return self.base.formula;
        }
        let keys = &self.keyframes;
        let per_key: Vec<Vec<f64>> = keys.iter().filter_map(|k| k.formula.map(Formula3D::parameters)).collect();
        let Some((i, t)) = self.segment(frame) else {
            return keys[0].formula.unwrap_or(self.base.formula);
        };
        let frames = self.key_frames();
        let values: Vec<f64> = (0..per_key[0].len())
            .map(|p| monotone_cubic(&frames, &per_key.iter().map(|v| v[p]).collect::<Vec<_>>(), i, t))
            .collect();
        self.base.formula.with_parameters(&values)
    }

    /// The full render parameters at `frame`: `base` with the animated
    /// camera and formula.
    pub fn params_at_frame(&self, frame: u32) -> Params3D {
        Params3D { camera: self.camera_at_frame(frame), formula: self.formula_at_frame(frame), ..self.base.clone() }
    }

    /// The last frame index (0 for an empty path).
    pub fn last_frame(&self) -> u32 {
        self.keyframes.last().map_or(0, |k| k.frame)
    }

    /// One [`Params3D`] per frame, `0..=last_frame`.
    pub fn generate_frames(&self) -> Vec<Params3D> {
        if self.keyframes.is_empty() {
            return Vec::new();
        }
        (0..=self.last_frame()).map(|frame| self.params_at_frame(frame)).collect()
    }

    /// One [`Params3D`] per frame of `frames` — the render range, which may
    /// extend before the first or past the last keyframe (camera and
    /// formula hold there).
    pub fn frames_in_range(&self, frames: std::ops::RangeInclusive<u32>) -> Vec<Params3D> {
        frames.map(|frame| self.params_at_frame(frame)).collect()
    }

    /// Frames of `frames` at which the camera would sit inside the
    /// fractal's solid (clearance ≤ 0) — the spline between two free
    /// keyframes can cut through a wall (or an animated formula can grow
    /// over the camera), and those frames render as a screen full of
    /// surface.
    pub fn blocked_frames(&self, frames: std::ops::RangeInclusive<u32>) -> Vec<u32> {
        frames.filter(|&frame| self.params_at_frame(frame).camera_clearance() <= 0.0).collect()
    }
}

/// Index `i` of the segment `keys[i] → keys[i + 1]` containing `frame`.
/// Assumes at least 2 keys, sorted by frame.
fn segment_index(keys: &[CameraKeyframe], frame: u32) -> usize {
    let last_start = keys.len() - 2;
    keys.iter().rposition(|k| k.frame <= frame).unwrap_or(0).min(last_start)
}

/// Angles shifted by whole turns so consecutive ones differ by at most half
/// a turn — interpolating these turns the short way around.
fn unwrapped(angles: impl Iterator<Item = f64>) -> Vec<f64> {
    use std::f64::consts::{PI, TAU};
    let mut out: Vec<f64> = Vec::new();
    for angle in angles {
        let angle = match out.last() {
            Some(&previous) => previous + (angle - previous + PI).rem_euclid(TAU) - PI,
            None => angle,
        };
        out.push(angle);
    }
    out
}

fn mirror(about: [f64; 3], p: [f64; 3]) -> [f64; 3] {
    [2.0 * p[0] - about[0], 2.0 * p[1] - about[1], 2.0 * p[2] - about[2]]
}

/// Value at `t ∈ [0, 1]` within segment `i` of a monotone cubic Hermite
/// spline through `(xs[k], ys[k])` (Fritsch–Butland tangents): smooth
/// through every keyframe, but never overshooting — a value held between
/// two keyframes stays put even if the next segment changes it.
fn monotone_cubic(xs: &[f64], ys: &[f64], i: usize, t: f64) -> f64 {
    let n = xs.len();
    let h = |k: usize| (xs[k + 1] - xs[k]).max(1e-9);
    let slope = |k: usize| (ys[k + 1] - ys[k]) / h(k);
    let tangent = |k: usize| {
        if k == 0 {
            return slope(0);
        }
        if k == n - 1 {
            return slope(n - 2);
        }
        let (d0, d1) = (slope(k - 1), slope(k));
        if d0 * d1 <= 0.0 {
            return 0.0;
        }
        let (h0, h1) = (h(k - 1), h(k));
        let (w0, w1) = (2.0 * h1 + h0, h1 + 2.0 * h0);
        (w0 + w1) / (w0 / d0 + w1 / d1)
    };
    let (t2, t3) = (t * t, t * t * t);
    let span = h(i);
    (2.0 * t3 - 3.0 * t2 + 1.0) * ys[i]
        + (t3 - 2.0 * t2 + t) * span * tangent(i)
        + (-2.0 * t3 + 3.0 * t2) * ys[i + 1]
        + (t3 - t2) * span * tangent(i + 1)
}

/// Point at `t ∈ [0, 1]` between `p1` and `p2` on the centripetal
/// (α = 0.5) Catmull-Rom spline through `p0..p3`, by the Barry–Goldman
/// recursion.
fn centripetal_catmull_rom(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3], p3: [f64; 3], t: f64) -> [f64; 3] {
    let gap = |a: [f64; 3], b: [f64; 3]| {
        let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().sqrt()
    };
    let middle = gap(p1, p2);
    if middle == 0.0 {
        // A held position: stay put rather than drift along the neighbours.
        return p1;
    }
    // A zero-length outer gap (repeated keyframe) would divide by zero;
    // a tiny knot spacing degrades gracefully to the linear end tangent.
    let min_gap = middle * 1e-6;
    let t0 = 0.0;
    let t1 = t0 + gap(p0, p1).max(min_gap);
    let t2 = t1 + middle;
    let t3 = t2 + gap(p2, p3).max(min_gap);
    let u = t1 + (t2 - t1) * t;

    let lerp = |a: [f64; 3], b: [f64; 3], ta: f64, tb: f64| {
        let w = (u - ta) / (tb - ta);
        [a[0] + (b[0] - a[0]) * w, a[1] + (b[1] - a[1]) * w, a[2] + (b[2] - a[2]) * w]
    };
    let a1 = lerp(p0, p1, t0, t1);
    let a2 = lerp(p1, p2, t1, t2);
    let a3 = lerp(p2, p3, t2, t3);
    let b1 = lerp(a1, a2, t0, t2);
    let b2 = lerp(a2, a3, t1, t3);
    lerp(b1, b2, t1, t2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fractal3d::{MANDELBOX_DEFAULT, MANDELBULB_DEFAULT};

    fn cam(position: [f64; 3], yaw: f64) -> Camera3D {
        Camera3D { position, yaw, pitch: 0.0, roll: 0.0, fov_y_degrees: 60.0 }
    }

    fn key(frame: u32, camera: Camera3D) -> CameraKeyframe {
        CameraKeyframe { frame, camera, formula: None, easing: Easing::Linear }
    }

    fn path(keys: Vec<CameraKeyframe>) -> CameraPath {
        CameraPath { base: Params3D::new(MANDELBOX_DEFAULT), keyframes: keys }
    }

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    #[test]
    fn path_passes_exactly_through_every_keyframe() {
        let keys = vec![
            key(0, cam([0.0, 0.0, -5.0], 0.0)),
            key(10, cam([1.0, 0.5, -2.0], 0.4)),
            key(25, cam([0.01, 0.02, -1.6], 1.2)),
            key(40, cam([0.011, 0.02, -1.59], 1.0)),
        ];
        let path = path(keys.clone());
        for k in &keys {
            let c = path.camera_at_frame(k.frame);
            assert!(close(c.position, k.camera.position), "frame {}: {:?}", k.frame, c.position);
            assert!((c.yaw - k.camera.yaw).abs() < 1e-9);
        }
        assert_eq!(path.generate_frames().len(), 41);
    }

    #[test]
    fn no_overshoot_when_diving_from_large_to_tiny_segments() {
        // A big approach followed by a tiny final segment: a uniform spline
        // would swing far past the tiny segment; the centripetal one stays
        // close to it.
        let keys = vec![
            key(0, cam([0.0, 0.0, -10.0], 0.0)),
            key(50, cam([0.0, 0.0, -1.0], 0.0)),
            key(100, cam([0.001, 0.0, -0.999], 0.0)),
        ];
        let path = path(keys);
        for frame in 50..=100 {
            let p = path.camera_at_frame(frame).position;
            assert!(p[2] >= -1.0 - 1e-3 && p[2] <= -0.999 + 1e-3, "frame {frame}: {p:?}");
        }
    }

    #[test]
    fn held_keyframes_do_not_drift() {
        let still = cam([0.3, 0.2, -2.0], 0.5);
        let keys = vec![key(0, cam([0.0, 0.0, -5.0], 0.0)), key(10, still), key(20, still), key(30, cam([1.0, 1.0, 1.0], 0.0))];
        let path = path(keys);
        for frame in 10..=20 {
            assert!(close(path.camera_at_frame(frame).position, still.position), "frame {frame}");
        }
    }

    #[test]
    fn yaw_turns_the_short_way_across_the_seam() {
        let keys = vec![key(0, cam([0.0; 3], 6.2)), key(10, cam([0.0; 3], 0.1))];
        let path = path(keys);
        let mid = path.camera_at_frame(5).yaw;
        // Halfway between 6.2 and 0.1 + 2π is ~0.0 (mod 2π) — not ~3.15.
        let distance_to_zero = mid.min(std::f64::consts::TAU - mid);
        assert!(distance_to_zero < 0.1, "{mid}");
    }

    #[test]
    fn a_later_turn_does_not_make_an_earlier_held_angle_swing() {
        // Yaw 0 → 0 over frames 0–40, then a turn to 1.2: the first
        // segment must hold still rather than anticipate the turn.
        let keys = vec![
            key(0, cam([0.0, 0.0, -3.5], 0.0)),
            key(40, cam([0.0, 0.0, -0.5], 0.0)),
            key(80, cam([0.0, 0.0, 0.0], 1.2)),
        ];
        let path = path(keys);
        for frame in 0..=40 {
            let yaw = path.camera_at_frame(frame).yaw;
            assert!(yaw.min(std::f64::consts::TAU - yaw) < 1e-9, "frame {frame}: {yaw}");
        }
        // ...and the turn itself is monotone, never past its target.
        let mut previous = 0.0;
        for frame in 40..=80 {
            let yaw = path.camera_at_frame(frame).yaw;
            assert!(yaw >= previous - 1e-12 && yaw <= 1.2 + 1e-12, "frame {frame}: {yaw}");
            previous = yaw;
        }
    }

    #[test]
    fn roll_is_keyframed_and_tilts_the_short_way_round() {
        let mut a = cam([0.0, 0.0, -3.0], 0.0);
        let mut b = cam([0.0, 0.0, -2.0], 0.0);
        a.roll = 3.0; // just short of +π
        b.roll = -3.0; // just past −π: 0.28 rad further, not 6 back
        let path = path(vec![key(0, a), key(10, b)]);
        assert!((path.camera_at_frame(0).roll - 3.0).abs() < 1e-9);
        assert!((path.camera_at_frame(10).roll + 3.0).abs() < 1e-9);
        let mid = path.camera_at_frame(5).roll;
        assert!(mid.abs() > 3.0, "should pass through ±π, got {mid}");
    }

    #[test]
    fn identical_keyframes_produce_finite_cameras() {
        let c = cam([0.0, 0.0, -3.0], 0.0);
        let path = path(vec![key(0, c), key(30, c)]);
        for frame in 0..=30 {
            let p = path.camera_at_frame(frame);
            assert!(p.position.iter().all(|v| v.is_finite()) && p.yaw.is_finite());
            assert!(close(p.position, c.position));
        }
    }

    #[test]
    fn frames_keep_the_base_params_and_only_move_the_camera() {
        let path = path(vec![key(0, cam([0.0, 0.0, -5.0], 0.0)), key(4, cam([0.0, 0.0, -4.0], 0.0))]);
        for frame in path.generate_frames() {
            assert_eq!(frame.formula, path.base.formula);
            assert_eq!(frame.max_iterations, path.base.max_iterations);
        }
    }

    #[test]
    fn render_range_holds_the_camera_outside_the_keyframes() {
        let a = cam([0.0, 0.0, -5.0], 0.2);
        let b = cam([1.0, 0.0, -3.0], 0.4);
        let path = path(vec![key(10, a), key(20, b)]);
        let frames = path.frames_in_range(0..=30);
        assert_eq!(frames.len(), 31);
        assert!(close(frames[0].camera.position, a.position) && close(frames[10].camera.position, a.position));
        assert!(close(frames[30].camera.position, b.position));
        // An empty path renders the base camera.
        let empty = CameraPath { base: Params3D::new(MANDELBOX_DEFAULT), keyframes: Vec::new() };
        assert_eq!(empty.frames_in_range(0..=2)[1].camera, empty.base.camera);
    }

    #[test]
    fn keyed_formula_parameters_animate_smoothly() {
        use crate::fractal3d::QUATERNION_JULIA_DEFAULT;
        let julia = |c0: f64| Formula3D::QuaternionJulia { c: [c0, 0.2, 0.0, 0.0], slice: 0.0 };
        let mut base = Params3D::new(QUATERNION_JULIA_DEFAULT);
        base.formula = julia(-1.0);
        let c = cam([0.0, 0.0, -3.0], 0.0);
        let keyed = |frame, c0| CameraKeyframe { frame, camera: c, formula: Some(julia(c0)), easing: Easing::Linear };
        let path = CameraPath { base, keyframes: vec![keyed(0, -0.4), keyed(10, 0.0), keyed(20, -0.4)] };
        assert!(path.animates_formula());
        let c0 = |frame| match path.formula_at_frame(frame) {
            Formula3D::QuaternionJulia { c, .. } => c[0],
            other => panic!("formula changed kind: {other:?}"),
        };
        assert!((c0(0) + 0.4).abs() < 1e-12 && c0(10).abs() < 1e-12 && (c0(20) + 0.4).abs() < 1e-12);
        // Monotone between keys, and no overshoot past the peak at 0.
        for frame in 0..10 {
            assert!(c0(frame) <= c0(frame + 1) + 1e-12 && c0(frame) <= 1e-12, "frame {frame}");
        }
        // Held outside the keys, and carried into the full frame params.
        assert!((c0(30) + 0.4).abs() < 1e-12);
        assert_eq!(path.params_at_frame(10).formula, julia(0.0));
    }

    #[test]
    fn unkeyed_or_mismatched_formulas_leave_the_base_formula_alone() {
        use crate::fractal3d::{MANDELBULB_DEFAULT, QUATERNION_JULIA_DEFAULT};
        let c = cam([0.0, 0.0, -3.0], 0.0);
        let base = Params3D::new(QUATERNION_JULIA_DEFAULT);
        // A key without a formula (older project) and one of another kind.
        for other in [None, Some(MANDELBULB_DEFAULT)] {
            let keys = vec![
                CameraKeyframe { frame: 0, camera: c, formula: Some(QUATERNION_JULIA_DEFAULT), easing: Easing::Linear },
                CameraKeyframe { frame: 10, camera: c, formula: other, easing: Easing::Linear },
            ];
            let path = CameraPath { base: base.clone(), keyframes: keys };
            assert!(!path.animates_formula());
            assert_eq!(path.formula_at_frame(5), base.formula);
        }
    }

    #[test]
    fn julia_seeds_animate_but_mixed_julia_modes_do_not() {
        use crate::fractal3d::MANDELBULB_DEFAULT;
        let c = cam([0.0, 0.0, -3.0], 0.0);
        let bulb = |seed: Option<[f64; 3]>| MANDELBULB_DEFAULT.with_julia(seed);
        let keyed = |frame, formula| CameraKeyframe { frame, camera: c, formula: Some(formula), easing: Easing::Linear };

        let mut base = Params3D::new(MANDELBULB_DEFAULT);
        base.formula = bulb(Some([0.0; 3]));
        let path = CameraPath {
            base: base.clone(),
            keyframes: vec![keyed(0, bulb(Some([0.0, 0.0, 0.0]))), keyed(10, bulb(Some([0.4, 0.0, -0.2])))],
        };
        assert!(path.animates_formula());
        let mid = path.formula_at_frame(5).julia_seed().unwrap();
        assert!((mid[0] - 0.2).abs() < 1e-12 && (mid[2] + 0.1).abs() < 1e-12, "{mid:?}");

        // One key without Julia mode: different parameter layout, so the
        // formula stays fixed rather than mixing up parameters.
        let mixed = CameraPath { base, keyframes: vec![keyed(0, bulb(Some([0.0; 3]))), keyed(10, bulb(None))] };
        assert!(!mixed.animates_formula());
        assert_eq!(mixed.formula_at_frame(5), mixed.base.formula);
    }

    #[test]
    fn blocked_frames_flags_a_path_through_the_bulb() {
        let path = CameraPath {
            base: Params3D::new(MANDELBULB_DEFAULT),
            keyframes: vec![key(0, cam([0.0, 0.0, -3.0], 0.0)), key(20, cam([0.0, 0.0, 3.0], 0.0))],
        };
        let blocked = path.blocked_frames(0..=20);
        assert!(!blocked.is_empty());
        assert!(!blocked.contains(&0) && !blocked.contains(&20));
        assert!(blocked.contains(&10), "the midpoint is the bulb's center: {blocked:?}");
    }
}
