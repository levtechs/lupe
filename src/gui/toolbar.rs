use eframe::egui::{self, RichText, Ui};

use crate::app::App;

pub fn panel(ui: &mut Ui, app: &mut App) {
    ui.group(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Output").strong());
            if ui.button("◀").clicked() {
                let _ = app.step_output(-1);
            }
            let label = app
                .project
                .as_ref()
                .and_then(|p| p.output_device.as_deref())
                .unwrap_or("No output");
            ui.add(egui::Label::new(RichText::new(label).monospace()).truncate());
            if ui.button("▶").clicked() {
                let _ = app.step_output(1);
            }

            ui.separator();

            let route_label = if app.router.is_some() {
                RichText::new("Stop").color(egui::Color32::from_rgb(255, 140, 140))
            } else {
                RichText::new("Route").strong()
            };
            if ui.button(route_label).clicked() {
                let _ = app.toggle_router_for_selected_track();
            }

            if ui.button(RichText::new("Save").strong()).clicked() {
                let _ = app.save_project();
            }

            if ui.button("Menu").clicked() {
                let _ = app.stop_router();
                app.screen = crate::app::Screen::MainMenu;
                app.recent_projects = crate::project::list_recent_projects().unwrap_or_default();
            }

            if ui.button(RichText::new("+ Track").strong()).clicked() {
                let _ = app.add_audio_track();
            }

            if ui.button("Rescan devices").clicked() {
                let _ = app.refresh_devices();
            }
        });
    });
}
