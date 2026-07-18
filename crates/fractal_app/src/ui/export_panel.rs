use std::path::PathBuf;

use eframe::egui;
use fractal_core::export::animation::{self, ZoomCurve};
use fractal_core::hp::MAX_INTERACTIVE_ZOOM;
use fractal_core::FractalParams;

use crate::app::FractalApp;

pub struct AnimationJob {
    pub frames: Vec<FractalParams>,
    pub next_index: usize,
    pub output_dir: PathBuf,
    pub width: u32,
    pub height: u32,
    pub supersample: u32,
}

pub struct ExportState {
    pub width: u32,
    pub height: u32,
    pub supersample: u32,
    pub status: Option<String>,
    pub anim_start_zoom: f64,
    pub anim_end_zoom: f64,
    pub anim_frame_count: u32,
    pub anim_curve: ZoomCurve,
    pub anim_job: Option<AnimationJob>,
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
            anim_curve: ZoomCurve::Exponential,
            anim_job: None,
        }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    ui.heading("Export image");
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

    ui.separator();
    show_animation_section(ui, app);
}

fn show_animation_section(ui: &mut egui::Ui, app: &mut FractalApp) {
    ui.heading("Export zoom animation");

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
        });
        egui::ComboBox::from_id_salt("zoom_curve")
            .selected_text(match app.export.anim_curve {
                ZoomCurve::Linear => "Linear",
                ZoomCurve::Exponential => "Exponential",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.export.anim_curve, ZoomCurve::Linear, "Linear");
                ui.selectable_value(&mut app.export.anim_curve, ZoomCurve::Exponential, "Exponential (recommended)");
            });

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
            });
        }
    } else if let Some(job) = &app.export.anim_job {
        let total = job.frames.len().max(1);
        let progress = job.next_index as f32 / total as f32;
        ui.add(egui::ProgressBar::new(progress).text(format!("{}/{}", job.next_index, job.frames.len())));
        if ui.button("Cancel").clicked() {
            app.export.anim_job = None;
        }
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
        app.export.anim_job = None;
    } else {
        if let Some(job) = app.export.anim_job.as_mut() {
            job.next_index += 1;
        }
        ui.ctx().request_repaint();
    }
}
