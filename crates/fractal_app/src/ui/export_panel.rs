use std::path::{Path, PathBuf};

use eframe::egui;
use fractal_core::export::image_export::{self, ImageError};
use fractal_core::fractal3d::Params3D;
use fractal_core::math_overlay::{self, MathOverlay};
use fractal_core::FractalParams;

use crate::app::FractalApp;
use crate::ui::{math_overlay as math_overlay_ui, theme, timeline};

/// One frame of an animation job: a 2D or a 3D render.
#[derive(Clone)]
pub enum FrameParams {
    Plane(FractalParams),
    Volume(Params3D),
}

impl FrameParams {
    fn overlay_lines(&self, overlay: &MathOverlay) -> Vec<String> {
        match self {
            FrameParams::Plane(params) => math_overlay::describe(params, overlay.detail),
            FrameParams::Volume(params) => math_overlay::describe_3d(params, overlay.detail),
        }
    }
}

pub struct AnimationJob {
    pub frames: Vec<FrameParams>,
    pub next_index: usize,
    pub output_dir: PathBuf,
    pub width: u32,
    pub height: u32,
    pub supersample: u32,
    /// Snapshot of the timeline's fps at render-start time, so a fps edit
    /// mid-render doesn't retroactively change what a completed job's
    /// "Stitch to video" step uses.
    pub fps: u32,
    /// Snapshot of `FractalApp::overlay` at render-start time, for the same
    /// reason as `fps`: every frame of one animation gets the same overlay.
    pub overlay: MathOverlay,
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
            anim_job: None,
            last_animation: None,
            ffmpeg_status: None,
        }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    theme::section(ui, "Math Overlay", |ui| {
        math_overlay_ui::show_settings(ui, app);
    });

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
            let (device, queue) = (&app.render_state.device, &app.render_state.queue);
            let rgba = if app.is_3d() {
                app.renderer.render_3d_to_rgba(device, queue, &app.params3d, render_w, render_h)
            } else {
                app.renderer.render_to_rgba(device, queue, &app.params, render_w, render_h)
            };
            let result = save_export_frame(
                rgba,
                (render_w, render_h),
                (app.export.width, app.export.height),
                &app.overlay_lines(),
                &app.overlay,
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

/// Renders the timeline's frame range (keyframes and range are edited in
/// the timeline below the viewport).
fn show_animation_section(ui: &mut egui::Ui, app: &mut FractalApp) {
    run_animation_step(ui, app);

    if let Some(job) = &app.export.anim_job {
        let total = job.frames.len().max(1);
        let progress = job.next_index as f32 / total as f32;
        ui.add(egui::ProgressBar::new(progress).text(format!("{}/{}", job.next_index, job.frames.len())));
        if ui.button("Cancel").clicked() {
            app.export.anim_job = None;
        }
    } else {
        let t = &app.timeline;
        ui.label(format!(
            "Frames {}–{} ({} frames, {:.1} s at {} fps)",
            t.start,
            t.end,
            t.frame_count(),
            f64::from(t.frame_count()) / f64::from(t.fps),
            t.fps
        ));
        let key_count = if app.is_3d() { t.keys_3d.len() } else { t.keys_2d.len() };
        let hint = if key_count == 0 {
            "No keyframes yet: every frame would show the current view. Key views in the timeline (I)."
        } else {
            "Set the range and keyframes in the timeline below the viewport."
        };
        ui.label(egui::RichText::new(hint).color(theme::TEXT_WEAK).small());
        if app.is_3d() && !app.timeline.keys_3d.is_empty() {
            let blocked = app.camera_path().blocked_frames(app.timeline.range());
            if !blocked.is_empty() {
                ui.label(
                    egui::RichText::new(format!(
                        "⚠ The camera path passes through the fractal at frames {}.",
                        timeline::frame_ranges(&blocked)
                    ))
                    .color(theme::ACCENT)
                    .small(),
                );
            }
        }

        ui.add_space(4.0);
        if ui.button("Render Animation...").clicked()
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            let range = app.timeline.range();
            let frames = if app.is_3d() {
                app.camera_path().frames_in_range(range).into_iter().map(FrameParams::Volume).collect()
            } else {
                app.timeline_2d().frames_in_range(range).into_iter().map(FrameParams::Plane).collect()
            };
            app.export.anim_job = Some(AnimationJob {
                frames,
                next_index: 0,
                output_dir: dir,
                width: app.export.width,
                height: app.export.height,
                supersample: app.export.supersample,
                fps: app.timeline.fps,
                overlay: app.overlay,
            });
            app.export.last_animation = None;
            app.export.ffmpeg_status = None;
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
        params: FrameParams,
        output_dir: PathBuf,
        index: usize,
        total: usize,
        width: u32,
        height: u32,
        supersample: u32,
        fps: u32,
        overlay: MathOverlay,
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
            overlay: job.overlay,
        })
    });

    let Some(step) = step else { return };

    let render_w = step.width * step.supersample;
    let render_h = step.height * step.supersample;
    let (device, queue) = (&app.render_state.device, &app.render_state.queue);
    let rgba = match &step.params {
        FrameParams::Plane(params) => app.renderer.render_to_rgba(device, queue, params, render_w, render_h),
        FrameParams::Volume(params) => app.renderer.render_3d_to_rgba(device, queue, params, render_w, render_h),
    };
    let path = step.output_dir.join(format!("frame_{:05}.png", step.index));
    let result = save_export_frame(
        rgba,
        (render_w, render_h),
        (step.width, step.height),
        &step.params.overlay_lines(&step.overlay),
        &step.overlay,
        &path,
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

/// Downsamples a supersampled render to the export size, burns the math
/// overlay's `lines` into it if enabled (after downsampling, so the text
/// stays crisp at the final resolution), and saves it as a PNG.
fn save_export_frame(
    rgba: Vec<u8>,
    (render_w, render_h): (u32, u32),
    (width, height): (u32, u32),
    lines: &[String],
    overlay: &MathOverlay,
    path: &Path,
) -> Result<(), ImageError> {
    let mut rgba = image_export::downsample(render_w, render_h, rgba, width, height);
    if overlay.burn_into_exports {
        crate::overlay_burn::burn(&mut rgba, width, height, lines, overlay);
    }
    image_export::save_png(width, height, &rgba, path)
}
