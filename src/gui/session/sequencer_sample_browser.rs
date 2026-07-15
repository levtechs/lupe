use eframe::egui::{self, RichText, Ui};

use crate::app::{App, SampleBrowserEntry, SampleBrowserFilter};
use crate::gui::theme;

const FILTERS: [SampleBrowserFilter; 8] = [
    SampleBrowserFilter::All,
    SampleBrowserFilter::Kick,
    SampleBrowserFilter::Snare,
    SampleBrowserFilter::Hat,
    SampleBrowserFilter::Tom,
    SampleBrowserFilter::Cymbal,
    SampleBrowserFilter::Perc,
    SampleBrowserFilter::Fx,
];

pub fn show(ui: &mut Ui, app: &mut App, available_height: f32) {
    app.poll_sample_browser_entries();
    if app.sample_browser_loading() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
    let installed = app.sample_browser_entries.len();
    theme::card_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.set_min_height(available_height);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Choose a sound").strong().size(16.0));
            ui.label(
                RichText::new(format!("{installed} samples · {} kits", app.content_registry.kit_packs().len()))
                    .small()
                    .color(theme::MUTED),
            );
            if ui.small_button("Rescan").clicked() {
                app.mark_sample_browser_dirty();
            }
            if app.sample_browser_loading() {
                ui.spinner();
                ui.label(RichText::new("Scanning…").small().color(theme::MUTED));
            }
            if ui.small_button("Close").clicked() {
                app.close_sample_browser();
            }
        });
        if app.sample_browser_target_lane.is_some() {
            ui.checkbox(&mut app.sample_browser_add_variant, "Add as alternate recording");
        }

        let search = ui.add(
            egui::TextEdit::singleline(&mut app.sample_browser_query)
                .hint_text("Search installed sounds")
                .desired_width(ui.available_width()),
        );
        if search.changed() {
            app.sample_browser_selected_row = 0;
            app.sample_browser_scroll_to_selected = true;
        }
        app.sequencer_text_input_active |= search.has_focus();
        ui.horizontal_wrapped(|ui| {
            for filter in FILTERS {
                if ui.selectable_label(app.sample_browser_filter == filter, filter.label()).clicked() {
                    app.sample_browser_filter = filter;
                    app.sample_browser_selected_row = 0;
                    app.sample_browser_scroll_to_selected = true;
                }
            }
        });

        let filtered = filtered_entries(app);
        if app.sample_browser_selected_row >= filtered.len() {
            app.sample_browser_selected_row = filtered.len().saturating_sub(1);
        }
        handle_list_shortcuts(ui, app, &filtered);
        if filtered.is_empty() {
            ui.label(
                RichText::new(if app.sample_browser_loading() {
                    "Scanning installed sounds…"
                } else {
                    "No installed sounds match this search."
                })
                .color(theme::MUTED),
            );
        } else {
            egui::ScrollArea::vertical()
                .id_salt("sequencer_sample_results")
                .max_height((available_height - 154.0).max(180.0))
                .auto_shrink([false, false])
                .show_rows(ui, 108.0, filtered.len(), |ui, rows| {
                for row in rows {
                    let Some(entry_index) = filtered.get(row).copied() else {
                        continue;
                    };
                    let Some(entry) = app.sample_browser_entries.get(entry_index).cloned() else {
                        continue;
                    };
                    show_sample_row(ui, app, &entry, row);
                }
                });
        }
    });
}

fn filtered_entries(app: &App) -> Vec<usize> {
    let query = app.sample_browser_query.trim().to_ascii_lowercase();
    app.sample_browser_entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            (app.sample_browser_filter == SampleBrowserFilter::All || entry.category == app.sample_browser_filter)
                && (query.is_empty()
                    || entry.title.to_ascii_lowercase().contains(&query)
                    || entry.folder.to_ascii_lowercase().contains(&query))
        })
        .map(|(index, _)| index)
        .collect()
}

fn handle_list_shortcuts(ui: &mut Ui, app: &mut App, filtered: &[usize]) {
    if filtered.is_empty() || app.sequencer_text_input_active {
        return;
    }
    if ui.ctx().input(|input| input.key_pressed(egui::Key::ArrowDown)) {
        app.sample_browser_selected_row = (app.sample_browser_selected_row + 1).min(filtered.len() - 1);
        app.sample_browser_scroll_to_selected = true;
    }
    if ui.ctx().input(|input| input.key_pressed(egui::Key::ArrowUp)) {
        app.sample_browser_selected_row = app.sample_browser_selected_row.saturating_sub(1);
        app.sample_browser_scroll_to_selected = true;
    }
    if ui.ctx().input(|input| input.key_pressed(egui::Key::Enter)) {
        if let Some(entry) = filtered
            .get(app.sample_browser_selected_row)
            .and_then(|index| app.sample_browser_entries.get(*index))
        {
            app.preview_sample_file(entry.path.clone());
        }
    }
}

fn show_sample_row(ui: &mut Ui, app: &mut App, entry: &SampleBrowserEntry, row: usize) {
    let selected = app.sample_browser_selected_row == row;
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 104.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
        if selected && app.sample_browser_scroll_to_selected {
            ui.scroll_to_rect(ui.max_rect(), Some(egui::Align::Center));
            app.sample_browser_scroll_to_selected = false;
        }
        egui::Frame::none()
            .fill(if selected { theme::ACCENT.gamma_multiply(0.18) } else { theme::CANVAS })
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .rounding(5.0)
            .inner_margin(egui::Margin::symmetric(8.0, 5.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let response = ui.add(egui::SelectableLabel::new(selected, RichText::new(&entry.title).strong()));
                ui.add(egui::Label::new(RichText::new(&entry.folder).small().color(theme::MUTED)).truncate());
                ui.horizontal_wrapped(|ui| {
                    let previewing = app.sample_preview_path.as_deref() == Some(entry.path.as_str());
                    if response.clicked() || ui.small_button(if previewing { "Stop" } else { "Preview" }).clicked() {
                        app.sample_browser_selected_row = row;
                        app.preview_sample_file(entry.path.clone());
                    }
                    let action = if app.sample_browser_add_variant {
                        "Add alternate"
                    } else if app.sample_browser_target_lane.is_some() {
                        "Use sound"
                    } else {
                        "+ Lane"
                    };
                    if ui.small_button(action).clicked() {
                        app.sample_browser_selected_row = row;
                        app.add_sequence_lane_from_sample(entry.path.clone());
                        app.close_sample_browser();
                    }
                });
            });
    });
}
