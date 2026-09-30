//! Logo layers.

use super::*;

labeled_enum! {
    /// What a logo is made of.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum LogoSource {
        /// A line (or a few lines) of text in a font.
        #[default]
        Text => "Text",
        /// An image; its shape comes from `LogoLayer::mask`.
        Image => "Image",
    }
}

labeled_enum! {
    /// Which parts of a logo image are the logo.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum LogoMask {
        /// The image's transparency.
        #[default]
        Alpha => "Transparency",
        /// Bright parts (a logo on black).
        Bright => "Bright parts",
        /// Dark parts (a logo on white).
        Dark => "Dark parts",
    }
}

labeled_enum! {
    /// Where a logo's colour comes from.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum LogoColors {
        /// The image's own colours times the tint (text: the tint).
        Image => "Image colours",
        /// Top to bottom gradient.
        #[default]
        Gradient => "Gradient",
    }
}

labeled_enum! {
    /// The shape of a logo's bevel, from the edge inward.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum LogoBevel {
        /// Flat: no lighting.
        #[default]
        Off => "Flat",
        /// A rounded edge (a quarter circle).
        Round => "Round",
        /// A straight slope, like cut metal.
        Chiselled => "Chiselled",
        /// Terraces.
        Stepped => "Stepped",
        /// The whole logo bulges like a cushion.
        Pillow => "Pillow",
    }
}

impl LogoBevel {
    pub fn index(self) -> u32 {
        LogoBevel::ALL.iter().position(|b| *b == self).unwrap_or(0) as u32
    }
}

labeled_enum! {
    /// How a logo appears as its reveal goes from 0 to 1.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum LogoReveal {
        /// The letters grow outward from their middle lines.
        #[default]
        Grow => "Grow from the middle",
        /// Outlines first, then they fill inward.
        Edges => "Edges first",
        /// A straight wipe across, in a direction.
        Wipe => "Wipe",
        /// A circle opening from the middle.
        Radial => "Circle",
    }
}

impl LogoReveal {
    pub fn index(self) -> u32 {
        LogoReveal::ALL.iter().position(|r| *r == self).unwrap_or(0) as u32
    }
}

labeled_enum! {
    /// The point of the logo that sits at its position (and that it turns
    /// around).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum LogoAnchor {
        TopLeft => "Top left",
        Top => "Top",
        TopRight => "Top right",
        Left => "Left",
        #[default]
        Centre => "Centre",
        Right => "Right",
        BottomLeft => "Bottom left",
        Bottom => "Bottom",
        BottomRight => "Bottom right",
    }
}

impl LogoAnchor {
    /// The point in the same place on the opposite side (Top ↔ Bottom,
    /// Left ↔ Right).
    pub fn opposite(self) -> LogoAnchor {
        let [x, y] = self.point();
        LogoAnchor::at(1.0 - x, 1.0 - y)
    }

    /// The anchor at (0, 0.5 or 1 from the left, from the bottom).
    pub fn at(x: f32, y: f32) -> LogoAnchor {
        let col = (x * 2.0).round().clamp(0.0, 2.0) as usize;
        let row = 2 - (y * 2.0).round().clamp(0.0, 2.0) as usize;
        LogoAnchor::ALL[row * 3 + col]
    }

    /// The anchor inside the logo (0..1 from the left, 0..1 from the bottom).
    pub fn point(self) -> [f32; 2] {
        let i = LogoAnchor::ALL.iter().position(|a| *a == self).unwrap_or(4);
        [(i % 3) as f32 * 0.5, 1.0 - (i / 3) as f32 * 0.5]
    }
}

