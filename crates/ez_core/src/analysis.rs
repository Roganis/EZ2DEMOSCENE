//! Offline music analysis: frequency bands, envelopes, hits (kick, snare,
//! hats, any onset), a 16-band spectrum, a pitch-class profile and the
//! tempo. Also a small live analyser for microphone input.
//!
//! Everything runs on mono samples at 100 analysis frames per second, with
//! a 2048-point FFT (a plain radix-2 one: no dependencies, fast enough for a
//! whole song in well under a second).

use crate::audio::{AudioEnvelope, Curve, HitKind, CURVES, HITS, SPECTRUM_BANDS};
use crate::music::MusicFrame;

/// Analysis frames per second.
pub const FRAME_RATE: f32 = 100.0;
const FFT_SIZE: usize = 2048;

// ---------------------------------------------------------------------------
// FFT

/// In-place iterative radix-2 FFT of (re, im); `re.len()` must be a power
/// of two.
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);
    // Bit reversal.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -std::f32::consts::TAU / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let a = start + k;
                let b = a + len / 2;
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

// ---------------------------------------------------------------------------
// Per-frame features

/// Raw features of one analysis window.
#[derive(Clone, Debug, Default)]
struct Features {
    level: f32,
    kick: f32,
    bass: f32,
    mids: f32,
    highs: f32,
    brightness: f32,
    spectrum: [f32; SPECTRUM_BANDS],
    chroma: [f32; 12],
    /// Log magnitudes (for spectral flux).
    logmag: Vec<f32>,
}

struct Analyzer {
    rate: f32,
    window: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    band_edges: [f32; SPECTRUM_BANDS + 1],
}

impl Analyzer {
    fn new(rate: f32) -> Analyzer {
        let window = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (FFT_SIZE - 1) as f32).cos())
            .collect();
        // Log-spaced bands from 40 Hz to 16 kHz.
        let mut band_edges = [0.0; SPECTRUM_BANDS + 1];
        for (i, e) in band_edges.iter_mut().enumerate() {
            *e = 40.0 * (16000.0f32 / 40.0).powf(i as f32 / SPECTRUM_BANDS as f32);
        }
        Analyzer {
            rate,
            window,
            re: vec![0.0; FFT_SIZE],
            im: vec![0.0; FFT_SIZE],
            band_edges,
        }
    }

    /// Features of the window of `samples` centred on `centre`.
    fn features(&mut self, samples: &[f32], centre: isize) -> Features {
        let start = centre - FFT_SIZE as isize / 2;
        let mut sq = 0.0f32;
        for i in 0..FFT_SIZE {
            let idx = start + i as isize;
            let s = if idx >= 0 && (idx as usize) < samples.len() {
                samples[idx as usize]
            } else {
                0.0
            };
            sq += s * s;
            self.re[i] = s * self.window[i];
            self.im[i] = 0.0;
        }
        fft(&mut self.re, &mut self.im);
        let half = FFT_SIZE / 2;
        let bin_hz = self.rate / FFT_SIZE as f32;
        let mut f = Features {
            level: (sq / FFT_SIZE as f32).sqrt(),
            logmag: Vec::with_capacity(half),
            ..Default::default()
        };
        let (mut num, mut den) = (0.0f32, 0.0f32);
        for k in 1..half {
            let m = (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt() / FFT_SIZE as f32;
            let hz = k as f32 * bin_hz;
            let e = m * m;
            if (40.0..120.0).contains(&hz) {
                f.kick += e;
            }
            if (60.0..250.0).contains(&hz) {
                f.bass += e;
            }
            if (250.0..2000.0).contains(&hz) {
                f.mids += e;
            }
            if (6000.0..16000.0).contains(&hz) {
                f.highs += e;
            }
            if hz >= 40.0 {
                num += hz * m;
                den += m;
                if let Some(b) = self
                    .band_edges
                    .windows(2)
                    .position(|w| hz >= w[0] && hz < w[1])
                {
                    f.spectrum[b] += e;
                }
            }
            if (100.0..2000.0).contains(&hz) {
                let midi = 69.0 + 12.0 * (hz / 440.0).log2();
                let pc = (midi.round() as i32).rem_euclid(12) as usize;
                f.chroma[pc] += m;
            }
            f.logmag.push((1.0 + m * 1000.0).ln());
        }
        f.kick = f.kick.sqrt();
        f.bass = f.bass.sqrt();
        f.mids = f.mids.sqrt();
        f.highs = f.highs.sqrt();
        for s in &mut f.spectrum {
            *s = (1.0 + s.sqrt() * 50.0).ln();
        }
        let centroid = if den > 0.0 { num / den } else { 0.0 };
        f.brightness =
            ((centroid.max(1.0) / 200.0).log2() / (8000.0f32 / 200.0).log2()).clamp(0.0, 1.0);
        f
    }

    /// Spectral flux of the bins inside `lo..hi` Hz.
    fn flux(&self, cur: &[f32], prev: &[f32], lo: f32, hi: f32) -> f32 {
        let bin_hz = self.rate / FFT_SIZE as f32;
        let a = ((lo / bin_hz) as usize).max(1) - 1;
        let b = ((hi / bin_hz) as usize).min(cur.len());
        let mut s = 0.0;
        for k in a..b {
            s += (cur[k] - prev[k]).max(0.0);
        }
        s
    }
}

