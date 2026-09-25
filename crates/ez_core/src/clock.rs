//! Loop clock: tempo, loop length and the evaluation context handed to every
//! animated value.

use crate::audio::AudioEnvelope;
use serde::{Deserialize, Serialize};

/// Tempo and loop length. The loop always spans a whole number of beats.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Timing {
    pub bpm: f32,
    pub loop_beats: u32,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            bpm: 120.0,
            loop_beats: 16,
        }
    }
}

impl Timing {
    pub fn beat_seconds(&self) -> f32 {
        60.0 / self.bpm.max(1.0)
    }

    pub fn loop_seconds(&self) -> f32 {
        self.beat_seconds() * self.loop_beats.max(1) as f32
    }

    /// Phase (0..1) for an absolute time in seconds.
    pub fn phase_at(&self, seconds: f64) -> f32 {
        let l = self.loop_seconds() as f64;
        (seconds / l).rem_euclid(1.0) as f32
    }

    /// Number of frames for one loop at `fps` (at least 1).
    pub fn frame_count(&self, fps: f32) -> u32 {
        ((self.loop_seconds() * fps).round() as u32).max(1)
    }
}

/// Everything an animated value may depend on for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EvalCtx {
    /// Loop phase in [0, 1).
    pub phase: f32,
    /// Beats in one loop.
    pub loop_beats: u32,
    /// Overall audio level (0..1) at this point in the loop, 0 without audio.
    pub audio: f32,
    /// Low-frequency ("kick") audio level (0..1).
    pub bass: f32,
}

impl EvalCtx {
    pub fn new(timing: &Timing, phase: f32, audio: Option<&AudioEnvelope>) -> Self {
        let (a, b) = audio
            .map(|e| e.sample(phase * timing.loop_seconds()))
            .unwrap_or((0.0, 0.0));
        EvalCtx {
            phase,
            loop_beats: timing.loop_beats.max(1),
            audio: a,
            bass: b,
        }
    }

    /// Context with only a phase (no audio), handy for tests and thumbnails.
    pub fn at(phase: f32) -> Self {
        EvalCtx {
            phase,
            loop_beats: 16,
            audio: 0.0,
            bass: 0.0,
        }
    }

    /// Continuous beat position within the loop (0..loop_beats).
    pub fn beat(&self) -> f32 {
        self.phase * self.loop_beats as f32
    }

    /// Fractional part of the current beat (0..1).
    pub fn beat_frac(&self) -> f32 {
        self.beat().fract()
    }

    /// Decaying pulse that fires on every beat (1 at the beat, towards 0 after).
    pub fn beat_pulse(&self, sharpness: f32) -> f32 {
        (-self.beat_frac() * sharpness).exp()
    }

    /// Angle helper: `turns` full rotations over the loop, in radians.
    pub fn turns(&self, turns: f32) -> f32 {
        self.phase * turns * std::f32::consts::TAU
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_length() {
        let t = Timing {
            bpm: 120.0,
            loop_beats: 8,
        };
        assert!((t.loop_seconds() - 4.0).abs() < 1e-6);
        assert_eq!(t.frame_count(30.0), 120);
        assert!((t.phase_at(5.0) - 0.25).abs() < 1e-6);
    }
}
