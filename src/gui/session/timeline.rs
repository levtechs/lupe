use eframe::egui::{self, Align, Align2, Color32, Context, FontId, Layout, RichText, Sense, Stroke, Ui, Vec2};

use crate::app::App;
use crate::gui::theme;

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

    ui.add_space(6.0);
    theme::panel_frame().show(ui, |ui| {
        ui.set_min_height((ui.available_height() - 2.0).max(120.0));
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Timeline").strong().size(19.0));
                ui.label(RichText::new(format!("{} tracks arranged across {} beats", tracks.len(), timeline_beats.ceil())).small().color(theme::MUTED));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.add(Button::new("New track").fill(theme::ACCENT.gamma_multiply(0.55))).clicked() {
                    app.add_audio_track();
                }
                badge(ui, if loop_enabled { "LOOP ON" } else { "LOOP OFF" }, loop_enabled);
                badge(ui, &format!("{bpm} BPM"), false);
                badge(ui, &format!("{beats_per_bar}/{beat_unit}"), false);
            });
        });
        ui.add_space(10.0);

        egui::ScrollArea::both().id_salt("timeline_scroll").show(ui, |ui| {
            draw_ruler(ui, app, timeline_beats);
            ui.add_space(5.0);
            for (track_index, track) in tracks.iter().enumerate() {
                track_row::show(ui, app, track_index, track, timeline_beats);
                ui.add_space(6.0);
            }
        });
    });

    ctx.request_repaint();
}

use eframe::egui::Button;

fn badge(ui: &mut Ui, text: &str, active: bool) {
    let fill = if active { theme::SUCCESS.gamma_multiply(0.35) } else { theme::CARD };
    let stroke = if active { theme::SUCCESS } else { theme::BORDER };
    egui::Frame::none()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .rounding(5.0)
        .inner_margin(egui::Margin::symmetric(7.0, 4.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).monospace().small().color(if active { theme::TEXT } else { theme::MUTED }));
        });
}

fn draw_ruler(ui: &mut Ui, app: &mut App, timeline_beats: f32) {
    let beats_per_bar = app.project.as_ref().map(|project| project.transport.beats_per_bar).unwrap_or(4) as usize;
    let total_beats = timeline_beats.ceil() as usize;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(INDEX_WIDTH + timeline_beats * BEAT_WIDTH, 30.0), Sense::click());

    ui.painter().rect_filled(rect, 6.0, theme::CARD);
    ui.painter().rect_stroke(rect, 6.0, Stroke::new(1.0, theme::BORDER));
    ui.painter().text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        Align2::LEFT_CENTER,
        "BARS",
        FontId::proportional(10.0),
        theme::MUTED,
    );
    for beat in 0..=total_beats {
        let x = rect.left() + INDEX_WIDTH + beat as f32 * BEAT_WIDTH;
        let bar = beat % beats_per_bar.max(1) == 0;
        ui.painter().line_segment(
            [egui::pos2(x, rect.bottom() - if bar { 13.0 } else { 8.0 }), egui::pos2(x, rect.bottom())],
            Stroke::new(if bar { 1.4 } else { 1.0 }, if bar { theme::BORDER_STRONG } else { theme::BORDER }),
        );
        if beat < total_beats && bar {
            ui.painter().text(
                egui::pos2(x + 5.0, rect.top() + 5.0),
                Align2::LEFT_TOP,
                format!("{}", beat / beats_per_bar.max(1) + 1),
                FontId::monospace(11.0),
                Color32::from_gray(210),
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
