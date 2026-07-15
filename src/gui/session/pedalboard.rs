use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, RichText, Rounding, Sense, Stroke, Ui, Vec2};

use crate::app::App;
use crate::gui::{meters, theme};
use crate::pedals::{PedalKind, PedalSpec};

const STORE_WIDTH: f32 = 264.0;
const PANEL_GAP: f32 = 8.0;
const PEDAL_WIDTH: f32 = 176.0;
const CARD_HEIGHT: f32 = 252.0;
const IO_WIDTH: f32 = 176.0;
const CABLE_WIDTH: f32 = 34.0;
const TEXT: Color32 = theme::TEXT;
const MUTED_TEXT: Color32 = theme::MUTED;

type PedalView = (usize, String, (u8, u8, u8), bool, String, Vec<(String, String)>);

pub fn show(ui: &mut Ui, app: &mut App) {
    let Some(project) = app.project.as_ref() else {
        return;
    };

    let input_device = project.input.device.clone().unwrap_or_else(|| "No input selected".to_string());
    let input_volume = project.input.volume;
    let output_device = project.output_device.clone().unwrap_or_else(|| "No output selected".to_string());
    let pedals = project
        .pedalboard
        .iter()
        .enumerate()
        .map(|(index, pedal)| {
            (
                index,
                pedal.display_name().to_string(),
                pedal.accent_rgb(),
                pedal.enabled(),
                pedal.summary(),
                (0..pedal.param_count())
                    .map(|param_index| (pedal.param_name(param_index).to_string(), pedal.param_value(param_index)))
                    .collect(),
            )
        })
        .collect::<Vec<PedalView>>();

    ui.add_space(6.0);
    let available = ui.available_size();
    let store_width = STORE_WIDTH.min((available.x * 0.34).max(220.0));
    let chain_width = (available.x - store_width - PANEL_GAP).max(300.0);

    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.allocate_ui_with_layout(Vec2::new(chain_width, available.y), Layout::top_down(Align::Min), |ui| {
            chain_area(ui, app, &input_device, input_volume, &output_device, &pedals);
        });
        ui.add_space(PANEL_GAP);
        ui.allocate_ui_with_layout(Vec2::new(store_width, available.y), Layout::top_down(Align::Min), |ui| {
            store_area(ui, app);
        });
    });
}

fn chain_area(ui: &mut Ui, app: &mut App, input_device: &str, input_volume: f32, output_device: &str, pedals: &[PedalView]) {
    panel_frame().show(ui, |ui| {
        ui.set_min_height((ui.available_height() - 2.0).max(CARD_HEIGHT + 70.0));
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("Pedalboard").color(TEXT).strong().size(19.0));
                ui.label(RichText::new("Your live input signal chain").color(MUTED_TEXT).size(12.0));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                route_button(ui, app);
            });
        });

        ui.add_space(12.0);
        Frame::none()
            .fill(theme::CANVAS)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::symmetric(14.0, 12.0))
            .show(ui, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("pedal_chain_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            input_block(ui, app, input_device, input_volume);
                            for (index, display_name, accent_rgb, enabled, summary, params) in pedals {
                                cable(ui);
                                pedal_block(ui, app, *index, display_name, *accent_rgb, *enabled, summary, params);
                            }
                            cable(ui);
                            output_block(ui, app, output_device);
                        });
                    });
            });
    });
}

fn route_button(ui: &mut Ui, app: &mut App) {
    let enabled = app.route_enabled();
    let (fill, stroke, label) = if enabled {
        (
            Color32::from_rgb(45, 91, 76),
            Color32::from_rgb(75, 151, 123),
            "Routing on",
        )
    } else {
        (Color32::from_rgb(39, 43, 53), theme::BORDER, "Routing off")
    };
    if ui
        .add_sized(
            [108.0, 32.0],
            egui::Button::new(RichText::new(label).color(TEXT).size(12.0))
                .fill(fill)
                .stroke(Stroke::new(1.0, stroke))
                .rounding(Rounding::same(6.0)),
        )
        .on_hover_text("Monitor live input through the pedal chain")
        .clicked()
    {
        let _ = app.toggle_route();
    }
}

