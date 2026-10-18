# Changelog

All notable changes to FractalGen are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project follows [Semantic Versioning](https://semver.org/).

## [0.5.0] - 2026-10-18

### Added

- **Quaternion Julia** as a 3D formula: `z → z² + c` with quaternions,
  sliced into 3D at a chosen fourth coordinate — smooth, organic bodies
  that flow into one another as the seed `c` changes. Works with the
  inside-out sphere inversion, the fly camera and the math overlay.
- **Animated formula parameters in 3D**: keyframes (I) now also store the
  formula's parameters, and the timeline interpolates them smoothly
  (never overshooting a keyed value) — e.g. morph a quaternion Julia's
  seed, a Mandelbulb's power, a Mandelbox's scale or a KIFS rotation over
  a flight. Older keyframes without stored parameters keep the formula
  fixed.

### Changed

- 3D surfaces look up the palette mirrored (0 → 1 → 0) instead of
  wrapping, which removes the hard, aliased color seams that showed on
  smooth surfaces.

## [0.4.0] - 2026-10-16

### Added

- **Project files** (`.fgproj`, human-readable RON): save and reopen a
  whole piece of work — the 2D and 3D fractals with their palettes, the
  animation timeline (keyframes, range, fps, playhead), the math-overlay
  and export settings, and which mode was showing. A File menu in the
  header offers New, Open…, Save and Save As… (Ctrl+N/O/S,
  Ctrl+Shift+S). Files carry a format version, so a newer file is refused
  with a clear message instead of loading half-way.
- **Unsaved-changes protection**: the window title shows the project name
  with `*` while it has unsaved changes, and New, Open and closing the
  window ask whether to save first.

### Fixed

- Saving and reloading a deep-zoom center (in bookmarks and projects)
  could shift it by one unit in its last digit per cycle, because both
  directions truncated; it now round-trips exactly.

## [0.3.2] - 2026-10-16

### Added

- **Timeline editor** below the viewport, for 2D and 3D animations alike
  (modeled on Blender's): a frame ruler to scrub, a keyframe track,
  play/pause with looping, jumps to start/end and between keyframes, a
  frame field, zoom (wheel) and pan (Ctrl+wheel, middle-drag), and frames
  where a 3D camera path runs through the fractal marked in red.
- **Blender-style keying**: put the playhead on a frame, set up the view or
  camera, press I (or "◆ Key") — replacing any keyframe on that frame.
  Click a keyframe to select it and jump there, drag it to retime it,
  delete it with X/Delete; easing is set per keyframe in the timeline
  header. Shortcuts: Space, I, X/Delete, ←/→, Shift+←/→, ↑/↓.

### Changed

- The animation's frame range is now a free start/end range, independent
  of the keyframes; frames before the first or after the last keyframe
  hold its values. Animation export renders that range.
- The keyframe strips, per-keyframe editors and scrub slider in the Export
  tab were replaced by the timeline; the Export tab now only renders.

## [0.3.1] - 2026-10-16

### Added

- **Camera tilt (roll)** in 3D: Z/C tilt the camera around its viewing
  direction (Shift for faster), with a tilt slider and a "Level" button in
  the camera panel. Mouse-look follows the tilted image, tilt is keyframed
  and smoothly interpolated in camera-path animations (always the short
  way round), and the math overlay states it.

## [0.3.0] - 2026-10-16

### Added

- **Menger / KIFS**: a kaleidoscopic IFS with scale, offset, and two
  rotation angles. Its defaults give exactly the Menger sponge and its
  grid of corridors; rotation twists it into ornamental architecture.
- **Pseudo-Kleinian**: Knighty's space-filling formula of arches and
  tunnels — the camera is always inside it.
- **Sphere inversion ("Inside-Out")** for every 3D formula: with the
  center inside the fractal's solid, the space around the fractal becomes
  a closed cave walled in by its surface — e.g. an inside-out Mandelbulb.
- **3D camera-path animation**: keyframes captured from the fly camera
  ("Add keyframe from camera", "Use current camera", "Go to"), joined by
  a centripetal spline for position — no overshoot, even when diving from
  an overview into a tiny crevice — and a monotone spline for view angles
  and field of view. Per-segment easing, a scrub slider, real-time
  preview playback in the viewport, and a warning for frames where the
  path runs through the fractal. Frames export as PNG sequences with the
  math overlay's per-frame values and optional ffmpeg stitching, like 2D
  animations.

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
