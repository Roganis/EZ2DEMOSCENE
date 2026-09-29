//! Environment maps for image-based lighting, worked out on the CPU (the
//! same on every backend, and testable without a GPU):
//! - an equirectangular panorama from a `.hdr` file or a built-in studio;
//! - a cube map of it with mips prefiltered for rougher and rougher
//!   reflections (GGX importance sampling);
//! - its diffuse light as 9 spherical-harmonic coefficients;
//! - the split-sum BRDF table (how much of the reflection a surface
//!   shows, by angle and roughness);
//! - its sun (the brightest spot), to light and shadow with.
//!
//! Directions: `u` = 0.5 looks along -z, `u` grows towards +x; `v` = 0 is
//! straight up.

use ez_core::Studio;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

/// Side of the cube map's largest mip.
pub const CUBE_SIZE: u32 = 256;
/// Mips of the cube map: roughness 0, 0.2, … 1.
pub const CUBE_MIPS: u32 = 6;
/// Side of the BRDF table.
pub const LUT_SIZE: u32 = 32;
/// Most GGX samples per texel of a prefiltered mip.
const SAMPLES: u32 = 64;

/// A panorama: `w` × `h` linear RGB, rows from the top.
#[derive(Clone, Debug)]
pub struct Equirect {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f32; 3]>,
}

/// Direction of panorama coordinates (0..1).
pub fn dir_of(u: f32, v: f32) -> Vec3 {
    let phi = (u - 0.5) * TAU;
    let theta = v * PI;
    Vec3::new(
        theta.sin() * phi.sin(),
        theta.cos(),
        -theta.sin() * phi.cos(),
    )
}

/// Panorama coordinates (0..1) of a direction.
pub fn uv_of(d: Vec3) -> (f32, f32) {
    let d = d.normalize_or(Vec3::Y);
    let u = d.x.atan2(-d.z) / TAU + 0.5;
    let v = d.y.clamp(-1.0, 1.0).acos() / PI;
    (u, v)
}

fn lum(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

impl Equirect {
    pub fn new(w: usize, h: usize, f: impl Fn(Vec3) -> [f32; 3]) -> Equirect {
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                px.push(f(dir_of(
                    (x as f32 + 0.5) / w as f32,
                    (y as f32 + 0.5) / h as f32,
                )));
            }
        }
        Equirect { w, h, px }
    }

    fn at(&self, x: i64, y: i64) -> Vec3 {
        let x = x.rem_euclid(self.w as i64) as usize;
        let y = y.clamp(0, self.h as i64 - 1) as usize;
        Vec3::from(self.px[y * self.w + x])
    }

    /// Bilinear sample towards `d`.
    pub fn sample(&self, d: Vec3) -> Vec3 {
        let (u, v) = uv_of(d);
        let fx = u * self.w as f32 - 0.5;
        let fy = v * self.h as f32 - 0.5;
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let a = self.at(x0, y0).lerp(self.at(x0 + 1, y0), tx);
        let b = self.at(x0, y0 + 1).lerp(self.at(x0 + 1, y0 + 1), tx);
        a.lerp(b, ty)
    }

    /// Half the size (2 × 2 boxes, rows weighted by how much of the sphere
    /// they cover, so blurring keeps the light's total).
    pub fn half(&self) -> Equirect {
        let (w, h) = ((self.w / 2).max(1), (self.h / 2).max(1));
        let row_weight = |y: i64| ((y as f32 + 0.5) / self.h as f32 * PI).sin().max(1e-4);
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            let (y2, y3) = (y as i64 * 2, (y as i64 * 2 + 1).min(self.h as i64 - 1));
            let (wa, wb) = (row_weight(y2), row_weight(y3));
            for x in 0..w {
                let x2 = x as i64 * 2;
                let c = (self.at(x2, y2) + self.at(x2 + 1, y2)) * wa
                    + (self.at(x2, y3) + self.at(x2 + 1, y3)) * wb;
                px.push((c / (2.0 * (wa + wb))).into());
            }
        }
        Equirect { w, h, px }
    }

    /// No wider than `w` (big photos are more than a reflection needs).
    pub fn at_most(mut self, w: usize) -> Equirect {
        while self.w > w {
            self = self.half();
        }
        self
    }

    /// Solid angle of a pixel of row `y`.
    fn pixel_angle(&self, y: usize) -> f32 {
        let theta = (y as f32 + 0.5) / self.h as f32 * PI;
        (TAU / self.w as f32) * (PI / self.h as f32) * theta.sin()
    }
}

