//! Pre-analysed music (see [`crate::analysis`]) used as modulation sources.

/// Continuous curves, 100 samples per second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    /// Overall loudness.
    Level = 0,
    /// 40–120 Hz: the kick drum.
    Kick,
    /// 60–250 Hz.
    Bass,
    /// 250 Hz – 2 kHz: voices, chords, leads.
    Mids,
    /// 6–16 kHz: hats, cymbals, air.
    Highs,
    /// Spectral centroid: dull (0) … bright (1).
    Brightness,
    /// Strongest pitch class / 12 (C = 0, C♯ = 1/12 …), held while unclear.
    Pitch,
}
pub const CURVES: usize = 7;

/// Detected (or MIDI) hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum HitKind {
    #[default]
    Kick = 0,
    Snare,
    Hats,
    /// Any onset.
    Any,
    /// New notes (melody).
    Note,
}
pub const HITS: usize = 5;

/// Log-spaced bands from 40 Hz to 16 kHz.
pub const SPECTRUM_BANDS: usize = 16;

/// Everything the analysis found in a piece of music.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioEnvelope {
    /// Curve samples per second.
    pub rate: f32,
    /// Length of the music in seconds.
    pub duration: f32,
    /// Sample rate of the decoded audio.
    pub sample_rate: u32,
    /// Curves with a quick attack and release (0..1).
    pub fast: [Vec<f32>; CURVES],
    /// Smoothed curves (0..1).
    pub smooth: [Vec<f32>; CURVES],
    /// Running integral of `fast` (seconds × value), one longer.
    pub prefix: [Vec<f32>; CURVES],
    pub spectrum: Vec<[f32; SPECTRUM_BANDS]>,
    /// Pitch-class profile per frame (C … B), strongest = 1.
    pub chroma: Vec<[f32; 12]>,
    /// Hits per kind: (seconds, strength 0..1), sorted.
    pub hits: [Vec<(f32, f32)>; HITS],
    /// Detected tempo and first bar downbeat (seconds).
    pub tempo: Option<(f32, f32)>,
    /// Which hit kinds come from a MIDI file.
    pub midi_hits: [bool; HITS],
}

fn follow(v: &[f32], attack: f32, release: f32, rate: f32) -> Vec<f32> {
    let dt = 1.0 / rate;
    let ka = 1.0 - (-dt / attack.max(1e-4)).exp();
    let kr = 1.0 - (-dt / release.max(1e-4)).exp();
    let mut y = 0.0f32;
    v.iter()
        .map(|&x| {
            y += (if x > y { ka } else { kr }) * (x - y);
            y
        })
        .collect()
}

impl AudioEnvelope {
    /// Build from raw (normalised) curves; derives the fast / smooth
    /// variants and the integrals.
    pub fn build(
        raw: [Vec<f32>; CURVES],
        spectrum: Vec<[f32; SPECTRUM_BANDS]>,
        chroma: Vec<[f32; 12]>,
        hits: [Vec<(f32, f32)>; HITS],
        tempo: Option<(f32, f32)>,
        duration: f32,
        sample_rate: u32,
    ) -> AudioEnvelope {
        let rate = crate::analysis::FRAME_RATE;
        let mut env = AudioEnvelope {
            rate,
            duration,
            sample_rate,
            spectrum,
            chroma,
            hits,
            tempo,
            ..Default::default()
        };
        env.set_curves(raw);
        env
    }

    fn set_curves(&mut self, raw: [Vec<f32>; CURVES]) {
        for (c, v) in raw.into_iter().enumerate() {
            let (fast, smooth) = if c == Curve::Pitch as usize {
                (v.clone(), v)
            } else {
                (
                    follow(&v, 0.005, 0.12, self.rate),
                    follow(&v, 0.08, 0.45, self.rate),
                )
            };
            let mut prefix = Vec::with_capacity(smooth.len() + 1);
            let mut acc = 0.0f64;
            prefix.push(0.0);
            for x in &fast {
                acc += *x as f64 / self.rate as f64;
                prefix.push(acc as f32);
            }
            self.fast[c] = fast;
            self.smooth[c] = smooth;
            self.prefix[c] = prefix;
        }
    }

