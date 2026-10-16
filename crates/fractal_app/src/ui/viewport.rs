use eframe::egui;
use egui::load::SizedTexture;
use fractal_core::hp::{required_bits, MARGIN_BITS, MAX_INTERACTIVE_ZOOM, ZOOM_F32_SAFE_CEILING};
use fractal_core::kinds::FractalKind;

use crate::app::FractalApp;

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    upgrade_precision_if_needed(app);

    #[cfg(debug_assertions)]
    handle_debug_deep_zoom_shortcut(ui, app);

    let available = ui.available_size();
    let width = available.x.max(1.0) as u32;
    let height = available.y.max(1.0) as u32;

    let device = app.render_state.device.clone();
    {
        let mut egui_renderer = app.render_state.renderer.write();
        app.renderer.resize(&device, &mut egui_renderer, width, height);
    }
    app.renderer.render(&app.render_state.device, &app.render_state.queue, &app.params);

    let texture_id = app.renderer.texture_id();
    let sized = SizedTexture::new(texture_id, available);
    let response = ui.add(egui::Image::new(sized).sense(egui::Sense::click_and_drag()));
    let rect = response.rect;

    if app.overlay.show_in_viewport {
        crate::ui::math_overlay::paint(ui, rect, app.overlay_lines(), &app.overlay);
    }

    if response.dragged() && rect.width() > 0.0 && rect.height() > 0.0 {
        let delta = response.drag_delta();
        let half_extent = 2.0 / app.params.zoom;
        let aspect = (rect.width() / rect.height()) as f64;
        let per_px_x = (half_extent * aspect * 2.0) / rect.width() as f64;
        let per_px_y = (half_extent * 2.0) / rect.height() as f64;
        let dx = -(delta.x as f64) * per_px_x;
        let dy = delta.y as f64 * per_px_y;
        app.params.center = app.params.center.add_f64_delta(dx, dy);
    }

    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    if scroll.abs() > 0.0
        && let Some(pointer) = response.hover_pos()
    {
        zoom_at(app, rect, pointer, scroll);
    }

    if response.clicked()
        && app.view.julia_seed_picker_active
        && let Some(pos) = response.interact_pointer_pos()
    {
        let world = screen_to_world_f64(app, rect, pos);
        app.params.kind = FractalKind::Julia { seed: world };
        app.view.julia_seed_picker_active = false;
    }
}

/// Camera/capability readout shown in the bottom status bar (moved out of a
/// viewport overlay so the render stays clean and edge-to-edge, Blender-
/// viewport style).
pub fn status_text(app: &FractalApp) -> String {
    let (center_x, center_y) = app.params.center.to_f64_pair();
    let mut status = format!("zoom: {:.3e}   center: {center_x:.6}, {center_y:.6}", app.params.zoom);
    if !app.renderer.supports_deep_zoom() {
        status.push_str("   [deep zoom unavailable: GPU lacks 64-bit shader float support]");
    }
    status
}

/// Bumps `center`'s precision up when the zoom has deepened past what its
/// current bit count safely covers, or down (with hysteresis, so it doesn't
/// churn right at a boundary) when zoom has shallowed back out.
fn upgrade_precision_if_needed(app: &mut FractalApp) {
    let needed = required_bits(app.params.zoom);
    let current = app.params.center.precision_bits();
    if needed > current || needed + MARGIN_BITS < current {
        app.params.center = app.params.center.with_precision(needed);
    }
}

/// Dev-build-only shortcut for verifying deep zoom without thousands of
/// physical scroll ticks: press "9" to multiply the *current* zoom by 100,
/// keeping whatever center the user already navigated to via scroll-zoom
/// (reliable) rather than trusting an unverified hardcoded coordinate.
/// Native window click automation is unreliable in this project's dev
/// environment — see feedback_windows_gui_automation in project memory.
#[cfg(debug_assertions)]
fn handle_debug_deep_zoom_shortcut(ui: &egui::Ui, app: &mut FractalApp) {
    if ui.input(|i| i.key_pressed(egui::Key::Num9)) {
        app.params.zoom = (app.params.zoom * 100.0).min(effective_max_zoom(app));
        app.params.center = app.params.center.with_precision(required_bits(app.params.zoom));
    }
    // "8": jump to a known deep-zoom-friendly coordinate (a "seahorse valley"
    // spiral point) at 1e9 zoom with a high iteration budget — this specific
    // reference point needs ~3087 iterations to escape (confirmed via
    // crates/fractal_core/examples/probe_orbit.rs), so a low max_iterations
    // would show uniform "interior" even though it's genuinely on the
    // boundary; that's not a bug, just an iteration-budget mismatch.
    if ui.input(|i| i.key_pressed(egui::Key::Num8)) {
        app.params.zoom = 1.0e9;
        app.params.max_iterations = 5000;
        app.params.center = fractal_core::hp::HpComplex::from_f64_pair(
            -0.743_643_887_037_151,
            0.131_825_904_205_330,
            required_bits(app.params.zoom),
        );
    }
}

/// The small, always-f64-safe offset from `center` to a screen position —
/// deliberately never combined with `center` itself except via a single
/// [`fractal_core::HpComplex::add_f64_delta`] call, so panning/zooming never
/// materializes a low-precision intermediate "world position".
fn pixel_offset(rect: egui::Rect, pos: egui::Pos2, zoom: f64) -> [f64; 2] {
    let px = ((pos.x - rect.left()) / rect.width()) as f64 * 2.0 - 1.0;
    let py = ((pos.y - rect.top()) / rect.height()) as f64 * 2.0 - 1.0;
    let half_extent = 2.0 / zoom;
    let aspect = (rect.width() / rect.height()) as f64;
    [px * half_extent * aspect, -py * half_extent]
}

/// Materializes a full `f64` world position — only safe to use where the
/// result doesn't get fed back into `center` (e.g. the Julia seed picker,
/// which stays plain-`f64` by design).
fn screen_to_world_f64(app: &FractalApp, rect: egui::Rect, pos: egui::Pos2) -> [f64; 2] {
    let (cx, cy) = app.params.center.to_f64_pair();
    let [ox, oy] = pixel_offset(rect, pos, app.params.zoom);
    [cx + ox, cy + oy]
}

/// The deepest zoom currently reachable: needs both a GPU that supports the
/// perturbation shader and a fractal kind that pipeline knows how to render
/// (see `FractalKind::supports_perturbation`) — otherwise clamped to where
/// the standard f32 shader stays accurate.
fn effective_max_zoom(app: &FractalApp) -> f64 {
    if app.renderer.supports_deep_zoom() && app.params.kind.supports_perturbation() {
        MAX_INTERACTIVE_ZOOM
    } else {
        ZOOM_F32_SAFE_CEILING
    }
}

fn zoom_at(app: &mut FractalApp, rect: egui::Rect, pointer: egui::Pos2, scroll: f32) {
    let offset_before = pixel_offset(rect, pointer, app.params.zoom);
    let factor = (scroll as f64 * 0.003).exp();
    app.params.zoom = (app.params.zoom * factor).clamp(0.01, effective_max_zoom(app));
    let offset_after = pixel_offset(rect, pointer, app.params.zoom);
    // world_before - world_after = (center+offset_before) - (center+offset_after)
    //                            = offset_before - offset_after
    // — the center terms cancel algebraically, so only their small
    // difference (always f64-safe) needs to be added to center.
    let dx = offset_before[0] - offset_after[0];
    let dy = offset_before[1] - offset_after[1];
    app.params.center = app.params.center.add_f64_delta(dx, dy);
}
