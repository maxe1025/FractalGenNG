//! The 3D viewport: a fly camera over the ray-marched fractal. W/A/S/D move,
//! Q/E move down/up, left-drag looks around, the scroll wheel sets the fly
//! speed and Shift boosts it. Speed is measured in multiples of the camera's
//! clearance (its distance to the surface) per second, so flying stays
//! controllable however deep into a crevice the camera gets — and
//! `Params3D::move_camera` keeps it from ever passing through a wall.
//!
//! While the camera moves, the fractal renders at a fraction of the
//! resolution and sharpens once the camera has been still for a moment.

use std::time::{Duration, Instant};

use eframe::egui;
use egui::Key;

use crate::app::FractalApp;

/// Render-resolution divisor while the camera is moving.
const PREVIEW_DOWNSCALE: f32 = 3.0;
/// How long the camera must be still before rendering at full resolution.
const SETTLE_TIME: Duration = Duration::from_millis(200);
/// Fly-speed multiplier while Shift is held.
const BOOST: f64 = 4.0;
pub const FLY_SPEED_RANGE: std::ops::RangeInclusive<f64> = 0.05..=50.0;
/// Floor for the clearance the fly speed scales with, so a camera that ended
/// up inside the solid (clearance ≤ 0) can still move back out.
const MIN_SPEED_CLEARANCE: f64 = 1e-4;

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    let available = ui.available_size();
    let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());

    if let Some(started) = app.view.path_preview {
        play_path_preview(ui, app, started);
    } else if fly(ui, &response, rect, app) {
        app.view.last_camera_motion = Some(Instant::now());
    }
    let previewing = app.view.last_camera_motion.is_some_and(|t| t.elapsed() < SETTLE_TIME);
    let divisor = if previewing { PREVIEW_DOWNSCALE } else { 1.0 };
    let width = (rect.width() / divisor).max(1.0) as u32;
    let height = (rect.height() / divisor).max(1.0) as u32;

    let device = app.render_state.device.clone();
    {
        let mut egui_renderer = app.render_state.renderer.write();
        app.renderer.resize(&device, &mut egui_renderer, width, height);
    }
    app.renderer.render_3d(&app.render_state.device, &app.render_state.queue, &app.params3d);

    let full_uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    ui.painter().image(app.renderer.texture_id(), rect, full_uv, egui::Color32::WHITE);
    if previewing {
        // Come back once the camera has settled to render the sharp frame.
        ui.ctx().request_repaint_after(SETTLE_TIME);
    }

    if app.overlay.show_in_viewport {
        crate::ui::math_overlay::paint(ui, rect, app.overlay_lines(), &app.overlay);
    }
}

/// Moves the camera along the Export tab's camera path in real time at the
/// animation's fps (in coarse preview resolution), stopping at the end.
fn play_path_preview(ui: &egui::Ui, app: &mut FractalApp, started: Instant) {
    let path = app.camera_path();
    let frame = (started.elapsed().as_secs_f64() * f64::from(app.export.anim_fps)) as u32;
    if frame > path.last_frame() {
        app.view.path_preview = None;
        return;
    }
    app.params3d.camera = path.camera_at_frame(frame);
    app.export.anim3d_scrub_frame = frame;
    app.view.last_camera_motion = Some(Instant::now());
    ui.ctx().request_repaint();
}

/// Camera readout and controls hint for the bottom status bar.
pub fn status_text(app: &FractalApp) -> String {
    let p = app.params3d.camera.position;
    format!(
        "camera: {:.5}, {:.5}, {:.5}   clearance: {:.3e}   speed: ×{:.2}   \
         [W/A/S/D move · Q/E down/up · drag to look · scroll: speed · Shift: fast]",
        p[0],
        p[1],
        p[2],
        app.params3d.camera_clearance(),
        app.view.fly_speed
    )
}

/// Applies this frame's mouse-look, scroll and movement input. Returns
/// whether the camera moved.
fn fly(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect, app: &mut FractalApp) -> bool {
    let mut moved = false;

    if response.dragged_by(egui::PointerButton::Primary) && rect.height() > 0.0 {
        let delta = response.drag_delta();
        let camera = &mut app.params3d.camera;
        // Dragging across the full viewport height turns by one field of view.
        let radians_per_point = camera.fov_y_degrees.to_radians() / f64::from(rect.height());
        camera.rotate(f64::from(delta.x) * radians_per_point, -f64::from(delta.y) * radians_per_point);
        moved |= delta != egui::Vec2::ZERO;
    }

    if response.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let speed = app.view.fly_speed * (f64::from(scroll) * 0.003).exp();
            app.view.fly_speed = speed.clamp(*FLY_SPEED_RANGE.start(), *FLY_SPEED_RANGE.end());
        }
    }

    // Don't steal letters typed into a text field in the side panel.
    if ui.ctx().egui_wants_keyboard_input() {
        return moved;
    }
    let (axes, dt, boost) = ui.input(|i| {
        let axis = |pos: Key, neg: Key| f64::from(u8::from(i.key_down(pos))) - f64::from(u8::from(i.key_down(neg)));
        // (right, up, forward)
        let axes = [axis(Key::D, Key::A), axis(Key::E, Key::Q), axis(Key::W, Key::S)];
        // Clamped so a hitch (or the first frame after idling) can't jump far.
        (axes, f64::from(i.stable_dt).min(0.1), i.modifiers.shift)
    });
    if axes == [0.0; 3] {
        return moved;
    }

    let camera = &app.params3d.camera;
    let basis = [camera.right(), camera.up(), camera.forward()];
    let mut dir = [0.0; 3];
    for (axis, vector) in axes.iter().zip(basis) {
        for (d, v) in dir.iter_mut().zip(vector) {
            *d += axis * v;
        }
    }
    let len = dir.iter().map(|d| d * d).sum::<f64>().sqrt();
    if len > 0.0 {
        let boost = if boost { BOOST } else { 1.0 };
        let clearance = app.params3d.camera_clearance().max(MIN_SPEED_CLEARANCE);
        let distance = app.view.fly_speed * boost * clearance * dt;
        app.params3d.move_camera(dir.map(|d| d / len * distance));
        moved = true;
    }
    // Keep frames coming while a key is held, even without mouse input.
    ui.ctx().request_repaint();
    moved
}
