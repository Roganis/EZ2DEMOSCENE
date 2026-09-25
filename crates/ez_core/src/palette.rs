//! Retro palettes used by the palette-reduction post effect and the texture
//! "retro-ize" import option. Colours are sRGB `0xRRGGBB`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PaletteId {
    /// IBM EGA 16 colours.
    #[default]
    Ega,
    /// CGA mode 4, palette 1 high intensity.
    Cga,
    /// Commodore 64 (Pepto).
    C64,
    /// Original Game Boy greens.
    GameBoy,
    /// PICO-8 fantasy console.
    Pico8,
    /// Amiga-style copper gradient (16 warm-to-cool steps).
    AmigaCopper,
    /// ZX Spectrum bright colours.
    Spectrum,
    /// VGA-like 6x6x6 colour cube (216 colours), handled as a quantiser.
    Vga,
    /// Monochrome green phosphor terminal.
    Phosphor,
}

impl PaletteId {
    pub const ALL: [PaletteId; 9] = [
        PaletteId::Ega,
        PaletteId::Cga,
        PaletteId::C64,
        PaletteId::GameBoy,
        PaletteId::Pico8,
        PaletteId::AmigaCopper,
        PaletteId::Spectrum,
        PaletteId::Vga,
        PaletteId::Phosphor,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PaletteId::Ega => "EGA (16)",
            PaletteId::Cga => "CGA (4)",
            PaletteId::C64 => "Commodore 64 (16)",
            PaletteId::GameBoy => "Game Boy (4)",
            PaletteId::Pico8 => "PICO-8 (16)",
            PaletteId::AmigaCopper => "Amiga copper (16)",
            PaletteId::Spectrum => "ZX Spectrum (8)",
            PaletteId::Vga => "VGA cube (216)",
            PaletteId::Phosphor => "Green phosphor (4)",
        }
    }

    /// Colours of the palette. Empty for the VGA cube (use a quantiser).
    pub fn colors(self) -> &'static [u32] {
        match self {
            PaletteId::Ega => &[
                0x000000, 0x0000aa, 0x00aa00, 0x00aaaa, 0xaa0000, 0xaa00aa, 0xaa5500, 0xaaaaaa,
                0x555555, 0x5555ff, 0x55ff55, 0x55ffff, 0xff5555, 0xff55ff, 0xffff55, 0xffffff,
            ],
            PaletteId::Cga => &[0x000000, 0x55ffff, 0xff55ff, 0xffffff],
            PaletteId::C64 => &[
                0x000000, 0xffffff, 0x68372b, 0x70a4b2, 0x6f3d86, 0x588d43, 0x352879, 0xb8c76f,
                0x6f4f25, 0x433900, 0x9a6759, 0x444444, 0x6c6c6c, 0x9ad284, 0x6c5eb5, 0x959595,
            ],
            PaletteId::GameBoy => &[0x0f380f, 0x306230, 0x8bac0f, 0x9bbc0f],
            PaletteId::Pico8 => &[
                0x000000, 0x1d2b53, 0x7e2553, 0x008751, 0xab5236, 0x5f574f, 0xc2c3c7, 0xfff1e8,
                0xff004d, 0xffa300, 0xffec27, 0x00e436, 0x29adff, 0x83769c, 0xff77a8, 0xffccaa,
            ],
            PaletteId::AmigaCopper => &[
                0x000000, 0x110022, 0x220044, 0x440066, 0x660077, 0x880066, 0xaa1144, 0xcc3322,
                0xee6600, 0xff9900, 0xffcc33, 0xffee88, 0xffffff, 0x88ccff, 0x3388dd, 0x114488,
            ],
            PaletteId::Spectrum => &[
                0x000000, 0x0000ff, 0xff0000, 0xff00ff, 0x00ff00, 0x00ffff, 0xffff00, 0xffffff,
            ],
            PaletteId::Vga => &[],
            PaletteId::Phosphor => &[0x001a00, 0x006600, 0x22cc22, 0x99ff99],
        }
    }

    /// Colours as sRGB floats (0..1).
    pub fn colors_f32(self) -> Vec<[f32; 3]> {
        self.colors()
            .iter()
            .map(|c| {
                [
                    ((c >> 16) & 0xff) as f32 / 255.0,
                    ((c >> 8) & 0xff) as f32 / 255.0,
                    (c & 0xff) as f32 / 255.0,
                ]
            })
            .collect()
    }
}

/// 4x4 Bayer matrix threshold in (-0.5, 0.5).
pub fn bayer4(x: u32, y: u32) -> f32 {
    const M: [u32; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
    (M[((y & 3) * 4 + (x & 3)) as usize] as f32 + 0.5) / 16.0 - 0.5
}

/// Quantise an sRGB colour (0..1) to the palette, with ordered dithering.
pub fn quantize(c: [f32; 3], palette: PaletteId, dither: f32, x: u32, y: u32) -> [f32; 3] {
    let d = bayer4(x, y) * dither;
    if palette == PaletteId::Vga {
        return c.map(|v| (((v + d / 5.0) * 5.0).round().clamp(0.0, 5.0)) / 5.0);
    }
    let cols = palette.colors_f32();
    let spread = 1.0 / (cols.len() as f32).sqrt();
    let p = c.map(|v| (v + d * spread).clamp(0.0, 1.0));
    let mut best = cols[0];
    let mut best_d = f32::MAX;
    for col in cols {
        // Weighted distance roughly matching perceived brightness.
        let dr = p[0] - col[0];
        let dg = p[1] - col[1];
        let db = p[2] - col[2];
        let dist = 0.3 * dr * dr + 0.59 * dg * dg + 0.11 * db * db;
        if dist < best_d {
            best_d = dist;
            best = col;
        }
    }
    best
}
