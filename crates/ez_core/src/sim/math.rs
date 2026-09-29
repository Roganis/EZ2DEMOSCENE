//! Maths that gives the same bits on every platform.
//!
//! A simulation feeds its own results back in, and chaotic ones (flocks,
//! piles, fluids) blow a one-bit difference up into a different picture.
//! `+ - * /` and `sqrt` are exactly rounded everywhere, but `f32::sin`,
//! `cos` and `exp` call the platform's maths library on desktop, which
//! differs between Linux, Windows, macOS and the browser. Simulations use
//! these instead: range reduction and a polynomial in `f64`, so they are
//! the same wherever they run (and within 1e-7 of the library versions).
//!
//! Also for simulations: `glam::Vec3` (plain scalar code on every target),
//! not `Vec3A` or `Quat` (SIMD on some targets only); rotations are
//! [`Rot`] with the helpers below. Never `mul_add`.

use std::f64::consts::{FRAC_PI_2, LN_2, LOG2_E};

/// Sine of `x` radians (accurate for |x| < 10⁵).
pub fn sin(x: f32) -> f32 {
    sin_cos(x).0
}

/// Cosine of `x` radians (accurate for |x| < 10⁵).
pub fn cos(x: f32) -> f32 {
    sin_cos(x).1
}

/// Sine and cosine of `x` radians.
pub fn sin_cos(x: f32) -> (f32, f32) {
    let x = x as f64;
    // x = k·π/2 + r with |r| <= π/4, π/2 split in two so r stays exact.
    const PIO2_HI: f64 = 1.570_796_326_734_125_6;
    const PIO2_LO: f64 = 6.077_100_506_506_192e-11;
    let k = (x * (1.0 / FRAC_PI_2) + 0.5).floor();
    let r = (x - k * PIO2_HI) - k * PIO2_LO;
    let (s, c) = (sin_poly(r), cos_poly(r));
    let (s, c) = match (k as i64).rem_euclid(4) {
        0 => (s, c),
        1 => (c, -s),
        2 => (-s, -c),
        _ => (-c, s),
    };
    (s as f32, c as f32)
}

// Taylor series to r^15 / r^16: below 1e-16 on |r| <= π/4.
fn sin_poly(r: f64) -> f64 {
    let r2 = r * r;
    let mut p = -1.0 / 1_307_674_368_000.0;
    p = p * r2 + 1.0 / 6_227_020_800.0;
    p = p * r2 - 1.0 / 39_916_800.0;
    p = p * r2 + 1.0 / 362_880.0;
    p = p * r2 - 1.0 / 5_040.0;
    p = p * r2 + 1.0 / 120.0;
    p = p * r2 - 1.0 / 6.0;
    r + r * r2 * p
}

fn cos_poly(r: f64) -> f64 {
    let r2 = r * r;
    let mut p = 1.0 / 20_922_789_888_000.0;
    p = p * r2 - 1.0 / 87_178_291_200.0;
    p = p * r2 + 1.0 / 479_001_600.0;
    p = p * r2 - 1.0 / 3_628_800.0;
    p = p * r2 + 1.0 / 40_320.0;
    p = p * r2 - 1.0 / 720.0;
    p = p * r2 + 1.0 / 24.0;
    p = p * r2 - 0.5;
    1.0 + r2 * p
}

/// e to the power `x` (0 below -103, infinity above 88.7, like `f32::exp`).
pub fn exp(x: f32) -> f32 {
    if x.is_nan() {
        return x;
    }
    let x = (x as f64).clamp(-104.0, 89.0);
    // x = k·ln 2 + r with |r| <= ln 2 / 2.
    const LN2_HI: f64 = 0.693_147_180_369_123_8;
    const LN2_LO: f64 = 1.908_214_929_270_587_7e-10;
    debug_assert!((LN2_HI + LN2_LO - LN_2).abs() < 1e-16);
    let k = (x * LOG2_E + 0.5).floor();
    let r = (x - k * LN2_HI) - k * LN2_LO;
    // Taylor series to r^12: below 1e-16 on |r| <= 0.35.
    let mut p = 1.0 / 479_001_600.0;
    for d in [
        39_916_800.0,
        3_628_800.0,
        362_880.0,
        40_320.0,
        5_040.0,
        720.0,
        120.0,
        24.0,
        6.0,
        2.0,
        1.0,
        1.0,
    ] {
        p = p * r + 1.0 / d;
    }
    // 2^k as bits (k is within the exponent range of f64 here).
    let scale = f64::from_bits(((k as i64 + 1023) as u64) << 52);
    (p * scale) as f32
}

