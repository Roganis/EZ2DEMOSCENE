//! Colour reduction for GIF frames: a palette of up to 256 colours made
//! from the frame's own colours (median cut), and every pixel mapped to its
//! nearest palette colour, optionally with ordered (Bayer) dithering.

/// Bits per channel of the colour histogram.
const HIST_BITS: u32 = 5;
/// Bits per channel of the nearest-colour cache.
const MAP_BITS: u32 = 6;
/// k-means passes that polish the median-cut palette.
const REFINE_PASSES: usize = 2;

#[derive(Clone, Copy, Default)]
struct Bin {
    count: u32,
    sum: [u64; 3],
}

/// A colour of the histogram: its mean and how many pixels have it.
#[derive(Clone, Copy)]
struct Entry {
    rgb: [f32; 3],
    count: u32,
}

/// Reusable buffers, so frames don't allocate a megabyte each.
pub struct Quantizer {
    hist: Vec<Bin>,
    cache: Vec<u16>,
}

impl Default for Quantizer {
    fn default() -> Self {
        Quantizer {
            hist: vec![Bin::default(); 1 << (3 * HIST_BITS)],
            cache: vec![u16::MAX; 1 << (3 * MAP_BITS)],
        }
    }
}

impl Quantizer {
    /// The palette (`r, g, b, …`, at most `colors` entries) and one palette
    /// index per pixel of `rgba` (tightly packed, `width` pixels a row).
    /// `bayer_scale` turns on ordered dithering: 1 is strong, 3 gentle.
    pub fn quantize(
        &mut self,
        rgba: &[u8],
        width: u32,
        colors: usize,
        bayer_scale: Option<u32>,
    ) -> (Vec<u8>, Vec<u8>) {
        let colors = colors.clamp(2, 256);
        let palette = self.palette(rgba, colors);
        self.cache.fill(u16::MAX);
        let width = width.max(1) as usize;
        let mut indices = Vec::with_capacity(rgba.len() / 4);
        for (i, px) in rgba.as_chunks::<4>().0.iter().enumerate() {
            let mut c = [px[0], px[1], px[2]];
            if let Some(scale) = bayer_scale {
                let (x, y) = (i % width, i / width);
                let d = (BAYER[(y & 7) * 8 + (x & 7)] >> scale) as i32 - ((1 << (5 - scale)) - 1);
                c = c.map(|v| (v as i32 + d).clamp(0, 255) as u8);
            }
            let key = c.iter().fold(0usize, |k, &v| {
                (k << MAP_BITS) | (v >> (8 - MAP_BITS)) as usize
            });
            if self.cache[key] == u16::MAX {
                self.cache[key] = nearest(&palette, c) as u16;
            }
            indices.push(self.cache[key] as u8);
        }
        let rgb = palette
            .iter()
            .flat_map(|c| c.map(|v| v.round() as u8))
            .collect();
        (rgb, indices)
    }

    /// Median cut: start with one box holding every colour and keep
    /// splitting the box with the most spread-out pixels, at the median of
    /// its widest channel; each box's mean colour is a palette colour.
    fn palette(&mut self, rgba: &[u8], colors: usize) -> Vec<[f32; 3]> {
        self.hist.fill(Bin::default());
        for px in rgba.as_chunks::<4>().0 {
            let key = px[..3].iter().fold(0usize, |k, &v| {
                (k << HIST_BITS) | (v >> (8 - HIST_BITS)) as usize
            });
            let bin = &mut self.hist[key];
            bin.count += 1;
            for (s, &v) in bin.sum.iter_mut().zip(&px[..3]) {
                *s += v as u64;
            }
        }
        let mut entries: Vec<Entry> = self
            .hist
            .iter()
            .filter(|b| b.count > 0)
            .map(|b| Entry {
                rgb: b.sum.map(|s| s as f32 / b.count as f32),
                count: b.count,
            })
            .collect();
        if entries.is_empty() {
            return vec![[0.0; 3]];
        }
        // Boxes are ranges of `entries`.
        let mut boxes = vec![(0, entries.len())];
        while boxes.len() < colors {
            let best = boxes
                .iter()
                .enumerate()
                .filter(|(_, &(a, b))| b - a > 1)
                .map(|(i, &(a, b))| {
                    let (axis, extent) = widest(&entries[a..b]);
                    let count: u64 = entries[a..b].iter().map(|e| e.count as u64).sum();
                    (i, axis, extent as f64 * count as f64)
                })
                .max_by(|x, y| x.2.total_cmp(&y.2));
            let Some((i, axis, score)) = best else { break };
            if score <= 0.0 {
                break;
            }
            let (a, b) = boxes[i];
            let span = &mut entries[a..b];
            span.sort_unstable_by(|x, y| x.rgb[axis].total_cmp(&y.rgb[axis]));
            let total: u64 = span.iter().map(|e| e.count as u64).sum();
            let mut seen = 0;
            let mut cut = 1;
            for (k, e) in span.iter().enumerate() {
                seen += e.count as u64;
                if seen * 2 >= total {
                    cut = k + 1;
                    break;
                }
            }
            let cut = cut.clamp(1, span.len() - 1);
            boxes[i] = (a, a + cut);
            boxes.push((a + cut, b));
        }
        let mut palette: Vec<[f32; 3]> = boxes.iter().map(|&(a, b)| mean(&entries[a..b])).collect();
        // Refine (k-means): every colour goes to its nearest palette colour,
        // which moves to the mean of its colours.
        for _ in 0..REFINE_PASSES {
            let owner: Vec<usize> = entries.iter().map(|e| nearest_f(&palette, e.rgb)).collect();
            let mut sums = vec![([0.0f64; 3], 0.0f64); palette.len()];
            for (e, &o) in entries.iter().zip(&owner) {
                let (sum, n) = &mut sums[o];
                for (s, v) in sum.iter_mut().zip(e.rgb) {
                    *s += v as f64 * e.count as f64;
                }
                *n += e.count as f64;
            }
            for (p, (sum, n)) in palette.iter_mut().zip(sums) {
                if n > 0.0 {
                    *p = sum.map(|s| (s / n) as f32);
                }
            }
        }
        palette
    }
}

