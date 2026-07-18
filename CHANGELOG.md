# Changelog

All notable changes to FractalGen are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project follows [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-07-18

Initial public release.

### Added

- **Fractal kinds**: Mandelbrot, Julia (with click-to-seed), Burning Ship,
  Tricorn, and Multibrot (configurable power).
- **Deep zoom**: arbitrary-precision camera plus a perturbation-theory GPU
  pipeline (native f64, in-shader glitch detection/rebasing, CPU-side
  secondary-reference correction) reaching roughly 1e250x on Mandelbrot and
  Julia, on GPUs that support `SHADER_F64`. Other fractal kinds render on
  the standard f32 pipeline up to roughly 1e5x.
- **Coloring**: 8 built-in palettes (Fire, Ocean, Grayscale, Twilight,
  Forest, Ice, Neon, Rainbow), a hand-rolled gradient stop editor, and an
  optional distance-estimator shading mode that reveals fine boundary detail
  (Mandelbrot/Julia).
- **Interactive viewport**: pan/zoom-at-cursor, click-to-seed Julia sets.
- **Export**: still-image PNG export with supersampling; zoom-animation
  frame-sequence export with Linear, Exponential, and Accelerating zoom
  curves, an fps field with a live duration estimate, and optional
  video stitching via a system `ffmpeg` if present on `PATH`.
- **Bookmarks**: save/load/go-to named focus points.
- **UI**: a Blender-inspired dark theme with an icon-tabbed properties
  panel, header/status bars, and an in-app About/Changelog tab.
