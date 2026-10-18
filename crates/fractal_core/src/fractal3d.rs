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
/// Ray-length cap for formulas that fill all of space (the pseudo-Kleinian),
/// and for inverted space, whose fractal reaches out to infinity.
const UNBOUNDED_RADIUS: f64 = 1000.0;

/// Which 3D distance-estimated formula to render. The GPU shader dispatches
/// on `shader_id()`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Formula3D {
    /// White/Nylander's power-`power` bulb, iterated in spherical coordinates.
    /// With a `julia` seed, `c` is that fixed point instead of the point
    /// being rendered — the bulb's Julia sets ("Juliabulbs").
    Mandelbulb {
        power: f64,
        #[serde(default)]
        julia: Option<[f64; 3]>,
    },
    /// Tom Lowe's Mandelbox: a box fold, a sphere fold, then `z = scale·z + c`.
    /// Negative scales (around −1.5) give the cave-like interiors best suited
    /// to flying through. With a `julia` seed, `c` is that fixed point.
    Mandelbox {
        scale: f64,
        min_radius: f64,
        fixed_radius: f64,
        #[serde(default)]
        julia: Option<[f64; 3]>,
    },
    /// A kaleidoscopic IFS: fold into one octant, sort the coordinates,
    /// rotate, then scale away from `offset`; the base shape is a cube. The
    /// defaults (scale 3, offset 1, no rotation) give exactly the Menger
    /// sponge — a grid of square corridors — and the rotation angles twist
    /// it into ornamental architecture.
    Kifs { scale: f64, offset: [f64; 3], rotation_degrees: [f64; 2] },
    /// Knighty's pseudo-Kleinian: box folds plus a sphere inversion that
    /// only ever shrinks, tiling all of space with organic tunnels — the
    /// camera is always inside it.
    PseudoKleinian { box_size: [f64; 3], size: f64, thickness: f64 },
    /// The quaternion Julia set: `z → z² + c` with `z` and `c` quaternions,
    /// sliced into 3D at `z₀ = (x, y, z, slice)`. Smooth, organic bodies
    /// that flow into each other as `c` changes.
    QuaternionJulia { c: [f64; 4], slice: f64 },
}

pub const MANDELBULB_DEFAULT: Formula3D = Formula3D::Mandelbulb { power: 8.0, julia: None };
pub const QUATERNION_JULIA_DEFAULT: Formula3D =
    Formula3D::QuaternionJulia { c: [-0.291, -0.399, 0.339, 0.437], slice: 0.0 };
pub const MANDELBOX_DEFAULT: Formula3D =
    Formula3D::Mandelbox { scale: -1.5, min_radius: 0.5, fixed_radius: 1.0, julia: None };
/// Seeds that give a good first look when Julia mode is switched on (see
/// [`Formula3D::with_julia`]).
pub const MANDELBULB_JULIA_SEED: [f64; 3] = [-0.6, 0.6, 0.4];
pub const MANDELBOX_JULIA_SEED: [f64; 3] = [1.2, -0.6, 0.4];
pub const KIFS_DEFAULT: Formula3D =
    Formula3D::Kifs { scale: 3.0, offset: [1.0, 1.0, 1.0], rotation_degrees: [0.0, 0.0] };
pub const PSEUDO_KLEINIAN_DEFAULT: Formula3D =
    Formula3D::PseudoKleinian { box_size: [0.92436, 0.90756, 0.92436], size: 1.0, thickness: 0.3 };

impl Formula3D {
    pub fn shader_id(self) -> u32 {
        match self {
            Formula3D::Mandelbulb { .. } => 0,
            Formula3D::Mandelbox { .. } => 1,
            Formula3D::Kifs { .. } => 2,
            Formula3D::PseudoKleinian { .. } => 3,
            Formula3D::QuaternionJulia { .. } => 4,
        }
    }

    /// The formula's numeric parameters, flattened — for interpolating
    /// between keyframes. [`Self::with_parameters`] is the inverse. A Julia
    /// seed adds three values, so two formulas only interpolate when they
    /// agree on Julia mode (same length).
    pub fn parameters(self) -> Vec<f64> {
        let with_seed = |mut v: Vec<f64>, julia: Option<[f64; 3]>| {
            v.extend(julia.into_iter().flatten());
            v
        };
        match self {
            Formula3D::Mandelbulb { power, julia } => with_seed(vec![power], julia),
            Formula3D::Mandelbox { scale, min_radius, fixed_radius, julia } => {
                with_seed(vec![scale, min_radius, fixed_radius], julia)
            }
            Formula3D::Kifs { scale, offset, rotation_degrees } => {
                vec![scale, offset[0], offset[1], offset[2], rotation_degrees[0], rotation_degrees[1]]
            }
            Formula3D::PseudoKleinian { box_size, size, thickness } => {
                vec![box_size[0], box_size[1], box_size[2], size, thickness]
            }
            Formula3D::QuaternionJulia { c, slice } => vec![c[0], c[1], c[2], c[3], slice],
        }
    }

