//! Projects in the app: capturing and applying the app state as a
//! [`fractal_core::project::Project`], the File menu and its shortcuts, the
//! window title, and the prompt before unsaved changes would be lost (new,
//! open, or closing the window).

use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Key, KeyboardShortcut, Modifiers};
use fractal_core::fractal3d::Params3D;
use fractal_core::math_overlay::MathOverlay;
use fractal_core::project::{ExportSettings, Mode, Project, FILE_EXTENSION, FORMAT_VERSION};
use fractal_core::FractalParams;

use crate::app::FractalApp;
use crate::ui::export_panel::ExportState;
use crate::ui::theme;
use crate::ui::timeline::TimelineState;
use crate::view_state::{Dimension, ViewState};

const NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const SAVE_AS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);

#[derive(Default)]
pub struct ProjectState {
    /// Where the project was last saved or opened from; `None` = untitled.
    pub path: Option<PathBuf>,
    /// The project as last saved or opened, serialized — compared against
    /// the current state to detect unsaved changes.
    saved: String,
    /// An action waiting for the unsaved-changes prompt.
    pending: Option<Action>,
    /// An error to show in a dialog.
    error: Option<String>,
    /// Set once the user agreed to close, so the next close request passes.
    allow_close: bool,
    /// The window title last sent, to only send changes.
    title: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Action {
    New,
    Open,
    Close,
}

impl Action {
    fn verb(self) -> &'static str {
        match self {
            Action::New => "starting a new project",
            Action::Open => "opening another project",
            Action::Close => "closing",
        }
    }
}

/// The app state a project file holds.
pub fn capture(app: &FractalApp) -> Project {
    Project {
        format_version: FORMAT_VERSION,
        written_by: env!("CARGO_PKG_VERSION").to_string(),
        mode: if app.is_3d() { Mode::Volume } else { Mode::Plane },
        plane: app.params.clone(),
        volume: app.params3d.clone(),
        animation: app.timeline.settings(),
        overlay: app.overlay,
        export: ExportSettings {
            width: app.export.width,
            height: app.export.height,
            supersample: app.export.supersample,
        },
        fly_speed: app.view.fly_speed,
    }
}

/// Replaces the app's project state with `project`. Transient state (a
/// running export, playback, selections) is reset; bookmarks and the GPU
/// renderer are not part of a project and stay as they are.
pub fn apply(app: &mut FractalApp, project: Project) {
    app.params = project.plane;
    app.params3d = project.volume;
    app.timeline.load_settings(project.animation);
    app.overlay = project.overlay;
    app.export = ExportState {
        width: project.export.width.max(1),
        height: project.export.height.max(1),
        supersample: project.export.supersample.clamp(1, 4),
        ..ExportState::default()
    };
    app.view = ViewState {
        active_tab: app.view.active_tab,
        dimension: match project.mode {
            Mode::Plane => Dimension::Two,
            Mode::Volume => Dimension::Three,
        },
        fly_speed: project.fly_speed,
        ..ViewState::default()
    };
    app.palette_ui.selected_stop = 0;
}

fn snapshot(app: &FractalApp) -> String {
    capture(app).to_ron().unwrap_or_default()
}

/// Records the current state as saved (after save/open/new, and at startup).
pub fn mark_saved(app: &mut FractalApp) {
    app.project.saved = snapshot(app);
}

fn is_dirty(app: &FractalApp) -> bool {
    snapshot(app) != app.project.saved
}

fn display_name(path: Option<&Path>) -> String {
    path.and_then(|p| p.file_stem()).map_or_else(|| "Untitled".to_string(), |s| s.to_string_lossy().into_owned())
}

/// Per-frame bookkeeping: shortcuts, intercepting a close with unsaved
/// changes, the window title, and the dialogs. Call before the panels so
/// Ctrl+S etc. reach it before any widget.
pub fn update(ctx: &egui::Context, app: &mut FractalApp) {
    let dirty = is_dirty(app);

    let dialog_open = app.project.pending.is_some() || app.project.error.is_some();
    if !dialog_open {
        handle_shortcuts(ctx, app, dirty);
    }

    if ctx.input(|i| i.viewport().close_requested()) && dirty && !app.project.allow_close {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        app.project.pending = Some(Action::Close);
    }

    let title = format!(
        "{}{} — FractalGen",
        display_name(app.project.path.as_deref()),
        if dirty { "*" } else { "" }
    );
    if title != app.project.title {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
        app.project.title = title;
    }

    unsaved_changes_dialog(ctx, app);
    error_dialog(ctx, app);
}

fn handle_shortcuts(ctx: &egui::Context, app: &mut FractalApp, dirty: bool) {
    // Most specific first: consume_shortcut ignores extra Shift, so Ctrl+S
    // would otherwise also fire on Ctrl+Shift+S.
    if ctx.input_mut(|i| i.consume_shortcut(&SAVE_AS)) {
        save_as(app);
    } else if ctx.input_mut(|i| i.consume_shortcut(&SAVE)) {
        save(app);
    } else if ctx.input_mut(|i| i.consume_shortcut(&OPEN)) {
        request(ctx, app, Action::Open, dirty);
    } else if ctx.input_mut(|i| i.consume_shortcut(&NEW)) {
        request(ctx, app, Action::New, dirty);
    }
}

