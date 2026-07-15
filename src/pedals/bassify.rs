use pitch_shift::{RawState, Shifter, TOTAL_F32};
use serde::{Deserialize, Serialize};

use super::{PedalDefinition, PedalKind};

const BLOCK_SIZE: usize = 128;
const OCTAVE_DOWN_SEMITONES: f32 = -12.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BassifyPedal {
    pub enabled: bool,
    pub level: f32,
}

impl Default for BassifyPedal {
    fn default() -> Self {
        Self {
            enabled: true,
            level: 0.82,
        }
    }
}

impl PedalDefinition for BassifyPedal {
    fn kind(&self) -> PedalKind { PedalKind::Bassify }
    fn display_name(&self) -> &'static str { "Bassify" }
    fn description(&self) -> &'static str { "Clean, fully wet pitch shift fixed at exactly one octave down." }
    fn accent_rgb(&self) -> (u8, u8, u8) { (218, 202, 86) }
    fn enabled(&self) -> bool { self.enabled }
    fn toggle_enabled(&mut self) { self.enabled = !self.enabled; }
    fn summary(&self) -> String {
        format!("1 octave down  output {}%", (self.level * 100.0).round() as i32)
    }
    fn param_count(&self) -> usize { 1 }
    fn param_name(&self, _index: usize) -> &'static str { "Output" }
    fn param_value(&self, _index: usize) -> String {
        format!("{}%", (self.level * 100.0).round() as i32)
    }
    fn step_param(&mut self, _index: usize, delta: i32) {
        self.level = (self.level + delta as f32 * 0.05).clamp(0.0, 1.2);
    }
}

pub struct BassifyState {
    shifter: Shifter<Box<RawState>>,
    input: [f32; BLOCK_SIZE],
    output: [f32; BLOCK_SIZE],
    block_index: usize,
    sample_rate: f32,
}

impl BassifyState {
    pub fn new(sample_rate: u32) -> Self {
        let state: Box<RawState> = vec![0.0; TOTAL_F32]
            .into_boxed_slice()
            .try_into()
            .expect("pitch shifter state has a fixed size");

        Self {
            shifter: Shifter::new(state),
            input: [0.0; BLOCK_SIZE],
            output: [0.0; BLOCK_SIZE],
            block_index: 0,
            sample_rate: sample_rate as f32,
        }
    }

    pub fn process(&mut self, input: f32, pedal: &BassifyPedal) -> f32 {
        let shifted = self.output[self.block_index];
        self.input[self.block_index] = input;
        self.block_index += 1;

        if self.block_index == BLOCK_SIZE {
            let output = self.shifter.shift(
                &self.input,
                OCTAVE_DOWN_SEMITONES,
                BLOCK_SIZE,
                self.sample_rate,
            );
            self.output.copy_from_slice(output);
            self.block_index = 0;
        }

        shifted * pedal.level
    }
}
