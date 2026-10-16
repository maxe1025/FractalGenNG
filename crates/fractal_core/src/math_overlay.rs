//! The math behind one render, as short human-readable lines (the
//! iteration formula, the actual values plugged into it, and optionally how
//! each pixel is colored) — shown over the viewport and/or burned into
//! exported images and animation frames.
//!
//! Text only uses plain Unicode (sub/superscript digits, `·`, `−`), no
//! markup, so any renderer with a reasonably complete font can draw it.

use serde::{Deserialize, Serialize};

use crate::fractal3d::{Formula3D, Params3D};
use crate::kinds::FractalKind;
use crate::params::FractalParams;

/// Fractional digits beyond `log10(zoom)` printed for the center, so it
/// still resolves well below one pixel on a tall export.
const CENTER_EXTRA_DECIMALS: usize = 4;
/// Hard cap on the center's printed fractional digits — a 1e250 zoom would
/// otherwise need ~250 digits per coordinate, far wider than any image.
/// Truncated values are marked with a trailing `…`.
pub const MAX_CENTER_DECIMALS: usize = 40;

/// How much of the math the overlay spells out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MathDetail {
    /// The formula and its stopping rule, in symbols.
    Formula,
    /// The formula plus the actual values of this frame (center, zoom,
    /// seed, bailout, iteration limit).
    Values,
    /// Everything in `Values`, plus how each pixel's color is derived and
    /// the center's working precision.
    Full,
}

impl MathDetail {
    pub const ALL: [MathDetail; 3] = [MathDetail::Formula, MathDetail::Values, MathDetail::Full];

    pub fn label(self) -> &'static str {
        match self {
            MathDetail::Formula => "Formula",
            MathDetail::Values => "Formula + values",
            MathDetail::Full => "Full (incl. coloring)",
        }
    }
}

/// Which image corner the overlay box sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl OverlayCorner {
    pub const ALL: [OverlayCorner; 4] =
        [OverlayCorner::TopLeft, OverlayCorner::TopRight, OverlayCorner::BottomLeft, OverlayCorner::BottomRight];

    pub fn label(self) -> &'static str {
        match self {
            OverlayCorner::TopLeft => "Top left",
            OverlayCorner::TopRight => "Top right",
            OverlayCorner::BottomLeft => "Bottom left",
            OverlayCorner::BottomRight => "Bottom right",
        }
    }

    pub fn is_right(self) -> bool {
        matches!(self, OverlayCorner::TopRight | OverlayCorner::BottomRight)
    }

    pub fn is_bottom(self) -> bool {
        matches!(self, OverlayCorner::BottomLeft | OverlayCorner::BottomRight)
    }
}

/// Overlay settings. Sizes are fractions of the image height, so the
/// viewport preview and an export at any resolution have the same layout.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MathOverlay {
    pub show_in_viewport: bool,
    pub burn_into_exports: bool,
    pub detail: MathDetail,
    pub corner: OverlayCorner,
    /// Text height as a fraction of the image height.
    pub text_scale: f32,
}

impl Default for MathOverlay {
    fn default() -> Self {
        Self {
            show_in_viewport: false,
            burn_into_exports: false,
            detail: MathDetail::Values,
            corner: OverlayCorner::BottomLeft,
            text_scale: 0.022,
        }
    }
}

impl MathOverlay {
    /// Text height in pixels for an image `image_height` pixels tall.
    pub fn text_px(&self, image_height: f32) -> f32 {
        (self.text_scale * image_height).max(6.0)
    }

    /// Padding between the box edge and its text.
    pub fn padding_px(&self, image_height: f32) -> f32 {
        self.text_px(image_height) * 0.6
    }

    /// Gap between the box and the image edges.
    pub fn margin_px(&self, image_height: f32) -> f32 {
        self.text_px(image_height)
    }

    /// Top-left corner of a `box_w × box_h` overlay box inside a
    /// `image_w × image_h` image.
    pub fn box_origin(&self, image_w: f32, image_h: f32, box_w: f32, box_h: f32) -> (f32, f32) {
        let margin = self.margin_px(image_h);
        let x = if self.corner.is_right() { image_w - margin - box_w } else { margin };
        let y = if self.corner.is_bottom() { image_h - margin - box_h } else { margin };
        (x.max(0.0), y.max(0.0))
    }
}

