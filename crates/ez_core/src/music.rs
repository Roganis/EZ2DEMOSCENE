//! How music drives a project: which part of the song the loop uses, time
//! warp, the sources a value can follow, and the per-frame music state.
//!
//! **Loop window.** The loop reacts to a window of the song (`offset` …
//! `offset + loop length`). It is treated as a circle: continuous curves
//! cross-fade into the window's start just before the end, and a hit near
//! the end keeps ringing after the loop point, so the loop stays seamless.
//!
//! **Full track.** The music runs from start to end; loop animations keep
//! cycling underneath.
//!
//! **Time warp.** Motion runs faster while the chosen source is strong. The
//! warped time is rescaled so a full loop still ends exactly where it
//! started, so anything with whole cycles per loop still loops.

use crate::audio::{AudioEnvelope, Curve, HitKind, CURVES, HITS, SPECTRUM_BANDS};
use crate::clock::Timing;
use crate::param::Wave;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MusicMode {
    /// A loop-length window of the song, played as a seamless loop.
    #[default]
    LoopWindow,
    /// The whole song; exports last as long as the song.
    FullTrack,
}

impl MusicMode {
    pub const ALL: [MusicMode; 2] = [MusicMode::LoopWindow, MusicMode::FullTrack];
    pub fn label(self) -> &'static str {
        match self {
            MusicMode::LoopWindow => "Loop a part of the song",
            MusicMode::FullTrack => "Whole song",
        }
    }
}

/// Something in the music a value can react to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioSource {
    Level,
    #[default]
    Kick,
    Bass,
    Mids,
    Highs,
    Brightness,
    Pitch,
    KickHit,
    SnareHit,
    HatHit,
    AnyHit,
    NoteHit,
}

impl AudioSource {
    pub const FOLLOW: [AudioSource; 7] = [
        AudioSource::Level,
        AudioSource::Kick,
        AudioSource::Bass,
        AudioSource::Mids,
        AudioSource::Highs,
        AudioSource::Brightness,
        AudioSource::Pitch,
    ];
    pub const HITS: [AudioSource; 5] = [
        AudioSource::KickHit,
        AudioSource::SnareHit,
        AudioSource::HatHit,
        AudioSource::AnyHit,
        AudioSource::NoteHit,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AudioSource::Level => "Loudness",
            AudioSource::Kick => "Kick (40–120 Hz)",
            AudioSource::Bass => "Bass (60–250 Hz)",
            AudioSource::Mids => "Mids (voices, leads)",
            AudioSource::Highs => "Highs (hats, air)",
            AudioSource::Brightness => "Brightness",
            AudioSource::Pitch => "Melody pitch",
            AudioSource::KickHit => "Each kick",
            AudioSource::SnareHit => "Each snare / clap",
            AudioSource::HatHit => "Each hi-hat",
            AudioSource::AnyHit => "Each hit (any)",
            AudioSource::NoteHit => "Each new note",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            AudioSource::Level => "Follows the overall loudness",
            AudioSource::Kick => "Follows the kick drum's energy",
            AudioSource::Bass => "Follows the bass line's energy",
            AudioSource::Mids => "Follows voices, chords and leads",
            AudioSource::Highs => "Follows hi-hats, cymbals and air",
            AudioSource::Brightness => "Dull sounds 0, bright sounds 1",
            AudioSource::Pitch => {
                "The melody's note: C = 0 … B = 11/12 (one turn of hue per octave)"
            }
            AudioSource::KickHit => "Plays the shape on every detected kick",
            AudioSource::SnareHit => "Plays the shape on every snare or clap",
            AudioSource::HatHit => "Plays the shape on every hi-hat",
            AudioSource::AnyHit => "Plays the shape on every onset",
            AudioSource::NoteHit => "Plays the shape on every new note",
        }
    }

    pub fn curve(self) -> Option<Curve> {
        Some(match self {
            AudioSource::Level => Curve::Level,
            AudioSource::Kick => Curve::Kick,
            AudioSource::Bass => Curve::Bass,
            AudioSource::Mids => Curve::Mids,
            AudioSource::Highs => Curve::Highs,
            AudioSource::Brightness => Curve::Brightness,
            AudioSource::Pitch => Curve::Pitch,
            _ => return None,
        })
    }

    pub fn hit(self) -> Option<HitKind> {
        Some(match self {
            AudioSource::KickHit => HitKind::Kick,
            AudioSource::SnareHit => HitKind::Snare,
            AudioSource::HatHit => HitKind::Hats,
            AudioSource::AnyHit => HitKind::Any,
            AudioSource::NoteHit => HitKind::Note,
            _ => return None,
        })
    }

    pub fn is_hit(self) -> bool {
        self.hit().is_some()
    }
}

