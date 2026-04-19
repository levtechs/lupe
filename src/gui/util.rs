use eframe::egui::Color32;

use crate::project::TrackColor;

pub fn track_accent(color: TrackColor) -> Color32 {
    match color {
        TrackColor::Blue => Color32::from_rgb(88, 148, 242),
        TrackColor::Green => Color32::from_rgb(110, 210, 140),
        TrackColor::Yellow => Color32::from_rgb(240, 200, 90),
        TrackColor::Magenta => Color32::from_rgb(220, 120, 220),
        TrackColor::Cyan => Color32::from_rgb(100, 210, 230),
        TrackColor::Red => Color32::from_rgb(240, 110, 110),
    }
}

pub fn meter_color(level: f32) -> Color32 {
    let level = level.clamp(0.0, 1.0);
    let r = (level * 220.0) as u8;
    let g = ((1.0 - level) * 200.0 + 40.0) as u8;
    Color32::from_rgb(r, g, 70)
}
