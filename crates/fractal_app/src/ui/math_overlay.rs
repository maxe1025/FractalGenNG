//! Math overlay: its settings section (Export tab) and the viewport preview.
//! Export frames get the same overlay burned in by `crate::overlay_burn`.

use eframe::egui;
use fractal_core::math_overlay::{describe, MathDetail, MathOverlay, OverlayCorner};
use fractal_core::FractalParams;

use crate::app::FractalApp;
use crate::overlay_burn::{BOX_ALPHA, TEXT_GRAY};

pub fn show_settings(ui: &mut egui::Ui, app: &mut FractalApp) {
    let overlay = &mut app.overlay;
    ui.checkbox(&mut overlay.show_in_viewport, "Show in viewport");
    ui.checkbox(&mut overlay.burn_into_exports, "Burn into exported images & animations");

    ui.horizontal(|ui| {
        ui.label("Detail:");
        egui::ComboBox::from_id_salt("math_overlay_detail").selected_text(overlay.detail.label()).show_ui(ui, |ui| {
            for detail in MathDetail::ALL {
                ui.selectable_value(&mut overlay.detail, detail, detail.label());
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label("Corner:");
        egui::ComboBox::from_id_salt("math_overlay_corner").selected_text(overlay.corner.label()).show_ui(ui, |ui| {
            for corner in OverlayCorner::ALL {
                ui.selectable_value(&mut overlay.corner, corner, corner.label());
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label("Text size:");
        let mut percent = overlay.text_scale * 100.0;
        if ui.add(egui::Slider::new(&mut percent, 1.0..=5.0).step_by(0.1).suffix("% of height")).changed() {
            overlay.text_scale = percent / 100.0;
        }
    });
}

/// Draws the overlay for `params` over the viewport image at `rect`.
pub fn paint(ui: &egui::Ui, rect: egui::Rect, params: &FractalParams, overlay: &MathOverlay) {
    let height = rect.height();
    let text_color = egui::Color32::from_gray(TEXT_GRAY);
    let painter = ui.painter_at(rect);
    let galley = painter.layout_no_wrap(
        describe(params, overlay.detail).join("\n"),
        egui::FontId::monospace(overlay.text_px(height)),
        text_color,
    );
    let padding = overlay.padding_px(height);
    let box_size = galley.size() + egui::vec2(2.0 * padding, 2.0 * padding);
    let (x, y) = overlay.box_origin(rect.width(), height, box_size.x, box_size.y);
    let box_rect = egui::Rect::from_min_size(rect.min + egui::vec2(x, y), box_size);
    painter.rect_filled(box_rect, 0.0, egui::Color32::from_black_alpha(BOX_ALPHA));
    painter.galley(box_rect.min + egui::vec2(padding, padding), galley, text_color);
}
