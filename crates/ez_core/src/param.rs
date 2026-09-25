//! Animatable scalar parameters.
//!
//! A [`Param`] is a base value plus an optional loop-locked oscillator and an
//! optional audio modulation. The oscillator runs an *integer* number of
//! cycles per loop, so `eval(phase = 0) == eval(phase = 1)` always holds.

use crate::clock::EvalCtx;
use crate::rng::hash2;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Oscillator shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Wave {
    #[default]
    Sine,
    Triangle,
    Saw,
    Square,
    /// Sharp attack, exponential decay: good for beat hits.
    Pulse,
    /// A new random value every cycle (sample & hold).
    Random,
}

impl Wave {
    pub const ALL: [Wave; 6] = [
        Wave::Sine,
        Wave::Triangle,
        Wave::Saw,
        Wave::Square,
        Wave::Pulse,
        Wave::Random,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Wave::Sine => "Sine",
            Wave::Triangle => "Triangle",
            Wave::Saw => "Saw",
            Wave::Square => "Square",
            Wave::Pulse => "Pulse",
            Wave::Random => "Random",
        }
    }

    /// Evaluate the wave. `x` is the continuous cycle position (cycles * phase
    /// + offset); returns -1..1 (Pulse returns 0..1).
    pub fn eval(self, x: f32, cycles: i32) -> f32 {
        let f = x.rem_euclid(1.0);
        match self {
            Wave::Sine => (f * std::f32::consts::TAU).sin(),
            Wave::Triangle => 1.0 - 4.0 * (f - 0.5).abs(),
            Wave::Saw => f * 2.0 - 1.0,
            Wave::Square => {
                if f < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Wave::Pulse => (-f * 7.0).exp(),
            Wave::Random => {
                let n = cycles.unsigned_abs().max(1) as i64;
                let idx = (x.floor() as i64).rem_euclid(n) as u32;
                hash2(idx, 0x5eed) * 2.0 - 1.0
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
