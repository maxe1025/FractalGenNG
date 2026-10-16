use astro_float::{BigFloat, Consts, Radix, RoundingMode, Sign};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Guard bits added on top of the bits strictly needed to resolve one pixel
/// at a given zoom, absorbing rounding error accumulated across iterations.
pub const MARGIN_BITS: u32 = 64;
pub const MIN_PRECISION_BITS: u32 = 64;
/// `zoom: f64` cannot exceed roughly 1.8e308 in the first place, and
/// `required_bits(1e300) ≈ 1060`, so a higher cap would only waste CPU on
/// precision `zoom` can never actually reach.
pub const MAX_PRECISION_BITS: u32 = 1024;

/// Zoom level at/above which rendering switches from the standard f32 shader
/// to the perturbation-theory deep-zoom shader — matches where f32 precision
/// visibly breaks down.
pub const ZOOM_PERTURBATION_THRESHOLD: f64 = 1.0e5;
/// Hard interactive-zoom clamp when the perturbation path is unavailable
/// (unsupported GPU) — the deepest zoom the f32 shader can still render.
pub const ZOOM_F32_SAFE_CEILING: f64 = 1.0e5;
/// Hard interactive-zoom clamp when the perturbation path *is* available.
pub const MAX_INTERACTIVE_ZOOM: f64 = 1.0e250;

/// Mantissa bits needed to distinguish neighboring pixels at the given
/// magnification, plus [`MARGIN_BITS`] of guard digits, clamped to
/// `[MIN_PRECISION_BITS, MAX_PRECISION_BITS]`.
pub fn required_bits(zoom: f64) -> u32 {
    let bits = zoom.max(1.0).log2().ceil() as i64 + i64::from(MARGIN_BITS);
    bits.clamp(i64::from(MIN_PRECISION_BITS), i64::from(MAX_PRECISION_BITS)) as u32
}

/// An arbitrary-precision complex number: the focus-point coordinate that
/// stays precise far beyond what `f64` can resolve at extreme zoom.
#[derive(Debug, Clone)]
pub struct HpComplex {
    re: BigFloat,
    im: BigFloat,
    precision_bits: u32,
}

impl HpComplex {
    pub fn from_f64_pair(re: f64, im: f64, precision_bits: u32) -> Self {
        let precision_bits = precision_bits.max(MIN_PRECISION_BITS);
        Self {
            re: BigFloat::from_f64(re, precision_bits as usize),
            im: BigFloat::from_f64(im, precision_bits as usize),
            precision_bits,
        }
    }

    /// Lossy downcast, for the shallow-zoom f32 shader path and UI display.
    pub fn to_f64_pair(&self) -> (f64, f64) {
        (bigfloat_to_f64(&self.re), bigfloat_to_f64(&self.im))
    }

    pub fn precision_bits(&self) -> u32 {
        self.precision_bits
    }

    /// Re-rounds to a new bit count. Called when the zoom depth crosses a
    /// threshold that needs more (or, with hysteresis, allows fewer) bits.
    pub fn with_precision(&self, new_bits: u32) -> Self {
        let new_bits = new_bits.max(MIN_PRECISION_BITS);
        let mut re = self.re.clone();
        let mut im = self.im.clone();
        let _ = re.set_precision(new_bits as usize, RoundingMode::ToEven);
        let _ = im.set_precision(new_bits as usize, RoundingMode::ToEven);
        Self { re, im, precision_bits: new_bits }
    }

    /// Adds a small `f64`-valued delta (e.g. a pan offset or the net shift
    /// from a zoom-at-cursor recenter) at this value's current precision.
    /// This is the only mutation pan/zoom needs — callers must never
    /// materialize an intermediate low-precision "world position" themselves,
    /// or the precision this type exists to preserve is lost immediately.
    pub fn add_f64_delta(&self, dx: f64, dy: f64) -> Self {
        let p = self.precision_bits as usize;
        let dx_bf = BigFloat::from_f64(dx, p);
        let dy_bf = BigFloat::from_f64(dy, p);
        Self {
            re: self.re.add(&dx_bf, p, RoundingMode::ToEven),
            im: self.im.add(&dy_bf, p, RoundingMode::ToEven),
            precision_bits: self.precision_bits,
        }
    }

    /// Full-precision complex addition, for the reference-orbit recurrence
    /// (`Z_ref_{n+1} = Z_ref_n^2 + C_ref`).
    pub fn add(&self, other: &Self) -> Self {
        let p = self.precision_bits.max(other.precision_bits) as usize;
        Self {
            re: self.re.add(&other.re, p, RoundingMode::ToEven),
            im: self.im.add(&other.im, p, RoundingMode::ToEven),
            precision_bits: p as u32,
        }
    }

