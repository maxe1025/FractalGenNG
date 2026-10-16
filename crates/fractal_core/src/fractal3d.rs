//! 3D fractals rendered by ray marching a distance estimator (DE): for a
//! point `p`, `DE(p)` is a lower bound on the distance from `p` to the
//! fractal's surface, so a ray can safely advance by `DE` each step until it
//! gets within one pixel's footprint of the surface.
//!
//! The DEs here are the f64 CPU twins of the ones in `shaders/raymarch.wgsl`
//! (which must stay in sync with them). The CPU side uses them to scale the
//! fly camera's speed to the free space around it and to keep it from flying
//! into the surface.

use serde::{Deserialize, Serialize};

use crate::palette::Palette;

/// Escape radius of the Mandelbulb iteration (the DE formula assumes 2).
const BULB_BAILOUT: f64 = 2.0;
/// Overflow guard for the Mandelbox iteration — far outside anything a
/// visible point reaches, so it doesn't change the DE in practice.
const BOX_BAILOUT_SQ: f64 = 1.0e8;

/// Which 3D distance-estimated formula to render. The GPU shader dispatches
/// on `shader_id()`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Formula3D {
    /// White/Nylander's power-`power` bulb, iterated in spherical coordinates.
    Mandelbulb { power: f64 },
    /// Tom Lowe's Mandelbox: a box fold, a sphere fold, then `z = scale·z + c`.
    /// Negative scales (around −1.5) give the cave-like interiors best suited
    /// to flying through.
    Mandelbox { scale: f64, min_radius: f64, fixed_radius: f64 },
}

pub const MANDELBULB_DEFAULT: Formula3D = Formula3D::Mandelbulb { power: 8.0 };
pub const MANDELBOX_DEFAULT: Formula3D = Formula3D::Mandelbox { scale: -1.5, min_radius: 0.5, fixed_radius: 1.0 };

impl Formula3D {
    pub fn shader_id(self) -> u32 {
        match self {
            Formula3D::Mandelbulb { .. } => 0,
            Formula3D::Mandelbox { .. } => 1,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Formula3D::Mandelbulb { .. } => "Mandelbulb",
            Formula3D::Mandelbox { .. } => "Mandelbox",
        }
    }

    /// Radius of a sphere around the origin that contains the whole
    /// fractal, used to cap how far a ray marches.
    pub fn bounding_radius(self) -> f64 {
        match self {
            Formula3D::Mandelbulb { .. } => 1.5,
            Formula3D::Mandelbox { scale, .. } => {
                let s = scale.abs();
                // The Mandelbox fits in a cube of half-size 2(|s|+1)/(|s|−1);
                // that blows up as |s| → 1, where the set itself does too.
                if s > 1.01 { (2.0 * (s + 1.0) / (s - 1.0) * 3f64.sqrt()).min(1000.0) } else { 1000.0 }
            }
        }
    }

    /// A camera placed outside the fractal, looking at its center.
    pub fn default_camera(self) -> Camera3D {
        let distance = match self {
            Formula3D::Mandelbulb { .. } => 2.6,
            // Positive scales fill the whole 2(s+1)/(s−1) cube; negative
            // ones fold back into a body only about ±2 wide.
            Formula3D::Mandelbox { scale, .. } if scale > 1.01 => 2.0 * (scale + 1.0) / (scale - 1.0) * 1.8,
            Formula3D::Mandelbox { .. } => 4.4,
        };
        Camera3D { position: [0.0, 0.0, -distance], yaw: 0.0, pitch: 0.0, fov_y_degrees: 60.0 }
    }

    /// `(distance estimate, orbit trap)` at `p`. The orbit trap is the
    /// smallest `|z|²` the iteration reached, used for coloring.
    pub fn estimate(self, p: [f64; 3], max_iterations: u32) -> (f64, f64) {
        match self {
            Formula3D::Mandelbulb { power } => mandelbulb_de(p, power, max_iterations),
            Formula3D::Mandelbox { scale, min_radius, fixed_radius } => {
                mandelbox_de(p, scale, min_radius, fixed_radius, max_iterations)
            }
        }
    }

    pub fn distance_estimate(self, p: [f64; 3], max_iterations: u32) -> f64 {
        self.estimate(p, max_iterations).0
    }
}

