//! Blender-style timeline below the viewport: transport controls and the
//! animation's frame range on top, then a frame ruler to scrub and a track
//! with the active dimension's keyframes (2D zoom/parameter or 3D camera).
//!
//! Keying works like Blender's: put the playhead on a frame, set up the view,
//! press I (or "◆ Key") — replacing any keyframe already on that frame.
//! Every playhead move applies the animation to the viewport, so changes
//! that weren't keyed are overwritten by the next frame change.
//!
//! Shortcuts (whenever no text field has focus): Space play/pause, I key,
//! X/Delete delete the selected key, ←/→ step a frame, Shift+←/→ jump to the
//! range's start/end, ↑/↓ jump to the next/previous keyframe.

use std::ops::RangeInclusive;
use std::time::Instant;

use eframe::egui;
use egui::{Color32, Key, Modifiers, Pos2, Rect, Sense, Stroke};
use fractal_core::export::animation::{Easing, Keyframe};
use fractal_core::export::camera_path::CameraKeyframe;
use fractal_core::export::keyframes::{self, Keyed};
use fractal_core::kinds::KindParam;

use crate::app::FractalApp;
use crate::ui::theme;

const RULER_HEIGHT: f32 = 20.0;
/// Pixels around a keyframe diamond that still count as hitting it.
const KEY_HIT_RADIUS: f32 = 7.0;
/// Narrowest/widest visible span, in frames.
const MIN_VIEW_SPAN: f32 = 10.0;
const MAX_VIEW_SPAN: f32 = 100_000.0;
const MAX_FRAME: u32 = 1_000_000;

pub struct TimelineState {
    /// Sorted by frame, at most one per frame (see `keyframes`).
    pub keys_2d: Vec<Keyframe>,
    pub keys_3d: Vec<CameraKeyframe>,
    pub selected_2d: Option<usize>,
    pub selected_3d: Option<usize>,
    /// The playhead.
    pub current_frame: u32,
    /// The rendered range, inclusive; independent of where keyframes sit.
    pub start: u32,
    pub end: u32,
    pub fps: u32,
    pub looping: bool,
    playback: Option<Playback>,
    /// Visible frame span of the ruler and track (zoom/pan).
    view: (f32, f32),
    key_drag: Option<KeyDrag>,
}

struct Playback {
    started: Instant,
    from_frame: u32,
}

struct KeyDrag {
    index: usize,
    origin_frame: u32,
    origin_x: f32,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            keys_2d: Vec::new(),
            keys_3d: Vec::new(),
            selected_2d: None,
            selected_3d: None,
            current_frame: 0,
            start: 0,
            end: 119,
            fps: 30,
            looping: true,
            playback: None,
            view: (-4.0, 124.0),
            key_drag: None,
        }
    }
}

impl TimelineState {
    pub fn range(&self) -> RangeInclusive<u32> {
        self.start..=self.end
    }

    pub fn frame_count(&self) -> u32 {
        self.end - self.start + 1
    }

    pub fn is_playing(&self) -> bool {
        self.playback.is_some()
    }

    fn fit_view(&mut self) {
        let margin = (self.frame_count() as f32 * 0.04).max(2.0);
        self.view = (self.start as f32 - margin, self.end as f32 + margin);
    }
}

/// Runs the keyed track operations against whichever dimension is active,
/// so the UI code below exists once for both.
macro_rules! with_active_track {
    ($app:expr, |$keys:ident, $selected:ident| $body:expr) => {
        if $app.is_3d() {
            let $keys = &mut $app.timeline.keys_3d;
            let $selected = &mut $app.timeline.selected_3d;
            $body
        } else {
            let $keys = &mut $app.timeline.keys_2d;
            let $selected = &mut $app.timeline.selected_2d;
            $body
        }
    };
}

/// Moves the playhead and applies the animation at that frame to the
/// viewport (if the active track has keyframes at all).
pub fn set_frame(app: &mut FractalApp, frame: u32) {
    app.timeline.current_frame = frame.min(MAX_FRAME);
    apply_current_frame(app);
}

