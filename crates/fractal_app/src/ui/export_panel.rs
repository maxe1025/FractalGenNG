use std::path::PathBuf;

use eframe::egui;
use fractal_core::export::animation::{Easing, Keyframe, Timeline};
use fractal_core::hp::MAX_INTERACTIVE_ZOOM;
use fractal_core::kinds::KindParam;
use fractal_core::FractalParams;

use crate::app::FractalApp;
use crate::ui::theme;

pub struct AnimationJob {
    pub frames: Vec<FractalParams>,
    pub next_index: usize,
    pub output_dir: PathBuf,
    pub width: u32,
    pub height: u32,
    pub supersample: u32,
    /// Snapshot of `ExportState::anim_fps` at render-start time, so a fps
    /// edit mid-render doesn't retroactively change what a completed job's
    /// "Stitch to video" step uses.
    pub fps: u32,
}

/// An animation export that finished, kept around so the "Stitch to video"
/// controls know which frame directory/frame-rate to hand ffmpeg.
pub struct CompletedAnimation {
    pub output_dir: PathBuf,
    pub fps: u32,
}

pub struct ExportState {
    pub width: u32,
    pub height: u32,
    pub supersample: u32,
    pub status: Option<String>,
    /// Sorted ascending by `frame`. Always has at least 2 entries — the
    /// first pinned at frame 0, the last at `anim_frame_count - 1` (kept in
    /// sync by `apply_frame_count`). Middle keyframes (added via the
    /// timeline's "+ Add keyframe") are freely repositionable between their
    /// neighbors.
    pub anim_keyframes: Vec<Keyframe>,
    pub anim_selected_keyframe: usize,
    pub anim_frame_count: u32,
    pub anim_fps: u32,
    pub anim_job: Option<AnimationJob>,
    pub last_animation: Option<CompletedAnimation>,
    pub ffmpeg_status: Option<String>,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            supersample: 2,
            status: None,
            anim_keyframes: vec![
                Keyframe { frame: 0, zoom: 1.0, kind_param: KindParam::None, easing: Easing::Linear },
                Keyframe { frame: 119, zoom: 1000.0, kind_param: KindParam::None, easing: Easing::Linear },
            ],
            anim_selected_keyframe: 0,
            anim_frame_count: 120,
            anim_fps: 30,
            anim_job: None,
            last_animation: None,
            ffmpeg_status: None,
        }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    theme::section(ui, "Export Image", |ui| {
        ui.horizontal(|ui| {
            ui.label("Width:");
            ui.add(egui::DragValue::new(&mut app.export.width).range(1..=16384));
            ui.label("Height:");
            ui.add(egui::DragValue::new(&mut app.export.height).range(1..=16384));
        });
        ui.horizontal(|ui| {
            ui.label("Supersampling:");
            egui::ComboBox::from_id_salt("supersample")
                .selected_text(format!("{}x", app.export.supersample))
                .show_ui(ui, |ui| {
                    for factor in [1u32, 2, 4] {
                        ui.selectable_value(&mut app.export.supersample, factor, format!("{factor}x"));
                    }
                });
        });

        if ui.button("Render & Save PNG...").clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_file_name("fractal.png")
                .add_filter("PNG image", &["png"])
                .save_file()
        {
            let render_w = app.export.width * app.export.supersample;
            let render_h = app.export.height * app.export.supersample;
            let rgba = app.renderer.render_to_rgba(
                &app.render_state.device,
                &app.render_state.queue,
                &app.params,
                render_w,
                render_h,
            );
            let result = fractal_core::export::image_export::save_png_supersampled(
                render_w,
                render_h,
                &rgba,
                app.export.width,
                app.export.height,
                &path,
            );
            app.export.status = Some(match result {
                Ok(()) => format!("Saved {}", path.display()),
                Err(e) => format!("Export failed: {e}"),
            });
        }

        if let Some(status) = &app.export.status {
            ui.label(status);
        }
    });

    theme::section(ui, "Export Animation", |ui| {
        show_animation_section(ui, app);
    });
}

