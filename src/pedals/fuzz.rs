use serde::{Deserialize, Serialize};

use super::{PedalDefinition, PedalKind};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FuzzPedal {
    pub enabled: bool,
    pub sustain: f32,
    pub bias: f32,
    pub level: f32,
}

impl Default for FuzzPedal {
    fn default() -> Self {
        Self {
            enabled: true,
            sustain: 2.4,
            bias: 0.0,
            level: 0.78,
        }
    }
}

impl PedalDefinition for FuzzPedal {
    fn kind(&self) -> PedalKind { PedalKind::Fuzz }
    fn display_name(&self) -> &'static str { "Fuzz" }
    fn description(&self) -> &'static str { "Heavy clipped sustain with a raw fuzz character." }
    fn accent_rgb(&self) -> (u8, u8, u8) { (214, 92, 112) }
    fn enabled(&self) -> bool { self.enabled }
    fn toggle_enabled(&mut self) { self.enabled = !self.enabled; }
    fn summary(&self) -> String {
        format!("sustain {:.1}  bias {}%  level {}%", self.sustain, (((self.bias + 1.0) * 50.0).round() as i32), (self.level * 100.0).round() as i32)
    }
    fn param_count(&self) -> usize { 3 }
    fn param_name(&self, index: usize) -> &'static str {
        match index { 0 => "Sustain", 1 => "Bias", _ => "Level" }
    }
    fn param_value(&self, index: usize) -> String {
        match index {
            0 => format!("{:.1}x", self.sustain),
            1 => format!("{}%", ((self.bias + 1.0) * 50.0).round() as i32),
            _ => format!("{}%", (self.level * 100.0).round() as i32),
        }
    }
    fn step_param(&mut self, index: usize, delta: i32) {
        match index {
            0 => self.sustain = (self.sustain + delta as f32 * 0.25).clamp(1.0, 8.0),
            1 => self.bias = (self.bias + delta as f32 * 0.08).clamp(-1.0, 1.0),
            _ => self.level = (self.level + delta as f32 * 0.05).clamp(0.0, 1.2),
        }
    }
}

pub struct FuzzState;

impl FuzzState {
    pub fn new() -> Self { Self }

    pub fn process(&mut self, input: f32, pedal: &FuzzPedal) -> f32 {
        let biased = input * pedal.sustain + pedal.bias * 0.4;
        let clipped = biased.clamp(-0.45, 0.45);
        ((clipped / 0.45) * pedal.level).clamp(-1.0, 1.0)
    }
}
