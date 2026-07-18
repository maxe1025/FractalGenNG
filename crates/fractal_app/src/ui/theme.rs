//! Blender-inspired dark theme: neutral charcoal panels, a warm amber accent
//! for selection/active state, flat low-rounding widgets. Applied once at
//! startup (`FractalApp::new`) via `egui::Context::set_visuals`.

use eframe::egui;
use egui::{Color32, CornerRadius, Margin, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(0xe0, 0x8a, 0x34);
pub const TEXT: Color32 = Color32::from_rgb(0xe4, 0xe4, 0xe6);
pub const TEXT_WEAK: Color32 = Color32::from_rgb(0x9a, 0x9a, 0xa2);

const BG_VIEWPORT: Color32 = Color32::from_rgb(0x14, 0x14, 0x16);
const BG_PANEL: Color32 = Color32::from_rgb(0x2b, 0x2b, 0x2e);
const BG_HEADER: Color32 = Color32::from_rgb(0x23, 0x23, 0x26);
const BG_SECTION: Color32 = Color32::from_rgb(0x32, 0x32, 0x36);
const BG_WIDGET_INACTIVE: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x3f);
const BG_WIDGET_HOVER: Color32 = Color32::from_rgb(0x47, 0x47, 0x4e);
const BG_WIDGET_ACTIVE: Color32 = Color32::from_rgb(0x54, 0x54, 0x5c);

/// Header/status bar background — one shade darker than the properties panel,
/// matching Blender's chrome-vs-content contrast.
pub fn header_bg() -> Color32 {
    BG_HEADER
}

pub fn viewport_bg() -> Color32 {
    BG_VIEWPORT
}

pub fn section_bg() -> Color32 {
    BG_SECTION
}

/// Builds the full dark visuals. Call once via `ctx.set_visuals(theme::visuals())`.
pub fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();

    v.override_text_color = Some(TEXT);
    v.hyperlink_color = ACCENT;
    v.panel_fill = BG_PANEL;
    v.window_fill = BG_PANEL;
    v.extreme_bg_color = BG_VIEWPORT;
    v.faint_bg_color = BG_SECTION;
    v.code_bg_color = BG_VIEWPORT;

    v.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, ACCENT);

    let corner = CornerRadius::same(3);

    v.widgets.noninteractive.bg_fill = BG_PANEL;
    v.widgets.noninteractive.weak_bg_fill = BG_PANEL;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_gray(0x18));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_WEAK);
    v.widgets.noninteractive.corner_radius = corner;

    v.widgets.inactive.bg_fill = BG_WIDGET_INACTIVE;
    v.widgets.inactive.weak_bg_fill = BG_WIDGET_INACTIVE;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.inactive.corner_radius = corner;

    v.widgets.hovered.bg_fill = BG_WIDGET_HOVER;
    v.widgets.hovered.weak_bg_fill = BG_WIDGET_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.hovered.corner_radius = corner;

    v.widgets.active.bg_fill = ACCENT;
    v.widgets.active.weak_bg_fill = ACCENT;
    v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    v.widgets.active.fg_stroke = Stroke::new(1.0, Color32::from_gray(0x12));
    v.widgets.active.corner_radius = corner;

    v.widgets.open.bg_fill = BG_WIDGET_ACTIVE;
    v.widgets.open.weak_bg_fill = BG_WIDGET_ACTIVE;
    v.widgets.open.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    v.widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.open.corner_radius = corner;

    v.window_corner_radius = CornerRadius::same(4);
    v.menu_corner_radius = CornerRadius::same(4);

    v
}

/// Tightens default spacing slightly for a denser, more tool-like feel.
pub fn apply_spacing(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.indent = 14.0;
    });
}

/// A Blender-style panel section: a small bold accent-colored title over a
/// slightly-raised background card, with the body indented below a thin
/// separator. Used to break up each properties tab into labeled groups
/// instead of one long unstructured scroll of widgets.
pub fn section(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(BG_SECTION)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(title).strong().color(ACCENT).size(13.0));
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);
            add_contents(ui);
        });
    ui.add_space(8.0);
}
