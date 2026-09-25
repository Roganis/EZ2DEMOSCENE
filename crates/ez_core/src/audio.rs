//! Pre-analysed audio envelope used as a modulation source.

/// Loudness envelopes sampled at a fixed rate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioEnvelope {
    /// Envelope samples per second.
    pub rate: f32,
    /// Overall level, normalised to 0..1.
    pub level: Vec<f32>,
    /// Low band ("kick") level, normalised to 0..1.
    pub bass: Vec<f32>,
    /// Duration of the analysed audio in seconds.
    pub duration: f32,
}

impl AudioEnvelope {
    /// Returns `(level, bass)` at `seconds`, wrapping around the audio length.
    pub fn sample(&self, seconds: f32) -> (f32, f32) {
        if self.level.is_empty() || self.rate <= 0.0 {
            return (0.0, 0.0);
        }
        let n = self.level.len();
        let pos = (seconds * self.rate).max(0.0);
        let i0 = (pos.floor() as usize) % n;
        let i1 = (i0 + 1) % n;
        let t = pos.fract();
        let l = self.level[i0] * (1.0 - t) + self.level[i1] * t;
        let b = self.bass[i0.min(self.bass.len().saturating_sub(1))] * (1.0 - t)
            + self.bass[i1.min(self.bass.len().saturating_sub(1))] * t;
        (l, b)
    }
}
