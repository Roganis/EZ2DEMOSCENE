//! Animatable scalar parameters.
//!
//! A [`Param`] is a base value plus an optional loop-locked oscillator and an
//! optional audio modulation. The oscillator runs an *integer* number of
//! cycles per loop, so `eval(phase = 0) == eval(phase = 1)` always holds.

use crate::clock::EvalCtx;
use crate::rng::hash2;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Oscillator shape.
///
/// LFO and random shapes swing both ways (-1..1). The fades run 0..1 once
/// per cycle, so with "×/loop" = beats per loop they fire on every beat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Wave {
    #[default]
    Sine,
    Triangle,
    /// Ramp up, then jump down.
    Saw,
    /// Ramp down, then jump up.
    RampDown,
    Square,
    /// Sharp attack, exponential decay: good for beat hits.
    Pulse,
    /// Slow start, fast finish (0 → 1).
    ExpIn,
    /// Fast drop, slow tail (1 → 0).
    ExpOut,
    /// Straight fade 0 → 1.
    LinearIn,
    /// Straight fade 1 → 0.
    LinearOut,
    /// Smooth swell 0 → 1 → 0.
    Swell,
    /// A new random value every cycle (sample & hold).
    Random,
    /// Random values joined by smooth glides.
    SmoothRandom,
    /// Random walk that wanders and comes back by the end of the loop.
    Drunk,
}

/// Menu groups for [`Wave`].
pub const WAVE_GROUPS: [(&str, &[Wave]); 3] = [
    (
        "LFO",
        &[
            Wave::Sine,
            Wave::Triangle,
            Wave::Saw,
            Wave::RampDown,
            Wave::Square,
        ],
    ),
    (
        "Beat fades",
        &[
            Wave::Pulse,
            Wave::ExpIn,
            Wave::ExpOut,
            Wave::LinearIn,
            Wave::LinearOut,
            Wave::Swell,
        ],
    ),
    ("Random", &[Wave::Random, Wave::SmoothRandom, Wave::Drunk]),
];

fn step_rand(idx: i64, n: i64, salt: u32) -> f32 {
    hash2(idx.rem_euclid(n) as u32, salt) * 2.0 - 1.0
}

