use eframe::egui::{self, Context};

use crate::{app::{App, VOLUME_STEP}, gui::theme};

pub fn show(ctx: &Context, app: &mut App) {
    if !app.input_popup_open {
        return;
    }

    let mut open = true;
    egui::Window::new("Audio devices")
        .open(&mut open)
        .resizable(false)
        .default_width(360.0)
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            let (input_label, output_label, input_volume) = match app.project.as_ref() {
                Some(project) => (
                    project.input.device.clone().unwrap_or_else(|| "none".to_string()),
                    project.output_device.clone().unwrap_or_else(|| "none".to_string()),
                    project.input.volume,
                ),
                None => return,
            };

            ui.label(egui::RichText::new("INPUT").small().strong().color(theme::MUTED));
            theme::card_frame().show(ui, |ui| {
            egui::ComboBox::from_id_salt("input_device_dropdown")
                .width(ui.available_width())
                .selected_text(input_label)
                .show_ui(ui, |ui| {
                    ui.label("Input device");
                    if ui.selectable_label(false, "none").clicked() {
                        let _ = app.set_input_device(None);
                    }
                    for device in app.input_devices.clone() {
                        if ui.selectable_label(app.project.as_ref().and_then(|project| project.input.device.as_ref()) == Some(&device), &device).clicked() {
                            let _ = app.set_input_device(Some(device));
                        }
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
            });

            ui.add_space(8.0);

            ui.label(egui::RichText::new("OUTPUT").small().strong().color(theme::MUTED));
            theme::card_frame().show(ui, |ui| {
            egui::ComboBox::from_id_salt("output_device_dropdown")
                .width(ui.available_width())
                .selected_text(output_label)
                .show_ui(ui, |ui| {
                    ui.label("Output device");
                    if ui.selectable_label(false, "none").clicked() {
                        let _ = app.set_output_device(None);
                    }
                    for device in app.output_devices.clone() {
                        if ui.selectable_label(app.project.as_ref().and_then(|project| project.output_device.as_ref()) == Some(&device), &device).clicked() {
                            let _ = app.set_output_device(Some(device));
                        }
                    }
                });
            });

            ui.add_space(8.0);
            ui.label(egui::RichText::new("The live route passes through the pedalboard before reaching the selected output.").small().color(theme::MUTED));
        });

    app.input_popup_open = open;
}
