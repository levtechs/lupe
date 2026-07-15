use eframe::egui::{self, Color32, FontFamily, FontId, Frame, Margin, Rounding, Stroke, TextStyle, Visuals};

pub const BACKGROUND: Color32 = Color32::from_rgb(15, 18, 24);
pub const PANEL: Color32 = Color32::from_rgb(24, 27, 34);
pub const CANVAS: Color32 = Color32::from_rgb(18, 21, 28);
pub const CARD: Color32 = Color32::from_rgb(30, 34, 43);
pub const CARD_HOVER: Color32 = Color32::from_rgb(38, 43, 54);
pub const BORDER: Color32 = Color32::from_rgb(47, 53, 66);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(67, 75, 92);
pub const TEXT: Color32 = Color32::from_rgb(226, 229, 236);
pub const MUTED: Color32 = Color32::from_rgb(145, 151, 166);
pub const ACCENT: Color32 = Color32::from_rgb(102, 142, 224);
pub const SUCCESS: Color32 = Color32::from_rgb(75, 151, 123);
pub const WARNING: Color32 = Color32::from_rgb(224, 178, 82);
pub const DANGER: Color32 = Color32::from_rgb(220, 112, 118);

pub fn panel_frame() -> Frame {
    Frame::none()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, BORDER))
        .rounding(Rounding::same(9.0))
        .inner_margin(Margin::same(13.0))
}

pub fn card_frame() -> Frame {
    Frame::none()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .rounding(Rounding::same(7.0))
        .inner_margin(Margin::same(11.0))
}

pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = CANVAS;
    visuals.faint_bg_color = CARD;
    visuals.widgets.noninteractive.bg_fill = CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(39, 43, 53);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.bg_fill = CARD_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.widgets.active.bg_fill = Color32::from_rgb(59, 86, 143);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.widgets.open.bg_fill = CARD_HOVER;
    visuals.selection.bg_fill = Color32::from_rgb(56, 85, 145);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.window_rounding = Rounding::same(10.0);
    visuals.window_stroke = Stroke::new(1.0, BORDER_STRONG);
    visuals.widgets.noninteractive.rounding = Rounding::same(6.0);
    visuals.widgets.inactive.rounding = Rounding::same(6.0);
    visuals.widgets.hovered.rounding = Rounding::same(6.0);
    visuals.widgets.active.rounding = Rounding::same(6.0);
    visuals.widgets.open.rounding = Rounding::same(6.0);
    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.window_margin = Margin::same(12.0);
        style.text_styles.insert(TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Button, FontId::new(13.0, FontFamily::Proportional));
        style.text_styles.insert(TextStyle::Small, FontId::new(11.0, FontFamily::Proportional));
    });
}
