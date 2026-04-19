mod bassify;
mod distortion;
mod equalizer;
mod fuzz;
mod metronome;
mod phaser;
mod reverb;

pub use bassify::BassifyPedal;
pub use distortion::DistortionPedal;
pub use equalizer::EqualizerPedal;
pub use metronome::MetronomePedal;
pub use fuzz::FuzzPedal;
pub use phaser::PhaserPedal;
pub use reverb::ReverbPedal;

use bassify::BassifyState;
use distortion::DistortionState;
use equalizer::EqualizerState;
use fuzz::FuzzState;
use phaser::PhaserState;
use reverb::ReverbState;
use serde::{Deserialize, Serialize};

pub trait PedalDefinition {
    fn kind(&self) -> PedalKind;
    fn display_name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn accent_rgb(&self) -> (u8, u8, u8);
    fn enabled(&self) -> bool;
    fn toggle_enabled(&mut self);
    fn summary(&self) -> String;
    fn param_count(&self) -> usize;
    fn param_name(&self, index: usize) -> &'static str;
    fn param_value(&self, index: usize) -> String;
    fn step_param(&mut self, index: usize, delta: i32);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PedalKind {
    Equalizer,
    Reverb,
    Distortion,
    Fuzz,
    Phaser,
    Bassify,
}

impl PedalKind {
    pub const ALL: [PedalKind; 6] = [
        PedalKind::Equalizer,
        PedalKind::Reverb,
        PedalKind::Distortion,
        PedalKind::Fuzz,
        PedalKind::Phaser,
        PedalKind::Bassify,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Equalizer => "equalizer",
            Self::Reverb => "reverb",
            Self::Distortion => "distortion",
            Self::Fuzz => "fuzz",
            Self::Phaser => "phaser",
            Self::Bassify => "bassify",
        }
    }

}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PedalSpec {
    Bassify(BassifyPedal),
    Distortion(DistortionPedal),
    Equalizer(EqualizerPedal),
    Fuzz(FuzzPedal),
    Phaser(PhaserPedal),
    Reverb(ReverbPedal),
}

impl PedalSpec {
    pub fn new(kind: PedalKind) -> Self {
        match kind {
            PedalKind::Bassify => Self::Bassify(BassifyPedal::default()),
            PedalKind::Distortion => Self::Distortion(DistortionPedal::default()),
            PedalKind::Equalizer => Self::Equalizer(EqualizerPedal::default()),
            PedalKind::Fuzz => Self::Fuzz(FuzzPedal::default()),
            PedalKind::Phaser => Self::Phaser(PhaserPedal::default()),
            PedalKind::Reverb => Self::Reverb(ReverbPedal::default()),
        }
    }

    pub fn kind(&self) -> PedalKind {
        match self {
            Self::Bassify(pedal) => pedal.kind(),
            Self::Distortion(pedal) => pedal.kind(),
            Self::Equalizer(pedal) => pedal.kind(),
            Self::Fuzz(pedal) => pedal.kind(),
            Self::Phaser(pedal) => pedal.kind(),
            Self::Reverb(pedal) => pedal.kind(),
        }
    }

