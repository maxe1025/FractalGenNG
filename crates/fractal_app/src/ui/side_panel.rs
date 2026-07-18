use eframe::egui;
use fractal_core::hp::{required_bits, HpComplex};
use fractal_core::kinds::{FractalKind, JULIA_DEFAULT_SEED};

use crate::app::FractalApp;

fn reset_center(app: &mut FractalApp) {
    let [cx, cy] = app.params.kind.default_center();
    app.params.center = HpComplex::from_f64_pair(cx, cy, required_bits(app.params.zoom));
}

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    ui.heading("FractalGen");
    ui.separator();

    let is_julia = matches!(app.params.kind, FractalKind::Julia { .. });
    egui::ComboBox::from_label("Fractal type")
        .selected_text(app.params.kind.display_name())
        .show_ui(ui, |ui| {
            if ui.selectable_label(!is_julia, "Mandelbrot").clicked() && is_julia {
                app.params.kind = FractalKind::Mandelbrot;
                reset_center(app);
            }
            if ui.selectable_label(is_julia, "Julia").clicked() && !is_julia {
                app.params.kind = FractalKind::Julia { seed: JULIA_DEFAULT_SEED };
                reset_center(app);
            }
        });

    if let FractalKind::Julia { mut seed } = app.params.kind {
        ui.horizontal(|ui| {
            ui.label("Seed:");
            let mut changed = false;
            changed |= ui.add(egui::DragValue::new(&mut seed[0]).speed(0.001).prefix("re ")).changed();
            changed |= ui.add(egui::DragValue::new(&mut seed[1]).speed(0.001).prefix("im ")).changed();
            if changed {
                app.params.kind = FractalKind::Julia { seed };
            }
        });
        ui.checkbox(&mut app.view.julia_seed_picker_active, "Pick seed on canvas (click)");
    }

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Max iterations:");
        ui.add(egui::DragValue::new(&mut app.params.max_iterations).range(10..=100_000).speed(10));
    });
    ui.horizontal(|ui| {
        ui.label("Bailout radius:");
        ui.add(egui::DragValue::new(&mut app.params.bailout_radius).range(2.0..=1000.0).speed(0.5));
    });

    ui.separator();
    let (center_x, center_y) = app.params.center.to_f64_pair();
    ui.label(format!("Center: {center_x:.6}, {center_y:.6}"));
    ui.label(format!("Zoom: {:.3e}", app.params.zoom));
    ui.label(format!("Precision: {} bits", app.params.center.precision_bits()));
    if ui.button("Reset view").clicked() {
        app.params.zoom = 1.0;
        reset_center(app);
    }
}