// ---------------------------------------------------------------------------
// Helpers

/// Scales so the 98th percentile becomes 1 (clamped).
fn normalize(v: &mut [f32]) {
    if v.is_empty() {
        return;
    }
    let mut sorted: Vec<f32> = v.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let peak = sorted[((sorted.len() as f32 * 0.98) as usize).min(sorted.len() - 1)].max(1e-9);
    for x in v.iter_mut() {
        *x = (*x / peak).clamp(0.0, 1.0);
    }
}

/// Peaks of an onset (flux) curve: above a moving average, local maxima,
/// at least `gap` seconds apart. Returns (seconds, strength 0..1).
fn pick_peaks(flux: &[f32], sensitivity: f32, gap: f32) -> Vec<(f32, f32)> {
    let mut norm = flux.to_vec();
    normalize(&mut norm);
    let n = norm.len();
    let w = 12usize;
    let mut out: Vec<(f32, f32)> = Vec::new();
    let min_gap = (gap * FRAME_RATE) as usize;
    let mut last: Option<usize> = None;
    for i in 0..n {
        let lo = i.saturating_sub(w);
        let hi = (i + w + 1).min(n);
        let mean = norm[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
        let x = norm[i];
        if x < mean * 1.4 + 0.08 * sensitivity {
            continue;
        }
        let a = i.saturating_sub(3);
        let b = (i + 4).min(n);
        if norm[a..b].iter().any(|&y| y > x) {
            continue;
        }
        if let Some(l) = last {
            if i - l < min_gap {
                continue;
            }
        }
        last = Some(i);
        out.push((i as f32 / FRAME_RATE, x.clamp(0.0, 1.0)));
    }
    out
}

/// Tempo from an onset curve: autocorrelation over 60–200 BPM with a mild
/// preference for ~120, then the beat offset with the strongest onsets and
/// the bar downbeat with the strongest kicks. Returns (bpm, first
/// downbeat in seconds).
pub fn detect_tempo(onset: &[f32], kick: &[f32]) -> Option<(f32, f32)> {
    let n = onset.len();
    if n < (FRAME_RATE * 3.0) as usize {
        return None;
    }
    let mean = onset.iter().sum::<f32>() / n as f32;
    let o: Vec<f32> = onset.iter().map(|x| (x - mean).max(0.0)).collect();
    let min_lag = (60.0 * FRAME_RATE / 200.0) as usize;
    let max_lag = (60.0 * FRAME_RATE / 60.0) as usize;
    let mut scores = vec![0.0f32; max_lag + 2];
    for (lag, score) in scores
        .iter_mut()
        .enumerate()
        .take(max_lag + 1)
        .skip(min_lag)
    {
        let mut s = 0.0;
        for i in 0..n.saturating_sub(lag * 2) {
            s += o[i] * (o[i + lag] + 0.5 * o[i + lag * 2]);
        }
        let bpm = 60.0 * FRAME_RATE / lag as f32;
        let prior = (-0.5 * ((bpm / 120.0).log2() / 0.9).powi(2)).exp();
        *score = s * prior;
    }
    let (best, _) = scores
        .iter()
        .enumerate()
        .skip(min_lag)
        .take(max_lag - min_lag + 1)
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))?;
    if scores[best] <= 0.0 {
        return None;
    }
    // Parabolic refinement of the lag.
    let (a, b, c) = (scores[best - 1], scores[best], scores[best + 1]);
    let den = a - 2.0 * b + c;
    let shift = if den.abs() > 1e-9 {
        (0.5 * (a - c) / den).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let lag = best as f32 + shift;
    let mut bpm = 60.0 * FRAME_RATE / lag;
    while bpm < 70.0 {
        bpm *= 2.0;
    }
    while bpm > 180.0 {
        bpm /= 2.0;
    }
    let period = 60.0 * FRAME_RATE / bpm;
    // Beat offset: phase whose beats land on the most onset energy.
    let steps = period.ceil() as usize;
    let sample = |v: &[f32], t: f32| -> f32 {
        let i = t.round() as usize;
        if i < v.len() {
            v[i]
        } else {
            0.0
        }
    };
    let mut best_off = 0usize;
    let mut best_s = -1.0f32;
    for off in 0..steps {
        let mut s = 0.0;
        let mut t = off as f32;
        while (t as usize) < n {
            s += sample(&o, t);
            t += period;
        }
        if s > best_s {
            best_s = s;
            best_off = off;
        }
    }
    // Downbeat: which of the four beats of a bar has the strongest kicks.
    let mut best_bar = 0usize;
    let mut best_k = -1.0f32;
    for b in 0..4 {
        let mut s = 0.0;
        let mut t = best_off as f32 + b as f32 * period;
        while (t as usize) < n {
            s += sample(kick, t);
            t += period * 4.0;
        }
        if s > best_k {
            best_k = s;
            best_bar = b;
        }
    }
    let first = (best_off as f32 + best_bar as f32 * period) / FRAME_RATE;
    Some((bpm, first))
}

