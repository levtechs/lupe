pub struct Reverb {
    buffer_a: Vec<f32>,
    buffer_b: Vec<f32>,
    buffer_c: Vec<f32>,
    idx_a: usize,
    idx_b: usize,
    idx_c: usize,
}

impl Reverb {
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate.max(8_000) as usize;
        Self {
            buffer_a: vec![0.0; (sr / 37).max(1)],
            buffer_b: vec![0.0; (sr / 29).max(1)],
            buffer_c: vec![0.0; (sr / 23).max(1)],
            idx_a: 0,
            idx_b: 0,
            idx_c: 0,
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let tap_a = self.buffer_a[self.idx_a];
        let tap_b = self.buffer_b[self.idx_b];
        let tap_c = self.buffer_c[self.idx_c];

        self.buffer_a[self.idx_a] = input + tap_a * 0.72;
        self.buffer_b[self.idx_b] = input + tap_b * 0.63;
        self.buffer_c[self.idx_c] = input + tap_c * 0.55;

        self.idx_a = (self.idx_a + 1) % self.buffer_a.len();
        self.idx_b = (self.idx_b + 1) % self.buffer_b.len();
        self.idx_c = (self.idx_c + 1) % self.buffer_c.len();

        (input * 0.7 + (tap_a + tap_b + tap_c) * 0.18).clamp(-1.0, 1.0)
    }
}
