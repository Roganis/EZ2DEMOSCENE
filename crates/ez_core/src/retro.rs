//! "Retro 3D": the quirks of 5th-generation consoles (PS1, Saturn, N64) and
//! Quake-era software renderers, applied to the whole 3D scene.
//!
//! None of these settings animate on their own, so they keep every loop
//! seamless: snapping and warping are pure functions of where the
//! geometry is, and the low resolution only changes how finely it is drawn.

use crate::param::Param;
use serde::{Deserialize, Serialize};

/// The resolution the 3D scene is drawn at before being blown up with
/// chunky, unsmoothed pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RetroRes {
    /// The output's own resolution (no chunky pixels).
    #[default]
    Full,
    /// 256 × 224: SNES, Saturn and many PS1 games.
    R256x224,
    /// 320 × 240: most PS1 and N64 games.
    R320x240,
    /// 640 × 480: PS1 and N64 high-resolution modes.
    R640x480,
    /// Your own size (see [`Retro3d::custom`]).
    Custom,
}

impl RetroRes {
    pub const ALL: [RetroRes; 5] = [
        RetroRes::Full,
        RetroRes::R256x224,
        RetroRes::R320x240,
        RetroRes::R640x480,
        RetroRes::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RetroRes::Full => "Full (output size)",
            RetroRes::R256x224 => "256 × 224",
            RetroRes::R320x240 => "320 × 240",
            RetroRes::R640x480 => "640 × 480",
            RetroRes::Custom => "Custom",
        }
    }

    /// The size on a 4:3 screen, or `None` for full resolution.
    pub fn size(self, custom: [u32; 2]) -> Option<[u32; 2]> {
        match self {
            RetroRes::Full => None,
            RetroRes::R256x224 => Some([256, 224]),
            RetroRes::R320x240 => Some([320, 240]),
            RetroRes::R640x480 => Some([640, 480]),
            RetroRes::Custom => Some([custom[0].clamp(16, 4096), custom[1].clamp(16, 4096)]),
        }
    }
}

/// A console size (made for a 4:3 television) fitted to an output of
/// `out` pixels: the height is kept and the width follows the output's
/// shape, so the pixels keep the shape they had on the TV. 320 × 240 on a
/// 16:9 output is 427 × 240. Never larger than the output.
pub fn fit_to_output(size: [u32; 2], out: (u32, u32)) -> (u32, u32) {
    let aspect = out.0.max(1) as f32 / out.1.max(1) as f32;
    let w = (size[0] as f32 * aspect * 0.75).round().max(1.0) as u32;
    let h = size[1].max(1);
    // Keep the shape when the output is smaller than the console size.
    if h > out.1 {
        return (out.0.max(1), out.1.max(1));
    }
    (w.min(out.0.max(1)), h)
}

/// One-click looks: each turns on the matching bundle of settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetroStyle {
    /// Everything off.
    Modern,
    /// Wobbly vertices, warped textures, square texture pixels, 15-bit
    /// dithered colour, chunky pixels at 320 × 240.
    Ps1,
    /// Like the PlayStation but steadier warp, 320 × 224 and no dither.
    Saturn,
    /// Soft 3-point filtered textures, thick fog close to the camera,
    /// dithered 15-bit colour smoothed by the video blur.
    N64,
    /// Software-rendered Quake: 320 × 200, square texture pixels, light
    /// stepping through a 256-colour palette with glowing fullbrights.
    Quake,
}

impl RetroStyle {
    pub const ALL: [RetroStyle; 5] = [
        RetroStyle::Modern,
        RetroStyle::Ps1,
        RetroStyle::Saturn,
        RetroStyle::N64,
        RetroStyle::Quake,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RetroStyle::Modern => "Modern (off)",
            RetroStyle::Ps1 => "PlayStation",
            RetroStyle::Saturn => "Saturn",
            RetroStyle::N64 => "Nintendo 64",
            RetroStyle::Quake => "Quake (software)",
        }
    }
}

/// Nintendo 64 style fog: nothing before `near`, then thickening in a
/// straight line to solid fog at `far` (distances from the camera, in
/// world units). Replaces the scene's distance fog while on; the fog
/// colour stays the scene's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct N64Fog {
    pub enabled: bool,
    /// Where it starts (animatable).
    pub near: Param,
    /// Where it is solid (animatable).
    pub far: Param,
}

impl Default for N64Fog {
    fn default() -> Self {
        N64Fog {
            enabled: false,
            near: Param::new(1.0),
            far: Param::new(24.0),
        }
    }
}

