use eframe::egui::{self, Color32, Context, RichText};

use crate::app::{sequence_step_count, App};

pub fn show(ctx: &Context, app: &mut App) {
    if !app.sequencer_popup_open {
        return;
    }

    let Some((beats_per_bar, beat_unit)) = app
        .project
        .as_ref()
        .map(|project| (project.transport.beats_per_bar, project.transport.beat_unit))
    else {
        return;
    };
    let Some(sequence) = app.sequence().cloned() else {
        return;
    };
    let total_steps = sequence_step_count(&sequence, beats_per_bar);

    let mut open = true;
    egui::Window::new("Drum sequencer")
        .open(&mut open)
        .default_width(760.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Measures").strong());
                if ui.small_button("-").clicked() {
                    app.adjust_selected_sequence_measures(-1);
                }
                ui.label(sequence.measures.to_string());
                if ui.small_button("+").clicked() {
                    app.adjust_selected_sequence_measures(1);
                }
                ui.separator();
                ui.label(RichText::new("Columns").strong());
                if ui.small_button("-").clicked() {
                    app.adjust_selected_sequence_subdivision(-1);
                }
                ui.label(sequence.subdivision.label());
                if ui.small_button("+").clicked() {
                    app.adjust_selected_sequence_subdivision(1);
                }
                ui.separator();
                ui.label(format!("Time signature: {} / {}", beats_per_bar, beat_unit));
            });

            ui.add_space(10.0);
            egui::ScrollArea::both().show(ui, |ui| {
                for (lane_index, lane) in sequence.lanes.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add_sized([90.0, 24.0], egui::Label::new(lane.name.clone()));
                        for step_index in 0..total_steps {
                            let active = lane.steps.get(step_index).copied().unwrap_or(false);
                            let bar_step = (step_index as u32) % (beats_per_bar * sequence.subdivision.steps_per_beat()) == 0;
                            let fill = if active {
                                Color32::from_rgb(86, 148, 236)
                            } else if bar_step {
                                Color32::from_rgb(58, 62, 78)
                            } else {
                                Color32::from_rgb(40, 44, 56)
                            };
                            if ui.add_sized([22.0, 22.0], egui::Button::new(" ").fill(fill)).clicked() {
                                app.toggle_sequence_step(lane_index, step_index);
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
            });
        });

    app.sequencer_popup_open = open;
}
