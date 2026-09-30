//! Retro palettes used by the palette-reduction post effect and the texture
//! "retro-ize" import option. Colours are sRGB `0xRRGGBB`.

use serde::{Deserialize, Serialize};

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
    pub enum PaletteId {
        /// IBM EGA 16 colours.
        #[default]
        Ega => "EGA (16)",
        /// CGA mode 4, palette 1 high intensity.
        Cga => "CGA (4)",
        /// Commodore 64 (Pepto).
        C64 => "Commodore 64 (16)",
        /// Original Game Boy greens.
        GameBoy => "Game Boy (4)",
        /// PICO-8 fantasy console.
        Pico8 => "PICO-8 (16)",
        /// Amiga-style copper gradient (16 warm-to-cool steps).
        AmigaCopper => "Amiga copper (16)",
        /// ZX Spectrum bright colours.
        Spectrum => "ZX Spectrum (8)",
        /// VGA-like 6x6x6 colour cube (216 colours), handled as a quantiser.
        Vga => "VGA cube (216)",
        /// Monochrome green phosphor terminal.
        Phosphor => "Green phosphor (4)",
        /// Nintendo Entertainment System (the 2C02 palette, 55 distinct colours).
        Nes => "NES (55)",
        /// Black and white only (Macintosh, ZX81, e-ink).
        OneBit => "1-bit black & white (2)",
        /// Virtual Boy: four shades of red on black.
        VirtualBoy => "Virtual Boy red (4)",
        /// Amstrad CPC: 3 levels per channel (27 colours), as a quantiser.
        AmstradCpc => "Amstrad CPC (27)",
        /// Sega Master System: 4 levels per channel (64 colours).
        MasterSystem => "Master System (64)",
        /// Sega Mega Drive / Genesis: 8 levels per channel (512 colours).
        MegaDrive => "Mega Drive (512)",
        /// Amiga OCS/ECS: 16 levels per channel (4096 colours).
        Amiga => "Amiga 12-bit (4096)",
    }
}

impl PaletteId {
    /// Levels per channel of the palettes that are colour cubes (every mix
    /// of that many levels of red, green and blue), handled as quantisers.
    pub fn levels(self) -> Option<u32> {
        match self {
            PaletteId::Vga => Some(6),
            PaletteId::AmstradCpc => Some(3),
            PaletteId::MasterSystem => Some(4),
            PaletteId::MegaDrive => Some(8),
            PaletteId::Amiga => Some(16),
            _ => None,
        }
    }

    /// Number of colours.
    pub fn count(self) -> usize {
        match self.levels() {
            Some(n) => (n * n * n) as usize,
            None => self.colors().len(),
        }
    }

    /// Every colour, cubes included (sRGB `0xRRGGBB`).
    pub fn all_colors(self) -> Vec<u32> {
        match self.levels() {
            Some(n) => {
                let lv = |i: u32| (i * 255 + (n - 1) / 2) / (n - 1);
                let mut v = Vec::with_capacity((n * n * n) as usize);
                for r in 0..n {
                    for g in 0..n {
                        for b in 0..n {
                            v.push((lv(r) << 16) | (lv(g) << 8) | lv(b));
                        }
                    }
                }
                v
            }
            None => self.colors().to_vec(),
        }
    }

    /// Colours of the palette. Empty for the colour cubes (see
    /// [`PaletteId::levels`]).
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
            // The widely used NTSC 2C02 approximation, duplicates and the
            // unused blacks left out.
            PaletteId::Nes => &[
                0x7c7c7c, 0x0000fc, 0x0000bc, 0x4428bc, 0x940084, 0xa80020, 0xa81000, 0x881400,
                0x503000, 0x007800, 0x006800, 0x005800, 0x004058, 0x000000, 0xbcbcbc, 0x0078f8,
                0x0058f8, 0x6844fc, 0xd800cc, 0xe40058, 0xf83800, 0xe45c10, 0xac7c00, 0x00b800,
                0x00a800, 0x00a844, 0x008888, 0xf8f8f8, 0x3cbcfc, 0x6888fc, 0x9878f8, 0xf878f8,
                0xf85898, 0xf87858, 0xfca044, 0xf8b800, 0xb8f818, 0x58d854, 0x58f898, 0x00e8d8,
                0x787878, 0xfcfcfc, 0xa4e4fc, 0xb8b8f8, 0xd8b8f8, 0xf8b8f8, 0xf8a4c0, 0xf0d0b0,
                0xfce0a8, 0xf8d878, 0xd8f878, 0xb8f8b8, 0xb8f8d8, 0x00fcfc, 0xf8d8f8,
            ],
            PaletteId::OneBit => &[0x000000, 0xffffff],
            PaletteId::VirtualBoy => &[0x000000, 0x550000, 0xaa0000, 0xff0000],
            PaletteId::AmstradCpc
            | PaletteId::MasterSystem
            | PaletteId::MegaDrive
            | PaletteId::Amiga => &[],
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
    if let Some(n) = palette.levels() {
        let k = (n - 1) as f32;
        return c.map(|v| (((v + d / k) * k).round().clamp(0.0, k)) / k);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_sizes() {
        assert_eq!(PaletteId::Nes.count(), 55);
        assert_eq!(PaletteId::AmstradCpc.count(), 27);
        assert_eq!(PaletteId::Vga.all_colors().len(), 216);
        assert_eq!(PaletteId::Amiga.count(), 4096);
        // Cubes reach black and white exactly.
        let c = PaletteId::MegaDrive.all_colors();
        assert_eq!((c[0], c[c.len() - 1]), (0x000000, 0xffffff));
        // The quantiser keeps a cube's own levels.
        let q = quantize([0.5, 0.2, 1.0], PaletteId::AmstradCpc, 0.0, 0, 0);
        assert_eq!(q, [0.5, 0.0, 1.0]);
    }
}
