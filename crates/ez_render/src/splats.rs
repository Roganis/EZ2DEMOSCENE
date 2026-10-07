//! Gaussian splats (3D Gaussian splatting): reading splat files, a small
//! built-in cloud, getting them ready to draw and sorting them.
//!
//! A splat is a soft ellipsoid: a centre, a size along each of its axes, a
//! rotation, a colour and an opacity. Files:
//!
//! - **PLY**, as the training tools write it: per splat `x y z`, the log of
//!   the sizes `scale_0..2`, the rotation `rot_0..3` (w x y z), the opacity
//!   before a sigmoid and the colour as spherical harmonics (`f_dc_0..2`
//!   for the base colour; the view-dependent `f_rest_*` are not used);
//! - **SPZ** (Niantic's compressed format, versions 1 to 3): the same,
//!   quantised and gzipped, about a tenth of the size;
//! - **.splat** (antimatter15's web viewer): 32 bytes a splat.
//!
//! They are drawn back to front as screen-facing ellipses (`splat.wgsl`),
//! from a texture holding two texels per splat (see [`Prepared`]), in an
//! order sorted on the CPU each frame ([`sort_back_to_front`]). A texture
//! and an index per instance work everywhere, WebGL2 included.

use anyhow::{bail, Context, Result};
use glam::{Mat3, Mat4, Quat, Vec3};
use std::io::Read;

/// The constant spherical harmonic: colour = 0.5 + SH_C0 · f_dc.
pub const SH_C0: f32 = 0.282_094_8;
/// Splats per texture row (two texels each).
pub const PER_ROW: u32 = 1024;
/// The most splats a layer draws, whatever it asks for (texture height).
pub const MAX_SPLATS: u32 = 4_000_000;
/// Splat file extensions read here (`.ksplat` and SOG are not).
pub const SPLAT_EXTENSIONS: &[&str] = ez_core::SPLAT_EXTENSIONS;

const SPZ_MAGIC: u32 = 0x5053_474e; // "NGSP"

/// Splats as a file holds them, in its own frame.
#[derive(Clone, Debug, Default)]
pub struct Splats {
    pub positions: Vec<Vec3>,
    /// Sizes along the splat's own axes (standard deviations).
    pub scales: Vec<Vec3>,
    pub rotations: Vec<Quat>,
    /// Colour (sRGB) and opacity.
    pub colors: Vec<[u8; 4]>,
}

impl Splats {
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    fn with_capacity(n: usize) -> Splats {
        Splats {
            positions: Vec::with_capacity(n),
            scales: Vec::with_capacity(n),
            rotations: Vec::with_capacity(n),
            colors: Vec::with_capacity(n),
        }
    }

    fn push(&mut self, p: Vec3, scale: Vec3, rot: Quat, color: [u8; 4]) {
        self.positions.push(p);
        self.scales.push(scale);
        self.rotations.push(rot.normalize());
        self.colors.push(color);
    }
}

/// Load splats by asset path (a file or an in-memory asset).
pub fn load_splats_asset(path: &str) -> Result<Splats> {
    let bytes = ez_core::store::read(path).with_context(|| format!("reading {path}"))?;
    load_splats_bytes(&ez_core::store::extension(path), &bytes)
}

/// Parse a splat file; `ext` is its extension (ply, spz, splat).
pub fn load_splats_bytes(ext: &str, bytes: &[u8]) -> Result<Splats> {
    let s = match ext {
        "ply" => read_ply(bytes)?,
        "spz" => read_spz(bytes)?,
        "splat" => read_splat(bytes)?,
        "ksplat" | "sog" => bail!(".{ext} files aren't read: convert them to PLY or SPZ"),
        _ => bail!("unsupported splat format '{ext}' (use .ply, .spz or .splat)"),
    };
    if s.is_empty() {
        bail!("the file has no splats");
    }
    Ok(s)
}