// ---------------------------------------------------------------------------
// Whole-song analysis

/// Analyse mono `samples` at `rate` Hz.
pub fn analyze(samples: &[f32], rate: f32) -> AudioEnvelope {
    let mut a = Analysis::new(samples.to_vec(), rate);
    a.step(usize::MAX);
    a.finish()
}

/// A whole-song analysis that can be run a slice at a time (so a browser
/// tab stays responsive): [`Analysis::step`] until it returns `true`, then
/// [`Analysis::finish`]. The result is identical to [`analyze`].
pub struct Analysis {
    samples: Vec<f32>,
    rate: f32,
    hop: f32,
    frames: usize,
    next: usize,
    an: Analyzer,
    raw: [Vec<f32>; CURVES],
    spectrum: Vec<[f32; SPECTRUM_BANDS]>,
    chroma: Vec<[f32; 12]>,
    flux: [Vec<f32>; HITS],
    prev: Vec<f32>,
}

impl Analysis {
    pub fn new(samples: Vec<f32>, rate: f32) -> Analysis {
        let hop = rate / FRAME_RATE;
        let frames = ((samples.len() as f32 / hop).ceil() as usize).max(1);
        Analysis {
            samples,
            rate,
            hop,
            frames,
            next: 0,
            an: Analyzer::new(rate),
            raw: Default::default(),
            spectrum: Vec::with_capacity(frames),
            chroma: Vec::with_capacity(frames),
            flux: Default::default(),
            prev: vec![0.0; FFT_SIZE / 2 - 1],
        }
    }

    /// Analysis frames in the whole song (100 per second).
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// 0..1.
    pub fn progress(&self) -> f32 {
        self.next as f32 / self.frames as f32
    }

    /// Analyse up to `max_frames` more frames; `true` once all are done.
    pub fn step(&mut self, max_frames: usize) -> bool {
        let end = self.next.saturating_add(max_frames).min(self.frames);
        let (an, raw, flux) = (&mut self.an, &mut self.raw, &mut self.flux);
        for f in self.next..end {
            let centre = (f as f32 * self.hop) as isize;
            let ft = an.features(&self.samples, centre);
            raw[Curve::Level as usize].push(ft.level);
            raw[Curve::Kick as usize].push(ft.kick);
            raw[Curve::Bass as usize].push(ft.bass);
            raw[Curve::Mids as usize].push(ft.mids);
            raw[Curve::Highs as usize].push(ft.highs);
            raw[Curve::Brightness as usize].push(ft.brightness);
            self.spectrum.push(ft.spectrum);
            self.chroma.push(ft.chroma);
            let prev = &self.prev;
            flux[HitKind::Kick as usize].push(an.flux(&ft.logmag, prev, 40.0, 130.0));
            flux[HitKind::Snare as usize].push(an.flux(&ft.logmag, prev, 1000.0, 5000.0));
            flux[HitKind::Hats as usize].push(an.flux(&ft.logmag, prev, 7000.0, 16000.0));
            flux[HitKind::Any as usize].push(an.flux(&ft.logmag, prev, 30.0, 16000.0));
            flux[HitKind::Note as usize].push(an.flux(&ft.logmag, prev, 150.0, 2000.0));
            self.prev = ft.logmag;
        }
        self.next = end;
        end == self.frames
    }