/// Keeps every keyframe's kind-param in sync with whatever the current
/// fractal kind actually animates. Without this, switching fractal kind
/// (e.g. Julia -> Mandelbrot -> Multibrot) via the Fractal tab would leave
/// stale `KindParam::Seed`s sitting in keyframes for a kind that doesn't
/// have a seed, silently doing nothing when the timeline is built.
fn sync_kind_param_defaults(app: &mut FractalApp) {
    let current = app.params.kind.animatable_param();
    let same_variant =
        |a: KindParam, b: KindParam| std::mem::discriminant(&a) == std::mem::discriminant(&b);
    for kf in &mut app.export.anim_keyframes {
        if !same_variant(kf.kind_param, current) {
            kf.kind_param = current;
        }
    }
}

/// Applies a `anim_frame_count` change: moves the last keyframe to the new
/// final frame, and clamps any middle keyframes that would otherwise end up
/// past it (the first keyframe always stays pinned at frame 0).
fn apply_frame_count(app: &mut FractalApp) {
    let last_frame = app.export.anim_frame_count.saturating_sub(1);
    let n = app.export.anim_keyframes.len();
    if n == 0 {
        return;
    }
    if let Some(last) = app.export.anim_keyframes.last_mut() {
        last.frame = last_frame;
    }
    for kf in app.export.anim_keyframes[..n - 1].iter_mut().skip(1) {
        kf.frame = kf.frame.min(last_frame.saturating_sub(1));
    }
}

fn show_animation_section(ui: &mut egui::Ui, app: &mut FractalApp) {
    run_animation_step(ui, app);

    if app.export.anim_job.is_none() {
        sync_kind_param_defaults(app);

        ui.horizontal(|ui| {
            ui.label("Frame count:");
            let frame_count_response =
                ui.add(egui::DragValue::new(&mut app.export.anim_frame_count).range(2..=10_000));
            // Only reclamp keyframes once editing actually settles (drag
            // released, or the text field loses focus), not on every
            // `.changed()` — DragValue's text-entry mode fires `.changed()`
            // per keystroke, so typing "12" transiently passes through "1"
            // (frame_count=1); reclamping there would permanently collapse
            // any middle keyframe to frame 0 via the one-way `.min()` below,
            // with no way to recover once the field settles back at "12".
            if frame_count_response.drag_stopped() || frame_count_response.lost_focus() {
                apply_frame_count(app);
            }
            ui.label("FPS:");
            ui.add(egui::DragValue::new(&mut app.export.anim_fps).range(1..=240));
        });
        ui.label(
            egui::RichText::new(format!(
                "~{:.1}s video at {} fps",
                f64::from(app.export.anim_frame_count) / f64::from(app.export.anim_fps),
                app.export.anim_fps
            ))
            .color(theme::TEXT_WEAK)
            .small(),
        );

        ui.add_space(6.0);
        timeline_strip(ui, app);
        if ui.small_button("+ Add keyframe").clicked() {
            add_keyframe(app);
        }

        ui.add_space(6.0);
        ui.separator();
        show_selected_keyframe_editor(ui, app);

        ui.add_space(4.0);
        if ui.button("Render Animation...").clicked()
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            let timeline = Timeline { base: app.params.clone(), keyframes: app.export.anim_keyframes.clone() };
            app.export.anim_job = Some(AnimationJob {
                frames: timeline.generate_frames(),
                next_index: 0,
                output_dir: dir,
                width: app.export.width,
                height: app.export.height,
                supersample: app.export.supersample,
                fps: app.export.anim_fps,
            });
            app.export.last_animation = None;
            app.export.ffmpeg_status = None;
        }
    } else if let Some(job) = &app.export.anim_job {
        let total = job.frames.len().max(1);
        let progress = job.next_index as f32 / total as f32;
        ui.add(egui::ProgressBar::new(progress).text(format!("{}/{}", job.next_index, job.frames.len())));
        if ui.button("Cancel").clicked() {
            app.export.anim_job = None;
        }
    }

    if let Some(status) = &app.export.status {
        ui.label(status);
    }

    if app.export.anim_job.is_none() {
        show_ffmpeg_stitch_controls(ui, app);
    }
}