fn mandelbulb_de(p: [f64; 3], power: f64, max_iterations: u32) -> (f64, f64) {
    let mut z = p;
    let mut dr = 1.0;
    let mut r = length(z);
    let mut trap = f64::MAX;
    for _ in 0..max_iterations {
        r = length(z);
        if r > BULB_BAILOUT || r < 1e-12 {
            break;
        }
        let theta = (z[2] / r).clamp(-1.0, 1.0).acos() * power;
        let phi = z[1].atan2(z[0]) * power;
        dr = r.powf(power - 1.0) * power * dr + 1.0;
        let zr = r.powf(power);
        z = [
            zr * theta.sin() * phi.cos() + p[0],
            zr * theta.sin() * phi.sin() + p[1],
            zr * theta.cos() + p[2],
        ];
        trap = trap.min(dot(z, z));
    }
    (0.5 * r.max(1e-12).ln() * r / dr, trap)
}

fn mandelbox_de(p: [f64; 3], scale: f64, min_radius: f64, fixed_radius: f64, max_iterations: u32) -> (f64, f64) {
    let min_r2 = min_radius * min_radius;
    let fixed_r2 = fixed_radius * fixed_radius;
    let mut z = p;
    let mut dr = 1.0;
    let mut trap = f64::MAX;
    for _ in 0..max_iterations {
        for c in &mut z {
            *c = c.clamp(-1.0, 1.0) * 2.0 - *c;
        }
        let r2 = dot(z, z);
        let k = if r2 < min_r2 {
            fixed_r2 / min_r2
        } else if r2 < fixed_r2 {
            fixed_r2 / r2
        } else {
            1.0
        };
        dr = dr * k * scale.abs() + 1.0;
        for (zc, pc) in z.iter_mut().zip(p) {
            *zc = *zc * k * scale + pc;
        }
        trap = trap.min(r2);
        if dot(z, z) > BOX_BAILOUT_SQ {
            break;
        }
    }
    (length(z) / dr.abs(), trap)
}

/// A fly camera: position plus yaw (around world +Y) and pitch (up/down),
/// no roll. Yaw 0 / pitch 0 looks along +Z; world up is +Y.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Camera3D {
    pub position: [f64; 3],
    /// Radians.
    pub yaw: f64,
    /// Radians, kept within ±[`MAX_PITCH`].
    pub pitch: f64,
    pub fov_y_degrees: f64,
}

/// Just short of straight up/down, where the yaw axis would degenerate.
pub const MAX_PITCH: f64 = 1.55;

impl Camera3D {
    pub fn forward(&self) -> [f64; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        [cp * sy, sp, cp * cy]
    }

    /// Screen-right, perpendicular to forward and world up.
    pub fn right(&self) -> [f64; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        [cy, 0.0, -sy]
    }

    /// Screen-up, perpendicular to forward and right.
    pub fn up(&self) -> [f64; 3] {
        cross(self.forward(), self.right())
    }

    /// Turns by mouse-look deltas (radians), clamping pitch.
    pub fn rotate(&mut self, delta_yaw: f64, delta_pitch: f64) {
        self.yaw = (self.yaw + delta_yaw).rem_euclid(std::f64::consts::TAU);
        self.pitch = (self.pitch + delta_pitch).clamp(-MAX_PITCH, MAX_PITCH);
    }
}

/// A complete, serializable description of one 3D render — the 3D
/// counterpart of [`crate::FractalParams`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Params3D {
    pub formula: Formula3D,
    pub camera: Camera3D,
    /// Fractal iterations per distance estimate.
    pub max_iterations: u32,
    /// Ray-march steps per pixel before giving up.
    pub max_steps: u32,
    /// Surface hit threshold, in pixel footprints: a ray stops once it is
    /// within `detail` pixels (projected at its current distance) of the
    /// surface. Lower = finer detail, slower.
    pub detail: f64,
    /// Fraction of the DE a ray advances per step (< 1 compensates for
    /// DEs that overestimate, like the Mandelbulb's).
    pub step_factor: f64,
    /// Fog reaches ~63% at this many times the camera's own distance to the
    /// surface — scale-invariant, so it reads the same however far into a
    /// crevice the camera has flown. 0 disables fog.
    pub fog_distance: f64,
    /// The palette colors the surface by orbit trap; its `interior_color`
    /// is the background/fog color, and `distance_shading` is unused.
    pub palette: Palette,
}

impl Params3D {
    pub fn new(formula: Formula3D) -> Self {
        Self {
            formula,
            camera: formula.default_camera(),
            max_iterations: 12,
            max_steps: 300,
            detail: 1.0,
            step_factor: match formula {
                Formula3D::Mandelbulb { .. } => 0.8,
                Formula3D::Mandelbox { .. } => 0.9,
            },
            fog_distance: 100.0,
            palette: Palette::ocean(),
        }
    }

    /// DE at the camera — its clearance to the surface.
    pub fn camera_clearance(&self) -> f64 {
        self.formula.distance_estimate(self.camera.position, self.max_iterations)
    }

    /// Furthest a ray needs to march: past the far side of the bounding sphere.
    pub fn max_distance(&self) -> f64 {
        length(self.camera.position) + 2.0 * self.formula.bounding_radius()
    }

