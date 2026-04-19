use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, RichText, Sense, Stroke, Ui, Vec2};

use crate::app::App;
use crate::gui::meters;
use crate::pedals::{PedalKind, PedalSpec};

const STORE_WIDTH: f32 = 236.0;
const PEDAL_WIDTH: f32 = 124.0;
const PEDAL_HEIGHT: f32 = 220.0;
const IO_WIDTH: f32 = 150.0;

pub fn show(ui: &mut Ui, app: &mut App) {
    let Some(project) = app.project.as_ref() else {
        return;
    };

    let input_device = project.input.device.clone().unwrap_or_else(|| "No input".to_string());
    let input_volume = project.input.volume;
    let output_device = project.output_device.clone().unwrap_or_else(|| "No output".to_string());
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
                    .map(|param_index| {
                        (
                            pedal.param_name(param_index).to_string(),
                            pedal.param_value(param_index),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();

    ui.add_space(6.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width((ui.available_width() - STORE_WIDTH - 14.0).max(280.0));
            chain_area(ui, app, &input_device, input_volume, &output_device, &pedals);
        });

        ui.add_space(14.0);

        ui.vertical(|ui| {
            ui.set_width(STORE_WIDTH);
            store_area(ui, app);
        });
    });
}

fn chain_area(
    ui: &mut Ui,
    app: &mut App,
    input_device: &str,
    input_volume: f32,
    output_device: &str,
    pedals: &[(usize, String, (u8, u8, u8), bool, String, Vec<(String, String)>)],
) {
    Frame::group(ui.style())
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Pedalboard").strong().size(18.0));
                ui.separator();
                ui.label(RichText::new("Live input chain").small().weak());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let fill = if app.route_enabled() {
                        Color32::from_rgb(70, 112, 188)
                    } else {
                        Color32::from_rgb(58, 62, 74)
                    };
                    if ui
                        .add(egui::Button::new("Route").fill(fill))
                        .on_hover_text("Monitor live input through the pedal chain")
                        .clicked()
                    {
                        let _ = app.toggle_route();
                    }
                });
            });

            ui.add_space(10.0);

            Frame::none()
                .fill(Color32::from_rgb(20, 23, 31))
                .stroke(Stroke::new(1.0, Color32::from_rgb(46, 52, 64)))
                .inner_margin(Margin::same(12.0))
                .show(ui, |ui| {
                    egui::ScrollArea::horizontal().id_salt("pedal_chain_scroll").show(ui, |ui| {
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

fn input_block(ui: &mut Ui, app: &mut App, device: &str, volume: f32) {
    fixed_card(ui, IO_WIDTH, PEDAL_HEIGHT, |ui| {
        io_block(ui, Color32::from_rgb(52, 92, 84), "INPUT", |ui| {
        ui.label(RichText::new(device).small());
        ui.add_space(6.0);
        meters::peak_strip(ui, "Input", app.input_meter);
        ui.add_space(6.0);
        let mut next_volume = volume;
        if ui
            .add(egui::Slider::new(&mut next_volume, 0.0..=1.0).text("Gain"))
            .changed()
        {
            let _ = app.set_input_volume(next_volume);
        }
        ui.add_space(8.0);
        if ui.button("Audio devices").clicked() {
            app.input_popup_open = true;
        }
        });
    });
}

fn output_block(ui: &mut Ui, app: &mut App, device: &str) {
    fixed_card(ui, IO_WIDTH, PEDAL_HEIGHT, |ui| {
        io_block(ui, Color32::from_rgb(96, 74, 112), "OUTPUT", |ui| {
        ui.label(RichText::new(device).small());
        ui.add_space(6.0);
        meters::peak_strip(ui, "Output", app.output_meter);
        ui.add_space(8.0);
        ui.label(RichText::new(format!("Latency {}", app.latency_label)).small().weak());
        ui.add_space(8.0);
        if ui.button("Audio devices").clicked() {
            app.input_popup_open = true;
        }
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
    let (r, g, b) = accent_rgb;
    let accent = Color32::from_rgb(r, g, b);

    fixed_card(ui, PEDAL_WIDTH, PEDAL_HEIGHT, |ui| {
    Frame::none()
        .fill(accent.gamma_multiply(0.23))
        .stroke(Stroke::new(1.2, accent))
        .inner_margin(Margin::same(10.0))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(display_name).strong().size(16.0));
            });
            ui.add_space(6.0);
            ui.label(RichText::new(summary).small().weak());
            ui.add_space(8.0);

            for (param_index, (name, value)) in params.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(name).small());
                    if ui.small_button("-").clicked() {
                        let _ = app.adjust_pedal_param(index, param_index, -1);
                    }
                    ui.label(RichText::new(value).small().weak());
                    if ui.small_button("+").clicked() {
                        let _ = app.adjust_pedal_param(index, param_index, 1);
                    }
                });
            }

            ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                if ui
                    .add_sized(
                        [ui.available_width(), 24.0],
                        egui::Button::new(RichText::new("Remove").color(Color32::from_rgb(220, 118, 118))),
                    )
                    .clicked()
                {
                    let _ = app.remove_pedal(index);
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.small_button("<- ").clicked() {
                        let _ = app.move_pedal(index, -1);
                    }
                    if ui.small_button(" ->").clicked() {
                        let _ = app.move_pedal(index, 1);
                    }
                });

                ui.add_space(6.0);
                let fill = if enabled {
                    Color32::from_rgb(64, 126, 92)
                } else {
                    Color32::from_rgb(90, 64, 64)
                };
                if ui
                    .add_sized([ui.available_width(), 28.0], egui::Button::new(if enabled { "On" } else { "Off" }).fill(fill))
                    .clicked()
                {
                    let _ = app.toggle_pedal_enabled(index);
                }
            });
        });
    });
}