/// The quirks of 5th-generation 3D, for the whole scene.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Retro3d {
    pub enabled: bool,
    /// Draw the 3D scene small, without smoothing, and blow it up with
    /// square pixels. Shapes, shading and edges alias as on the console.
    pub resolution: RetroRes,
    /// Size for [`RetroRes::Custom`] (on a 4:3 screen).
    pub custom: [u32; 2],
    /// Text layers and logos stay at the output's full resolution. Off:
    /// they are as chunky as the scene.
    pub sharp_overlays: bool,
    /// Corners of the triangles jump to a coarse grid of screen pixels:
    /// the wobble of PlayStation polygons.
    pub snap: bool,
    /// The grid, on a 4:3 screen (fitted to the output like the
    /// resolution).
    pub snap_res: [u32; 2],
    /// How far the corners move to the grid, 0..1 (animatable: fade the
    /// wobble in).
    pub snap_amount: Param,
    /// Textures stretched straight across each triangle on the screen
    /// instead of with perspective, 0..1: the swimming, bending textures of
    /// the PlayStation and Saturn. Subdivide a shape to make it smaller.
    pub affine: Param,
    /// One texture filter for every shape and terrain (`None`: each
    /// material's own).
    pub filter: Option<TexFilter>,
    /// Fog thickening straight from near the camera (Nintendo 64).
    pub fog: N64Fog,
    /// Colours rounded to 32 levels per channel (15-bit colour), per
    /// polygon as it is drawn.
    pub color_15bit: bool,
    /// Ordered 4 × 4 dither before the rounding, 0..1 (animatable).
    pub dither: Param,
    /// The Nintendo 64's video output filter, 0..1 (animatable): smooths
    /// dither patterns away and softens the picture sideways.
    pub vi_blur: Param,
    /// Triangles with a corner closer to the camera than this vanish
    /// (world units, 0 = off, animatable), as on the PlayStation.
    pub near_cull: Param,
    /// Lighting that steps through a palette's colours (Quake).
    pub colormap: Colormap,
    /// The whole picture (text, logos and effects too) at an 8- or 16-bit
    /// machine's resolution, pixels as wide as they were.
    pub screen: ConsoleScreen,
}

impl Default for Retro3d {
    fn default() -> Self {
        Retro3d {
            enabled: false,
            resolution: RetroRes::Full,
            custom: [320, 240],
            sharp_overlays: true,
            snap: false,
            snap_res: [320, 240],
            snap_amount: Param::new(1.0),
            affine: Param::new(0.0),
            filter: None,
            fog: N64Fog::default(),
            color_15bit: false,
            dither: Param::new(1.0),
            vi_blur: Param::new(0.0),
            near_cull: Param::new(0.0),
            colormap: Colormap::default(),
            screen: ConsoleScreen::default(),
        }
    }
}

impl Retro3d {
    /// The size to draw the 3D scene at for an output of `out` pixels, or
    /// `None` to draw it at full size.
    pub fn internal_size(&self, out: (u32, u32)) -> Option<(u32, u32)> {
        if !self.enabled {
            return None;
        }
        let size = self.resolution.size(self.custom)?;
        Some(fit_to_output(size, out))
    }

    /// The vertex snapping grid (pixels across, down) for an output of
    /// `out` pixels, or `None` without snapping.
    pub fn snap_grid(&self, out: (u32, u32)) -> Option<(u32, u32)> {
        if !self.enabled || !self.snap {
            return None;
        }
        let s = [
            self.snap_res[0].clamp(8, 4096),
            self.snap_res[1].clamp(8, 4096),
        ];
        Some(fit_to_output(s, out))
    }

    /// Turn on the bundle of settings of a console look.
    pub fn apply_style(&mut self, style: RetroStyle) {
        let keep_sharp = self.sharp_overlays;
        *self = Retro3d {
            sharp_overlays: keep_sharp,
            ..Default::default()
        };
        match style {
            RetroStyle::Modern => {}
            RetroStyle::Ps1 => {
                self.enabled = true;
                self.resolution = RetroRes::R320x240;
                self.snap = true;
                self.snap_res = [320, 240];
                self.affine = Param::new(1.0);
                self.filter = Some(TexFilter::Nearest);
                self.color_15bit = true;
                self.dither = Param::new(1.0);
            }
            RetroStyle::Saturn => {
                self.enabled = true;
                self.resolution = RetroRes::Custom;
                self.custom = [320, 224];
                self.snap = true;
                self.snap_res = [320, 224];
                self.affine = Param::new(0.6);
                self.filter = Some(TexFilter::Nearest);
                self.color_15bit = true;
                self.dither = Param::new(0.0);
            }
            RetroStyle::N64 => {
                self.enabled = true;
                self.resolution = RetroRes::R320x240;
                self.filter = Some(TexFilter::ThreePoint);
                self.fog.enabled = true;
                self.color_15bit = true;
                self.dither = Param::new(1.0);
                self.vi_blur = Param::new(1.0);
            }
            RetroStyle::Quake => {
                self.enabled = true;
                self.resolution = RetroRes::Custom;
                self.custom = [320, 200];
                self.filter = Some(TexFilter::Nearest);
                self.colormap = Colormap {
                    enabled: true,
                    ..Default::default()
                };
            }
        }
    }

    /// The texture filter of a material with `own` as its filter.
    pub fn filter_for(&self, own: TexFilter) -> TexFilter {
        match (self.enabled, self.filter) {
            (true, Some(f)) => f,
            _ => own,
        }
    }

    /// Whether anything is switched on.
    pub fn is_active(&self) -> bool {
        let on = |p: &Param| p.base != 0.0 || p.is_animated();
        self.enabled
            && (self.resolution != RetroRes::Full
                || self.snap
                || on(&self.affine)
                || self.filter.is_some()
                || self.fog.enabled
                || self.color_15bit
                || on(&self.vi_blur)
                || on(&self.near_cull)
                || self.colormap.enabled
                || self.screen.enabled)
    }
}

