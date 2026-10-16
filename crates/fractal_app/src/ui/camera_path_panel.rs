//! The 3D animation editor in the Export tab. Keyframes are camera poses
//! captured from the fly camera: fly somewhere, add or update a keyframe
//! from the current camera, and the frames in between follow a smooth
//! spline (see `fractal_core::export::camera_path`). Scrubbing and the
//! preview playback drive the viewport camera along the path.

use std::time::Instant;

use eframe::egui;
use fractal_core::export::animation::Easing;
use fractal_core::export::camera_path::CameraKeyframe;

use crate::app::FractalApp;
use crate::ui::export_panel::{easing_editor, timeline_strip};
use crate::ui::theme;

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    seed_keyframes(app);

    timeline_strip(ui, &mut app.export.anim3d_keyframes, &mut app.export.anim3d_selected_keyframe, "camera_kf");
    if ui
        .small_button("+ Add keyframe from camera")
        .on_hover_text("Inserts the current viewport camera after the selected keyframe.")
        .clicked()
    {
        add_keyframe_from_camera(app);
    }

    ui.add_space(4.0);
    preview_controls(ui, app);
    blocked_warning(ui, app);

    ui.add_space(6.0);
    ui.separator();
    selected_keyframe_editor(ui, app);
}

/// A fresh path starts and ends at the current camera; fly elsewhere and
/// "Use current camera" on the end keyframe to give it somewhere to go.
fn seed_keyframes(app: &mut FractalApp) {
    if app.export.anim3d_keyframes.len() >= 2 {
        return;
    }
    let camera = app.params3d.camera;
    let last = app.export.anim_frame_count.saturating_sub(1).max(1);
    app.export.anim3d_keyframes = vec![
        CameraKeyframe { frame: 0, camera, easing: Easing::Linear },
        CameraKeyframe { frame: last, camera, easing: Easing::Linear },
    ];
    app.export.anim3d_selected_keyframe = 1;
}

/// Inserts the current camera into the gap after the selected keyframe (or
/// before the last, if the last is selected). No-op if that gap has no
/// free frame left.
fn add_keyframe_from_camera(app: &mut FractalApp) {
    let keys = &mut app.export.anim3d_keyframes;
    let insert_after = app.export.anim3d_selected_keyframe.min(keys.len().saturating_sub(2));
    let a = keys[insert_after].frame;
    let b = keys[insert_after + 1].frame;
    if b <= a + 1 {
        return;
    }
    let camera = app.params3d.camera;
    keys.insert(insert_after + 1, CameraKeyframe { frame: a + (b - a) / 2, camera, easing: Easing::Linear });
    app.export.anim3d_selected_keyframe = insert_after + 1;
}

fn preview_controls(ui: &mut egui::Ui, app: &mut FractalApp) {
    let last_frame = app.export.anim3d_keyframes.last().map_or(0, |k| k.frame);
    ui.horizontal(|ui| {
        let playing = app.view.path_preview.is_some();
        if ui.button(if playing { "■ Stop" } else { "▶ Preview" }).clicked() {
            app.view.path_preview = if playing { None } else { Some(Instant::now()) };
        }
        let mut frame = app.export.anim3d_scrub_frame.min(last_frame);
        let scrub = ui.add_enabled(!playing, egui::Slider::new(&mut frame, 0..=last_frame).text("frame"));
        if scrub.changed() {
            app.params3d.camera = app.camera_path().camera_at_frame(frame);
            app.view.last_camera_motion = Some(Instant::now());
        }
        app.export.anim3d_scrub_frame = frame;
    });
    ui.label(
        egui::RichText::new("Preview, scrubbing and \"Go to\" move the viewport camera.")
            .color(theme::TEXT_WEAK)
            .small(),
    );
}

/// Warns about frames where the spline between two free keyframes runs
/// through the fractal's solid.
fn blocked_warning(ui: &mut egui::Ui, app: &FractalApp) {
    if app.view.path_preview.is_some() {
        return;
    }
    let blocked = app.camera_path().blocked_frames();
    if blocked.is_empty() {
        return;
    }
    ui.label(
        egui::RichText::new(format!(
            "⚠ The path passes through the fractal at frames {}. Move or add a keyframe there.",
            frame_ranges(&blocked)
        ))
        .color(theme::ACCENT)
        .small(),
    );
}

/// `[3, 4, 5, 9, 12, 13]` → `"3–5, 9, 12–13"`.
fn frame_ranges(frames: &[u32]) -> String {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for &f in frames {
        match ranges.last_mut() {
            Some((_, end)) if *end + 1 == f => *end = f,
            _ => ranges.push((f, f)),
        }
    }
    ranges
        .iter()
        .map(|&(a, b)| if a == b { a.to_string() } else { format!("{a}–{b}") })
        .collect::<Vec<_>>()
        .join(", ")
}

fn selected_keyframe_editor(ui: &mut egui::Ui, app: &mut FractalApp) {
    let n = app.export.anim3d_keyframes.len();
    let selected = app.export.anim3d_selected_keyframe.min(n - 1);
    app.export.anim3d_selected_keyframe = selected;
    let is_first = selected == 0;
    let is_last = selected == n - 1;

    let key = app.export.anim3d_keyframes[selected];
    let title = if is_first { "Start keyframe" } else if is_last { "End keyframe" } else { "Keyframe" };
    ui.label(egui::RichText::new(format!("{title} (frame {})", key.frame)).strong());
    let p = key.camera.position;
    ui.label(egui::RichText::new(format!("at {:.5}, {:.5}, {:.5}", p[0], p[1], p[2])).monospace().small());

    ui.horizontal(|ui| {
        if ui.button("Use current camera").clicked() {
            app.export.anim3d_keyframes[selected].camera = app.params3d.camera;
        }
        if ui.button("Go to").clicked() {
            app.params3d.camera = key.camera;
            app.view.last_camera_motion = Some(Instant::now());
        }
    });

    if !is_first {
        easing_editor(ui, &mut app.export.anim3d_keyframes[selected].easing, ("camera_kf_easing", selected));
    }

    let can_remove = n > 2 && !is_first && !is_last;
    if ui.add_enabled(can_remove, egui::Button::new("Remove keyframe")).clicked() {
        app.export.anim3d_keyframes.remove(selected);
        app.export.anim3d_selected_keyframe = selected.min(app.export.anim3d_keyframes.len() - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::frame_ranges;

    #[test]
    fn frame_ranges_groups_consecutive_frames() {
        assert_eq!(frame_ranges(&[3, 4, 5, 9, 12, 13]), "3–5, 9, 12–13");
        assert_eq!(frame_ranges(&[7]), "7");
    }
}
