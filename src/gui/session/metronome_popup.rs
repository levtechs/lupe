use eframe::egui::{self, Context, RichText};

use crate::{app::App, gui::theme};

pub fn show(ctx: &Context, app: &mut App) {
    if !app.metronome_popup_open {
        return;
    }

    let mut open = true;
    egui::Window::new("Metronome settings")
        .open(&mut open)
        .resizable(false)
        .default_width(320.0)
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            let Some(project) = app.project.as_ref() else {
                return;
            };
            ui.label(RichText::new("CLICK SOUND").small().strong().color(theme::MUTED));
            ui.label(RichText::new(format!("{} mode", project.metronome.mode.label())).strong().size(17.0));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Tempo");
                ui.label(RichText::new(format!("{} bpm", project.transport.bpm)).strong());
                ui.label(RichText::new("from transport").small().weak());
            });
            ui.add_space(6.0);

            let rows = [
                ("Accent", format!("{} beats", project.metronome.sound.accent_every)),
                ("Tone", format!("{} hz", project.metronome.sound.tone_hz.round() as i32)),
                ("Volume", format!("{}%", (project.metronome.sound.volume * 100.0).round() as i32)),
            ];

            theme::card_frame().show(ui, |ui| for (index, (label, value)) in rows.into_iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui.small_button("-").clicked() {
                        app.adjust_metronome_param(index + 1, -1);
                    }
                    ui.label(value);
                    if ui.small_button("+").clicked() {
                        app.adjust_metronome_param(index + 1, 1);
                    }
                });
            });
        });

    app.metronome_popup_open = open;
}