/// A link from the music to one value.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MusicMod {
    pub source: AudioSource,
    /// Added at full strength (0 = off).
    pub amount: f32,
    /// Follow: use the smoothed curve instead of the snappy one.
    pub smooth: bool,
    /// Follow: ignore values below this (0..1).
    pub threshold: f32,
    /// Hits: the shape played on every hit (a beat fade).
    pub shape: Wave,
    /// Hits: how long the shape lasts, in beats.
    pub length: f32,
}

impl Default for MusicMod {
    fn default() -> Self {
        MusicMod {
            source: AudioSource::Kick,
            amount: 0.0,
            smooth: false,
            threshold: 0.0,
            shape: Wave::ExpOut,
            length: 0.5,
        }
    }
}

impl MusicMod {
    pub fn is_active(&self) -> bool {
        self.amount != 0.0
    }

    /// Contribution at this moment.
    pub fn eval(&self, m: &MusicFrame, beat_seconds: f32) -> f32 {
        if self.amount == 0.0 || !m.active {
            return 0.0;
        }
        if let Some(c) = self.source.curve() {
            let v = if self.smooth {
                m.smooth[c as usize]
            } else {
                m.fast[c as usize]
            };
            let t = self.threshold.clamp(0.0, 0.99);
            let v = if c == Curve::Pitch {
                v
            } else {
                ((v - t) / (1.0 - t)).max(0.0)
            };
            return self.amount * v;
        }
        let h = m.hits[self.source.hit().map(|k| k as usize).unwrap_or(0)];
        let len = (self.length * beat_seconds).max(1e-3);
        if h.since < 0.0 || h.since >= len {
            return 0.0;
        }
        let x = h.since / len;
        let shape = if self.shape.is_unipolar() {
            self.shape.eval(x, 1)
        } else {
            (1.0 - x).max(0.0)
        };
        self.amount * h.strength * shape
    }
}

/// Time warp: motion speeds up (or slows down) with the music.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TimeWarp {
    pub source: AudioSource,
    /// 0 = off; 2 = up to three times as fast on full hits; negative slows
    /// down instead (down to −0.9).
    pub amount: f32,
}

impl Default for TimeWarp {
    fn default() -> Self {
        TimeWarp {
            source: AudioSource::Kick,
            amount: 0.0,
        }
    }
}

/// Music settings saved with the project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MusicSettings {
    pub mode: MusicMode,
    /// Loop window start in the song (seconds).
    pub offset: f32,
    /// Optional MIDI file with the song's notes (asset path).
    pub midi: Option<String>,
    /// Shift of the MIDI notes against the audio (seconds).
    pub midi_offset: f32,
    pub warp: TimeWarp,
}