    /// Build the envelope; runs any frames not yet analysed first.
    pub fn finish(mut self) -> AudioEnvelope {
        self.step(usize::MAX);
        let Analysis {
            samples,
            rate,
            frames,
            raw,
            spectrum,
            chroma,
            flux,
            ..
        } = self;
        finish(&samples, rate, frames, raw, spectrum, chroma, flux)
    }
}

fn finish(
    samples: &[f32],
    rate: f32,
    frames: usize,
    mut raw: [Vec<f32>; CURVES],
    mut spectrum: Vec<[f32; SPECTRUM_BANDS]>,
    mut chroma: Vec<[f32; 12]>,
    flux: [Vec<f32>; HITS],
) -> AudioEnvelope {
    for c in [
        Curve::Level,
        Curve::Kick,
        Curve::Bass,
        Curve::Mids,
        Curve::Highs,
    ] {
        normalize(&mut raw[c as usize]);
    }
    // Spectrum bands: each normalised on its own so quiet highs still move.
    for b in 0..SPECTRUM_BANDS {
        let mut col: Vec<f32> = spectrum
            .iter()
            .map(|s: &[f32; SPECTRUM_BANDS]| s[b])
            .collect();
        normalize(&mut col);
        for (s, v) in spectrum.iter_mut().zip(col) {
            s[b] = v;
        }
    }
    // Chroma: normalised per frame; the pitch curve is the strongest pitch
    // class, held while the sound is too quiet or unclear.
    let mut pitch = Vec::with_capacity(frames);
    let mut held = 0.0f32;
    let loud = &raw[Curve::Level as usize];
    for (i, c) in chroma.iter_mut().enumerate() {
        let max = c.iter().cloned().fold(0.0f32, f32::max);
        let sum: f32 = c.iter().sum();
        if max > 0.0 {
            for x in c.iter_mut() {
                *x /= max;
            }
        }
        let clear = sum > 0.0 && max / sum > 0.14 && loud[i] > 0.05;
        if clear {
            let pc = c
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(0);
            held = pc as f32 / 12.0;
        }
        pitch.push(held);
    }
    raw[Curve::Pitch as usize] = pitch;

    let mut hits: [Vec<(f32, f32)>; HITS] = Default::default();
    let gaps = [0.12, 0.12, 0.07, 0.08, 0.1];
    for k in 0..HITS {
        hits[k] = pick_peaks(&flux[k], 1.0, gaps[k]);
    }
    // Kicks also need real low-end energy.
    let kick_curve = &raw[Curve::Kick as usize];
    hits[HitKind::Kick as usize].retain(|(t, _)| {
        let i = ((t * FRAME_RATE) as usize + 2).min(kick_curve.len() - 1);
        kick_curve[i] > 0.25
    });

    let mut onset = flux[HitKind::Any as usize].clone();
    normalize(&mut onset);
    let tempo = detect_tempo(&onset, &flux[HitKind::Kick as usize]);

    let duration = samples.len() as f32 / rate;
    AudioEnvelope::build(raw, spectrum, chroma, hits, tempo, duration, rate as u32)
}

// ---------------------------------------------------------------------------
// Song sections

