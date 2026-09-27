//! Logos: a text or an image baked once into a signed distance field with
//! colour, for the flat logo layer.
//!
//! The bake is a float RGBA image: linear colour in rgb (spread outward
//! from the shape so edges never pick up the background) and the distance
//! field in alpha, with the text atlas's convention: 0.5 on the outline,
//! rising inside and falling outside by 0.5 per `spread` pixels. It is not
//! clamped, so effects can reach far from the shape. The shape is found at
//! a few times the output resolution (an exact Euclidean distance
//! transform of the inside and the outside) and averaged down, which puts
//! the outline between pixels where it belongs.

use ez_core::scene::{LogoMask, TextFont};
use glam::Vec2;
use image::RgbaImage;

/// Pixels of the shape itself (padding not counted) aimed for.
const CONTENT_PIXELS: f32 = 150_000.0;
/// Longest side of the baked image, padding included.
const MAX_SIDE: u32 = 2048;
/// Pixels worked on before averaging down.
const WORK_PIXELS: u32 = 1_500_000;
/// Distance field spread as a fraction of the shape's shorter side.
const SPREAD: f32 = 0.2;

/// A baked logo.
#[derive(Clone, Debug)]
pub struct LogoBake {
    pub width: u32,
    pub height: u32,
    /// Row-major from the top: linear rgb, distance field.
    pub pixels: Vec<[f32; 4]>,
    /// Width over height of the shape (without the padding).
    pub aspect: f32,
    /// Padding on each side as a fraction of the shape's width and height.
    pub pad: [f32; 2],
    /// Pixels from the outline to a field value of 0 or 1.
    pub spread: f32,
}

impl LogoBake {
    /// Field value at a pixel.
    pub fn field(&self, x: u32, y: u32) -> f32 {
        self.pixels[(y * self.width + x) as usize][3]
    }
}

/// Sizes of a bake: the shape in output pixels, the padding, the work scale.
struct Frame {
    /// Shape size in output pixels.
    cw: u32,
    ch: u32,
    pad: u32,
    spread: f32,
    /// Work pixels per output pixel.
    k: u32,
}

impl Frame {
    /// Output pixels per shape unit for a shape `w` × `h` units, with at
    /// most `max_scale` output pixels per unit.
    fn new(w: f32, h: f32, max_scale: f32) -> (Frame, f32) {
        let (w, h) = (w.max(1e-6), h.max(1e-6));
        let mut s = (CONTENT_PIXELS / (w * h)).sqrt().min(max_scale);
        // Room for the padding within the largest texture.
        let fit = |s: f32| {
            let short = (w.min(h) * s).max(1.0);
            let pad = (short * SPREAD * 1.25).ceil();
            (w.max(h) * s + 2.0 * pad) as u32
        };
        while fit(s) > MAX_SIDE && s > 1e-3 {
            s *= 0.9;
        }
        let cw = ((w * s).round() as u32).max(1);
        let ch = ((h * s).round() as u32).max(1);
        let spread = (cw.min(ch) as f32 * SPREAD).max(2.0);
        let pad = (spread * 1.25).ceil() as u32;
        let (ow, oh) = (cw + 2 * pad, ch + 2 * pad);
        let k = ((WORK_PIXELS as f32 / (ow * oh) as f32).sqrt() as u32).clamp(2, 4);
        (
            Frame {
                cw,
                ch,
                pad,
                spread,
                k,
            },
            s,
        )
    }

    fn out_size(&self) -> (u32, u32) {
        (self.cw + 2 * self.pad, self.ch + 2 * self.pad)
    }

    fn work_size(&self) -> (u32, u32) {
        let (w, h) = self.out_size();
        (w * self.k, h * self.k)
    }
}

