use eframe::egui::{self, RichText, Ui};

use crate::app::App;

pub fn panel(ui: &mut Ui, app: &mut App) {
    ui.group(|ui| {
        ui.set_max_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Transport").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(project) = app.project.as_mut() {
                    let count_in = egui::Checkbox::new(&mut project.transport.count_in_enabled, "Count-in");
                    if ui.add(count_in).changed() {
                        project.dirty = true;
                    }
                    let metro = egui::Checkbox::new(&mut project.transport.metronome_enabled, "Metronome");
                    if ui.add(metro).changed() {
                        project.dirty = true;
                    }
                }
            });
        });
        ui.add_space(4.0);
        let (bpm, beats, loop_bars) = app
            .project
            .as_ref()
            .map(|p| (p.transport.bpm, p.transport.beats_per_bar, p.transport.loop_bars))
            .unwrap_or((120, 4, 4));
        ui.horizontal(|ui| {
            ui.label("BPM");
            if ui.small_button("−").clicked() {
                app.adjust_bpm(-1);
            }
            ui.label(RichText::new(format!("{bpm}")).monospace());
            if ui.small_button("+").clicked() {
                app.adjust_bpm(1);
            }
            ui.add_space(16.0);
            ui.label("Beats / bar");
            if ui.small_button("−").clicked() {
                app.adjust_beats(-1);
            }
            ui.label(RichText::new(format!("{beats}")).monospace());
            if ui.small_button("+").clicked() {
                app.adjust_beats(1);
            }
            ui.add_space(16.0);
            ui.label("Loop bars");
            if ui.small_button("−").clicked() {
                app.adjust_loop_bars(-1);
            }
            ui.label(RichText::new(format!("{loop_bars}")).monospace());
            if ui.small_button("+").clicked() {
                app.adjust_loop_bars(1);
            }
        });
    });
}