fn store_area(ui: &mut Ui, app: &mut App) {
    Frame::group(ui.style())
        .inner_margin(Margin::same(12.0))
        .show(ui, |ui| {
            ui.label(RichText::new("Pedal Store").strong().size(18.0));
            ui.label(RichText::new("Scrollable pedal previews").small().weak());
            ui.add_space(10.0);

            egui::ScrollArea::vertical().id_salt("pedal_store_scroll").show(ui, |ui| {
                for kind in PedalKind::ALL {
                    let preview = PedalSpec::new(kind);
                    store_card(ui, &preview, || {
                        let _ = app.add_pedal(kind);
                    });
                    ui.add_space(10.0);
                }
            });
        });
}

fn store_card(ui: &mut Ui, pedal: &PedalSpec, on_add: impl FnOnce()) {
    let (r, g, b) = pedal.accent_rgb();
    let accent = Color32::from_rgb(r, g, b);
    let response = Frame::none()
        .fill(accent.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0, accent))
        .inner_margin(Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_min_height(104.0);
            ui.label(RichText::new(pedal.display_name()).strong());
            ui.add_space(4.0);
            ui.label(RichText::new(pedal.description()).small().weak());
            ui.add_space(10.0);
            ui.button("Add to board")
        })
        .inner;

    if response.clicked() {
        on_add();
    }
}

fn io_block(ui: &mut Ui, accent: Color32, title: &str, add_contents: impl FnOnce(&mut Ui)) {
    Frame::none()
        .fill(accent.gamma_multiply(0.2))
        .stroke(Stroke::new(1.2, accent))
        .inner_margin(Margin::same(10.0))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(title).strong().size(16.0));
            });
            ui.add_space(8.0);
            add_contents(ui);
        });
}

fn fixed_card(ui: &mut Ui, width: f32, height: f32, add_contents: impl FnOnce(&mut Ui)) {
    ui.allocate_ui_with_layout(Vec2::new(width, height), Layout::top_down(Align::Min), |ui| {
        ui.set_width(width);
        add_contents(ui);
    });
}

fn cable(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(42.0, PEDAL_HEIGHT), Sense::hover());
    let mid = rect.center().y;
    let start = egui::pos2(rect.left() + 2.0, mid);
    let end = egui::pos2(rect.right() - 2.0, mid);
    ui.painter().line_segment([start, end], Stroke::new(3.0, Color32::from_rgb(90, 96, 116)));
    ui.painter().circle_filled(start, 4.0, Color32::from_rgb(125, 130, 146));
    ui.painter().circle_filled(end, 4.0, Color32::from_rgb(125, 130, 146));
}
