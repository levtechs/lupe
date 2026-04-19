use eframe::egui::{RichText, Ui};

use crate::app::App;
use crate::gui::meters;

pub fn panel(ui: &mut Ui, app: &mut App) {
    ui.group(|ui| {
        ui.label(RichText::new("Inspector").strong());
        ui.add_space(8.0);

        if let Some(track) = app.selected_track_ref() {
            ui.label(format!("Track: {}", track.name));
            ui.label(RichText::new(format!("Type: {}", track.kind.label())).weak());
            ui.label(RichText::new(format!("Color: {}", track.color.label())).weak());
            ui.label(format!("Input: {}", track.input_device.as_deref().unwrap_or("none")));
            if let Some(project) = app.project.as_ref() {
                ui.label(format!("Output: {}", project.output_device.as_deref().unwrap_or("none")));
            }
            ui.label(format!("Pedals: {}", track.pedals.len()));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                meters::peak_strip(ui, "In", app.input_meter);
                ui.add_space(12.0);
                meters::peak_strip(ui, "Out", app.output_meter);
            });
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Latency: {}", app.latency_label)).small());
        } else {
            ui.label("No track");
        }

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
        ui.label(RichText::new("Status").strong());
        ui.label(RichText::new(&app.status).small().italics());
    });
}