/// The File menu, for the header bar.
pub fn file_menu(ui: &mut egui::Ui, app: &mut FractalApp) {
    ui.menu_button("File", |ui| {
        let ctx = ui.ctx().clone();
        let item = |ui: &mut egui::Ui, label: &str, shortcut: &KeyboardShortcut| {
            ui.add(egui::Button::new(label).shortcut_text(ctx.format_shortcut(shortcut))).clicked()
        };
        if item(ui, "New", &NEW) {
            let dirty = is_dirty(app);
            request(&ctx, app, Action::New, dirty);
            ui.close();
        }
        if item(ui, "Open…", &OPEN) {
            let dirty = is_dirty(app);
            request(&ctx, app, Action::Open, dirty);
            ui.close();
        }
        ui.separator();
        if item(ui, "Save", &SAVE) {
            save(app);
            ui.close();
        }
        if item(ui, "Save As…", &SAVE_AS) {
            save_as(app);
            ui.close();
        }
    });
}

/// Runs `action` now, or after the unsaved-changes prompt if needed.
fn request(ctx: &egui::Context, app: &mut FractalApp, action: Action, dirty: bool) {
    if dirty {
        app.project.pending = Some(action);
    } else {
        perform(ctx, app, action);
    }
}

fn perform(ctx: &egui::Context, app: &mut FractalApp, action: Action) {
    match action {
        Action::New => {
            apply(app, new_project());
            app.project.path = None;
            mark_saved(app);
        }
        Action::Open => open(app),
        Action::Close => {
            app.project.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn new_project() -> Project {
    Project {
        format_version: FORMAT_VERSION,
        written_by: String::new(),
        mode: Mode::Plane,
        plane: FractalParams::default(),
        volume: Params3D::default(),
        animation: TimelineState::default().settings(),
        overlay: MathOverlay::default(),
        export: {
            let e = ExportState::default();
            ExportSettings { width: e.width, height: e.height, supersample: e.supersample }
        },
        fly_speed: ViewState::default().fly_speed,
    }
}

fn file_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new().add_filter("FractalGen project", &[FILE_EXTENSION])
}

fn open(app: &mut FractalApp) {
    let Some(path) = file_dialog().pick_file() else { return };
    match Project::load(&path) {
        Ok(project) => {
            apply(app, project);
            app.project.path = Some(path);
            mark_saved(app);
        }
        Err(e) => app.project.error = Some(format!("Could not open {}:\n{e}", path.display())),
    }
}

/// Saves to the current file, asking for one if the project is untitled.
/// Returns whether the project was saved.
pub fn save(app: &mut FractalApp) -> bool {
    match app.project.path.clone() {
        Some(path) => write(app, path),
        None => save_as(app),
    }
}

pub fn save_as(app: &mut FractalApp) -> bool {
    let name = format!("{}.{FILE_EXTENSION}", display_name(app.project.path.as_deref()));
    let Some(mut path) = file_dialog().set_file_name(name).save_file() else { return false };
    if path.extension().is_none_or(|e| e != FILE_EXTENSION) {
        path.set_extension(FILE_EXTENSION);
    }
    write(app, path)
}

fn write(app: &mut FractalApp, path: PathBuf) -> bool {
    match capture(app).save(&path) {
        Ok(()) => {
            app.project.path = Some(path);
            mark_saved(app);
            true
        }
        Err(e) => {
            app.project.error = Some(format!("Could not save {}:\n{e}", path.display()));
            false
        }
    }
}

fn unsaved_changes_dialog(ctx: &egui::Context, app: &mut FractalApp) {
    let Some(action) = app.project.pending else { return };
    let name = display_name(app.project.path.as_deref());
    let mut choice = None;
    let modal = egui::Modal::new(egui::Id::new("unsaved_changes")).show(ctx, |ui| {
        ui.set_width(340.0);
        ui.label(egui::RichText::new("Unsaved changes").strong().size(15.0));
        ui.add_space(6.0);
        ui.label(format!("Save the changes to “{name}” before {}?", action.verb()));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                choice = Some(true);
            }
            if ui.button("Don't save").clicked() {
                choice = Some(false);
            }
            if ui.button("Cancel").clicked() {
                app.project.pending = None;
            }
        });
    });
    if modal.should_close() {
        app.project.pending = None;
    }
    match choice {
        // Only go ahead once the save actually happened (not cancelled).
        Some(true) => {
            app.project.pending = None;
            if save(app) {
                perform(ctx, app, action);
            }
        }
        Some(false) => {
            app.project.pending = None;
            perform(ctx, app, action);
        }
        None => {}
    }
}

fn error_dialog(ctx: &egui::Context, app: &mut FractalApp) {
    let Some(message) = app.project.error.clone() else { return };
    let modal = egui::Modal::new(egui::Id::new("project_error")).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(egui::RichText::new("Project").strong().size(15.0));
        ui.add_space(6.0);
        ui.label(egui::RichText::new(message).color(theme::TEXT));
        ui.add_space(10.0);
        ui.button("OK").clicked()
    });
    if modal.inner || modal.should_close() {
        app.project.error = None;
    }
}
