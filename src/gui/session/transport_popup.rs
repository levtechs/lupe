use eframe::egui::{self, Context};

use crate::app::App;

pub fn show(ctx: &Context, app: &mut App) {
    if !app.transport_popup_open {
        return;
    }

    let mut open = true;
    egui::Window::new("Transport")
        .open(&mut open)
        .resizable(false)
        .default_width(320.0)
        .show(ctx, |ui| {
            let Some(project) = app.project.as_ref() else {
                return;
            };

            let mut bpm = project.transport.bpm;
            let mut beats_per_bar = project.transport.beats_per_bar;
            let mut beat_unit = project.transport.beat_unit;
            let loop_enabled = project.transport.loop_enabled;
            let loop_bars = project.transport.loop_bars;

            ui.horizontal(|ui| {
                ui.label("Loop bars");
                if ui.small_button("-").clicked() {
                    app.adjust_loop_bars(-1);
                }
                ui.label(loop_bars.to_string());
                if ui.small_button("+").clicked() {
                    app.adjust_loop_bars(1);
                }
            });

            let mut loop_toggle = loop_enabled;
            if ui.checkbox(&mut loop_toggle, "Loop enabled").changed() {
                app.toggle_loop_enabled();
            }

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("BPM");
                if ui.small_button("-").clicked() {
                    app.adjust_bpm(-1);
                    bpm = bpm.saturating_sub(2);
                }
                if ui.add(egui::DragValue::new(&mut bpm).range(40..=240)).changed() {
                    app.set_bpm(bpm);
                }
                if ui.small_button("+").clicked() {
                    app.adjust_bpm(1);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Beats per bar");
                if ui.small_button("-").clicked() {
                    app.adjust_beats(-1);
                    beats_per_bar = beats_per_bar.saturating_sub(1);
                }
                if ui.add(egui::DragValue::new(&mut beats_per_bar).range(1..=12)).changed() {
                    app.set_beats_per_bar(beats_per_bar);
                }
                if ui.small_button("+").clicked() {
                    app.adjust_beats(1);
                }
            });

            ui.horizontal(|ui| {
                ui.label("Beat unit");
                if ui.small_button("-").clicked() {
                    app.adjust_beat_unit(-1);
                    beat_unit = beat_unit.saturating_sub(1);
                }
                ui.add(egui::DragValue::new(&mut beat_unit).range(1..=16));
                if ui.small_button("+").clicked() {
                    app.adjust_beat_unit(1);
                }
            });
        });

    app.transport_popup_open = open;
}