/// Reads a Radiance `.hdr` panorama.
pub fn load_hdr(bytes: &[u8]) -> Result<Equirect, String> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Hdr)
        .map_err(|e| e.to_string())?
        .to_rgb32f();
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w < 2 || h < 1 {
        return Err("empty image".into());
    }
    let px = img
        .pixels()
        .map(|p| [p[0].max(0.0), p[1].max(0.0), p[2].max(0.0)])
        .collect();
    Ok(Equirect { w, h, px }.at_most(2048))
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A soft rectangle of light seen in direction `d`: centred on `at`, `w`
/// wide and `h` high (radians, roughly).
fn softbox(d: Vec3, at: Vec3, w: f32, h: f32) -> f32 {
    let at = at.normalize();
    let side = Vec3::Y.cross(at).normalize_or(Vec3::X);
    let up = at.cross(side);
    let f = d.dot(at);
    if f <= 0.0 {
        return 0.0;
    }
    let (x, y) = (d.dot(side) / f, d.dot(up) / f);
    let edge = 0.08;
    smooth(w * 0.5 + edge, w * 0.5, x.abs()) * smooth(h * 0.5 + edge, h * 0.5, y.abs())
}

/// A built-in environment (made, not loaded).
pub fn studio(kind: Studio) -> Equirect {
    let (w, h) = (512, 256);
    match kind {
        Studio::Softbox => Equirect::new(w, h, |d| {
            let floor = smooth(0.1, -0.3, d.y);
            let mut c =
                Vec3::splat(0.05 + 0.04 * d.y.max(0.0)) * (1.0 - floor) + Vec3::splat(0.02) * floor;
            c += Vec3::splat(9.0) * softbox(d, Vec3::new(-0.8, 0.6, -0.4), 0.9, 0.6);
            c += Vec3::new(6.0, 6.2, 6.6) * softbox(d, Vec3::new(0.9, 0.35, 0.2), 0.5, 1.2);
            c += Vec3::splat(4.0) * softbox(d, Vec3::new(0.1, 1.0, 0.05), 0.8, 0.8);
            c += Vec3::new(2.0, 1.8, 1.5) * softbox(d, Vec3::new(0.0, 0.2, 1.0), 1.6, 0.25);
            c.into()
        }),
        Studio::Overcast => Equirect::new(w, h, |d| {
            let sky = Vec3::new(0.85, 0.9, 1.0) * (0.9 + 0.6 * d.y.max(0.0));
            let ground = Vec3::new(0.25, 0.23, 0.2);
            let c = ground.lerp(sky, smooth(-0.08, 0.08, d.y));
            (c * 1.2).into()
        }),
        Studio::Sunset => Equirect::new(w, h, |d| {
            let sun = Vec3::new(0.0, 0.08, -1.0).normalize();
            let t = d.y.max(0.0);
            let sky =
                Vec3::new(1.4, 0.55, 0.25).lerp(Vec3::new(0.15, 0.3, 0.8), smooth(0.0, 0.5, t));
            let glow = Vec3::new(2.0, 0.9, 0.4) * d.dot(sun).max(0.0).powf(8.0);
            let disc = Vec3::new(60.0, 40.0, 20.0) * smooth(0.9985, 0.9995, d.dot(sun));
            let ground = Vec3::new(0.12, 0.08, 0.07);
            let c = ground.lerp(sky + glow, smooth(-0.04, 0.02, d.y)) + disc;
            c.into()
        }),
        Studio::NeonRoom => Equirect::new(w, h, |d| {
            let mut c = Vec3::splat(0.015);
            // Horizontal tubes around the room at two heights, and a ring
            // overhead.
            let a = d.x.atan2(-d.z);
            let tube = |y: f32| smooth(0.035, 0.0, (d.y - y).abs());
            let seg = |k: f32| smooth(0.15, 0.3, (a * k).sin().abs());
            c += Vec3::new(6.0, 0.4, 3.5) * tube(0.25) * seg(3.0);
            c += Vec3::new(0.3, 4.0, 6.0) * tube(-0.1) * seg(2.0);
            c += Vec3::new(3.0, 3.0, 4.0) * smooth(0.03, 0.0, (d.y - 0.92).abs());
            c.into()
        }),
    }
}