/// How a texture is smoothed between its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TexFilter {
    /// Smooth, with smaller copies further away (modern).
    #[default]
    Smooth,
    /// Square pixels (PlayStation, Saturn, Quake).
    Nearest,
    /// Blended between the four nearest pixels, without smaller copies
    /// (so it shimmers in the distance).
    Bilinear,
    /// The Nintendo 64's cheaper blend of three pixels: soft, with a
    /// faint diagonal grain.
    ThreePoint,
}

impl TexFilter {
    pub const ALL: [TexFilter; 4] = [
        TexFilter::Smooth,
        TexFilter::Nearest,
        TexFilter::Bilinear,
        TexFilter::ThreePoint,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TexFilter::Smooth => "Smooth",
            TexFilter::Nearest => "Nearest (PS1)",
            TexFilter::Bilinear => "Bilinear, no mipmaps",
            TexFilter::ThreePoint => "3-point (N64)",
        }
    }

    pub fn index(self) -> u32 {
        TexFilter::ALL.iter().position(|f| *f == self).unwrap_or(0) as u32
    }
}

// ---------------------------------------------------------------------------
// The whole screen at an old machine's resolution

/// How the console's picture sits on the output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ScreenFrame {
    /// Stretched over the whole output (pixels as wide as that makes them).
    Fill,
    /// On a 4:3 television, with the border colour at the sides: pixels
    /// as wide as they were (a C64's 160 × 200 pixels are twice as wide
    /// as tall).
    #[default]
    Tv,
    /// Square pixels (handhelds: the Game Boy's 160 × 144 screen).
    SquarePixels,
}

impl ScreenFrame {
    pub const ALL: [ScreenFrame; 3] = [
        ScreenFrame::Fill,
        ScreenFrame::Tv,
        ScreenFrame::SquarePixels,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ScreenFrame::Fill => "Fill the output",
            ScreenFrame::Tv => "4:3 television",
            ScreenFrame::SquarePixels => "Square pixels",
        }
    }
}

/// The whole picture at an old machine's resolution: sampled once per
/// console pixel (after everything else is drawn), so pixels can be wider
/// than tall, inside an optional border.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConsoleScreen {
    pub enabled: bool,
    /// Pixels across and down.
    pub size: [u32; 2],
    pub frame: ScreenFrame,
    /// Colour around the picture.
    pub border: crate::color::Rgb,
    /// Border inside the frame on each side, as a share of its width and
    /// height (0 = the picture fills the frame). Spectrum: a wide border
    /// all round; top and bottom only: a letterbox.
    #[serde(skip_serializing_if = "is_zero2")]
    pub inset: [f32; 2],
    /// Stripes in the border while a tape loads (ZX Spectrum).
    #[serde(skip_serializing_if = "is_default")]
    pub stripes: BorderStripes,
}

fn is_zero2(v: &[f32; 2]) -> bool {
    *v == [0.0, 0.0]
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

impl Default for ConsoleScreen {
    fn default() -> Self {
        ConsoleScreen {
            enabled: false,
            size: [320, 200],
            frame: ScreenFrame::Tv,
            border: [0.0; 3],
            inset: [0.0, 0.0],
            stripes: BorderStripes::default(),
        }
    }
}

/// What the border shows while a tape loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StripeMode {
    #[default]
    Off,
    /// The pilot tone: broad red and cyan bands rolling.
    Pilot,
    /// Data: thin, jittering blue and yellow bands.
    Data,
    /// The whole load over the loop: pilot, then the picture's lines
    /// arriving in the Spectrum's memory order in black and white, then its
    /// colours by character rows, then a moment finished.
    Loading,
}

impl StripeMode {
    pub const ALL: [StripeMode; 4] = [
        StripeMode::Off,
        StripeMode::Pilot,
        StripeMode::Data,
        StripeMode::Loading,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StripeMode::Off => "Off",
            StripeMode::Pilot => "Pilot tone (red / cyan)",
            StripeMode::Data => "Data (blue / yellow)",
            StripeMode::Loading => "Loading screen (whole load)",
        }
    }
}

/// Border stripes of a loading tape (ZX Spectrum style).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BorderStripes {
    pub mode: StripeMode,
    /// Pilot bands down the frame.
    pub bands: f32,
    /// Pairs of pilot bands rolling past per loop (a whole number).
    pub speed: u32,
}

impl Default for BorderStripes {
    fn default() -> Self {
        BorderStripes {
            mode: StripeMode::Off,
            bands: 14.0,
            speed: 24,
        }
    }
}

/// The border stripes and the picture's loading at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripeState {
    /// 0 none, 1 pilot, 2 data.
    pub mode: u32,
    /// Bands down the frame.
    pub bands: f32,
    /// How far the pilot bands have rolled (bands; whole pairs per loop).
    pub offset: f32,
    /// Changes a whole number of times per loop: the data bands' pattern.
    pub seed: u32,
    /// The picture's loading: 0 shown in full; 0.5 black (pilot); 1..2 its
    /// lines arriving (black and white); 2..3 its colours arriving.
    pub reveal: f32,
}