/// A horizontal strip spanning frame 0 to the last keyframe's frame, with a
/// draggable marker per keyframe (click to select; drag to reposition — the
/// first/last markers are pinned, since they define the animation's start
/// and total length rather than a point along it).
fn timeline_strip(ui: &mut egui::Ui, app: &mut FractalApp) {
    let bar_height = 28.0;
    let desired_size = egui::vec2(ui.available_width(), bar_height);
    let (rect, _response) = ui.allocate_exact_size(desired_size, egui::Sense::hover());

    ui.painter().rect_filled(rect, 4.0, theme::section_bg());
    ui.painter().line_segment(
        [egui::pos2(rect.left(), rect.center().y), egui::pos2(rect.right(), rect.center().y)],
        egui::Stroke::new(1.5, theme::TEXT_WEAK),
    );

    let n = app.export.anim_keyframes.len();
    let last_frame = app.export.anim_keyframes.last().map(|k| k.frame).unwrap_or(1).max(1);
    let marker_radius = 7.0;

    for i in 0..n {
        let frame = app.export.anim_keyframes[i].frame;
        let x = rect.left() + (frame as f32 / last_frame as f32) * rect.width();
        let marker_center = egui::pos2(x, rect.center().y);
        let marker_rect =
            egui::Rect::from_center_size(marker_center, egui::vec2(marker_radius * 2.0, marker_radius * 2.0));
        let marker_id = ui.id().with("timeline_kf").with(i);
        let draggable = i != 0 && i != n - 1;
        let sense = if draggable { egui::Sense::click_and_drag() } else { egui::Sense::click() };
        let marker_response = ui.interact(marker_rect, marker_id, sense);

        let is_selected = app.export.anim_selected_keyframe == i;
        ui.painter().circle(
            marker_center,
            marker_radius,
            if is_selected { theme::ACCENT } else { theme::TEXT_WEAK },
            egui::Stroke::new(2.0, if is_selected { egui::Color32::WHITE } else { egui::Color32::from_gray(60) }),
        );

        if marker_response.clicked() {
            app.export.anim_selected_keyframe = i;
        }
        if draggable && marker_response.dragged() && rect.width() > 0.0 {
            let delta_frames = (marker_response.drag_delta().x / rect.width()) * last_frame as f32;
            let lower = app.export.anim_keyframes[i - 1].frame + 1;
            let upper = app.export.anim_keyframes[i + 1].frame.saturating_sub(1).max(lower);
            let new_frame = (app.export.anim_keyframes[i].frame as f32 + delta_frames)
                .round()
                .clamp(lower as f32, upper as f32) as u32;
            app.export.anim_keyframes[i].frame = new_frame;
            app.export.anim_selected_keyframe = i;
        }
    }
}

/// Inserts a new keyframe into the gap after the selected one (or before the
/// last, if the selected keyframe already is the last), seeding its zoom
/// and kind-param from the existing curve at that frame so it doesn't
/// create a visible jump. No-op if there's no room left in that gap.
fn add_keyframe(app: &mut FractalApp) {
    let n = app.export.anim_keyframes.len();
    let insert_after = app.export.anim_selected_keyframe.min(n.saturating_sub(2));
    let a = app.export.anim_keyframes[insert_after].frame;
    let b = app.export.anim_keyframes[insert_after + 1].frame;
    if b <= a + 1 {
        return;
    }
    let mid_frame = a + (b - a) / 2;

    let timeline = Timeline { base: app.params.clone(), keyframes: app.export.anim_keyframes.clone() };
    let (zoom, kind_param) = timeline.value_at_frame(mid_frame);

    app.export.anim_keyframes.insert(insert_after + 1, Keyframe { frame: mid_frame, zoom, kind_param, easing: Easing::Linear });
    app.export.anim_selected_keyframe = insert_after + 1;
}