/// The sun of a panorama: its direction, and the light it gives (the
/// radiance of the spot times its solid angle, as RGB).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sun {
    pub dir: Vec3,
    pub light: Vec3,
}

/// Finds the brightest spot; if it stands out (much brighter than the
/// sky around it), takes it out of the map and returns it as a sun.
pub fn extract_sun(eq: &mut Equirect) -> Option<Sun> {
    let (mut best, mut at) = (0.0, 0);
    for (i, p) in eq.px.iter().enumerate() {
        let l = lum(*p);
        if l > best {
            best = l;
            at = i;
        }
    }
    let median = {
        let mut l: Vec<f32> = eq.px.iter().step_by(7).map(|p| lum(*p)).collect();
        l.sort_by(f32::total_cmp);
        l[l.len() / 2]
    };
    if best < median.max(1e-4) * 20.0 {
        return None;
    }
    let (sx, sy) = (at % eq.w, at / eq.w);
    let centre = dir_of(
        (sx as f32 + 0.5) / eq.w as f32,
        (sy as f32 + 0.5) / eq.h as f32,
    );
    // The sun: everything near the peak brighter than a tenth of it.
    let cut = best * 0.1;
    let mut light = Vec3::ZERO;
    let mut dir = Vec3::ZERO;
    let floor = Vec3::splat(median);
    for y in 0..eq.h {
        let angle = eq.pixel_angle(y);
        for x in 0..eq.w {
            let d = dir_of(
                (x as f32 + 0.5) / eq.w as f32,
                (y as f32 + 0.5) / eq.h as f32,
            );
            let i = y * eq.w + x;
            if d.dot(centre) < 0.98 || lum(eq.px[i]) < cut {
                continue;
            }
            let p = Vec3::from(eq.px[i]);
            let extra = (p - floor).max(Vec3::ZERO);
            light += extra * angle;
            dir += d * lum(extra.into()) * angle;
            eq.px[i] = p.min(floor).into();
        }
    }
    Some(Sun {
        dir: dir.normalize_or(centre),
        light,
    })
}

/// Direction of texel (`x`, `y`) of cube face `face` (+x, -x, +y, -y, +z,
/// -z, rows from the top, as the GPU reads them).
pub fn cube_dir(face: u32, x: f32, y: f32, size: u32) -> Vec3 {
    let s = 2.0 * (x + 0.5) / size as f32 - 1.0;
    let t = 2.0 * (y + 0.5) / size as f32 - 1.0;
    match face {
        0 => Vec3::new(1.0, -t, -s),
        1 => Vec3::new(-1.0, -t, s),
        2 => Vec3::new(s, 1.0, t),
        3 => Vec3::new(s, -1.0, -t),
        4 => Vec3::new(s, -t, 1.0),
        _ => Vec3::new(-s, -t, -1.0),
    }
    .normalize()
}

/// Point `i` of `n` of the Hammersley set.
fn hammersley(i: u32, n: u32) -> (f32, f32) {
    (
        i as f32 / n as f32,
        (i.reverse_bits() as f64 / 4_294_967_296.0) as f32,
    )
}

/// A GGX-distributed half vector around `n` (roughness `a` = rough²).
fn ggx_half(xi: (f32, f32), n: Vec3, a: f32) -> Vec3 {
    let phi = TAU * xi.0;
    let cos_t = ((1.0 - xi.1) / (1.0 + (a * a - 1.0) * xi.1))
        .max(0.0)
        .sqrt();
    let sin_t = (1.0 - cos_t * cos_t).max(0.0).sqrt();
    let h = Vec3::new(sin_t * phi.cos(), sin_t * phi.sin(), cos_t);
    let up = if n.z.abs() < 0.999 { Vec3::Z } else { Vec3::X };
    let tx = up.cross(n).normalize();
    let ty = n.cross(tx);
    tx * h.x + ty * h.y + n * h.z
}

fn d_ggx(noh: f32, a: f32) -> f32 {
    let a2 = a * a;
    let d = noh * noh * (a2 - 1.0) + 1.0;
    a2 / (PI * d * d)
}