fn apply_current_frame(app: &mut FractalApp) {
    let frame = app.timeline.current_frame;
    if app.is_3d() {
        if !app.timeline.keys_3d.is_empty() {
            app.params3d.camera = app.camera_path().camera_at_frame(frame);
            app.view.last_camera_motion = Some(Instant::now());
        }
    } else if !app.timeline.keys_2d.is_empty() {
        app.params = app.timeline_2d().params_at_frame(frame);
    }
}

/// Keys the current view at the playhead, replacing a keyframe already on
/// that frame (keeping its easing).
pub fn insert_key(app: &mut FractalApp) {
    let frame = app.timeline.current_frame;
    if app.is_3d() {
        let keys = &mut app.timeline.keys_3d;
        let easing = keyframes::index_at_frame(keys, frame).map_or(Easing::Linear, |i| keys[i].easing);
        let index = keyframes::insert_or_replace(keys, CameraKeyframe { frame, camera: app.params3d.camera, easing });
        app.timeline.selected_3d = Some(index);
    } else {
        let keys = &mut app.timeline.keys_2d;
        let easing = keyframes::index_at_frame(keys, frame).map_or(Easing::Linear, |i| keys[i].easing);
        let key =
            Keyframe { frame, zoom: app.params.zoom, kind_param: app.params.kind.animatable_param(), easing };
        let index = keyframes::insert_or_replace(keys, key);
        app.timeline.selected_2d = Some(index);
    }
}

pub fn delete_selected_key(app: &mut FractalApp) {
    with_active_track!(app, |keys, selected| {
        if let Some(i) = selected.take().filter(|&i| i < keys.len()) {
            keys.remove(i);
        }
    });
}

pub fn toggle_playback(app: &mut FractalApp) {
    let timeline = &mut app.timeline;
    if timeline.playback.take().is_some() {
        return;
    }
    // Starting from the end (or outside the range) restarts at the start.
    let from_frame = if timeline.range().contains(&timeline.current_frame) && timeline.current_frame < timeline.end {
        timeline.current_frame
    } else {
        timeline.start
    };
    timeline.playback = Some(Playback { started: Instant::now(), from_frame });
    set_frame(app, from_frame);
}

/// Advances playback to the frame due now, at the timeline's fps.
fn tick_playback(ctx: &egui::Context, app: &mut FractalApp) {
    let Some(playback) = &app.timeline.playback else { return };
    let elapsed_frames = (playback.started.elapsed().as_secs_f64() * f64::from(app.timeline.fps)) as u32;
    let mut frame = playback.from_frame.saturating_add(elapsed_frames);
    if frame > app.timeline.end {
        if app.timeline.looping {
            frame = app.timeline.start;
            app.timeline.playback = Some(Playback { started: Instant::now(), from_frame: frame });
        } else {
            frame = app.timeline.end;
            app.timeline.playback = None;
        }
    }
    if frame != app.timeline.current_frame {
        set_frame(app, frame);
    }
    ctx.request_repaint();
}

fn stop_playback(app: &mut FractalApp) {
    app.timeline.playback = None;
}

/// Global timeline shortcuts; call once per frame before any widget can
/// claim the keys (Space would otherwise re-press the last clicked button).
pub fn handle_shortcuts(ctx: &egui::Context, app: &mut FractalApp) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let pressed = |key: Key, modifiers: Modifiers| ctx.input_mut(|i| i.consume_key(modifiers, key));
    if pressed(Key::Space, Modifiers::NONE) {
        toggle_playback(app);
    }
    if pressed(Key::I, Modifiers::NONE) {
        insert_key(app);
    }
    if pressed(Key::X, Modifiers::NONE) || pressed(Key::Delete, Modifiers::NONE) {
        delete_selected_key(app);
    }
    let current = app.timeline.current_frame;
    let target = if pressed(Key::ArrowLeft, Modifiers::SHIFT) {
        Some(app.timeline.start)
    } else if pressed(Key::ArrowRight, Modifiers::SHIFT) {
        Some(app.timeline.end)
    } else if pressed(Key::ArrowLeft, Modifiers::NONE) {
        Some(current.saturating_sub(1))
    } else if pressed(Key::ArrowRight, Modifiers::NONE) {
        Some(current + 1)
    } else if pressed(Key::ArrowUp, Modifiers::NONE) {
        next_key_frame(app, current)
    } else if pressed(Key::ArrowDown, Modifiers::NONE) {
        previous_key_frame(app, current)
    } else {
        None
    };
    if let Some(frame) = target {
        stop_playback(app);
        set_frame(app, frame);
    }
}

