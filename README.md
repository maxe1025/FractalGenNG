# FractalGen

A GPU-accelerated fractal explorer built in Rust, with true deep zoom via
perturbation theory, still-image and animation export, and a
Blender-inspired dark UI.

![FractalGen screenshot](docs/screenshot.png)

## Features

- **Fractal kinds**: Mandelbrot, Julia (click-to-seed), Burning Ship,
  Tricorn, and Multibrot (configurable power).
- **Deep zoom**: an arbitrary-precision camera plus a perturbation-theory
  GPU pipeline (in-shader glitch detection/rebasing, CPU-side secondary-
  reference correction) reaching roughly 1e250x on Mandelbrot and Julia, on
  GPUs that support `SHADER_F64`.
- **Coloring**: 8 built-in palettes, a gradient stop editor, and an optional
  distance-estimator shading mode that reveals fine boundary detail.
- **Interactive viewport**: pan/zoom-at-cursor, click-to-seed Julia sets.
- **3D fractals**: ray-marched Mandelbox, Mandelbulb, Menger sponge/KIFS,
  and pseudo-Kleinian, plus a sphere-inversion mode that turns any of
  them inside out into a closed cave. A fly camera (W/A/S/D/Q/E, drag to
  look, Z/C to tilt) travels through their caves and corridors without
  passing through walls.
- **Math overlay**: the formula and the actual values behind the current
  frame, shown over the viewport and/or burned into exports.
- **Timeline**: a Blender-style timeline below the viewport — scrub the
  frame ruler, play/loop, set a free start/end range, and key the current
  view at the playhead with I (zoom, Julia seed, and Multibrot power in
  2D; smooth camera flights in 3D), with draggable keyframes and
  per-keyframe easing.
- **Export**: still-image PNG export with supersampling (2D and 3D);
  animation frame-sequence export of the timeline's range, and optional
  video stitching via a system `ffmpeg` if present on `PATH`.
- **Projects**: save and reopen everything you set up — 2D and 3D
  fractals, palettes, the animation timeline, overlay and export settings
  — as a `.fgproj` file (File menu, Ctrl+S/O/N).
- **Bookmarks**: save/load/go-to named focus points.

See [CHANGELOG.md](CHANGELOG.md) for the full version history — it's also
shown in-app under the ℹ️ About tab.

## Requirements

- Rust (edition 2024 — install via [rustup](https://rustup.rs/))
- A GPU with Vulkan or DX12 support (the app renders via `wgpu`); deep zoom
  additionally needs a GPU/driver that exposes `SHADER_F64`
- Optional: [ffmpeg](https://ffmpeg.org/) on `PATH`, to stitch an exported
  animation's PNG frames into a video from within the app

## Building & running

```sh
cargo run --release --bin fractal_app
```

Run the test suite with:

```sh
cargo test --workspace
```

## License

MIT licensed — see [LICENSE](LICENSE). Copyright (c) 2026 Maxe.
