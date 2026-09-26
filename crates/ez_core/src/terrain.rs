//! The terrain's height field on the CPU: a line-for-line port of
//! `t_height` in `terrain.wgsl`, so copies can stand on the landscape the
//! GPU draws.

use crate::clock::EvalCtx;
use crate::scene::{Terrain, TerrainShape};
use glam::Vec3;
use std::f32::consts::TAU;

fn hash_u(x_in: u32) -> u32 {
    let mut x = x_in;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn hash1(x: u32) -> f32 {
    (hash_u(x) >> 8) as f32 / 16_777_216.0
}

fn t_hash(x: i32, y: i32, seed: u32) -> f32 {
    hash1(
        hash_u((x as u32).wrapping_mul(0x8da6_b343))
            ^ hash_u((y as u32).wrapping_mul(0xd816_3841))
            ^ seed,
    )
}

fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Value noise repeating every `period` cells.
fn t_noise(px: f32, py: f32, period: i32, seed: u32) -> f32 {
    let (ix, iy) = (px.floor() as i32, py.floor() as i32);
    let (fx, fy) = (px - px.floor(), py - py.floor());
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let x0 = ((ix % period) + period) % period;
    let y0 = ((iy % period) + period) % period;
    let x1 = (x0 + 1) % period;
    let y1 = (y0 + 1) % period;
    let a = t_hash(x0, y0, seed);
    let b = t_hash(x1, y0, seed);
    let c = t_hash(x0, y1, seed);
    let d = t_hash(x1, y1, seed);
    let ab = a + (b - a) * sx;
    let cd = c + (d - c) * sx;
    ab + (cd - ab) * sy
}

fn t_fbm(u: f32, w: f32, period_in: i32, rough: f32, seed: u32, octaves: i32) -> f32 {
    let (mut sum, mut amp, mut norm, mut period) = (0.0, 1.0, 0.0, period_in);
    for o in 0..octaves {
        let p = period as f32;
        sum += t_noise(u * p, w * p, period, seed.wrapping_add(o as u32 * 101)) * amp;
        norm += amp;
        amp *= rough;
        period *= 2;
    }
    sum / f32::max(norm, 1e-4)
}

fn t_ridged(u: f32, w: f32, period_in: i32, rough: f32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm, mut period, mut prev) = (0.0, 1.0, 0.0, period_in, 1.0);
    for o in 0..4 {
        let p = period as f32;
        let mut r = 1.0
            - (t_noise(u * p, w * p, period, seed.wrapping_add(o as u32 * 101)) * 2.0 - 1.0).abs();
        r *= r;
        sum += r * amp * prev;
        prev = 1.0 + (r - 1.0) * 0.6;
        norm += amp;
        amp *= rough.max(0.25);
        period *= 2;
    }
    sum / f32::max(norm, 1e-4)
}

fn t_craters(u: f32, w: f32, period: i32, seed: u32) -> f32 {
    let (px, py) = (u * period as f32, w * period as f32);
    let (cx0, cy0) = (px.floor(), py.floor());
    let mut h = 0.0;
    for j in -1..=1 {
        for i in -1..=1 {
            let (cellx, celly) = (cx0 + i as f32, cy0 + j as f32);
            let cx = ((cellx as i32 % period) + period) % period;
            let cy = ((celly as i32 % period) + period) % period;
            let r0 = t_hash(cx, cy, seed.wrapping_add(7));
            if r0 < 0.25 {
                continue;
            }
            let centre_x = cellx + t_hash(cx, cy, seed.wrapping_add(11)) * 0.6 + 0.2;
            let centre_y = celly + t_hash(cx, cy, seed.wrapping_add(13)) * 0.6 + 0.2;
            let rad = 0.2 + 0.45 * t_hash(cx, cy, seed.wrapping_add(17));
            let d = ((px - centre_x).powi(2) + (py - centre_y).powi(2)).sqrt() / rad;
            let bowl = if d < 1.0 { (d * d - 1.0) * 0.7 } else { 0.0 };
            let rim = (-(d - 1.0) * (d - 1.0) * 14.0).exp() * 0.35;
            h += (bowl + rim) * rad;
        }
    }
    h
}

/// Settings of a terrain at one moment, as the renderer sends them.
#[derive(Clone, Copy, Debug)]
pub struct Ground {
    pub size: f32,
    pub cells: u32,
    pub height: f32,
    hills: i32,
    rough: f32,
    valley: f32,
    seed: u32,
    shape: TerrainShape,
    /// Scroll offset (0..1 of the terrain).
    pub scroll: f32,
    /// Liquid surface height (local units), when there is a liquid.
    pub liquid: Option<f32>,
}

