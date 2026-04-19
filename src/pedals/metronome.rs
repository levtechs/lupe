use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetronomePedal {
    pub enabled: bool,
    pub bpm: f32,
    pub accent_every: u32,
    pub tone_hz: f32,
    pub volume: f32,
}

impl Default for MetronomePedal {
    fn default() -> Self {
        Self {
            enabled: false,
            bpm: 110.0,
            accent_every: 4,
            tone_hz: 1_200.0,
            volume: 0.18,
        }
    }
}