fn next_key_frame(app: &FractalApp, frame: u32) -> Option<u32> {
    if app.is_3d() {
        keyframes::next_key_frame(&app.timeline.keys_3d, frame)
    } else {
        keyframes::next_key_frame(&app.timeline.keys_2d, frame)
    }
}

fn previous_key_frame(app: &FractalApp, frame: u32) -> Option<u32> {
    if app.is_3d() {
        keyframes::previous_key_frame(&app.timeline.keys_3d, frame)
    } else {
        keyframes::previous_key_frame(&app.timeline.keys_2d, frame)
    }
}

/// Keeps 2D keyframes' free parameter in step with the current fractal
/// kind — switching kind (e.g. Julia → Multibrot) would otherwise leave
/// seeds on keys for a kind that has none, silently doing nothing.
fn sync_kind_params(app: &mut FractalApp) {
    let current = app.params.kind.animatable_param();
    let same_variant = |a: KindParam, b: KindParam| std::mem::discriminant(&a) == std::mem::discriminant(&b);
    for key in &mut app.timeline.keys_2d {
        if !same_variant(key.kind_param, current) {
            key.kind_param = current;
        }
    }
}

pub fn show(ui: &mut egui::Ui, app: &mut FractalApp) {
    tick_playback(ui.ctx(), app);
    if !app.is_3d() {
        sync_kind_params(app);
    }
    header(ui, app);
    ui.add_space(2.0);
    let area = ui.available_rect_before_wrap();
    ruler_and_track(ui, app, area);
}

fn header(ui: &mut egui::Ui, app: &mut FractalApp) {
    // Wraps onto a second row when the window is too narrow for all controls.
    ui.horizontal_wrapped(|ui| {
        let transport = |ui: &mut egui::Ui, glyph: &str, hint: &str| ui.button(glyph).on_hover_text(hint).clicked();
        if transport(ui, "⏮", "Jump to start (Shift+←)") {
            stop_playback(app);
            set_frame(app, app.timeline.start);
        }
        if transport(ui, "⏪", "Previous keyframe (↓)")
            && let Some(frame) = previous_key_frame(app, app.timeline.current_frame)
        {
            stop_playback(app);
            set_frame(app, frame);
        }
        let play_glyph = if app.timeline.is_playing() { "⏸" } else { "▶" };
        if transport(ui, play_glyph, "Play/pause (Space)") {
            toggle_playback(app);
        }
        if transport(ui, "⏩", "Next keyframe (↑)")
            && let Some(frame) = next_key_frame(app, app.timeline.current_frame)
        {
            stop_playback(app);
            set_frame(app, frame);
        }
        if transport(ui, "⏭", "Jump to end (Shift+→)") {
            stop_playback(app);
            set_frame(app, app.timeline.end);
        }
        ui.toggle_value(&mut app.timeline.looping, "🔁").on_hover_text("Loop playback");

        ui.separator();
        let mut frame = app.timeline.current_frame;
        ui.label("Frame");
        if ui.add(egui::DragValue::new(&mut frame).range(0..=MAX_FRAME)).changed() {
            stop_playback(app);
            set_frame(app, frame);
        }
        ui.label("Start");
        let end = app.timeline.end;
        ui.add(egui::DragValue::new(&mut app.timeline.start).range(0..=end.saturating_sub(1)));
        ui.label("End");
        let start = app.timeline.start;
        ui.add(egui::DragValue::new(&mut app.timeline.end).range(start + 1..=MAX_FRAME));
        ui.label("FPS");
        ui.add(egui::DragValue::new(&mut app.timeline.fps).range(1..=240));
        ui.label(
            egui::RichText::new(format!(
                "{} frames · {:.1} s",
                app.timeline.frame_count(),
                f64::from(app.timeline.frame_count()) / f64::from(app.timeline.fps)
            ))
            .color(theme::TEXT_WEAK)
            .small(),
        );

        ui.separator();
        if ui.button("◆ Key").on_hover_text("Key the current view at the playhead (I)").clicked() {
            insert_key(app);
        }
        let has_selection =
            if app.is_3d() { app.timeline.selected_3d.is_some() } else { app.timeline.selected_2d.is_some() };
        if ui.add_enabled(has_selection, egui::Button::new("🗑")).on_hover_text("Delete the selected keyframe (X)").clicked()
        {
            delete_selected_key(app);
        }
        with_active_track!(app, |keys, selected| selected_key_controls(ui, keys, selected));

        ui.separator();
        if ui.small_button("Fit").on_hover_text("Show the whole frame range (wheel zooms, Ctrl+wheel or middle-drag pans)").clicked()
        {
            app.timeline.fit_view();
        }
    });
}

