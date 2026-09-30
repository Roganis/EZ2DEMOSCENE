//! Post-processing effects.

use super::*;

/// Effects applied to the finished picture, in a fixed order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
#[derive(Default)]
pub struct PostStack {
    pub bloom: Bloom,
    pub kaleido: Kaleido,
    pub mirror: MirrorSplit,
    pub chroma: Chroma,
    pub pixelate: Pixelate,
    pub palette: PaletteFx,
    pub crt: Crt,
    pub grade: Grade,
    /// Light shafts streaming from the sun, plus lens flare.
    #[serde(skip_serializing_if = "is_default")]
    pub rays: GodRays,
    /// Shimmering heat distortion.
    #[serde(skip_serializing_if = "is_default")]
    pub haze: HeatHaze,
    /// Blur what is nearer or further than the focus.
    #[serde(skip_serializing_if = "is_default")]
    pub dof: DepthOfField,
    /// Trails: the previous frames linger, zooming, turning and changing
    /// colour.
    #[serde(skip_serializing_if = "is_default")]
    pub feedback: Feedback,
    /// The lines of the picture wobble, as in retro RPG battles.
    #[serde(skip_serializing_if = "is_default")]
    pub wobble: LineWobble,
    /// Worn video tape: jittering lines, noise bands, colour bleed.
    #[serde(skip_serializing_if = "is_default")]
    pub vhs: Vhs,
    /// The picture drawn with text characters.
    #[serde(skip_serializing_if = "is_default")]
    pub ascii: Ascii,
    /// A fisheye lens (or its opposite).
    #[serde(skip_serializing_if = "is_default")]
    pub lens: Lens,
}

/// Lens distortion of the whole picture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Lens {
    pub enabled: bool,
    /// Above 0: fisheye, the middle bulges out and the corners stay put.
    /// Below 0: the middle shrinks away and the edges stretch.
    pub amount: Param,
}

impl Default for Lens {
    fn default() -> Self {
        Lens {
            enabled: false,
            amount: Param::new(0.5),
        }
    }
}

/// Line wobble of the whole picture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LineWobble {
    pub enabled: bool,
    pub mode: LineWarp,
    /// How far lines move (fraction of the picture's height).
    pub amount: Param,
    /// Waves from the top of the picture to the bottom.
    pub waves: Param,
    /// Times the waves roll by per loop.
    pub speed: i32,
    /// Lines from top to bottom for the interlacing (0 = every pixel row).
    pub lines: u32,
}

impl Default for LineWobble {
    fn default() -> Self {
        LineWobble {
            enabled: false,
            mode: LineWarp::Wave,
            amount: Param::new(0.02),
            waves: Param::new(4.0),
            speed: 2,
            lines: 240,
        }
    }
}

/// Worn video tape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Vhs {
    pub enabled: bool,
    /// Overall strength.
    pub amount: Param,
    /// Colour bleeding sideways.
    pub bleed: Param,
    /// Noisy bands rolling through the picture.
    pub bands: Param,
}

impl Default for Vhs {
    fn default() -> Self {
        Vhs {
            enabled: false,
            amount: Param::new(0.5),
            bleed: Param::new(0.5),
            bands: Param::new(0.5),
        }
    }
}

labeled_enum! {
    /// Colours of the ASCII characters.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum AsciiColor {
        /// The picture's colours.
        #[default]
        Picture => "Picture colours",
        /// Green terminal.
        Green => "Green terminal",
        /// Amber terminal.
        Amber => "Amber terminal",
        /// White on black.
        White => "White",
    }
}

impl AsciiColor {
    pub fn rgb(self) -> Option<[f32; 3]> {
        match self {
            AsciiColor::Picture => None,
            AsciiColor::Green => Some([0.25, 1.0, 0.35]),
            AsciiColor::Amber => Some([1.0, 0.7, 0.15]),
            AsciiColor::White => Some([1.0, 1.0, 1.0]),
        }
    }
}

/// The picture as text characters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Ascii {
    pub enabled: bool,
    /// Character rows from top to bottom.
    pub rows: Param,
    pub color: AsciiColor,
    /// How much of the picture shows behind the characters (0..1).
    pub backdrop: Param,
}

impl Default for Ascii {
    fn default() -> Self {
        Ascii {
            enabled: false,
            rows: Param::new(60.0),
            color: AsciiColor::Picture,
            backdrop: Param::new(0.0),
        }
    }
}

/// Video feedback trails.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Feedback {
    pub enabled: bool,
    /// How long trails last (0 = none, 1 = very long).
    pub length: Param,
    /// Zoom per second (1 = none, above 1 trails fly outwards).
    pub zoom: f32,
    /// Turn per second, in degrees.
    pub turn: f32,
    /// Hue change per second, in turns.
    pub hue: f32,
}

impl Default for Feedback {
    fn default() -> Self {
        Feedback {
            enabled: false,
            length: Param::new(0.6),
            zoom: 1.2,
            turn: 0.0,
            hue: 0.0,
        }
    }
}

/// Camera-lens blur away from a focus distance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DepthOfField {
    pub enabled: bool,
    /// Focus on the point the camera looks at (else on `focus`).
    pub auto_focus: bool,
    /// Focus distance in world units (animatable).
    pub focus: Param,
    /// How strong the blur gets (0..1, animatable).
    pub blur: Param,
}

impl Default for DepthOfField {
    fn default() -> Self {
        DepthOfField {
            enabled: false,
            auto_focus: true,
            focus: Param::new(10.0),
            blur: Param::new(0.5),
        }
    }
}