/// Bake a text logo (`bytes` overrides the built-in font). `None` when
/// there is nothing to draw.
pub fn bake_text(text: &str, font: TextFont, bytes: Option<&[u8]>) -> Option<LogoBake> {
    let contours = crate::text::logo_contours(text, font, bytes)?;
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for p in contours.iter().flatten() {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let size = hi - lo;
    if !(size.x > 0.0 && size.y > 0.0) {
        return None;
    }
    let (frame, s) = Frame::new(size.x, size.y, f32::MAX);
    let (ww, wh) = frame.work_size();
    // Ems to work pixels (y down).
    let ks = s * frame.k as f32;
    let off = (frame.pad * frame.k) as f32;
    let to_px = |p: Vec2| Vec2::new((p.x - lo.x) * ks + off, (hi.y - p.y) * ks + off);
    let polys: Vec<Vec<Vec2>> = contours
        .iter()
        .map(|c| c.iter().map(|p| to_px(*p)).collect())
        .collect();
    let inside = fill_nonzero(&polys, ww, wh);
    Some(finish(&frame, &inside, None))
}

/// Bake an image logo; `mask` picks which parts are the logo. The logo is
/// cropped to those parts. `None` when no part is.
pub fn bake_image(img: &RgbaImage, mask: LogoMask) -> Option<LogoBake> {
    let (sw, sh) = img.dimensions();
    if sw == 0 || sh == 0 {
        return None;
    }
    let src: Vec<[f32; 4]> = img
        .pixels()
        .map(|p| {
            let c = p.0;
            [
                srgb_to_linear(c[0] as f32 / 255.0),
                srgb_to_linear(c[1] as f32 / 255.0),
                srgb_to_linear(c[2] as f32 / 255.0),
                c[3] as f32 / 255.0,
            ]
        })
        .collect();
    let cover = |c: [f32; 4]| -> f32 {
        let luma = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        // Brightness judged as it looks (gamma), so mid-grey is half.
        let luma = luma.max(0.0).powf(1.0 / 2.2);
        match mask {
            LogoMask::Alpha => c[3],
            LogoMask::Bright => luma * c[3],
            LogoMask::Dark => (1.0 - luma) * c[3],
        }
    };
    let cov: Vec<f32> = src.iter().map(|c| cover(*c)).collect();
    // Crop to the logo (a pixel of margin keeps its edges soft).
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..sh {
        for x in 0..sw {
            if cov[(y * sw + x) as usize] >= 0.5 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let (bw, bh) = ((x1 - x0) as f32, (y1 - y0) as f32);
    // Up to twice the image's own resolution (smoother edges when small).
    let (frame, s) = Frame::new(bw, bh, 2.0);
    let (ww, wh) = frame.work_size();
    let ks = s * frame.k as f32;
    let off = (frame.pad * frame.k) as f32;
    // Bilinear sample of the image (premultiplied colour, coverage) at a
    // work pixel; outside the image is empty.
    let sample = |px: f32, py: f32| -> ([f32; 3], f32, f32) {
        let u = x0 as f32 + (px + 0.5 - off) / ks - 0.5;
        let v = y0 as f32 + (py + 0.5 - off) / ks - 0.5;
        let (fx, fy) = (u.floor(), v.floor());
        let (tx, ty) = (u - fx, v - fy);
        let mut rgb = [0.0f32; 3];
        let (mut a, mut c) = (0.0f32, 0.0f32);
        for (dx, dy, w) in [
            (0, 0, (1.0 - tx) * (1.0 - ty)),
            (1, 0, tx * (1.0 - ty)),
            (0, 1, (1.0 - tx) * ty),
            (1, 1, tx * ty),
        ] {
            let (x, y) = (fx as i64 + dx, fy as i64 + dy);
            if x < 0 || y < 0 || x >= sw as i64 || y >= sh as i64 {
                continue;
            }
            let i = (y as u32 * sw + x as u32) as usize;
            let p = src[i];
            for j in 0..3 {
                rgb[j] += p[j] * p[3] * w;
            }
            a += p[3] * w;
            c += cov[i] * w;
        }
        (rgb, a, c)
    };
    let n = (ww * wh) as usize;
    let mut inside = vec![false; n];
    let mut color = vec![[0.0f32; 3]; n];
    for y in 0..wh {
        for x in 0..ww {
            let (rgb, a, c) = sample(x as f32, y as f32);
            let i = (y * ww + x) as usize;
            inside[i] = c >= 0.5;
            if a > 1e-4 {
                color[i] = [rgb[0] / a, rgb[1] / a, rgb[2] / a];
            }
        }
    }
    Some(finish(&frame, &inside, Some(&color)))
}

/// Distance field (and colour spread outward) from the inside mask at work
/// resolution, averaged down to the output.
fn finish(frame: &Frame, inside: &[bool], color: Option<&[[f32; 3]]>) -> LogoBake {
    let (ww, wh) = frame.work_size();
    let (ow, oh) = frame.out_size();
    // Squared distances to the nearest inside and outside work pixel.
    let (to_in, nearest_in) = edt(ww, wh, |i| inside[i]);
    let (to_out, _) = edt(ww, wh, |i| !inside[i]);
    let k = frame.k;
    let kk = (k * k) as f32;
    let mut pixels = vec![[0.0f32; 4]; (ow * oh) as usize];
    for oy in 0..oh {
        for ox in 0..ow {
            let mut d = 0.0f32;
            let mut rgb = [0.0f32; 3];
            for sy in 0..k {
                for sx in 0..k {
                    let i = ((oy * k + sy) * ww + ox * k + sx) as usize;
                    // Distance to the boundary between pixel centres.
                    d += if inside[i] {
                        to_out[i].sqrt() - 0.5
                    } else {
                        0.5 - to_in[i].sqrt()
                    };
                    if let Some(c) = color {
                        let src = if inside[i] { i } else { nearest_in[i] };
                        let c = c.get(src).copied().unwrap_or([1.0; 3]);
                        for j in 0..3 {
                            rgb[j] += c[j];
                        }
                    }
                }
            }
            let d = d / kk / k as f32;
            let rgb = if color.is_some() {
                [rgb[0] / kk, rgb[1] / kk, rgb[2] / kk]
            } else {
                [1.0; 3]
            };
            pixels[(oy * ow + ox) as usize] =
                [rgb[0], rgb[1], rgb[2], 0.5 + d / (2.0 * frame.spread)];
        }
    }
    LogoBake {
        width: ow,
        height: oh,
        pixels,
        aspect: frame.cw as f32 / frame.ch as f32,
        pad: [
            frame.pad as f32 / frame.cw as f32,
            frame.pad as f32 / frame.ch as f32,
        ],
        spread: frame.spread,
    }
}

/// Pixel centres inside the polygons by the non-zero winding rule.
fn fill_nonzero(polys: &[Vec<Vec2>], w: u32, h: u32) -> Vec<bool> {
    let mut out = vec![false; (w * h) as usize];
    let mut edges: Vec<(Vec2, Vec2)> = Vec::new();
    for p in polys {
        for i in 0..p.len() {
            edges.push((p[i], p[(i + 1) % p.len()]));
        }
    }
    let mut hits: Vec<(f32, i32)> = Vec::new();
    for y in 0..h {
        let cy = y as f32 + 0.5;
        hits.clear();
        for (a, b) in &edges {
            if (a.y <= cy) != (b.y <= cy) {
                let x = a.x + (cy - a.y) * (b.x - a.x) / (b.y - a.y);
                hits.push((x, if b.y > a.y { 1 } else { -1 }));
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut wind = 0;
        for pair in hits.windows(2) {
            wind += pair[0].1;
            if wind != 0 {
                // Pixel centres between the two crossings.
                let x0 = (pair[0].0 - 0.5).ceil().max(0.0) as u32;
                let x1 = (pair[1].0 - 0.5).ceil().clamp(0.0, w as f32) as u32;
                for x in x0..x1 {
                    out[(y * w + x) as usize] = true;
                }
            }
        }
    }
    out
}

/// Exact squared Euclidean distance from every pixel to the nearest pixel
/// where `feature` holds, and that pixel's index (Felzenszwalb &
/// Huttenlocher: lower envelopes of parabolas, columns then rows). The
/// columns are worked on transposed, so both passes read memory in order.
fn edt(w: u32, h: u32, feature: impl Fn(usize) -> bool) -> (Vec<f32>, Vec<usize>) {
    const FAR: f32 = 1e20;
    let (w, h) = (w as usize, h as usize);
    let f: Vec<f32> = (0..w * h)
        .map(|i| if feature(i) { 0.0 } else { FAR })
        .collect();
    // Columns as rows (h long each).
    let (dt, at) = rows_1d(&transpose(&f, w, h), h);
    let col_d = transpose(&dt, h, w);
    let col_arg = transpose(&at, h, w);
    let (out_d, mut out_arg) = rows_1d(&col_d, w);
    for (i, a) in out_arg.iter_mut().enumerate() {
        let cx = *a;
        *a = col_arg[i - i % w + cx] * w + cx;
    }
    (out_d, out_arg)
}

/// `edt_1d` on every row of `len` values, on several threads where there
/// are threads.
fn rows_1d(src: &[f32], len: usize) -> (Vec<f32>, Vec<usize>) {
    let mut d = vec![0.0f32; src.len()];
    let mut arg = vec![0usize; src.len()];
    let work = |src: &[f32], d: &mut [f32], arg: &mut [usize]| {
        let mut v = vec![0usize; len];
        let mut z = vec![0.0f32; len + 1];
        for ((s, d), a) in src
            .chunks(len)
            .zip(d.chunks_mut(len))
            .zip(arg.chunks_mut(len))
        {
            edt_1d(s, d, a, &mut v, &mut z);
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        let rows = src.len() / len.max(1);
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
        if threads > 1 && rows >= threads * 4 {
            let per = rows.div_ceil(threads) * len;
            std::thread::scope(|scope| {
                for ((s, d), a) in src
                    .chunks(per)
                    .zip(d.chunks_mut(per))
                    .zip(arg.chunks_mut(per))
                {
                    scope.spawn(move || work(s, d, a));
                }
            });
            return (d, arg);
        }
    }
    work(src, &mut d, &mut arg);
    (d, arg)
}

/// `src` is `h` rows of `w`; the result is `w` rows of `h`. Done in tiles
/// to stay in the cache.
fn transpose<T: Copy + Default>(src: &[T], w: usize, h: usize) -> Vec<T> {
    const TILE: usize = 32;
    let mut out = vec![T::default(); w * h];
    for y0 in (0..h).step_by(TILE) {
        for x0 in (0..w).step_by(TILE) {
            for y in y0..(y0 + TILE).min(h) {
                for x in x0..(x0 + TILE).min(w) {
                    out[x * h + y] = src[y * w + x];
                }
            }
        }
    }
    out
}

/// One dimension: `d[q] = min_p (q - p)² + f[p]`, with the minimising `p`.
fn edt_1d(f: &[f32], d: &mut [f32], arg: &mut [usize], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        let fq = f[q] + (q * q) as f32;
        // Drop parabolas the new one hides; the intersection is only
        // divided out once it is kept (s <= z[k] as num <= z[k] * den).
        let (num, den) = loop {
            let p = v[k];
            let num = fq - (f[p] + (p * p) as f32);
            let den = 2.0 * (q - p) as f32;
            if k > 0 && num <= z[k] * den {
                k -= 1;
            } else {
                break (num, den);
            }
        };
        k += 1;
        v[k] = q;
        z[k] = num / den;
        z[k + 1] = f32::INFINITY;
    }
    k = 0;
    for q in 0..n {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f32 - p as f32;
        d[q] = dq * dq + f[p];
        arg[q] = p;
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Half-float bits of `x` (rounded to nearest; tiny values flush to zero).
pub fn f16_bits(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let man = b & 0x7f_ffff;
    if x.is_nan() {
        return 0x7e00;
    }
    if exp <= 0 {
        return sign;
    }
    if exp >= 31 {
        return sign | 0x7bff;
    }
    // Round to nearest (a carry into the exponent is still correct).
    let v = ((exp as u32) << 10) | (man >> 13);
    let v = v + ((man >> 12) & 1);
    if v >= 0x7c00 {
        sign | 0x7bff
    } else {
        sign | v as u16
    }
}

/// Mip chain of a float RGBA image (2×2 averages), full size first.
pub fn mips(w: u32, h: u32, px: &[[f32; 4]]) -> Vec<(u32, u32, Vec<[f32; 4]>)> {
    let mut out = vec![(w, h, px.to_vec())];
    loop {
        let (lw, lh, l) = out.last().expect("level");
        if *lw == 1 && *lh == 1 {
            break;
        }
        let (nw, nh) = ((lw / 2).max(1), (lh / 2).max(1));
        let mut next = vec![[0.0f32; 4]; (nw * nh) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let mut sum = [0.0f32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let sx = (x * 2 + dx).min(lw - 1);
                    let sy = (y * 2 + dy).min(lh - 1);
                    let p = l[(sy * lw + sx) as usize];
                    for j in 0..4 {
                        sum[j] += p[j] * 0.25;
                    }
                }
                next[(y * nw + x) as usize] = sum;
            }
        }
        out.push((nw, nh, next));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn edt_matches_brute_force() {
        let (w, h) = (23u32, 17u32);
        let feat: Vec<bool> = (0..w * h)
            .map(|i| (i * 7919 + 13) % 37 == 0 || i == 200)
            .collect();
        let (d, arg) = edt(w, h, |i| feat[i]);
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let mut best = f32::MAX;
                for (j, f) in feat.iter().enumerate() {
                    if *f {
                        let (fx, fy) = ((j as u32 % w) as i64, (j as u32 / w) as i64);
                        best = best.min(((fx - x).pow(2) + (fy - y).pow(2)) as f32);
                    }
                }
                let i = (y * w as i64 + x) as usize;
                assert_eq!(d[i], best, "at {x},{y}");
                let (ax, ay) = ((arg[i] as u32 % w) as i64, (arg[i] as u32 / w) as i64);
                assert!(feat[arg[i]]);
                assert_eq!(((ax - x).pow(2) + (ay - y).pow(2)) as f32, best);
            }
        }
    }

    /// A disc: the field is 0.5 on its rim and grows by 0.5 per spread.
    #[test]
    fn disc_field_is_a_distance() {
        let n = 200u32;
        let r = 60.0f32;
        let img = RgbaImage::from_fn(n, n, |x, y| {
            let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(Vec2::splat(n as f32 / 2.0));
            let a = (r - d + 0.5).clamp(0.0, 1.0);
            Rgba([255, 0, 0, (a * 255.0) as u8])
        });
        let b = bake_image(&img, LogoMask::Alpha).expect("bake");
        assert!((b.aspect - 1.0).abs() < 0.02);
        let (cx, cy) = (b.width as f32 / 2.0, b.height as f32 / 2.0);
        // Output pixels per image pixel.
        let s = (b.width as f32 / (1.0 + 2.0 * b.pad[0])) / (2.0 * r);
        for y in (0..b.height).step_by(7) {
            for x in (0..b.width).step_by(7) {
                let d = Vec2::new(x as f32 + 0.5 - cx, y as f32 + 0.5 - cy).length();
                let want = 0.5 + (r * s - d) / (2.0 * b.spread);
                let got = b.field(x, y);
                assert!(
                    (got - want).abs() < 0.03,
                    "at {x},{y}: {got} vs {want} (spread {})",
                    b.spread
                );
            }
        }
        // Colour is the image's red everywhere, spread outside too.
        let p = b.pixels[0];
        assert!(p[0] > 0.9 && p[1] < 0.05 && p[2] < 0.05, "{p:?}");
    }

    #[test]
    fn masks_pick_parts() {
        // White square on black, no transparency.
        let img = RgbaImage::from_fn(64, 64, |x, y| {
            let on = (16..40).contains(&x) && (20..36).contains(&y);
            if on {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });
        let bright = bake_image(&img, LogoMask::Bright).expect("bright");
        assert!(
            (bright.aspect - 24.0 / 16.0).abs() < 0.05,
            "{}",
            bright.aspect
        );
        let dark = bake_image(&img, LogoMask::Dark).expect("dark");
        assert!((dark.aspect - 1.0).abs() < 0.05);
        let alpha = bake_image(&img, LogoMask::Alpha).expect("alpha");
        assert!((alpha.aspect - 1.0).abs() < 0.05);
        let empty = RgbaImage::new(8, 8);
        assert!(bake_image(&empty, LogoMask::Alpha).is_none());
    }

    #[test]
    fn text_bakes_for_every_font() {
        for font in TextFont::ALL {
            let b = bake_text("EZ2", font, None).expect("bake");
            assert!(b.aspect > 1.5, "{font:?}: {}", b.aspect);
            assert!(b.width <= MAX_SIDE && b.height <= MAX_SIDE);
            let inside = b.pixels.iter().filter(|p| p[3] > 0.5).count();
            let frac = inside as f32 / b.pixels.len() as f32;
            assert!(frac > 0.05 && frac < 0.7, "{font:?}: {frac}");
            // The border is well outside.
            assert!(b.field(0, 0) < 0.0);
        }
        assert!(bake_text("  ", TextFont::Mono, None).is_none());
        // A long line still fits.
        let long = bake_text(&"GREETINGS ".repeat(12), TextFont::Sans, None).expect("long");
        assert!(long.width <= MAX_SIDE);
    }

    #[test]
    fn half_floats() {
        for x in [0.0f32, 1.0, -2.5, 0.5, 0.333, 1000.0, 1e-9, 70000.0] {
            let h = f16_bits(x);
            let exp = ((h >> 10) & 0x1f) as i32;
            let man = (h & 0x3ff) as f32;
            let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
            let back = if exp == 0 {
                0.0
            } else {
                sign * (1.0 + man / 1024.0) * 2f32.powi(exp - 15)
            };
            let want = x.clamp(-65504.0, 65504.0);
            let want = if want.abs() < 6.2e-5 { 0.0 } else { want };
            assert!((back - want).abs() <= want.abs() * 1e-3, "{x}: {back}");
        }
    }
}
