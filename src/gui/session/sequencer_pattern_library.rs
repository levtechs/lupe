use eframe::egui::{self, RichText, Ui};

use crate::app::{App, PatternApplyMode, PatternBrowserFilter};
use crate::gui::theme;
use crate::content::PatternKind;

const FILTERS: [(PatternBrowserFilter, &str); 3] = [
    (PatternBrowserFilter::Loops, "Loops"),
    (PatternBrowserFilter::Fills, "Fills"),
    (PatternBrowserFilter::MyPatterns, "My Patterns"),
];

pub fn show(ui: &mut Ui, app: &mut App, available_height: f32) {
    theme::card_frame().show(ui, |ui| {
        ui.set_min_height(available_height);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Pattern library").strong().size(16.0));
            for (filter, label) in FILTERS {
                if ui.selectable_label(app.pattern_browser_filter == filter, label).clicked() {
                    app.pattern_browser_filter = filter;
                }
            }
            ui.checkbox(&mut app.pattern_show_all, "Show all");
            if ui.small_button("Empty pattern").clicked() {
                app.start_empty_sequence();
            }
        });

        let search = ui.add(
            egui::TextEdit::singleline(&mut app.pattern_browser_query)
                .hint_text("Search style, name, or tempo")
                .desired_width(ui.available_width()),
        );
        app.sequencer_text_input_active |= search.has_focus();

        if app.pattern_browser_filter == PatternBrowserFilter::MyPatterns {
            ui.horizontal_wrapped(|ui| {
                ui.label("Save current as");
                ui.selectable_value(&mut app.user_pattern_kind, PatternKind::Loop, "Loop");
                ui.selectable_value(&mut app.user_pattern_kind, PatternKind::Fill, "Fill");
                let name = ui.add(
                    egui::TextEdit::singleline(&mut app.user_pattern_name)
                        .desired_width((ui.available_width() - 74.0).max(100.0)),
                );
                app.sequencer_text_input_active |= name.has_focus();
                if ui.button("Save").clicked() {
                    app.save_current_user_pattern();
                }
            });
        }

        let query = app.pattern_browser_query.trim().to_ascii_lowercase();
        let patterns = app
            .content_registry
            .patterns()
            .iter()
            .enumerate()
            .filter(|(_, pattern)| app.pattern_browser_filter.matches(pattern.kind, pattern.user_owned))
            .filter(|(_, pattern)| app.pattern_show_all || pattern.featured || pattern.user_owned)
            .filter(|(_, pattern)| {
                query.is_empty()
                    || pattern.name.to_ascii_lowercase().contains(&query)
                    || pattern.style.to_ascii_lowercase().contains(&query)
                    || pattern.bpm.to_string().contains(&query)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();

        if patterns.is_empty() {
            ui.label(RichText::new("No patterns match this view.").color(theme::MUTED));
        } else {
            egui::ScrollArea::vertical()
                .id_salt(("sequencer_pattern_results", app.pattern_browser_filter.label()))
                .max_height((available_height - 118.0).max(180.0))
                .auto_shrink([false, false])
                .show_rows(ui, 128.0, patterns.len(), |ui, rows| {
                for row in rows {
                    let Some(pattern_index) = patterns.get(row).copied() else {
                        continue;
                    };
                    let pattern = app.content_registry.patterns()[pattern_index].clone();
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 124.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            egui::Frame::none()
                                .fill(theme::CANVAS)
                                .stroke(egui::Stroke::new(1.0, theme::BORDER))
                                .rounding(6.0)
                                .inner_margin(egui::Margin::symmetric(8.0, 6.0))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.add(egui::Label::new(RichText::new(&pattern.name).strong()).truncate());
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(format!(
                                                "{} · {} BPM · {}/4 · {}",
                                                title_case(&pattern.style),
                                                pattern.bpm,
                                                pattern.beats_per_bar,
                                                pattern.kind.label()
                                            ))
                                            .small()
                                            .color(theme::MUTED),
                                        )
                                        .truncate(),
                                    );
                                    ui.add(
                                        egui::Label::new(RichText::new(&pattern.description).small().color(theme::MUTED))
                                            .truncate(),
                                    );
                                    ui.horizontal_wrapped(|ui| {
                                        let previewing = app.is_library_pattern_previewing(&pattern.id);
                                        if ui.small_button(if previewing { "Stop" } else { "Preview" }).clicked() {
                                            app.preview_library_pattern(&pattern.id);
                                        }
                                        if ui.small_button("Replace").clicked() {
                                            app.apply_pattern(&pattern.id, PatternApplyMode::Replace);
                                        }
                                        if ui.small_button("Append").clicked() {
                                            app.apply_pattern(&pattern.id, PatternApplyMode::Append);
                                        }
                                        if ui.small_button("Overlay").clicked() {
                                            app.apply_pattern(&pattern.id, PatternApplyMode::Overlay);
                                        }
                                    });
                                });
                        },
                    );
                }
                });
        }
    });
}

fn title_case(value: &str) -> String {
    value
        .split(['-', '_', '/'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            characters
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + characters.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