labeled_enum! {
    /// Where heat haze shimmers.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum HazeRegion {
        /// Above bright, hot things (lava, fire, the sun).
        #[default]
        HotSpots => "Above hot spots",
        /// Strongest at the bottom of the picture (hot ground).
        Ground => "Near the ground",
        /// Everywhere.
        Everywhere => "Everywhere",
    }
}

/// Heat shimmer: the picture wobbles as if seen through rising hot air.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct HeatHaze {
    pub enabled: bool,
    pub region: HazeRegion,
    pub amount: Param,
    /// Size of the ripples.
    pub scale: f32,
    /// Times the shimmer rises through the picture per loop.
    pub speed: i32,
}

impl Default for HeatHaze {
    fn default() -> Self {
        HeatHaze {
            enabled: false,
            region: HazeRegion::HotSpots,
            amount: Param::new(1.0),
            scale: 1.0,
            speed: 4,
        }
    }
}

/// Glow around bright parts of the picture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Bloom {
    pub enabled: bool,
    pub intensity: Param,
    pub threshold: Param,
    /// Spread (0..1).
    pub radius: Param,
}

impl Default for Bloom {
    fn default() -> Self {
        Bloom {
            enabled: true,
            intensity: Param::new(0.8),
            threshold: Param::new(0.8),
            radius: Param::new(0.7),
        }
    }
}

labeled_enum! {
    /// Where light rays come from.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum RaySource {
        /// From the sun (the light direction).
        #[default]
        Sun => "Sun",
        /// From the middle of the picture (light at the end of a tunnel).
        Centre => "Picture centre",
    }
}

/// Screen-space light shafts ("god rays"): bright parts of the picture near
/// the light are smeared towards it, so anything dark in front casts
/// streaks of shadow through the haze.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct GodRays {
    pub enabled: bool,
    pub source: RaySource,
    pub intensity: Param,
    /// Length of the shafts (0..1).
    pub length: Param,
    /// Brightness above which the picture casts rays.
    pub threshold: Param,
    pub tint: Rgb,
    /// Lens flare ghosts and halo (0 = none).
    pub flare: Param,
}

impl Default for GodRays {
    fn default() -> Self {
        GodRays {
            enabled: false,
            source: RaySource::Sun,
            intensity: Param::new(1.0),
            length: Param::new(0.7),
            threshold: Param::new(0.5),
            tint: [1.0, 0.95, 0.85],
            flare: Param::new(0.0),
        }
    }
}

/// A kaleidoscope: the picture mirrored into segments around a centre.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Kaleido {
    pub enabled: bool,
    pub segments: u32,
    /// Base rotation in degrees.
    pub angle: Param,
    /// Whole rotations per loop.
    pub turns: i32,
    pub zoom: Param,
    pub center: [f32; 2],
}

impl Default for Kaleido {
    fn default() -> Self {
        Kaleido {
            enabled: false,
            segments: 6,
            angle: Param::new(0.0),
            turns: 0,
            zoom: Param::new(1.0),
            center: [0.5, 0.5],
        }
    }
}

labeled_enum! {
    /// How the picture is mirrored.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum MirrorSplitMode {
        #[default]
        LeftToRight => "Left → right",
        TopToBottom => "Top → bottom",
        Quad => "Quad",
    }
}

/// The picture mirrored onto itself (left to right, top to bottom, or into four).
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct MirrorSplit {
    pub enabled: bool,
    pub mode: MirrorSplitMode,
}

/// Chromatic aberration: colour fringes toward the edges.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Chroma {
    pub enabled: bool,
    pub amount: Param,
}

impl Default for Chroma {
    fn default() -> Self {
        Chroma {
            enabled: false,
            amount: Param::new(0.004),
        }
    }
}

/// Big pixels over the whole picture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Pixelate {
    pub enabled: bool,
    /// Size of a "fat pixel" in output pixels.
    pub size: Param,
}

impl Default for Pixelate {
    fn default() -> Self {
        Pixelate {
            enabled: false,
            size: Param::new(4.0),
        }
    }
}

/// The picture reduced to a retro palette, optionally dithered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct PaletteFx {
    pub enabled: bool,
    pub palette: PaletteId,
    /// Ordered dithering strength (0..1).
    pub dither: Param,
}

impl Default for PaletteFx {
    fn default() -> Self {
        PaletteFx {
            enabled: false,
            palette: PaletteId::Ega,
            dither: Param::new(0.6),
        }
    }
}

/// A CRT screen look: scanlines, curvature and noise.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Crt {
    pub enabled: bool,
    pub scanlines: Param,
    pub curvature: Param,
    /// Horizontal wobble / VHS noise.
    pub noise: Param,
}

impl Default for Crt {
    fn default() -> Self {
        Crt {
            enabled: false,
            scanlines: Param::new(0.5),
            curvature: Param::new(0.15),
            noise: Param::new(0.1),
        }
    }
}

/// Colour grading of the final picture: exposure, contrast, saturation, vignette and film grain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Grade {
    pub exposure: Param,
    pub contrast: Param,
    pub saturation: Param,
    pub vignette: Param,
    pub grain: Param,
    /// White flash on every beat (0 = none).
    pub beat_flash: Param,
}

impl Default for Grade {
    fn default() -> Self {
        Grade {
            exposure: Param::new(1.0),
            contrast: Param::new(1.05),
            saturation: Param::new(1.1),
            vignette: Param::new(0.35),
            grain: Param::new(0.03),
            beat_flash: Param::new(0.0),
        }
    }
}
