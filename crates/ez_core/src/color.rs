//! Color helpers. Colors in the model are stored as *linear* RGB triples.

pub type Rgb = [f32; 3];

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// `0xRRGGBB` (sRGB) to linear RGB.
pub fn hex(rgb: u32) -> Rgb {
    let r = ((rgb >> 16) & 0xff) as f32 / 255.0;
    let g = ((rgb >> 8) & 0xff) as f32 / 255.0;
    let b = (rgb & 0xff) as f32 / 255.0;
    [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b)]
}

/// Linear RGB to `0xRRGGBB` (sRGB).
pub fn to_hex(c: Rgb) -> u32 {
    let q = |v: f32| (linear_to_srgb(v).clamp(0.0, 1.0) * 255.0).round() as u32;
    (q(c[0]) << 16) | (q(c[1]) << 8) | q(c[2])
}

/// Rotate the hue of a linear color by `turns` (1.0 = full circle): a
/// rotation around the grey axis, so a full turn is exactly the identity.
pub fn hue_rotate(c: Rgb, turns: f32) -> Rgb {
    let a = turns * std::f32::consts::TAU;
    let (s, co) = a.sin_cos();
    let k = 1.0 / 3.0;
    let sq = (1.0f32 / 3.0).sqrt();
    let m0 = co + (1.0 - co) * k;
    let m1 = k * (1.0 - co) - sq * s;
    let m2 = k * (1.0 - co) + sq * s;
    [
        (c[0] * m0 + c[1] * m1 + c[2] * m2).max(0.0),
        (c[0] * m2 + c[1] * m0 + c[2] * m1).max(0.0),
        (c[0] * m1 + c[1] * m2 + c[2] * m0).max(0.0),
    ]
}

pub fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

