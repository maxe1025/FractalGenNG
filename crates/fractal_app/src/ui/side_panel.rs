use eframe::egui;
use fractal_core::hp::{required_bits, HpComplex};
use fractal_core::kinds::{FractalKind, JULIA_DEFAULT_SEED, MULTIBROT_DEFAULT_POWER};

use crate::app::FractalApp;
use crate::ui::theme;

pub(crate) fn reset_center(app: &mut FractalApp) {
    let [cx, cy] = app.params.kind.default_center();
    app.params.center = HpComplex::from_f64_pair(cx, cy, required_bits(app.params.zoom));
}

/// The fixed set of selectable kinds, paired with a representative instance
/// used only to compare/construct — `Julia`'s seed and `Multibrot`'s power
/// are overwritten with their current live value where applicable so
/// picking the same kind twice is a no-op rather than resetting a param the
/// user already tweaked.
const KIND_CHOICES: [(&str, FractalKind); 5] = [
    ("Mandelbrot", FractalKind::Mandelbrot),
    ("Julia", FractalKind::Julia { seed: JULIA_DEFAULT_SEED }),
    ("Burning Ship", FractalKind::BurningShip),
    ("Tricorn", FractalKind::Tricorn),
    ("Multibrot", FractalKind::Multibrot { power: MULTIBROT_DEFAULT_POWER }),
];

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    theme::section(ui, "Formula", |ui| {
        egui::ComboBox::from_label("Fractal type")
            .selected_text(app.params.kind.display_name())
            .show_ui(ui, |ui| {
                for (name, choice) in KIND_CHOICES {
                    let selected = choice.shader_id() == app.params.kind.shader_id();
                    // `!selected` means we're switching in from a different
                    // kind, so `choice`'s default seed/power (not whatever
                    // the previous kind happened to carry) is the right value.
                    if ui.selectable_label(selected, name).clicked() && !selected {
                        app.params.kind = choice;
                        reset_center(app);
                    }
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

        if let FractalKind::Multibrot { mut power } = app.params.kind {
            ui.horizontal(|ui| {
                ui.label("Power:");
                if ui.add(egui::DragValue::new(&mut power).range(2.0..=8.0).speed(0.05)).changed() {
                    app.params.kind = FractalKind::Multibrot { power };
                }
            });
        }

        if !app.params.kind.supports_perturbation() {
            ui.label(
                egui::RichText::new("Deep zoom not yet supported for this fractal type (max ~1e5x).")
                    .color(theme::TEXT_WEAK)
                    .small(),
            );
        }
    });

    theme::section(ui, "Iteration", |ui| {
        ui.horizontal(|ui| {
            ui.label("Max iterations:");
            ui.add(egui::DragValue::new(&mut app.params.max_iterations).range(10..=100_000).speed(10));
        });
        ui.horizontal(|ui| {
            ui.label("Bailout radius:");
            ui.add(egui::DragValue::new(&mut app.params.bailout_radius).range(2.0..=1000.0).speed(0.5));
        });
    });

    theme::section(ui, "Camera", |ui| {
        let (center_x, center_y) = app.params.center.to_f64_pair();
        ui.label(format!("Center: {center_x:.6}, {center_y:.6}"));
        ui.label(format!("Zoom: {:.3e}", app.params.zoom));
        ui.label(format!("Precision: {} bits", app.params.center.precision_bits()));
        if ui.button("Reset view").clicked() {
            app.params.zoom = 1.0;
            reset_center(app);
        }
    });
}