    /// This formula with its parameters replaced by `v` (as laid out by
    /// [`Self::parameters`]); a too-short `v` leaves the formula unchanged.
    pub fn with_parameters(self, v: &[f64]) -> Self {
        if v.len() < self.parameters().len() {
            return self;
        }
        // The seed, if this formula has one, follows its own parameters.
        let seed = |julia: Option<[f64; 3]>, at: usize| julia.map(|_| [v[at], v[at + 1], v[at + 2]]);
        match self {
            Formula3D::Mandelbulb { julia, .. } => Formula3D::Mandelbulb { power: v[0], julia: seed(julia, 1) },
            Formula3D::Mandelbox { julia, .. } => {
                Formula3D::Mandelbox { scale: v[0], min_radius: v[1], fixed_radius: v[2], julia: seed(julia, 3) }
            }
            Formula3D::Kifs { .. } => {
                Formula3D::Kifs { scale: v[0], offset: [v[1], v[2], v[3]], rotation_degrees: [v[4], v[5]] }
            }
            Formula3D::PseudoKleinian { .. } => {
                Formula3D::PseudoKleinian { box_size: [v[0], v[1], v[2]], size: v[3], thickness: v[4] }
            }
            Formula3D::QuaternionJulia { .. } => Formula3D::QuaternionJulia { c: [v[0], v[1], v[2], v[3]], slice: v[4] },
        }
    }

    /// Half-size of the cube a Mandelbox fits in, or `None` when |scale| is
    /// too close to 1 for it to be bounded. A coordinate beyond `h` grows
    /// every iteration once |s|·(h − 2) − |c| > h: with `c` the point
    /// itself that gives the classic 2(|s|+1)/(|s|−1), with a fixed Julia
    /// seed (2|s| + |c|)/(|s|−1).
    fn mandelbox_half_size(self) -> Option<f64> {
        let Formula3D::Mandelbox { scale, julia, .. } = self else { return None };
        let s = scale.abs();
        if s <= 1.01 {
            return None;
        }
        Some(match julia {
            None => 2.0 * (s + 1.0) / (s - 1.0),
            Some(seed) => (2.0 * s + length(seed)) / (s - 1.0),
        })
    }

    /// Whether this formula adds the rendered point as `c` each iteration,
    /// and so has a Julia variant with a fixed seed.
    pub fn supports_julia(self) -> bool {
        matches!(self, Formula3D::Mandelbulb { .. } | Formula3D::Mandelbox { .. })
    }

    pub fn julia_seed(self) -> Option<[f64; 3]> {
        match self {
            Formula3D::Mandelbulb { julia, .. } | Formula3D::Mandelbox { julia, .. } => julia,
            _ => None,
        }
    }

    /// Switches Julia mode on (with the given seed) or off; formulas
    /// without a Julia variant are returned unchanged.
    pub fn with_julia(self, seed: Option<[f64; 3]>) -> Self {
        match self {
            Formula3D::Mandelbulb { power, .. } => Formula3D::Mandelbulb { power, julia: seed },
            Formula3D::Mandelbox { scale, min_radius, fixed_radius, .. } => {
                Formula3D::Mandelbox { scale, min_radius, fixed_radius, julia: seed }
            }
            other => other,
        }
    }

    /// A seed that gives a good first look when Julia mode is switched on.
    pub fn default_julia_seed(self) -> Option<[f64; 3]> {
        match self {
            Formula3D::Mandelbulb { .. } => Some(MANDELBULB_JULIA_SEED),
            Formula3D::Mandelbox { .. } => Some(MANDELBOX_JULIA_SEED),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Formula3D::Mandelbulb { .. } => "Mandelbulb",
            Formula3D::Mandelbox { .. } => "Mandelbox",
            Formula3D::Kifs { .. } => "Menger / KIFS",
            Formula3D::PseudoKleinian { .. } => "Pseudo-Kleinian",
            Formula3D::QuaternionJulia { .. } => "Quaternion Julia",
        }
    }