/// Split the song into sections where its sound changes (a drop, a
/// breakdown, a new part), as lengths in bars from `start` seconds; bars
/// are `4 × beat_seconds` long. Sections are at least `min_bars` long and
/// the lengths add up to every whole bar left in the song.
pub fn sections(env: &AudioEnvelope, beat_seconds: f32, start: f32, min_bars: u32) -> Vec<u32> {
    let bar = beat_seconds.max(0.05) * 4.0;
    let bars = ((env.duration - start) / bar).floor().max(0.0) as usize;
    if bars < 2 {
        return vec![bars.max(1) as u32];
    }
    // One feature vector per bar: average spectrum and loudness.
    const SAMPLES: usize = 16;
    let feature = |b: usize| {
        let mut f = [0.0f32; SPECTRUM_BANDS + 1];
        for k in 0..SAMPLES {
            let t = start + (b as f32 + (k as f32 + 0.5) / SAMPLES as f32) * bar;
            let s = env.spectrum_at(t);
            for (x, v) in f.iter_mut().zip(s) {
                *x += v / SAMPLES as f32;
            }
            f[SPECTRUM_BANDS] += env.value(Curve::Level, true, t) * 2.0 / SAMPLES as f32;
        }
        f
    };
    let feats: Vec<[f32; SPECTRUM_BANDS + 1]> = (0..bars).map(feature).collect();
    // Novelty at each bar boundary: how different the bars before and
    // after are.
    let novelty: Vec<f32> = (0..=bars)
        .map(|b| {
            let w = 4.min(b).min(bars - b);
            if w == 0 {
                return 0.0;
            }
            let mean = |r: std::ops::Range<usize>| {
                let mut m = [0.0f32; SPECTRUM_BANDS + 1];
                for f in &feats[r.clone()] {
                    for (a, v) in m.iter_mut().zip(f) {
                        *a += v / r.len() as f32;
                    }
                }
                m
            };
            let (a, c) = (mean(b - w..b), mean(b..b + w));
            a.iter()
                .zip(&c)
                .map(|(x, y)| (x - y) * (x - y))
                .sum::<f32>()
                .sqrt()
        })
        .collect();
    let inner = &novelty[1..bars];
    let avg = inner.iter().sum::<f32>() / inner.len().max(1) as f32;
    let sd =
        (inner.iter().map(|v| (v - avg).powi(2)).sum::<f32>() / inner.len().max(1) as f32).sqrt();
    let min_bars = min_bars.max(1) as usize;
    let mut candidates: Vec<usize> = (1..bars)
        .filter(|&b| novelty[b] >= novelty[b - 1] && novelty[b] >= novelty[b + 1])
        .filter(|&b| novelty[b] > avg + 0.5 * sd && novelty[b] > 1e-3)
        .collect();
    candidates.sort_by(|a, b| novelty[*b].total_cmp(&novelty[*a]));
    let mut cuts: Vec<usize> = Vec::new();
    for b in candidates {
        if b < min_bars || bars - b < min_bars {
            continue;
        }
        if cuts.iter().all(|c| c.abs_diff(b) >= min_bars) {
            cuts.push(b);
        }
    }
    cuts.sort();
    let mut out = Vec::new();
    let mut last = 0;
    for c in cuts.into_iter().chain(std::iter::once(bars)) {
        out.push((c - last) as u32);
        last = c;
    }
    out
}

// ---------------------------------------------------------------------------
// Live analysis (microphone)

/// Real-time analysis of an incoming stream. Feed it samples, then read a
/// [`MusicFrame`] for "now". Not deterministic, so only for previews.
pub struct LiveAnalyzer {
    an: Analyzer,
    buf: Vec<f32>,
    prev: Vec<f32>,
    /// Running peaks used to normalise each curve.
    peaks: [f32; CURVES],
    flux_avg: [f32; HITS],
    fast: [f32; CURVES],
    smooth: [f32; CURVES],
    spectrum_peak: [f32; SPECTRUM_BANDS],
    frame: MusicFrame,
    since_frames: u64,
}

impl LiveAnalyzer {
    pub fn new(rate: f32) -> LiveAnalyzer {
        LiveAnalyzer {
            an: Analyzer::new(rate),
            buf: Vec::new(),
            prev: vec![0.0; FFT_SIZE / 2 - 1],
            peaks: [1e-3; CURVES],
            flux_avg: [1e-3; HITS],
            fast: [0.0; CURVES],
            smooth: [0.0; CURVES],
            spectrum_peak: [1e-3; SPECTRUM_BANDS],
            frame: MusicFrame::default(),
            since_frames: 0,
        }
    }

