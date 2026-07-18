use eframe::egui;
use fractal_core::GradientStop;

use crate::app::FractalApp;
use crate::ui::theme;

#[derive(Default)]
pub struct PaletteEditorState {
    pub selected_stop: usize,
}

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    ui.horizontal_wrapped(|ui| {
        for (name, ctor) in fractal_core::Palette::presets() {
            if ui.button(name).clicked() {
                app.params.palette = ctor();
                app.palette_ui.selected_stop = 0;
            }
        }
    });
    ui.add_space(6.0);

    theme::section(ui, "Gradient", |ui| {
        gradient_editor(ui, app);
    });

    theme::section(ui, "Coloring", |ui| {
        ui.horizontal(|ui| {
            ui.label("Cycle count:");
            ui.add(egui::DragValue::new(&mut app.params.palette.cycle_count).range(0.1..=50.0).speed(0.1));
        });

        ui.horizontal(|ui| {
            ui.label("Interior color:");
            let interior = app.params.palette.interior_color;
            let mut color = egui::Color32::from_rgba_unmultiplied(interior[0], interior[1], interior[2], interior[3]);
            if egui::color_picker::color_edit_button_srgba(ui, &mut color, egui::color_picker::Alpha::Opaque).changed() {
                app.params.palette.interior_color = color.to_array();
            }
        });

        ui.checkbox(&mut app.params.palette.distance_shading, "Distance shading");
        if app.params.palette.distance_shading && !app.params.kind.supports_distance_shading() {
            ui.label(
                egui::RichText::new(
                    "Distance shading only affects Mandelbrot/Julia — no effect on this fractal type yet.",
                )
                .color(theme::TEXT_WEAK)
                .small(),
            );
        }
    });
}

fn gradient_editor(ui: &mut egui::Ui, app: &mut FractalApp) {
    let bar_height = 28.0;
    let desired_size = egui::vec2(ui.available_width(), bar_height);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());

    let steps = 64;
    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let t1 = (i + 1) as f32 / steps as f32;
        let color = app.params.palette.sample((t0 + t1) * 0.5);
        let x0 = rect.left() + t0 * rect.width();
        let x1 = rect.left() + t1 * rect.width();
        ui.painter().rect_filled(
            egui::Rect::from_min_max(egui::pos2(x0, rect.top()), egui::pos2(x1, rect.bottom())),
            0.0,
            egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]),
        );
    }

    if response.double_clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let t = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let color = app.params.palette.sample(t);
        app.params.palette.stops.push(GradientStop { position: t, color });
        app.params.palette.stops.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap());
        app.palette_ui.selected_stop =
            app.params.palette.stops.iter().position(|s| (s.position - t).abs() < 1e-6).unwrap_or(0);
    }

    let marker_y = rect.bottom() + 10.0;
    let marker_radius = 6.0;
    let stop_count = app.params.palette.stops.len();
    for i in 0..stop_count {
        let stop = app.params.palette.stops[i];
        let x = rect.left() + stop.position * rect.width();
        let marker_rect =
            egui::Rect::from_center_size(egui::pos2(x, marker_y), egui::vec2(marker_radius * 2.0, marker_radius * 2.0));
        let marker_id = ui.id().with("palette_stop").with(i);
        let marker_response = ui.interact(marker_rect, marker_id, egui::Sense::click_and_drag());
        let is_selected = app.palette_ui.selected_stop == i;
        let stroke_color = if is_selected { egui::Color32::WHITE } else { egui::Color32::GRAY };
        ui.painter().circle(
            marker_rect.center(),
            marker_radius,
            egui::Color32::from_rgba_unmultiplied(stop.color[0], stop.color[1], stop.color[2], 255),
            egui::Stroke::new(2.0, stroke_color),
        );
        if marker_response.clicked() {
            app.palette_ui.selected_stop = i;
        }
        if marker_response.dragged() {
            let delta = marker_response.drag_delta().x / rect.width();
            let lower = if i == 0 { 0.0 } else { app.params.palette.stops[i - 1].position + 0.005 };
            let upper =
                if i + 1 == stop_count { 1.0 } else { app.params.palette.stops[i + 1].position - 0.005 };
            let (lower, upper) = (lower.min(upper), upper.max(lower));
            app.params.palette.stops[i].position =
                (app.params.palette.stops[i].position + delta).clamp(lower, upper);
            app.palette_ui.selected_stop = i;
        }
    }

    ui.add_space(marker_radius + 6.0);

    ui.horizontal(|ui| {
        let can_remove = stop_count > 2;
        if ui.add_enabled(can_remove, egui::Button::new("Remove selected stop")).clicked() {
            app.params.palette.stops.remove(app.palette_ui.selected_stop);
            app.palette_ui.selected_stop = app.palette_ui.selected_stop.min(app.params.palette.stops.len() - 1);
        }
    });

    if let Some(stop) = app.params.palette.stops.get_mut(app.palette_ui.selected_stop) {
        ui.horizontal(|ui| {
            ui.label("Stop color:");
            let mut color = egui::Color32::from_rgba_unmultiplied(stop.color[0], stop.color[1], stop.color[2], stop.color[3]);
            if egui::color_picker::color_edit_button_srgba(ui, &mut color, egui::color_picker::Alpha::Opaque).changed() {
                stop.color = color.to_array();
            }
        });
    }
}