    /// Radius of a sphere around the origin that contains the whole
    /// fractal, used to cap how far a ray marches.
    pub fn bounding_radius(self) -> f64 {
        match self {
            Formula3D::Mandelbulb { julia: None, .. } => 1.5,
            // Julia sets reach out to the iteration's escape radius (2).
            Formula3D::Mandelbulb { julia: Some(_), .. } => BULB_BAILOUT,
            Formula3D::Mandelbox { .. } => match self.mandelbox_half_size() {
                Some(half) => (half * 3f64.sqrt()).min(UNBOUNDED_RADIUS),
                None => UNBOUNDED_RADIUS,
            },
            // The attractor stays within the offset's reach of the origin;
            // doubled for the slack rotation adds.
            Formula3D::Kifs { offset, .. } => 2.0 * length(offset).max(1.0),
            Formula3D::PseudoKleinian { .. } => UNBOUNDED_RADIUS,
            // Every orbit starting beyond R = (1 + √(1 + 4|c|))/2 escapes
            // (|z|² − |c| > |z| there), so the set lies within it.
            Formula3D::QuaternionJulia { c, .. } => {
                let c_len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2] + c[3] * c[3]).sqrt();
                (1.0 + (1.0 + 4.0 * c_len).sqrt()) / 2.0 + 0.1
            }
        }
    }

    /// A camera with a good first view: outside the bounded fractals looking
    /// at their center (the KIFS straight down its central corridor), inside
    /// a tunnel for the space-filling pseudo-Kleinian.
    pub fn default_camera(self) -> Camera3D {
        let position = match self {
            Formula3D::Mandelbulb { .. } => [0.0, 0.0, -2.6],
            // Positive scales fill the whole 2(s+1)/(s−1) cube; negative
            // ones fold back into a body only about ±2 wide.
            // A Julia Mandelbox fills most of its cube, so step back from it.
            Formula3D::Mandelbox { julia: Some(_), .. } => {
                [0.0, 0.0, -1.6 * self.mandelbox_half_size().unwrap_or(4.0)]
            }
            Formula3D::Mandelbox { scale, .. } if scale > 1.01 => {
                [0.0, 0.0, -2.0 * (scale + 1.0) / (scale - 1.0) * 1.8]
            }
            Formula3D::Mandelbox { .. } => [0.0, 0.0, -4.4],
            Formula3D::Kifs { offset, .. } => [0.0, 0.0, -3.5 * offset[2].abs().max(0.5)],
            Formula3D::PseudoKleinian { .. } => PSEUDO_KLEINIAN_START,
            Formula3D::QuaternionJulia { .. } => [0.0, 0.0, -3.0],
        };
        Camera3D { position, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_y_degrees: 60.0 }
    }

    /// `(distance estimate, orbit trap)` at `p`. The orbit trap is the
    /// smallest `|z|²` the iteration reached, used for coloring.
    pub fn estimate(self, p: [f64; 3], max_iterations: u32) -> (f64, f64) {
        match self {
            Formula3D::Mandelbulb { power, julia } => mandelbulb_de(p, power, julia, max_iterations),
            Formula3D::Mandelbox { scale, min_radius, fixed_radius, julia } => {
                mandelbox_de(p, scale, min_radius, fixed_radius, julia, max_iterations)
            }
            Formula3D::Kifs { scale, offset, rotation_degrees } => {
                kifs_de(p, scale, offset, kifs_rotation(rotation_degrees), max_iterations)
            }
            Formula3D::PseudoKleinian { box_size, size, thickness } => {
                pseudo_kleinian_de(p, box_size, size, thickness, max_iterations)
            }
            Formula3D::QuaternionJulia { c, slice } => quaternion_julia_de(p, c, slice, max_iterations),
        }
    }

    pub fn distance_estimate(self, p: [f64; 3], max_iterations: u32) -> f64 {
        self.estimate(p, max_iterations).0
    }
}

/// A free spot inside one of the pseudo-Kleinian's tunnels.
const PSEUDO_KLEINIAN_START: [f64; 3] = [0.9, 0.0, 0.3];

/// Sphere inversion of space: `p → center + radius²·(p − center)/|p − center|²`.
/// With `center` inside the fractal's solid, the infinite space around the
/// fractal maps to a closed cave around `center`, walled in by the fractal's
/// surface turned inside out.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Inversion {
    pub center: [f64; 3],
    pub radius: f64,
}

impl Inversion {
    /// Radius of the ball around `center` that the inverted fractal can't
    /// reach: the whole fractal lies within `|center| + bounding radius` of
    /// the center, so its image lies at least `R²` over that away.
    pub fn free_radius(self, formula: Formula3D) -> f64 {
        self.radius * self.radius / (length(self.center) + formula.bounding_radius())
    }