/// A rotation as a unit quaternion `[x, y, z, w]`.
pub type Rot = [f32; 4];

/// No rotation.
pub const ROT_IDENTITY: Rot = [0.0, 0.0, 0.0, 1.0];

/// Rotation by `angle` radians around the unit vector `axis`.
pub fn rot_axis_angle(axis: glam::Vec3, angle: f32) -> Rot {
    let (s, c) = sin_cos(angle * 0.5);
    [axis.x * s, axis.y * s, axis.z * s, c]
}

/// `a * b`: rotate by `b`, then by `a`.
pub fn rot_mul(a: Rot, b: Rot) -> Rot {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

/// `r` scaled back to unit length (identity if it is zero).
pub fn rot_normalize(r: Rot) -> Rot {
    let len = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] + r[3] * r[3]).sqrt();
    if len > 0.0 {
        [r[0] / len, r[1] / len, r[2] / len, r[3] / len]
    } else {
        ROT_IDENTITY
    }
}

/// From `a` (t = 0) to `b` (t = 1) the short way round, normalised lerp.
pub fn rot_nlerp(a: Rot, b: Rot, t: f32) -> Rot {
    let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let s = if d < 0.0 { -1.0 } else { 1.0 };
    rot_normalize([
        a[0] + (b[0] * s - a[0]) * t,
        a[1] + (b[1] * s - a[1]) * t,
        a[2] + (b[2] * s - a[2]) * t,
        a[3] + (b[3] * s - a[3]) * t,
    ])
}

/// Turns `r` by the angular velocity `w` (radians per second, world axes)
/// over `dt` seconds.
pub fn rot_integrate(r: Rot, w: glam::Vec3, dt: f32) -> Rot {
    let angle = w.length() * dt;
    if angle <= 0.0 {
        return r;
    }
    rot_normalize(rot_mul(rot_axis_angle(w / w.length(), angle), r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_to_the_library() {
        let mut worst = [0.0f32; 3];
        for i in -20_000..=20_000 {
            let x = i as f32 * 0.0137;
            worst[0] = worst[0].max((sin(x) - x.sin()).abs());
            worst[1] = worst[1].max((cos(x) - x.cos()).abs());
            let e = i as f32 * 0.0044;
            worst[2] = worst[2].max(((exp(e) - e.exp()) / e.exp()).abs());
        }
        assert!(worst.iter().all(|&w| w < 2.5e-7), "{worst:?}");
        assert_eq!(exp(-200.0), 0.0);
        assert_eq!(exp(200.0), f32::INFINITY);
        assert_eq!(exp(0.0), 1.0);
        assert_eq!(sin(0.0), 0.0);
        assert_eq!(cos(0.0), 1.0);
    }

    #[test]
    fn rotations() {
        let q = rot_axis_angle(glam::Vec3::Y, 1.0);
        let g = glam::Quat::from_axis_angle(glam::Vec3::Y, 1.0);
        let r = rot_mul(q, q);
        let gr = g * g;
        for (a, b) in r.iter().zip(gr.to_array()) {
            assert!((a - b).abs() < 1e-6);
        }
        // Short way round: q and -q are the same rotation.
        let neg = q.map(|v| -v);
        let mid = rot_nlerp(q, neg, 0.5);
        assert!((mid[3].abs() - q[3].abs()).abs() < 1e-6);
        // Spinning at 1 rad/s for 1 s turns by 1 rad.
        let mut s = ROT_IDENTITY;
        for _ in 0..100 {
            s = rot_integrate(s, glam::Vec3::Y, 0.01);
        }
        for (a, b) in s.iter().zip(q) {
            assert!((a - b).abs() < 1e-5);
        }
    }
}
