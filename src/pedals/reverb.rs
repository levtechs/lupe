use serde::{Deserialize, Serialize};

use super::{PedalDefinition, PedalKind};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReverbPedal {
    pub enabled: bool,
    pub mix: f32,
    pub decay: f32,
    pub tone: f32,
}

impl Default for ReverbPedal {
    fn default() -> Self {
        Self {
            enabled: true,
            mix: 0.35,
            decay: 0.62,
            tone: 0.55,
        }
    }
}

impl PedalDefinition for ReverbPedal {
    fn kind(&self) -> PedalKind {
        PedalKind::Reverb
    }

    fn display_name(&self) -> &'static str {
        "Reverb"
    }

    fn description(&self) -> &'static str {
        "Add ambience and tail to the monitored input."
    }

    fn accent_rgb(&self) -> (u8, u8, u8) {
        (168, 96, 238)
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn toggle_enabled(&mut self) {
        self.enabled = !self.enabled;
    }

    fn summary(&self) -> String {
        format!(
            "mix {:>3}%  decay {:>3}%  tone {:>3}%",
            (self.mix * 100.0).round() as i32,
            (self.decay * 100.0).round() as i32,
            (self.tone * 100.0).round() as i32
        )
    }

    fn param_count(&self) -> usize {
        3
    }

    fn param_name(&self, index: usize) -> &'static str {
        match index {
            0 => "Mix",
            1 => "Decay",
            _ => "Tone",
        }
    }

    fn param_value(&self, index: usize) -> String {
        match index {
            0 => format!("{}%", (self.mix * 100.0).round() as i32),
            1 => format!("{}%", (self.decay * 100.0).round() as i32),
            _ => format!("{}%", (self.tone * 100.0).round() as i32),
        }
    }

    fn step_param(&mut self, index: usize, delta: i32) {
        match index {
            0 => self.mix = (self.mix + delta as f32 * 0.05).clamp(0.0, 1.0),
            1 => self.decay = (self.decay + delta as f32 * 0.05).clamp(0.1, 0.92),
            _ => self.tone = (self.tone + delta as f32 * 0.05).clamp(0.05, 0.95),
        }
    }
}

pub struct ReverbState {
    buffer_a: Vec<f32>,
    buffer_b: Vec<f32>,
    buffer_c: Vec<f32>,
    idx_a: usize,
    idx_b: usize,
    idx_c: usize,
    damp_a: f32,
    damp_b: f32,
    damp_c: f32,
}

impl ReverbState {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(8_000) as usize;
        Self {
            buffer_a: vec![0.0; (sr / 41).max(1)],
            buffer_b: vec![0.0; (sr / 31).max(1)],
            buffer_c: vec![0.0; (sr / 23).max(1)],
            idx_a: 0,
            idx_b: 0,
            idx_c: 0,
            damp_a: 0.0,
            damp_b: 0.0,
            damp_c: 0.0,
        }
    }

    pub fn process(&mut self, input: f32, pedal: &ReverbPedal) -> f32 {
        let tap_a = self.buffer_a[self.idx_a];
        let tap_b = self.buffer_b[self.idx_b];
        let tap_c = self.buffer_c[self.idx_c];
        let damp = pedal.tone.clamp(0.05, 0.95);

        self.damp_a += (tap_a - self.damp_a) * damp;
        self.damp_b += (tap_b - self.damp_b) * damp;
        self.damp_c += (tap_c - self.damp_c) * damp;

        let decay_a = pedal.decay.clamp(0.1, 0.92);
        let decay_b = (pedal.decay * 0.92).clamp(0.1, 0.9);
        let decay_c = (pedal.decay * 0.84).clamp(0.1, 0.88);

        self.buffer_a[self.idx_a] = input + self.damp_a * decay_a;
        self.buffer_b[self.idx_b] = input + self.damp_b * decay_b;
        self.buffer_c[self.idx_c] = input + self.damp_c * decay_c;

        self.idx_a = (self.idx_a + 1) % self.buffer_a.len();
        self.idx_b = (self.idx_b + 1) % self.buffer_b.len();
        self.idx_c = (self.idx_c + 1) % self.buffer_c.len();

        let wet = (self.damp_a + self.damp_b + self.damp_c) * 0.2;
        input * (1.0 - pedal.mix) + wet * pedal.mix
    }
}