    /// Full-precision complex multiplication: `(a+bi)(c+di) = (ac-bd) + (ad+bc)i`.
    pub fn mul(&self, other: &Self) -> Self {
        let p = self.precision_bits.max(other.precision_bits) as usize;
        let ac = self.re.mul(&other.re, p, RoundingMode::ToEven);
        let bd = self.im.mul(&other.im, p, RoundingMode::ToEven);
        let ad = self.re.mul(&other.im, p, RoundingMode::ToEven);
        let bc = self.im.mul(&other.re, p, RoundingMode::ToEven);
        Self {
            re: ac.sub(&bd, p, RoundingMode::ToEven),
            im: ad.add(&bc, p, RoundingMode::ToEven),
            precision_bits: p as u32,
        }
    }

    pub fn square(&self) -> Self {
        self.mul(self)
    }

    /// Both components as plain positional decimal strings (`"-0.7436"`),
    /// rounded to `decimals` fractional digits at full precision, with
    /// trailing zeros trimmed. For display only — unlike [`Self::to_f64_pair`]
    /// it never loses the digits a deep zoom depends on, as long as
    /// `decimals` asks for them.
    pub fn to_fixed_decimal_pair(&self, decimals: usize) -> (String, String) {
        (fixed_decimal_string(&self.re, decimals), fixed_decimal_string(&self.im, decimals))
    }
}

impl PartialEq for HpComplex {
    fn eq(&self, other: &Self) -> bool {
        self.re == other.re && self.im == other.im
    }
}

fn bigfloat_to_f64(value: &BigFloat) -> f64 {
    decimal_string(value, RoundingMode::ToEven).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
}

fn bigfloat_from_decimal_str(s: &str, precision_bits: u32) -> BigFloat {
    let mut cc = Consts::new().expect("astro-float constants cache allocation");
    BigFloat::parse(s, Radix::Dec, precision_bits as usize, RoundingMode::None, &mut cc)
}

fn decimal_string(value: &BigFloat, rm: RoundingMode) -> Option<String> {
    let mut cc = Consts::new().expect("astro-float constants cache allocation");
    let (sign, digits, exponent) = value.convert_to_radix(Radix::Dec, rm, &mut cc).ok()?;
    if digits.is_empty() {
        return Some("0".to_string());
    }
    let mut s = String::with_capacity(digits.len() + 8);
    if sign == Sign::Neg {
        s.push('-');
    }
    s.push_str("0.");
    for d in &digits {
        s.push((b'0' + d) as char);
    }
    s.push('e');
    s.push_str(&exponent.to_string());
    Some(s)
}

fn fixed_decimal_string(value: &BigFloat, decimals: usize) -> String {
    let mut cc = Consts::new().expect("astro-float constants cache allocation");
    match value.convert_to_radix(Radix::Dec, RoundingMode::ToEven, &mut cc) {
        Ok((sign, digits, exponent)) => {
            round_to_fixed_decimals(sign == Sign::Neg, &digits, i64::from(exponent), decimals)
        }
        Err(_) => "0".to_string(),
    }
}

/// Formats `0.d₁d₂d₃… × 10^exponent` (the shape `convert_to_radix` returns)
/// positionally, rounded half-up to `decimals` fractional digits, trailing
/// zeros trimmed. A value that rounds to zero is printed unsigned.
fn round_to_fixed_decimals(negative: bool, digits: &[u8], exponent: i64, decimals: usize) -> String {
    let int_len = exponent.max(0) as usize;
    let mut int_digits: Vec<u8> = (0..int_len).map(|i| digits.get(i).copied().unwrap_or(0)).collect();
    // Leading fractional zeros beyond `decimals + 1` can't affect the
    // rounded result, so don't materialize all of them for tiny values.
    let lead_zeros = ((-exponent).max(0) as usize).min(decimals + 1);
    let frac_all: Vec<u8> =
        std::iter::repeat_n(0, lead_zeros).chain(digits.iter().skip(int_len).copied()).take(decimals + 1).collect();
    let round_up = frac_all.get(decimals).is_some_and(|&d| d >= 5);
    let mut frac: Vec<u8> = frac_all.into_iter().take(decimals).collect();

    if round_up {
        let mut carry = true;
        for d in frac.iter_mut().rev().chain(int_digits.iter_mut().rev()) {
            if *d == 9 {
                *d = 0;
            } else {
                *d += 1;
                carry = false;
                break;
            }
        }
        if carry {
            int_digits.insert(0, 1);
        }
    }
    while frac.last() == Some(&0) {
        frac.pop();
    }
    if int_digits.is_empty() {
        int_digits.push(0);
    }

    let is_zero = int_digits.iter().chain(frac.iter()).all(|&d| d == 0);
    let mut s = String::with_capacity(int_digits.len() + frac.len() + 2);
    if negative && !is_zero {
        s.push('-');
    }
    s.extend(int_digits.iter().map(|&d| (b'0' + d) as char));
    if !frac.is_empty() {
        s.push('.');
        s.extend(frac.iter().map(|&d| (b'0' + d) as char));
    }
    s
}

