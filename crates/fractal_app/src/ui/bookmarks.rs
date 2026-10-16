use eframe::egui;
use fractal_core::Bookmark;

use crate::app::FractalApp;
use crate::ui::theme;

#[derive(Default)]
pub struct BookmarksState {
    pub new_name: String,
}

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    theme::section(ui, "Save current view", |ui| {
        if app.is_3d() {
            ui.label(egui::RichText::new("Bookmarks hold 2D views only for now.").color(theme::TEXT_WEAK).small());
            return;
        }
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut app.bookmarks_ui.new_name);
            let name_valid = !app.bookmarks_ui.new_name.trim().is_empty();
            if ui.add_enabled(name_valid, egui::Button::new("Save")).clicked() {
                app.bookmarks.push(Bookmark {
                    name: app.bookmarks_ui.new_name.trim().to_string(),
                    params: app.params.clone(),
                });
                app.bookmarks_ui.new_name.clear();
                crate::presets::save_bookmarks(&app.bookmarks);
            }
        });
    });

    theme::section(ui, "Saved", |ui| {
        let mut goto_index = None;
        let mut delete_index = None;
        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for (i, bookmark) in app.bookmarks.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(&bookmark.name);
                    if ui.small_button("Go to").clicked() {
                        goto_index = Some(i);
                    }
                    if ui.small_button("Delete").clicked() {
                        delete_index = Some(i);
                    }
                });
            }
        });

        if let Some(i) = goto_index {
            app.params = app.bookmarks[i].params.clone();
            app.view.dimension = crate::view_state::Dimension::Two;
        }
        if let Some(i) = delete_index {
            app.bookmarks.remove(i);
            crate::presets::save_bookmarks(&app.bookmarks);
        }
    });
}
