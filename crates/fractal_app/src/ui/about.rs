use eframe::egui;

use crate::app::FractalApp;
use crate::ui::theme;

/// Embedded at compile time so the in-app changelog can never drift out of
/// sync with the repository's `CHANGELOG.md` — there is only one copy.
const CHANGELOG: &str = include_str!("../../../../CHANGELOG.md");

pub fn show(ui: &mut egui::Ui, _app: &mut FractalApp) {
    theme::section(ui, "FractalGen", |ui| {
        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
        ui.hyperlink_to("Source on GitHub", "https://github.com/maxe1025/FractalGenNG");
        ui.label(
            egui::RichText::new("MIT licensed — see LICENSE in the repository.")
                .color(theme::TEXT_WEAK)
                .small(),
        );
    });

    theme::section(ui, "Changelog", |ui| {
        egui::ScrollArea::vertical().max_height(400.0).id_salt("changelog_scroll").show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(CHANGELOG).monospace().small()).wrap());
        });
    });
}