pub fn add(a: Rgb, b: Rgb) -> Rgb {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub fn scale(c: Rgb, s: f32) -> Rgb {
    [c[0] * s, c[1] * s, c[2] * s]
}

// ---------------------------------------------------------------------------
// OKLab: a perceptual colour space (Björn Ottosson). Equal steps look
// equal; lightness does not change when the hue does.

/// Linear RGB to OKLab (L, a, b).
pub fn to_oklab(c: Rgb) -> [f32; 3] {
    let [r, g, b] = c;
    let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let (l, m, s) = (l.max(0.0).cbrt(), m.max(0.0).cbrt(), s.max(0.0).cbrt());
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// OKLab to linear RGB (may fall outside 0..1).
pub fn from_oklab(lab: [f32; 3]) -> Rgb {
    let [l, a, b] = lab;
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    [
        4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3,
        -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3,
        -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
    ]
}

/// Lightness, chroma and hue (radians) of a linear colour.
pub fn to_oklch(c: Rgb) -> [f32; 3] {
    let [l, a, b] = to_oklab(c);
    [l, a.hypot(b), b.atan2(a)]
}

fn from_oklch(l: f32, c: f32, h: f32) -> Rgb {
    from_oklab([l, c * h.cos(), c * h.sin()])
}

/// The colour at lightness `l`, hue `h`, with as much of chroma `c` as
/// fits in the displayable range.
fn in_gamut(l: f32, c: f32, h: f32) -> Rgb {
    let fits = |rgb: Rgb| rgb.iter().all(|v| (-1e-4..=1.0 + 1e-4).contains(v));
    let full = from_oklch(l, c, h);
    if fits(full) {
        return full.map(|v| v.clamp(0.0, 1.0));
    }
    let (mut lo, mut hi) = (0.0f32, c);
    for _ in 0..16 {
        let mid = (lo + hi) * 0.5;
        if fits(from_oklch(l, mid, h)) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    from_oklch(l, lo, h).map(|v| v.clamp(0.0, 1.0))
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The shortest signed turn from angle `a` to angle `b` (radians).
fn angle_to(a: f32, b: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (b - a + PI).rem_euclid(TAU) - PI
}

/// Brings colours into harmony with a key colour: each keeps its
/// lightness while its hue is pulled toward the nearest of the scheme's
/// hues and, optionally, its chroma toward the key's. Greys stay grey
/// unless tinted (see [`Harmoniser::with_grey_tint`]).
#[derive(Clone, Debug)]
pub struct Harmoniser {
    /// Scheme hues before the turn (radians).
    pub hues: Vec<f32>,
    /// The turn added to every hue (radians). Each colour picks its scheme
    /// hue before the turn, so a turning scheme turns every colour
    /// smoothly all the way round instead of jumping between hues.
    pub turn: f32,
    /// The key colour's lightness and chroma.
    pub key_lightness: f32,
    pub key_chroma: f32,
    /// 0 = colours unchanged, 1 = hues on the scheme.
    pub pull: f32,
    /// 0 = colours keep their chroma, 1 = all take the key's.
    pub chroma_match: f32,
    /// 0 = greys stay grey, 1 = they take the key's hue at its chroma.
    pub grey_tint: f32,
}

/// Chroma below which a colour counts as grey and is left alone.
const GREY: [f32; 2] = [0.015, 0.05];

impl Harmoniser {
    /// A scheme from a key colour turned by `turn` (radians) and the hue
    /// `offsets` (radians) of a harmony rule.
    pub fn new(key: Rgb, turn: f32, offsets: &[f32], pull: f32, chroma_match: f32) -> Self {
        let [l, c, h] = to_oklch(key);
        Harmoniser {
            hues: offsets.iter().map(|o| h + o).collect(),
            turn,
            key_lightness: l,
            key_chroma: c,
            pull: pull.clamp(0.0, 1.0),
            chroma_match: chroma_match.clamp(0.0, 1.0),
            grey_tint: 0.0,
        }
    }

    /// Let greys (and whites, and a new shape's default material) take the
    /// key's hue too: 0 = they stay grey, 1 = as colourful as the key.
    pub fn with_grey_tint(mut self, tint: f32) -> Self {
        self.grey_tint = tint.clamp(0.0, 1.0);
        self
    }

    /// `c` brought into the scheme. Colours brighter than 1 (glows) keep
    /// their brightness.
    pub fn apply(&self, c: Rgb) -> Rgb {
        let c = c.map(|v| v.max(0.0));
        let peak = c[0].max(c[1]).max(c[2]);
        if peak <= 1e-6 {
            return c;
        }
        let over = peak.max(1.0);
        let unit = c.map(|v| v / over);
        let [l, chroma, h] = to_oklch(unit);
        let colourful = smoothstep(GREY[0], GREY[1], chroma);
        // Greys take the key's hue (turned with the scheme) when tinted.
        let grey = if self.grey_tint > 0.0 && colourful < 1.0 {
            let hue = self.hues.first().copied().unwrap_or(0.0) + self.turn;
            in_gamut(l, self.key_chroma * self.grey_tint, hue).map(|v| v * over)
        } else {
            c
        };
        if colourful <= 0.0 {
            return grey;
        }
        let nearest = self
            .hues
            .iter()
            .map(|t| angle_to(h, *t))
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
            .unwrap_or(0.0);
        let h2 = h + (nearest * self.pull + self.turn) * colourful;
        let c2 = chroma + (self.key_chroma - chroma) * self.chroma_match * colourful;
        let coloured = in_gamut(l, c2, h2).map(|v| v * over);
        if colourful >= 1.0 {
            return coloured;
        }
        // Between grey and colourful: blend, so nothing jumps.
        std::array::from_fn(|i| grey[i] + (coloured[i] - grey[i]) * colourful)
    }

    /// The scheme's colours at the key's lightness and chroma (for
    /// swatches).
    pub fn swatches(&self) -> Vec<Rgb> {
        self.hues
            .iter()
            .map(|h| in_gamut(self.key_lightness, self.key_chroma.max(0.05), h + self.turn))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Rgb, b: Rgb, eps: f32) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < eps)
    }

    #[test]
    fn grey_tint_colours_greys_only_when_asked() {
        let key = hex(0x2060ff);
        let offsets = [0.0];
        let grey = hex(0xb0b0b8);
        let plain = Harmoniser::new(key, 0.0, &offsets, 1.0, 0.0);
        assert!(close(plain.apply(grey), grey, 1e-6));
        let tinted = plain.clone().with_grey_tint(1.0);
        let [_, c, h] = to_oklch(tinted.apply(grey));
        let [_, kc, kh] = to_oklch(key);
        assert!(c > kc * 0.5, "chroma {c}");
        assert!(angle_to(h, kh).abs() < 0.15, "hue {h} vs {kh}");
        // Black stays black.
        let b = tinted.apply([0.0; 3]);
        assert!(b.iter().all(|v| *v < 1e-4));
        // Lightness is kept.
        assert!((to_oklch(tinted.apply(grey))[0] - to_oklch(grey)[0]).abs() < 0.02);
    }

    #[test]
    fn oklab_round_trips() {
        for v in [0x000000, 0xffffff, 0xff2020, 0x123456, 0xd4a017, 0x20ff80] {
            let c = hex(v);
            assert!(close(from_oklab(to_oklab(c)), c, 1e-4), "{v:06x}");
        }
        // White is lightness 1, no chroma.
        let w = to_oklch([1.0; 3]);
        assert!((w[0] - 1.0).abs() < 1e-3 && w[1] < 1e-3);
    }

    #[test]
    fn harmonising_keeps_lightness_and_greys() {
        use std::f32::consts::PI;
        let key = hex(0x2060ff);
        let comp = [0.0, PI];
        let h = Harmoniser::new(key, 0.0, &comp, 1.0, 0.0);
        for v in [0xff2020, 0x20ff80, 0xffd23c, 0xa020ff] {
            let c = hex(v);
            let out = h.apply(c);
            let (a, b) = (to_oklch(c), to_oklch(out));
            assert!(
                (a[0] - b[0]).abs() < 2e-3,
                "{v:06x} lightness {} -> {}",
                a[0],
                b[0]
            );
            // On one of the scheme's hues (when it kept some chroma).
            if b[1] > 0.03 {
                let off = h
                    .hues
                    .iter()
                    .map(|t| angle_to(b[2], *t).abs())
                    .fold(f32::MAX, f32::min);
                assert!(off < 0.02, "{v:06x} hue off by {off}");
            }
        }
        for grey in [[0.0; 3], [0.2; 3], [1.0; 3], [3.0; 3]] {
            assert!(close(h.apply(grey), grey, 1e-5));
        }
        // No pull, no change.
        let none = Harmoniser::new(key, 0.0, &comp, 0.0, 0.0);
        for v in [0xff2020, 0x20ff80] {
            assert!(close(none.apply(hex(v)), hex(v), 2e-3));
        }
        // A full turn of the key is no turn.
        let full = Harmoniser::new(key, std::f32::consts::TAU, &comp, 1.0, 0.0);
        let c = hex(0xff8020);
        assert!(close(full.apply(c), h.apply(c), 1e-4));
        // Glows keep their brightness: the same lightness at the same scale.
        let glow = h.apply([3.0, 0.3, 0.1]);
        let (a, b) = (
            to_oklch([1.0, 0.1, 0.1 / 3.0]),
            to_oklch(glow.map(|v| v / 3.0)),
        );
        assert!((a[0] - b[0]).abs() < 2e-3, "{} vs {}", a[0], b[0]);
    }

    #[test]
    fn hex_roundtrip() {
        for v in [0x000000, 0xffffff, 0xff2020, 0x123456, 0xd4a017] {
            assert_eq!(to_hex(hex(v)), v);
        }
    }

    /// A turning scheme turns colours smoothly: no jumps between hues.
    #[test]
    fn turning_scheme_is_smooth() {
        let offsets: Vec<f32> = [0.0f32, 120.0, 240.0]
            .iter()
            .map(|d| d.to_radians())
            .collect();
        for v in [0xff2020, 0x20ff80, 0xa020ff] {
            let c = hex(v);
            let mut last: Option<[f32; 3]> = None;
            for step in 0..=72 {
                let turn = (step as f32 * 5.0).to_radians();
                let out =
                    to_oklab(Harmoniser::new(hex(0x2060ff), turn, &offsets, 1.0, 0.0).apply(c));
                if let Some(p) = last {
                    let d = ((out[1] - p[1]).powi(2) + (out[2] - p[2]).powi(2)).sqrt();
                    assert!(d < 0.05, "{v:06x} jumped by {d} at {step}");
                }
                last = Some(out);
            }
        }
    }

    #[test]
    fn hue_full_turn_is_identity() {
        let c = hex(0xff4020);
        let r = hue_rotate(c, 1.0);
        assert!(close(c, r, 1e-4));
    }
}