/// Where a whole load is at `phase`: pilot, bitmap, colours, finished.
const LOAD_PILOT: f32 = 0.12;
const LOAD_BITMAP: f32 = 0.8;
const LOAD_COLOURS: f32 = 0.92;

impl BorderStripes {
    pub fn at(&self, phase: f32, loop_beats: u32) -> StripeState {
        let p = phase.rem_euclid(1.0);
        let steps = (loop_beats.max(1) * 12) as f32;
        let mut st = StripeState {
            mode: 0,
            bands: self.bands.max(1.0),
            offset: (p * 2.0 * self.speed as f32).rem_euclid(2.0),
            seed: (p * steps).floor() as u32,
            reveal: 0.0,
        };
        match self.mode {
            StripeMode::Off => {}
            StripeMode::Pilot => st.mode = 1,
            StripeMode::Data => st.mode = 2,
            StripeMode::Loading => {
                if p < LOAD_PILOT {
                    st.mode = 1;
                    st.reveal = 0.5;
                } else if p < LOAD_BITMAP {
                    st.mode = 2;
                    st.reveal = 1.0 + (p - LOAD_PILOT) / (LOAD_BITMAP - LOAD_PILOT);
                } else if p < LOAD_COLOURS {
                    st.mode = 2;
                    st.reveal = 2.0 + (p - LOAD_BITMAP) / (LOAD_COLOURS - LOAD_BITMAP);
                }
            }
        }
        st
    }
}

impl ConsoleScreen {
    /// The picture's rectangle on an output of `out` pixels, in 0..1 of
    /// the output (left, top, width, height).
    pub fn rect(&self, out: (u32, u32)) -> [f32; 4] {
        let f = self.frame_rect(out);
        let ix = self.inset[0].clamp(0.0, 0.45);
        let iy = self.inset[1].clamp(0.0, 0.45);
        [
            f[0] + f[2] * ix,
            f[1] + f[3] * iy,
            f[2] * (1.0 - 2.0 * ix),
            f[3] * (1.0 - 2.0 * iy),
        ]
    }

    /// The whole frame (picture and border) on the output, as `rect`.
    pub fn frame_rect(&self, out: (u32, u32)) -> [f32; 4] {
        let a = out.0.max(1) as f32 / out.1.max(1) as f32;
        let ix = self.inset[0].clamp(0.0, 0.45);
        let iy = self.inset[1].clamp(0.0, 0.45);
        let d = match self.frame {
            ScreenFrame::Fill => a,
            ScreenFrame::Tv => 4.0 / 3.0,
            // Square pixels in the picture: the frame grows round it.
            ScreenFrame::SquarePixels => {
                self.size[0].max(1) as f32 / self.size[1].max(1) as f32 * (1.0 - 2.0 * iy)
                    / (1.0 - 2.0 * ix)
            }
        };
        if d < a {
            let w = d / a;
            [(1.0 - w) * 0.5, 0.0, w, 1.0]
        } else {
            let h = a / d;
            [0.0, (1.0 - h) * 0.5, 1.0, h]
        }
    }
}

/// Old machines' screens, with their palettes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenPreset {
    C64Multicolour,
    C64Hires,
    ZxSpectrum,
    AmstradMode0,
    Cga,
    MsDosVga,
    AmigaLores,
    Nes,
    MasterSystem,
    MegaDrive,
    GameBoy,
    VirtualBoy,
    Macintosh,
}