    /// `(distance estimate, orbit trap)` at `p` in inverted space: the
    /// formula's estimate at the inverted point, scaled back by the
    /// inversion's local stretch (`r²·DE / (R² + r·DE)`, with `r` the
    /// distance from the center). That estimate degenerates to `r/2` near
    /// the center (which maps to infinity), stalling rays aimed at it, so
    /// it is floored by the exact clearance [`Self::free_radius`] − `r`.
    pub fn estimate(self, formula: Formula3D, p: [f64; 3], max_iterations: u32) -> (f64, f64) {
        let d = sub(p, self.center);
        let r2 = dot(d, d).max(1e-30);
        let r = r2.sqrt();
        let rr = self.radius * self.radius;
        let inverted = [
            self.center[0] + d[0] * rr / r2,
            self.center[1] + d[1] * rr / r2,
            self.center[2] + d[2] * rr / r2,
        ];
        let (de, trap) = formula.estimate(inverted, max_iterations);
        let de = (r2 * de / (rr + r * de.abs())).max(self.free_radius(formula) - r);
        (de, trap)
    }

    /// Where the camera starts inside the cave: a little behind the
    /// center, looking across it (the center itself maps to infinity).
    pub fn default_camera(self) -> Camera3D {
        let mut position = self.center;
        position[2] -= 0.3 * self.radius;
        Camera3D { position, yaw: 0.0, pitch: 0.0, roll: 0.0, fov_y_degrees: 75.0 }
    }
}

/// Per-iteration KIFS rotation, as matrix rows: `Rz(z°) · Rx(x°)`.
pub fn kifs_rotation(rotation_degrees: [f64; 2]) -> [[f64; 3]; 3] {
    let (sa, ca) = rotation_degrees[0].to_radians().sin_cos();
    let (sb, cb) = rotation_degrees[1].to_radians().sin_cos();
    [[cb, -sb * ca, sb * sa], [sb, cb * ca, -cb * sa], [0.0, sa, ca]]
}

/// The `c` added each iteration, and its derivative with respect to the
/// point: the point itself (derivative 1) for the classic sets, or a fixed
/// Julia seed (derivative 0), which drops the `+ 1` from the DE recurrence.
fn julia_addend(p: [f64; 3], julia: Option<[f64; 3]>) -> ([f64; 3], f64) {
    match julia {
        Some(seed) => (seed, 0.0),
        None => (p, 1.0),
    }
}

fn mandelbulb_de(p: [f64; 3], power: f64, julia: Option<[f64; 3]>, max_iterations: u32) -> (f64, f64) {
    let (c, dc) = julia_addend(p, julia);
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
        dr = r.powf(power - 1.0) * power * dr + dc;
        let zr = r.powf(power);
        z = [
            zr * theta.sin() * phi.cos() + c[0],
            zr * theta.sin() * phi.sin() + c[1],
            zr * theta.cos() + c[2],
        ];
        trap = trap.min(dot(z, z));
    }
    (0.5 * r.max(1e-12).ln() * r / dr, trap)
}

fn mandelbox_de(
    p: [f64; 3],
    scale: f64,
    min_radius: f64,
    fixed_radius: f64,
    julia: Option<[f64; 3]>,
    max_iterations: u32,
) -> (f64, f64) {
    let (c, dc) = julia_addend(p, julia);
    let min_r2 = min_radius * min_radius;
    let fixed_r2 = fixed_radius * fixed_radius;
    let mut z = p;
    let mut dr = 1.0;
    let mut trap = f64::MAX;
    for _ in 0..max_iterations {
        for x in &mut z {
            *x = x.clamp(-1.0, 1.0) * 2.0 - *x;
        }
        let r2 = dot(z, z);
        let k = if r2 < min_r2 {
            fixed_r2 / min_r2
        } else if r2 < fixed_r2 {
            fixed_r2 / r2
        } else {
            1.0
        };
        dr = dr * k * scale.abs() + dc;
        for (zc, cc) in z.iter_mut().zip(c) {
            *zc = *zc * k * scale + cc;
        }
        trap = trap.min(r2);
        if dot(z, z) > BOX_BAILOUT_SQ {
            break;
        }
    }
    (length(z) / dr.abs(), trap)
}

fn kifs_de(p: [f64; 3], scale: f64, offset: [f64; 3], rotation: [[f64; 3]; 3], max_iterations: u32) -> (f64, f64) {
    let mut z = p;
    let mut stretch = 1.0;
    let mut trap = f64::MAX;
    for _ in 0..max_iterations {
        // Fold into the octant x ≥ y ≥ z ≥ 0.
        z = z.map(f64::abs);
        if z[0] < z[1] {
            z.swap(0, 1);
        }
        if z[0] < z[2] {
            z.swap(0, 2);
        }
        if z[1] < z[2] {
            z.swap(1, 2);
        }
        z = rotation.map(|row| dot(row, z));
        for (zc, oc) in z.iter_mut().zip(offset) {
            *zc = *zc * scale - oc * (scale - 1.0);
        }
        // Keep the middle slab (the Menger sponge's face-center hole).
        if z[2] < -0.5 * offset[2] * (scale - 1.0) {
            z[2] += offset[2] * (scale - 1.0);
        }
        stretch *= scale;
        trap = trap.min(dot(z, z));
    }
    (unit_box_distance(z) / stretch, trap)
}

