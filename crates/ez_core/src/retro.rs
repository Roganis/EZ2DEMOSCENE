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
    /// Wobbly vertices, warped textures, chunky pixels at 320 × 240.
    Ps1,
}

impl RetroStyle {
    pub const ALL: [RetroStyle; 2] = [RetroStyle::Modern, RetroStyle::Ps1];

    pub fn label(self) -> &'static str {
        match self {
            RetroStyle::Modern => "Modern (off)",
            RetroStyle::Ps1 => "PlayStation",
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
        self.enabled
            && *self
                != Retro3d {
                    enabled: true,
                    sharp_overlays: self.sharp_overlays,
                    custom: self.custom,
                    snap_res: self.snap_res,
                    ..Default::default()
                }
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
    fn saves_nothing_when_off() {
        let json = serde_json::to_string(&Retro3d::default()).unwrap();
        let back: Retro3d = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Retro3d::default());
    }
}
