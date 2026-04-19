use eframe::egui::{self, Align2, Color32, Context, FontId, RichText, Sense, Ui, Vec2};

use crate::app::App;

use super::track_row::{self, BEAT_WIDTH, INDEX_WIDTH};

pub fn show(ui: &mut Ui, ctx: &Context, app: &mut App) {
    let Some(project) = app.project.as_ref() else {
        return;
    };
    let timeline_beats = app.max_timeline_beats();
    let beats_per_bar = project.transport.beats_per_bar;
    let beat_unit = project.transport.beat_unit;
    let bpm = project.transport.bpm;
    let loop_enabled = project.transport.loop_enabled;
    let tracks = project.tracks.clone();

    ui.vertical(|ui| {
        ui.set_width(ui.available_width());

        ui.horizontal(|ui| {
            ui.label(RichText::new("Tracks").strong());
            ui.separator();
            ui.label(format!("{} / {}", beats_per_bar, beat_unit));
            ui.label(format!("{} BPM", bpm));
            ui.label(if loop_enabled { "Loop on" } else { "Loop off" });
        });
        ui.add_space(6.0);

        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::both().id_salt("timeline_scroll").show(ui, |ui| {
                draw_ruler(ui, app, timeline_beats);

                for (track_index, track) in tracks.iter().enumerate() {
                    track_row::show(ui, app, track_index, track, timeline_beats);
                    ui.add_space(6.0);
                }

                if ui.button(RichText::new("New track").strong()).clicked() {
                    app.add_audio_track();
                }
                ui.add_space(10.0);
            });
        });
    });

    ctx.request_repaint();
}

fn draw_ruler(ui: &mut Ui, app: &mut App, timeline_beats: f32) {
    let beats_per_bar = app.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4) as usize;
    let total_beats = timeline_beats.ceil() as usize;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(INDEX_WIDTH + timeline_beats * BEAT_WIDTH, 34.0), Sense::click());

    ui.painter().rect_filled(rect, 8.0, Color32::from_rgb(23, 26, 34));
    for beat in 0..=total_beats {
        let x = rect.left() + INDEX_WIDTH + beat as f32 * BEAT_WIDTH;
        ui.painter().line_segment(
            [egui::pos2(x, rect.bottom() - 10.0), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, Color32::from_rgb(130, 136, 150)),
        );
        if beat < total_beats && beat % beats_per_bar.max(1) == 0 {
            ui.painter().text(
                egui::pos2(x + 4.0, rect.top() + 6.0),
                Align2::LEFT_TOP,
                format!("{}", beat / beats_per_bar.max(1) + 1),
                FontId::proportional(12.0),
                Color32::from_gray(220),
            );
        }
    }

    if response.clicked() {
        if let Some(pointer) = response.interact_pointer_pos() {
            if pointer.x >= rect.left() + INDEX_WIDTH {
                let beat = ((pointer.x - rect.left() - INDEX_WIDTH) / BEAT_WIDTH).clamp(0.0, timeline_beats);
                app.set_playhead(beat);
            }
        }
    }
}