/// A logo or title drawn flat on the screen over the scene (before the
/// post effects, so bloom, rays and trails apply). Its shape is a signed
/// distance field baked from the text or image, which gives outlines,
/// glows, shadows and bevels at any size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LogoLayer {
    pub source: LogoSource,
    pub text: String,
    pub font: TextFont,
    /// A TTF/OTF file used instead of `font`.
    pub font_file: Option<String>,
    /// A texture (built-in or added to the project).
    pub image: Option<String>,
    pub mask: LogoMask,
    /// Another logo layer (by name) this one is placed against; empty:
    /// the screen.
    pub attach_to: String,
    /// The point of the screen (or of that logo) the position is measured
    /// from. Bottom left (the default) makes the position absolute.
    pub attach_point: LogoAnchor,
    /// Position of the anchor from the attach point, as fractions of the
    /// screen's width and height (to the right, up).
    pub x: Param,
    pub y: Param,
    pub anchor: LogoAnchor,
    /// Height as a fraction of the screen height; the width follows.
    pub size: Param,
    /// Degrees, anticlockwise, around the anchor.
    pub rotation: Param,
    pub opacity: Param,
    pub colors: LogoColors,
    /// Multiplies the image colours.
    pub tint: Rgb,
    pub color_top: Rgb,
    pub color_bottom: Rgb,
    /// Brightness; above 1 it glows.
    pub glow: Param,
    /// Outline width (0 = none, 1 = thick).
    pub outline: Param,
    pub outline_color: Rgb,
    /// Drop shadow strength.
    pub shadow: Param,
    /// Chrome: a shiny bevel reflecting the sky.
    pub chrome: Param,
    /// Relief lit by a light on the screen.
    pub bevel: LogoBevel,
    /// How far in from the edge the bevel reaches (1 = a fifth of the
    /// logo's shorter side; Pillow always spans the whole logo).
    pub bevel_width: Param,
    /// How steep the bevel is.
    pub bevel_depth: Param,
    /// Terraces of the Stepped bevel.
    pub steps: u32,
    /// Where the light comes from on the screen, in degrees (0 = from the
    /// right, 90 = from above).
    pub light_angle: Param,
    /// How high the light is above the logo, in degrees.
    pub light_height: Param,
    pub light_color: Rgb,
    /// How much the light shades the logo (0 = flat colour).
    pub lighting: Param,
    /// Highlights.
    pub shine: Param,
    /// Small, sharp highlights (1) or broad ones (0).
    pub gloss: f32,
    /// A material from a picture of a lit sphere (built-in or yours),
    /// looked up by the bevel's slope.
    pub matcap: Option<String>,
    pub matcap_amount: Param,
    /// A bright band sweeping across the logo.
    pub glint: Param,
    /// Sweeps per loop.
    pub glint_cycles: i32,
    /// Band width (fraction of the logo's height).
    pub glint_width: f32,
    /// Direction the band travels, in degrees (0 = to the right).
    pub glint_angle: f32,
    pub glint_color: Rgb,
    // Distance-field effects. Distances are in logo heights.
    /// Rings rippling out from the edges (brightness; 0 = none).
    pub contours: Param,
    /// Distance between rings.
    pub contour_spacing: f32,
    /// Rings passing per loop (negative = inward).
    pub contour_cycles: i32,
    /// How far out they fade.
    pub contour_reach: f32,
    /// Line thickness (fraction of the spacing).
    pub contour_width: f32,
    pub contour_color: Rgb,
    /// Rings inside the letters too.
    pub contour_inside: bool,
    /// Solid outlines stacked around the logo (0 = none).
    pub stack: u32,
    pub stack_width: Param,
    /// Space between them.
    pub stack_gap: f32,
    /// Colours of the first and last outline.
    pub stack_color_a: Rgb,
    pub stack_color_b: Rgb,
    /// Fake 3D depth behind the logo, its length (0 = none).
    pub extrude: Param,
    /// Direction it goes, in degrees (0 = to the right, -90 = down).
    pub extrude_angle: f32,
    pub extrude_color: Rgb,
    /// Burning away: 0 = whole, 1 = gone.
    pub dissolve: Param,
    /// Size of the burnt patches (patches per logo height).
    pub dissolve_scale: f32,
    /// 0: patches anywhere; 1: eaten from the edges inward.
    pub dissolve_edges: f32,
    /// Width of the glowing burn front.
    pub burn_width: f32,
    pub burn_color: Rgb,
    pub dissolve_seed: u32,
    /// How the logo appears as `reveal_amount` goes from 0 to 1.
    pub reveal: LogoReveal,
    /// 1 = fully shown.
    pub reveal_amount: Param,
    /// Wipe direction, in degrees (0 = left to right).
    pub reveal_angle: f32,
    /// Softness of a wipe's or circle's edge.
    pub reveal_soft: f32,
    /// Blend towards a second logo: 0 = this one, 1 = the other.
    pub morph: Param,
    pub morph_source: LogoSource,
    pub morph_text: String,
    pub morph_image: Option<String>,
    /// Which parts of the morph image are the logo (`None`: as for this
    /// logo's own image, which is how projects saved before worked).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub morph_mask: Option<LogoMask>,
    // Rasters and distortion.
    /// Copper bars scrolling through the letters (0 = none, 1 = all bars).
    pub copper: Param,
    /// Bars per logo height.
    pub copper_bars: f32,
    /// Pairs of bars scrolling past per loop (negative = upward).
    pub copper_cycles: i32,
    /// The two alternating bar colours.
    pub copper_a: Rgb,
    pub copper_b: Rgb,
    /// Rows swaying sideways (logo heights).
    pub wobble_x: Param,
    /// Columns bobbing up and down (logo heights).
    pub wobble_y: Param,
    /// Waves per logo height.
    pub wobble_waves: f32,
    /// Times the waves roll past per loop.
    pub wobble_cycles: i32,
    /// Horizontal slices jumping sideways (largest jump, logo heights).
    pub glitch: Param,
    /// Slices per logo height.
    pub glitch_slices: f32,
    /// Share of the slices that jump.
    pub glitch_chance: f32,
    /// New jumps per loop (16 = every beat of a 16-beat loop).
    pub glitch_per_loop: u32,
    /// Colour split of a jumping slice (logo heights).
    pub glitch_split: f32,
    /// Red and blue pulled apart (logo heights).
    pub chroma: Param,
    /// Direction red moves, in degrees.
    pub chroma_angle: f32,
    // Retro looks.
    /// Blocks (logo heights; 0 = sharp). Animate it to pixelate in or out.
    pub pixelate: Param,
    /// Only colours of a retro palette.
    pub palette: Option<PaletteId>,
    /// Ordered (Bayer) dither between palette colours (0..1).
    pub dither: f32,
    /// Pick by brightness along the palette sorted dark to light, instead of
    /// the nearest colour.
    pub palette_by_brightness: bool,
    /// Palette colours rotating, whole turns per loop.
    pub palette_cycles: i32,
    /// Halftone dots (0 = none, 1 = only dots).
    pub halftone: Param,
    /// Dot spacing (logo heights).
    pub halftone_size: f32,
    /// Screen angle in degrees.
    pub halftone_angle: f32,
    /// Dark gaps between scanlines (0..1).
    pub scanlines: Param,
    /// Scanlines per logo height.
    pub scanline_count: f32,
    /// Red, green and blue phosphor stripes (0..1).
    pub crt_mask: f32,
    /// Extra brightness in the lines (above 0 they bloom).
    pub crt_glow: Param,
    /// Two turning line patterns beating against each other (0..1).
    pub moire: Param,
    /// Lines per logo height.
    pub moire_lines: f32,
    /// Turns per loop (the two patterns turn opposite ways).
    pub moire_cycles: i32,
    // The logo meets the scene.
    /// Glass letters: the scene behind them, bent (0 = none, 1 = all glass).
    pub glass: Param,
    /// How far the glass bends the scene (logo heights).
    pub refraction: f32,
    /// Colours bent by different amounts (0..1 of the bend).
    pub dispersion: f32,
    pub glass_tint: Rgb,
    /// Light rays streaming out from the logo (0 = none).
    pub rays: Param,
    /// Length of the rays (0..1).
    pub rays_length: f32,
    /// Only light brighter than this streams.
    pub rays_threshold: f32,
    /// Rays of the light behind the logo, with the logo's shadow cut out of
    /// them, instead of the logo's own light.
    pub rays_shadow: bool,
    pub rays_tint: Rgb,
    /// Fading copies of the logo where it was a moment ago (0 = none).
    pub echoes: u32,
    /// Time between copies (fraction of the loop).
    pub echo_spacing: f32,
    /// Each copy's opacity relative to the one after it.
    pub echo_fade: f32,
}

