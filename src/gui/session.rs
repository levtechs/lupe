mod input_settings_popup;
mod inspector;
mod metronome_popup;
mod pedalboard;
mod sequencer_pattern_library;
mod rename_project_popup;
mod sequencer_sample_browser;
mod sequencer_popup;
mod timeline;
mod track_row;
mod top_bar;
mod transport_popup;

use eframe::egui::{self, Context};

use crate::app::App;
use crate::gui::theme;

const WORKSPACE_GUTTER: f32 = 8.0;

pub fn show(ctx: &Context, app: &mut App) {
    let space_pressed = ctx.input(|input| input.key_pressed(egui::Key::Space));
    let delete_pressed = ctx.input(|input| input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace));
    let left_pressed = ctx.input(|input| input.key_pressed(egui::Key::ArrowLeft));
    let right_pressed = ctx.input(|input| input.key_pressed(egui::Key::ArrowRight));
    let up_pressed = ctx.input(|input| input.key_pressed(egui::Key::ArrowUp));
    let down_pressed = ctx.input(|input| input.key_pressed(egui::Key::ArrowDown));
    let wants_keyboard = ctx.wants_keyboard_input();
    if space_pressed && !app.sequencer_popup_open {
        app.toggle_playback();
    }
    if delete_pressed && !wants_keyboard && !app.sequencer_popup_open && app.selected_clip.is_some() {
        app.delete_selected_clip();
    }
    if !wants_keyboard && !app.sequencer_popup_open {
        if left_pressed || up_pressed {
            app.set_playhead((app.playhead_beats - 0.25).max(0.0));
        }
        if right_pressed || down_pressed {
            app.set_playhead(app.playhead_beats + 0.25);
        }
    }

    top_bar::show(ctx, app);

    egui::SidePanel::right("inspector_panel")
        .resizable(false)
        .default_width(294.0)
        .show_separator_line(false)
        .frame(
            egui::Frame::none()
                .fill(theme::BACKGROUND)
                .inner_margin(egui::Margin {
                    left: WORKSPACE_GUTTER,
                    right: WORKSPACE_GUTTER,
                    top: 6.0,
                    bottom: WORKSPACE_GUTTER,
                }),
        )
        .show(ctx, |ui| {
            let available = ui.available_size();
            theme::panel_frame().show(ui, |ui| {
                ui.set_min_width((available.x - 26.0).max(0.0));
                ui.set_min_height((available.y - 26.0).max(0.0));
                inspector::show(ui, app);
            });
        });

    egui::CentralPanel::default().frame(egui::Frame::none().fill(theme::BACKGROUND)).show(ctx, |ui| {
        let rect = ui.max_rect();
        let total_height = rect.height();
        let splitter_height = 8.0;
        let desired_top = (total_height * 0.36).clamp(300.0, 420.0);
        if app.reset_session_layout {
            let preferred_bottom = (total_height - desired_top - splitter_height).max(320.0);
            app.pedalboard_ratio = (preferred_bottom / total_height).clamp(0.15, 0.75);
            app.reset_session_layout = false;
        }

        let min_top = desired_top.min((total_height - 320.0 - splitter_height).max(140.0));
        let min_bottom = 320.0_f32.min((total_height - min_top - splitter_height).max(120.0));

        let mut bottom_height = (total_height * app.pedalboard_ratio)
            .clamp(min_bottom, (total_height - min_top - splitter_height).max(min_bottom));

        let top_height = (total_height - bottom_height - splitter_height).max(min_top);
        let top_rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), top_height));
        let splitter_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left(), top_rect.bottom()),
            egui::vec2(rect.width(), splitter_height),
        );
        let bottom_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left(), splitter_rect.bottom()),
            rect.max,
        );

        let splitter_response = ui.interact(
            splitter_rect,
            ui.id().with("session_splitter"),
            egui::Sense::click_and_drag(),
        );
        if splitter_response.hovered() || splitter_response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if splitter_response.dragged() {
            bottom_height = (bottom_height - splitter_response.drag_delta().y)
                .clamp(min_bottom, (total_height - min_top - splitter_height).max(min_bottom));
            app.pedalboard_ratio = (bottom_height / total_height).clamp(0.15, 0.75);
        }

        ui.painter().rect_filled(splitter_rect, 0.0, theme::BACKGROUND);
        let handle = egui::Rect::from_center_size(splitter_rect.center(), egui::vec2(52.0, 3.0));
        ui.painter().rect_filled(handle, 2.0, if splitter_response.hovered() { theme::ACCENT } else { theme::BORDER_STRONG });

        let top_content_rect = egui::Rect::from_min_max(
            egui::pos2(top_rect.left() + WORKSPACE_GUTTER, top_rect.top()),
            top_rect.max,
        );
        let mut top_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(top_content_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        timeline::show(&mut top_ui, ctx, app);

        let bottom_content_rect = egui::Rect::from_min_max(
            egui::pos2(bottom_rect.left() + WORKSPACE_GUTTER, bottom_rect.top()),
            egui::pos2(bottom_rect.right(), bottom_rect.bottom() - WORKSPACE_GUTTER),
        );
        let mut bottom_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bottom_content_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        pedalboard::show(&mut bottom_ui, app);
    });

    transport_popup::show(ctx, app);
    metronome_popup::show(ctx, app);
    input_settings_popup::show(ctx, app);
    rename_project_popup::show(ctx, app);
    sequencer_popup::show(ctx, app);
}
