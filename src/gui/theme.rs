use eframe::egui::{self, Color32, Rounding, Stroke, Visuals};

pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = Color32::from_rgb(22, 24, 30);
    visuals.window_fill = Color32::from_rgb(32, 35, 44);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(28, 30, 38);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(40, 44, 56);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(52, 58, 74);
    visuals.widgets.active.bg_fill = Color32::from_rgb(64, 110, 168);
    visuals.selection.bg_fill = Color32::from_rgb(56, 96, 150);
    visuals.window_rounding = Rounding::same(10.0);
    visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(60, 66, 82));
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 6.0);
    });
}