/// Easing of the segment leading into the selected keyframe (none for the
/// first keyframe, which no segment leads into).
fn selected_key_controls<K: Keyed>(ui: &mut egui::Ui, keys: &mut [K], selected: &mut Option<usize>) {
    let Some(i) = selected.filter(|&i| i < keys.len()) else {
        *selected = None;
        return;
    };
    ui.label(egui::RichText::new(format!("Key @ {}", keys[i].frame())).color(theme::TEXT_WEAK));
    if i == 0 {
        return;
    }
    let easing = keys[i].easing_mut();
    let name = match easing {
        Easing::Linear => "Linear",
        Easing::Accelerating { .. } => "Accelerating",
    };
    egui::ComboBox::from_id_salt("timeline_key_easing").selected_text(name).width(100.0).show_ui(ui, |ui| {
        if ui.selectable_label(matches!(easing, Easing::Linear), "Linear").clicked() {
            *easing = Easing::Linear;
        }
        if ui.selectable_label(matches!(easing, Easing::Accelerating { .. }), "Accelerating").clicked() {
            *easing = Easing::Accelerating { exponent: 2.0 };
        }
    });
    if let Easing::Accelerating { exponent } = easing {
        ui.add(egui::DragValue::new(exponent).range(0.2..=5.0).speed(0.05).prefix("×"))
            .on_hover_text("Easing into this key: above 1 starts slow and speeds up; below 1 starts fast and slows down.");
    }
}

/// Maps frames to x positions within the timeline area for the current view.
struct FrameAxis {
    left: f32,
    view_start: f32,
    px_per_frame: f32,
}

impl FrameAxis {
    fn x(&self, frame: f32) -> f32 {
        self.left + (frame - self.view_start) * self.px_per_frame
    }

    fn frame_at(&self, x: f32) -> u32 {
        (self.view_start + (x - self.left) / self.px_per_frame).round().clamp(0.0, MAX_FRAME as f32) as u32
    }
}

