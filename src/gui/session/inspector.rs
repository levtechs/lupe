use eframe::egui::{self, Color32, RichText, Ui};

use crate::app::{track_action_label, App, VOLUME_STEP};
use crate::gui::util;
use crate::project::TrackKind;

pub fn show(ui: &mut Ui, app: &mut App) {
    ui.heading("Inspector");
    ui.add_space(8.0);

    let Some(track) = app.selected_track_ref().cloned() else {
        ui.label("No track selected");
        return;
    };

    ui.label(RichText::new(format!("Row {}", app.selected_track)).weak());

    let mut name = track.name.clone();
    if ui.text_edit_singleline(&mut name).changed() {
        app.rename_selected_track(name);
    }

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Color");
        if ui.small_button("<").clicked() {
            app.step_track_color(-1);
        }
        ui.colored_label(util::track_accent(track.color), track.color.label());
        if ui.small_button(">").clicked() {
            app.step_track_color(1);
        }
    });

    ui.horizontal(|ui| {
        let mute_fill = if track.muted { Color32::from_rgb(160, 74, 74) } else { Color32::from_rgb(50, 54, 66) };
        let solo_fill = if track.solo { Color32::from_rgb(172, 130, 52) } else { Color32::from_rgb(50, 54, 66) };
        if ui.add(egui::Button::new("Mute").fill(mute_fill)).clicked() {
            app.toggle_selected_track_mute();
        }
        if ui.add(egui::Button::new("Solo").fill(solo_fill)).clicked() {
            app.toggle_selected_track_solo();
        }
    });

    ui.add_space(8.0);
    if track.kind == TrackKind::Drum {
        if ui.button("Sequence chunks").clicked() {
            app.begin_new_sequence_chunk();
        }
    } else {
        let record_active = app.selected_track_is_recording();
        let action_label = if record_active { "Stop" } else { track_action_label(&track) };
        if ui.button(action_label).clicked() {
            if record_active {
                app.stop_playback();
            } else {
                app.arm_selected_track();
            }
        }

        let mut overwrite = track.overwrite;
        if ui.checkbox(&mut overwrite, "Overwrite existing clips").changed() {
            app.set_selected_track_overwrite(overwrite);
        }

        let mut count_in_enabled = track.count_in_enabled;
        if ui.checkbox(&mut count_in_enabled, "Count in").changed() {
            app.set_selected_track_count_in_enabled(count_in_enabled);
        }
        if count_in_enabled {
            ui.horizontal(|ui| {
                ui.label("Count-in beats");
                if ui.small_button("-").clicked() {
                    app.adjust_selected_track_count_in_beats(-1);
                }
                ui.label(track.count_in_beats.to_string());
                if ui.small_button("+").clicked() {
                    app.adjust_selected_track_count_in_beats(1);
                }
            });
        }
    }

    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);

    ui.label(RichText::new("Track Settings").strong());
    if track.kind == TrackKind::Audio {
        ui.horizontal(|ui| {
            ui.label("Input");
            if ui.small_button("<").clicked() {
                let _ = app.step_selected_track_input(-1);
            }
            ui.label(track.input_device.as_deref().unwrap_or("none"));
            if ui.small_button(">").clicked() {
                let _ = app.step_selected_track_input(1);
            }
        });
    }

    let mut volume = track.volume;
    if ui.add(egui::Slider::new(&mut volume, 0.0..=1.0).text("Track volume")).changed() {
        app.set_track_volume(volume);
    }
    ui.horizontal(|ui| {
        if ui.small_button("- vol").clicked() {
            app.adjust_track_volume(-VOLUME_STEP);
        }
        if ui.small_button("+ vol").clicked() {
            app.adjust_track_volume(VOLUME_STEP);
        }
    });

    ui.add_space(8.0);
    if ui
        .add_enabled(app.selected_track != 0, egui::Button::new(RichText::new("Delete track").color(Color32::from_rgb(220, 118, 118))))
        .clicked()
    {
        app.remove_selected_track();
    }

    if let Some(clip) = app.selected_clip_ref().cloned() {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        ui.label(RichText::new("Selected Chunk").strong());
        ui.label(format!("{}  {:.2} -> {:.2}", clip.title, clip.start_beat, clip.end_beat()));
        let mut loop_count = clip.loop_count;
        if ui.add(egui::DragValue::new(&mut loop_count).range(0.25..=64.0).speed(0.1).prefix("Loop ")).changed() {
            app.set_selected_clip_loop_count(loop_count);
        }
        if ui.button(RichText::new("Delete chunk").color(Color32::from_rgb(220, 118, 118))).clicked() {
            app.delete_selected_clip();
        }
        if ui.button("Duplicate").clicked() {
            app.duplicate_selected_clip();
        }
        if clip.is_drum_sequence() && ui.button("Edit sequence").clicked() {
            app.edit_selected_sequence_chunk();
        }
        if ui.button("Split at playhead").clicked() {
            app.split_selected_clip();
        }
        ui.horizontal(|ui| {
            if ui.small_button("Left").clicked() {
                app.nudge_selected_clip(-0.25, false);
            }
            if ui.small_button("Snap Left").clicked() {
                app.nudge_selected_clip(-1.0, true);
            }
            if ui.small_button("Right").clicked() {
                app.nudge_selected_clip(0.25, false);
            }
            if ui.small_button("Snap Right").clicked() {
                app.nudge_selected_clip(1.0, true);
            }
        });
    }
}