fn remove_selected_keyframe(app: &mut FractalApp) {
    let n = app.export.anim_keyframes.len();
    let selected = app.export.anim_selected_keyframe;
    if n <= 2 || selected == 0 {
        return;
    }
    app.export.anim_keyframes.remove(selected);
    app.export.anim_selected_keyframe = selected.min(app.export.anim_keyframes.len() - 1);
}

fn show_selected_keyframe_editor(ui: &mut egui::Ui, app: &mut FractalApp) {
    let n = app.export.anim_keyframes.len();
    let selected = app.export.anim_selected_keyframe.min(n - 1);
    app.export.anim_selected_keyframe = selected;
    let is_first = selected == 0;
    let is_last = selected == n - 1;

    ui.label(
        egui::RichText::new(if is_first { "Start keyframe" } else if is_last { "End keyframe" } else { "Keyframe" })
            .strong(),
    );

    ui.horizontal(|ui| {
        ui.label("Zoom:");
        let mut zoom = app.export.anim_keyframes[selected].zoom;
        if ui.add(egui::DragValue::new(&mut zoom).range(0.01..=MAX_INTERACTIVE_ZOOM).speed(1.0)).changed() {
            app.export.anim_keyframes[selected].zoom = zoom;
        }
        if ui.small_button("Use current").clicked() {
            app.export.anim_keyframes[selected].zoom = app.params.zoom;
        }
    });

    match app.params.kind.animatable_param() {
        KindParam::None => {}
        KindParam::Seed(current_seed) => {
            if let KindParam::Seed(mut seed) = app.export.anim_keyframes[selected].kind_param {
                ui.horizontal(|ui| {
                    ui.label("Seed:");
                    let mut changed = false;
                    changed |= ui.add(egui::DragValue::new(&mut seed[0]).speed(0.001).prefix("re ")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut seed[1]).speed(0.001).prefix("im ")).changed();
                    if changed {
                        app.export.anim_keyframes[selected].kind_param = KindParam::Seed(seed);
                    }
                    if ui.small_button("Use current").clicked() {
                        app.export.anim_keyframes[selected].kind_param = KindParam::Seed(current_seed);
                    }
                });
            }
        }
        KindParam::Power(current_power) => {
            if let KindParam::Power(mut power) = app.export.anim_keyframes[selected].kind_param {
                ui.horizontal(|ui| {
                    ui.label("Power:");
                    if ui.add(egui::DragValue::new(&mut power).range(2.0..=8.0).speed(0.05)).changed() {
                        app.export.anim_keyframes[selected].kind_param = KindParam::Power(power);
                    }
                    if ui.small_button("Use current").clicked() {
                        app.export.anim_keyframes[selected].kind_param = KindParam::Power(current_power);
                    }
                });
            }
        }
    }

    if !is_first {
        // Easing governs the segment leading INTO this keyframe from the
        // previous one — meaningless on the first keyframe, since no
        // segment precedes it.
        let easing = app.export.anim_keyframes[selected].easing;
        let easing_name = match easing {
            Easing::Linear => "Linear",
            Easing::Accelerating { .. } => "Accelerating",
        };
        ui.horizontal(|ui| {
            ui.label("Easing in:");
            egui::ComboBox::from_id_salt(("kf_easing", selected)).selected_text(easing_name).show_ui(ui, |ui| {
                if ui.selectable_label(matches!(easing, Easing::Linear), "Linear").clicked() {
                    app.export.anim_keyframes[selected].easing = Easing::Linear;
                }
                if ui.selectable_label(matches!(easing, Easing::Accelerating { .. }), "Accelerating").clicked() {
                    app.export.anim_keyframes[selected].easing = Easing::Accelerating { exponent: 2.0 };
                }
            });
        });
        if let Easing::Accelerating { mut exponent } = app.export.anim_keyframes[selected].easing {
            ui.horizontal(|ui| {
                ui.label("Acceleration:");
                if ui.add(egui::DragValue::new(&mut exponent).range(0.2..=5.0).speed(0.05)).changed() {
                    app.export.anim_keyframes[selected].easing = Easing::Accelerating { exponent };
                }
            });
        }
    }

    let can_remove = n > 2 && !is_first;
    if ui.add_enabled(can_remove, egui::Button::new("Remove keyframe")).clicked() {
        remove_selected_keyframe(app);
    }
}

