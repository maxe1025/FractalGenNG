use std::path::PathBuf;

use eframe::egui;
use fractal_core::export::animation::{self, ZoomCurve};
use fractal_core::hp::MAX_INTERACTIVE_ZOOM;
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
    pub anim_start_zoom: f64,
    pub anim_end_zoom: f64,
    pub anim_frame_count: u32,
    pub anim_fps: u32,
    pub anim_curve: ZoomCurve,
    /// `exponent` for `ZoomCurve::Accelerating`, kept as its own field (not
    /// just read out of `anim_curve`) so the value survives switching to
    /// Linear/Exponential and back without resetting to the default.
    pub anim_accel_exponent: f64,
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
            anim_start_zoom: 1.0,
            anim_end_zoom: 1000.0,
            anim_frame_count: 120,
            anim_fps: 30,
            anim_curve: ZoomCurve::Exponential,
            anim_accel_exponent: 2.0,
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

fn show_animation_section(ui: &mut egui::Ui, app: &mut FractalApp) {
    run_animation_step(ui, app);

    if app.export.anim_job.is_none() {
        ui.horizontal(|ui| {
            ui.label("Start zoom:");
            ui.add(egui::DragValue::new(&mut app.export.anim_start_zoom).range(0.01..=MAX_INTERACTIVE_ZOOM).speed(1.0));
            if ui.small_button("Use current").clicked() {
                app.export.anim_start_zoom = app.params.zoom;
            }
        });
        ui.horizontal(|ui| {
            ui.label("End zoom:");
            ui.add(egui::DragValue::new(&mut app.export.anim_end_zoom).range(0.01..=MAX_INTERACTIVE_ZOOM).speed(1.0));
            if ui.small_button("Use current").clicked() {
                app.export.anim_end_zoom = app.params.zoom;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Frame count:");
            ui.add(egui::DragValue::new(&mut app.export.anim_frame_count).range(2..=10_000));
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

        let curve_name = match app.export.anim_curve {
            ZoomCurve::Linear => "Linear",
            ZoomCurve::Exponential => "Exponential",
            ZoomCurve::Accelerating { .. } => "Accelerating",
        };
        egui::ComboBox::from_id_salt("zoom_curve").selected_text(curve_name).show_ui(ui, |ui| {
            ui.selectable_value(&mut app.export.anim_curve, ZoomCurve::Linear, "Linear");
            ui.selectable_value(&mut app.export.anim_curve, ZoomCurve::Exponential, "Exponential");
            if ui
                .selectable_label(matches!(app.export.anim_curve, ZoomCurve::Accelerating { .. }), "Accelerating (recommended)")
                .clicked()
            {
                app.export.anim_curve = ZoomCurve::Accelerating { exponent: app.export.anim_accel_exponent };
            }
        });
        if let ZoomCurve::Accelerating { mut exponent } = app.export.anim_curve {
            ui.horizontal(|ui| {
                ui.label("Acceleration:");
                if ui.add(egui::DragValue::new(&mut exponent).range(0.2..=5.0).speed(0.05)).changed() {
                    app.export.anim_accel_exponent = exponent;
                    app.export.anim_curve = ZoomCurve::Accelerating { exponent };
                }
            });
            ui.label(
                egui::RichText::new("> 1 starts slow and speeds up; < 1 starts fast and eases in.")
                    .color(theme::TEXT_WEAK)
                    .small(),
            );
        }

        if ui.button("Render Animation...").clicked()
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            let frames = animation::generate_zoom_frames(
                &app.params,
                app.export.anim_start_zoom,
                app.export.anim_end_zoom,
                app.export.anim_frame_count,
                app.export.anim_curve,
            );
            app.export.anim_job = Some(AnimationJob {
                frames,
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