fn ruler_and_track(ui: &mut egui::Ui, app: &mut FractalApp, area: Rect) {
    let area = Rect::from_min_size(area.min, egui::vec2(area.width(), area.height().max(RULER_HEIGHT + 24.0)));
    ui.allocate_rect(area, Sense::hover());
    let ruler = Rect::from_min_size(area.min, egui::vec2(area.width(), RULER_HEIGHT));
    let track = Rect::from_min_max(egui::pos2(area.left(), ruler.bottom()), area.max);

    let ruler_response = ui.interact(ruler, ui.id().with("timeline_ruler"), Sense::click_and_drag());
    let track_response = ui.interact(track, ui.id().with("timeline_track"), Sense::click_and_drag());

    navigate_view(ui, app, area, [&ruler_response, &track_response]);
    let (view_start, view_end) = app.timeline.view;
    let axis = FrameAxis { left: area.left(), view_start, px_per_frame: area.width() / (view_end - view_start) };

    // Scrub: press or drag on the ruler.
    let primary_down = ui.input(|i| i.pointer.primary_down());
    if ruler_response.is_pointer_button_down_on()
        && primary_down
        && let Some(pos) = ruler_response.interact_pointer_pos()
    {
        let frame = axis.frame_at(pos.x);
        stop_playback(app);
        if frame != app.timeline.current_frame {
            set_frame(app, frame);
        }
    }
    track_interaction(ui, app, &track_response, &axis);

    paint(ui, app, ruler, track, &axis);
}

/// Mouse wheel zooms around the pointer, horizontal wheel/Ctrl+wheel and
/// middle-drag pan.
fn navigate_view(ui: &egui::Ui, app: &mut FractalApp, area: Rect, responses: [&egui::Response; 2]) {
    let (start, end) = app.timeline.view;
    let frames_per_px = (end - start) / area.width().max(1.0);
    for response in responses {
        if response.dragged_by(egui::PointerButton::Middle) {
            let shift = -response.drag_delta().x * frames_per_px;
            app.timeline.view = (start + shift, end + shift);
        }
    }
    let Some(pointer) = ui.input(|i| i.pointer.hover_pos()).filter(|p| area.contains(*p)) else { return };
    let (scroll, ctrl) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.ctrl));
    let (start, end) = app.timeline.view;
    let pan_px = if ctrl { scroll.y + scroll.x } else { scroll.x };
    if pan_px != 0.0 {
        let shift = -pan_px * frames_per_px;
        app.timeline.view = (start + shift, end + shift);
    }
    if !ctrl && scroll.y != 0.0 {
        let span = end - start;
        let new_span = (span * (-scroll.y * 0.003).exp()).clamp(MIN_VIEW_SPAN, MAX_VIEW_SPAN);
        let anchor = start + (pointer.x - area.left()) / area.width().max(1.0) * span;
        let ratio = new_span / span;
        app.timeline.view = (anchor - (anchor - start) * ratio, anchor + (end - anchor) * ratio);
    }
}

/// Click a key to select it and move the playhead there; drag it to retime
/// it; click or drag on empty track to scrub.
fn track_interaction(ui: &egui::Ui, app: &mut FractalApp, response: &egui::Response, axis: &FrameAxis) {
    let frames: Vec<u32> = if app.is_3d() {
        app.timeline.keys_3d.iter().map(|k| k.frame).collect()
    } else {
        app.timeline.keys_2d.iter().map(|k| k.frame).collect()
    };
    let hit = |x: f32| {
        frames
            .iter()
            .enumerate()
            .map(|(i, &f)| (i, (axis.x(f as f32) - x).abs()))
            .filter(|&(_, d)| d <= KEY_HIT_RADIUS)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };

    if let Some(pos) = response.hover_pos()
        && hit(pos.x).is_some()
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }

    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(pos) = response.interact_pointer_pos()
        && let Some(index) = hit(pos.x)
    {
        app.timeline.key_drag = Some(KeyDrag { index, origin_frame: frames[index], origin_x: pos.x });
        select(app, Some(index));
    }

    if let Some(KeyDrag { index, origin_frame, origin_x }) = app.timeline.key_drag {
        let mut index = index;
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            let delta = ((pos.x - origin_x) / axis.px_per_frame).round() as i64;
            let target = (i64::from(origin_frame) + delta).clamp(0, i64::from(MAX_FRAME)) as u32;
            index = with_active_track!(app, |keys, selected| {
                let i = keyframes::move_key(keys, index, target);
                *selected = Some(i);
                i
            });
            app.timeline.key_drag = Some(KeyDrag { index, origin_frame, origin_x });
        }
        if response.drag_stopped() {
            app.timeline.key_drag = None;
            let frame = key_frame(app, index);
            set_frame(app, frame);
        }
        return;
    }

    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        match hit(pos.x) {
            Some(index) => {
                select(app, Some(index));
                stop_playback(app);
                set_frame(app, frames[index]);
            }
            None => select(app, None),
        }
    } else if response.dragged_by(egui::PointerButton::Primary)
        && let Some(pos) = response.interact_pointer_pos()
    {
        let frame = axis.frame_at(pos.x);
        stop_playback(app);
        if frame != app.timeline.current_frame {
            set_frame(app, frame);
        }
    }
}