/// The channel the colours spread most along, and how far.
fn widest(entries: &[Entry]) -> (usize, f32) {
    (0..3)
        .map(|c| {
            let (lo, hi) = entries.iter().fold((f32::MAX, f32::MIN), |(lo, hi), e| {
                (lo.min(e.rgb[c]), hi.max(e.rgb[c]))
            });
            (c, hi - lo)
        })
        .max_by(|x, y| x.1.total_cmp(&y.1))
        .unwrap()
}

/// The weighted mean colour of `entries`.
fn mean(entries: &[Entry]) -> [f32; 3] {
    let mut sum = [0.0f64; 3];
    let mut n = 0.0f64;
    for e in entries {
        for (s, v) in sum.iter_mut().zip(e.rgb) {
            *s += v as f64 * e.count as f64;
        }
        n += e.count as f64;
    }
    sum.map(|s| (s / n) as f32)
}

fn nearest(palette: &[[f32; 3]], c: [u8; 3]) -> usize {
    nearest_f(palette, c.map(|v| v as f32))
}

fn nearest_f(palette: &[[f32; 3]], c: [f32; 3]) -> usize {
    palette
        .iter()
        .map(|p| (0..3).map(|k| (p[k] - c[k]).powi(2)).sum::<f32>())
        .enumerate()
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map_or(0, |(i, _)| i)
}

/// The 8×8 Bayer matrix (0..63).
const BAYER: [u8; 64] = [
    0, 32, 8, 40, 2, 34, 10, 42, //
    48, 16, 56, 24, 50, 18, 58, 26, //
    12, 44, 4, 36, 14, 46, 6, 38, //
    60, 28, 52, 20, 62, 30, 54, 22, //
    3, 35, 11, 43, 1, 33, 9, 41, //
    51, 19, 59, 27, 49, 17, 57, 25, //
    15, 47, 7, 39, 13, 45, 5, 37, //
    63, 31, 55, 23, 61, 29, 53, 21, //
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A horizontal rainbow over a vertical dark-to-light ramp.
    fn picture(w: u32, h: u32) -> Vec<u8> {
        (0..w * h)
            .flat_map(|i| {
                let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
                let hue = |o: f32| ((x + o) * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                [hue(0.0), hue(1.0 / 3.0), hue(2.0 / 3.0)]
                    .map(|c| (c * (0.2 + 0.8 * y) * 255.0) as u8)
                    .into_iter()
                    .chain([255])
            })
            .collect()
    }

    fn error(rgba: &[u8], palette: &[u8], indices: &[u8]) -> f64 {
        let sum: f64 = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(indices)
            .map(|(px, &i)| {
                (0..3)
                    .map(|k| (px[k] as f64 - palette[i as usize * 3 + k] as f64).powi(2))
                    .sum::<f64>()
            })
            .sum();
        (sum / indices.len() as f64).sqrt()
    }

    #[test]
    fn palettes_have_at_most_the_colours_asked() {
        let img = picture(160, 90);
        let mut q = Quantizer::default();
        let mut last = f64::MAX;
        for colors in [16, 32, 64, 128, 256] {
            let (palette, indices) = q.quantize(&img, 160, colors, None);
            assert_eq!(palette.len(), colors * 3);
            assert_eq!(indices.len(), 160 * 90);
            assert!(indices.iter().all(|&i| (i as usize) < colors));
            // More colours, closer to the picture.
            let e = error(&img, &palette, &indices);
            assert!(e < last, "{colors} colours: error {e} not below {last}");
            last = e;
        }
        assert!(last < 12.0, "256 colours: error {last}");
    }

    #[test]
    fn few_colours_are_kept_exactly() {
        let img: Vec<u8> = [[255, 0, 0, 255], [0, 0, 255, 255], [10, 200, 30, 255]]
            .into_iter()
            .cycle()
            .take(300)
            .flatten()
            .collect();
        let (palette, indices) = Quantizer::default().quantize(&img, 10, 256, Some(3));
        assert_eq!(palette.len(), 9);
        assert_eq!(error(&img, &palette, &indices), 0.0);
    }

    #[test]
    fn dithering_mixes_neighbours() {
        // Black, white and a strip of dark grey between them: with two
        // colours the grey becomes one of them, unless dithered into a mix.
        let mut img = [0, 0, 0, 255].repeat(2048);
        img.extend([255, 255, 255, 255].repeat(2048));
        img.extend([120, 120, 120, 255].repeat(128));
        let mut q = Quantizer::default();
        let distinct = |idx: &[u8]| {
            let mut v = idx[4096..].to_vec();
            v.sort();
            v.dedup();
            v.len()
        };
        let (_, plain) = q.quantize(&img, 64, 2, None);
        let (_, dithered) = q.quantize(&img, 64, 2, Some(1));
        assert_eq!(distinct(&plain), 1);
        assert_eq!(distinct(&dithered), 2);
    }
}
