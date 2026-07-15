use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Stroke, Ui, Vec2};

use crate::app::{App, ClipSelection};
use crate::gui::{theme, util};
use crate::project::{AudioClip, Track};

pub const INDEX_WIDTH: f32 = 118.0;
pub const ROW_HEIGHT: f32 = 76.0;
pub const BEAT_WIDTH: f32 = 48.0;

pub fn show(ui: &mut Ui, app: &mut App, track_index: usize, track: &Track, timeline_beats: f32) {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(INDEX_WIDTH + timeline_beats * BEAT_WIDTH, ROW_HEIGHT),
        Sense::click_and_drag(),
    );

    let row_rect = rect;
    let bg = if track_index == app.selected_track {
        theme::CARD_HOVER
    } else {
        theme::CARD
    };

    ui.painter().rect_filled(row_rect, 8.0, bg);
    ui.painter().rect_stroke(
        row_rect,
        8.0,
        Stroke::new(1.0, if track_index == app.selected_track { theme::BORDER_STRONG } else { theme::BORDER }),
    );

    let accent = util::track_accent(track.color);
    let index_rect = Rect::from_min_size(row_rect.min, Vec2::new(INDEX_WIDTH, row_rect.height()));
    ui.painter().rect_filled(index_rect, 8.0, theme::CANVAS);
    ui.painter().rect_filled(Rect::from_min_size(index_rect.min, Vec2::new(4.0, index_rect.height())), 2.0, accent);
    ui.painter().text(
        index_rect.left_top() + egui::vec2(10.0, 10.0),
        Align2::LEFT_TOP,
        format!("{:02}", track_index + 1),
        FontId::monospace(13.0),
        accent,
    );
    ui.painter().text(
        index_rect.left_bottom() - egui::vec2(-10.0, 16.0),
        Align2::LEFT_BOTTOM,
        &track.name,
        FontId::proportional(13.0),
        theme::TEXT,
    );

    let timeline_rect = Rect::from_min_max(
        egui::pos2(row_rect.left() + INDEX_WIDTH, row_rect.top()),
        row_rect.max,
    );
    draw_grid(ui, app, timeline_rect);
    draw_clips(ui, app, track_index, track, timeline_rect);
    draw_playhead(ui, app, timeline_rect);

    if response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            if pointer.x >= timeline_rect.left() {
                let beat = ((pointer.x - timeline_rect.left()) / BEAT_WIDTH).clamp(0.0, timeline_beats);
                app.set_playhead(beat);
                app.clear_clip_selection();
            }
        }
        app.select_track(track_index);
    }
}

fn draw_grid(ui: &Ui, app: &App, timeline_rect: Rect) {
    let beats_per_bar = app.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4) as usize;
    let total_beats = app.max_timeline_beats().ceil() as usize;
    for beat in 0..=total_beats {
        let x = timeline_rect.left() + beat as f32 * BEAT_WIDTH;
        let stroke = if beat % beats_per_bar.max(1) == 0 {
            Stroke::new(1.4, theme::BORDER_STRONG)
        } else {
            Stroke::new(1.0, theme::BORDER)
        };
        ui.painter().line_segment([egui::pos2(x, timeline_rect.top()), egui::pos2(x, timeline_rect.bottom())], stroke);
    }
}

fn draw_clips(ui: &Ui, app: &mut App, track_index: usize, track: &Track, timeline_rect: Rect) {
    for (clip_index, clip) in track.clips.iter().enumerate() {
        draw_clip(ui, app, track_index, clip_index, clip, timeline_rect, false, track.color);
    }
    if let Some(clip) = app.active_recording_preview(track_index) {
        draw_clip(ui, app, track_index, usize::MAX, &clip, timeline_rect, true, track.color);
    }
}

