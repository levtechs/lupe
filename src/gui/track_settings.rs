use eframe::egui::{self, Context, RichText};

use crate::app::{App, VOLUME_STEP};
use crate::pedals::PedalKind;

pub fn modal(ctx: &Context, app: &mut App) {
    if app.settings.is_none() {
        return;
    }

    let mut open = true;
    let window = egui::Window::new("Track settings").open(&mut open).default_width(520.0);

    window.show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Close").strong()).clicked() {
                app.close_track_settings();
            }
        });
        ui.add_space(8.0);

        {
            let name_changed = {
                let Some(track) = app.selected_track_mut() else {
                    ui.label("No track");
                    return;
                };
                let response = ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut track.name)
                });
                response.inner.changed()
            };
            if name_changed {
                app.mark_dirty();
            }
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Color");
            if ui.button("◀").clicked() {
                let _ = app.step_track_color(-1);
            }
            if let Some(track) = app.selected_track_ref() {
                ui.label(RichText::new(track.color.label()).monospace());
            }
            if ui.button("▶").clicked() {
                let _ = app.step_track_color(1);
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Input");
            if ui.button("◀").clicked() {
                let _ = app.step_track_input(-1);
            }
            let input_lbl = app
                .selected_track_ref()
                .and_then(|t| t.input_device.clone())
                .unwrap_or_else(|| "No input".into());
            ui.add(egui::Label::new(RichText::new(input_lbl).monospace()).truncate());
            if ui.button("▶").clicked() {
                let _ = app.step_track_input(1);
            }
        });

        ui.add_space(8.0);
        let mut vol = app.selected_track_ref().map(|t| t.volume).unwrap_or(0.0);
        if ui
            .add(egui::Slider::new(&mut vol, 0.0..=1.0).text("Track volume"))
            .changed()
        {
            let _ = app.set_track_volume(vol);
        }

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("+ Equalizer").clicked() {
                let _ = app.add_selected_track_pedal(PedalKind::Equalizer);
            }
            if ui.button("+ Reverb").clicked() {
                let _ = app.add_selected_track_pedal(PedalKind::Reverb);
            }
        });

        ui.add_space(10.0);
        ui.label(RichText::new("Pedals").strong());
        ui.add_space(4.0);

        let pedal_count = app.selected_track_ref().map(|t| t.pedals.len()).unwrap_or(0);
        for index in 0..pedal_count {
            let selected = app.settings.as_ref().and_then(|s| s.pedal_index) == Some(index);
            let row_label = {
                let t = app.selected_track_ref().unwrap();
                format!("{}  {}", t.pedals[index].label(), t.pedals[index].summary())
            };

            ui.horizontal(|ui| {
                if ui
                    .add(egui::SelectableLabel::new(selected, RichText::new(row_label).monospace()))
                    .clicked()
                {
                    app.select_modal_pedal(index);
                }
                let enabled = app.selected_track_ref().unwrap().pedals[index].enabled();
                let on_label = if enabled { "On" } else { "Off" };
                if ui.button(on_label).clicked() {
                    app.select_modal_pedal(index);
                    let _ = app.toggle_selected_track_pedal();
                }
                if ui.button("Up").clicked() {
                    app.select_modal_pedal(index);
                    let _ = app.move_selected_track_pedal(-1);
                }
                if ui.button("Down").clicked() {
                    app.select_modal_pedal(index);
                    let _ = app.move_selected_track_pedal(1);
                }
                if ui.button("Remove").clicked() {
                    app.select_modal_pedal(index);
                    let _ = app.remove_selected_track_pedal();
                }
            });
        }

        ui.add_space(10.0);
        ui.label(RichText::new("Parameters").strong());
        ui.add_space(4.0);

        let param_count = app.selected_modal_pedal().map(|p| p.param_count()).unwrap_or(0);
        for pi in 0..param_count {
            let selected = app.settings.as_ref().map(|s| s.param_index == pi).unwrap_or(false);
            let label = app
                .selected_modal_pedal()
                .map(|p| format!("{}: {}", p.param_name(pi), p.param_value(pi)))
                .unwrap_or_default();

            ui.horizontal(|ui| {
                if ui.small_button("−").clicked() {
                    app.select_modal_param(pi);
                    let _ = app.adjust_selected_track_param(-1);
                }
                if ui
                    .add(egui::SelectableLabel::new(selected, RichText::new(label).monospace()))
                    .clicked()
                {
                    app.select_modal_param(pi);
                }
                if ui.small_button("+").clicked() {
                    app.select_modal_param(pi);
                    let _ = app.adjust_selected_track_param(1);
                }
            });
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Nudge volume down").clicked() {
                let _ = app.adjust_track_volume(-VOLUME_STEP);
            }
            if ui.button("Nudge volume up").clicked() {
                let _ = app.adjust_track_volume(VOLUME_STEP);
            }
        });
    });

    if !open {
        app.close_track_settings();
    }
}
