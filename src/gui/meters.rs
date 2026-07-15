use eframe::egui::{self, Ui};

use crate::gui::{theme, util};

pub fn peak_strip(ui: &mut Ui, label: &str, level: f32) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new(label).small().weak());
        let (rect, _resp) = ui.allocate_exact_size(egui::vec2(120.0, 10.0), egui::Sense::hover());
        let fill_w = rect.width() * level.clamp(0.0, 1.0);
        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(fill_w, rect.height()));
        ui.painter().rect_filled(rect, 3.0, theme::CANVAS);
        ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(1.0, theme::BORDER));
        if fill_w > 1.0 {
            ui.painter().rect_filled(fill, 3.0, util::meter_color(level));
        }
    });
}