/// The overlay's lines for one render of `params`.
pub fn describe(params: &FractalParams, detail: MathDetail) -> Vec<String> {
    let kind = params.kind;
    let mut lines = vec![iteration_formula(kind)];

    let start = match (kind, detail) {
        (FractalKind::Julia { seed }, MathDetail::Values | MathDetail::Full) => {
            format!("z₀ = pixel,  c = {}", format_complex_f64(seed[0], seed[1]))
        }
        (FractalKind::Julia { .. }, MathDetail::Formula) => "z₀ = pixel,  c = seed".to_string(),
        _ => "z₀ = 0,  c = pixel".to_string(),
    };
    lines.push(start);

    if detail == MathDetail::Formula {
        lines.push("n = iterations until |z| > R  (max N)".to_string());
        return lines;
    }

    lines.push(format!("center = {}", format_center(params)));
    lines.push(format!("zoom = {}  (view ±{} tall)", format_scalar(params.zoom), format_scalar(2.0 / params.zoom)));
    lines.push(format!(
        "n = iterations until |z| > {}  (max {})",
        format_scalar(params.bailout_radius),
        params.max_iterations
    ));

    if detail == MathDetail::Full {
        lines.push(format!(
            "color = palette((n + 1 − log{} log₂|z|) / {})",
            subscript(i64::from(growth_power(kind))),
            format_scalar(f64::from(params.palette.cycle_count))
        ));
        if params.palette.distance_shading && kind.supports_distance_shading() {
            let wrt = if matches!(kind, FractalKind::Julia { .. }) { "z₀" } else { "c" };
            lines.push(format!("shade by distance |z|·ln|z| / |dz/d{wrt}|"));
        }
        lines.push(format!("center precision = {} bits", params.center.precision_bits()));
    }
    lines
}

/// The overlay's lines for one 3D render of `params`.
pub fn describe_3d(params: &Params3D, detail: MathDetail) -> Vec<String> {
    let mut lines = Vec::new();
    match params.formula {
        Formula3D::Mandelbulb { power } => {
            let p = format_scalar(power);
            lines.push(format!("z → {} + c   (r → {}, θ → {p}θ, φ → {p}φ)", power_of("z", power), power_of("r", power)));
            lines.push("z₀ = c = point on the ray".to_string());
            lines.push(format!("DE = 0.5·ln r·r / dr,   dr → {p}·{}·dr + 1", power_of("r", power - 1.0)));
        }
        Formula3D::Mandelbox { scale, min_radius, fixed_radius } => {
            let values = detail != MathDetail::Formula;
            lines.push(if values {
                format!("z → s·sphereFold(boxFold(z)) + c,   s = {}", format_scalar(scale))
            } else {
                "z → s·sphereFold(boxFold(z)) + c".to_string()
            });
            lines.push("boxFold: x → clamp(x, −1, 1)·2 − x   (per axis)".to_string());
            lines.push(if values {
                format!(
                    "sphereFold: z·(R/m)² if r < m,  z·(R/r)² if r < R   (m = {}, R = {})",
                    format_scalar(min_radius),
                    format_scalar(fixed_radius)
                )
            } else {
                "sphereFold: z·(R/m)² if r < m,  z·(R/r)² if r < R".to_string()
            });
            lines.push("z₀ = c = point on the ray,   DE = |z| / |dr|".to_string());
        }
    }

    if detail == MathDetail::Formula {
        lines.push("ray: t → t + DE(camera + t·dir)  until DE < pixel size".to_string());
        return lines;
    }

    let cam = &params.camera;
    lines.push(format!("camera = {}  looking {}", format_vec3(cam.position), format_vec3(cam.forward())));
    lines.push(format!(
        "field of view {}°,  {} iterations,  max {} steps",
        format_scalar(cam.fov_y_degrees),
        params.max_iterations,
        params.max_steps
    ));
    lines.push(format!(
        "ray: t → t + {}·DE  until DE < {} px   (clearance {})",
        format_scalar(params.step_factor),
        format_scalar(params.detail),
        format_scalar(params.camera_clearance())
    ));

    if detail == MathDetail::Full {
        lines.push(format!(
            "color = palette(log₂(1 + min|z|²) · {} / 4)",
            format_scalar(f64::from(params.palette.cycle_count))
        ));
        lines.push("light = headlight + key light,  × ambient occlusion".to_string());
        if params.fog_distance > 0.0 {
            lines.push(format!("fog = 1 − exp(−t / ({}·clearance))", format_scalar(params.fog_distance)));
        }
    }
    lines
}

/// `base` raised to `exponent`: a superscript for integers, `^x` otherwise
/// (the fonts have no superscript decimal point).
fn power_of(base: &str, exponent: f64) -> String {
    if exponent.fract() == 0.0 && exponent.abs() < 1e6 {
        format!("{base}{}", superscript(exponent as i64))
    } else {
        format!("{base}^{}", format_scalar(exponent))
    }
}