/// Whether a file holds splats: SPZ and .splat files do, and PLY files
/// whose vertices have splat properties (others are meshes or points).
pub fn is_splat_file(ext: &str, bytes: &[u8]) -> bool {
    match ext {
        "spz" | "splat" | "ksplat" | "sog" => true,
        "ply" => crate::ply::Ply::parse(bytes)
            .ok()
            .and_then(|p| {
                p.element("vertex")
                    .map(|v| v.has("f_dc_0") && v.has("opacity"))
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn unit_byte(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The usual 3D Gaussian splatting PLY.
fn read_ply(bytes: &[u8]) -> Result<Splats> {
    let ply = crate::ply::Ply::parse(bytes)?;
    if ply.element("chunk").is_some() {
        bail!("compressed PLY splats (SuperSplat) aren't read: save them as a plain PLY or SPZ");
    }
    let names = [
        "x", "y", "z", "scale_0", "scale_1", "scale_2", "rot_0", "rot_1", "rot_2", "rot_3",
        "opacity", "f_dc_0", "f_dc_1", "f_dc_2",
    ];
    let data = ply.read(&[("vertex", &names)])?;
    let v = &data
        .get("vertex")
        .context("the PLY file has no vertices")?
        .scalars;
    let mut cols = Vec::new();
    for n in names {
        cols.push(
            v.get(n)
                .with_context(|| format!("not a splat PLY file: its vertices have no {n}"))?,
        );
    }
    let n = cols[0].len();
    let mut out = Splats::with_capacity(n);
    #[allow(clippy::needless_range_loop)] // one value from each of 14 columns
    for i in 0..n {
        let c = |k: usize| cols[k][i];
        let color = [11, 12, 13].map(|k| unit_byte(0.5 + SH_C0 * c(k)));
        out.push(
            Vec3::new(c(0), c(1), c(2)),
            Vec3::new(c(3).exp(), c(4).exp(), c(5).exp()),
            Quat::from_xyzw(c(7), c(8), c(9), c(6)),
            [color[0], color[1], color[2], unit_byte(sigmoid(c(10)))],
        );
    }
    Ok(out)
}

/// antimatter15's `.splat`: position and size as floats, colour and
/// opacity as bytes, the rotation (w x y z) as bytes around 128.
fn read_splat(bytes: &[u8]) -> Result<Splats> {
    if !bytes.len().is_multiple_of(32) {
        bail!("not a .splat file (its size isn't a multiple of 32 bytes)");
    }
    let mut out = Splats::with_capacity(bytes.len() / 32);
    for r in bytes.chunks_exact(32) {
        let f = |k: usize| f32::from_le_bytes(r[k * 4..k * 4 + 4].try_into().unwrap());
        let q = |k: usize| (r[28 + k] as f32 - 128.0) / 128.0;
        out.push(
            Vec3::new(f(0), f(1), f(2)),
            Vec3::new(f(3), f(4), f(5)),
            Quat::from_xyzw(q(1), q(2), q(3), q(0)),
            [r[24], r[25], r[26], r[27]],
        );
    }
    Ok(out)
}

/// SPZ, versions 1 (half-float positions), 2 (24-bit fixed point
/// positions, rotation x y z) and 3 (rotation as its three smallest
/// components).
fn read_spz(bytes: &[u8]) -> Result<Splats> {
    let mut data = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut data)
        .context("not an SPZ file (not gzipped)")?;
    if data.len() < 16 {
        bail!("not an SPZ file");
    }
    let u32_at = |k: usize| u32::from_le_bytes(data[k..k + 4].try_into().unwrap());
    let (magic, version, n) = (u32_at(0), u32_at(4), u32_at(8) as usize);
    let (degree, bits) = (data[12], data[13]);
    if magic != SPZ_MAGIC || degree > 3 {
        bail!("not an SPZ file");
    }
    if !(1..=3).contains(&version) {
        bail!("SPZ version {version}: versions 1 to 3 are read");
    }
    let sh = [0, 3, 8, 15][degree as usize] * 3;
    let sizes = [
        n * if version == 1 { 6 } else { 9 },
        n,
        n * 3,
        n * 3,
        n * if version >= 3 { 4 } else { 3 },
        n * sh,
    ];
    if data.len() < 16 + sizes.iter().sum::<usize>() {
        bail!("the SPZ file ends inside the splats");
    }
    let mut at = 16;
    let mut part = |size: usize| {
        let s = &data[at..at + size];
        at += size;
        s
    };
    let (pos, alpha, color, scale, rot) = (
        part(sizes[0]),
        part(sizes[1]),
        part(sizes[2]),
        part(sizes[3]),
        part(sizes[4]),
    );
    let fixed = 1.0 / (1u32 << bits.min(24)) as f32;
    let mut out = Splats::with_capacity(n);
    for i in 0..n {
        let p = if version == 1 {
            let h = |k: usize| {
                f16_to_f32(u16::from_le_bytes([
                    pos[i * 6 + k * 2],
                    pos[i * 6 + k * 2 + 1],
                ]))
            };
            Vec3::new(h(0), h(1), h(2))
        } else {
            let v = |k: usize| {
                let b = &pos[i * 9 + k * 3..];
                let raw = b[0] as i32 | (b[1] as i32) << 8 | (b[2] as i32) << 16;
                // Sign-extend the 24 bits.
                ((raw << 8) >> 8) as f32 * fixed
            };
            Vec3::new(v(0), v(1), v(2))
        };
        let s = |k: usize| (scale[i * 3 + k] as f32 / 16.0 - 10.0).exp();
        let q = if version >= 3 {
            smallest_three(u32::from_le_bytes(
                rot[i * 4..i * 4 + 4].try_into().unwrap(),
            ))
        } else {
            let c = |k: usize| rot[i * 3 + k] as f32 / 127.5 - 1.0;
            let xyz = Vec3::new(c(0), c(1), c(2));
            Quat::from_xyzw(
                xyz.x,
                xyz.y,
                xyz.z,
                (1.0 - xyz.length_squared()).max(0.0).sqrt(),
            )
        };
        // Colour bytes hold the spherical-harmonic base coefficient.
        let c = |k: usize| unit_byte(0.5 + SH_C0 * (color[i * 3 + k] as f32 / 255.0 - 0.5) / 0.15);
        out.push(
            p,
            Vec3::new(s(0), s(1), s(2)),
            q,
            [c(0), c(1), c(2), alpha[i]],
        );
    }
    Ok(out)
}

/// SPZ 3's rotation: the index (2 bits) of the largest component of
/// (x, y, z, w), left out, then the others last first, each a sign bit and
/// 9 bits of magnitude in units of √½ / 511.
fn smallest_three(mut bits: u32) -> Quat {
    let largest = (bits >> 30) as usize;
    let mut q = [0.0f32; 4];
    let mut sum = 0.0;
    for k in (0..4).rev() {
        if k == largest {
            continue;
        }
        let magnitude = (bits & 511) as f32 * std::f32::consts::FRAC_1_SQRT_2 / 511.0;
        q[k] = if bits & 512 != 0 {
            -magnitude
        } else {
            magnitude
        };
        sum += q[k] * q[k];
        bits >>= 10;
    }
    q[largest] = (1.0 - sum).max(0.0).sqrt();
    Quat::from_xyzw(q[0], q[1], q[2], q[3])
}

fn f16_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = (h >> 10) & 31;
    let frac = (h & 1023) as f32;
    sign * match exp {
        0 => frac * 2f32.powi(-24),
        31 => f32::INFINITY,
        e => (1.0 + frac / 1024.0) * 2f32.powi(e as i32 - 15),
    }
}

/// A small built-in cloud (a spiral galaxy), shown until a file is chosen.
/// Y up.
pub fn sample() -> Splats {
    let mut rng = ez_core::rng::Rng::new(7);
    let n = 24_000;
    let mut out = Splats::with_capacity(n);
    for i in 0..n {
        let core = i < n / 5;
        let arm = (i % 3) as f32;
        let t = rng.f32();
        let (p, color) = if core {
            let d = Vec3::new(rng.f32() - 0.5, rng.f32() - 0.5, rng.f32() - 0.5);
            let p = d.normalize_or(Vec3::Y) * rng.f32().powf(2.0) * 0.35;
            (p * Vec3::new(1.0, 0.45, 1.0), [255, 214, 150])
        } else {
            let r = 0.12 + t * 0.88;
            let angle = arm * std::f32::consts::TAU / 3.0 + r * 5.0 + (rng.f32() - 0.5) * 0.5;
            let spread = 0.06 + 0.05 * r;
            let p = Vec3::new(angle.cos() * r, 0.0, angle.sin() * r)
                + Vec3::new(rng.f32() - 0.5, (rng.f32() - 0.5) * 0.3, rng.f32() - 0.5) * spread;
            let warm = (1.0 - r).clamp(0.0, 1.0);
            let pink = rng.f32() < 0.08;
            let c = if pink {
                [255, 90, 170]
            } else {
                [
                    (120.0 + 135.0 * warm) as u8,
                    (150.0 + 70.0 * warm) as u8,
                    255 - (90.0 * warm) as u8,
                ]
            };
            (p, c)
        };
        let size = 0.004 + rng.f32() * 0.012;
        let flat = Vec3::new(size, size * (0.3 + 0.7 * rng.f32()), size);
        let turn = Quat::from_rotation_y(rng.f32() * std::f32::consts::TAU);
        let alpha = (120.0 + 120.0 * rng.f32()) as u8;
        out.push(p, flat, turn, [color[0], color[1], color[2], alpha]);
    }
    out
}

/// Splats ready for the GPU: two texels (`[u32; 4]`) per splat, `PER_ROW`
/// splats a row:
///
/// - texel 0: x, y, z (f32 bits), colour and opacity (rgba8);
/// - texel 1: size along x, y, z (f32 bits), rotation (w, x, y, z as
///   snorm8).
pub struct Prepared {
    pub texels: Vec<[u32; 4]>,
    /// Texture size in texels.
    pub size: (u32, u32),
    /// Centres, for sorting.
    pub centres: Vec<Vec3>,
    /// Distance of the farthest splat (with its size) from the origin.
    pub radius: f32,
}

impl Prepared {
    pub fn count(&self) -> u32 {
        self.centres.len() as u32
    }
}

/// Turn `s` by `up` (rows: the file's axes to the scene's), keep at most
/// `max` (the most visible: opaque and large), fit them into the unit
/// sphere if `fit` (the 2nd to 98th percentiles of the opaque splats, so
/// strays far out don't shrink the rest) and pack them.
pub fn prepare(s: &Splats, up: [[f32; 3]; 3], fit: bool, max: u32) -> Prepared {
    let max = max.clamp(1, MAX_SPLATS) as usize;
    let mut keep: Vec<u32> = (0..s.len() as u32).collect();
    if keep.len() > max {
        let score = |i: &u32| {
            let i = *i as usize;
            let v = s.scales[i];
            s.colors[i][3] as f32 * (v.x * v.y * v.z).abs().cbrt()
        };
        keep.select_nth_unstable_by(max, |a, b| score(b).total_cmp(&score(a)));
        keep.truncate(max);
        keep.sort_unstable();
    }
    let turn = Mat3::from_cols(up[0].into(), up[1].into(), up[2].into()).transpose();
    let turn_q = Quat::from_mat3(&turn);
    let mut centres: Vec<Vec3> = keep
        .iter()
        .map(|&i| turn * s.positions[i as usize])
        .collect();
    let mut factor = 1.0;
    if fit && !centres.is_empty() {
        let solid: Vec<Vec3> = keep
            .iter()
            .zip(&centres)
            .filter(|(&i, _)| s.colors[i as usize][3] >= 128)
            .map(|(_, p)| *p)
            .collect();
        let pts = if solid.len() >= 16 { &solid } else { &centres };
        let (lo, hi) = percentile_box(pts, 0.02);
        let centre = (lo + hi) * 0.5;
        factor = 1.0 / ((hi - lo).length() * 0.5).max(1e-6);
        for p in &mut centres {
            *p = (*p - centre) * factor;
        }
    }
    let rows = (keep.len() as u32).div_ceil(PER_ROW).max(1);
    let mut texels = vec![[0u32; 4]; (rows * PER_ROW * 2) as usize];
    let mut radius = 0.0f32;
    for (k, &i) in keep.iter().enumerate() {
        let i = i as usize;
        let p = centres[k];
        let scale = s.scales[i] * factor;
        let q = (turn_q * s.rotations[i]).normalize();
        // q and −q are the same turn: keep w positive.
        let q = if q.w < 0.0 { -q } else { q };
        radius = radius.max(p.length() + 3.0 * scale.max_element());
        let c = s.colors[i];
        let snorm = |v: f32| ((v.clamp(-1.0, 1.0) * 127.0).round() as i8) as u8 as u32;
        texels[2 * k] = [
            p.x.to_bits(),
            p.y.to_bits(),
            p.z.to_bits(),
            u32::from_le_bytes(c),
        ];
        texels[2 * k + 1] = [
            scale.x.to_bits(),
            scale.y.to_bits(),
            scale.z.to_bits(),
            snorm(q.w) | snorm(q.x) << 8 | snorm(q.y) << 16 | snorm(q.z) << 24,
        ];
    }
    Prepared {
        texels,
        size: (PER_ROW * 2, rows),
        centres,
        radius,
    }
}

/// The box between the `q` and `1 - q` quantiles of each axis.
fn percentile_box(pts: &[Vec3], q: f32) -> (Vec3, Vec3) {
    let mut lo = Vec3::ZERO;
    let mut hi = Vec3::ZERO;
    for axis in 0..3 {
        let mut v: Vec<f32> = pts.iter().map(|p| p[axis]).collect();
        let n = v.len();
        let a = ((n as f32 * q) as usize).min(n - 1);
        let b = ((n as f32 * (1.0 - q)) as usize).min(n - 1);
        lo[axis] = *v.select_nth_unstable_by(a, f32::total_cmp).1;
        hi[axis] = *v.select_nth_unstable_by(b, f32::total_cmp).1;
    }
    (lo, hi)
}

/// The splats' indices in drawing order, farthest from the camera first,
/// for `view_model` (splat space to the camera's: it looks down −z). A
/// counting sort on 16-bit depths: linear in the number of splats.
pub fn sort_back_to_front(centres: &[Vec3], view_model: Mat4, order: &mut Vec<u32>) {
    let row = view_model.row(2);
    let depth: Vec<f32> = centres
        .iter()
        .map(|p| row.x * p.x + row.y * p.y + row.z * p.z + row.w)
        .collect();
    let (lo, hi) = depth
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &d| (lo.min(d), hi.max(d)));
    let scale = if hi > lo { 65535.0 / (hi - lo) } else { 0.0 };
    // Most negative z (farthest) first.
    let keys: Vec<u16> = depth
        .iter()
        .map(|&d| ((d - lo) * scale).clamp(0.0, 65535.0) as u16)
        .collect();
    let mut starts = vec![0u32; 65537];
    for &k in &keys {
        starts[k as usize + 1] += 1;
    }
    for i in 1..starts.len() {
        starts[i] += starts[i - 1];
    }
    order.clear();
    order.resize(centres.len(), 0);
    for (i, &k) in keys.iter().enumerate() {
        let at = &mut starts[k as usize];
        order[*at as usize] = i as u32;
        *at += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ply_bytes(rows: &[[f32; 14]]) -> Vec<u8> {
        let names = [
            "x", "y", "z", "f_dc_0", "f_dc_1", "f_dc_2", "opacity", "scale_0", "scale_1",
            "scale_2", "rot_0", "rot_1", "rot_2", "rot_3",
        ];
        let mut b = format!(
            "ply\nformat binary_little_endian 1.0\nelement vertex {}\n{}property float f_rest_0\nend_header\n",
            rows.len(),
            names.iter().map(|n| format!("property float {n}\n")).collect::<String>()
        )
        .into_bytes();
        for r in rows {
            for v in r.iter().chain([&9.0]) {
                b.extend(v.to_le_bytes());
            }
        }
        b
    }

    #[test]
    fn reads_ply_splats() {
        // Centre, colour 0.5 + C0 · dc, opacity 0, log sizes, rotation (w x y z).
        let b = ply_bytes(&[[
            1.0, 2.0, 3.0, 1.0, 0.0, -1.0, 0.0, 0.0, -1.0, 1.0, 2.0, 0.0, 0.0, 0.0,
        ]]);
        assert!(is_splat_file("ply", &b));
        let s = load_splats_bytes("ply", &b).unwrap();
        assert_eq!(s.positions, [Vec3::new(1.0, 2.0, 3.0)]);
        assert_eq!(
            s.colors[0],
            [unit_byte(0.5 + SH_C0), 128, unit_byte(0.5 - SH_C0), 128]
        );
        assert!((s.scales[0] - Vec3::new(1.0, (-1.0f32).exp(), 1f32.exp())).length() < 1e-5);
        assert!(s.rotations[0].abs_diff_eq(Quat::IDENTITY, 1e-6));
        // A PLY mesh is not splats.
        let mesh = b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nend_header\n";
        assert!(!is_splat_file("ply", mesh));
        assert!(load_splats_bytes("ply", mesh).is_err());
    }

    #[test]
    fn reads_splat_files() {
        let mut b = Vec::new();
        for v in [1.0f32, 2.0, 3.0, 0.5, 1.0, 2.0] {
            b.extend(v.to_le_bytes());
        }
        b.extend([10, 20, 30, 200, 128, 255, 128, 128]); // rgba, then w x y z
        let s = load_splats_bytes("splat", &b).unwrap();
        assert_eq!(s.colors[0], [10, 20, 30, 200]);
        assert_eq!(s.scales[0], Vec3::new(0.5, 1.0, 2.0));
        let x_turn = Quat::from_xyzw(127.0 / 128.0, 0.0, 0.0, 0.0).normalize();
        assert!(s.rotations[0].abs_diff_eq(x_turn, 1e-6));
        assert!(load_splats_bytes("splat", &b[..31]).is_err());
    }

    fn spz(version: u32, splats: &[(Vec3, Quat)]) -> Vec<u8> {
        let n = splats.len();
        let mut d = Vec::new();
        d.extend(SPZ_MAGIC.to_le_bytes());
        d.extend(version.to_le_bytes());
        d.extend((n as u32).to_le_bytes());
        d.extend([0, 12, 0, 0]);
        for (p, _) in splats {
            for v in p.to_array() {
                if version == 1 {
                    // Exact halves for these test values.
                    let h = half(v);
                    d.extend(h.to_le_bytes());
                } else {
                    let f = (v * 4096.0).round() as i32;
                    d.extend(&f.to_le_bytes()[..3]);
                }
            }
        }
        d.extend(vec![255u8; n]); // opacity
        d.extend(vec![128u8; n * 3]); // colour
        d.extend(vec![160u8; n * 3]); // size: exp(0)
        for (_, q) in splats {
            let q = if q.w < 0.0 { -*q } else { *q };
            if version >= 3 {
                let xyzw = q.to_array();
                let largest = (0..4)
                    .max_by(|&a, &b| xyzw[a].abs().total_cmp(&xyzw[b].abs()))
                    .unwrap();
                let s = xyzw[largest].signum();
                let mut word = largest as u32;
                for (k, v) in xyzw.iter().enumerate() {
                    if k != largest {
                        let v = v * s;
                        let m = (v.abs() / std::f32::consts::FRAC_1_SQRT_2 * 511.0).round() as u32;
                        word = word << 10 | ((v < 0.0) as u32) << 9 | m;
                    }
                }
                d.extend(word.to_le_bytes());
            } else {
                for v in [q.x, q.y, q.z] {
                    d.push(((v + 1.0) * 127.5).round() as u8);
                }
            }
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &d).unwrap();
        gz.finish().unwrap()
    }

    fn half(v: f32) -> u16 {
        if v == 0.0 {
            return 0;
        }
        let sign = if v < 0.0 { 0x8000 } else { 0 };
        let e = v.abs().log2().floor() as i32;
        let frac = ((v.abs() / 2f32.powi(e) - 1.0) * 1024.0).round() as u16;
        sign | (((e + 15) as u16) << 10) | frac
    }

    #[test]
    fn reads_spz_versions_1_to_3() {
        let turn = Quat::from_rotation_y(0.7) * Quat::from_rotation_x(-0.3);
        let splats = [
            (Vec3::new(1.5, -2.25, 0.5), turn),
            (Vec3::new(-0.5, 0.0, 3.0), Quat::IDENTITY),
        ];
        for version in 1..=3 {
            let s = load_splats_bytes("spz", &spz(version, &splats)).unwrap();
            assert_eq!(s.len(), 2);
            for (k, (p, q)) in splats.iter().enumerate() {
                assert!((s.positions[k] - *p).length() < 1e-3, "v{version} position");
                assert!(s.rotations[k].dot(*q).abs() > 0.999, "v{version} rotation");
                assert!((s.scales[k] - Vec3::ONE).length() < 1e-5);
                assert_eq!(s.colors[k], [128, 128, 128, 255]);
            }
        }
        let mut bad = spz(2, &splats);
        bad.truncate(bad.len() / 2);
        assert!(load_splats_bytes("spz", &bad).is_err());
        assert!(load_splats_bytes("spz", b"not gzip").is_err());
        assert!(load_splats_bytes("ksplat", b"")
            .unwrap_err()
            .to_string()
            .contains("convert"));
    }

    #[test]
    fn prepare_fits_turns_and_packs() {
        let mut s = Splats::default();
        for i in 0..100 {
            let x = i as f32 / 99.0 * 10.0 + 5.0; // 5..15 along x
            s.push(
                Vec3::new(x, 0.0, 0.0),
                Vec3::splat(0.1),
                Quat::IDENTITY,
                [255, 0, 0, 255],
            );
        }
        // A faint stray far out: left out of the fit, and of `max`.
        s.push(
            Vec3::new(1000.0, 0.0, 0.0),
            Vec3::splat(0.001),
            Quat::IDENTITY,
            [0, 0, 0, 1],
        );
        let z_up = ez_core::SplatUp::ZUp.matrix("ply");
        let p = prepare(&s, z_up, true, 100);
        assert_eq!(p.count(), 100);
        let xs: Vec<f32> = p.centres.iter().map(|c| c.x).collect();
        let (lo, hi) = xs
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
        assert!(lo > -1.1 && hi < 1.1 && hi - lo > 1.8, "{lo} {hi}");
        assert_eq!(p.size, (PER_ROW * 2, 1));
        // Texel 0 holds the centre and colour, texel 1 the size and rotation.
        assert_eq!(f32::from_bits(p.texels[0][0]), p.centres[0].x);
        assert_eq!(p.texels[0][3], u32::from_le_bytes([255, 0, 0, 255]));
        assert!((f32::from_bits(p.texels[1][0]) - 0.1 / 5.0).abs() < 0.01);
        // The Z-up turn is a quarter turn about x: w = cos 45°.
        assert_eq!(p.texels[1][3] & 0xff, 90);
    }

    #[test]
    fn sorts_farthest_first() {
        let centres: Vec<Vec3> = [3.0, -5.0, 0.0, 9.0, -1.0]
            .map(|z| Vec3::new(0.0, 0.0, z))
            .into();
        // The camera at z = 20 looking down −z: the farthest has the smallest z.
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 20.0), Vec3::ZERO, Vec3::Y);
        let mut order = Vec::new();
        sort_back_to_front(&centres, view, &mut order);
        assert_eq!(order, [1, 4, 2, 0, 3]);
    }

    #[test]
    fn sample_cloud() {
        let s = sample();
        assert!(s.len() > 10_000);
        let p = prepare(&s, ez_core::SplatUp::YUp.matrix("ply"), true, 2_000_000);
        assert!(p.radius > 0.8 && p.radius < 2.0, "{}", p.radius);
    }
}
