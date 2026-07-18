use serde::{Deserialize, Serialize};

/// A single color stop in a gradient. `position` is in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub position: f32,
    pub color: [u8; 4],
}

/// Maximum stop count the GPU uniform buffer reserves space for.
pub const MAX_GPU_STOPS: usize = 8;

/// A built-in palette constructor, as returned by [`Palette::presets`].
pub type PaletteCtor = fn() -> Palette;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Palette {
    /// Sorted ascending by `position`, at most `MAX_GPU_STOPS` entries.
    pub stops: Vec<GradientStop>,
    /// How many times the gradient repeats across the escape-iteration range,
    /// giving the classic banded fractal look.
    pub cycle_count: f32,
    pub interior_color: [u8; 4],
}

impl Palette {
    pub fn fire() -> Self {
        Self {
            stops: vec![
                GradientStop { position: 0.0, color: [10, 10, 40, 255] },
                GradientStop { position: 0.25, color: [80, 30, 120, 255] },
                GradientStop { position: 0.5, color: [220, 60, 40, 255] },
                GradientStop { position: 0.75, color: [250, 180, 30, 255] },
                GradientStop { position: 1.0, color: [255, 250, 220, 255] },
            ],
            cycle_count: 6.0,
            interior_color: [0, 0, 0, 255],
        }
    }

    pub fn ocean() -> Self {
        Self {
            stops: vec![
                GradientStop { position: 0.0, color: [5, 15, 40, 255] },
                GradientStop { position: 0.3, color: [10, 60, 100, 255] },
                GradientStop { position: 0.6, color: [20, 140, 160, 255] },
                GradientStop { position: 0.85, color: [140, 220, 210, 255] },
                GradientStop { position: 1.0, color: [240, 255, 250, 255] },
            ],
            cycle_count: 6.0,
            interior_color: [0, 0, 10, 255],
        }
    }

    pub fn grayscale() -> Self {
        Self {
            stops: vec![
                GradientStop { position: 0.0, color: [0, 0, 0, 255] },
                GradientStop { position: 1.0, color: [255, 255, 255, 255] },
            ],
            cycle_count: 10.0,
            interior_color: [0, 0, 0, 255],
        }
    }

    pub fn twilight() -> Self {
        Self {
            stops: vec![
                GradientStop { position: 0.0, color: [15, 10, 40, 255] },
                GradientStop { position: 0.3, color: [80, 20, 90, 255] },
                GradientStop { position: 0.55, color: [180, 50, 110, 255] },
                GradientStop { position: 0.8, color: [255, 140, 120, 255] },
                GradientStop { position: 1.0, color: [255, 230, 200, 255] },
            ],
            cycle_count: 6.0,
            interior_color: [10, 5, 20, 255],
        }
    }

    pub fn forest() -> Self {
        Self {
            stops: vec![
                GradientStop { position: 0.0, color: [5, 15, 10, 255] },
                GradientStop { position: 0.3, color: [15, 60, 30, 255] },
                GradientStop { position: 0.55, color: [60, 110, 40, 255] },
                GradientStop { position: 0.8, color: [170, 190, 60, 255] },
                GradientStop { position: 1.0, color: [240, 250, 220, 255] },
            ],
            cycle_count: 6.0,
            interior_color: [0, 10, 5, 255],
        }
    }

    pub fn ice() -> Self {
        Self {
            stops: vec![
                GradientStop { position: 0.0, color: [0, 5, 15, 255] },
                GradientStop { position: 0.3, color: [10, 50, 90, 255] },
                GradientStop { position: 0.6, color: [60, 160, 220, 255] },
                GradientStop { position: 0.85, color: [180, 230, 255, 255] },
                GradientStop { position: 1.0, color: [255, 255, 255, 255] },
            ],
            cycle_count: 6.0,
            interior_color: [0, 0, 10, 255],
        }
    }

    pub fn neon() -> Self {
        Self {
            stops: vec![
                GradientStop { position: 0.0, color: [5, 0, 20, 255] },
                GradientStop { position: 0.25, color: [255, 0, 180, 255] },
                GradientStop { position: 0.5, color: [0, 255, 220, 255] },
                GradientStop { position: 0.75, color: [255, 255, 0, 255] },
                GradientStop { position: 1.0, color: [255, 255, 255, 255] },
            ],
            cycle_count: 5.0,
            interior_color: [0, 0, 0, 255],
        }
    }

