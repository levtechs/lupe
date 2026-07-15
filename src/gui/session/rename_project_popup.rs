use eframe::egui::{self, Context, RichText};

use crate::{app::App, gui::theme};

pub fn show(ctx: &Context, app: &mut App) {
    if !app.rename_project_popup_open {
        return;
    }

    let mut open = true;
    egui::Window::new("Rename project")
        .open(&mut open)
        .resizable(false)
        .default_width(340.0)
        .frame(theme::panel_frame())
        .show(ctx, |ui| {
            ui.label(RichText::new("PROJECT NAME").small().strong().color(theme::MUTED));
            ui.add_sized([ui.available_width(), 28.0], egui::TextEdit::singleline(&mut app.rename_project_draft));

            let trimmed = app.rename_project_draft.trim();
            let available = !trimmed.is_empty() && app.rename_project_name_available();
            let availability_text = if trimmed.is_empty() {
                "Enter a project name".to_string()
            } else if available {
                "Name available".to_string()
            } else {
                "Name already taken".to_string()
            };
            let availability_color = if available {
                theme::SUCCESS
            } else {
                theme::DANGER
            };
            ui.colored_label(availability_color, availability_text);

            if let Some(error) = &app.rename_project_error {
                ui.label(RichText::new(error).color(theme::DANGER));
            }

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(available, egui::Button::new("Rename").fill(theme::ACCENT.gamma_multiply(0.65))).clicked() {
                    app.confirm_rename_project();
                }
                if ui.button("Cancel").clicked() {
                    app.rename_project_popup_open = false;
                }
            });
        });

    app.rename_project_popup_open &= open;
}