    fn frames(&self) -> usize {
        self.fast[0].len()
    }

    /// Curve value at `seconds` (linear interpolation, wrapping around the
    /// music's length).
    pub fn value(&self, curve: Curve, smooth: bool, seconds: f32) -> f32 {
        let v = if smooth {
            &self.smooth[curve as usize]
        } else {
            &self.fast[curve as usize]
        };
        let n = v.len();
        if n == 0 || self.rate <= 0.0 {
            return 0.0;
        }
        let pos = (seconds * self.rate).rem_euclid(n as f32);
        let i0 = (pos.floor() as usize) % n;
        let i1 = (i0 + 1) % n;
        let t = pos.fract();
        if curve == Curve::Pitch {
            return v[i0];
        }
        v[i0] * (1.0 - t) + v[i1] * t
    }

    /// `(level, kick)` at `seconds`, wrapping around the music's length.
    pub fn sample(&self, seconds: f32) -> (f32, f32) {
        (
            self.value(Curve::Level, false, seconds),
            self.value(Curve::Kick, false, seconds),
        )
    }

    /// Integral of the fast curve from 0 to `seconds` (not wrapped;
    /// beyond the end it grows with the mean).
    pub fn integral(&self, curve: Curve, seconds: f32) -> f32 {
        let p = &self.prefix[curve as usize];
        let n = p.len().saturating_sub(1);
        if n == 0 {
            return 0.0;
        }
        let total = p[n];
        let len = n as f32 / self.rate;
        let laps = (seconds / len).floor();
        let rest = seconds - laps * len;
        let pos = (rest * self.rate).clamp(0.0, n as f32);
        let i0 = (pos.floor() as usize).min(n);
        let i1 = (i0 + 1).min(n);
        let t = pos - i0 as f32;
        laps * total + p[i0] * (1.0 - t) + p[i1] * t
    }

    pub fn spectrum_at(&self, seconds: f32) -> [f32; SPECTRUM_BANDS] {
        self.frame_of(&self.spectrum, seconds)
    }

    pub fn chroma_at(&self, seconds: f32) -> [f32; 12] {
        self.frame_of(&self.chroma, seconds)
    }

    fn frame_of<const N: usize>(&self, v: &[[f32; N]], seconds: f32) -> [f32; N] {
        if v.is_empty() {
            return [0.0; N];
        }
        let i = ((seconds * self.rate).rem_euclid(v.len() as f32) as usize).min(v.len() - 1);
        v[i]
    }

    /// The last hit of `kind` at or before `seconds` and after `after`.
    pub fn last_hit(&self, kind: HitKind, after: f32, seconds: f32) -> Option<(f32, f32)> {
        let h = &self.hits[kind as usize];
        let idx = h.partition_point(|(t, _)| *t <= seconds);
        if idx == 0 {
            return None;
        }
        let (t, s) = h[idx - 1];
        (t >= after).then_some((t, s))
    }

    /// An envelope made only from MIDI notes (no audio file).
    pub fn from_midi(midi: &crate::midi::MidiData) -> AudioEnvelope {
        let rate = crate::analysis::FRAME_RATE;
        let duration = midi.duration.max(0.1);
        let frames = (duration * rate).ceil() as usize + 1;
        let mut env = AudioEnvelope {
            rate,
            duration,
            sample_rate: 44100,
            spectrum: vec![[0.0; SPECTRUM_BANDS]; frames],
            chroma: vec![[0.0; 12]; frames],
            ..Default::default()
        };
        env.set_curves(std::array::from_fn(|_| vec![0.0; frames]));
        env.apply_midi(midi, 0.0);
        env
    }

