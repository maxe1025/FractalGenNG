use eframe::egui;
use fractal_core::{Bookmark, FractalParams};

use crate::gpu::FractalRenderer;
use crate::ui::bookmarks::BookmarksState;
use crate::ui::export_panel::ExportState;
use crate::ui::palette_editor::PaletteEditorState;
use crate::ui::{bookmarks, export_panel, palette_editor, side_panel, viewport};
use crate::view_state::ViewState;

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
        egui::Panel::left("side_panel").min_size(320.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                side_panel::show(ui, self);
                ui.separator();
                palette_editor::show(ui, self);
                ui.separator();
                bookmarks::show(ui, self);
                ui.separator();
                export_panel::show(ui, self);
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            viewport::show(ui, self);
        });
    }
}
