//! Web music playback through Web Audio, locked to the loop clock (same
//! interface as the desktop player).
//!
//! The song is decoded once and played by a looping buffer source over the
//! current region (the loop window, or the whole song): the browser loops
//! it sample-accurately, so nothing is sought at the loop point (an
//! `<audio>` element stutters there). It only jumps when the clock does (a
//! scrub), and it reports where it is so the picture can follow it.

use anyhow::{anyhow, Result};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;

/// The node playing now, and how to tell where it is.
struct Playing {
    node: web_sys::AudioBufferSourceNode,
    /// Region it loops (song seconds).
    start: f64,
    end: f64,
    /// Context time at which it was at `from` (song seconds).
    t0: f64,
    from: f64,
}

pub struct AudioPlayer {
    ctx: web_sys::AudioContext,
    gain: web_sys::GainNode,
    /// Filled in once the browser has decoded the song.
    buffer: Rc<RefCell<Option<web_sys::AudioBuffer>>>,
    playing: Option<Playing>,
    pub volume: f32,
}

impl AudioPlayer {
    pub fn new(path: &str) -> Result<AudioPlayer> {
        let bytes = ez_core::store::read(path)?;
        let ctx = web_sys::AudioContext::new().map_err(|e| anyhow!("{e:?}"))?;
        let gain = ctx.create_gain().map_err(|e| anyhow!("{e:?}"))?;
        gain.connect_with_audio_node(&ctx.destination())
            .map_err(|e| anyhow!("{e:?}"))?;
        let buffer: Rc<RefCell<Option<web_sys::AudioBuffer>>> = Rc::default();
        let array = js_sys::Uint8Array::from(&*bytes);
        let promise = ctx
            .decode_audio_data(&array.buffer())
            .map_err(|e| anyhow!("{e:?}"))?;
        let slot = buffer.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(b) => *slot.borrow_mut() = b.dyn_into().ok(),
                Err(e) => log::warn!("could not decode the music: {e:?}"),
            }
        });
        Ok(AudioPlayer {
            ctx,
            gain,
            buffer,
            playing: None,
            volume: 0.8,
        })
    }

    /// Length of the song in seconds (0 until it is decoded).
    pub fn duration(&self) -> f64 {
        self.buffer.borrow().as_ref().map_or(0.0, |b| b.duration())
    }

    fn stop(&mut self) {
        if let Some(p) = self.playing.take() {
            #[allow(deprecated)]
            let _ = p.node.stop();
            let _ = p.node.disconnect();
        }
    }

    /// Where the playing node is (song seconds).
    fn position(&self, p: &Playing) -> f64 {
        let len = (p.end - p.start).max(1e-6);
        p.start + (p.from - p.start + self.ctx.current_time() - p.t0).rem_euclid(len)
    }

    fn start(&mut self, at: f64, start: f64, end: f64) {
        self.stop();
        let Some(buffer) = self.buffer.borrow().clone() else {
            return;
        };
        let Ok(node) = self.ctx.create_buffer_source() else {
            return;
        };
        node.set_buffer(Some(&buffer));
        node.set_loop(true);
        node.set_loop_start(start);
        node.set_loop_end(end);
        if node.connect_with_audio_node(&self.gain).is_err() {
            return;
        }
        let t0 = self.ctx.current_time();
        log::debug!("music: playing from {at:.3} s, looping {start:.3}–{end:.3} s");
        if node.start_with_when_and_grain_offset(0.0, at).is_ok() {
            self.playing = Some(Playing {
                node,
                start,
                end,
                t0,
                from: at,
            });
        }
    }

    /// Play the region `start..end` (song seconds) round and round. It
    /// starts at `seconds`, and jumps there only when `seek` (the clock was
    /// moved by hand) or the region changes: never at the loop point.
    /// Returns where the music is (song seconds) while it plays. Call every
    /// frame.
    pub fn sync(
        &mut self,
        playing: bool,
        seconds: f64,
        start: f64,
        end: f64,
        seek: bool,
    ) -> Option<f64> {
        self.gain.gain().set_value(self.volume.clamp(0.0, 1.0));
        if !playing {
            self.stop();
            return None;
        }
        // Browsers only start sound after a user gesture: until then, ask
        // again every frame (and let the picture keep its own clock).
        if self.ctx.state() != web_sys::AudioContextState::Running {
            let _ = self.ctx.resume();
            self.stop();
            return None;
        }
        let dur = self.duration();
        if dur <= 0.0 {
            return None;
        }
        let end = end.min(dur);
        let start = start.clamp(0.0, (end - 1e-3).max(0.0));
        let target = seconds.clamp(start, end);
        match &self.playing {
            Some(p) if !seek && (p.start - start).abs() < 1e-6 && (p.end - end).abs() < 1e-6 => {
                Some(self.position(p))
            }
            _ => {
                self.start(target, start, end);
                None
            }
        }
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop();
        let _ = self.ctx.close();
    }
}
