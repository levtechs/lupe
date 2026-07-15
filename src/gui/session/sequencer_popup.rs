use eframe::egui::{self, Color32, Context, RichText, Sense, Stroke};

use crate::app::{sequence_step_count, App, SequencerKeymapMode, SequencerPadKey};
use crate::gui::theme;
use crate::project::{DrumRole, DrumSequence};

use super::{sequencer_pattern_library, sequencer_sample_browser};

pub fn show(ctx: &Context, app: &mut App) {
    if !app.sequencer_popup_open {
        return;
    }
    let Some((beats_per_bar, beat_unit)) = app
        .project
        .as_ref()
        .map(|project| (project.transport.beats_per_bar, project.transport.beat_unit))
    else {
        return;
    };
    if !app.sequencer_text_input_active {
        let keys = pressed_pad_keys(ctx);
        if !keys.is_empty() {
            app.handle_sequencer_pad_keys(&keys);
        }
        if ctx.input(|input| input.key_pressed(egui::Key::Space)) {
            app.toggle_sequencer_preview_playback();
        }
    }
    app.sequencer_text_input_active = false;
    let Some(sequence) = app.sequence().cloned() else {
        return;
    };
    if !ctx.input(|input| input.pointer.any_down()) {
        app.sequencer_drag_paint_mode = None;
    }

    let screen = ctx.input(|input| input.screen_rect().size());
    let max_width = (screen.x - 24.0).max(280.0).min(920.0);
    let body_max_height = (screen.y - 160.0).clamp(220.0, 680.0);
    let drawer_height = (body_max_height - 112.0).max(280.0);
    let default_width = 760.0_f32.min(max_width);
    let min_width = 500.0_f32.min(max_width);
    let preview_step = app.current_sequencer_preview_step_index();
    let total_steps = sequence_step_count(&sequence, beats_per_bar);

    let mut open = true;
    egui::Window::new("Drum sequencer")
        .id(egui::Id::new("drum_sequencer_v5"))
        .open(&mut open)
        .default_width(default_width)
        .min_width(min_width)
        .max_width(max_width)
        .constrain(true)
        .resizable([true, false])
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("sequencer_body")
                .max_height(body_max_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    show_toolbar(ui, app, &sequence, beats_per_bar, beat_unit);
                    ui.add_space(8.0);
                    let drawer_open = app.pattern_browser_open || app.sample_browser_open;
                    if drawer_open && ui.available_width() < 620.0 {
                        if app.pattern_browser_open {
                            sequencer_pattern_library::show(ui, app, drawer_height);
                        } else {
                            sequencer_sample_browser::show(ui, app, drawer_height);
                        }
                        ui.add_space(6.0);
                        show_grid(ctx, ui, app, &sequence, total_steps, beats_per_bar, preview_step);
                    } else {
                        ui.horizontal_top(|ui| {
                            if drawer_open {
                                let drawer_width = ui.available_width().min(300.0);
                                ui.vertical(|ui| {
                                    ui.set_width(drawer_width);
                                    if app.pattern_browser_open {
                                        sequencer_pattern_library::show(ui, app, drawer_height);
                                    } else {
                                        sequencer_sample_browser::show(ui, app, drawer_height);
                                    }
                                });
                                ui.separator();
                            }
                            show_grid(ctx, ui, app, &sequence, total_steps, beats_per_bar, preview_step);
                        });
                    }
                });
        });

    if !app.sequencer_popup_open {
        return;
    }
    app.sequencer_popup_open = open;
    if !open {
        app.cancel_sequence_chunk();
    }
}