impl ScreenPreset {
    pub const ALL: [ScreenPreset; 13] = [
        ScreenPreset::C64Multicolour,
        ScreenPreset::C64Hires,
        ScreenPreset::ZxSpectrum,
        ScreenPreset::AmstradMode0,
        ScreenPreset::Cga,
        ScreenPreset::MsDosVga,
        ScreenPreset::AmigaLores,
        ScreenPreset::Nes,
        ScreenPreset::MasterSystem,
        ScreenPreset::MegaDrive,
        ScreenPreset::GameBoy,
        ScreenPreset::VirtualBoy,
        ScreenPreset::Macintosh,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ScreenPreset::C64Multicolour => "C64 multicolour 160×200",
            ScreenPreset::C64Hires => "C64 hi-res 320×200",
            ScreenPreset::ZxSpectrum => "ZX Spectrum 256×192",
            ScreenPreset::AmstradMode0 => "Amstrad CPC mode 0 160×200",
            ScreenPreset::Cga => "PC CGA 320×200",
            ScreenPreset::MsDosVga => "MS-DOS VGA 320×200",
            ScreenPreset::AmigaLores => "Amiga lo-res 320×256",
            ScreenPreset::Nes => "NES 256×240",
            ScreenPreset::MasterSystem => "Master System 256×192",
            ScreenPreset::MegaDrive => "Mega Drive 320×224",
            ScreenPreset::GameBoy => "Game Boy 160×144",
            ScreenPreset::VirtualBoy => "Virtual Boy 384×224",
            ScreenPreset::Macintosh => "Macintosh 512×342",
        }
    }

    /// Border inside the frame (share of width, height per side): the
    /// machines whose picture sat inside a wide border.
    pub fn inset(self) -> [f32; 2] {
        match self {
            // 256 × 192 inside a 352 × 296 display.
            ScreenPreset::ZxSpectrum => [0.136, 0.176],
            // 320 × 200 inside roughly 384 × 272.
            ScreenPreset::C64Multicolour | ScreenPreset::C64Hires => [0.083, 0.132],
            ScreenPreset::AmstradMode0 => [0.083, 0.132],
            _ => [0.0, 0.0],
        }
    }

    /// Size, frame, border (sRGB) and palette.
    pub fn spec(self) -> ([u32; 2], ScreenFrame, u32, crate::palette::PaletteId) {
        use crate::palette::PaletteId as P;
        use ScreenFrame::*;
        match self {
            ScreenPreset::C64Multicolour => ([160, 200], Tv, 0x6c5eb5, P::C64),
            ScreenPreset::C64Hires => ([320, 200], Tv, 0x6c5eb5, P::C64),
            ScreenPreset::ZxSpectrum => ([256, 192], Tv, 0x0000ff, P::Spectrum),
            ScreenPreset::AmstradMode0 => ([160, 200], Tv, 0x000080, P::AmstradCpc),
            ScreenPreset::Cga => ([320, 200], Tv, 0x000000, P::Cga),
            ScreenPreset::MsDosVga => ([320, 200], Tv, 0x000000, P::Vga),
            ScreenPreset::AmigaLores => ([320, 256], Tv, 0x000000, P::Amiga),
            ScreenPreset::Nes => ([256, 240], Tv, 0x000000, P::Nes),
            ScreenPreset::MasterSystem => ([256, 192], Tv, 0x000000, P::MasterSystem),
            ScreenPreset::MegaDrive => ([320, 224], Tv, 0x000000, P::MegaDrive),
            ScreenPreset::GameBoy => ([160, 144], SquarePixels, 0x8bac0f, P::GameBoy),
            ScreenPreset::VirtualBoy => ([384, 224], SquarePixels, 0x000000, P::VirtualBoy),
            ScreenPreset::Macintosh => ([512, 342], SquarePixels, 0x000000, P::OneBit),
        }
    }

    /// Turn the screen and its palette on (the palette reduction post
    /// effect, dithered).
    pub fn apply(self, retro: &mut Retro3d, palette: &mut crate::scene::PaletteFx) {
        let (size, frame, border, pal) = self.spec();
        retro.enabled = true;
        retro.screen = ConsoleScreen {
            enabled: true,
            size,
            frame,
            border: crate::color::hex(border),
            inset: self.inset(),
            stripes: BorderStripes::default(),
        };
        palette.enabled = true;
        palette.palette = pal;
    }
}

// ---------------------------------------------------------------------------
// Quake-era features

/// A light flickering by a string of letters, the way Quake's light
/// styles work: 'a' is dark, 'm' normal and 'z' about twice as bright;
/// one letter after the other, the whole string `plays` times per loop
/// (a whole number, so the loop stays seamless).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightStyle {
    /// Letters a..z; empty = steady.
    pub pattern: String,
    /// Whole plays of the string per loop.
    pub plays: u32,
}

impl Default for LightStyle {
    fn default() -> Self {
        LightStyle {
            pattern: String::new(),
            plays: 1,
        }
    }
}

/// Quake played light styles at ten letters a second.
pub const LIGHT_STYLE_RATE: f32 = 10.0;

impl LightStyle {
    pub fn is_on(&self) -> bool {
        self.letters().next().is_some()
    }

    fn letters(&self) -> impl Iterator<Item = u8> + '_ {
        self.pattern
            .bytes()
            .map(|b| b.to_ascii_lowercase())
            .filter(u8::is_ascii_lowercase)
    }

    /// Brightness at a loop phase (1 = normal, 'm'), steady 1 when off.
    pub fn eval(&self, phase: f32) -> f32 {
        let letters: Vec<u8> = self.letters().collect();
        if letters.is_empty() {
            return 1.0;
        }
        let n = letters.len();
        let steps = n as f32 * self.plays.max(1) as f32;
        let i = (phase.rem_euclid(1.0) * steps).floor() as usize % n;
        (letters[i] - b'a') as f32 / (b'm' - b'a') as f32
    }

    /// Letters per second for a loop of `loop_seconds`.
    pub fn rate(&self, loop_seconds: f32) -> f32 {
        let n = self.letters().count() as f32;
        n * self.plays.max(1) as f32 / loop_seconds.max(1e-3)
    }

    /// The whole number of plays per loop closest to `rate` letters per
    /// second.
    pub fn plays_for_rate(&self, rate: f32, loop_seconds: f32) -> u32 {
        let n = self.letters().count().max(1) as f32;
        ((rate * loop_seconds / n).round() as u32).max(1)
    }
}

/// Ready-made light styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightStylePreset {
    Steady,
    Flicker,
    Candle,
    Pulse,
    SlowPulse,
    Strobe,
    SlowStrobe,
    Fluorescent,
    Torch,
}

