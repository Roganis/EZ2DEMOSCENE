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

pub fn scale(c: Rgb, s: f32) -> Rgb {
    [c[0] * s, c[1] * s, c[2] * s]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        for v in [0x000000, 0xffffff, 0xff2020, 0x123456, 0xd4a017] {
            assert_eq!(to_hex(hex(v)), v);
        }
    }

    #[test]
    fn hue_full_turn_is_identity() {
        let c = hex(0xff4020);
        let r = hue_rotate(c, 1.0);
        for i in 0..3 {
            assert!((c[i] - r[i]).abs() < 1e-4);
        }
    }
}
