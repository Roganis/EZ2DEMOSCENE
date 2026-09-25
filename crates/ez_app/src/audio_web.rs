//! Web music playback through an HTML `<audio>` element, locked to the loop
//! clock (same interface as the desktop player).

use anyhow::{anyhow, Result};

pub struct AudioPlayer {
    el: web_sys::HtmlAudioElement,
    url: String,
    pub volume: f32,
}

impl AudioPlayer {
    pub fn new(path: &str) -> Result<AudioPlayer> {
        let bytes = ez_core::store::read(path)?;
        let array = js_sys::Uint8Array::from(&*bytes);
        let blob = web_sys::Blob::new_with_u8_array_sequence(&js_sys::Array::of1(&array))
            .map_err(|e| anyhow!("{e:?}"))?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| anyhow!("{e:?}"))?;
        let el = web_sys::HtmlAudioElement::new_with_src(&url).map_err(|e| anyhow!("{e:?}"))?;
        el.set_loop(true);
        Ok(AudioPlayer {
            el,
            url,
            volume: 0.8,
        })
    }

    /// Keep playback at `seconds` (within the loop). Call every frame.
    pub fn sync(&mut self, playing: bool, seconds: f32) {
        self.el.set_volume(self.volume.clamp(0.0, 1.0) as f64);
        if !playing {
            if !self.el.paused() {
                let _ = self.el.pause();
            }
            return;
        }
        let pos = self.el.current_time() as f32;
        if (pos - seconds).abs() > 0.12 || self.el.paused() {
            self.el.set_current_time(seconds.max(0.0) as f64);
        }
        if self.el.paused() {
            // Browsers only allow playback after a user gesture; if this is
            // refused the next frame simply tries again.
            let _ = self.el.play();
        }
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        let _ = self.el.pause();
        let _ = web_sys::Url::revoke_object_url(&self.url);
    }
}