fn draw_clip(
    ui: &Ui,
    app: &mut App,
    track_index: usize,
    clip_index: usize,
    clip: &AudioClip,
    timeline_rect: Rect,
    preview: bool,
    color: crate::project::TrackColor,
) {
    let left = timeline_rect.left() + clip.start_beat * BEAT_WIDTH;
    let width = clip.span_beats() * BEAT_WIDTH;
    let rect = Rect::from_min_size(
        egui::pos2(left + 4.0, timeline_rect.top() + 10.0),
        Vec2::new((width - 8.0).max(12.0), timeline_rect.height() - 20.0),
    );
    let selected = app.selected_clip == Some(ClipSelection { track_index, clip_index });
    let accent = util::track_accent(color);
    let fill = if preview {
        accent.gamma_multiply(0.25)
    } else if selected {
        accent.gamma_multiply(0.6)
    } else {
        accent.gamma_multiply(0.38)
    };
    ui.painter().rect_filled(rect, 8.0, fill);
    ui.painter().rect_stroke(rect, 8.0, Stroke::new(1.2, accent));
    draw_clip_preview(ui, app, clip, rect, accent);
    ui.painter().text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        if clip.is_drum_sequence() { format!("{}  [seq]", clip.title) } else { clip.title.clone() },
        FontId::proportional(13.0),
        Color32::WHITE,
    );

    let handle_width = 8.0;
    let left_handle = Rect::from_min_max(rect.min, egui::pos2((rect.left() + handle_width).min(rect.right()), rect.bottom()));
    let right_handle = Rect::from_min_max(egui::pos2((rect.right() - handle_width).max(rect.left()), rect.top()), rect.max);
    let body_rect = Rect::from_min_max(
        egui::pos2((rect.left() + handle_width).min(rect.right()), rect.top()),
        egui::pos2((rect.right() - handle_width).max(rect.left()), rect.bottom()),
    );
    ui.painter().rect_filled(left_handle, 4.0, accent.gamma_multiply(0.9));
    ui.painter().rect_filled(right_handle, 4.0, accent.gamma_multiply(0.9));

    if preview {
        return;
    }

    let left_response = ui.interact(left_handle, ui.id().with(("clip-left", track_index, clip_index)), Sense::click_and_drag());
    let right_response = ui.interact(right_handle, ui.id().with(("clip-right", track_index, clip_index)), Sense::click_and_drag());
    let body_response = ui.interact(body_rect, ui.id().with(("clip", track_index, clip_index)), Sense::click_and_drag());

    if left_response.clicked() || right_response.clicked() || body_response.clicked() {
        app.select_clip(track_index, clip_index);
    }

    if left_response.dragged() {
        let delta_beats = left_response.drag_delta().x / BEAT_WIDTH;
        let snap = ui.input(|input| input.modifiers.shift);
        app.trim_clip_left(track_index, clip_index, clip.start_beat + delta_beats, snap);
    } else if right_response.dragged() {
        let delta_beats = right_response.drag_delta().x / BEAT_WIDTH;
        let snap = ui.input(|input| input.modifiers.shift);
        app.set_clip_end(track_index, clip_index, clip.end_beat() + delta_beats, snap);
    } else if body_response.dragged() {
        let delta_beats = body_response.drag_delta().x / BEAT_WIDTH;
        let snap = ui.input(|input| input.modifiers.shift);
        app.set_clip_start(track_index, clip_index, clip.start_beat + delta_beats, snap);
    }
}

fn draw_clip_preview(ui: &Ui, app: &mut App, clip: &AudioClip, rect: Rect, accent: Color32) {
    if let Some(sequence) = &clip.drum_sequence {
        draw_drum_preview(ui, rect, clip, sequence, accent);
    } else {
        draw_audio_preview(ui, app, rect, clip, accent);
    }
}

