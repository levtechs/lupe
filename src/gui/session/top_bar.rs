use eframe::egui::{self, Align, Button, Context, Layout, RichText, Stroke, ViewportCommand};

use crate::app::{App, Screen};
use crate::gui::theme;
use crate::project::MetronomeMode;

pub fn show(ctx: &Context, app: &mut App) {
    egui::TopBottomPanel::top("top_bar")
        .frame(egui::Frame::none().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12.0, 8.0)))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("lupe").size(21.0).strong());
                ui.separator();
                file_menu(ui, app);

                if ui.button("Transport").clicked() {
                    app.transport_popup_open = true;
                }

                let (play_label, play_fill, play_stroke) = if app.is_playing {
                    ("Stop", theme::DANGER.gamma_multiply(0.55), theme::DANGER)
                } else {
                    ("Play", theme::SUCCESS.gamma_multiply(0.5), theme::SUCCESS)
                };
                if ui
                    .add(Button::new(play_label).fill(play_fill).stroke(Stroke::new(1.0, play_stroke)))
                    .clicked()
                {
                    app.toggle_playback();
                }

                let route_enabled = app.route_enabled();
                if ui
                    .add(
                        Button::new(if route_enabled { "Route on" } else { "Route off" })
                            .fill(if route_enabled {
                                theme::SUCCESS.gamma_multiply(0.45)
                            } else {
                                Color32::from_rgb(39, 43, 53)
                            })
                            .stroke(Stroke::new(1.0, if route_enabled { theme::SUCCESS } else { theme::BORDER })),
                    )
                    .clicked()
                {
                    let _ = app.toggle_route();
                }

                metronome_controls(ui, app);
                ui.separator();
                if ui.small_button("<").on_hover_text("Previous anchor").clicked() {
                    app.jump_playhead_to_previous_anchor();
                }
                ui.label(RichText::new(format!("{:.2}", app.playhead_beats)).monospace().strong());
                if ui.small_button(">").on_hover_text("Next anchor").clicked() {
                    app.jump_playhead_to_next_anchor();
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let project_name = app.project.as_ref().map(|project| project.name.as_str()).unwrap_or("No project");
                    ui.label(RichText::new(project_name).strong());
                    if let Some(beats_left) = app.pending_record_beats() {
                        ui.label(RichText::new(format!("COUNT-IN {:.1}", beats_left)).color(theme::WARNING).small().strong());
                    } else if !app.status.is_empty() {
                        ui.label(RichText::new(&app.status).small().color(theme::MUTED));
                    }
                });
            });
        });
}

use eframe::egui::Color32;

fn file_menu(ui: &mut egui::Ui, app: &mut App) {
    ui.menu_button("File", |ui| {
        if ui.button("New").clicked() {
            let _ = app.stop_router();
            let _ = app.create_new_project();
            ui.close_menu();
        }
        if ui.button("Save").clicked() {
            let _ = app.save_project();
            ui.close_menu();
        }
        if ui.button("Rename project").clicked() {
            app.open_rename_project_popup();
            ui.close_menu();
        }
        if ui.button("Delete project").clicked() {
            app.delete_current_project();
            ui.close_menu();
        }
        ui.separator();
        if ui.button("Open project").clicked() {
            let _ = app.stop_router();
            app.screen = Screen::MainMenu;
            app.recent_projects = crate::project::list_recent_projects().unwrap_or_default();
            ui.close_menu();
        }
        if ui.button("Quit").clicked() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
    });
}

fn metronome_controls(ui: &mut egui::Ui, app: &mut App) {
    let mode = app
        .project
        .as_ref()
        .map(|project| project.metronome.mode)
        .unwrap_or(MetronomeMode::Off);
    let (fill, stroke) = match mode {
        MetronomeMode::Off => (Color32::from_rgb(39, 43, 53), theme::BORDER),
        MetronomeMode::Always => (theme::ACCENT.gamma_multiply(0.42), theme::ACCENT),
        MetronomeMode::On => (theme::SUCCESS.gamma_multiply(0.45), theme::SUCCESS),
    };
    if ui.add(Button::new("Metronome").fill(fill).stroke(Stroke::new(1.0, stroke))).clicked() {
        match mode {
            MetronomeMode::Off => app.cycle_metronome_mode(2),
            MetronomeMode::Always | MetronomeMode::On => app.cycle_metronome_mode(-1),
        }
    }
    if ui.small_button("Settings").clicked() {
        app.metronome_popup_open = true;
    }
}