fn show_toolbar(ui: &mut egui::Ui, app: &mut App, sequence: &DrumSequence, beats_per_bar: u32, beat_unit: u32) {
    ui.horizontal(|ui| {
        if ui.button(if app.sequencer_preview_playing { "Pause" } else { "Play" }).clicked() {
            app.toggle_sequencer_preview_playback();
        }
        if ui
            .selectable_label(
                app.pattern_browser_open,
                if app.pattern_browser_open { "Hide library" } else { "Pattern library" },
            )
            .clicked()
        {
            app.pattern_browser_open = !app.pattern_browser_open;
            if app.pattern_browser_open {
                app.stop_sample_preview();
                app.sample_browser_open = false;
            } else {
                app.stop_library_pattern_preview();
            }
        }
        if ui
            .selectable_label(app.sample_browser_open, if app.sample_browser_open { "Hide sounds" } else { "Add sound" })
            .clicked()
        {
            if app.sample_browser_open {
                app.close_sample_browser();
            } else {
                app.toggle_sample_browser_for_lane(None);
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let save = if matches!(app.sequencer_target, Some(crate::app::SequencerTarget::Edit(_))) {
                "Save chunk"
            } else {
                "Create chunk"
            };
            if ui.add(egui::Button::new(save).fill(theme::ACCENT.gamma_multiply(0.65))).clicked() {
                app.save_sequence_chunk();
            }
            if ui.button("Cancel").clicked() {
                app.cancel_sequence_chunk();
            }
        });
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Bars");
        if ui.small_button("-").clicked() {
            app.adjust_selected_sequence_measures(-1);
        }
        ui.label(sequence.measures.to_string());
        if ui.small_button("+").clicked() {
            app.adjust_selected_sequence_measures(1);
        }
        ui.label("Grid");
        if ui.small_button("-").clicked() {
            app.adjust_selected_sequence_subdivision(-1);
        }
        ui.label(sequence.subdivision.label());
        if ui.small_button("+").clicked() {
            app.adjust_selected_sequence_subdivision(1);
        }
        ui.label(RichText::new(format!("{beats_per_bar}/{beat_unit}")).color(theme::MUTED));
        let mut mode = app.sequencer_keymap_mode;
        egui::ComboBox::from_id_salt("sequencer_keymap_mode")
            .selected_text(mode.label())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut mode, SequencerKeymapMode::DrumKit, SequencerKeymapMode::DrumKit.label());
                ui.selectable_value(&mut mode, SequencerKeymapMode::Asdf, SequencerKeymapMode::Asdf.label());
                ui.selectable_value(&mut mode, SequencerKeymapMode::Custom, SequencerKeymapMode::Custom.label());
            });
        if mode != app.sequencer_keymap_mode {
            app.set_sequencer_keymap_mode(mode);
        }
        ui.label("Count-in");
        if ui.small_button("-").clicked() {
            app.adjust_sequencer_record_count_in_beats(-1);
        }
        ui.label(app.sequencer_record_count_in_beats.to_string());
        if ui.small_button("+").clicked() {
            app.adjust_sequencer_record_count_in_beats(1);
        }
        let record = if app.sequencer_record_armed { "Recording" } else { "Record" };
        if ui
            .add(egui::Button::new(record).fill(if app.sequencer_record_armed {
                theme::DANGER.gamma_multiply(0.65)
            } else {
                Color32::from_rgb(56, 62, 74)
            }))
            .clicked()
        {
            app.toggle_sequencer_record_armed();
        }
        if let Some(left) = app.sequencer_count_in_remaining_beats {
            ui.label(RichText::new(format!("{left:.1} beats")).color(theme::WARNING));
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn show_grid(
    ctx: &Context,
    ui: &mut egui::Ui,
    app: &mut App,
    sequence: &DrumSequence,
    total_steps: usize,
    beats_per_bar: u32,
    preview_step: Option<usize>,
) {
    const LANE_CONTROLS_WIDTH: f32 = 208.0;
    const ROW_HEIGHT: f32 = 30.0;
    const STEP_SIZE: f32 = 24.0;

    egui::ScrollArea::horizontal()
        .id_salt("sequencer_grid_scroll_v3")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if sequence.lanes.is_empty() {
                ui.label(RichText::new("Choose a pattern or add a sound to begin.").color(theme::MUTED));
                return;
            }
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(LANE_CONTROLS_WIDTH, 18.0),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.label(RichText::new("LANES").small().strong().color(theme::MUTED));
                        },
                    );
                    for step_index in 0..total_steps {
                        let steps_per_bar = beats_per_bar as usize * sequence.subdivision.steps_per_beat() as usize;
                        let label = if step_index % steps_per_bar == 0 {
                            format!("{}", step_index / steps_per_bar + 1)
                        } else {
                            String::new()
                        };
                        ui.add_sized(
                            [STEP_SIZE, 18.0],
                            egui::Label::new(RichText::new(label).small().monospace().color(theme::MUTED)),
                        );
                    }
                });
                for (lane_index, lane) in sequence.lanes.iter().enumerate() {
                ui.horizontal(|ui| {
                    show_lane_controls(ui, app, lane, lane_index, LANE_CONTROLS_WIDTH, ROW_HEIGHT);
                    for step_index in 0..total_steps {
                        show_step(
                            ctx,
                            ui,
                            app,
                            sequence,
                            lane_index,
                            step_index,
                            beats_per_bar,
                            preview_step,
                            STEP_SIZE,
                        );
                    }
                });
                }
            });
        });
}

