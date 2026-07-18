use crate::hp::HpComplex;
use crate::kinds::FractalKind;

/// Computes the arbitrary-precision reference orbit anchored at `center`,
/// downsampled to `f64` pairs — valid because the orbit itself stays
/// bailout-bounded (O(1) magnitude) regardless of zoom depth; only the
/// *anchor point* needs arbitrary precision to compute it accurately in the
/// first place. Per this codebase's conventions (`kinds/mandelbrot.rs`,
/// `kinds/julia.rs`):
///
/// - Mandelbrot (`z_0=0, c=pixel`): the reference is anchored at `C_ref =
///   center`, `Z_ref_0 = 0`, `Z_ref_{n+1} = Z_ref_n^2 + C_ref`.
/// - Julia (`z_0=pixel, c=seed`): the reference is anchored at `Z_ref_0 =
///   center` (since it's `z_0`, not `c`, that's being deep-zoomed here),
///   `Z_ref_{n+1} = Z_ref_n^2 + seed` (seed stays plain `f64` — it isn't
///   being zoomed).
///
/// Stops at `max_iterations` or once `|Z_ref_n|^2` exceeds `bailout_sq`,
/// whichever comes first; the returned `Vec`'s length is that stopping point.
pub fn compute_reference_orbit(
    center: &HpComplex,
    kind: &FractalKind,
    max_iterations: u32,
    bailout_radius: f64,
) -> Vec<[f64; 2]> {
    let bailout_sq = bailout_radius * bailout_radius;
    let precision_bits = center.precision_bits();
    let mut orbit = Vec::with_capacity(max_iterations as usize + 1);

    let (mut z, addend) = match kind {
        FractalKind::Mandelbrot => (HpComplex::from_f64_pair(0.0, 0.0, precision_bits), center.clone()),
        FractalKind::Julia { seed } => {
            (center.clone(), HpComplex::from_f64_pair(seed[0], seed[1], precision_bits))
        }
    };

    for _ in 0..=max_iterations {
        let (zx, zy) = z.to_f64_pair();
        orbit.push([zx, zy]);
        if zx * zx + zy * zy > bailout_sq {
            break;
        }
        z = z.square().add(&addend);
    }

    orbit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_mandelbrot_center_runs_full_iteration_count() {
        let center = HpComplex::from_f64_pair(-0.5, 0.0, 64);
        let orbit = compute_reference_orbit(&center, &FractalKind::Mandelbrot, 500, 4.0);
        assert_eq!(orbit.len(), 501);
    }

    #[test]
    fn escaping_mandelbrot_center_stops_early() {
        let center = HpComplex::from_f64_pair(2.0, 2.0, 64);
        let orbit = compute_reference_orbit(&center, &FractalKind::Mandelbrot, 500, 4.0);
        assert!(orbit.len() < 10, "expected a quick escape, got {} iterations", orbit.len());
        let last = orbit.last().unwrap();
        assert!(last[0] * last[0] + last[1] * last[1] > 16.0);
    }

    #[test]
    fn julia_orbit_uses_center_as_z0_and_seed_as_c() {
        // z0=0 with seed=0 never escapes (stays at exactly 0 forever).
        let center = HpComplex::from_f64_pair(0.0, 0.0, 64);
        let orbit = compute_reference_orbit(&center, &FractalKind::Julia { seed: [0.0, 0.0] }, 200, 4.0);
        assert_eq!(orbit.len(), 201);
        assert_eq!(orbit[0], [0.0, 0.0]);
    }

    #[test]
    fn julia_orbit_with_escaping_z0_stops_early() {
        let center = HpComplex::from_f64_pair(3.0, 3.0, 64);
        let orbit = compute_reference_orbit(&center, &FractalKind::Julia { seed: [-0.4, 0.6] }, 200, 4.0);
        assert!(orbit.len() < 10, "expected a quick escape, got {} iterations", orbit.len());
    }
}