impl LightStylePreset {
    pub const ALL: [LightStylePreset; 9] = [
        LightStylePreset::Steady,
        LightStylePreset::Flicker,
        LightStylePreset::Candle,
        LightStylePreset::Pulse,
        LightStylePreset::SlowPulse,
        LightStylePreset::Strobe,
        LightStylePreset::SlowStrobe,
        LightStylePreset::Fluorescent,
        LightStylePreset::Torch,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LightStylePreset::Steady => "Steady",
            LightStylePreset::Flicker => "Flicker",
            LightStylePreset::Candle => "Candle",
            LightStylePreset::Pulse => "Pulse",
            LightStylePreset::SlowPulse => "Slow pulse",
            LightStylePreset::Strobe => "Strobe",
            LightStylePreset::SlowStrobe => "Slow strobe",
            LightStylePreset::Fluorescent => "Broken fluorescent",
            LightStylePreset::Torch => "Torch",
        }
    }

    pub fn pattern(self) -> &'static str {
        match self {
            LightStylePreset::Steady => "",
            LightStylePreset::Flicker => "mmnmlmnompmnmlmmonmmlnmm",
            LightStylePreset::Candle => "mmmlmmnmmmlklmmmmnnmmlmmmkmm",
            LightStylePreset::Pulse => "abcdefghijklmnopqrstuvwxyzyxwvutsrqponmlkjihgfedcb",
            LightStylePreset::SlowPulse => "ghijklmnopqrstuvwxyzzyxwvutsrqponmlkjihgg",
            LightStylePreset::Strobe => "zazaaaza",
            LightStylePreset::SlowStrobe => "aaaaaaaazzzzzzzz",
            LightStylePreset::Fluorescent => "mmammmmmamamaaammmmmmammma",
            LightStylePreset::Torch => "nmlnopmnlmomnmlpnmonmlnmp",
        }
    }

    /// The style, playing at about Quake's ten letters a second in a loop
    /// of `loop_seconds`.
    pub fn style(self, loop_seconds: f32) -> LightStyle {
        let mut s = LightStyle {
            pattern: self.pattern().into(),
            plays: 1,
        };
        s.plays = s.plays_for_rate(LIGHT_STYLE_RATE, loop_seconds);
        s
    }
}

/// Quake's turbulent warp for liquids: each texture coordinate wobbles by
/// a sine of the other, `cycles` whole times per loop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Turbulence {
    /// How far the texture wobbles, in tiles (0 = off; animatable).
    pub amount: Param,
    /// Waves per texture tile.
    pub waves: f32,
    /// Whole wobbles per loop.
    pub cycles: i32,
}

impl Default for Turbulence {
    fn default() -> Self {
        Turbulence {
            amount: Param::new(0.0),
            waves: 1.0,
            cycles: 4,
        }
    }
}

impl Turbulence {
    pub fn is_on(&self) -> bool {
        self.amount.base != 0.0 || self.amount.is_animated()
    }
}

/// Which palette the colormap lighting shades through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColormapPalette {
    /// Our own 256 colours in 16 ramps of 16 shades, the last 32 glowing
    /// (fullbright), in the style of 90s software 3D.
    #[default]
    Software256,
    /// One of the retro palettes (no fullbrights).
    Retro(crate::palette::PaletteId),
}

impl ColormapPalette {
    pub fn label(self) -> String {
        match self {
            ColormapPalette::Software256 => "Software 3D (256)".into(),
            ColormapPalette::Retro(p) => p.label().into(),
        }
    }

    /// The colours (sRGB bytes).
    pub fn colors(self) -> Vec<[u8; 3]> {
        match self {
            ColormapPalette::Software256 => software_palette(),
            ColormapPalette::Retro(p) => p
                .all_colors()
                .iter()
                .take(256)
                .map(|c| [(c >> 16) as u8, (c >> 8) as u8, *c as u8])
                .collect(),
        }
    }

    /// Entries from this index up glow whatever the light (fullbrights).
    pub fn fullbright_from(self) -> usize {
        match self {
            ColormapPalette::Software256 => 224,
            ColormapPalette::Retro(_) => usize::MAX,
        }
    }
}