/// Signed distance from `p` to the cube [−1, 1]³.
fn unit_box_distance(p: [f64; 3]) -> f64 {
    let q = p.map(|c| c.abs() - 1.0);
    length(q.map(|c| c.max(0.0))) + q[0].max(q[1]).max(q[2]).min(0.0)
}

/// Escape radius² of the quaternion Julia iteration — larger than the
/// minimal 4 so the DE formula, which assumes |z| ≫ 1, is accurate.
const QUATERNION_BAILOUT_SQ: f64 = 256.0;

fn quaternion_julia_de(p: [f64; 3], c: [f64; 4], slice: f64, max_iterations: u32) -> (f64, f64) {
    // z = (real, i, j, k)
    let mut z = [p[0], p[1], p[2], slice];
    let mut dr = 1.0;
    let mut r2 = z.iter().map(|v| v * v).sum::<f64>();
    let mut trap = f64::MAX;
    for _ in 0..max_iterations {
        // |d(z²)| = 2|z|·|dz| for quaternions, as for complex numbers.
        dr *= 2.0 * r2.sqrt();
        // (a + v)² = a² − |v|² + 2a·v
        let imag2 = z[1] * z[1] + z[2] * z[2] + z[3] * z[3];
        z = [z[0] * z[0] - imag2 + c[0], 2.0 * z[0] * z[1] + c[1], 2.0 * z[0] * z[2] + c[2], 2.0 * z[0] * z[3] + c[3]];
        r2 = z.iter().map(|v| v * v).sum::<f64>();
        trap = trap.min(r2);
        if r2 > QUATERNION_BAILOUT_SQ {
            break;
        }
    }
    let r = r2.sqrt().max(1e-30);
    (0.5 * r * r.ln() / dr.max(1e-30), trap)
}

