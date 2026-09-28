//! Music playback locked to the loop clock.
//!
//! The song is decoded once into memory and played by a source that loops
//! the current region (the loop window, or the whole song) by itself,
//! sample-accurately: nothing is sought or reopened at the loop point, so
//! the music doesn't stutter there. It only jumps when the clock does (a
//! scrub), and it reports where it is so the picture can follow it.

use anyhow::{Context, Result};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// No seek requested.
const NO_SEEK: u64 = u64::MAX;

/// State shared between the app and the audio thread, in frames (one
/// sample per channel).
struct Shared {
    /// Frame being played.
    pos: AtomicU64,
    /// The looped region, `start..end`.
    start: AtomicU64,
    end: AtomicU64,
    /// Jump here at the next frame (`NO_SEEK`: none).
    seek: AtomicU64,
}

/// Plays `samples` round and round over the shared region.
struct LoopSource {
    samples: Arc<Vec<f32>>,
    channels: rodio::ChannelCount,
    rate: rodio::SampleRate,
    shared: Arc<Shared>,
    frame: u64,
    channel: u16,
}

impl Iterator for LoopSource {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<rodio::Sample> {
        let ch = self.channels.get() as u64;
        let frames = self.samples.len() as u64 / ch;
        if frames == 0 {
            return None;
        }
        if self.channel == 0 {
            // Frame boundary: take a pending jump, loop at the region's end.
            let seek = self.shared.seek.swap(NO_SEEK, Ordering::Relaxed);
            if seek != NO_SEEK {
                self.frame = seek;
            }
            let start = self.shared.start.load(Ordering::Relaxed).min(frames - 1);
            let end = self
                .shared
                .end
                .load(Ordering::Relaxed)
                .clamp(start + 1, frames);
            if self.frame >= end || self.frame < start {
                self.frame = start + self.frame.saturating_sub(start) % (end - start);
            }
            self.shared.pos.store(self.frame, Ordering::Relaxed);
        }
        let s = self.samples[(self.frame * ch + self.channel as u64) as usize];
        self.channel += 1;
        if self.channel as u64 == ch {
            self.channel = 0;
            self.frame += 1;
        }
        Some(s)
    }
}

impl rodio::Source for LoopSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.channels
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub struct AudioPlayer {
    _sink: rodio::MixerDeviceSink,
    player: rodio::Player,
    shared: Arc<Shared>,
    rate: f64,
    frames: u64,
    pub volume: f32,
}

impl AudioPlayer {
    pub fn new(path: &str) -> Result<AudioPlayer> {
        let file = std::fs::File::open(path).with_context(|| format!("opening {path}"))?;
        let dec = rodio::Decoder::try_from(file).context("decoding audio")?;
        let channels = rodio::Source::channels(&dec);
        let rate = rodio::Source::sample_rate(&dec);
        let samples: Vec<f32> = dec.collect();
        let frames = samples.len() as u64 / channels.get() as u64;
        let shared = Arc::new(Shared {
            pos: AtomicU64::new(0),
            start: AtomicU64::new(0),
            end: AtomicU64::new(frames),
            seek: AtomicU64::new(NO_SEEK),
        });
        let sink =
            rodio::DeviceSinkBuilder::open_default_sink().context("no audio output device")?;
        let player = rodio::Player::connect_new(sink.mixer());
        player.pause();
        player.append(LoopSource {
            samples: Arc::new(samples),
            channels,
            rate,
            shared: shared.clone(),
            frame: 0,
            channel: 0,
        });
        Ok(AudioPlayer {
            _sink: sink,
            player,
            shared,
            rate: rate.get() as f64,
            frames,
            volume: 0.8,
        })
    }

    /// Length of the song in seconds.
    pub fn duration(&self) -> f64 {
        self.frames as f64 / self.rate
    }

    /// Play the region `start..end` (song seconds) round and round. It
    /// starts at `seconds`, and jumps there only when `seek` (the clock was
    /// moved by hand): never at the loop point. Returns where the music is
    /// (song seconds) while it plays. Call every frame.
    pub fn sync(
        &mut self,
        playing: bool,
        seconds: f64,
        start: f64,
        end: f64,
        seek: bool,
    ) -> Option<f64> {
        self.player.set_volume(self.volume);
        let frame = |s: f64| ((s.max(0.0) * self.rate) as u64).min(self.frames);
        let (a, b) = (frame(start), frame(end).max(frame(start) + 1));
        self.shared.start.store(a, Ordering::Relaxed);
        self.shared.end.store(b, Ordering::Relaxed);
        if !playing {
            if !self.player.is_paused() {
                self.player.pause();
            }
            return None;
        }
        let target = frame(seconds).clamp(a, b - 1);
        if self.player.is_paused() || seek {
            self.shared.seek.store(target, Ordering::Relaxed);
            self.player.play();
            return None;
        }
        // Until the audio thread has taken a jump, it isn't there yet.
        if self.shared.seek.load(Ordering::Relaxed) != NO_SEEK {
            return None;
        }
        Some(self.shared.pos.load(Ordering::Relaxed) as f64 / self.rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(frames: u64, start: u64, end: u64) -> (LoopSource, Arc<Shared>) {
        let shared = Arc::new(Shared {
            pos: AtomicU64::new(0),
            start: AtomicU64::new(start),
            end: AtomicU64::new(end),
            seek: AtomicU64::new(NO_SEEK),
        });
        // Stereo: left = frame index, right = its negative.
        let samples = (0..frames).flat_map(|f| [f as f32, -(f as f32)]).collect();
        let src = LoopSource {
            samples: Arc::new(samples),
            channels: rodio::ChannelCount::new(2).unwrap(),
            rate: rodio::SampleRate::new(100).unwrap(),
            shared: shared.clone(),
            frame: start,
            channel: 0,
        };
        (src, shared)
    }

    #[test]
    fn loops_the_region_without_a_gap() {
        let (src, _) = source(20, 5, 9);
        let left: Vec<f32> = src.step_by(2).take(10).collect();
        assert_eq!(left, [5.0, 6.0, 7.0, 8.0, 5.0, 6.0, 7.0, 8.0, 5.0, 6.0]);
    }

    #[test]
    fn seeks_on_a_frame_boundary_and_reports_its_place() {
        let (mut src, shared) = source(20, 0, 20);
        src.next();
        shared.seek.store(12, Ordering::Relaxed);
        // The right channel of frame 0 still comes first.
        assert_eq!(src.next(), Some(-0.0));
        assert_eq!(src.next(), Some(12.0));
        assert_eq!(shared.pos.load(Ordering::Relaxed), 12);
        // A region that moved puts the frame back inside it.
        shared.start.store(2, Ordering::Relaxed);
        shared.end.store(6, Ordering::Relaxed);
        src.next();
        assert!((2.0..6.0).contains(&src.next().unwrap()));
    }
}
