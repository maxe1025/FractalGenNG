use eframe::egui;
use fractal_core::fractal3d::{
    Formula3D, Inversion, Params3D, KIFS_DEFAULT, MANDELBOX_DEFAULT, MANDELBULB_DEFAULT, PSEUDO_KLEINIAN_DEFAULT,
};

use crate::app::FractalApp;
use crate::ui::theme;
use crate::ui::viewport3d::FLY_SPEED_RANGE;

const FORMULA_CHOICES: [Formula3D; 4] = [MANDELBOX_DEFAULT, MANDELBULB_DEFAULT, KIFS_DEFAULT, PSEUDO_KLEINIAN_DEFAULT];

/// Centered on the origin, which lies inside the Mandelbulb's and the
/// Mandelbox's solid — so the inverted space is a closed cave.
const DEFAULT_INVERSION: Inversion = Inversion { center: [0.0, 0.0, 0.0], radius: 1.0 };

fn vec3_editor(ui: &mut egui::Ui, label: &str, v: &mut [f64; 3], range: std::ops::RangeInclusive<f64>) {
    ui.horizontal(|ui| {
        ui.label(label);
        for (c, axis) in v.iter_mut().zip(["x ", "y ", "z "]) {
            ui.add(egui::DragValue::new(c).range(range.clone()).speed(0.002).prefix(axis));
        }
    });
}

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
            Formula3D::Kifs { scale, offset, rotation_degrees } => {
                ui.horizontal(|ui| {
                    ui.label("Scale:");
                    ui.add(egui::DragValue::new(scale).range(1.2..=5.0).speed(0.005))
                        .on_hover_text("3 with offset 1 and no rotation is the Menger sponge.");
                });
                vec3_editor(ui, "Offset:", offset, 0.0..=3.0);
                ui.horizontal(|ui| {
                    ui.label("Rotation:");
                    ui.add(egui::DragValue::new(&mut rotation_degrees[0]).range(-180.0..=180.0).speed(0.2).prefix("x ").suffix("°"));
                    ui.add(egui::DragValue::new(&mut rotation_degrees[1]).range(-180.0..=180.0).speed(0.2).prefix("z ").suffix("°"));
                });
            }
            Formula3D::PseudoKleinian { box_size, size, thickness } => {
                vec3_editor(ui, "Box size:", box_size, 0.1..=2.0);
                ui.horizontal(|ui| {
                    ui.label("Inversion size:");
                    ui.add(egui::DragValue::new(size).range(0.1..=3.0).speed(0.002));
                });
                ui.horizontal(|ui| {
                    ui.label("Thickness:");
                    ui.add(egui::DragValue::new(thickness).range(0.0..=0.5).speed(0.0005));
                });
            }
        }
    });

    theme::section(ui, "Inside-Out", |ui| {
        let mut inverted = app.params3d.inversion.is_some();
        if ui
            .checkbox(&mut inverted, "Sphere inversion")
            .on_hover_text(
                "Turns space inside out around a center point. With the center inside the fractal's solid, \
                 the space around the fractal becomes a closed cave walled in by its surface.",
            )
            .changed()
        {
            app.params3d.set_inversion(inverted.then_some(DEFAULT_INVERSION));
        }
        if let Some(inversion) = &mut app.params3d.inversion {
            vec3_editor(ui, "Center:", &mut inversion.center, -3.0..=3.0);
            ui.horizontal(|ui| {
                ui.label("Radius:");
                ui.add(egui::DragValue::new(&mut inversion.radius).range(0.05..=5.0).speed(0.005));
            });
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
            ui.label("Tilt:");
            let mut degrees = app.params3d.camera.roll.to_degrees();
            if ui.add(egui::Slider::new(&mut degrees, -180.0..=180.0).suffix("°")).changed() {
                app.params3d.camera.roll = 0.0;
                app.params3d.camera.roll_by(degrees.to_radians());
            }
            if ui.small_button("Level").on_hover_text("Straighten the horizon (tilt 0°).").clicked() {
                app.params3d.camera.roll = 0.0;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Fly speed:");
            ui.add(egui::Slider::new(&mut app.view.fly_speed, FLY_SPEED_RANGE).logarithmic(true).prefix("×"));
        });
        if ui.button("Reset camera").clicked() {
            app.reset_view();
        }
        ui.label(
            egui::RichText::new("W/A/S/D move · Q/E down/up · Z/C tilt · drag to look · scroll: speed · Shift: fast")
                .color(theme::TEXT_WEAK)
                .small(),
        );
    });
}
