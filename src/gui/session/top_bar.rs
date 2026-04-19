use eframe::egui::{self, Button, Context, RichText, ViewportCommand};

use crate::app::{App, Screen};

pub fn show(ctx: &Context, app: &mut App) {
    egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("lupe").size(22.0).strong());

            ui.menu_button("File", |ui| {
                if ui.button("Save").clicked() {
                    let _ = app.save_project();
                    ui.close_menu();
                }
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

            if ui.button("Transport").clicked() {
                app.transport_popup_open = true;
            }

            let play_label = if app.is_playing { "Stop" } else { "Play" };
            let play_fill = if app.is_playing {
                egui::Color32::from_rgb(150, 70, 70)
            } else {
                egui::Color32::from_rgb(65, 120, 85)
            };
            if ui.add(Button::new(play_label).fill(play_fill)).clicked() {
                app.toggle_playback();
            }

            let route_fill = if app.route_enabled() {
                egui::Color32::from_rgb(72, 108, 180)
            } else {
                egui::Color32::from_rgb(48, 52, 64)
            };
            if ui.add(Button::new("Route").fill(route_fill)).clicked() {
                let _ = app.toggle_route();
            }

            ui.horizontal(|ui| {
                ui.label("Metronome");
                if ui.small_button("<").clicked() {
                    app.cycle_metronome_mode(-1);
                }
                let label = app
                    .project
                    .as_ref()
                    .map(|project| project.metronome.mode.label())
                    .unwrap_or("Off");
                ui.label(RichText::new(label).strong());
                if ui.small_button(">").clicked() {
                    app.cycle_metronome_mode(1);
                }
            });

            if ui.button("Metronome settings").clicked() {
                app.metronome_popup_open = true;
            }

            ui.separator();
            ui.label(format!("Playhead {:.2}", app.playhead_beats));
            if let Some(beats_left) = app.pending_record_beats() {
                ui.label(RichText::new(format!("Count-in {:.1}", beats_left)).italics());
            }
            ui.label(RichText::new(&app.status).small().weak());
        });
        ui.add_space(4.0);
    });
}