    /// Add mono samples; analyses every 10 ms of new audio.
    pub fn push(&mut self, samples: &[f32]) {
        let hop = (self.an.rate / FRAME_RATE) as usize;
        self.buf.extend_from_slice(samples);
        let keep = FFT_SIZE + hop * 8;
        while self.buf.len() >= FFT_SIZE + hop {
            let centre = (self.buf.len() - FFT_SIZE / 2) as isize;
            let buf = std::mem::take(&mut self.buf);
            self.step(&buf, centre);
            self.buf = buf;
            let drop = hop.min(self.buf.len());
            self.buf.drain(..drop);
            if self.buf.len() > keep {
                let extra = self.buf.len() - keep;
                self.buf.drain(..extra);
            }
        }
    }

    #[allow(clippy::needless_range_loop)]
    fn step(&mut self, samples: &[f32], centre: isize) {
        let ft = self.an.features(samples, centre);
        let vals = [
            ft.level,
            ft.kick,
            ft.bass,
            ft.mids,
            ft.highs,
            ft.brightness,
            0.0,
        ];
        let dt = 1.0 / FRAME_RATE;
        for c in 0..CURVES {
            if c == Curve::Pitch as usize {
                continue;
            }
            let v = if c == Curve::Brightness as usize {
                vals[c]
            } else {
                // Slowly forgetting peak for normalisation.
                self.peaks[c] = (self.peaks[c] * (1.0 - dt / 8.0)).max(vals[c]).max(1e-4);
                (vals[c] / self.peaks[c]).clamp(0.0, 1.0)
            };
            let ka = 1.0 - (-dt / 0.01f32).exp();
            let kr = 1.0 - (-dt / 0.12f32).exp();
            self.fast[c] += (if v > self.fast[c] { ka } else { kr }) * (v - self.fast[c]);
            let ka = 1.0 - (-dt / 0.08f32).exp();
            let kr = 1.0 - (-dt / 0.4f32).exp();
            self.smooth[c] += (if v > self.smooth[c] { ka } else { kr }) * (v - self.smooth[c]);
        }
        let max = ft.chroma.iter().cloned().fold(0.0f32, f32::max);
        if max > 0.0 && self.fast[Curve::Level as usize] > 0.05 {
            let pc = ft.chroma.iter().position(|&x| x == max).unwrap_or(0);
            self.fast[Curve::Pitch as usize] = pc as f32 / 12.0;
            self.smooth[Curve::Pitch as usize] = pc as f32 / 12.0;
        }
        let ranges = [
            (40.0, 130.0),
            (1000.0, 5000.0),
            (7000.0, 16000.0),
            (30.0, 16000.0),
            (150.0, 2000.0),
        ];
        let mut frame = self.frame;
        for k in 0..HITS {
            let fl = self
                .an
                .flux(&ft.logmag, &self.prev, ranges[k].0, ranges[k].1);
            let avg = self.flux_avg[k];
            let h = &mut frame.hits[k];
            h.since += dt;
            if fl > avg * 2.2 + 1e-3 && h.since > 0.08 {
                h.since = 0.0;
                h.strength = (fl / (avg * 5.0)).clamp(0.2, 1.0);
            }
            self.flux_avg[k] = avg + (fl - avg) * 0.05;
        }
        for (b, s) in ft.spectrum.iter().enumerate() {
            self.spectrum_peak[b] = (self.spectrum_peak[b] * (1.0 - dt / 8.0)).max(*s).max(1e-4);
            frame.spectrum[b] = (s / self.spectrum_peak[b]).clamp(0.0, 1.0);
        }
        if max > 0.0 {
            for (i, c) in ft.chroma.iter().enumerate() {
                frame.chroma[i] = c / max;
            }
        }
        frame.fast = self.fast;
        frame.smooth = self.smooth;
        frame.active = true;
        self.frame = frame;
        self.prev = ft.logmag;
        self.since_frames += 1;
    }