fn pseudo_kleinian_de(p: [f64; 3], box_size: [f64; 3], size: f64, thickness: f64, max_iterations: u32) -> (f64, f64) {
    let mut z = p;
    let mut stretch = 1.0;
    let mut trap = f64::MAX;
    for _ in 0..max_iterations {
        let previous = z;
        for (zc, b) in z.iter_mut().zip(box_size) {
            *zc = 2.0 * zc.clamp(-b, b) - *zc;
        }
        let r2 = dot(z, z).max(1e-30);
        trap = trap.min(r2);
        let k = (size / r2).max(1.0);
        z = z.map(|c| c * k);
        stretch *= k;
        // Once a fold changes nothing, every later one won't either.
        if z == previous {
            break;
        }
    }
    let rxy = (z[0] * z[0] + z[1] * z[1]).sqrt();
    let de = (rxy - thickness).max((rxy * z[2]).abs() / length(z).max(1e-30));
    (de / stretch, trap)
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
    /// Tilt around the viewing direction, radians in `[−π, π)`. Positive
    /// banks right (like an aircraft): the image turns counter-clockwise.
    #[serde(default)]
    pub roll: f64,
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

    /// Screen-right: perpendicular to forward, level with the world
    /// horizon when unrolled.
    pub fn right(&self) -> [f64; 3] {
        let (sr, cr) = self.roll.sin_cos();
        let (right, up) = (self.level_right(), self.level_up());
        [0, 1, 2].map(|i| right[i] * cr - up[i] * sr)
    }

    /// Screen-up, perpendicular to forward and right.
    pub fn up(&self) -> [f64; 3] {
        let (sr, cr) = self.roll.sin_cos();
        let (right, up) = (self.level_right(), self.level_up());
        [0, 1, 2].map(|i| up[i] * cr + right[i] * sr)
    }

    fn level_right(&self) -> [f64; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        [cy, 0.0, -sy]
    }

    fn level_up(&self) -> [f64; 3] {
        cross(self.forward(), self.level_right())
    }

    /// Turns by mouse-look deltas (radians), clamping pitch.
    pub fn rotate(&mut self, delta_yaw: f64, delta_pitch: f64) {
        self.yaw = (self.yaw + delta_yaw).rem_euclid(std::f64::consts::TAU);
        self.pitch = (self.pitch + delta_pitch).clamp(-MAX_PITCH, MAX_PITCH);
    }

    /// Turns by a mouse-look drag measured in screen space (`right`/`up`
    /// in radians, as seen in the rolled image), so dragging right turns
    /// toward the image's right edge whatever the tilt.
    pub fn rotate_screen(&mut self, right: f64, up: f64) {
        let (sr, cr) = self.roll.sin_cos();
        self.rotate(right * cr + up * sr, up * cr - right * sr);
    }

    /// Tilts by `delta` radians, wrapping into `[−π, π)`.
    pub fn roll_by(&mut self, delta: f64) {
        use std::f64::consts::{PI, TAU};
        self.roll = (self.roll + delta + PI).rem_euclid(TAU) - PI;
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
    /// Sphere inversion applied to all of space before the formula; `None`
    /// renders the fractal as is.
    #[serde(default)]
    pub inversion: Option<Inversion>,
}

impl Params3D {
    pub fn new(formula: Formula3D) -> Self {
        Self {
            formula,
            camera: formula.default_camera(),
            max_iterations: match formula {
                // Every KIFS level is 3× finer; f32 can't resolve much past 8.
                Formula3D::Kifs { .. } => 8,
                Formula3D::PseudoKleinian { .. } => 8,
                _ => 12,
            },
            max_steps: 300,
            detail: 1.0,
            step_factor: match formula {
                Formula3D::Mandelbulb { .. } | Formula3D::PseudoKleinian { .. } => 0.8,
                Formula3D::Mandelbox { .. } | Formula3D::Kifs { .. } | Formula3D::QuaternionJulia { .. } => 0.9,
            },
            fog_distance: 100.0,
            palette: Palette::ocean(),
            inversion: None,
        }
    }

    /// `(distance estimate, orbit trap)` at `p`, in inverted space if an
    /// inversion is set.
    pub fn estimate(&self, p: [f64; 3]) -> (f64, f64) {
        match self.inversion {
            Some(inversion) => inversion.estimate(self.formula, p, self.max_iterations),
            None => self.formula.estimate(p, self.max_iterations),
        }
    }

    /// Turns inversion on (with the camera moved into the resulting cave) or
    /// off (with the camera back at the formula's default view).
    pub fn set_inversion(&mut self, inversion: Option<Inversion>) {
        self.inversion = inversion;
        self.camera = self.default_camera();
    }

    /// The default view for the current formula and inversion.
    pub fn default_camera(&self) -> Camera3D {
        match self.inversion {
            Some(inversion) => inversion.default_camera(),
            None => self.formula.default_camera(),
        }
    }

    /// DE at the camera — its clearance to the surface.
    pub fn camera_clearance(&self) -> f64 {
        self.estimate(self.camera.position).0
    }

    /// Furthest a ray needs to march: past the far side of the bounding
    /// sphere — or a fixed cap in inverted space, where the fractal reaches
    /// out to infinity.
    pub fn max_distance(&self) -> f64 {
        let radius = if self.inversion.is_some() { UNBOUNDED_RADIUS } else { self.formula.bounding_radius() };
        length(self.camera.position) + 2.0 * radius
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

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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
        let mut cam = Camera3D { position: [0.0; 3], yaw: 0.0, pitch: 0.0, roll: 0.0, fov_y_degrees: 60.0 };
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
    fn roll_tilts_the_screen_axes_around_the_view_direction() {
        let mut cam = Camera3D { position: [0.0; 3], yaw: 0.4, pitch: -0.3, roll: 0.0, fov_y_degrees: 60.0 };
        let (forward, right, up) = (cam.forward(), cam.right(), cam.up());
        cam.roll_by(std::f64::consts::FRAC_PI_2);
        // Banking right by 90°: forward unchanged, screen-up is the old
        // screen-right, screen-right the old screen-down.
        assert_close(cam.forward(), forward);
        assert_close(cam.up(), right);
        assert_close(cam.right(), up.map(|c| -c));

        cam.roll_by(0.7);
        let (f, r, u) = (cam.forward(), cam.right(), cam.up());
        assert!(dot(f, r).abs() < 1e-12 && dot(f, u).abs() < 1e-12 && dot(r, u).abs() < 1e-12);
        assert!((length(r) - 1.0).abs() < 1e-12 && (length(u) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn roll_wraps_into_half_open_range() {
        let mut cam = Formula3D::default_camera(MANDELBULB_DEFAULT);
        cam.roll_by(3.0);
        cam.roll_by(1.0);
        assert!((cam.roll - (4.0 - std::f64::consts::TAU)).abs() < 1e-12, "{}", cam.roll);
    }

    #[test]
    fn screen_space_look_follows_the_tilted_image() {
        // Rolled 90° right, the image's right edge points to world-down:
        // dragging right must pitch the view down, not turn it.
        let mut cam = Formula3D::default_camera(MANDELBULB_DEFAULT);
        cam.roll = std::f64::consts::FRAC_PI_2;
        cam.rotate_screen(0.2, 0.0);
        assert!(cam.yaw.abs() < 1e-12 && (cam.pitch + 0.2).abs() < 1e-12, "yaw {} pitch {}", cam.yaw, cam.pitch);
    }

    #[test]
    fn cameras_without_roll_still_deserialize() {
        let ron = "(position: (0.0, 0.0, -2.0), yaw: 0.5, pitch: 0.1, fov_y_degrees: 60.0)";
        let cam: Camera3D = ron::from_str(ron).expect("deserialize");
        assert_eq!(cam.roll, 0.0);
    }

    #[test]
    fn pitch_is_clamped() {
        let mut cam = Formula3D::default_camera(MANDELBULB_DEFAULT);
        cam.rotate(0.0, 10.0);
        assert_eq!(cam.pitch, MAX_PITCH);
    }

    const ALL_DEFAULTS: [Formula3D; 5] =
        [MANDELBULB_DEFAULT, MANDELBOX_DEFAULT, KIFS_DEFAULT, PSEUDO_KLEINIAN_DEFAULT, QUATERNION_JULIA_DEFAULT];

    #[test]
    fn default_cameras_start_in_free_space_with_room_to_move() {
        for formula in ALL_DEFAULTS {
            let params = Params3D::new(formula);
            assert!(params.camera_clearance() > 0.1, "{}: {}", formula.display_name(), params.camera_clearance());
        }
    }

    #[test]
    fn inverted_default_cameras_start_inside_a_cave() {
        let inversion = Inversion { center: [0.0; 3], radius: 1.0 };
        for formula in [MANDELBULB_DEFAULT, MANDELBOX_DEFAULT] {
            let mut params = Params3D::new(formula);
            params.set_inversion(Some(inversion));
            assert!(params.camera_clearance() > 0.01, "{}: {}", formula.display_name(), params.camera_clearance());
            // Walled in: a ray in any axis direction hits the surface soon.
            for dir in [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]] {
                let mut t = 0.0;
                let hit = (0..1000).any(|_| {
                    let p = params.camera.position;
                    let de = params.estimate([p[0] + dir[0] * t, p[1] + dir[1] * t, p[2] + dir[2] * t]).0;
                    t += de * 0.9;
                    de < 1e-5
                });
                assert!(hit && t < 5.0, "{}: no wall along {dir:?}", formula.display_name());
            }
        }
    }

    #[test]
    fn inversion_center_maps_to_free_space() {
        // The center is the image of infinity: a ray aimed straight through
        // it must not stall there.
        let inversion = Inversion { center: [0.0; 3], radius: 1.0 };
        let (de, _) = inversion.estimate(MANDELBULB_DEFAULT, [0.0; 3], 12);
        assert!(de > 0.3, "{de}");
        assert!((inversion.free_radius(MANDELBULB_DEFAULT) - 1.0 / 1.5).abs() < 1e-12);
    }

    #[test]
    fn menger_sponge_de_matches_known_distances() {
        // At the center of the sponge's corridor cross the nearest solid
        // edge (1/3, 1/3, 0) is √2/3 away.
        let de = KIFS_DEFAULT.distance_estimate([0.0; 3], 8);
        assert!((de - 2f64.sqrt() / 3.0).abs() < 1e-9, "{de}");
        // Inside a first-level edge cube: solid.
        assert!(KIFS_DEFAULT.distance_estimate([0.5, 0.5, 0.0], 8) <= 0.0);
        // Far outside, facing a face whose center is a hole: the nearest
        // solid point is (1, 1/3, 1/3), and a DE must never exceed that.
        let far = KIFS_DEFAULT.distance_estimate([5.0, 0.0, 0.0], 8);
        let true_distance = (16.0f64 + 2.0 / 9.0).sqrt();
        assert!(far > 3.9 && far <= true_distance, "{far} vs {true_distance}");
    }

    #[test]
    fn quaternion_julia_with_zero_c_is_the_unit_sphere() {
        // z → z² keeps |z| < 1 inside the unit ball and escapes outside:
        // the slice is exactly the unit sphere, so the DE has a known answer.
        let sphere = Formula3D::QuaternionJulia { c: [0.0; 4], slice: 0.0 };
        let de = sphere.distance_estimate([0.0, 0.0, 3.0], 12);
        assert!(de > 1.5 && de <= 2.0 + 1e-9, "{de}");
        let near = sphere.distance_estimate([0.0, 1.05, 0.0], 12);
        assert!(near > 0.0 && near <= 0.05 + 1e-9, "{near}");
        assert!(sphere.distance_estimate([0.3, -0.2, 0.1], 12) < 0.0, "inside the ball");
        // A 4D slice off-center cuts a smaller sphere: at w = 0.8 its
        // radius is 0.6, so (0, 0, 0.7) is outside.
        let sliced = Formula3D::QuaternionJulia { c: [0.0; 4], slice: 0.8 };
        assert!(sliced.distance_estimate([0.0, 0.0, 0.7], 12) > 0.0);
        assert!(sliced.distance_estimate([0.0, 0.0, 0.5], 12) < 0.0);
    }

    #[test]
    fn quaternion_julia_lies_within_its_bounding_radius() {
        let r = QUATERNION_JULIA_DEFAULT.bounding_radius();
        for dir in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [-0.6, 0.0, 0.8]] {
            let p = dir.map(|d| d * r);
            assert!(QUATERNION_JULIA_DEFAULT.distance_estimate(p, 12) > 0.0, "{p:?} should be outside");
        }
    }

    #[test]
    fn julia_bulb_with_zero_seed_is_the_unit_sphere() {
        // With c = 0 the bulb's map only takes r → r⁸: points inside the
        // unit sphere stay bounded, points outside escape.
        let julia = MANDELBULB_DEFAULT.with_julia(Some([0.0; 3]));
        let de = julia.distance_estimate([0.0, 0.0, 3.0], 12);
        assert!(de > 1.5 && de <= 2.0 + 1e-9, "{de}");
        let near = julia.distance_estimate([1.05, 0.0, 0.0], 12);
        assert!(near > 0.0 && near <= 0.05 + 1e-9, "{near}");
        assert!(julia.distance_estimate([0.2, 0.3, -0.1], 12) < 0.0);
    }

    #[test]
    fn julia_mode_toggles_only_where_supported() {
        for formula in ALL_DEFAULTS {
            let seed = formula.default_julia_seed();
            assert_eq!(seed.is_some(), formula.supports_julia());
            let on = formula.with_julia(seed);
            assert_eq!(on.julia_seed(), seed);
            assert_eq!(on.with_julia(None), formula, "switching off restores the classic formula");
        }
        // A Julia seed really changes the shape.
        let point = [0.3, 0.4, 0.5];
        let classic = MANDELBOX_DEFAULT.distance_estimate(point, 12);
        let julia = MANDELBOX_DEFAULT.with_julia(Some(MANDELBOX_JULIA_SEED)).distance_estimate(point, 12);
        assert!((classic - julia).abs() > 1e-6);
    }

    #[test]
    fn julia_default_cameras_start_in_free_space() {
        for formula in [MANDELBULB_DEFAULT, MANDELBOX_DEFAULT] {
            let mut params = Params3D::new(formula);
            params.formula = formula.with_julia(formula.default_julia_seed());
            params.camera = params.default_camera();
            assert!(params.camera_clearance() > 0.1, "{}: {}", formula.display_name(), params.camera_clearance());
        }
    }

    #[test]
    fn formulas_saved_before_julia_mode_still_load() {
        let bulb: Formula3D = ron::from_str("Mandelbulb(power: 8.0)").unwrap();
        assert_eq!(bulb, MANDELBULB_DEFAULT);
        let r#box: Formula3D = ron::from_str("Mandelbox(scale: -1.5, min_radius: 0.5, fixed_radius: 1.0)").unwrap();
        assert_eq!(r#box, MANDELBOX_DEFAULT);
    }

    #[test]
    fn formula_parameters_round_trip_for_every_formula() {
        let julias = [MANDELBULB_DEFAULT, MANDELBOX_DEFAULT].map(|f| f.with_julia(f.default_julia_seed()));
        for formula in ALL_DEFAULTS.into_iter().chain(julias) {
            let values = formula.parameters();
            let shifted: Vec<f64> = values.iter().map(|v| v + 0.25).collect();
            let changed = formula.with_parameters(&shifted);
            assert_eq!(changed.shader_id(), formula.shader_id());
            assert_eq!(changed.parameters(), shifted);
            assert_eq!(formula.with_parameters(&[]), formula, "too few values leave it unchanged");
        }
    }

    #[test]
    fn kifs_rotation_is_orthonormal() {
        let m = kifs_rotation([23.0, -71.0]);
        for i in 0..3 {
            for j in 0..3 {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((dot(m[i], m[j]) - expected).abs() < 1e-12);
            }
        }
        assert_eq!(kifs_rotation([0.0, 0.0]), [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
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
