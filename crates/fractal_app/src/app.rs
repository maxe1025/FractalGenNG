use eframe::egui;
use fractal_core::{Bookmark, FractalParams};

use crate::gpu::FractalRenderer;
use crate::ui::bookmarks::BookmarksState;
use crate::ui::export_panel::ExportState;
use crate::ui::palette_editor::PaletteEditorState;
use crate::ui::side_panel::reset_center;
use crate::ui::{about, bookmarks, export_panel, palette_editor, side_panel, theme, viewport};
use crate::view_state::{PropertiesTab, ViewState};

pub struct FractalApp {
    pub params: FractalParams,
    pub view: ViewState,
    pub renderer: FractalRenderer,
    pub render_state: egui_wgpu::RenderState,
    pub export: ExportState,
    pub bookmarks: Vec<Bookmark>,
    pub bookmarks_ui: BookmarksState,
    pub palette_ui: PaletteEditorState,
}

impl FractalApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        cc.egui_ctx.set_visuals(theme::visuals());
        theme::apply_spacing(&cc.egui_ctx);

        let render_state = cc
            .wgpu_render_state
            .clone()
            .expect("FractalGen requires the wgpu backend (eframe::Renderer::Wgpu)");

        let renderer = {
            let mut egui_renderer = render_state.renderer.write();
            FractalRenderer::new(&render_state.device, &mut egui_renderer, 800, 600)
        };

        Self {
            params: FractalParams::default(),
            view: ViewState::default(),
            renderer,
            render_state,
            export: ExportState::default(),
            bookmarks: crate::presets::load_bookmarks(),
            bookmarks_ui: BookmarksState::default(),
            palette_ui: PaletteEditorState::default(),
        }
    }
}

impl eframe::App for FractalApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        header_bar(ui, self);
        properties_panel(ui, self);
        status_bar(ui, self);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::viewport_bg()))
            .show(ui, |ui| {
                viewport::show(ui, self);
            });
    }
}

/// Top chrome strip: app identity on the left, a quick "Reset view" action on
/// the right — mirrors Blender's header holding global/frequent actions
/// rather than burying them in a properties tab.
fn header_bar(ui: &mut egui::Ui, app: &mut FractalApp) {
    egui::Panel::top("header_bar")
        .exact_size(34.0)
        .frame(egui::Frame::new().fill(theme::header_bg()).inner_margin(egui::Margin::symmetric(10, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("🌀 FractalGen").strong().size(15.0).color(theme::ACCENT));
                ui.label(
                    egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .color(theme::TEXT_WEAK)
                        .small(),
                );
                ui.separator();
                ui.label(egui::RichText::new(app.params.kind.display_name()).color(theme::TEXT_WEAK));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Reset view").clicked() {
                        app.params.zoom = 1.0;
                        reset_center(app);
                    }
                });
            });
        });
}

/// Bottom chrome strip: live camera/capability readout, Blender-status-bar
/// style, kept separate from the viewport itself so the render stays clean.
fn status_bar(ui: &mut egui::Ui, app: &FractalApp) {
    egui::Panel::bottom("status_bar")
        .exact_size(24.0)
        .frame(egui::Frame::new().fill(theme::header_bg()).inner_margin(egui::Margin::symmetric(10, 0)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new(viewport::status_text(app)).monospace().color(theme::TEXT_WEAK).size(11.5));
            });
        });
}

/// Left chrome: a narrow icon-tab column (Blender Properties-editor style)
/// plus the active tab's content, replacing the old single long scrolling
/// stack of every section at once.
fn properties_panel(ui: &mut egui::Ui, app: &mut FractalApp) {
    egui::Panel::left("properties_panel")
        .default_size(320.0)
        .size_range(280.0..=480.0)
        .frame(egui::Frame::new().fill(theme::header_bg()))
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                icon_tab_column(ui, app);
                ui.separator();
                // `Frame`/`ScrollArea` don't force a layout direction of their
                // own — they inherit whatever's ambient, which here would
                // otherwise still be `horizontal_top`'s left-to-right flow.
                // Force vertical explicitly so properties sections stack
                // top-to-bottom instead of spreading out sideways.
                ui.vertical(|ui| {
                    ui.set_min_width(ui.available_width());
                    egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                        egui::ScrollArea::vertical().id_salt("properties_scroll").show(ui, |ui| {
                            active_tab_content(ui, app);
                        });
                    });
                });
            });
        });
}

fn icon_tab_column(ui: &mut egui::Ui, app: &mut FractalApp) {
    ui.vertical(|ui| {
        ui.add_space(4.0);
        for tab in PropertiesTab::ALL {
            let selected = app.view.active_tab == tab;
            let button = egui::Button::new(egui::RichText::new(tab.icon()).size(18.0))
                .fill(if selected { theme::ACCENT.gamma_multiply(0.35) } else { egui::Color32::TRANSPARENT })
                .min_size(egui::vec2(36.0, 36.0));
            if ui.add(button).on_hover_text(tab.label()).clicked() {
                app.view.active_tab = tab;
            }
            ui.add_space(2.0);
        }
    });
}

fn active_tab_content(ui: &mut egui::Ui, app: &mut FractalApp) {
    ui.label(egui::RichText::new(app.view.active_tab.label()).strong().size(16.0));
    ui.add_space(6.0);
    match app.view.active_tab {
        PropertiesTab::Fractal => side_panel::show(ui, app),
        PropertiesTab::Palette => palette_editor::show(ui, app),
        PropertiesTab::Bookmarks => bookmarks::show(ui, app),
        PropertiesTab::Export => export_panel::show(ui, app),
        PropertiesTab::About => about::show(ui, app),
    }
}