fn select(app: &mut FractalApp, index: Option<usize>) {
    with_active_track!(app, |_keys, selected| *selected = index);
}

fn key_frame(app: &FractalApp, index: usize) -> u32 {
    if app.is_3d() { app.timeline.keys_3d[index].frame } else { app.timeline.keys_2d[index].frame }
}

fn paint(ui: &egui::Ui, app: &FractalApp, ruler: Rect, track: Rect, axis: &FrameAxis) {
    let painter = ui.painter_at(ruler.union(track));
    let timeline = &app.timeline;
    painter.rect_filled(ruler, 0.0, theme::header_bg());
    painter.rect_filled(track, 0.0, theme::section_bg());

    // Shade everything outside the render range.
    let out_of_range = Color32::from_black_alpha(90);
    let range_left = axis.x(timeline.start as f32 - 0.5);
    let range_right = axis.x(timeline.end as f32 + 0.5);
    let full = ruler.union(track);
    if range_left > full.left() {
        painter.rect_filled(Rect::from_min_max(full.min, egui::pos2(range_left, full.bottom())), 0.0, out_of_range);
    }
    if range_right < full.right() {
        painter.rect_filled(Rect::from_min_max(egui::pos2(range_right, full.top()), full.max), 0.0, out_of_range);
    }

    // Frames where the 3D camera path runs through the fractal.
    if app.is_3d() && !timeline.is_playing() && !timeline.keys_3d.is_empty() {
        let blocked = app.camera_path().blocked_frames(timeline.range());
        let warn = Color32::from_rgba_unmultiplied(220, 60, 50, 90);
        for (a, b) in frame_runs(&blocked) {
            let r = Rect::from_min_max(
                egui::pos2(axis.x(a as f32 - 0.5), track.top()),
                egui::pos2(axis.x(b as f32 + 0.5), track.bottom()),
            );
            painter.rect_filled(r, 0.0, warn);
        }
    }

    // Ticks: the smallest 1-2-5 step at least ~48 px apart.
    let step = tick_step(axis.px_per_frame);
    let first = ((axis.view_start / step as f32).ceil() as i64).max(0) * step as i64;
    let grid = Stroke::new(1.0, Color32::from_white_alpha(14));
    let mut frame = first;
    while (frame as f32) <= axis.view_start + track.width() / axis.px_per_frame {
        let x = axis.x(frame as f32);
        painter.line_segment([egui::pos2(x, ruler.bottom() - 5.0), egui::pos2(x, ruler.bottom())], Stroke::new(1.0, theme::TEXT_WEAK));
        painter.line_segment([egui::pos2(x, track.top()), egui::pos2(x, track.bottom())], grid);
        painter.text(
            egui::pos2(x, ruler.top() + 2.0),
            egui::Align2::CENTER_TOP,
            frame.to_string(),
            egui::FontId::proportional(10.5),
            theme::TEXT_WEAK,
        );
        frame += step as i64;
    }

    // Keyframes.
    let (frames, selected): (Vec<u32>, Option<usize>) = if app.is_3d() {
        (timeline.keys_3d.iter().map(|k| k.frame).collect(), timeline.selected_3d)
    } else {
        (timeline.keys_2d.iter().map(|k| k.frame).collect(), timeline.selected_2d)
    };
    let cy = track.center().y;
    for (i, &f) in frames.iter().enumerate() {
        let is_selected = selected == Some(i);
        let fill = if is_selected { theme::ACCENT } else { Color32::from_gray(200) };
        let stroke = Stroke::new(1.0, if is_selected { Color32::WHITE } else { Color32::from_gray(40) });
        painter.add(egui::Shape::convex_polygon(diamond(egui::pos2(axis.x(f as f32), cy), 6.0), fill, stroke));
    }
    if frames.is_empty() {
        painter.text(
            track.center(),
            egui::Align2::CENTER_CENTER,
            "No keyframes — set up the view and press I to key it at the playhead",
            egui::FontId::proportional(12.0),
            theme::TEXT_WEAK,
        );
    }

    // Playhead.
    let x = axis.x(timeline.current_frame as f32);
    painter.line_segment([egui::pos2(x, ruler.top()), egui::pos2(x, track.bottom())], Stroke::new(2.0, theme::ACCENT));
    let label = timeline.current_frame.to_string();
    let galley = painter.layout_no_wrap(label, egui::FontId::proportional(11.0), Color32::from_gray(15));
    let badge = Rect::from_center_size(egui::pos2(x, ruler.center().y), galley.size() + egui::vec2(8.0, 2.0));
    painter.rect_filled(badge, 3.0, theme::ACCENT);
    painter.galley(badge.min + egui::vec2(4.0, 1.0), galley, Color32::from_gray(15));
}