    /// Moves the camera by `delta` (world units) without letting it fly into
    /// the surface: a single move may cover at most half the current
    /// clearance (the DE guarantees nothing is closer), so the camera can
    /// approach the surface arbitrarily closely but never cross it. When the
    /// camera is already inside the solid (DE ≤ 0, e.g. after changing a
    /// formula parameter), moves are unrestricted so it can get back out.
    pub fn move_camera(&mut self, delta: [f64; 3]) {
        let clearance = self.camera_clearance();
        let len = length(delta);
        let max_len = 0.5 * clearance;
        let scale = if clearance > 0.0 && len > max_len { max_len / len } else { 1.0 };
        for (pos, d) in self.camera.position.iter_mut().zip(delta) {
            *pos += d * scale;
        }
    }
}

impl Default for Params3D {
    fn default() -> Self {
        Self::new(MANDELBOX_DEFAULT)
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn length(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: [f64; 3], b: [f64; 3]) {
        for i in 0..3 {
            assert!((a[i] - b[i]).abs() < 1e-12, "{a:?} != {b:?}");
        }
    }

    #[test]
    fn camera_basis_is_orthonormal_and_right_handed_screen() {
        let mut cam = Camera3D { position: [0.0; 3], yaw: 0.0, pitch: 0.0, fov_y_degrees: 60.0 };
        assert_close(cam.forward(), [0.0, 0.0, 1.0]);
        assert_close(cam.right(), [1.0, 0.0, 0.0]);
        assert_close(cam.up(), [0.0, 1.0, 0.0]);

        cam.rotate(0.7, -0.4);
        let (f, r, u) = (cam.forward(), cam.right(), cam.up());
        for v in [f, r, u] {
            assert!((length(v) - 1.0).abs() < 1e-12);
        }
        assert!(dot(f, r).abs() < 1e-12 && dot(f, u).abs() < 1e-12 && dot(r, u).abs() < 1e-12);
        assert!(u[1] > 0.0, "up keeps pointing upward while pitch is within limits");
    }

    #[test]
    fn pitch_is_clamped() {
        let mut cam = Formula3D::default_camera(MANDELBULB_DEFAULT);
        cam.rotate(0.0, 10.0);
        assert_eq!(cam.pitch, MAX_PITCH);
    }

    #[test]
    fn default_cameras_start_outside_the_fractal_with_room_to_move() {
        for formula in [MANDELBULB_DEFAULT, MANDELBOX_DEFAULT] {
            let params = Params3D::new(formula);
            assert!(params.camera_clearance() > 0.1, "{}: {}", formula.display_name(), params.camera_clearance());
        }
    }

    #[test]
    fn de_is_a_lower_bound_far_from_the_bulb() {
        // The bulb lies within radius ~1.2; at distance 5 the true distance
        // is >= 3.8, and the DE must not claim more than the true distance
        // to the origin-side bound, nor be wildly smaller.
        let de = MANDELBULB_DEFAULT.distance_estimate([0.0, 0.0, 5.0], 12);
        assert!(de > 1.0 && de < 5.0, "{de}");
    }

    #[test]
    fn points_inside_the_solid_have_no_clearance() {
        // Both fractals contain the origin's neighbourhood.
        assert!(MANDELBULB_DEFAULT.distance_estimate([0.0, 0.0, 0.1], 12) <= 1e-3);
        assert!(MANDELBOX_DEFAULT.distance_estimate([0.05, 0.02, 0.01], 12) <= 1e-3);
    }

    #[test]
    fn moving_never_crosses_into_the_surface() {
        let mut params = Params3D::new(MANDELBULB_DEFAULT);
        // Fly straight at the bulb with huge steps.
        for _ in 0..200 {
            params.move_camera([0.0, 0.0, 10.0]);
            assert!(params.camera_clearance() > 0.0, "camera entered the solid at {:?}", params.camera.position);
            // The bulb's near pole is at z ≈ −1.1; tunnelling would put
            // the camera on the far side instead.
            assert!(params.camera.position[2] < -1.0, "camera passed through to {:?}", params.camera.position);
        }
        assert!(params.camera_clearance() < 1e-3, "camera should have closed in on the surface");
    }

    #[test]
    fn moving_away_is_not_restricted_more_than_half_the_clearance() {
        let mut params = Params3D::new(MANDELBULB_DEFAULT);
        let before = params.camera.position[2];
        let clearance = params.camera_clearance();
        params.move_camera([0.0, 0.0, -0.1 * clearance]);
        assert!((params.camera.position[2] - (before - 0.1 * clearance)).abs() < 1e-12);
    }
}
