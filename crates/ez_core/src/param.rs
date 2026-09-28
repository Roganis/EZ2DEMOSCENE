//! Animatable scalar parameters.
//!
//! A [`Param`] is a base value plus an optional loop-locked oscillator and an
//! optional audio modulation. The oscillator runs an *integer* number of
//! cycles per loop, so `eval(phase = 0) == eval(phase = 1)` always holds.

use crate::clock::EvalCtx;
use crate::music::{AudioSource, MusicMod};
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
    /// Your own shape: points joined by curves (see [`Envelope`]).
    Envelope,
}

/// Menu groups for [`Wave`].
pub const WAVE_GROUPS: [(&str, &[Wave]); 4] = [
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
    ("Custom", &[Wave::Envelope]),
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
            Wave::Envelope => "Envelope",
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
            Wave::Envelope => {
                "Draw your own shape: points on the timeline joined by curves \
                 (click to add, drag to move, right-click for the curve or to delete)"
            }
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
                | Wave::Envelope
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
            // Its points live in the Param (see `Param::eval_offset`).
            Wave::Envelope => Envelope::DEFAULT.eval(f),
        }
    }
}

/// How an envelope goes from one point to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Curve {
    /// Straight line.
    #[default]
    Linear,
    /// Eases out of one point and into the next.
    Smooth,
    /// Starts slowly, arrives fast.
    EaseIn,
    /// Starts fast, arrives slowly.
    EaseOut,
    /// Stays, then jumps at the next point.
    Hold,
}

impl Curve {
    pub const ALL: [Curve; 5] = [
        Curve::Linear,
        Curve::Smooth,
        Curve::EaseIn,
        Curve::EaseOut,
        Curve::Hold,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Curve::Linear => "Straight",
            Curve::Smooth => "Smooth",
            Curve::EaseIn => "Ease in (slow start)",
            Curve::EaseOut => "Ease out (slow end)",
            Curve::Hold => "Hold, then jump",
        }
    }

    /// Progress 0..1 along a segment shaped by the curve.
    fn shape(self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Curve::Linear => u,
            Curve::Smooth => u * u * (3.0 - 2.0 * u),
            Curve::EaseIn => u * u * u,
            Curve::EaseOut => 1.0 - (1.0 - u).powi(3),
            Curve::Hold => 0.0,
        }
    }
}

/// One point of an envelope: where in the cycle (0..1), how high (0..1),
/// and how it goes on to the next point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EnvPoint {
    pub t: f32,
    pub v: f32,
    #[serde(default)]
    pub curve: Curve,
}

impl EnvPoint {
    pub const fn new(t: f32, v: f32, curve: Curve) -> Self {
        EnvPoint { t, v, curve }
    }
}

/// A multi-segment envelope: up to [`Envelope::MAX`] points over one cycle,
/// joined by curves, the last one leading back round to the first so the
/// loop stays seamless. Values run 0..1 (the Param's amount scales them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Envelope {
    points: [EnvPoint; Envelope::MAX],
    len: u8,
}

impl Envelope {
    pub const MAX: usize = 16;

    /// A rise to the top halfway through the cycle and a fall back.
    pub const DEFAULT: Envelope = {
        let mut points = [EnvPoint::new(0.0, 0.0, Curve::Linear); Envelope::MAX];
        points[0] = EnvPoint::new(0.0, 0.0, Curve::Smooth);
        points[1] = EnvPoint::new(0.5, 1.0, Curve::Smooth);
        Envelope { points, len: 2 }
    };

    /// The points, in time order.
    pub fn points(&self) -> &[EnvPoint] {
        &self.points[..self.len as usize]
    }

    /// Replace the points (kept in 0..1, sorted, at most [`Self::MAX`]).
    pub fn set(&mut self, points: &[EnvPoint]) {
        let mut v: Vec<EnvPoint> = points
            .iter()
            .take(Self::MAX)
            .map(|p| EnvPoint {
                t: if p.t.is_finite() {
                    p.t.clamp(0.0, 1.0)
                } else {
                    0.0
                },
                v: if p.v.is_finite() {
                    p.v.clamp(0.0, 1.0)
                } else {
                    0.0
                },
                curve: p.curve,
            })
            .collect();
        v.sort_by(|a, b| a.t.total_cmp(&b.t));
        self.len = v.len() as u8;
        self.points[..v.len()].copy_from_slice(&v);
    }

    /// The value (0..1) at `f` (0..1) in the cycle.
    pub fn eval(&self, f: f32) -> f32 {
        let pts = self.points();
        let n = pts.len();
        match n {
            0 => return 0.0,
            1 => return pts[0].v,
            _ => {}
        }
        let f = f.rem_euclid(1.0);
        // The segment `f` falls in: from the last point at or before it
        // (before the first point, the wrap-around segment from the last).
        let i = match pts.iter().rposition(|p| p.t <= f) {
            Some(i) => i,
            None => n - 1,
        };
        let a = pts[i];
        let b = pts[(i + 1) % n];
        let (ta, mut tb) = (a.t, b.t);
        let mut x = f;
        if i + 1 >= n {
            // Round the end of the cycle to the first point.
            tb += 1.0;
            if x < ta {
                x += 1.0;
            }
        }
        let len = tb - ta;
        if len <= 1e-6 {
            return b.v;
        }
        a.v + (b.v - a.v) * a.curve.shape((x - ta) / len)
    }
}

