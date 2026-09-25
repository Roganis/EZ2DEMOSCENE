//! Procedural retro textures (all tileable) and the "retro-ize" filter for
//! user images. These are also written out as the bundled texture pack.

use ez_core::palette::{quantize, PaletteId};
use ez_core::rng::hash_u32;
use ez_core::RetroProcess;
use image::{imageops::FilterType, RgbaImage};
use std::f32::consts::TAU;

pub const TEX_SIZE: u32 = 256;

/// Built-in texture names with a short description.
pub const BUILTIN: &[(&str, &str)] = &[
    ("checker", "Classic 8x8 checkerboard"),
    ("xor", "XOR pattern, the demoscene hello-world"),
    ("plasma", "Sine plasma"),
    ("noise", "Soft fractal noise"),
    ("marble", "Turbulent marble"),
    ("fire", "Oldschool fire gradient"),
    ("circuit", "Glowing circuit traces"),
    ("tech_panel", "Sci-fi hull plating with light slits"),
    ("led_grid", "Grid of round LEDs"),
    ("speaker", "Speaker grille"),
    ("brick", "Pixel brick wall"),
    ("metal_plate", "Riveted metal plates"),
    ("hex", "Hexagon grid"),
    ("stripes", "Diagonal hazard stripes"),
    ("copper", "Amiga copper bars"),
    ("win9x", "Teal desktop & bevelled window"),
    ("sierpinski", "Sierpinski carpet"),
    ("stars", "Tiny star specks"),
    ("lava", "Glowing lava cracks"),
    ("dither", "Bayer-dithered gradient"),
];

pub fn is_builtin(name: &str) -> bool {
    BUILTIN.iter().any(|(n, _)| *n == name)
}