fn format_vec3(v: [f64; 3]) -> String {
    format!("({}, {}, {})", format_coord(v[0]), format_coord(v[1]), format_coord(v[2]))
}

/// A coordinate with enough decimals to follow the camera into small
/// crevices without the line growing unboundedly.
fn format_coord(x: f64) -> String {
    trim_fixed(x, 5).replace('-', "−")
}

fn iteration_formula(kind: FractalKind) -> String {
    match kind {
        FractalKind::Mandelbrot | FractalKind::Julia { .. } => "z → z² + c".to_string(),
        FractalKind::BurningShip => "z → (|Re z| + i·|Im z|)² + c".to_string(),
        FractalKind::Tricorn => "z → conj(z)² + c".to_string(),
        FractalKind::Multibrot { .. } => format!("z → z{} + c", superscript(i64::from(growth_power(kind)))),
    }
}

/// The exponent the iteration actually raises `z` to. Multibrot's power is
/// rounded the same way the GPU uniforms round it (see `gpu/buffers.rs`),
/// so the overlay states what was rendered, not the un-rounded slider value.
fn growth_power(kind: FractalKind) -> u32 {
    match kind {
        FractalKind::Multibrot { power } => power.round().max(2.0) as u32,
        _ => 2,
    }
}

fn format_center(params: &FractalParams) -> String {
    let wanted = params.zoom.max(1.0).log10().ceil() as usize + CENTER_EXTRA_DECIMALS;
    let decimals = wanted.min(MAX_CENTER_DECIMALS);
    let truncated = wanted > MAX_CENTER_DECIMALS;
    let (re, im) = params.center.to_fixed_decimal_pair(decimals);
    let ellipsis = if truncated { "…" } else { "" };
    join_complex(&format!("{re}{ellipsis}"), &format!("{im}{ellipsis}"))
}

fn format_complex_f64(re: f64, im: f64) -> String {
    join_complex(&trim_fixed(re, 6), &trim_fixed(im, 6))
}

/// `"a + bi"` / `"a − bi"` from two ASCII-signed decimal strings, with a
/// typographic minus sign.
fn join_complex(re: &str, im: &str) -> String {
    let re = re.replace('-', "−");
    match im.strip_prefix('-') {
        Some(abs) => format!("{re} − {abs}i"),
        None => format!("{re} + {im}i"),
    }
}

