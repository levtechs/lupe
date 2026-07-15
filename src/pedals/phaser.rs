use std::f32::consts::TAU;

use serde::{Deserialize, Serialize};

use super::{PedalDefinition, PedalKind};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PhaserPedal {
    pub enabled: bool,
    pub rate_hz: f32,
    pub depth: f32,
    pub mix: f32,
}

impl Default for PhaserPedal {
    fn default() -> Self {
        Self {
            enabled: true,
            rate_hz: 0.55,
            depth: 0.7,
            mix: 0.55,
        }
    }
}

impl PedalDefinition for PhaserPedal {
    fn kind(&self) -> PedalKind { PedalKind::Phaser }
    fn display_name(&self) -> &'static str { "Phaser" }
    fn description(&self) -> &'static str { "Animated phase sweep for motion and width." }
    fn accent_rgb(&self) -> (u8, u8, u8) { (104, 220, 188) }
    fn enabled(&self) -> bool { self.enabled }
    fn toggle_enabled(&mut self) { self.enabled = !self.enabled; }
    fn summary(&self) -> String {
        format!("rate {:.2}hz  depth {}%  mix {}%", self.rate_hz, (self.depth * 100.0).round() as i32, (self.mix * 100.0).round() as i32)
    }
    fn param_count(&self) -> usize { 3 }
    fn param_name(&self, index: usize) -> &'static str {
        match index { 0 => "Rate", 1 => "Depth", _ => "Mix" }
    }
    fn param_value(&self, index: usize) -> String {
        match index {
            0 => format!("{:.2}hz", self.rate_hz),
            1 => format!("{}%", (self.depth * 100.0).round() as i32),
            _ => format!("{}%", (self.mix * 100.0).round() as i32),
        }
    }
    fn step_param(&mut self, index: usize, delta: i32) {
        match index {
            0 => self.rate_hz = (self.rate_hz + delta as f32 * 0.08).clamp(0.05, 4.0),
            1 => self.depth = (self.depth + delta as f32 * 0.05).clamp(0.0, 1.0),
            _ => self.mix = (self.mix + delta as f32 * 0.05).clamp(0.0, 1.0),
        }
    }
}

pub struct PhaserState {
    sample_rate: f32,
    phase: f32,
    z1: f32,
    z2: f32,
}

impl PhaserState {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate: sample_rate as f32,
            phase: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn process(&mut self, input: f32, pedal: &PhaserPedal) -> f32 {
        self.phase = (self.phase + TAU * pedal.rate_hz / self.sample_rate) % TAU;
        let lfo = (self.phase.sin() * 0.5 + 0.5) * pedal.depth;
        let a = (0.15 + lfo * 0.7).clamp(0.05, 0.95);

        let stage1 = -a * input + self.z1;
        self.z1 = input + a * stage1;
        let stage2 = -a * stage1 + self.z2;
        self.z2 = stage1 + a * stage2;

        input * (1.0 - pedal.mix) + stage2 * pedal.mix
    }
}
