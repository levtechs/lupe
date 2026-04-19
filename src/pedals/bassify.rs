use serde::{Deserialize, Serialize};

use super::{PedalDefinition, PedalKind};

const MAX_DELAY: usize = 8192;
const MIN_DELAY: f32 = 256.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BassifyPedal {
    pub enabled: bool,
    pub sub_mix: f32,
    pub smoothness: f32,
    pub level: f32,
}

impl Default for BassifyPedal {
    fn default() -> Self {
        Self {
            enabled: true,
            sub_mix: 0.55,
            smoothness: 0.7,
            level: 0.82,
        }
    }
}

impl PedalDefinition for BassifyPedal {
    fn kind(&self) -> PedalKind { PedalKind::Bassify }
    fn display_name(&self) -> &'static str { "Bassify" }
    fn description(&self) -> &'static str { "Octave-down pitch shifter. Best on single notes; still synthetic on voice, but now shifts pitch instead of only generating rumble." }
    fn accent_rgb(&self) -> (u8, u8, u8) { (218, 202, 86) }
    fn enabled(&self) -> bool { self.enabled }
    fn toggle_enabled(&mut self) { self.enabled = !self.enabled; }
    fn summary(&self) -> String {
        format!(
            "octave {}%  blur {}%  output {}%",
            (self.sub_mix * 100.0).round() as i32,
            (self.smoothness * 100.0).round() as i32,
            (self.level * 100.0).round() as i32
        )
    }
    fn param_count(&self) -> usize { 3 }
    fn param_name(&self, index: usize) -> &'static str {
        match index { 0 => "Octave", 1 => "Blur", _ => "Output" }
    }
    fn param_value(&self, index: usize) -> String {
        match index {
            0 => format!("{}%", (self.sub_mix * 100.0).round() as i32),
            1 => format!("{}%", (self.smoothness * 100.0).round() as i32),
            _ => format!("{}%", (self.level * 100.0).round() as i32),
        }
    }
    fn step_param(&mut self, index: usize, delta: i32) {
        match index {
            0 => self.sub_mix = (self.sub_mix + delta as f32 * 0.05).clamp(0.0, 1.0),
            1 => self.smoothness = (self.smoothness + delta as f32 * 0.05).clamp(0.0, 1.0),
            _ => self.level = (self.level + delta as f32 * 0.05).clamp(0.0, 1.2),
        }
    }
}

pub struct BassifyState {
    buffer: [f32; MAX_DELAY],
    write_index: usize,
    phase: f32,
    lowpass: f32,
}

impl BassifyState {
    pub fn new() -> Self {
        Self {
            buffer: [0.0; MAX_DELAY],
            write_index: 0,
            phase: 0.0,
            lowpass: 0.0,
        }
    }

    pub fn process(&mut self, input: f32, pedal: &BassifyPedal) -> f32 {
        self.buffer[self.write_index] = input;

        let shifted = self.shifted_sample();

        // Extra smoothing helps suppress zipper artifacts from the delay sweep.
        let blur = 0.008 + pedal.smoothness * 0.045;
        self.lowpass += (shifted - self.lowpass) * blur;

        self.write_index = (self.write_index + 1) % MAX_DELAY;

        let range = self.delay_range();
        let phase_step = (1.0 - 0.5) / range.max(1.0);
        self.phase = (self.phase + phase_step) % 1.0;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }

        let wet = self.lowpass * pedal.sub_mix * 1.8;
        let dry = input * (1.0 - pedal.sub_mix).powi(3);
        let makeup_gain = 1.0 + pedal.sub_mix * 0.95;
        ((wet + dry) * pedal.level * makeup_gain).clamp(-1.0, 1.0)
    }

    fn shifted_sample(&self) -> f32 {
        let head_a = self.read_head(self.phase);
        let head_b = self.read_head((self.phase + 0.5) % 1.0);
        head_a + head_b
    }

    fn read_head(&self, phase: f32) -> f32 {
        let delay = MIN_DELAY + phase * self.delay_range();
        let read_pos = self.write_index as f32 - delay;
        let window = (std::f32::consts::PI * phase).sin().powi(2);
        self.sample_at(read_pos) * window
    }

    fn delay_range(&self) -> f32 {
        (MAX_DELAY as f32 * 0.45).max(MIN_DELAY + 1.0) - MIN_DELAY
    }

    fn sample_at(&self, position: f32) -> f32 {
        let wrapped = position.rem_euclid(MAX_DELAY as f32);
        let base = wrapped.floor() as usize % MAX_DELAY;
        let next = (base + 1) % MAX_DELAY;
        let frac = wrapped - wrapped.floor();
        self.buffer[base] * (1.0 - frac) + self.buffer[next] * frac
    }
}