/// A cube map level: six faces of `size`² texels.
struct CubeLevel {
    size: u32,
    faces: [Vec<Vec3>; 6],
}

/// Face and face coordinates (-1..1) of a direction (the inverse of
/// [`cube_dir`]).
fn cube_face(d: Vec3) -> (usize, f32, f32) {
    let a = d.abs();
    if a.x >= a.y && a.x >= a.z {
        if d.x > 0.0 {
            (0, -d.z / a.x, -d.y / a.x)
        } else {
            (1, d.z / a.x, -d.y / a.x)
        }
    } else if a.y >= a.z {
        if d.y > 0.0 {
            (2, d.x / a.y, d.z / a.y)
        } else {
            (3, d.x / a.y, -d.z / a.y)
        }
    } else if d.z > 0.0 {
        (4, d.x / a.z, -d.y / a.z)
    } else {
        (5, -d.x / a.z, -d.y / a.z)
    }
}

impl CubeLevel {
    fn from_equirect(eq: &Equirect, size: u32) -> CubeLevel {
        let faces = std::array::from_fn(|f| {
            let mut v = Vec::with_capacity((size * size) as usize);
            for y in 0..size {
                for x in 0..size {
                    v.push(eq.sample(cube_dir(f as u32, x as f32, y as f32, size)));
                }
            }
            v
        });
        CubeLevel { size, faces }
    }

    /// Half the size, texels weighted by the solid angle they cover.
    fn half(&self) -> CubeLevel {
        let (n, size) = (self.size, (self.size / 2).max(1));
        let faces = std::array::from_fn(|f| {
            let mut v = Vec::with_capacity((size * size) as usize);
            for y in 0..size {
                for x in 0..size {
                    let (mut c, mut w) = (Vec3::ZERO, 0.0);
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (sx, sy) = ((x * 2 + dx).min(n - 1), (y * 2 + dy).min(n - 1));
                        let k = cube_dir(f as u32, sx as f32, sy as f32, n)
                            .abs()
                            .max_element()
                            .powi(3);
                        c += self.faces[f][(sy * n + sx) as usize] * k;
                        w += k;
                    }
                    v.push(c / w);
                }
            }
            v
        });
        CubeLevel { size, faces }
    }

    /// Bilinear sample towards `d` (clamped at face edges).
    fn sample(&self, d: Vec3) -> Vec3 {
        let (f, s, t) = cube_face(d);
        let n = self.size as f32;
        let fx = ((s + 1.0) * 0.5 * n - 0.5).clamp(0.0, n - 1.0);
        let fy = ((t + 1.0) * 0.5 * n - 0.5).clamp(0.0, n - 1.0);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = (
            (x0 + 1).min(self.size as usize - 1),
            (y0 + 1).min(self.size as usize - 1),
        );
        let (tx, ty) = (fx.fract(), fy.fract());
        let row = self.size as usize;
        let p = &self.faces[f];
        let a = p[y0 * row + x0].lerp(p[y0 * row + x1], tx);
        let b = p[y1 * row + x0].lerp(p[y1 * row + x1], tx);
        a.lerp(b, ty)
    }
}