impl Serialize for HpComplex {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Repr<'a> {
            precision_bits: u32,
            re: &'a str,
            im: &'a str,
        }
        let re_s = decimal_string(&self.re, RoundingMode::None).unwrap_or_else(|| "0".to_string());
        let im_s = decimal_string(&self.im, RoundingMode::None).unwrap_or_else(|| "0".to_string());
        Repr { precision_bits: self.precision_bits, re: &re_s, im: &im_s }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for HpComplex {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Repr {
            precision_bits: u32,
            re: String,
            im: String,
        }
        let repr = Repr::deserialize(deserializer)?;
        Ok(HpComplex {
            re: bigfloat_from_decimal_str(&repr.re, repr.precision_bits),
            im: bigfloat_from_decimal_str(&repr.im, repr.precision_bits),
            precision_bits: repr.precision_bits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_f64() {
        let hp = HpComplex::from_f64_pair(-0.75, 0.125, 128);
        let (re, im) = hp.to_f64_pair();
        assert!((re - (-0.75)).abs() < 1e-12);
        assert!((im - 0.125).abs() < 1e-12);
    }

    #[test]
    fn add_f64_delta_moves_by_exactly_the_delta() {
        let hp = HpComplex::from_f64_pair(1.0, 2.0, 128);
        let moved = hp.add_f64_delta(0.5, -0.25);
        let (re, im) = moved.to_f64_pair();
        assert!((re - 1.5).abs() < 1e-12);
        assert!((im - 1.75).abs() < 1e-12);
    }

    #[test]
    fn with_precision_preserves_value() {
        let hp = HpComplex::from_f64_pair(0.36237, -0.08214, 64);
        let upgraded = hp.with_precision(512);
        assert_eq!(upgraded.precision_bits(), 512);
        let (re, im) = upgraded.to_f64_pair();
        assert!((re - 0.36237).abs() < 1e-9);
        assert!((im - (-0.08214)).abs() < 1e-9);
    }

    #[test]
    fn serde_round_trip_preserves_value_and_precision() {
        let hp = HpComplex::from_f64_pair(-1.25, 0.5, 256);
        let ron = ron::ser::to_string(&hp).expect("serialize");
        let back: HpComplex = ron::from_str(&ron).expect("deserialize");
        assert_eq!(back.precision_bits(), 256);
        assert_eq!(back, hp);
    }

    #[test]
    fn fixed_decimal_pair_rounds_and_trims() {
        let hp = HpComplex::from_f64_pair(-0.75, 0.131_825_904_205_33, 128);
        assert_eq!(hp.to_fixed_decimal_pair(4), ("-0.75".to_string(), "0.1318".to_string()));
        assert_eq!(hp.to_fixed_decimal_pair(6), ("-0.75".to_string(), "0.131826".to_string()));
    }

    #[test]
    fn fixed_decimal_rounding_carries_into_integer_part() {
        // 0.9996 at 3 decimals -> 1.0 -> "1"
        assert_eq!(round_to_fixed_decimals(false, &[9, 9, 9, 6], 0, 3), "1");
        // 12.345 at 2 decimals -> "12.35"
        assert_eq!(round_to_fixed_decimals(true, &[1, 2, 3, 4, 5], 2, 2), "-12.35");
        // 0.00004 at 3 decimals rounds to zero, printed unsigned
        assert_eq!(round_to_fixed_decimals(true, &[4], -4, 3), "0");
        assert_eq!(round_to_fixed_decimals(false, &[], 0, 3), "0");
    }

    #[test]
    fn fixed_decimal_pair_keeps_digits_beyond_f64() {
        // 0.5 + 1e-30: only representable at high precision.
        let hp = HpComplex::from_f64_pair(0.5, 0.0, 256).add(&HpComplex::from_f64_pair(1.0e-30, 0.0, 256));
        let (re, _) = hp.to_fixed_decimal_pair(32);
        assert_eq!(re, format!("0.5{}1", "0".repeat(28)));
    }

    #[test]
    fn required_bits_grows_with_zoom_and_is_clamped() {
        assert_eq!(required_bits(1.0), MIN_PRECISION_BITS);
        assert!(required_bits(1.0e10) > required_bits(1.0));
        assert!(required_bits(1.0e300) <= MAX_PRECISION_BITS);
    }
}