fn diamond(center: Pos2, r: f32) -> Vec<Pos2> {
    vec![center + egui::vec2(0.0, -r), center + egui::vec2(r, 0.0), center + egui::vec2(0.0, r), center + egui::vec2(-r, 0.0)]
}

/// Smallest step from 1, 2, 5, 10, 20, 50, ... that keeps labels ~48 px apart.
fn tick_step(px_per_frame: f32) -> u32 {
    let mut magnitude = 1u32;
    loop {
        for m in [1, 2, 5] {
            let step = m * magnitude;
            if step as f32 * px_per_frame >= 48.0 || step >= 1_000_000 {
                return step;
            }
        }
        magnitude *= 10;
    }
}

/// Consecutive frames grouped into inclusive runs.
pub(crate) fn frame_runs(frames: &[u32]) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &f in frames {
        match runs.last_mut() {
            Some((_, end)) if *end + 1 == f => *end = f,
            _ => runs.push((f, f)),
        }
    }
    runs
}

/// `[3, 4, 5, 9]` → `"3–5, 9"`.
pub(crate) fn frame_ranges(frames: &[u32]) -> String {
    frame_runs(frames)
        .iter()
        .map(|&(a, b)| if a == b { a.to_string() } else { format!("{a}–{b}") })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_steps_follow_1_2_5_and_keep_labels_apart() {
        assert_eq!(tick_step(60.0), 1);
        assert_eq!(tick_step(30.0), 2);
        assert_eq!(tick_step(12.0), 5);
        assert_eq!(tick_step(5.0), 10);
        assert_eq!(tick_step(2.0), 50);
        assert_eq!(tick_step(0.05), 1000);
    }

    #[test]
    fn frame_ranges_group_consecutive_frames() {
        assert_eq!(frame_ranges(&[3, 4, 5, 9, 12, 13]), "3–5, 9, 12–13");
        assert_eq!(frame_ranges(&[7]), "7");
        assert!(frame_ranges(&[]).is_empty());
    }

    #[test]
    fn transport_glyphs_are_drawable() {
        for glyph in ["⏮", "⏪", "▶", "⏸", "⏩", "⏭", "🔁", "◆", "🗑"] {
            assert!(crate::overlay_burn::missing_glyphs(glyph).is_empty(), "no glyph for {glyph}");
        }
    }
}
