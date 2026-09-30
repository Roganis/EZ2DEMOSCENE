//! User textures.

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserTexture {
    pub name: String,
    pub path: String,
    /// Optional "retro-ize" processing applied on load.
    pub retro: Option<RetroProcess>,
    /// Tile it flipped: every other copy is a mirror image, so opposite
    /// edges always meet and a picture that doesn't tile shows no seams.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mirror: bool,
    /// An animation (from a GIF or a video): the picture holds its frames
    /// in a grid. Image layers play it like a sprite sheet; everywhere
    /// else it plays by itself, looped a whole number of times per loop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<FrameSheet>,
}

impl Default for UserTexture {
    fn default() -> Self {
        UserTexture {
            name: "texture".into(),
            path: String::new(),
            retro: None,
            mirror: false,
            clip: None,
        }
    }
}

/// The frames of an animated picture, laid out in a grid read left to
/// right, top to bottom.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FrameSheet {
    pub columns: u32,
    pub rows: u32,
    pub frames: u32,
    /// Length of one pass through the frames at its own speed.
    pub seconds: f32,
}

impl FrameSheet {
    /// Whole passes per loop that keep it nearest its own speed (at least
    /// one, so it always loops seamlessly).
    pub fn cycles_per_loop(&self, loop_seconds: f32) -> i32 {
        ((loop_seconds / self.seconds.max(1e-3)).round() as i32).max(1)
    }

    /// The frame shown at loop phase `phase` (0..1) playing `cycles`
    /// passes per loop.
    pub fn frame_at(&self, phase: f32, cycles: i32) -> u32 {
        let n = self.frames.max(1);
        let t = (phase * cycles as f32).rem_euclid(1.0);
        ((t * n as f32) as u32).min(n - 1)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RetroProcess {
    /// Downscale to this many pixels on the longest side (0 = keep).
    pub max_size: u32,
    pub palette: PaletteId,
    pub dither: f32,
}

impl Default for RetroProcess {
    fn default() -> Self {
        RetroProcess {
            max_size: 128,
            palette: PaletteId::Vga,
            dither: 0.5,
        }
    }
}