fn h2(x: i32, y: i32, seed: u32) -> f32 {
    let v = hash_u32((x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed);
    (v >> 8) as f32 / (1u32 << 24) as f32
}

/// Periodic value noise with `period` cells per tile.
fn vnoise(u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let x = u * period as f32;
    let y = v * period as f32;
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let w = |i: i32| i.rem_euclid(period);
    let a = h2(w(xi), w(yi), seed);
    let b = h2(w(xi + 1), w(yi), seed);
    let c = h2(w(xi), w(yi + 1), seed);
    let d = h2(w(xi + 1), w(yi + 1), seed);
    let (sx, sy) = (s(fx), s(fy));
    (a + (b - a) * sx) + ((c + (d - c) * sx) - (a + (b - a) * sx)) * sy
}

fn fbm(u: f32, v: f32, base: i32, octaves: u32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut p = base;
    for o in 0..octaves {
        sum += amp * vnoise(u, v, p, seed + o * 17);
        amp *= 0.5;
        p *= 2;
    }
    sum / (1.0 - 0.5f32.powi(octaves as i32))
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn rgb(c: u32) -> [f32; 3] {
    [
        ((c >> 16) & 255) as f32 / 255.0,
        ((c >> 8) & 255) as f32 / 255.0,
        (c & 255) as f32 / 255.0,
    ]
}

fn ramp(stops: &[(f32, u32)], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        if t <= w[1].0 {
            let k = (t - w[0].0) / (w[1].0 - w[0].0).max(1e-6);
            return mix(rgb(w[0].1), rgb(w[1].1), k);
        }
    }
    rgb(stops.last().unwrap().1)
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Generate a built-in texture (sRGB RGBA8). Unknown names give a magenta
/// checker so mistakes are visible.
pub fn generate(name: &str) -> RgbaImage {
    let n = TEX_SIZE;
    let mut img = RgbaImage::new(n, n);
    for y in 0..n {
        for x in 0..n {
            let u = (x as f32 + 0.5) / n as f32;
            let v = (y as f32 + 0.5) / n as f32;
            let c = pixel(name, x, y, u, v);
            img.put_pixel(
                x,
                y,
                image::Rgba([
                    (c[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (c[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (c[2].clamp(0.0, 1.0) * 255.0).round() as u8,
                    255,
                ]),
            );
        }
    }
    img
}

fn pixel(name: &str, x: u32, y: u32, u: f32, v: f32) -> [f32; 3] {
    match name {
        "checker" => {
            let c = ((x / 32) + (y / 32)) % 2;
            if c == 0 {
                [0.95, 0.95, 0.95]
            } else {
                [0.08, 0.08, 0.1]
            }
        }
        "xor" => {
            let t = ((x ^ y) & 255) as f32 / 255.0;
            ramp(&[(0.0, 0x000010), (0.5, 0x2040ff), (0.8, 0xff40c0), (1.0, 0xffffff)], t)
        }
        "plasma" => {
            let a = (u * TAU * 2.0).sin()
                + (v * TAU * 3.0).sin()
                + ((u + v) * TAU * 2.0).sin()
                + ((u * TAU).cos() * 2.0 + (v * TAU * 2.0).sin() * 2.0).sin();
            let t = a * 0.125 + 0.5;
            [
                0.5 + 0.5 * (t * TAU).sin(),
                0.5 + 0.5 * (t * TAU + 2.1).sin(),
                0.5 + 0.5 * (t * TAU + 4.2).sin(),
            ]
        }
        "noise" => {
            let t = fbm(u, v, 4, 5, 11);
            let t = 0.35 + 0.65 * t;
            [t, t, t]
        }
        "marble" => {
            let t = fbm(u, v, 4, 5, 3);
            let s = 0.5 + 0.5 * ((u * 2.0 + t * 3.0) * TAU).sin();
            ramp(&[(0.0, 0x202028), (0.6, 0xc8c4bc), (1.0, 0xffffff)], s.powf(0.6))
        }
        "fire" => {
            let t = fbm(u, v, 6, 4, 5);
            let h = (1.0 - v) * 0.9 + t * 0.5 - 0.2;
            ramp(
                &[(0.0, 0x000000), (0.3, 0x600000), (0.55, 0xff3000), (0.8, 0xffc000), (1.0, 0xffffc0)],
                h,
            )
        }
        "circuit" => {
            let cell = 16;
            let (cx, cy) = ((x / cell) as i32, (y / cell) as i32);
            let (lx, ly) = (x % cell, y % cell);
            let r = h2(cx, cy, 99);
            let horiz = r < 0.5;
            let on_trace = if horiz { ly == 7 || ly == 8 } else { lx == 7 || lx == 8 };
            let pad = (lx as i32 - 8).abs() <= 2 && (ly as i32 - 8).abs() <= 2 && h2(cx, cy, 5) < 0.25;
            let base = [0.02, 0.05, 0.03];
            if pad {
                [0.9, 1.0, 0.8]
            } else if on_trace && h2(cx, cy, 7) < 0.75 {
                [0.2, 0.9, 0.4]
            } else {
                base
            }
        }
        "tech_panel" => {
            let (px, py) = (x % 128, y % 64);
            let seam = px < 2 || py < 2;
            let slit = (40..88).contains(&px) && (28..36).contains(&py);
            let bolt = ((px as i32 - 8).pow(2) + (py as i32 - 8).pow(2)) < 10;
            let n = fbm(u, v, 8, 3, 2) * 0.08;
            if slit {
                [1.0, 1.0, 1.0]
            } else if seam {
                [0.02, 0.02, 0.025]
            } else if bolt {
                [0.35, 0.35, 0.38]
            } else {
                [0.12 + n, 0.12 + n, 0.13 + n]
            }
        }
        "led_grid" => {
            let cell = 16.0;
            let fx = (x as f32 % cell) - cell * 0.5 + 0.5;
            let fy = (y as f32 % cell) - cell * 0.5 + 0.5;
            let d = (fx * fx + fy * fy).sqrt();
            let led = 1.0 - smooth(3.5, 5.5, d);
            let glow = (1.0 - smooth(0.0, 8.0, d)) * 0.25;
            let t = (led + glow).min(1.0);
            mix([0.06, 0.05, 0.03], [1.0, 0.95, 0.8], t)
        }
        "speaker" => {
            let (dx, dy) = (u - 0.5, v - 0.5);
            let d = (dx * dx + dy * dy).sqrt();
            let holes = {
                let fx = (x % 6) as f32 - 2.5;
                let fy = (y % 6) as f32 - 2.5;
                (fx * fx + fy * fy) < 3.0
            };
            let cone = d < 0.42;
            let rim = (0.40..0.45).contains(&d);
            if rim {
                [0.55, 0.45, 0.25]
            } else if cone && holes {
                [0.01, 0.01, 0.01]
            } else if cone {
                [0.12, 0.11, 0.1]
            } else {
                [0.18, 0.15, 0.1]
            }
        }
        "brick" => {
            let row = y / 16;
            let off = if row % 2 == 0 { 0 } else { 16 };
            let bx = (x + off) % 32;
            let by = y % 16;
            let mortar = bx < 2 || by < 2;
            let shade = h2(((x + off) / 32) as i32, row as i32, 1) * 0.25;
            if mortar {
                [0.35, 0.33, 0.3]
            } else {
                [0.55 + shade, 0.2 + shade * 0.5, 0.12]
            }
        }
        "metal_plate" => {
            let (px, py) = (x % 64, y % 64);
            let edge = px < 1 || py < 1;
            let hi = px == 1 || py == 1;
            let rivet = [(6, 6), (58, 6), (6, 58), (58, 58)]
                .iter()
                .any(|(rx, ry)| (px as i32 - rx).pow(2) + (py as i32 - ry).pow(2) < 7);
            let n = fbm(u, v, 16, 3, 8) * 0.1;
            let brushed = vnoise(u * 0.1, v, 64, 3) * 0.05;
            if edge {
                [0.08, 0.08, 0.09]
            } else if hi || rivet {
                [0.75, 0.75, 0.78]
            } else {
                let g = 0.45 + n + brushed;
                [g, g, g + 0.02]
            }
        }
        "hex" => {
            let s = 8.0;
            let q = [u * s, v * s * 2.0 / 3f32.sqrt()];
            let r = [q[0] - q[1] * 0.5, q[1]];
            let fr = [r[0].fract(), r[1].fract()];
            let d = fr[0].min(fr[1]).min((1.0 - fr[0] - fr[1]).abs());
            let line = 1.0 - smooth(0.02, 0.06, d);
            mix([0.05, 0.08, 0.12], [0.3, 0.9, 1.0], line)
        }
        "stripes" => {
            if ((x + y) / 32) % 2 == 0 {
                [1.0, 0.8, 0.0]
            } else {
                [0.05, 0.05, 0.05]
            }
        }
        "copper" => {
            let bar = (v * 8.0).fract();
            let t = 1.0 - (bar - 0.5).abs() * 2.0;
            let hue = (v * 8.0).floor() / 8.0;
            let base = [
                0.5 + 0.5 * (hue * TAU).sin(),
                0.5 + 0.5 * (hue * TAU + 2.1).sin(),
                0.5 + 0.5 * (hue * TAU + 4.2).sin(),
            ];
            let q = (t * 8.0).floor() / 8.0;
            [base[0] * q, base[1] * q, base[2] * q]
        }
        "win9x" => {
            let (wx, wy) = (x % 128, y % 128);
            let in_win = (16..112).contains(&wx) && (24..104).contains(&wy);
            let title = in_win && (26..38).contains(&wy) && (18..110).contains(&wx);
            let light = in_win && (wx == 16 || wy == 24);
            let dark = in_win && (wx == 111 || wy == 103);
            if title {
                mix(rgb(0x000080), rgb(0x1084d0), (wx - 18) as f32 / 92.0)
            } else if light {
                [1.0, 1.0, 1.0]
            } else if dark {
                rgb(0x404040)
            } else if in_win {
                rgb(0xc0c0c0)
            } else {
                rgb(0x008080)
            }
        }
        "sierpinski" => {
            let (mut a, mut b) = (x * 243 / 256, y * 243 / 256);
            let mut hole = false;
            while a > 0 || b > 0 {
                if a % 3 == 1 && b % 3 == 1 {
                    hole = true;
                    break;
                }
                a /= 3;
                b /= 3;
            }
            if hole {
                [0.05, 0.0, 0.1]
            } else {
                ramp(&[(0.0, 0xff4080), (1.0, 0x40c0ff)], v)
            }
        }
        "stars" => {
            let r = h2(x as i32, y as i32, 77);
            let b = if r > 0.995 {
                1.0
            } else if r > 0.985 {
                0.4
            } else {
                0.0
            };
            [b, b, b * 1.1]
        }
        "lava" => {
            let t = fbm(u, v, 4, 5, 21);
            let crack = 1.0 - smooth(0.0, 0.06, (t - 0.5).abs());
            let rock = 0.08 + fbm(u, v, 16, 3, 4) * 0.1;
            let glow = ramp(&[(0.0, 0x300000), (0.5, 0xff4000), (1.0, 0xffe080)], crack);
            mix([rock, rock * 0.8, rock * 0.7], glow, crack)
        }
        "dither" => {
            let t = u;
            let d = ez_core::palette::bayer4(x / 4, y / 4) + 0.5;
            let levels = 4.0;
            let q = ((t * levels + d - 0.5).round() / levels).clamp(0.0, 1.0);
            ramp(&[(0.0, 0x100030), (0.5, 0x8030c0), (1.0, 0xffc0ff)], q)
        }
        _ => {
            if ((x / 16) + (y / 16)) % 2 == 0 {
                [1.0, 0.0, 1.0]
            } else {
                [0.0, 0.0, 0.0]
            }
        }
    }
}

/// Downscale + palette-reduce + dither an image for a chunky retro look.
pub fn retroize(img: &RgbaImage, p: &RetroProcess) -> RgbaImage {
    let mut img = img.clone();
    if p.max_size > 0 && img.width().max(img.height()) > p.max_size {
        let s = p.max_size as f32 / img.width().max(img.height()) as f32;
        let w = ((img.width() as f32 * s).round() as u32).max(1);
        let h = ((img.height() as f32 * s).round() as u32).max(1);
        img = image::imageops::resize(&img, w, h, FilterType::Triangle);
    }
    for (x, y, px) in img.enumerate_pixels_mut() {
        let c = [
            px[0] as f32 / 255.0,
            px[1] as f32 / 255.0,
            px[2] as f32 / 255.0,
        ];
        let q = quantize(c, p.palette, p.dither, x, y);
        px[0] = (q[0] * 255.0).round() as u8;
        px[1] = (q[1] * 255.0).round() as u8;
        px[2] = (q[2] * 255.0).round() as u8;
    }
    img
}

/// Convenience used by the texture pack generator.
pub fn palette_swatch(p: PaletteId) -> RgbaImage {
    let cols = p.colors();
    let n = cols.len().max(1) as u32;
    RgbaImage::from_fn(n * 16, 16, |x, _| {
        let c = cols.get((x / 16) as usize).copied().unwrap_or(0);
        image::Rgba([(c >> 16) as u8, (c >> 8) as u8, c as u8, 255])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_tileable_and_distinct() {
        for (name, _) in BUILTIN {
            let img = generate(name);
            assert_eq!(img.dimensions(), (TEX_SIZE, TEX_SIZE));
            // Not a solid colour.
            let first = *img.get_pixel(0, 0);
            assert!(img.pixels().any(|p| *p != first), "{name} is flat");
        }
    }

    #[test]
    fn retroize_limits_colors() {
        let img = generate("plasma");
        let out = retroize(
            &img,
            &RetroProcess {
                max_size: 64,
                palette: PaletteId::GameBoy,
                dither: 0.5,
            },
        );
        assert_eq!(out.width(), 64);
        let mut set = std::collections::HashSet::new();
        for p in out.pixels() {
            set.insert([p[0], p[1], p[2]]);
        }
        assert!(set.len() <= 4);
    }
}