/// Our 256-colour "software 3D" palette: 14 lit ramps of 16 shades from
/// near black to light (greys, browns, stone, greens, reds, rust, gold,
/// skin, purples, blues, teal, olive, clay, sky), then two fullbright ramps
/// (fire, and blue-and-red lamps).
pub fn software_palette() -> Vec<[u8; 3]> {
    // (dark end, bright end) of each lit ramp, sRGB.
    const RAMPS: [([f32; 3], [f32; 3]); 14] = [
        ([0.0, 0.0, 0.0], [0.92, 0.92, 0.92]),
        ([0.06, 0.04, 0.02], [0.75, 0.55, 0.36]),
        ([0.04, 0.04, 0.05], [0.66, 0.66, 0.72]),
        ([0.02, 0.05, 0.01], [0.45, 0.72, 0.30]),
        ([0.06, 0.01, 0.01], [0.95, 0.30, 0.26]),
        ([0.07, 0.03, 0.01], [0.85, 0.48, 0.22]),
        ([0.06, 0.05, 0.01], [0.95, 0.82, 0.35]),
        ([0.08, 0.05, 0.04], [0.98, 0.78, 0.66]),
        ([0.04, 0.01, 0.06], [0.72, 0.46, 0.85]),
        ([0.01, 0.02, 0.07], [0.45, 0.58, 0.95]),
        ([0.01, 0.05, 0.05], [0.38, 0.80, 0.78]),
        ([0.04, 0.05, 0.02], [0.70, 0.72, 0.42]),
        ([0.07, 0.04, 0.03], [0.82, 0.62, 0.50]),
        ([0.03, 0.04, 0.07], [0.72, 0.84, 0.98]),
    ];
    let mut out = Vec::with_capacity(256);
    for (dark, bright) in RAMPS {
        for k in 0..16 {
            let t = k as f32 / 15.0;
            let c: [f32; 3] = std::array::from_fn(|i| dark[i] + (bright[i] - dark[i]) * t);
            out.push(c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    // Fullbrights: fire from deep red to pale yellow, then blue and red
    // lamp colours.
    for k in 0..16 {
        let t = k as f32 / 15.0;
        out.push([
            (160.0 + 95.0 * t.min(0.5) * 2.0) as u8,
            (30.0 + 210.0 * t) as u8,
            (10.0 + 150.0 * t * t) as u8,
        ]);
    }
    for k in 0..8 {
        let t = k as f32 / 7.0;
        out.push([(40.0 + 200.0 * t) as u8, (90.0 + 160.0 * t) as u8, 255]);
    }
    for k in 0..8 {
        let t = k as f32 / 7.0;
        out.push([255, (40.0 + 200.0 * t) as u8, (40.0 + 160.0 * t) as u8]);
    }
    out
}

/// Palette-space lighting ("colormap"): lit colours step through the
/// palette's own colours, like Quake's software renderer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colormap {
    pub enabled: bool,
    pub palette: ColormapPalette,
    /// Light levels from dark to twice as bright (Quake: 32).
    pub levels: u32,
    /// The palette's glowing entries ignore light.
    pub fullbrights: bool,
}

impl Default for Colormap {
    fn default() -> Self {
        Colormap {
            enabled: false,
            palette: ColormapPalette::Software256,
            levels: 32,
            fullbrights: true,
        }
    }
}

/// Side of the colour cube that finds a colour's palette entry.
pub const COLORMAP_CUBE: u32 = 32;

/// The colormap's lookup tables:
/// - `index`: for each colour of a 32 × 32 × 32 cube (sRGB; red across,
///   green in blocks of 32 across, blue down: 1024 × 32), its nearest
///   palette entry;
/// - `table`: for each palette entry (across, 256 wide) and light level
///   (down, `levels` from black to twice as bright) the nearest palette
///   colour to that entry lit (in sRGB; fullbrights stay as they are).
pub struct ColormapTables {
    pub index: Vec<u8>,
    pub table: Vec<[u8; 4]>,
    pub levels: u32,
}

fn nearest_entry(pal: &[[f32; 3]], c: [f32; 3], lit_only: usize) -> usize {
    let mut best = 0;
    let mut best_d = f32::MAX;
    for (i, p) in pal.iter().enumerate().take(lit_only) {
        let d = 0.3 * (p[0] - c[0]).powi(2)
            + 0.59 * (p[1] - c[1]).powi(2)
            + 0.11 * (p[2] - c[2]).powi(2);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best
}

impl Colormap {
    pub fn tables(&self) -> ColormapTables {
        let pal: Vec<[f32; 3]> = self
            .palette
            .colors()
            .iter()
            .map(|c| c.map(|v| v as f32 / 255.0))
            .collect();
        let n = pal.len().clamp(1, 256);
        let fb = if self.fullbrights {
            self.palette.fullbright_from().min(n)
        } else {
            n
        };
        let cube = COLORMAP_CUBE as usize;
        let mut index = vec![0u8; cube * cube * cube];
        for b in 0..cube {
            for g in 0..cube {
                for r in 0..cube {
                    let c = [r, g, b].map(|v| v as f32 / (cube - 1) as f32);
                    // Every entry (fullbrights too) can be a surface colour.
                    index[b * cube * cube + g * cube + r] = nearest_entry(&pal, c, n) as u8;
                }
            }
        }
        let levels = self.levels.clamp(2, 64);
        let mut table = vec![[0u8, 0, 0, 255]; 256 * levels as usize];
        for l in 0..levels as usize {
            let k = l as f32 / (levels - 1) as f32 * 2.0;
            for (i, p) in pal.iter().enumerate().take(n) {
                let out = if i >= fb {
                    *p
                } else {
                    // Lit colours are found among the lit entries only.
                    let lit = p.map(|v| (v * k).min(1.0));
                    pal[nearest_entry(&pal, lit, fb.max(1))]
                };
                table[l * 256 + i] = [
                    (out[0] * 255.0).round() as u8,
                    (out[1] * 255.0).round() as u8,
                    (out[2] * 255.0).round() as u8,
                    255,
                ];
            }
        }
        ColormapTables {
            index,
            table,
            levels,
        }
    }

    /// What the tables are made from (a cache key).
    pub fn key(&self) -> String {
        format!(
            "{:?}:{}:{}",
            self.palette,
            self.levels.clamp(2, 64),
            self.fullbrights
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_sizes_keep_their_pixel_shape() {
        // 4:3 output: as is.
        assert_eq!(fit_to_output([320, 240], (1440, 1080)), (320, 240));
        // 16:9: wider.
        assert_eq!(fit_to_output([320, 240], (1920, 1080)), (427, 240));
        // Smaller outputs than the console size are drawn at their size.
        assert_eq!(fit_to_output([320, 240], (160, 90)), (160, 90));
    }

    #[test]
    fn off_means_full_size() {
        let mut r = Retro3d::default();
        assert_eq!(r.internal_size((1920, 1080)), None);
        r.apply_style(RetroStyle::Ps1);
        assert!(r.is_active());
        assert_eq!(r.internal_size((1920, 1080)), Some((427, 240)));
        assert_eq!(r.snap_grid((1920, 1080)), Some((427, 240)));
        r.apply_style(RetroStyle::Modern);
        assert!(!r.is_active());
    }

    #[test]
    fn light_styles_step_through_letters_and_loop() {
        let s = LightStyle {
            pattern: "amz".into(),
            plays: 2,
        };
        assert_eq!(s.eval(0.0), 0.0);
        assert_eq!(s.eval(1.0 / 6.0 + 1e-4), 1.0);
        assert!((s.eval(2.0 / 6.0 + 1e-4) - 25.0 / 12.0).abs() < 1e-6);
        // Second play, and the loop wraps to the first letter.
        assert_eq!(s.eval(0.5 + 1e-4), 0.0);
        assert_eq!(s.eval(1.0), s.eval(0.0));
        assert_eq!(LightStyle::default().eval(0.3), 1.0);
        // Ten letters a second over an 8-second loop: 80 letters.
        let c = LightStylePreset::Flicker.style(8.0);
        assert_eq!(c.plays, (80.0f32 / 24.0).round() as u32);
        assert!((c.rate(8.0) - 10.0).abs() < 2.0);
    }

    #[test]
    fn software_palette_and_colormap() {
        let pal = software_palette();
        assert_eq!(pal.len(), 256);
        let cm = Colormap {
            enabled: true,
            ..Default::default()
        };
        let t = cm.tables();
        assert_eq!(t.index.len(), 32 * 32 * 32);
        assert_eq!(t.table.len(), 256 * 32);
        // Level 0 is black for lit entries; fullbrights keep their colour.
        let white = 15; // brightest grey
        assert_eq!(&t.table[white][..3], &[0, 0, 0]);
        assert_eq!(&t.table[240][..3], &pal[240]);
        // Half way (normal light) a colour maps to itself.
        let mid = (t.levels as usize - 1) / 2;
        let lit = t.table[mid * 256 + 40];
        let d: i32 = (0..3)
            .map(|c| (lit[c] as i32 - pal[40][c] as i32).abs())
            .sum();
        assert!(d < 40, "normal light moves the colour: {d}");
    }

    #[test]
    fn screens_keep_their_shape() {
        let mut s = ConsoleScreen {
            enabled: true,
            size: [160, 200],
            ..Default::default()
        };
        // 4:3 on 16:9: pillarboxed to three quarters of the width.
        let r = s.rect((1920, 1080));
        assert!((r[2] - 0.75).abs() < 1e-6 && (r[0] - 0.125).abs() < 1e-6 && r[3] == 1.0);
        // Game Boy: square pixels, 10:9.
        s.size = [160, 144];
        s.frame = ScreenFrame::SquarePixels;
        let r = s.rect((1920, 1080));
        assert!((r[2] * 1920.0 / 160.0 - 1080.0 / 144.0).abs() < 1e-3);
        // Taller output than the picture: bars above and below.
        s.frame = ScreenFrame::Tv;
        let r = s.rect((1080, 1920));
        assert!(r[2] == 1.0 && r[3] < 1.0 && r[1] > 0.0);
        s.frame = ScreenFrame::Fill;
        assert_eq!(s.rect((1920, 1080)), [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn borders_and_loading() {
        let s = ConsoleScreen {
            enabled: true,
            size: [256, 192],
            inset: [0.1, 0.2],
            ..Default::default()
        };
        let (f, r) = (s.frame_rect((1920, 1080)), s.rect((1920, 1080)));
        assert!((r[3] - f[3] * 0.6).abs() < 1e-6 && (r[1] - (f[1] + f[3] * 0.2)).abs() < 1e-6);
        // A load: pilot, bitmap, colours, done; the same at both ends.
        let b = BorderStripes {
            mode: StripeMode::Loading,
            ..Default::default()
        };
        let (a, z) = (b.at(0.0, 16), b.at(1.0, 16));
        assert_eq!(
            (a.mode, a.reveal, a.seed, a.offset),
            (z.mode, z.reveal, z.seed, z.offset)
        );
        assert_eq!(a.mode, 1);
        assert_eq!(b.at(0.5, 16).mode, 2);
        assert!((1.0..2.0).contains(&b.at(0.5, 16).reveal));
        assert!((2.0..3.0).contains(&b.at(0.85, 16).reveal));
        assert_eq!((b.at(0.95, 16).mode, b.at(0.95, 16).reveal), (0, 0.0));
    }

    #[test]
    fn saves_nothing_when_off() {
        let json = serde_json::to_string(&Retro3d::default()).unwrap();
        let back: Retro3d = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Retro3d::default());
    }
}