    /// Replace hits (and the pitch curve) with the notes of a MIDI file,
    /// shifted by `offset` seconds. Drums (channel 10) give kick, snare and
    /// hat hits; other channels give note hits, the pitch and the level.
    pub fn apply_midi(&mut self, midi: &crate::midi::MidiData, offset: f32) {
        use crate::midi::DrumKind;
        let mut hits: [Vec<(f32, f32)>; HITS] = Default::default();
        for n in &midi.notes {
            let t = n.time + offset;
            if t < 0.0 {
                continue;
            }
            let v = n.velocity as f32 / 127.0;
            let kind = match n.drum() {
                Some(DrumKind::Kick) => Some(HitKind::Kick),
                Some(DrumKind::Snare) => Some(HitKind::Snare),
                Some(DrumKind::Hat) => Some(HitKind::Hats),
                Some(DrumKind::Other) => None,
                None => Some(HitKind::Note),
            };
            if let Some(k) = kind {
                hits[k as usize].push((t, v));
            }
            hits[HitKind::Any as usize].push((t, v));
        }
        for (k, h) in hits.iter_mut().enumerate() {
            if h.is_empty() {
                continue;
            }
            h.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            // Chords: keep one hit per moment (the strongest).
            h.dedup_by(|b, a| {
                if (b.0 - a.0).abs() < 0.015 {
                    a.1 = a.1.max(b.1);
                    true
                } else {
                    false
                }
            });
            self.hits[k] = std::mem::take(h);
            self.midi_hits[k] = true;
        }
        // Pitch: the highest note of the melody sounding, held.
        let melody: Vec<_> = midi.notes.iter().filter(|n| n.drum().is_none()).collect();
        if !melody.is_empty() {
            let frames = self.frames();
            let mut pitch = vec![0.0f32; frames];
            let mut level = vec![0.0f32; frames];
            let mut top = vec![0u8; frames];
            for n in &melody {
                let a = (((n.time + offset) * self.rate).max(0.0) as usize).min(frames);
                let b = (((n.time + n.length + offset) * self.rate).max(0.0) as usize).min(frames);
                for i in a..b {
                    if level[i] == 0.0 || n.key > top[i] {
                        top[i] = n.key;
                        pitch[i] = (n.key % 12) as f32 / 12.0;
                    }
                    level[i] = level[i].max(n.velocity as f32 / 127.0);
                }
            }
            let mut held = 0.0;
            for (i, p) in pitch.iter_mut().enumerate() {
                if level[i] > 0.0 {
                    held = *p;
                }
                *p = held;
            }
            let mut raw: [Vec<f32>; CURVES] = std::array::from_fn(|c| self.fast[c].clone());
            raw[Curve::Pitch as usize] = pitch;
            if self.fast[Curve::Level as usize].iter().all(|v| *v == 0.0) {
                raw[Curve::Level as usize] = level;
            }
            self.set_curves(raw);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp_env() -> AudioEnvelope {
        let n = 400;
        let raw: [Vec<f32>; CURVES] =
            std::array::from_fn(|_| (0..n).map(|i| i as f32 / n as f32).collect());
        AudioEnvelope::build(
            raw,
            vec![[0.0; SPECTRUM_BANDS]; n],
            vec![[0.0; 12]; n],
            Default::default(),
            None,
            4.0,
            44100,
        )
    }

    #[test]
    fn sampling_wraps() {
        let e = ramp_env();
        assert_eq!(e.sample(0.0), e.sample(4.0));
        assert!(e.value(Curve::Level, true, 3.0) > e.value(Curve::Level, true, 1.0));
    }

    #[test]
    fn integral_is_monotonic_and_extends() {
        let e = ramp_env();
        let mut last = -1.0;
        for i in 0..100 {
            let v = e.integral(Curve::Level, i as f32 * 0.1);
            assert!(v >= last);
            last = v;
        }
        let one = e.integral(Curve::Level, 4.0);
        assert!((e.integral(Curve::Level, 8.0) - 2.0 * one).abs() < 1e-3);
    }
}
