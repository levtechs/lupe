use eframe::egui::{self, Context};

use crate::app::{App, VOLUME_STEP};

pub fn show(ctx: &Context, app: &mut App) {
    if !app.input_popup_open {
        return;
    }

    let mut open = true;
    egui::Window::new("Audio devices")
        .open(&mut open)
        .resizable(false)
        .default_width(320.0)
        .show(ctx, |ui| {
            let (input_label, output_label, input_volume) = match app.project.as_ref() {
                Some(project) => (
                    project.input.device.clone().unwrap_or_else(|| "none".to_string()),
                    project.output_device.clone().unwrap_or_else(|| "none".to_string()),
                    project.input.volume,
                ),
                None => return,
            };

            ui.label("Input");
            ui.horizontal(|ui| {
                ui.label("Device");
                if ui.small_button("<").clicked() {
                    let _ = app.step_input_device(-1);
                }
                ui.label(input_label);
                if ui.small_button(">").clicked() {
                    let _ = app.step_input_device(1);
                }
            });

            let mut volume = input_volume;
            if ui.add(egui::Slider::new(&mut volume, 0.0..=1.0).text("Input volume")).changed() {
                let _ = app.set_input_volume(volume);
            }
            ui.horizontal(|ui| {
                if ui.small_button("- vol").clicked() {
                    let _ = app.adjust_input_volume(-VOLUME_STEP);
                }
                if ui.small_button("+ vol").clicked() {
                    let _ = app.adjust_input_volume(VOLUME_STEP);
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            ui.label("Output");
            ui.horizontal(|ui| {
                ui.label("Device");
                if ui.small_button("<").clicked() {
                    let _ = app.step_output_device(-1);
                }
                ui.label(output_label);
                if ui.small_button(">").clicked() {
                    let _ = app.step_output_device(1);
                }
            });

            ui.add_space(8.0);
            ui.label("Routing uses the selected input, global pedalboard, and selected output.");
        });

    app.input_popup_open = open;
}
