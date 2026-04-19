use serde::{Deserialize, Serialize};

use super::{PedalDefinition, PedalKind};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EqualizerPedal {
    pub enabled: bool,
    pub low_gain: f32,
    pub mid_gain: f32,
    pub high_gain: f32,
}

impl Default for EqualizerPedal {
    fn default() -> Self {
        Self {
            enabled: true,
            low_gain: 1.0,
            mid_gain: 1.0,
            high_gain: 1.0,
        }
    }
}

impl PedalDefinition for EqualizerPedal {
    fn kind(&self) -> PedalKind {
        PedalKind::Equalizer
    }

    fn display_name(&self) -> &'static str {
        "Equalizer"
    }

    fn description(&self) -> &'static str {
        "Shape lows, mids, and highs before monitoring or recording."
    }

    fn accent_rgb(&self) -> (u8, u8, u8) {
        (68, 138, 255)
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn toggle_enabled(&mut self) {
        self.enabled = !self.enabled;
    }

    fn summary(&self) -> String {
        format!(
            "low {:.1}  mid {:.1}  high {:.1}",
            self.low_gain, self.mid_gain, self.high_gain
        )
    }

    fn param_count(&self) -> usize {
        3
    }

    fn param_name(&self, index: usize) -> &'static str {
        match index {
            0 => "Low",
            1 => "Mid",
            _ => "High",
        }
    }

    fn param_value(&self, index: usize) -> String {
        match index {
            0 => format!("{:.1}x", self.low_gain),
            1 => format!("{:.1}x", self.mid_gain),
            _ => format!("{:.1}x", self.high_gain),
        }
    }

    fn step_param(&mut self, index: usize, delta: i32) {
        match index {
            0 => self.low_gain = (self.low_gain + delta as f32 * 0.1).clamp(0.0, 2.0),
            1 => self.mid_gain = (self.mid_gain + delta as f32 * 0.1).clamp(0.0, 2.0),
            _ => self.high_gain = (self.high_gain + delta as f32 * 0.1).clamp(0.0, 2.0),
        }
    }
}

pub struct EqualizerState {
    low_state: f32,
    mid_state: f32,
}

impl EqualizerState {
    pub fn new() -> Self {
        Self {
            low_state: 0.0,
            mid_state: 0.0,
        }
    }

    pub fn process(&mut self, input: f32, pedal: &EqualizerPedal) -> f32 {
        let low_alpha = 0.045;
        let mid_alpha = 0.18;

        self.low_state += (input - self.low_state) * low_alpha;
        self.mid_state += (input - self.mid_state) * mid_alpha;

        let low = self.low_state;
        let high = input - self.mid_state;
        let mid = input - low - high;

        (low * pedal.low_gain + mid * pedal.mid_gain + high * pedal.high_gain).clamp(-1.0, 1.0)
    }
}
