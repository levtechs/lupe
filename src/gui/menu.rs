use eframe::egui::{self, Context, RichText, ViewportCommand};

use crate::app::{short_path, App};

pub fn show(ctx: &Context, app: &mut App) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(48.0);
            ui.label(RichText::new("lupe").size(36.0).strong());
            ui.add_space(4.0);
            ui.label(RichText::new("Jam projects").weak());
            ui.add_space(40.0);

            if ui.button(RichText::new("  New project  ").strong()).clicked() {
                if let Err(err) = app.create_new_project() {
                    app.status = format!("{err:#}");
                }
            }
            ui.add_space(8.0);
            if ui.button("  Quit  ").clicked() {
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }

            ui.add_space(36.0);
            ui.separator();
            ui.add_space(12.0);
            ui.label(RichText::new("Recent").strong());
            ui.add_space(8.0);

            if app.recent_projects.is_empty() {
                ui.label(RichText::new("No saved projects yet.").weak());
            } else {
                let entries: Vec<(String, std::path::PathBuf)> = app
                    .recent_projects
                    .iter()
                    .map(|s| (s.name.clone(), s.path.clone()))
                    .collect();
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (name, path) in &entries {
                        let label = format!("{name}  —  {}", short_path(path));
                        if ui.selectable_label(false, label).clicked() {
                            if let Err(err) = app.open_project(path.clone()) {
                                app.status = format!("{err:#}");
                            }
                        }
                    }
                });
            }

            ui.add_space(24.0);
            ui.label(RichText::new(&app.status).small().italics());
        });
    });
}
