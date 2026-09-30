//! Text layers.

use super::*;

labeled_enum! {
    /// Built-in fonts.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
    pub enum TextFont {
        /// Chunky pixel letters, demoscene style.
        #[default]
        Pixel => "Pixel",
        /// Clean monospaced letters (Hack).
        Mono => "Mono",
        /// Light rounded letters (Ubuntu Light).
        Sans => "Sans",
    }
}

labeled_enum! {
    /// How text is shown and moves.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum TextStyle {
        /// Still text, centred.
        #[default]
        Static => "Still",
        /// Runs across a window a whole number of times per loop.
        Scroller => "Scroller",
        /// A scroller whose letters ride a sine wave.
        SineScroller => "Sine scroller",
        /// Letters appear one by one on the beat.
        Typewriter => "Typewriter",
        /// One line at a time, a new line every few beats.
        Greetings => "Greetings list",
    }
}

/// Glowing letters in the scene (a flat sign facing +z; place and turn it
/// like any layer).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TextLayer {
    /// One line per line for Greetings; Scrollers run it as one line.
    pub text: String,
    pub font: TextFont,
    /// A TTF/OTF file used instead of `font`.
    pub font_file: Option<String>,
    pub style: TextStyle,
    /// Letter height in world units.
    pub size: f32,
    /// Extra space between letters (fraction of the size).
    pub spacing: f32,
    /// Width of the scroller window in world units.
    pub width: f32,
    /// Times the scroller runs past per loop (negative = the other way).
    pub speed: i32,
    /// Sine scroller: wave height (fraction of the size).
    pub wave: Param,
    /// Letters per wave.
    pub wavelength: f32,
    /// Times the wave rolls per loop.
    pub wave_cycles: i32,
    /// Typewriter: letters per beat.
    pub letters_per_beat: u32,
    /// Greetings: beats per line.
    pub beats_per_line: u32,
    /// Colour at the top and bottom of the letters.
    pub color_top: Rgb,
    pub color_bottom: Rgb,
    pub glow: Param,
    /// Outline width (0 = none, 1 = thick).
    pub outline: f32,
    pub outline_color: Rgb,
    /// Drop shadow strength.
    pub shadow: f32,
    /// Chrome: shiny bevelled letters reflecting the sky.
    pub chrome: f32,
    /// Always turn the text towards the camera.
    pub face_camera: bool,
}

impl Default for TextLayer {
    fn default() -> Self {
        TextLayer {
            text: "EZ2DEMOSCENE".into(),
            font: TextFont::Pixel,
            font_file: None,
            style: TextStyle::Static,
            size: 1.0,
            spacing: 0.0,
            width: 12.0,
            speed: 1,
            wave: Param::new(0.4),
            wavelength: 8.0,
            wave_cycles: 2,
            letters_per_beat: 2,
            beats_per_line: 4,
            color_top: hex(0xffffff),
            color_bottom: hex(0xff2bd6),
            glow: Param::new(1.0),
            outline: 0.0,
            outline_color: hex(0x000000),
            shadow: 0.0,
            chrome: 0.0,
            face_camera: false,
        }
    }
}