fn trim_fixed(x: f64, decimals: usize) -> String {
    let s = format!("{x:.decimals$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { &s };
    if s == "-0" { "0".to_string() } else { s.to_string() }
}

/// A plain decimal for moderate magnitudes (`4`, `0.0125`), scientific with
/// a superscript exponent otherwise (`5·10⁷`, `1.25·10⁻⁸`).
fn format_scalar(x: f64) -> String {
    if x == 0.0 || !x.is_finite() {
        return trim_fixed(x, 0);
    }
    let exp = x.abs().log10().floor() as i64;
    if (-3..=4).contains(&exp) {
        return trim_fixed(x, (3 - exp).max(0) as usize).replace('-', "−");
    }
    let mut mantissa = x / 10f64.powi(exp as i32);
    let mut exp = exp;
    // 9.996·10³ would print as "10·10³" after rounding to 2 decimals.
    if (mantissa.abs() * 100.0).round() >= 1000.0 {
        mantissa /= 10.0;
        exp += 1;
    }
    format!("{}·10{}", trim_fixed(mantissa, 2), superscript(exp)).replace('-', "−")
}

fn superscript(n: i64) -> String {
    map_digits(n, ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'], '⁻')
}

fn subscript(n: i64) -> String {
    map_digits(n, ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'], '₋')
}

fn map_digits(n: i64, digits: [char; 10], minus: char) -> String {
    let mut s = String::new();
    if n < 0 {
        s.push(minus);
    }
    for c in n.unsigned_abs().to_string().chars() {
        s.push(digits[c.to_digit(10).unwrap_or(0) as usize]);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hp::{required_bits, HpComplex};

    #[test]
    fn mandelbrot_values_describe_the_actual_frame() {
        let params = FractalParams {
            center: HpComplex::from_f64_pair(-0.743_643_887_037_151, 0.131_825_904_205_330, required_bits(5.0e7)),
            zoom: 5.0e7,
            max_iterations: 2000,
            ..FractalParams::default()
        };
        let lines = describe(&params, MathDetail::Values);
        assert_eq!(lines[0], "z → z² + c");
        assert_eq!(lines[1], "z₀ = 0,  c = pixel");
        // log10(5e7) -> 8 digits + 4 extra = 12 decimals.
        assert_eq!(lines[2], "center = −0.743643887037 + 0.131825904205i");
        assert_eq!(lines[3], "zoom = 5·10⁷  (view ±4·10⁻⁸ tall)");
        assert_eq!(lines[4], "n = iterations until |z| > 4  (max 2000)");
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn formula_detail_has_no_concrete_values() {
        let params = FractalParams::julia_default();
        let lines = describe(&params, MathDetail::Formula);
        assert_eq!(lines, vec!["z → z² + c", "z₀ = pixel,  c = seed", "n = iterations until |z| > R  (max N)"]);
    }

    #[test]
    fn julia_values_show_the_seed_with_a_typographic_minus() {
        let params = FractalParams { kind: FractalKind::Julia { seed: [-0.8, -0.156] }, ..FractalParams::julia_default() };
        let lines = describe(&params, MathDetail::Values);
        assert_eq!(lines[1], "z₀ = pixel,  c = −0.8 − 0.156i");
    }

    #[test]
    fn multibrot_shows_the_rounded_power_the_gpu_renders() {
        let params = FractalParams { kind: FractalKind::Multibrot { power: 4.6 }, ..FractalParams::default() };
        let lines = describe(&params, MathDetail::Full);
        assert_eq!(lines[0], "z → z⁵ + c");
        assert!(lines.iter().any(|l| l.starts_with("color = palette((n + 1 − log₅ log₂|z|)")));
    }

    #[test]
    fn full_detail_mentions_distance_shading_only_when_enabled() {
        let mut params = FractalParams::default();
        assert!(!describe(&params, MathDetail::Full).iter().any(|l| l.contains("distance")));
        params.palette.distance_shading = true;
        assert!(describe(&params, MathDetail::Full).iter().any(|l| l.contains("dz/dc")));
    }

    #[test]
    fn deep_zoom_center_is_capped_and_marked() {
        let params = FractalParams { zoom: 1.0e100, ..FractalParams::default() };
        let center = &describe(&params, MathDetail::Values)[2];
        assert!(center.contains('…'), "{center}");
    }

    #[test]
    fn mandelbulb_3d_lines_state_power_and_camera() {
        let params = Params3D::new(crate::fractal3d::MANDELBULB_DEFAULT);
        let lines = describe_3d(&params, MathDetail::Values);
        assert_eq!(lines[0], "z → z⁸ + c   (r → r⁸, θ → 8θ, φ → 8φ)");
        assert_eq!(lines[2], "DE = 0.5·ln r·r / dr,   dr → 8·r⁷·dr + 1");
        assert_eq!(lines[3], "camera = (0, 0, −2.6)  looking (0, 0, 1)");
        assert!(lines[4].starts_with("field of view 60°,  12 iterations,  max 300 steps"));
    }

    #[test]
    fn mandelbox_formula_detail_uses_symbols_only() {
        let params = Params3D::new(crate::fractal3d::MANDELBOX_DEFAULT);
        let lines = describe_3d(&params, MathDetail::Formula);
        assert_eq!(lines[0], "z → s·sphereFold(boxFold(z)) + c");
        assert!(!lines.iter().any(|l| l.contains("camera =")));
        let values = describe_3d(&params, MathDetail::Full);
        assert!(values[0].ends_with("s = −1.5"));
        assert!(values.iter().any(|l| l.starts_with("fog = ")));
    }

    #[test]
    fn non_integer_powers_use_caret_notation() {
        assert_eq!(power_of("z", 8.0), "z⁸");
        assert_eq!(power_of("z", 7.5), "z^7.5");
    }

    #[test]
    fn scalar_formatting() {
        assert_eq!(format_scalar(4.0), "4");
        assert_eq!(format_scalar(0.0125), "0.0125");
        assert_eq!(format_scalar(1.0e-8 * 1.25), "1.25·10⁻⁸");
        assert_eq!(format_scalar(9.996e5), "1·10⁶");
        assert_eq!(format_scalar(-2.0e9), "−2·10⁹");
    }

    #[test]
    fn box_origin_respects_corner_and_margin() {
        let overlay = MathOverlay { corner: OverlayCorner::BottomRight, text_scale: 0.02, ..MathOverlay::default() };
        // 1000 px tall -> 20 px text -> 20 px margin.
        assert_eq!(overlay.box_origin(1600.0, 1000.0, 300.0, 100.0), (1280.0, 880.0));
        let overlay = MathOverlay { corner: OverlayCorner::TopLeft, ..overlay };
        assert_eq!(overlay.box_origin(1600.0, 1000.0, 300.0, 100.0), (20.0, 20.0));
    }
}
