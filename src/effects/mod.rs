mod gain;
mod none;
mod reverb;

pub use reverb::Reverb;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectKind {
    None,
    Gain,
    Reverb,
}

impl EffectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gain => "gain",
            Self::Reverb => "reverb",
        }
    }

    pub fn step(self, delta: i32) -> Self {
        let idx = match self {
            Self::None => 0,
            Self::Gain => 1,
            Self::Reverb => 2,
        };

        match (idx + delta).rem_euclid(3) {
            0 => Self::None,
            1 => Self::Gain,
            _ => Self::Reverb,
        }
    }
}

pub struct EffectsChain {
    reverb: Reverb,
}

impl EffectsChain {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            reverb: Reverb::new(sample_rate),
        }
    }

    pub fn process(&mut self, effect: EffectKind, sample: f32) -> f32 {
        match effect {
            EffectKind::None => none::process(sample),
            EffectKind::Gain => gain::process(sample),
            EffectKind::Reverb => self.reverb.process(sample),
        }
    }
}