fn input_block(ui: &mut Ui, app: &mut App, device: &str, volume: f32) {
    fixed_card(ui, IO_WIDTH, CARD_HEIGHT, |ui| {
        io_block(ui, Color32::from_rgb(54, 133, 112), "INPUT", device, |ui| {
            meters::peak_strip(ui, "Input level", app.input_meter);
            ui.add_space(7.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Gain").color(MUTED_TEXT).size(12.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(format!("{volume:.2}")).color(TEXT).monospace().size(12.0));
                });
            });
            let mut next_volume = volume;
            if ui.add(egui::Slider::new(&mut next_volume, 0.0..=1.0).show_value(false)).changed() {
                let _ = app.set_input_volume(next_volume);
            }

            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                if ui
                    .add_sized([ui.available_width(), 28.0], egui::Button::new("Audio settings"))
                    .clicked()
                {
                    app.input_popup_open = true;
                }
            });
        });
    });
}

fn output_block(ui: &mut Ui, app: &mut App, device: &str) {
    fixed_card(ui, IO_WIDTH, CARD_HEIGHT, |ui| {
        io_block(ui, Color32::from_rgb(139, 99, 163), "OUTPUT", device, |ui| {
            meters::peak_strip(ui, "Output level", app.output_meter);
            ui.add_space(10.0);
            ui.label(RichText::new("MONITORING LATENCY").color(MUTED_TEXT).size(10.0));
            ui.label(RichText::new(&app.latency_label).color(TEXT).monospace().size(12.0));

            ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                if ui
                    .add_sized([ui.available_width(), 28.0], egui::Button::new("Audio settings"))
                    .clicked()
                {
                    app.input_popup_open = true;
                }
            });
        });
    });
}

fn pedal_block(
    ui: &mut Ui,
    app: &mut App,
    index: usize,
    display_name: &str,
    accent_rgb: (u8, u8, u8),
    enabled: bool,
    summary: &str,
    params: &[(String, String)],
) {
    let accent = Color32::from_rgb(accent_rgb.0, accent_rgb.1, accent_rgb.2);

    fixed_card(ui, PEDAL_WIDTH, CARD_HEIGHT, |ui| {
        Frame::none()
            .fill(theme::CARD)
            .stroke(Stroke::new(1.0, if enabled { accent.gamma_multiply(0.8) } else { theme::BORDER }))
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::same(11.0))
            .show(ui, |ui| {
                ui.set_min_height(CARD_HEIGHT - 24.0);
                let stripe = egui::Rect::from_min_size(ui.min_rect().min, egui::vec2(ui.available_width(), 3.0));
                ui.painter().rect_filled(stripe, 2.0, accent);
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new(display_name).color(TEXT).strong().size(15.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let status = if enabled { "ACTIVE" } else { "BYPASS" };
                        let color = if enabled { accent } else { MUTED_TEXT };
                        ui.label(RichText::new(status).color(color).strong().size(9.0));
                    });
                });
                ui.label(RichText::new(summary).color(MUTED_TEXT).size(11.0));
                ui.add_space(7.0);

                for (param_index, (name, value)) in params.iter().enumerate() {
                    parameter_row(ui, app, index, param_index, name, value);
                }

                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        let arrow_width = 28.0;
                        if ui.add_sized([arrow_width, 25.0], egui::Button::new("<")).on_hover_text("Move left").clicked() {
                            let _ = app.move_pedal(index, -1);
                        }
                        if ui.add_sized([arrow_width, 25.0], egui::Button::new(">")).on_hover_text("Move right").clicked() {
                            let _ = app.move_pedal(index, 1);
                        }
                        let remove_width = ui.available_width();
                        if ui
                            .add_sized(
                                [remove_width, 25.0],
                                egui::Button::new(RichText::new("Remove").color(theme::DANGER)),
                            )
                            .clicked()
                        {
                            let _ = app.remove_pedal(index);
                        }
                    });

                    ui.add_space(5.0);
                    let (label, fill) = if enabled {
                        ("Enabled", accent.gamma_multiply(0.55))
                    } else {
                        ("Bypassed", Color32::from_rgb(48, 51, 61))
                    };
                    if ui
                        .add_sized(
                            [ui.available_width(), 28.0],
                            egui::Button::new(RichText::new(label).color(TEXT)).fill(fill),
                        )
                        .clicked()
                    {
                        let _ = app.toggle_pedal_enabled(index);
                    }
                });
            });
    });
}