    /// The analysed state right now.
    pub fn frame(&self) -> MusicFrame {
        self.frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 22050.0;

    /// A 120 BPM beat: a low thump on every beat, a noisy snap on 2 and 4,
    /// and a 440 Hz tone underneath.
    pub fn beat_track(seconds: f32) -> Vec<f32> {
        let n = (seconds * RATE) as usize;
        let mut out = vec![0.0f32; n];
        let mut seed = 12345u32;
        for (i, s) in out.iter_mut().enumerate() {
            let t = i as f32 / RATE;
            let beat = t * 2.0;
            let bt = (beat.fract()) / 2.0; // seconds since the beat
            let kick = (std::f32::consts::TAU * 60.0 * bt).sin() * (-bt * 30.0).exp();
            let b = beat.floor() as i32;
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            let snare = if b % 2 == 1 {
                noise * (-bt * 40.0).exp() * 0.5
            } else {
                0.0
            };
            let tone = (std::f32::consts::TAU * 440.0 * t).sin() * 0.1;
            *s = kick * 0.8 + snare + tone;
        }
        out
    }

    #[test]
    fn fft_matches_a_sine() {
        let n = 64;
        let mut re: Vec<f32> = (0..n)
            .map(|i| (std::f32::consts::TAU * 5.0 * i as f32 / n as f32).cos())
            .collect();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        let mag: Vec<f32> = (0..n)
            .map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt())
            .collect();
        let peak = mag[..n / 2]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        assert_eq!(peak, 5);
        assert!((mag[5] - n as f32 / 2.0).abs() < 1e-3);
    }

    #[test]
    fn finds_kicks_snares_tempo_and_pitch() {
        let env = analyze(&beat_track(8.0), RATE);
        let kicks = &env.hits[HitKind::Kick as usize];
        assert!(
            kicks.len() >= 12 && kicks.len() <= 18,
            "{} kicks",
            kicks.len()
        );
        // Kicks sit on the beats (every 0.5 s).
        for (t, _) in kicks {
            let off = (t * 2.0 - (t * 2.0).round()).abs() / 2.0;
            assert!(off < 0.04, "kick at {t} is off the beat");
        }
        let snares = &env.hits[HitKind::Snare as usize];
        assert!(snares.len() >= 6, "{} snares", snares.len());
        let (bpm, _) = env.tempo.expect("tempo");
        assert!((bpm - 120.0).abs() < 3.0, "bpm {bpm}");
        // 440 Hz is an A (pitch class 9).
        let p = env.fast[Curve::Pitch as usize][300];
        assert!((p * 12.0 - 9.0).abs() < 0.01, "pitch {p}");
    }

    #[test]
    fn finds_song_sections() {
        // 40 bars at 120 BPM: quiet lows, loud highs from bar 12, lows again
        // from bar 28.
        let rate = 100.0;
        let bar = 2.0;
        let duration = 40.0 * bar;
        let frames = (duration * rate) as usize + 1;
        let mut env = AudioEnvelope {
            rate,
            duration,
            ..Default::default()
        };
        let part = |t: f32| {
            if t < 12.0 * bar || t >= 28.0 * bar {
                0
            } else {
                1
            }
        };
        env.spectrum = (0..frames)
            .map(|i| {
                let mut s = [0.0; SPECTRUM_BANDS];
                let t = i as f32 / rate;
                // A little movement within parts, so novelty isn't flat.
                let wobble = 0.05 * (t * 3.0).sin();
                if part(t) == 0 {
                    s[1] = 0.8 + wobble;
                    s[2] = 0.6;
                } else {
                    s[10] = 0.9 + wobble;
                    s[12] = 0.7;
                }
                s
            })
            .collect();
        for c in 0..CURVES {
            env.fast[c] = vec![0.0; frames];
            env.smooth[c] = (0..frames)
                .map(|i| if part(i as f32 / rate) == 0 { 0.2 } else { 0.9 })
                .collect();
        }
        let s = sections(&env, 0.5, 0.0, 4);
        assert_eq!(s, vec![12, 16, 12], "{s:?}");
        assert_eq!(s.iter().sum::<u32>(), 40);
    }

    #[test]
    fn stepped_analysis_matches_whole() {
        let track = beat_track(3.0);
        let whole = analyze(&track, RATE);
        let mut a = Analysis::new(track, RATE);
        let mut steps = 0;
        while !a.step(37) {
            steps += 1;
            assert!(a.progress() < 1.0);
        }
        assert!(steps > 5);
        assert_eq!(a.finish(), whole);
    }

    #[test]
    fn live_analyser_detects_hits() {
        let track = beat_track(3.0);
        let mut live = LiveAnalyzer::new(RATE);
        let mut hits = 0;
        let mut last = 1e6;
        for chunk in track.chunks(512) {
            live.push(chunk);
            let s = live.frame().hits[HitKind::Kick as usize].since;
            if s < last && s < 0.02 {
                hits += 1;
            }
            last = s;
        }
        assert!(hits >= 3, "{hits} live kicks");
    }
}
