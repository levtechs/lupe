use eframe::egui::{self, Context, RichText};

use crate::app::App;

pub fn show(ctx: &Context, app: &mut App) {
    if !app.metronome_popup_open {
        return;
    }

    let mut open = true;
    egui::Window::new("Metronome settings")
        .open(&mut open)
        .resizable(false)
        .default_width(320.0)
        .show(ctx, |ui| {
            let Some(project) = app.project.as_ref() else {
                return;
            };
            ui.label(RichText::new(format!("Mode: {}", project.metronome.mode.label())).strong());
            ui.add_space(8.0);

            let rows = [
                ("Tempo", format!("{} bpm", project.metronome.sound.bpm.round() as i32)),
                ("Accent", format!("{} beats", project.metronome.sound.accent_every)),
                ("Tone", format!("{} hz", project.metronome.sound.tone_hz.round() as i32)),
                ("Volume", format!("{}%", (project.metronome.sound.volume * 100.0).round() as i32)),
            ];

            for (index, (label, value)) in rows.into_iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui.small_button("-").clicked() {
                        app.adjust_metronome_param(index, -1);
                    }
                    ui.label(value);
                    if ui.small_button("+").clicked() {
                        app.adjust_metronome_param(index, 1);
                    }
                });
            }
        });

    app.metronome_popup_open = open;
}