impl Default for Envelope {
    fn default() -> Self {
        Envelope::DEFAULT
    }
}

impl Envelope {
    /// An envelope of these points (see [`Envelope::set`]).
    pub fn from_points(points: &[EnvPoint]) -> Envelope {
        let mut e = Envelope::DEFAULT;
        e.set(points);
        e
    }

    fn key(&self) -> Vec<u32> {
        self.points()
            .iter()
            .flat_map(|p| [p.t.to_bits(), p.v.to_bits(), p.curve as u32])
            .collect()
    }
}

/// A [`Param`]'s envelope: a small handle to its points, which live once in
/// a shared table. It keeps `Param` small and `Copy` (a project holds
/// thousands of them, and undo keeps copies of the project), and equal
/// envelopes get the same handle, so comparing Params still compares the
/// points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct EnvRef(u32);

struct EnvTable {
    list: Vec<Envelope>,
    index: std::collections::HashMap<Vec<u32>, u32>,
}

static ENVELOPES: std::sync::LazyLock<std::sync::RwLock<EnvTable>> =
    std::sync::LazyLock::new(|| {
        let default = Envelope::DEFAULT;
        let mut index = std::collections::HashMap::new();
        index.insert(default.key(), 0);
        std::sync::RwLock::new(EnvTable {
            list: vec![default],
            index,
        })
    });

impl EnvRef {
    /// The handle of these points (the same handle for the same points).
    pub fn new(env: &Envelope) -> EnvRef {
        let key = env.key();
        if let Some(i) = ENVELOPES.read().unwrap().index.get(&key) {
            return EnvRef(*i);
        }
        let mut t = ENVELOPES.write().unwrap();
        if let Some(i) = t.index.get(&key) {
            return EnvRef(*i);
        }
        let i = t.list.len() as u32;
        t.list.push(*env);
        t.index.insert(key, i);
        EnvRef(i)
    }

    /// The handle of an envelope made of these points.
    pub fn from_points(points: &[EnvPoint]) -> EnvRef {
        EnvRef::new(&Envelope::from_points(points))
    }

    /// The envelope.
    pub fn get(self) -> Envelope {
        ENVELOPES
            .read()
            .unwrap()
            .list
            .get(self.0 as usize)
            .copied()
            .unwrap_or(Envelope::DEFAULT)
    }

    /// Its value (0..1) at `f` in the cycle.
    pub fn eval(self, f: f32) -> f32 {
        let t = ENVELOPES.read().unwrap();
        match t.list.get(self.0 as usize) {
            Some(e) => e.eval(f),
            None => Envelope::DEFAULT.eval(f),
        }
    }

    pub fn is_default(&self) -> bool {
        self.0 == 0
    }
}

impl Serialize for EnvRef {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.get().points().serialize(s)
    }
}

impl<'de> Deserialize<'de> for EnvRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Vec::<EnvPoint>::deserialize(d)?;
        Ok(EnvRef::from_points(&v))
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
    /// Amount of audio level added (0 = none). Older projects; new ones
    /// use `music`.
    pub audio: f32,
    /// Link to the music (kick, bass, hits, pitch…).
    pub music: MusicMod,
    /// The points of the `Envelope` wave.
    pub env: EnvRef,
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
            music: MusicMod {
                source: AudioSource::Kick,
                amount: 0.0,
                smooth: false,
                threshold: 0.0,
                shape: Wave::ExpOut,
                length: 0.5,
            },
            env: EnvRef(0),
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

    /// Builder: react to the music.
    pub const fn with_music(mut self, source: AudioSource, amount: f32) -> Self {
        self.music.source = source;
        self.music.amount = amount;
        self
    }

    pub fn is_animated(&self) -> bool {
        self.amp != 0.0 || self.audio != 0.0 || self.music.amount != 0.0
    }

    /// True when the value depends on the music.
    pub fn uses_music(&self) -> bool {
        self.audio != 0.0 || self.music.amount != 0.0
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
            // Beat fades stay on the real beat; everything else follows the
            // (possibly time-warped) motion clock.
            let phase = if self.wave.is_unipolar() {
                ctx.beat_phase
            } else {
                ctx.phase
            };
            let x = phase * self.cycles as f32 + self.offset + extra;
            v += self.amp
                * match self.wave {
                    Wave::Envelope => self.env.eval(x),
                    w => w.eval(x, self.cycles),
                };
        }
        if self.audio != 0.0 {
            v += self.audio * ctx.audio;
        }
        v += self.music.eval(&ctx.music, ctx.beat_seconds);
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
    #[serde(skip_serializing_if = "music_off")]
    music: MusicMod,
    #[serde(skip_serializing_if = "EnvRef::is_default")]
    env: EnvRef,
}