impl Default for MusicSettings {
    fn default() -> Self {
        MusicSettings {
            mode: MusicMode::LoopWindow,
            offset: 0.0,
            midi: None,
            midi_offset: 0.0,
            warp: TimeWarp::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HitState {
    /// Seconds since the last hit (huge when there was none).
    pub since: f32,
    pub strength: f32,
    /// Hits so far in the loop window (or the song, in full-track mode).
    pub count: u32,
}

impl Default for HitState {
    fn default() -> Self {
        HitState {
            since: 1e6,
            strength: 0.0,
            count: 0,
        }
    }
}

/// The music at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct MusicFrame {
    /// False without music: every music link is silent.
    pub active: bool,
    pub fast: [f32; CURVES],
    pub smooth: [f32; CURVES],
    pub hits: [HitState; HITS],
    pub spectrum: [f32; SPECTRUM_BANDS],
    pub chroma: [f32; 12],
}

impl MusicFrame {
    /// Spectrum band for copy `i` of `n` (low notes first), 0..1.
    pub fn band_for(&self, i: usize, n: usize) -> f32 {
        if !self.active || n == 0 {
            return 0.0;
        }
        let x = (i as f32 + 0.5) / n as f32 * SPECTRUM_BANDS as f32 - 0.5;
        let a = x.floor().clamp(0.0, (SPECTRUM_BANDS - 1) as f32) as usize;
        let b = (a + 1).min(SPECTRUM_BANDS - 1);
        let t = (x - a as f32).clamp(0.0, 1.0);
        self.spectrum[a] * (1.0 - t) + self.spectrum[b] * t
    }
}

fn mix_arr<const N: usize>(a: [f32; N], b: [f32; N], t: f32) -> [f32; N] {
    std::array::from_fn(|i| a[i] * (1.0 - t) + b[i] * t)
}

/// Music state `t` seconds into the loop window (`LoopWindow`) or the song
/// (`FullTrack`).
pub fn frame_at(env: &AudioEnvelope, s: &MusicSettings, timing: &Timing, t: f32) -> MusicFrame {
    match s.mode {
        MusicMode::FullTrack => song_frame(env, t, None),
        MusicMode::LoopWindow => {
            let len = timing.loop_seconds().max(0.01);
            let t = t.rem_euclid(len);
            let base = song_frame(env, s.offset + t, Some((s.offset, len, t)));
            // Cross-fade into the window's start over the last moments.
            let fade = (len * 0.25).min(0.25);
            if t > len - fade {
                let w = (t - (len - fade)) / fade;
                let w = w * w * (3.0 - 2.0 * w);
                let wrap = song_frame(env, s.offset + t - len, None);
                let mut f = base;
                for c in 0..CURVES {
                    if c == Curve::Pitch as usize {
                        continue;
                    }
                    f.fast[c] = base.fast[c] * (1.0 - w) + wrap.fast[c] * w;
                    f.smooth[c] = base.smooth[c] * (1.0 - w) + wrap.smooth[c] * w;
                }
                f.spectrum = mix_arr(base.spectrum, wrap.spectrum, w);
                f.chroma = mix_arr(base.chroma, wrap.chroma, w);
                return f;
            }
            base
        }
    }
}

/// `window`: (start, length, time into the window) for circular hits.
fn song_frame(env: &AudioEnvelope, ts: f32, window: Option<(f32, f32, f32)>) -> MusicFrame {
    let mut f = MusicFrame {
        active: true,
        ..Default::default()
    };
    let curves = [
        Curve::Level,
        Curve::Kick,
        Curve::Bass,
        Curve::Mids,
        Curve::Highs,
        Curve::Brightness,
        Curve::Pitch,
    ];
    for c in curves {
        f.fast[c as usize] = env.value(c, false, ts);
        f.smooth[c as usize] = env.value(c, true, ts);
    }
    f.spectrum = env.spectrum_at(ts);
    f.chroma = env.chroma_at(ts);
    for k in 0..HITS {
        let kind = [
            HitKind::Kick,
            HitKind::Snare,
            HitKind::Hats,
            HitKind::Any,
            HitKind::Note,
        ][k];
        f.hits[k] = match window {
            None => match env.last_hit(kind, f32::MIN, ts) {
                Some((t, s)) => HitState {
                    since: ts - t,
                    strength: s,
                    ..Default::default()
                },
                None => HitState::default(),
            },
            Some((start, len, t)) => {
                // Hits inside the window only; before the window's first
                // hit, the last one of the window is still ringing.
                match env.last_hit(kind, start, start + t) {
                    Some((ht, s)) => HitState {
                        since: start + t - ht,
                        strength: s,
                        ..Default::default()
                    },
                    None => match env.last_hit(kind, start + t, start + len - 1e-4) {
                        Some((ht, s)) => HitState {
                            since: t + (start + len - ht),
                            strength: s,
                            ..Default::default()
                        },
                        None => HitState::default(),
                    },
                }
            }
        };
        let h = &env.hits[kind as usize];
        let upto = h.partition_point(|(t, _)| *t <= ts);
        let from = window.map_or(0, |(start, _, _)| h.partition_point(|(t, _)| *t < start));
        f.hits[k].count = upto.saturating_sub(from) as u32;
    }
    f
}

/// The loop phase after time warp, for `t` seconds into the loop window
/// (or song, in full-track mode).
pub fn warped_phase(env: &AudioEnvelope, s: &MusicSettings, timing: &Timing, t: f32) -> f32 {
    let len = timing.loop_seconds().max(0.01);
    let k = s.warp.amount.max(-0.9);
    let curve = s
        .warp
        .source
        .curve()
        .filter(|c| *c != Curve::Pitch)
        .unwrap_or(Curve::Kick);
    if k == 0.0 {
        return (t / len).rem_euclid(1.0);
    }
    match s.mode {
        MusicMode::LoopWindow => {
            let t = t.rem_euclid(len);
            let i0 = env.integral(curve, s.offset);
            let it = env.integral(curve, s.offset + t) - i0;
            let il = env.integral(curve, s.offset + len) - i0;
            let tau = (t + k * it) / (len + k * il).max(1e-4) * len;
            (tau / len).clamp(0.0, 1.0 - 1e-7)
        }
        MusicMode::FullTrack => {
            // Average speed 1 over the song.
            let d = env.duration.max(0.01);
            let mean = env.integral(curve, d) / d;
            let tau = (t + k * env.integral(curve, t)) / (1.0 + k * mean).max(1e-4);
            (tau / len).rem_euclid(1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioEnvelope, CURVES};

    /// 8 s of music with a kick every 0.5 s (curves pulse too).
    fn env() -> AudioEnvelope {
        let n = 800;
        let raw: [Vec<f32>; CURVES] = std::array::from_fn(|c| {
            (0..n)
                .map(|i| {
                    let t = i as f32 / 100.0;
                    if c == Curve::Pitch as usize {
                        ((t * 2.0).floor() % 12.0) / 12.0
                    } else {
                        (-(t * 2.0).fract() * 6.0).exp()
                    }
                })
                .collect()
        });
        let mut hits: [Vec<(f32, f32)>; HITS] = Default::default();
        hits[HitKind::Kick as usize] = (0..16).map(|i| (i as f32 * 0.5 + 0.1, 1.0)).collect();
        AudioEnvelope::build(
            raw,
            vec![[0.5; SPECTRUM_BANDS]; n],
            vec![[0.0; 12]; n],
            hits,
            Some((120.0, 0.1)),
            8.0,
            44100,
        )
    }

    fn timing() -> Timing {
        Timing {
            bpm: 120.0,
            loop_beats: 4,
        }
    }

    #[test]
    fn loop_window_is_seamless() {
        let e = env();
        let s = MusicSettings {
            offset: 1.3,
            ..Default::default()
        };
        let tm = timing();
        let len = tm.loop_seconds();
        let a = frame_at(&e, &s, &tm, 0.0);
        let b = frame_at(&e, &s, &tm, len - 1e-4);
        for c in 0..CURVES - 1 {
            assert!((a.fast[c] - b.fast[c]).abs() < 0.02, "curve {c}");
        }
        // A kick near the end of the window rings on after the loop point.
        let k = HitKind::Kick as usize;
        assert!((a.hits[k].since - (b.hits[k].since + 1e-4)).abs() < 1e-3);
        let m = MusicMod {
            source: AudioSource::KickHit,
            amount: 1.0,
            length: 1.0,
            ..Default::default()
        };
        let beat = tm.beat_seconds();
        assert!((m.eval(&a, beat) - m.eval(&b, beat)).abs() < 0.01);
    }

    #[test]
    fn warp_keeps_the_loop_and_speeds_up_on_kicks() {
        let e = env();
        let s = MusicSettings {
            offset: 0.7,
            warp: TimeWarp {
                source: AudioSource::Kick,
                amount: 3.0,
            },
            ..Default::default()
        };
        let tm = timing();
        let len = tm.loop_seconds();
        assert!(warped_phase(&e, &s, &tm, 0.0).abs() < 1e-5);
        assert!(warped_phase(&e, &s, &tm, len - 1e-4) > 0.999);
        let mut last = -1.0;
        let mut speeds = Vec::new();
        for i in 0..200 {
            let t = i as f32 / 200.0 * len;
            let p = warped_phase(&e, &s, &tm, t);
            assert!(p >= last, "warp must never run backwards");
            speeds.push(p - last);
            last = p;
        }
        let max = speeds[1..].iter().cloned().fold(0.0, f32::max);
        let min = speeds[1..].iter().cloned().fold(1.0, f32::min);
        assert!(max > min * 2.0, "no surge: {min} .. {max}");
    }

    #[test]
    fn follow_and_hit_links() {
        let e = env();
        let s = MusicSettings::default();
        let tm = timing();
        let on = frame_at(&e, &s, &tm, 0.11);
        let off = frame_at(&e, &s, &tm, 0.45);
        let hit = MusicMod {
            source: AudioSource::KickHit,
            amount: 2.0,
            length: 0.5,
            ..Default::default()
        };
        let beat = tm.beat_seconds();
        assert!(hit.eval(&on, beat) > 1.5);
        assert!(hit.eval(&off, beat) < 0.1);
        let follow = MusicMod {
            source: AudioSource::Kick,
            amount: 1.0,
            ..Default::default()
        };
        assert!(follow.eval(&on, beat) > follow.eval(&off, beat));
        assert_eq!(follow.eval(&MusicFrame::default(), beat), 0.0);
    }
}