    /// The traditional full-hue-sweep look classic fractal software (e.g.
    /// Fractint) is known for. Starts and ends on the same red so repeated
    /// cycles (`cycle_count`) tile seamlessly with no visible seam.
    pub fn rainbow() -> Self {
        Self {
            stops: vec![
                GradientStop { position: 0.0, color: [255, 0, 0, 255] },
                GradientStop { position: 0.17, color: [255, 165, 0, 255] },
                GradientStop { position: 0.33, color: [255, 255, 0, 255] },
                GradientStop { position: 0.5, color: [0, 200, 60, 255] },
                GradientStop { position: 0.67, color: [0, 120, 255, 255] },
                GradientStop { position: 0.83, color: [160, 0, 220, 255] },
                GradientStop { position: 1.0, color: [255, 0, 0, 255] },
            ],
            cycle_count: 3.0,
            interior_color: [0, 0, 0, 255],
        }
    }

    /// Built-in presets shown in the palette editor's preset dropdown.
    pub fn presets() -> [(&'static str, PaletteCtor); 8] {
        [
            ("Fire", Palette::fire as PaletteCtor),
            ("Ocean", Palette::ocean),
            ("Grayscale", Palette::grayscale),
            ("Twilight", Palette::twilight),
            ("Forest", Palette::forest),
            ("Ice", Palette::ice),
            ("Neon", Palette::neon),
            ("Rainbow", Palette::rainbow),
        ]
    }

    /// Sample the gradient at `t` (wrapped into `0.0..=1.0`), linearly interpolating
    /// between the surrounding stops. Used by the palette editor preview bar; the
    /// GPU shader performs the equivalent lookup independently for rendering.
    pub fn sample(&self, t: f32) -> [u8; 4] {
        let t = t.rem_euclid(1.0);
        if self.stops.is_empty() {
            return [0, 0, 0, 255];
        }
        if self.stops.len() == 1 || t <= self.stops[0].position {
            return self.stops[0].color;
        }
        if let Some(last) = self.stops.last()
            && t >= last.position
        {
            return last.color;
        }
        for pair in self.stops.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if t >= a.position && t <= b.position {
                let span = (b.position - a.position).max(f32::EPSILON);
                let f = (t - a.position) / span;
                let mut out = [0u8; 4];
                for (i, channel) in out.iter_mut().enumerate() {
                    *channel = (a.color[i] as f32 + (b.color[i] as f32 - a.color[i] as f32) * f).round() as u8;
                }
                return out;
            }
        }
        self.stops.last().copied().map(|s| s.color).unwrap_or([0, 0, 0, 255])
    }

    /// Normalized `(position, rgba in 0.0..=1.0)` pairs, padded/truncated to
    /// `MAX_GPU_STOPS` entries, for upload to the GPU uniform buffer.
    pub fn to_gpu_stops(&self) -> ([f32; MAX_GPU_STOPS], [[f32; 4]; MAX_GPU_STOPS], u32) {
        let mut positions = [0.0f32; MAX_GPU_STOPS];
        let mut colors = [[0.0f32; 4]; MAX_GPU_STOPS];
        let count = self.stops.len().min(MAX_GPU_STOPS);
        for i in 0..count {
            positions[i] = self.stops[i].position;
            colors[i] = self.stops[i].color.map(|c| c as f32 / 255.0);
        }
        (positions, colors, count as u32)
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::fire()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_have_distinct_names_and_fit_gpu_stop_limit() {
        let presets = Palette::presets();
        let mut names: Vec<&str> = presets.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), presets.len(), "preset names must be unique");

        for (name, ctor) in presets {
            let palette = ctor();
            assert!(!palette.stops.is_empty(), "{name} has no stops");
            assert!(
                palette.stops.len() <= MAX_GPU_STOPS,
                "{name} has {} stops, exceeding MAX_GPU_STOPS={MAX_GPU_STOPS}",
                palette.stops.len()
            );
        }
    }

    #[test]
    fn rainbow_wraps_seamlessly() {
        // First and last stop must match so repeated cycle_count passes
        // don't show a visible seam where the gradient jumps back to red.
        let rainbow = Palette::rainbow();
        assert_eq!(rainbow.stops.first().unwrap().color, rainbow.stops.last().unwrap().color);
    }
}