fn show_ffmpeg_stitch_controls(ui: &mut egui::Ui, app: &mut FractalApp) {
    let Some(completed) = &app.export.last_animation else { return };
    let output_dir = completed.output_dir.clone();
    let fps = completed.fps;

    ui.add_space(4.0);
    ui.separator();
    ui.label(egui::RichText::new("Stitch frames to video").strong());

    if ui.button("Stitch to video (ffmpeg)...").clicked()
        && let Some(out_path) =
            rfd::FileDialog::new().set_file_name("animation.mp4").add_filter("MP4 video", &["mp4"]).save_file()
    {
        app.export.ffmpeg_status = Some(if crate::ffmpeg::is_available() {
            match crate::ffmpeg::spawn_stitch(&output_dir, fps, &out_path) {
                Ok(()) => format!("ffmpeg started in the background — check {} shortly.", out_path.display()),
                Err(e) => format!("Failed to start ffmpeg: {e}"),
            }
        } else {
            "ffmpeg not found on PATH. Copy the command below and run it yourself once ffmpeg is installed."
                .to_string()
        });
    }

    let default_out = output_dir.join("animation.mp4");
    let cmd = crate::ffmpeg::command_string(&output_dir, fps, &default_out);
    ui.horizontal(|ui| {
        if ui.small_button("Copy command").clicked() {
            ui.copy_text(cmd.clone());
        }
        ui.label(egui::RichText::new("(edit the output path before running)").color(theme::TEXT_WEAK).small());
    });
    ui.label(egui::RichText::new(&cmd).monospace().small().color(theme::TEXT_WEAK));

    if let Some(status) = &app.export.ffmpeg_status {
        ui.label(status);
    }
}

/// Renders and saves exactly one animation frame per UI tick, so a long export
/// doesn't block the interactive viewport. Requests a repaint while a job is
/// active so the export keeps progressing without needing user input.
fn run_animation_step(ui: &mut egui::Ui, app: &mut FractalApp) {
    struct Step {
        params: FractalParams,
        output_dir: PathBuf,
        index: usize,
        total: usize,
        width: u32,
        height: u32,
        supersample: u32,
        fps: u32,
    }

    let step = app.export.anim_job.as_ref().and_then(|job| {
        (job.next_index < job.frames.len()).then(|| Step {
            params: job.frames[job.next_index].clone(),
            output_dir: job.output_dir.clone(),
            index: job.next_index,
            total: job.frames.len(),
            width: job.width,
            height: job.height,
            supersample: job.supersample,
            fps: job.fps,
        })
    });

    let Some(step) = step else { return };

    let render_w = step.width * step.supersample;
    let render_h = step.height * step.supersample;
    let rgba = app.renderer.render_to_rgba(
        &app.render_state.device,
        &app.render_state.queue,
        &step.params,
        render_w,
        render_h,
    );
    let path = step.output_dir.join(format!("frame_{:05}.png", step.index));
    let result = fractal_core::export::image_export::save_png_supersampled(
        render_w, render_h, &rgba, step.width, step.height, &path,
    );

    if let Err(e) = result {
        app.export.status = Some(format!("Animation export failed at frame {}: {e}", step.index));
        app.export.anim_job = None;
        return;
    }

    if step.index + 1 >= step.total {
        app.export.status = Some(format!("Animation saved to {}", step.output_dir.display()));
        app.export.last_animation = Some(CompletedAnimation { output_dir: step.output_dir, fps: step.fps });
        app.export.anim_job = None;
    } else {
        if let Some(job) = app.export.anim_job.as_mut() {
            job.next_index += 1;
        }
        ui.ctx().request_repaint();
    }
}