fn music_off(m: &MusicMod) -> bool {
    m.amount == 0.0
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
            music: p.music,
            env: p.env,
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
                music: self.music,
                env: self.env,
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
                music: f.music,
                env: f.env,
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
    fn envelope_passes_its_points_and_loops() {
        let mut e = Envelope::DEFAULT;
        e.set(&[
            EnvPoint::new(0.1, 0.2, Curve::Linear),
            EnvPoint::new(0.4, 1.0, Curve::Hold),
            EnvPoint::new(0.6, 0.5, Curve::Smooth),
            EnvPoint::new(0.9, 0.0, Curve::EaseOut),
        ]);
        for p in e.points() {
            assert!((e.eval(p.t) - p.v).abs() < 1e-5, "{p:?}");
        }
        // Straight halfway between the first two.
        assert!((e.eval(0.25) - 0.6).abs() < 1e-5);
        // Hold keeps the value until the next point.
        assert!((e.eval(0.59) - 1.0).abs() < 1e-5);
        // Round the loop: from the last point (0.9, 0) to the first (1.1, 0.2).
        assert!((e.eval(0.0) - e.eval(1.0)).abs() < 1e-6);
        let mid = e.eval(0.0);
        assert!(mid > 0.0 && mid < 0.2, "{mid}");
        for i in 0..=1000 {
            let v = e.eval(i as f32 / 1000.0);
            assert!((0.0..=1.0).contains(&v), "{v}");
        }
    }

    #[test]
    fn envelope_edge_cases_and_saving() {
        let mut e = Envelope::DEFAULT;
        e.set(&[]);
        assert_eq!(e.eval(0.3), 0.0);
        e.set(&[EnvPoint::new(0.5, 0.7, Curve::Linear)]);
        assert_eq!(e.eval(0.1), 0.7);
        // Out of order and out of range: sorted and clamped.
        e.set(&[
            EnvPoint::new(0.8, 2.0, Curve::Linear),
            EnvPoint::new(-1.0, 0.5, Curve::Smooth),
        ]);
        assert_eq!(e.points()[0], EnvPoint::new(0.0, 0.5, Curve::Smooth));
        assert_eq!(e.points()[1], EnvPoint::new(0.8, 1.0, Curve::Linear));
        // More than MAX points are cut.
        let many: Vec<EnvPoint> = (0..40)
            .map(|i| EnvPoint::new(i as f32 / 40.0, 0.5, Curve::Linear))
            .collect();
        e.set(&many);
        assert_eq!(e.points().len(), Envelope::MAX);
        // A Param with an envelope saves its points and loads them back.
        let mut p = Param::new(1.0).osc(Wave::Envelope, 2.0, 4);
        p.env = EnvRef::from_points(&[
            EnvPoint::new(0.0, 0.0, Curve::Hold),
            EnvPoint::new(0.3, 1.0, Curve::EaseIn),
        ]);
        // The same points: the same handle (Params compare their points).
        assert_eq!(
            p.env,
            EnvRef::from_points(&[
                EnvPoint::new(0.0, 0.0, Curve::Hold),
                EnvPoint::new(0.3, 1.0, Curve::EaseIn),
            ])
        );
        assert!(
            std::mem::size_of::<Param>() < 80,
            "{}",
            std::mem::size_of::<Param>()
        );
        let json = serde_json::to_string(&p).unwrap();
        let q: Param = serde_json::from_str(&json).unwrap();
        assert_eq!(p, q);
        // Other params don't save the default envelope.
        let json = serde_json::to_string(&Param::new(1.0).osc(Wave::Sine, 1.0, 1)).unwrap();
        assert!(!json.contains("env"), "{json}");
    }

    #[test]
    fn envelope_param_loops_and_repeats() {
        let mut p = Param::new(1.0).osc(Wave::Envelope, 2.0, 4).with_offset(0.3);
        p.env = EnvRef::from_points(&[
            EnvPoint::new(0.2, 0.0, Curve::Linear),
            EnvPoint::new(0.7, 1.0, Curve::Smooth),
        ]);
        let a = p.eval(&EvalCtx::at(0.0));
        let b = p.eval(&EvalCtx::at(1.0));
        assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        // Four repeats per loop: a quarter of the loop later it's the same.
        let c = p.eval(&EvalCtx::at(0.1));
        let d = p.eval(&EvalCtx::at(0.35));
        assert!((c - d).abs() < 1e-4, "{c} vs {d}");
        // Values between base and base + amount.
        for i in 0..=200 {
            let v = p.eval(&EvalCtx::at(i as f32 / 200.0));
            assert!((1.0 - 1e-4..=3.0 + 1e-4).contains(&v), "{v}");
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
