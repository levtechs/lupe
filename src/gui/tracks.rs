use eframe::egui::{self, RichText, Stroke, Ui};

use crate::app::{track_action_label, App};
use crate::gui::util;
use crate::project::TrackKind;

pub fn list(ui: &mut Ui, app: &mut App) {
    let Some(track_count) = app.project.as_ref().map(|p| p.tracks.len()) else {
        return;
    };

    ui.group(|ui| {
        ui.label(RichText::new("Tracks").strong());
        ui.add_space(6.0);
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for index in 0..track_count {
                let (name, kind, color, vol, pedal_n, input_lbl, armed, muted, solo, action_label) = {
                    let p = app.project.as_ref().unwrap();
                    let t = &p.tracks[index];
                    (
                        t.name.clone(),
                        t.kind,
                        t.color,
                        t.volume,
                        t.pedals.len(),
                        t.input_device.clone().unwrap_or_else(|| "no input".into()),
                        t.armed,
                        t.muted,
                        t.solo,
                        track_action_label(t),
                    )
                };

                let accent = util::track_accent(color);
                let selected = index == app.selected_track;
                let stroke = if selected {
                    Stroke::new(1.5, accent)
                } else {
                    Stroke::new(1.0, egui::Color32::from_rgb(48, 52, 64))
                };

                egui::Frame::none()
                    .fill(if selected {
                        egui::Color32::from_rgb(34, 38, 50)
                    } else {
                        egui::Color32::from_rgb(28, 30, 38)
                    })
                    .stroke(stroke)
                    .rounding(6.0)
                    .inner_margin(egui::Margin::same(8.0))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let row_title = format!(
                                "{}  {}  {}  ·  {:.0}%  ·  {} pedals{}",
                                kind.label(),
                                name,
                                input_lbl,
                                vol * 100.0,
                                pedal_n,
                                if armed { "  armed" } else { "" },
                            );
                            if ui
                                .add(egui::SelectableLabel::new(selected, RichText::new(row_title).color(accent)))
                                .clicked()
                            {
                                app.selected_track = index;
                                let _ = app.sync_router_after_track_change();
                            }

                            if ui
                                .add(egui::SelectableLabel::new(
                                    muted,
                                    RichText::new("M").color(egui::Color32::from_rgb(255, 120, 120)),
                                ))
                                .clicked()
                            {
                                app.selected_track = index;
                                let _ = app.toggle_track_mute();
                            }
                            if ui
                                .add(egui::SelectableLabel::new(
                                    solo,
                                    RichText::new("S").color(egui::Color32::from_rgb(240, 200, 90)),
                                ))
                                .clicked()
                            {
                                app.selected_track = index;
                                let _ = app.toggle_track_solo();
                            }

                            if ui.button(action_label).clicked() {
                                app.selected_track = index;
                                let _ = app.toggle_track_action();
                            }

                            if ui.button("Settings").clicked() {
                                app.selected_track = index;
                                app.open_track_settings();
                                let _ = app.sync_router_after_track_change();
                            }

                            let can_remove = kind != TrackKind::Drum;
                            ui.add_enabled_ui(can_remove, |ui| {
                                if ui
                                    .button(RichText::new("Remove").color(egui::Color32::from_rgb(255, 140, 140)))
                                    .clicked()
                                {
                                    app.selected_track = index;
                                    let _ = app.remove_selected_track();
                                }
                            });
                        });
                    });
                ui.add_space(4.0);
            }
        });
    });
}
