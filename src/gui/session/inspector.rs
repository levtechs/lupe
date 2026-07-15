use eframe::egui::{self, Align, Color32, Layout, RichText, Ui};

use crate::app::{track_action_label, App, VOLUME_STEP};
use crate::gui::{theme, util};
use crate::project::TrackKind;

pub fn show(ui: &mut Ui, app: &mut App) {
    egui::ScrollArea::vertical()
        .id_salt("inspector_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| show_content(ui, app));
}

fn show_content(ui: &mut Ui, app: &mut App) {
    ui.label(RichText::new("Inspector").strong().size(19.0));
    ui.label(RichText::new("Selected track and chunk settings").small().color(theme::MUTED));
    ui.add_space(12.0);

    let Some(track) = app.selected_track_ref().cloned() else {
        theme::card_frame().show(ui, |ui| {
            ui.label(RichText::new("No track selected").color(theme::MUTED));
            ui.label(RichText::new("Choose a track in the timeline to edit it.").small().color(theme::MUTED));
        });
        return;
    };
    let accent = util::track_accent(track.color);

    section_card(ui, "TRACK", accent, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{:02}", app.selected_track + 1)).monospace().strong().color(accent));
            let mut name = track.name.clone();
            if ui.add_sized([ui.available_width(), 26.0], egui::TextEdit::singleline(&mut name)).changed() {
                app.rename_selected_track(name);
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("COLOR").small().color(theme::MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button(">").clicked() {
                    app.step_track_color(1);
                }
                ui.label(RichText::new(track.color.label()).strong().color(accent));
                if ui.small_button("<").clicked() {
                    app.step_track_color(-1);
                }
            });
        });
    });

    ui.add_space(8.0);
    section_card(ui, "PERFORMANCE", accent, |ui| {
        ui.columns(2, |columns| {
            let mute_fill = if track.muted { theme::DANGER.gamma_multiply(0.55) } else { Color32::from_rgb(39, 43, 53) };
            if columns[0].add_sized([columns[0].available_width(), 28.0], egui::Button::new("Mute").fill(mute_fill)).clicked() {
                app.toggle_selected_track_mute();
            }
            let solo_fill = if track.solo { theme::WARNING.gamma_multiply(0.5) } else { Color32::from_rgb(39, 43, 53) };
            if columns[1].add_sized([columns[1].available_width(), 28.0], egui::Button::new("Solo").fill(solo_fill)).clicked() {
                app.toggle_selected_track_solo();
            }
        });
        ui.add_space(4.0);

        if track.kind == TrackKind::Drum {
            if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("Sequence chunks").fill(accent.gamma_multiply(0.45))).clicked() {
                app.begin_new_sequence_chunk();
            }
        } else {
            let record_active = app.selected_track_is_recording();
            let action_label = if record_active { "Stop recording" } else { track_action_label(&track) };
            let fill = if record_active { theme::DANGER.gamma_multiply(0.55) } else { accent.gamma_multiply(0.45) };
            if ui.add_sized([ui.available_width(), 30.0], egui::Button::new(action_label).fill(fill)).clicked() {
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
                let delta = stepper(ui, "Count-in beats", &track.count_in_beats.to_string());
                if delta != 0 {
                    app.adjust_selected_track_count_in_beats(delta);
                }
            }
        }
    });

    if track.kind == TrackKind::Drum {
        ui.add_space(8.0);
        section_card(ui, "HUMAN FEEL", accent, |ui| {
            ui.label(
                RichText::new("Applies subtle variation to every sequence chunk on this track.")
                    .small()
                    .color(theme::MUTED),
            );
            ui.add_space(4.0);
            let mut timing = track.drum_humanize.timing_ms;
            let mut dynamics = track.drum_humanize.velocity_variation * 100.0;
            let mut swing = track.drum_humanize.swing * 100.0;
            let mut feel = track.drum_humanize.feel_ms;
            let mut evolving = track.drum_humanize.evolving;
            let changed = ui
                .add(egui::Slider::new(&mut timing, 0.0..=30.0).text("Timing variation").suffix(" ms"))
                .changed()
                | ui
                    .add(egui::Slider::new(&mut dynamics, 0.0..=35.0).text("Dynamic variation").suffix("%"))
                    .changed()
                | ui.add(egui::Slider::new(&mut swing, 0.0..=100.0).text("Swing").suffix("%")).changed()
                | ui
                    .add(egui::Slider::new(&mut feel, -20.0..=20.0).text("Push / pull").suffix(" ms"))
                    .changed()
                | ui.checkbox(&mut evolving, "Change variation each loop").changed();
            if changed {
                app.set_selected_drum_humanize(timing, dynamics / 100.0, swing / 100.0, feel, evolving);
            }
            if ui.button("New variation").clicked() {
                app.reroll_selected_drum_humanize();
            }
        });
    }

    ui.add_space(8.0);
    section_card(ui, "AUDIO", accent, |ui| {
        if track.kind == TrackKind::Audio {
            let delta = stepper(ui, "Input", track.input_device.as_deref().unwrap_or("None"));
            if delta != 0 {
                let _ = app.step_selected_track_input(delta);
            }
            ui.add_space(4.0);
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new("VOLUME").small().color(theme::MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(format!("{}%", (track.volume * 100.0).round() as i32)).monospace().strong());
            });
        });
        let mut volume = track.volume;
        if ui.add(egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false)).changed() {
            app.set_track_volume(volume);
        }
        ui.columns(2, |columns| {
            if columns[0].button("Quieter").clicked() {
                app.adjust_track_volume(-VOLUME_STEP);
            }
            if columns[1].button("Louder").clicked() {
                app.adjust_track_volume(VOLUME_STEP);
            }
        });
    });

    if let Some(clip) = app.selected_clip_ref().cloned() {
        ui.add_space(8.0);
        section_card(ui, "SELECTED CHUNK", accent, |ui| {
            ui.label(RichText::new(&clip.title).strong());
            ui.label(
                RichText::new(format!("{:.2} - {:.2} beats", clip.start_beat, clip.end_beat()))
                    .small()
                    .monospace()
                    .color(theme::MUTED),
            );
            let mut loop_count = clip.loop_count;
            if ui.add(egui::DragValue::new(&mut loop_count).range(0.25..=64.0).speed(0.1).prefix("Loops  ")).changed() {
                app.set_selected_clip_loop_count(loop_count);
            }
            ui.columns(2, |columns| {
                if columns[0].button("Duplicate").clicked() {
                    app.duplicate_selected_clip();
                }
                if columns[1].button("Split").on_hover_text("Split at playhead").clicked() {
                    app.split_selected_clip();
                }
            });
            if clip.is_drum_sequence()
                && ui.add_sized([ui.available_width(), 27.0], egui::Button::new("Edit sequence")).clicked()
            {
                app.edit_selected_sequence_chunk();
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui.small_button("< 1/4").clicked() {
                    app.nudge_selected_clip(-0.25, false);
                }
                if ui.small_button("< 1").clicked() {
                    app.nudge_selected_clip(-1.0, true);
                }
                if ui.small_button("1 >").clicked() {
                    app.nudge_selected_clip(1.0, true);
                }
                if ui.small_button("1/4 >").clicked() {
                    app.nudge_selected_clip(0.25, false);
                }
            });
            if ui
                .add_sized(
                    [ui.available_width(), 27.0],
                    egui::Button::new(RichText::new("Delete chunk").color(theme::DANGER)),
                )
                .clicked()
            {
                app.delete_selected_clip();
            }
        });
    }

    ui.add_space(8.0);
    if ui
        .add_enabled(
            app.selected_track != 0,
            egui::Button::new(RichText::new("Delete track").color(theme::DANGER)).min_size(egui::vec2(ui.available_width(), 28.0)),
        )
        .clicked()
    {
        app.remove_selected_track();
    }
    ui.add_space(8.0);
}

fn section_card(ui: &mut Ui, title: &str, accent: Color32, add_contents: impl FnOnce(&mut Ui)) {
    theme::card_frame().show(ui, |ui| {
        ui.label(RichText::new(title).small().strong().color(accent));
        ui.add_space(6.0);
        add_contents(ui);
    });
}

fn stepper(ui: &mut Ui, label: &str, value: &str) -> i32 {
    let mut delta = 0;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).small().color(theme::MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.small_button("+").clicked() {
                delta = 1;
            }
            ui.label(RichText::new(value).monospace().small());
            if ui.small_button("-").clicked() {
                delta = -1;
            }
        });
    });
    delta
}
