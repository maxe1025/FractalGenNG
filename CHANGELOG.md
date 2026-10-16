# Changelog

All notable changes to FractalGen are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project follows [Semantic Versioning](https://semver.org/).

## [0.2.0] - 2026-10-16

### Added

- **3D mode**: a 2D/3D switch in the header. 3D ray-marches distance-
  estimated fractals — the Mandelbox (scale, min/fixed radius; negative
  scales give cave-like interiors) and the Mandelbulb (configurable
  power) — with headlight and key lighting, ambient occlusion, orbit-trap
  palette coloring, and fog scaled to the camera's distance from the
  surface.
- **Fly camera**: W/A/S/D/Q/E to fly, drag to look, scroll to set the
  speed, Shift to boost. Speed scales with the free space around the
  camera, and the camera can approach a wall arbitrarily closely but never
  pass through it. The viewport renders a coarse preview while moving and
  sharpens once the camera is still. 3D views export as PNG with
  supersampling.
- **Math overlay**: shows the formula and the actual values behind the
  current frame (center, zoom, seed, iterations; in 3D the camera and
  ray-march settings), optionally including how pixels are colored. Can be
  shown over the viewport and/or burned into exported images and
  animation frames, with per-frame values; corner and text size are
  configurable.
- **Keyframe timeline**: animations are built from a draggable keyframe
  strip — add, move, and remove keyframes, each with its own zoom and
  easing for the segment leading into it.
- **Parameter animation**: Julia seeds and Multibrot powers can be
  animated across keyframes alongside zoom.

### Changed

- Zoom is now always interpolated geometrically (constant ratio per
  frame); the zoom curves became per-segment easings (Linear,
  Accelerating). The former plain-arithmetic "Linear" zoom curve was
  removed.

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