/// The cube map: per mip, per face, `size`² RGBA texels. Mip 0 is the
/// panorama as it is (mirror); mip `m` is blurred like a surface of
/// roughness `m / (CUBE_MIPS - 1)` reflects it.
pub fn prefilter(eq: &Equirect) -> Vec<Vec<Vec<[f32; 4]>>> {
    // The map at every size, so wide samples read a blurred one (fewer
    // samples, no sparkle).
    let mut pyramid = vec![CubeLevel::from_equirect(eq, CUBE_SIZE)];
    while pyramid.last().is_some_and(|l| l.size > 1) {
        let next = pyramid.last().map(CubeLevel::half).expect("a level");
        pyramid.push(next);
    }
    let read = |d: Vec3, lod: f32| {
        let lod = lod.clamp(0.0, (pyramid.len() - 1) as f32);
        let (l0, t) = (lod.floor() as usize, lod.fract());
        let a = pyramid[l0].sample(d);
        if t > 0.0 && l0 + 1 < pyramid.len() {
            a.lerp(pyramid[l0 + 1].sample(d), t)
        } else {
            a
        }
    };
    let texel_angle = 4.0 * PI / (6 * CUBE_SIZE * CUBE_SIZE) as f32;
    let rgba = |c: Vec3| [c.x, c.y, c.z, 1.0];
    (0..CUBE_MIPS)
        .map(|mip| {
            let size = (CUBE_SIZE >> mip).max(1);
            if mip == 0 {
                return pyramid[0]
                    .faces
                    .iter()
                    .map(|f| f.iter().map(|c| rgba(*c)).collect())
                    .collect();
            }
            let rough = mip as f32 / (CUBE_MIPS - 1) as f32;
            let a = rough * rough;
            // The same samples for every texel (in its own frame): the
            // half vector, the weight (n·l) and the level to read.
            // Narrow lobes (reading blurred levels) need fewer.
            let count = (16 * (mip + 1)).min(SAMPLES);
            let samples: Vec<(Vec3, f32, f32)> = (0..count)
                .filter_map(|i| {
                    let h = ggx_half(hammersley(i, count), Vec3::Z, a);
                    let nol = 2.0 * h.z * h.z - 1.0;
                    if nol <= 0.0 {
                        return None;
                    }
                    // With N = V = R, pdf(l) = D / 4.
                    let pdf = d_ggx(h.z, a) * 0.25 + 1e-4;
                    let lod = 0.5 * ((1.0 / (count as f32 * pdf)) / texel_angle).log2() + 1.0;
                    Some((h, nol, lod))
                })
                .collect();
            let face = |f: u32| -> Vec<[f32; 4]> {
                let mut out = Vec::with_capacity((size * size) as usize);
                for y in 0..size {
                    for x in 0..size {
                        let n = cube_dir(f, x as f32, y as f32, size);
                        let up = if n.z.abs() < 0.999 { Vec3::Z } else { Vec3::X };
                        let tx = up.cross(n).normalize();
                        let ty = n.cross(tx);
                        let (mut sum, mut weight) = (Vec3::ZERO, 0.0);
                        for &(h, nol, lod) in &samples {
                            let hw = tx * h.x + ty * h.y + n * h.z;
                            let l = hw * (2.0 * h.z) - n;
                            sum += read(l, lod) * nol;
                            weight += nol;
                        }
                        out.push(rgba(sum / weight.max(1e-6)));
                    }
                }
                out
            };
            faces_in_parallel(&face)
        })
        .collect()
}

/// `face(0..6)`, on threads where there are threads.
fn faces_in_parallel(face: &(dyn Fn(u32) -> Vec<[f32; 4]> + Sync)) -> Vec<Vec<[f32; 4]>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..6).map(|f| s.spawn(move || face(f))).collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("face"))
                .collect()
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        (0..6).map(face).collect()
    }
}

/// Diffuse light as 9 spherical-harmonic coefficients (RGB): the light
/// falling on a surface facing `n` is `Σ c_i Y_i(n)` (see [`sh_eval`]).
pub fn irradiance_sh(eq: &Equirect) -> [[f32; 3]; 9] {
    let small = eq.clone().at_most(128);
    let mut c = [Vec3::ZERO; 9];
    for y in 0..small.h {
        let angle = small.pixel_angle(y);
        for x in 0..small.w {
            let d = dir_of(
                (x as f32 + 0.5) / small.w as f32,
                (y as f32 + 0.5) / small.h as f32,
            );
            let l = Vec3::from(small.px[y * small.w + x]) * angle;
            for (k, b) in sh_basis(d).iter().enumerate() {
                c[k] += l * *b;
            }
        }
    }
    // Convolve with the cosine lobe (Ramamoorthi and Hanrahan).
    let band = [
        PI,
        2.0 * PI / 3.0,
        2.0 * PI / 3.0,
        2.0 * PI / 3.0,
        PI / 4.0,
        PI / 4.0,
        PI / 4.0,
        PI / 4.0,
        PI / 4.0,
    ];
    std::array::from_fn(|k| (c[k] * band[k]).into())
}

/// The first 9 real spherical harmonics at `d`.
pub fn sh_basis(d: Vec3) -> [f32; 9] {
    [
        0.282095,
        0.488603 * d.y,
        0.488603 * d.z,
        0.488603 * d.x,
        1.092548 * d.x * d.y,
        1.092548 * d.y * d.z,
        0.315392 * (3.0 * d.z * d.z - 1.0),
        1.092548 * d.x * d.z,
        0.546274 * (d.x * d.x - d.y * d.y),
    ]
}