fn show_lane_controls(
    ui: &mut egui::Ui,
    app: &mut App,
    lane: &crate::project::DrumLane,
    lane_index: usize,
    width: f32,
    height: f32,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_sized(
                [86.0, height],
                egui::Label::new(RichText::new(&lane.name).strong()).truncate(),
            )
            .on_hover_text(&lane.name);
            if ui
                .add_sized(
                    [28.0, height],
                    egui::Button::new("M").fill(if lane.muted { theme::WARNING.gamma_multiply(0.7) } else { theme::CARD }),
                )
                .on_hover_text("Mute lane")
                .clicked()
            {
                app.toggle_sequence_lane_mute(lane_index);
            }
            let sound_open = app.sample_browser_open && app.sample_browser_target_lane == Some(lane_index);
            if ui
                .add_sized(
                    [48.0, height],
                    egui::Button::new("Sound").fill(if sound_open { theme::ACCENT.gamma_multiply(0.55) } else { theme::CARD }),
                )
                .clicked()
            {
                app.toggle_sample_browser_for_lane(Some(lane_index));
            }
            ui.menu_button("…", |ui| show_lane_menu(ui, app, lane, lane_index));
        },
    );
}

fn show_lane_menu(ui: &mut egui::Ui, app: &mut App, lane: &crate::project::DrumLane, lane_index: usize) {
    ui.set_min_width(210.0);
    ui.label(RichText::new("Lane settings").strong());
    ui.separator();

    let mut role = lane.effective_role();
    egui::ComboBox::from_id_salt(("lane_role", lane_index))
        .selected_text(role.label())
        .width(150.0)
        .show_ui(ui, |ui| {
            for option in DrumRole::ALL {
                ui.selectable_value(&mut role, option, option.label());
            }
        });
    if role != lane.effective_role() {
        app.set_sequence_lane_role(lane_index, role);
    }

    let mut binding = app.sequencer_lane_binding(lane_index);
    egui::ComboBox::from_id_salt(("lane_key_binding", lane_index))
        .selected_text(binding.map(|key| format!("Pad key: {}", key.label())).unwrap_or_else(|| "Pad key: none".to_string()))
        .width(150.0)
        .show_ui(ui, |ui| {
            if ui.selectable_label(binding.is_none(), "None").clicked() {
                binding = None;
            }
            for key in App::sequencer_available_pad_keys() {
                if ui.selectable_label(binding == Some(*key), key.label()).clicked() {
                    binding = Some(*key);
                }
            }
        });
    if binding != app.sequencer_lane_binding(lane_index) {
        app.set_sequencer_lane_binding(lane_index, binding);
    }

    let mut gain = lane.gain;
    if ui.add(egui::Slider::new(&mut gain, 0.0..=2.0).text("Level")).changed() {
        app.set_sequence_lane_gain(lane_index, gain);
    }
    ui.horizontal(|ui| {
        if ui.small_button("Move up").clicked() {
            app.move_sequence_lane(lane_index, -1);
            ui.close_menu();
        }
        if ui.small_button("Move down").clicked() {
            app.move_sequence_lane(lane_index, 1);
            ui.close_menu();
        }
    });
    if ui.button(RichText::new("Remove lane").color(theme::DANGER)).clicked() {
        app.remove_sequence_lane(lane_index);
        ui.close_menu();
    }
}