    pub fn label(&self) -> &'static str {
        self.kind().label()
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Bassify(pedal) => pedal.display_name(),
            Self::Distortion(pedal) => pedal.display_name(),
            Self::Equalizer(pedal) => pedal.display_name(),
            Self::Fuzz(pedal) => pedal.display_name(),
            Self::Phaser(pedal) => pedal.display_name(),
            Self::Reverb(pedal) => pedal.display_name(),
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Bassify(pedal) => pedal.description(),
            Self::Distortion(pedal) => pedal.description(),
            Self::Equalizer(pedal) => pedal.description(),
            Self::Fuzz(pedal) => pedal.description(),
            Self::Phaser(pedal) => pedal.description(),
            Self::Reverb(pedal) => pedal.description(),
        }
    }

    pub fn accent_rgb(&self) -> (u8, u8, u8) {
        match self {
            Self::Bassify(pedal) => pedal.accent_rgb(),
            Self::Distortion(pedal) => pedal.accent_rgb(),
            Self::Equalizer(pedal) => pedal.accent_rgb(),
            Self::Fuzz(pedal) => pedal.accent_rgb(),
            Self::Phaser(pedal) => pedal.accent_rgb(),
            Self::Reverb(pedal) => pedal.accent_rgb(),
        }
    }

    pub fn enabled(&self) -> bool {
        match self {
            Self::Bassify(pedal) => pedal.enabled(),
            Self::Distortion(pedal) => pedal.enabled(),
            Self::Equalizer(pedal) => pedal.enabled(),
            Self::Fuzz(pedal) => pedal.enabled(),
            Self::Phaser(pedal) => pedal.enabled(),
            Self::Reverb(pedal) => pedal.enabled(),
        }
    }

    pub fn toggle_enabled(&mut self) {
        match self {
            Self::Bassify(pedal) => pedal.toggle_enabled(),
            Self::Distortion(pedal) => pedal.toggle_enabled(),
            Self::Equalizer(pedal) => pedal.toggle_enabled(),
            Self::Fuzz(pedal) => pedal.toggle_enabled(),
            Self::Phaser(pedal) => pedal.toggle_enabled(),
            Self::Reverb(pedal) => pedal.toggle_enabled(),
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Self::Bassify(pedal) => pedal.summary(),
            Self::Distortion(pedal) => pedal.summary(),
            Self::Equalizer(pedal) => pedal.summary(),
            Self::Fuzz(pedal) => pedal.summary(),
            Self::Phaser(pedal) => pedal.summary(),
            Self::Reverb(pedal) => pedal.summary(),
        }
    }

    pub fn param_count(&self) -> usize {
        match self {
            Self::Bassify(pedal) => pedal.param_count(),
            Self::Distortion(pedal) => pedal.param_count(),
            Self::Equalizer(pedal) => pedal.param_count(),
            Self::Fuzz(pedal) => pedal.param_count(),
            Self::Phaser(pedal) => pedal.param_count(),
            Self::Reverb(pedal) => pedal.param_count(),
        }
    }

    pub fn param_name(&self, index: usize) -> &'static str {
        match self {
            Self::Bassify(pedal) => pedal.param_name(index),
            Self::Distortion(pedal) => pedal.param_name(index),
            Self::Equalizer(pedal) => pedal.param_name(index),
            Self::Fuzz(pedal) => pedal.param_name(index),
            Self::Phaser(pedal) => pedal.param_name(index),
            Self::Reverb(pedal) => pedal.param_name(index),
        }
    }

    pub fn param_value(&self, index: usize) -> String {
        match self {
            Self::Bassify(pedal) => pedal.param_value(index),
            Self::Distortion(pedal) => pedal.param_value(index),
            Self::Equalizer(pedal) => pedal.param_value(index),
            Self::Fuzz(pedal) => pedal.param_value(index),
            Self::Phaser(pedal) => pedal.param_value(index),
            Self::Reverb(pedal) => pedal.param_value(index),
        }
    }

    pub fn step_param(&mut self, index: usize, delta: i32) {
        match self {
            Self::Bassify(pedal) => pedal.step_param(index, delta),
            Self::Distortion(pedal) => pedal.step_param(index, delta),
            Self::Equalizer(pedal) => pedal.step_param(index, delta),
            Self::Fuzz(pedal) => pedal.step_param(index, delta),
            Self::Phaser(pedal) => pedal.step_param(index, delta),
            Self::Reverb(pedal) => pedal.step_param(index, delta),
        }
    }
}

enum RuntimePedal {
    Bassify(BassifyState),
    Distortion(DistortionState),
    Equalizer(EqualizerState),
    Fuzz(FuzzState),
    Phaser(PhaserState),
    Reverb(ReverbState),
}

pub struct PedalChain {
    version: u64,
    sample_rate: u32,
    pedals: Vec<RuntimePedal>,
}

impl PedalChain {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            version: u64::MAX,
            sample_rate,
            pedals: Vec::new(),
        }
    }

    pub fn sync(&mut self, specs: &[PedalSpec], version: u64) {
        if self.version == version {
            return;
        }

        self.pedals = specs
            .iter()
            .map(|spec| match spec {
                PedalSpec::Bassify(_) => RuntimePedal::Bassify(BassifyState::new()),
                PedalSpec::Distortion(_) => RuntimePedal::Distortion(DistortionState::new()),
                PedalSpec::Equalizer(_) => RuntimePedal::Equalizer(EqualizerState::new()),
                PedalSpec::Fuzz(_) => RuntimePedal::Fuzz(FuzzState::new()),
                PedalSpec::Phaser(_) => RuntimePedal::Phaser(PhaserState::new(self.sample_rate)),
                PedalSpec::Reverb(_) => RuntimePedal::Reverb(ReverbState::new(self.sample_rate)),
            })
            .collect();
        self.version = version;
    }

    pub fn process(&mut self, input: f32, specs: &[PedalSpec]) -> f32 {
        let mut sample = input;

        for (runtime, spec) in self.pedals.iter_mut().zip(specs.iter()) {
            if !spec.enabled() {
                continue;
            }

            sample = match (runtime, spec) {
                (RuntimePedal::Bassify(runtime), PedalSpec::Bassify(pedal)) => runtime.process(sample, pedal),
                (RuntimePedal::Distortion(runtime), PedalSpec::Distortion(pedal)) => runtime.process(sample, pedal),
                (RuntimePedal::Equalizer(runtime), PedalSpec::Equalizer(pedal)) => runtime.process(sample, pedal),
                (RuntimePedal::Fuzz(runtime), PedalSpec::Fuzz(pedal)) => runtime.process(sample, pedal),
                (RuntimePedal::Phaser(runtime), PedalSpec::Phaser(pedal)) => runtime.process(sample, pedal),
                (RuntimePedal::Reverb(runtime), PedalSpec::Reverb(pedal)) => runtime.process(sample, pedal),
                _ => sample,
            };
        }

        sample.clamp(-1.0, 1.0)
    }
}
