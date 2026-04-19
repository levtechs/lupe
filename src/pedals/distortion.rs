use serde::{Deserialize, Serialize};

use super::{PedalDefinition, PedalKind};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DistortionPedal {
    pub enabled: bool,
    pub drive: f32,
    pub tone: f32,
    pub level: f32,
}

impl Default for DistortionPedal {
    fn default() -> Self {
        Self {
            enabled: true,
            drive: 1.8,
            tone: 0.55,
            level: 0.8,
        }
    }
}

impl PedalDefinition for DistortionPedal {
    fn kind(&self) -> PedalKind { PedalKind::Distortion }
    fn display_name(&self) -> &'static str { "Distortion" }
    fn description(&self) -> &'static str { "Amp-like clipping and bite for driven tones." }
    fn accent_rgb(&self) -> (u8, u8, u8) { (242, 148, 68) }
    fn enabled(&self) -> bool { self.enabled }
    fn toggle_enabled(&mut self) { self.enabled = !self.enabled; }
    fn summary(&self) -> String {
        format!("drive {:.1}  tone {}%  level {}%", self.drive, (self.tone * 100.0).round() as i32, (self.level * 100.0).round() as i32)
    }
    fn param_count(&self) -> usize { 3 }
    fn param_name(&self, index: usize) -> &'static str {
        match index { 0 => "Drive", 1 => "Tone", _ => "Level" }
    }
    fn param_value(&self, index: usize) -> String {
        match index {
            0 => format!("{:.1}x", self.drive),
            1 => format!("{}%", (self.tone * 100.0).round() as i32),
            _ => format!("{}%", (self.level * 100.0).round() as i32),
        }
    }
    fn step_param(&mut self, index: usize, delta: i32) {
        match index {
            0 => self.drive = (self.drive + delta as f32 * 0.2).clamp(0.8, 6.0),
            1 => self.tone = (self.tone + delta as f32 * 0.05).clamp(0.0, 1.0),
            _ => self.level = (self.level + delta as f32 * 0.05).clamp(0.0, 1.2),
        }
    }
}

pub struct DistortionState {
    tone_state: f32,
}

impl DistortionState {
    pub fn new() -> Self {
        Self { tone_state: 0.0 }
    }

    pub fn process(&mut self, input: f32, pedal: &DistortionPedal) -> f32 {
        let driven = (input * pedal.drive).tanh();
        let tone = pedal.tone.clamp(0.0, 1.0);
        self.tone_state += (driven - self.tone_state) * (0.03 + tone * 0.27);
        let high = driven - self.tone_state;
        ((self.tone_state * (1.0 - tone) + high * tone) * pedal.level).clamp(-1.0, 1.0)
    }
}