impl Wave {
    pub const ALL: [Wave; 14] = [
        Wave::Sine,
        Wave::Triangle,
        Wave::Saw,
        Wave::RampDown,
        Wave::Square,
        Wave::Pulse,
        Wave::ExpIn,
        Wave::ExpOut,
        Wave::LinearIn,
        Wave::LinearOut,
        Wave::Swell,
        Wave::Random,
        Wave::SmoothRandom,
        Wave::Drunk,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Wave::Sine => "Sine",
            Wave::Triangle => "Triangle",
            Wave::Saw => "Saw up",
            Wave::RampDown => "Saw down",
            Wave::Square => "Square",
            Wave::Pulse => "Pulse (hit)",
            Wave::ExpIn => "Exp fade in",
            Wave::ExpOut => "Exp fade out",
            Wave::LinearIn => "Linear fade in",
            Wave::LinearOut => "Linear fade out",
            Wave::Swell => "Swell in & out",
            Wave::Random => "Sample & hold",
            Wave::SmoothRandom => "Smooth random",
            Wave::Drunk => "Drunken walk",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Wave::Sine => "Smooth up and down",
            Wave::Triangle => "Straight up and down",
            Wave::Saw => "Ramps up, then jumps back",
            Wave::RampDown => "Ramps down, then jumps back",
            Wave::Square => "Jumps between two values",
            Wave::Pulse => "Sharp hit that dies away quickly",
            Wave::ExpIn => "Builds up slowly, then rushes to the top",
            Wave::ExpOut => "Drops fast, then settles slowly",
            Wave::LinearIn => "Even fade from 0 to the full amount",
            Wave::LinearOut => "Even fade from the full amount to 0",
            Wave::Swell => "Smoothly rises and falls back",
            Wave::Random => "A new random value each cycle, held until the next",
            Wave::SmoothRandom => "Random values with smooth glides in between",
            Wave::Drunk => "Wanders randomly and finds its way back by the loop's end",
        }
    }

    /// True for shapes that run 0..1 (the fades); the others swing -1..1.
    pub fn is_unipolar(self) -> bool {
        matches!(
            self,
            Wave::Pulse
                | Wave::ExpIn
                | Wave::ExpOut
                | Wave::LinearIn
                | Wave::LinearOut
                | Wave::Swell
        )
    }

    /// Evaluate the wave. `x` is the continuous cycle position (cycles * phase
    /// + offset); returns -1..1 (the fades return 0..1).
    pub fn eval(self, x: f32, cycles: i32) -> f32 {
        const K: f32 = 5.0;
        let f = x.rem_euclid(1.0);
        let n = cycles.unsigned_abs().max(1) as i64;
        let idx = x.floor() as i64;
        match self {
            Wave::Sine => (f * std::f32::consts::TAU).sin(),
            Wave::Triangle => 1.0 - 4.0 * (f - 0.5).abs(),
            Wave::Saw => f * 2.0 - 1.0,
            Wave::RampDown => 1.0 - f * 2.0,
            Wave::Square => {
                if f < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Wave::Pulse => (-f * 7.0).exp(),
            Wave::ExpIn => ((K * f).exp() - 1.0) / (K.exp() - 1.0),
            Wave::ExpOut => ((-K * f).exp() - (-K).exp()) / (1.0 - (-K).exp()),
            Wave::LinearIn => f,
            Wave::LinearOut => 1.0 - f,
            Wave::Swell => (f * std::f32::consts::PI).sin().powi(2),
            Wave::Random => step_rand(idx, n, 0x5eed),
            Wave::SmoothRandom => {
                let a = step_rand(idx, n, 0x5e1d);
                let b = step_rand(idx + 1, n, 0x5e1d);
                let t = f * f * (3.0 - 2.0 * f);
                a + (b - a) * t
            }
            Wave::Drunk => {
                // Random steps with the mean removed: the walk returns to
                // its start after `n` steps, so the loop stays seamless.
                let mean = (0..n).map(|i| step_rand(i, n, 0xd2c)).sum::<f32>() / n as f32;
                let mut pos = vec![0.0f32; n as usize + 1];
                for i in 0..n as usize {
                    pos[i + 1] = pos[i] + step_rand(i as i64, n, 0xd2c) - mean;
                }
                let peak = pos.iter().fold(1e-6f32, |m, v| m.max(v.abs()));
                let k = idx.rem_euclid(n) as usize;
                let t = f * f * (3.0 - 2.0 * f);
                (pos[k] + (pos[k + 1] - pos[k]) * t) / peak
            }
        }
    }
}

/// An animatable number.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Param {
    /// Value when not animated.
    pub base: f32,
    /// Oscillator amplitude (0 = static).
    pub amp: f32,
    pub wave: Wave,
    /// Whole cycles per loop (negative runs backwards).
    pub cycles: i32,
    /// Phase offset of the oscillator in cycles (0..1).
    pub offset: f32,
    /// Amount of audio level added (0 = none).
    pub audio: f32,
}

impl Param {
    pub const fn new(base: f32) -> Self {
        Param {
            base,
            amp: 0.0,
            wave: Wave::Sine,
            cycles: 1,
            offset: 0.0,
            audio: 0.0,
        }
    }

    /// Builder: add an oscillator.
    pub const fn osc(mut self, wave: Wave, amp: f32, cycles: i32) -> Self {
        self.wave = wave;
        self.amp = amp;
        self.cycles = cycles;
        self
    }