impl Ground {
    pub fn at(t: &Terrain, ctx: &EvalCtx) -> Ground {
        let height = t.height.eval(ctx);
        Ground {
            size: t.size.max(1.0),
            cells: t.cells.clamp(4, t.max_cells()),
            height,
            hills: t.hills.clamp(1, 64) as i32,
            rough: t.roughness.eval(ctx).clamp(0.0, 1.0),
            valley: t.valley.eval(ctx).clamp(0.0, 1.0),
            seed: t.seed % 65536,
            shape: t.shape,
            scroll: (ctx.phase * t.scroll as f32).rem_euclid(1.0),
            liquid: (t.liquid.kind.index() > 0).then(|| t.liquid.level.eval(ctx) * height),
        }
    }

    /// Ground height at noise coordinates (u across, w along, 0..1).
    pub fn height(&self, u: f32, w: f32) -> f32 {
        let (hills, rough, seed) = (self.hills, self.rough, self.seed);
        let mut h = match self.shape {
            TerrainShape::Mountains => t_ridged(u, w, hills, rough, seed).powf(1.6) * 1.5,
            TerrainShape::Mesas => {
                let f = t_fbm(u, w, hills, rough * 0.6, seed, 3);
                let q = f * f * 1.6 * 5.0;
                let h = (q.floor() + smoothstep(0.65, 1.0, fract(q))) / 5.0;
                h + t_fbm(u, w, hills * 8, 0.5, seed.wrapping_add(3), 1) * 0.03 * rough
            }
            TerrainShape::Dunes => {
                let warp = t_fbm(u, w, hills, 0.5, seed, 2);
                let bend = (u * TAU * hills as f32).sin() * 0.6
                    + ((u + w) * TAU * (hills * 2) as f32).sin() * 0.25;
                let x = w * (hills * 3) as f32 + warp * 4.0 + bend;
                let f = fract(x);
                let crest = if f < 0.75 { f / 0.75 } else { (1.0 - f) / 0.25 };
                let prof = smoothstep(0.0, 1.0, crest);
                let h = prof * (0.2 + 0.6 * t_fbm(u, w, hills * 2, 0.5, seed.wrapping_add(5), 2));
                h + t_fbm(u, w, hills * 4, rough, seed.wrapping_add(9), 2) * 0.12 * rough
            }
            TerrainShape::Canyons => {
                let f = t_fbm(u, w, hills, 0.45, seed, 3);
                let river = (f - 0.5).abs();
                let cut = smoothstep(0.015, 0.09 + 0.05 * rough, river);
                let q = cut * 3.0;
                let steps = (q.floor() + smoothstep(0.6, 1.0, fract(q))) / 3.0;
                0.05 + 0.85 * steps
                    + t_fbm(u, w, hills * 4, 0.5, seed.wrapping_add(3), 2) * 0.12 * rough
            }
            TerrainShape::Craters => {
                let f = t_fbm(u, w, hills, rough, seed, 3);
                (0.35 + f * 0.35 + t_craters(u, w, hills * 2, seed)).max(0.0)
            }
            TerrainShape::Hills => {
                let f = t_fbm(u, w, hills, rough, seed, 4);
                f * f * 1.6
            }
        };
        if self.valley > 0.0 {
            let x = (u - 0.5).abs() * 2.0;
            h *= smoothstep(self.valley * 0.5, self.valley * 0.5 + 0.35, x);
        }
        h * self.height
    }

    /// Where a point fixed to the landscape at noise coordinates (u, w)
    /// is now, in the terrain's own space: position on the surface (on the
    /// liquid where it is flooded), surface normal, and how far it is from
    /// the terrain's edges (0 at an edge, 1 well inside).
    pub fn point(&self, u: f32, w: f32) -> (Vec3, Vec3, f32) {
        let h = self.height(u, w);
        let n = self.cells as f32;
        let e = 1.0 / n;
        let hx = self.height(u + e, w) - self.height(u - e, w);
        let hz = self.height(u, w + e) - self.height(u, w - e);
        let step = self.size / n;
        let mut normal = Vec3::new(-hx, 2.0 * step, -hz).normalize_or(Vec3::Y);
        let mut top = h;
        if let Some(level) = self.liquid {
            if h < level {
                top = level;
                normal = Vec3::Y;
            }
        }
        // The landscape slides towards +z and wraps.
        let z = (w + self.scroll).rem_euclid(1.0);
        let pos = Vec3::new((u - 0.5) * self.size, top + 0.03, (z - 0.5) * self.size);
        let edge = u.min(1.0 - u).min(z).min(1.0 - z);
        (pos, normal, smoothstep(0.0, 0.06, edge))
    }
}
