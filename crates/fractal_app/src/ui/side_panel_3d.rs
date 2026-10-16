use eframe::egui;
use fractal_core::fractal3d::{Formula3D, Params3D, MANDELBOX_DEFAULT, MANDELBULB_DEFAULT};

use crate::app::FractalApp;
use crate::ui::theme;
use crate::ui::viewport3d::FLY_SPEED_RANGE;

const FORMULA_CHOICES: [Formula3D; 2] = [MANDELBOX_DEFAULT, MANDELBULB_DEFAULT];

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    theme::section(ui, "Formula", |ui| {
        egui::ComboBox::from_label("Fractal type").selected_text(app.params3d.formula.display_name()).show_ui(
            ui,
            |ui| {
                for choice in FORMULA_CHOICES {
                    let selected = choice.shader_id() == app.params3d.formula.shader_id();
                    if ui.selectable_label(selected, choice.display_name()).clicked() && !selected {
                        // Start from the new formula's own defaults (camera,
                        // step size, ...) but keep the user's palette.
                        let palette = app.params3d.palette.clone();
                        app.params3d = Params3D { palette, ..Params3D::new(choice) };
                    }
                }
            },
        );

        match &mut app.params3d.formula {
            Formula3D::Mandelbulb { power } => {
                ui.horizontal(|ui| {
                    ui.label("Power:");
                    ui.add(egui::DragValue::new(power).range(2.0..=16.0).speed(0.02));
                });
            }
            Formula3D::Mandelbox { scale, min_radius, fixed_radius } => {
                ui.horizontal(|ui| {
                    ui.label("Scale:");
                    ui.add(egui::DragValue::new(scale).range(-3.0..=3.0).speed(0.005))
                        .on_hover_text("Negative scales (around −1.5) give cave-like interiors.");
                });
                ui.horizontal(|ui| {
                    ui.label("Min radius:");
                    ui.add(egui::DragValue::new(min_radius).range(0.01..=2.0).speed(0.005));
                });
                ui.horizontal(|ui| {
                    ui.label("Fixed radius:");
                    ui.add(egui::DragValue::new(fixed_radius).range(0.1..=3.0).speed(0.005));
                });
            }
        }
    });

    theme::section(ui, "Ray Marching", |ui| {
        let params = &mut app.params3d;
        ui.horizontal(|ui| {
            ui.label("Iterations:");
            ui.add(egui::DragValue::new(&mut params.max_iterations).range(2..=60));
        });
        ui.horizontal(|ui| {
            ui.label("Max steps:");
            ui.add(egui::DragValue::new(&mut params.max_steps).range(16..=4000).speed(4));
        });
        ui.horizontal(|ui| {
            ui.label("Detail:");
            ui.add(egui::DragValue::new(&mut params.detail).range(0.1..=10.0).speed(0.02).suffix(" px"))
                .on_hover_text("A ray stops this many pixels from the surface. Lower = finer, slower.");
        });
        ui.horizontal(|ui| {
            ui.label("Step size:");
            ui.add(egui::DragValue::new(&mut params.step_factor).range(0.2..=1.0).speed(0.005))
                .on_hover_text("Fraction of the distance estimate each ray step advances. Lower it if you see holes.");
        });
        ui.horizontal(|ui| {
            ui.label("Fog distance:");
            ui.add(egui::DragValue::new(&mut params.fog_distance).range(0.0..=1000.0).speed(1.0).suffix("× clearance"))
                .on_hover_text("0 turns fog off.");
        });
    });

    theme::section(ui, "Camera", |ui| {
        let p = app.params3d.camera.position;
        ui.label(format!("Position: {:.5}, {:.5}, {:.5}", p[0], p[1], p[2]));
        ui.label(format!("Clearance: {:.3e}", app.params3d.camera_clearance()));
        ui.horizontal(|ui| {
            ui.label("Field of view:");
            ui.add(egui::Slider::new(&mut app.params3d.camera.fov_y_degrees, 20.0..=120.0).suffix("°"));
        });
        ui.horizontal(|ui| {
            ui.label("Fly speed:");
            ui.add(egui::Slider::new(&mut app.view.fly_speed, FLY_SPEED_RANGE).logarithmic(true).prefix("×"));
        });
        if ui.button("Reset camera").clicked() {
            app.reset_view();
        }
        ui.label(
            egui::RichText::new("W/A/S/D move · Q/E down/up · drag to look · scroll: speed · Shift: fast")
                .color(theme::TEXT_WEAK)
                .small(),
        );
    });
}