#[allow(clippy::too_many_arguments)]
fn show_step(
    ctx: &Context,
    ui: &mut egui::Ui,
    app: &mut App,
    sequence: &DrumSequence,
    lane_index: usize,
    step_index: usize,
    beats_per_bar: u32,
    preview_step: Option<usize>,
    size: f32,
) {
    let lane = &sequence.lanes[lane_index];
    let active = lane.steps.get(step_index).copied().unwrap_or(false);
    let bar_step = step_index as u32 % (beats_per_bar * sequence.subdivision.steps_per_beat()) == 0;
    let current = app.sequencer_preview_playing && preview_step == Some(step_index);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), Sense::click_and_drag());
    let fill = if current {
        theme::DANGER
    } else if active {
        theme::ACCENT
    } else if bar_step {
        theme::BORDER_STRONG
    } else {
        theme::CARD
    };
    ui.painter().rect(rect, 4.0, fill, Stroke::new(1.0, theme::BORDER));

    if response.clicked() {
        app.toggle_sequence_step(lane_index, step_index);
    } else if response.drag_started() {
        let value = !ctx.input(|input| input.modifiers.shift) && !active;
        app.sequencer_drag_paint_mode = Some(value);
        app.set_sequence_step_enabled(lane_index, step_index, value, false);
    } else if response.hovered() && ctx.input(|input| input.pointer.primary_down()) {
        if let Some(value) = app.sequencer_drag_paint_mode {
            app.set_sequence_step_enabled(lane_index, step_index, value, false);
        }
    }
    if response.secondary_clicked() || (response.hovered() && ctx.input(|input| input.pointer.secondary_down())) {
        app.sequencer_drag_paint_mode = Some(false);
        app.set_sequence_step_enabled(lane_index, step_index, false, false);
    }
}

fn pressed_pad_keys(ctx: &Context) -> Vec<SequencerPadKey> {
    App::sequencer_available_pad_keys()
        .iter()
        .copied()
        .filter(|key| ctx.input(|input| input.key_pressed(to_egui_key(*key))))
        .collect()
}

fn to_egui_key(key: SequencerPadKey) -> egui::Key {
    match key {
        SequencerPadKey::Q => egui::Key::Q,
        SequencerPadKey::W => egui::Key::W,
        SequencerPadKey::E => egui::Key::E,
        SequencerPadKey::R => egui::Key::R,
        SequencerPadKey::T => egui::Key::T,
        SequencerPadKey::Y => egui::Key::Y,
        SequencerPadKey::U => egui::Key::U,
        SequencerPadKey::I => egui::Key::I,
        SequencerPadKey::O => egui::Key::O,
        SequencerPadKey::P => egui::Key::P,
        SequencerPadKey::A => egui::Key::A,
        SequencerPadKey::S => egui::Key::S,
        SequencerPadKey::D => egui::Key::D,
        SequencerPadKey::F => egui::Key::F,
        SequencerPadKey::G => egui::Key::G,
        SequencerPadKey::H => egui::Key::H,
        SequencerPadKey::J => egui::Key::J,
        SequencerPadKey::K => egui::Key::K,
        SequencerPadKey::L => egui::Key::L,
        SequencerPadKey::Z => egui::Key::Z,
        SequencerPadKey::X => egui::Key::X,
        SequencerPadKey::C => egui::Key::C,
        SequencerPadKey::V => egui::Key::V,
        SequencerPadKey::B => egui::Key::B,
        SequencerPadKey::N => egui::Key::N,
        SequencerPadKey::M => egui::Key::M,
    }
}