/// Irradiance towards `n` from coefficients (as the shader works it out).
pub fn sh_eval(c: &[[f32; 3]; 9], n: Vec3) -> Vec3 {
    sh_basis(n)
        .iter()
        .zip(c)
        .map(|(b, c)| Vec3::from(*c) * *b)
        .sum()
}

/// The split-sum table: for (n·v across, roughness down), the scale and
/// bias of the reflection's Fresnel (`f0 · x + y`).
pub fn brdf_lut() -> Vec<[f32; 2]> {
    let n = LUT_SIZE;
    let mut out = Vec::with_capacity((n * n) as usize);
    for j in 0..n {
        let rough = (j as f32 + 0.5) / n as f32;
        let a = rough * rough;
        for i in 0..n {
            let nov = (i as f32 + 0.5) / n as f32;
            let v = Vec3::new((1.0 - nov * nov).sqrt(), 0.0, nov);
            let (mut sa, mut sb) = (0.0, 0.0);
            const N: u32 = 256;
            for k in 0..N {
                let h = ggx_half(hammersley(k, N), Vec3::Z, a);
                let l = h * (2.0 * v.dot(h)) - v;
                let (nol, noh, voh) = (l.z.max(0.0), h.z.max(0.0), v.dot(h).max(0.0));
                if nol > 0.0 {
                    // Smith-GGX (height-correlated form of Karis, IBL k).
                    let kk = a / 2.0;
                    let g = |x: f32| x / (x * (1.0 - kk) + kk);
                    let vis = g(nov) * g(nol) * voh / (noh * nov).max(1e-6);
                    let fc = (1.0 - voh).powi(5);
                    sa += (1.0 - fc) * vis;
                    sb += fc * vis;
                }
            }
            out.push([sa / N as f32, sb / N as f32]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions_round_trip() {
        for (u, v) in [(0.5, 0.5), (0.25, 0.3), (0.9, 0.75), (0.01, 0.5)] {
            let (u2, v2) = uv_of(dir_of(u, v));
            assert!((u - u2).abs() < 1e-4 && (v - v2).abs() < 1e-4, "{u},{v}");
        }
        assert!(dir_of(0.5, 0.5).distance(-Vec3::Z) < 1e-5);
        assert!(dir_of(0.75, 0.5).distance(Vec3::X) < 1e-5);
        assert!(dir_of(0.3, 0.0).distance(Vec3::Y) < 1e-5);
        // Cube faces point where they say.
        for (f, want) in [
            (0, Vec3::X),
            (1, -Vec3::X),
            (2, Vec3::Y),
            (3, -Vec3::Y),
            (4, Vec3::Z),
            (5, -Vec3::Z),
        ] {
            assert!(cube_dir(f, 1.5, 1.5, 4).distance(want) < 1e-5);
        }
    }

    #[test]
    fn a_uniform_sky_stays_uniform() {
        let eq = Equirect::new(64, 32, |_| [0.7, 0.5, 0.3]);
        let cube = prefilter(&eq);
        for mip in &cube {
            for face in mip {
                for t in face {
                    assert!(
                        (t[0] - 0.7).abs() < 1e-3 && (t[2] - 0.3).abs() < 1e-3,
                        "{t:?}"
                    );
                }
            }
        }
        // Irradiance from a uniform sky of radiance L is π L.
        let sh = irradiance_sh(&eq);
        for n in [
            Vec3::Y,
            -Vec3::Y,
            Vec3::X,
            Vec3::new(0.3, -0.5, 0.8).normalize(),
        ] {
            let e = sh_eval(&sh, n);
            assert!((e - Vec3::new(0.7, 0.5, 0.3) * PI).length() < 0.02, "{e}");
        }
    }

    #[test]
    fn rougher_mips_are_blurrier_and_keep_their_energy() {
        // A bright band round the horizon, dark above and below.
        let eq = Equirect::new(
            128,
            64,
            |d| if d.y.abs() < 0.2 { [4.0; 3] } else { [0.0; 3] },
        );
        let cube = prefilter(&eq);
        let at = |mip: usize, face: u32, y: f32| {
            let size = (CUBE_SIZE >> mip) as f32;
            cube[mip][face as usize][((y * size) as usize) * size as usize + size as usize / 2][0]
        };
        // On a side face: the horizon row is bright, the top is dark, less
        // and less so with roughness.
        let contrast: Vec<f32> = (0..CUBE_MIPS as usize)
            .map(|m| at(m, 4, 0.5) - at(m, 4, 0.02))
            .collect();
        for w in contrast.windows(2) {
            assert!(w[1] <= w[0] + 1e-3, "{contrast:?}");
        }
        assert!(contrast[0] > 3.0 && contrast[5] < 2.0, "{contrast:?}");
        // The average over the sphere hardly changes with blurring.
        let mean = |m: usize| {
            let size = CUBE_SIZE >> m;
            let mut s = 0.0;
            for face in 0..6u32 {
                for y in 0..size {
                    for x in 0..size {
                        let d = cube_dir(face, x as f32, y as f32, size);
                        // Texel solid angle ∝ (1 + s² + t²)^-1.5 = |d_z|³.
                        let w = d.abs().max_element().powi(3);
                        s += cube[m][face as usize][(y * size + x) as usize][0] * w;
                    }
                }
            }
            s
        };
        let base = mean(0);
        for m in 1..CUBE_MIPS as usize {
            let e = mean(m) * 4f32.powi(m as i32);
            assert!((e / base - 1.0).abs() < 0.05, "mip {m}: {} vs {base}", e);
        }
    }

    #[test]
    fn the_brdf_table_is_sensible() {
        let lut = brdf_lut();
        let at = |nov: f32, rough: f32| {
            let i = ((nov * LUT_SIZE as f32) as u32).min(LUT_SIZE - 1);
            let j = ((rough * LUT_SIZE as f32) as u32).min(LUT_SIZE - 1);
            lut[(j * LUT_SIZE + i) as usize]
        };
        // Smooth and facing: nearly all reflected, no Fresnel bias.
        let [a, b] = at(1.0, 0.0);
        assert!(a > 0.9 && b < 0.05, "{a} {b}");
        // Grazing: Fresnel brightens (bias grows).
        assert!(at(0.05, 0.1)[1] > at(1.0, 0.1)[1]);
        // Never more than all the light.
        assert!(lut.iter().all(|[a, b]| a + b <= 1.05));
    }

    #[test]
    fn the_sun_is_found_and_taken_out() {
        let mut eq = studio(Studio::Sunset);
        let sun = extract_sun(&mut eq).expect("a sunset has a sun");
        assert!(
            sun.dir.distance(Vec3::new(0.0, 0.08, -1.0).normalize()) < 0.05,
            "{:?}",
            sun.dir
        );
        assert!(sun.light.x > sun.light.z, "a warm sun: {:?}", sun.light);
        let peak = eq.px.iter().map(|p| lum(*p)).fold(0.0, f32::max);
        assert!(peak < 10.0, "still a bright spot: {peak}");
        // An even sky has none.
        let mut flat = studio(Studio::Overcast);
        assert!(extract_sun(&mut flat).is_none());
    }

    #[test]
    fn hdr_files_load() {
        // Encode a small panorama and read it back.
        let img =
            image::Rgb32FImage::from_fn(8, 4, |x, y| image::Rgb([x as f32 * 0.5, y as f32, 2.0]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb32F(img)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Hdr,
            )
            .unwrap();
        let eq = load_hdr(&bytes).unwrap();
        assert_eq!((eq.w, eq.h), (8, 4));
        assert!((eq.px[3][0] - 1.5).abs() < 0.05 && (eq.px[3][2] - 2.0).abs() < 0.05);
        assert!(load_hdr(b"not an image").is_err());
    }
}

#[cfg(test)]
mod timing {
    #[test]
    #[ignore]
    fn time_prefilter() {
        let t = std::time::Instant::now();
        let eq = super::studio(ez_core::Studio::Softbox);
        let a = t.elapsed();
        let _ = super::prefilter(&eq);
        let b = t.elapsed();
        let _ = super::irradiance_sh(&eq);
        let _ = super::brdf_lut();
        println!(
            "studio {a:?}, prefilter {:?}, sh+lut {:?}",
            b - a,
            t.elapsed() - b
        );
    }
}
