//! Music playback locked to the loop clock.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct AudioPlayer {
    _sink: rodio::MixerDeviceSink,
    player: rodio::Player,
    path: PathBuf,
    pub volume: f32,
}

impl AudioPlayer {
    pub fn new(path: &Path) -> Result<AudioPlayer> {
        let sink =
            rodio::DeviceSinkBuilder::open_default_sink().context("no audio output device")?;
        let player = rodio::Player::connect_new(sink.mixer());
        player.pause();
        let mut a = AudioPlayer {
            _sink: sink,
            player,
            path: path.to_path_buf(),
            volume: 0.8,
        };
        a.refill()?;
        Ok(a)
    }

    fn refill(&mut self) -> Result<()> {
        let file = std::fs::File::open(&self.path)
            .with_context(|| format!("opening {}", self.path.display()))?;
        let dec = rodio::Decoder::try_from(file).context("decoding audio")?;
        self.player.clear();
        self.player.append(dec);
        self.player.set_volume(self.volume);
        Ok(())
    }

    /// Keep playback at `seconds` (within the loop). Call every frame.
    pub fn sync(&mut self, playing: bool, seconds: f32) {
        self.player.set_volume(self.volume);
        if !playing {
            if !self.player.is_paused() {
                self.player.pause();
            }
            return;
        }
        if self.player.empty() {
            let _ = self.refill();
        }
        let pos = self.player.get_pos().as_secs_f32();
        // Re-seek when drifting (after a scrub or at the loop point).
        let drift = (pos - seconds).abs();
        if drift > 0.08 || self.player.is_paused() {
            let _ = self
                .player
                .try_seek(Duration::from_secs_f32(seconds.max(0.0)));
        }
        if self.player.is_paused() {
            self.player.play();
        }
    }
}