impl Default for LogoLayer {
    fn default() -> Self {
        LogoLayer {
            source: LogoSource::Text,
            text: "EZ2DEMOSCENE".into(),
            font: TextFont::Mono,
            font_file: None,
            image: None,
            mask: LogoMask::Alpha,
            attach_to: String::new(),
            attach_point: LogoAnchor::BottomLeft,
            x: Param::new(0.5),
            y: Param::new(0.5),
            anchor: LogoAnchor::Centre,
            size: Param::new(0.2),
            rotation: Param::new(0.0),
            opacity: Param::new(1.0),
            colors: LogoColors::Gradient,
            tint: [1.0, 1.0, 1.0],
            color_top: hex(0xffffff),
            color_bottom: hex(0x2bd6ff),
            glow: Param::new(1.0),
            outline: Param::new(0.0),
            outline_color: hex(0x000000),
            shadow: Param::new(0.0),
            chrome: Param::new(0.0),
            bevel: LogoBevel::Off,
            bevel_width: Param::new(0.4),
            bevel_depth: Param::new(1.0),
            steps: 3,
            light_angle: Param::new(120.0),
            light_height: Param::new(40.0),
            light_color: [1.0, 1.0, 1.0],
            lighting: Param::new(1.0),
            shine: Param::new(0.5),
            gloss: 0.6,
            matcap: None,
            matcap_amount: Param::new(1.0),
            glint: Param::new(0.0),
            glint_cycles: 1,
            glint_width: 0.12,
            glint_angle: 20.0,
            glint_color: [1.0, 1.0, 1.0],
            contours: Param::new(0.0),
            contour_spacing: 0.08,
            contour_cycles: 2,
            contour_reach: 0.5,
            contour_width: 0.15,
            contour_color: hex(0x40e0ff),
            contour_inside: false,
            stack: 0,
            stack_width: Param::new(0.03),
            stack_gap: 0.0,
            stack_color_a: hex(0xff2bd6),
            stack_color_b: hex(0x2040ff),
            extrude: Param::new(0.0),
            extrude_angle: -60.0,
            extrude_color: hex(0x6020a0),
            dissolve: Param::new(0.0),
            dissolve_scale: 6.0,
            dissolve_edges: 0.3,
            burn_width: 0.08,
            burn_color: hex(0xff7020),
            dissolve_seed: 1,
            reveal: LogoReveal::Grow,
            reveal_amount: Param::new(1.0),
            reveal_angle: 0.0,
            reveal_soft: 0.1,
            morph: Param::new(0.0),
            morph_source: LogoSource::Text,
            morph_text: "LOOP".into(),
            morph_image: None,
            morph_mask: None,
            copper: Param::new(0.0),
            copper_bars: 3.0,
            copper_cycles: 1,
            copper_a: hex(0xff3040),
            copper_b: hex(0x3060ff),
            wobble_x: Param::new(0.0),
            wobble_y: Param::new(0.0),
            wobble_waves: 1.5,
            wobble_cycles: 1,
            glitch: Param::new(0.0),
            glitch_slices: 12.0,
            glitch_chance: 0.3,
            glitch_per_loop: 16,
            glitch_split: 0.02,
            chroma: Param::new(0.0),
            chroma_angle: 0.0,
            pixelate: Param::new(0.0),
            palette: None,
            dither: 0.5,
            palette_by_brightness: false,
            palette_cycles: 0,
            halftone: Param::new(0.0),
            halftone_size: 0.04,
            halftone_angle: 45.0,
            scanlines: Param::new(0.0),
            scanline_count: 40.0,
            crt_mask: 0.0,
            crt_glow: Param::new(0.0),
            moire: Param::new(0.0),
            moire_lines: 30.0,
            moire_cycles: 1,
            glass: Param::new(0.0),
            refraction: 0.08,
            dispersion: 0.2,
            glass_tint: [1.0, 1.0, 1.0],
            rays: Param::new(0.0),
            rays_length: 0.5,
            rays_threshold: 0.0,
            rays_shadow: false,
            rays_tint: [1.0, 1.0, 1.0],
            echoes: 0,
            echo_spacing: 0.02,
            echo_fade: 0.6,
        }
    }
}

#[cfg(test)]
mod logo_anchor_tests {
    use super::LogoAnchor;

    #[test]
    fn anchors_round_trip_and_mirror() {
        for a in LogoAnchor::ALL {
            let [x, y] = a.point();
            assert_eq!(LogoAnchor::at(x, y), a);
            assert_eq!(a.opposite().opposite(), a);
        }
        assert_eq!(LogoAnchor::Bottom.opposite(), LogoAnchor::Top);
        assert_eq!(LogoAnchor::TopLeft.opposite(), LogoAnchor::BottomRight);
        assert_eq!(LogoAnchor::Centre.opposite(), LogoAnchor::Centre);
    }
}
