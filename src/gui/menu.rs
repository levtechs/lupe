use eframe::egui::{self, Align, Context, Layout, RichText, Stroke, Vec2, ViewportCommand};

use crate::app::{short_path, App};
use crate::gui::theme;

pub fn show(ctx: &Context, app: &mut App) {
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(theme::BACKGROUND))
        .show(ctx, |ui| {
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                ui.add_space(54.0);
                ui.label(RichText::new("lupe").size(36.0).strong());
                ui.label(RichText::new("A quiet place for loud ideas").size(13.0).color(theme::MUTED));
                ui.add_space(34.0);

                ui.allocate_ui_with_layout(Vec2::new(440.0, ui.available_height()), Layout::top_down(Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Projects").size(18.0).strong());
                            ui.label(RichText::new("Open a session or start fresh").small().color(theme::MUTED));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("New project").strong())
                                        .fill(theme::ACCENT.gamma_multiply(0.65))
                                        .stroke(Stroke::new(1.0, theme::ACCENT)),
                                )
                                .clicked()
                            {
                                if let Err(err) = app.create_new_project() {
                                    app.status = format!("{err:#}");
                                }
                            }
                        });
                    });

                    ui.add_space(18.0);
                    ui.label(RichText::new("RECENT").size(10.0).strong().color(theme::MUTED));
                    ui.add_space(5.0);

                    let entries: Vec<(String, std::path::PathBuf)> = app
                        .recent_projects
                        .iter()
                        .map(|project| (project.name.clone(), project.path.clone()))
                        .collect();
                    egui::ScrollArea::vertical().max_height(290.0).show(ui, |ui| {
                        if entries.is_empty() {
                            ui.add_space(12.0);
                            ui.label(RichText::new("No saved projects yet").color(theme::MUTED));
                            ui.label(RichText::new("Your projects will appear here.").small().color(theme::MUTED));
                        }

                        for (name, path) in &entries {
                            let row = egui::Frame::none()
                                .fill(theme::CARD)
                                .stroke(Stroke::new(1.0, theme::BORDER))
                                .rounding(6.0)
                                .inner_margin(egui::Margin::symmetric(10.0, 7.0))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(name).strong().size(13.0));
                                            ui.label(RichText::new(short_path(path)).small().color(theme::MUTED));
                                        });
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            ui.label(RichText::new("Open").small().color(theme::ACCENT));
                                        });
                                    });
                                });
                            if ui
                                .interact(row.response.rect, ui.id().with(("recent_project", path)), egui::Sense::click())
                                .clicked()
                            {
                                if let Err(err) = app.open_project(path.clone()) {
                                    app.status = format!("{err:#}");
                                }
                            }
                            ui.add_space(5.0);
                        }
                    });

                    if !app.status.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new(&app.status).small().color(theme::MUTED));
                    }

                    ui.add_space(10.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                    });
                });
            });
        });
}