fn draw_audio_preview(ui: &Ui, app: &mut App, rect: Rect, clip: &AudioClip, accent: Color32) {
    let Some(path) = clip.file_path.as_deref() else {
        return;
    };
    let Some(preview) = app.audio_preview(path) else {
        return;
    };
    let desired_columns = ((rect.width() / 5.0).floor() as usize).clamp(16, 96);
    let body = Rect::from_min_max(
        egui::pos2(rect.left() + 10.0, rect.top() + 10.0),
        egui::pos2(rect.right() - 10.0, rect.bottom() - 10.0),
    );
    let center_y = body.center().y;
    let max_half_height = (body.height() * 0.36).max(6.0);
    let width = (body.width() / desired_columns as f32).max(2.0);
    for index in 0..desired_columns {
        let start = (index * preview.len()) / desired_columns;
        let end = ((index + 1) * preview.len()) / desired_columns;
        let slice = &preview[start.min(preview.len() - 1)..end.max(start + 1).min(preview.len())];
        let level = slice.iter().copied().fold(0.0_f32, f32::max);
        let x = body.left() + index as f32 * width + width * 0.5;
        let half = (level * max_half_height).max(1.5);
        ui.painter().line_segment(
            [egui::pos2(x, center_y - half), egui::pos2(x, center_y + half)],
            Stroke::new(1.4, accent.gamma_multiply(0.95)),
        );
    }
}

fn draw_drum_preview(ui: &Ui, rect: Rect, clip: &AudioClip, sequence: &crate::project::DrumSequence, accent: Color32) {
    let lane_count = sequence.lanes.len().max(1);
    let step_count = sequence
        .lanes
        .iter()
        .map(|lane| lane.steps.len())
        .max()
        .unwrap_or(0)
        .max(1);
    let base_length_beats = clip.length_beats.max(0.25);
    let span_beats = clip.span_beats();
    let step_beats = base_length_beats / step_count as f32;
    let inner = Rect::from_min_max(
        egui::pos2(rect.left() + 10.0, rect.top() + 12.0),
        egui::pos2(rect.right() - 10.0, rect.bottom() - 12.0),
    );
    let px_per_beat = inner.width() / span_beats.max(0.25);
    let lane_height = inner.height() / lane_count as f32;

    let beat_lines = span_beats.ceil() as usize;
    for beat_index in 0..=beat_lines {
        let x = inner.left() + beat_index as f32 * px_per_beat;
        let strong = (beat_index as f32 % base_length_beats).abs() < 0.01;
        ui.painter().line_segment(
            [egui::pos2(x, inner.top()), egui::pos2(x, inner.bottom())],
            Stroke::new(if strong { 1.0 } else { 0.6 }, accent.gamma_multiply(if strong { 0.35 } else { 0.18 })),
        );
    }

    for lane_index in 0..lane_count {
        let lane_top = inner.top() + lane_index as f32 * lane_height + 2.0;
        let lane_bottom = inner.top() + (lane_index + 1) as f32 * lane_height - 2.0;
        for display_step in 0..((span_beats / step_beats).ceil() as usize) {
            let beat_start = display_step as f32 * step_beats;
            if beat_start >= span_beats {
                break;
            }
            let source_step = (((clip.source_offset_beats / step_beats).floor() as usize) + display_step) % step_count;
            let active = sequence
                .lanes
                .get(lane_index)
                .and_then(|lane| lane.steps.get(source_step))
                .copied()
                .unwrap_or(false);
            if !active {
                continue;
            }
            let x0 = inner.left() + beat_start * px_per_beat;
            let x1 = inner.left() + ((beat_start + step_beats).min(span_beats)) * px_per_beat;
            let cell = Rect::from_min_max(
                egui::pos2(x0, lane_top),
                egui::pos2(x1.max(x0 + 1.0), lane_bottom),
            );
            ui.painter().rect_filled(cell, 1.0, accent.gamma_multiply(0.92));
        }
    }
}

fn draw_playhead(ui: &Ui, app: &App, timeline_rect: Rect) {
    let x = timeline_rect.left() + app.playhead_beats * BEAT_WIDTH;
    let color = if app.metronome_flash > 0.01 {
        Color32::from_rgb(
            (160.0 + 80.0 * app.metronome_flash) as u8,
            (80.0 + 110.0 * app.metronome_flash) as u8,
            90,
        )
    } else {
        Color32::from_rgb(220, 90, 90)
    };
    ui.painter().line_segment([egui::pos2(x, timeline_rect.top()), egui::pos2(x, timeline_rect.bottom())], Stroke::new(2.0, color));
}