    pub const fn with_offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }

    pub const fn with_audio(mut self, amount: f32) -> Self {
        self.audio = amount;
        self
    }

    pub fn is_animated(&self) -> bool {
        self.amp != 0.0 || self.audio != 0.0
    }

    /// Evaluate at the given context.
    pub fn eval(&self, ctx: &EvalCtx) -> f32 {
        self.eval_offset(ctx, 0.0)
    }

    /// Evaluate with an extra phase offset (in cycles), used to stagger
    /// instances while keeping the loop seamless.
    pub fn eval_offset(&self, ctx: &EvalCtx, extra: f32) -> f32 {
        let mut v = self.base;
        if self.amp != 0.0 {
            let x = ctx.phase * self.cycles as f32 + self.offset + extra;
            v += self.amp * self.wave.eval(x, self.cycles);
        }
        if self.audio != 0.0 {
            v += self.audio * ctx.audio;
        }
        v
    }
}

impl From<f32> for Param {
    fn from(v: f32) -> Self {
        Param::new(v)
    }
}

// Serialize static params as plain numbers to keep project files readable.
#[derive(Serialize, Deserialize)]
#[serde(default)]
struct ParamFull {
    base: f32,
    amp: f32,
    wave: Wave,
    cycles: i32,
    offset: f32,
    audio: f32,
}

impl Default for ParamFull {
    fn default() -> Self {
        let p = Param::new(0.0);
        ParamFull {
            base: p.base,
            amp: p.amp,
            wave: p.wave,
            cycles: p.cycles,
            offset: p.offset,
            audio: p.audio,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ParamRepr {
    Const(f32),
    Full(ParamFull),
}

impl Serialize for Param {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if !self.is_animated() {
            s.serialize_f32(self.base)
        } else {
            ParamFull {
                base: self.base,
                amp: self.amp,
                wave: self.wave,
                cycles: self.cycles,
                offset: self.offset,
                audio: self.audio,
            }
            .serialize(s)
        }
    }
}

impl<'de> Deserialize<'de> for Param {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match ParamRepr::deserialize(d)? {
            ParamRepr::Const(v) => Param::new(v),
            ParamRepr::Full(f) => Param {
                base: f.base,
                amp: f.amp,
                wave: f.wave,
                cycles: f.cycles,
                offset: f.offset,
                audio: f.audio,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_wave_loops() {
        for wave in Wave::ALL {
            for cycles in [-3, -1, 1, 2, 7] {
                let p = Param::new(1.0).osc(wave, 0.5, cycles).with_offset(0.3);
                let a = p.eval(&EvalCtx::at(0.0));
                let b = p.eval(&EvalCtx::at(1.0));
                assert!((a - b).abs() < 1e-3, "{wave:?} x{cycles}: {a} vs {b}");
            }
        }
    }

    #[test]
    fn wave_ranges() {
        for wave in Wave::ALL {
            let (lo, hi) = if wave.is_unipolar() {
                (0.0, 1.0)
            } else {
                (-1.0, 1.0)
            };
            for cycles in [1, 4, 16] {
                for i in 0..=400 {
                    let x = i as f32 / 400.0 * cycles as f32;
                    let v = wave.eval(x, cycles);
                    assert!(v >= lo - 1e-4 && v <= hi + 1e-4, "{wave:?} {x}: {v}");
                }
            }
        }
        // Fades start and end where their names say.
        assert!(Wave::ExpIn.eval(0.0, 1) < 0.01 && Wave::ExpIn.eval(0.999, 1) > 0.97);
        assert!(Wave::LinearOut.eval(0.0, 1) > 0.99);
        assert!(Wave::Swell.eval(0.5, 1) > 0.99);
    }

    #[test]
    fn serde_compact() {
        let p = Param::new(2.5);
        assert_eq!(serde_json::to_string(&p).unwrap(), "2.5");
        let q: Param = serde_json::from_str("3").unwrap();
        assert_eq!(q.base, 3.0);
        let a = Param::new(1.0).osc(Wave::Pulse, 2.0, 4);
        let s = serde_json::to_string(&a).unwrap();
        let back: Param = serde_json::from_str(&s).unwrap();
        assert_eq!(a, back);
    }
}