fn parameter_row(ui: &mut Ui, app: &mut App, pedal_index: usize, param_index: usize, name: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        ui.add_sized([41.0, 20.0], egui::Label::new(RichText::new(name).color(MUTED_TEXT).size(11.0)));
        if ui.add_sized([25.0, 20.0], egui::Button::new("-")).clicked() {
            let _ = app.adjust_pedal_param(pedal_index, param_index, -1);
        }
        ui.add_sized([34.0, 20.0], egui::Label::new(RichText::new(value).color(TEXT).monospace().size(11.0)));
        if ui.add_sized([25.0, 20.0], egui::Button::new("+")).clicked() {
            let _ = app.adjust_pedal_param(pedal_index, param_index, 1);
        }
    });
}

fn store_area(ui: &mut Ui, app: &mut App) {
    panel_frame().show(ui, |ui| {
        ui.set_min_height((ui.available_height() - 2.0).max(CARD_HEIGHT + 70.0));
        ui.label(RichText::new("Add a pedal").color(TEXT).strong().size(19.0));
        ui.label(RichText::new("Shape your sound with an effect").color(MUTED_TEXT).size(12.0));
        ui.add_space(12.0);

        egui::ScrollArea::vertical()
            .id_salt("pedal_store_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for kind in PedalKind::ALL {
                    let preview = PedalSpec::new(kind);
                    store_card(ui, &preview, || {
                        let _ = app.add_pedal(kind);
                    });
                    ui.add_space(8.0);
                }
            });
    });
}

fn store_card(ui: &mut Ui, pedal: &PedalSpec, on_add: impl FnOnce()) {
    let accent_rgb = pedal.accent_rgb();
    let accent = Color32::from_rgb(accent_rgb.0, accent_rgb.1, accent_rgb.2);
    let clicked = Frame::none()
        .fill(theme::CARD)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .rounding(Rounding::same(7.0))
        .inner_margin(Margin::symmetric(11.0, 9.0))
        .show(ui, |ui| {
            ui.set_min_height(64.0);
            let marker = egui::Rect::from_min_size(ui.min_rect().min, egui::vec2(3.0, 64.0));
            ui.painter().rect_filled(marker, 2.0, accent);

            ui.horizontal(|ui| {
                let text_width = (ui.available_width() - 52.0).max(110.0);
                ui.allocate_ui_with_layout(Vec2::new(text_width, 62.0), Layout::top_down(Align::Min), |ui| {
                    ui.label(RichText::new(pedal.display_name()).color(TEXT).strong().size(14.0));
                    ui.add_space(2.0);
                    ui.label(RichText::new(pedal.description()).color(MUTED_TEXT).size(11.0));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(
                        egui::Button::new(RichText::new("Add").color(TEXT).strong())
                            .fill(accent.gamma_multiply(0.42))
                            .stroke(Stroke::new(1.0, accent.gamma_multiply(0.8))),
                    )
                    .clicked()
                })
                .inner
            })
            .inner
        })
        .inner;

    if clicked {
        on_add();
    }
}

fn io_block(ui: &mut Ui, accent: Color32, title: &str, device: &str, add_contents: impl FnOnce(&mut Ui)) {
    Frame::none()
        .fill(theme::CARD)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(11.0))
        .show(ui, |ui| {
            ui.set_min_height(CARD_HEIGHT - 24.0);
            let stripe = egui::Rect::from_min_size(ui.min_rect().min, egui::vec2(ui.available_width(), 3.0));
            ui.painter().rect_filled(stripe, 2.0, accent);
            ui.add_space(8.0);
            ui.label(RichText::new(title).color(accent).strong().size(10.0));
            ui.label(RichText::new(device).color(TEXT).strong().size(13.0));
            ui.add_space(10.0);
            add_contents(ui);
        });
}

fn panel_frame() -> Frame {
    theme::panel_frame()
}

fn fixed_card(ui: &mut Ui, width: f32, height: f32, add_contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(Vec2::new(width, height), Layout::top_down(Align::Min), |ui| {
        ui.set_min_size(Vec2::new(width, height));
        add_contents(ui);
    });
}

fn cable(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(CABLE_WIDTH, CARD_HEIGHT), Sense::hover());
    let mid = rect.center().y;
    let start = egui::pos2(rect.left() + 2.0, mid);
    let end = egui::pos2(rect.right() - 2.0, mid);
    let cable = Color32::from_rgb(82, 91, 112);
    ui.painter().line_segment([start, end], Stroke::new(2.0, cable));
    ui.painter().circle_filled(start, 3.5, Color32::from_rgb(116, 124, 143));
    ui.painter().circle_filled(end, 3.5, Color32::from_rgb(116, 124, 143));
}
