//! Cloth: a sheet of particles held together by distance constraints
//! (position-based dynamics, XPBD), blown by a wind that loops.

use super::math;
use super::{Body, Sim, SimLoop};
use crate::{EvalCtx, Param};
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Most particles along a side (a bake keeps every one at every key).
pub const CLOTH_MAX_DETAIL: u32 = 48;
/// Simulation substeps per step (small steps, one constraint pass each).
const SUBSTEPS: usize = 6;

/// What the cloth is and where it hangs from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClothKind {
    /// Upright, held along one side by a pole.
    #[default]
    Flag,
    /// Upright, held along the top.
    Curtain,
    /// Upright, held by its two top corners.
    Banner,
    /// Flat, dropped over a ball: a tablecloth or a sheet over a ghost.
    Drape,
}

impl ClothKind {
    pub const ALL: [ClothKind; 4] = [
        ClothKind::Flag,
        ClothKind::Curtain,
        ClothKind::Banner,
        ClothKind::Drape,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ClothKind::Flag => "Flag",
            ClothKind::Curtain => "Curtain",
            ClothKind::Banner => "Banner",
            ClothKind::Drape => "Drape over a ball",
        }
    }
}

/// A cloth's settings (the shape source "Cloth").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Cloth {
    pub kind: ClothKind,
    /// Width and height (for a drape: width and depth), in units.
    pub size: [f32; 2],
    /// Particles across (up to [`CLOTH_MAX_DETAIL`]); down follows the
    /// shape.
    pub detail: u32,
    /// Resistance to folding (0 = silk, 1 = canvas).
    pub stiffness: f32,
    /// How fast motion dies down (0..1).
    pub damping: f32,
    /// Wind speed (units per second; animatable, music-linkable).
    pub wind: Param,
    /// Where the wind blows to, in degrees around the vertical (0 = along
    /// +x; an oscillator turns it whole cycles per loop).
    pub wind_direction: Param,
    /// Gusts: how much the wind varies over the cloth and the loop.
    pub gusts: f32,
    /// Stop falling at the floor (the layer's y = `floor`).
    pub floor_on: bool,
    pub floor: f32,
    /// The ball a drape falls on (centre from the layer's origin, radius).
    pub ball: [f32; 4],
    pub seed: u32,
    /// How the loop is closed.
    pub looping: SimLoop,
}

impl Default for Cloth {
    fn default() -> Self {
        Cloth {
            kind: ClothKind::Flag,
            size: [3.0, 2.0],
            detail: 24,
            stiffness: 0.3,
            damping: 0.1,
            wind: Param::new(8.0),
            wind_direction: Param::new(0.0),
            gusts: 0.5,
            floor_on: false,
            floor: -2.0,
            ball: [0.0, -1.0, 0.0, 1.0],
            seed: 1,
            // A cloth in a steady rhythm of wind settles into a cycle:
            // warm up into it, then steer back over a quarter loop.
            looping: SimLoop {
                warmup: 3,
                ..SimLoop::default()
            },
        }
    }
}

impl Cloth {
    /// Particles across and down.
    pub fn grid(&self) -> (u32, u32) {
        let cols = self.detail.clamp(4, CLOTH_MAX_DETAIL);
        let [w, h] = self.size.map(|v| v.abs().max(0.01));
        let rows = ((cols as f32 * h / w).round() as u32).clamp(2, CLOTH_MAX_DETAIL);
        (cols, rows)
    }

    /// Whether the music changes the cloth's motion.
    pub fn uses_music(&self) -> bool {
        self.wind.uses_music() || self.wind_direction.uses_music()
    }

    /// How far the cloth can reach from the layer's origin.
    pub fn reach(&self) -> f32 {
        let [w, h] = self.size.map(f32::abs);
        let sheet = (w * w + h * h).sqrt();
        match self.kind {
            // Hung from one side or the top: it can swing all around.
            ClothKind::Flag | ClothKind::Curtain | ClothKind::Banner => sheet * 1.1,
            ClothKind::Drape => {
                let ball = Vec3::new(self.ball[0], self.ball[1], self.ball[2]).length();
                let floor = if self.floor_on { self.floor.abs() } else { 0.0 };
                sheet + ball + floor + self.ball[3].abs()
            }
        }
    }

    /// Where particle (column `i`, row `j`) starts, and whether it is
    /// pinned there.
    fn rest(&self, i: u32, j: u32) -> (Vec3, bool) {
        let (cols, rows) = self.grid();
        let u = i as f32 / (cols - 1) as f32;
        let v = j as f32 / (rows - 1) as f32;
        let [w, h] = self.size.map(|s| s.abs().max(0.01));
        match self.kind {
            // Row 0 is the top; the pole is column 0 at x = 0.
            ClothKind::Flag => (Vec3::new(u * w, h * 0.5 - v * h, 0.0), i == 0),
            ClothKind::Curtain => (Vec3::new((u - 0.5) * w, h * 0.5 - v * h, 0.0), j == 0),
            ClothKind::Banner => (
                Vec3::new((u - 0.5) * w, h * 0.5 - v * h, 0.0),
                j == 0 && (i == 0 || i == cols - 1),
            ),
            // Flat above the ball, a little off centre so it slides.
            ClothKind::Drape => {
                let top = self.ball[1] + self.ball[3].abs() + 0.3;
                (
                    Vec3::new((u - 0.5) * w + 0.07, top, (v - 0.5) * h - 0.05),
                    false,
                )
            }
        }
    }

    /// The simulation, ready to bake.
    pub fn sim(&self) -> ClothSim {
        let (cols, rows) = self.grid();
        let mut bodies = Vec::with_capacity((cols * rows) as usize);
        let mut pinned = Vec::with_capacity(bodies.capacity());
        for j in 0..rows {
            for i in 0..cols {
                let (p, pin) = self.rest(i, j);
                bodies.push(Body::at(p));
                pinned.push(pin);
            }
        }
        // Distance constraints in a fixed order: structure (neighbours),
        // shear (diagonals), bending (every other particle).
        let (w, h) = (cols as usize, rows as usize);
        let at = |i: usize, j: usize| j * w + i;
        // Compliance 1e-2 (silk) to 1e-6 (canvas), as 10^x by `math::exp`.
        let bend =
            math::exp((-2.0 - 4.0 * self.stiffness.clamp(0.0, 1.0)) * std::f32::consts::LN_10);
        let mut links = Vec::new();
        let mut link = |a: usize, b: usize, compliance: f32| {
            let rest = bodies[a].pos.distance(bodies[b].pos);
            links.push(Link {
                a: a as u32,
                b: b as u32,
                rest,
                compliance,
            });
        };
        for j in 0..h {
            for i in 0..w {
                if i + 1 < w {
                    link(at(i, j), at(i + 1, j), 0.0);
                }
                if j + 1 < h {
                    link(at(i, j), at(i, j + 1), 0.0);
                }
                if i + 1 < w && j + 1 < h {
                    link(at(i, j), at(i + 1, j + 1), 1e-6);
                    link(at(i + 1, j), at(i, j + 1), 1e-6);
                }
                if i + 2 < w {
                    link(at(i, j), at(i + 2, j), bend);
                }
                if j + 2 < h {
                    link(at(i, j), at(i, j + 2), bend);
                }
            }
        }
        ClothSim {
            set: self.clone(),
            cols: w,
            rows: h,
            rest: bodies.iter().map(|b| b.pos).collect(),
            bodies,
            pinned,
            links,
            prev: Vec::new(),
            normals: Vec::new(),
        }
    }
}

/// A distance constraint.
struct Link {
    a: u32,
    b: u32,
    rest: f32,
    /// XPBD compliance (0 = stiff).
    compliance: f32,
}

/// A cloth being simulated.
pub struct ClothSim {
    set: Cloth,
    cols: usize,
    rows: usize,
    bodies: Vec<Body>,
    rest: Vec<Vec3>,
    pinned: Vec<bool>,
    links: Vec<Link>,
    prev: Vec<Vec3>,
    normals: Vec<Vec3>,
}

/// Smooth value noise in -1..1 (arithmetic only: the same everywhere).
fn noise(p: Vec3) -> f32 {
    let f = p.floor();
    let t = p - f;
    let s = t * t * (Vec3::splat(3.0) - t * 2.0);
    let c = [f.x as i32, f.y as i32, f.z as i32];
    let h = |dx: i32, dy: i32, dz: i32| {
        let k = (c[0] + dx) as u32
            ^ ((c[1] + dy) as u32).wrapping_mul(0x9e37_79b9)
            ^ ((c[2] + dz) as u32).wrapping_mul(0x85eb_ca6b);
        crate::rng::hash_u32(k) as f32 / u32::MAX as f32 * 2.0 - 1.0
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(h(0, 0, 0), h(1, 0, 0), s.x);
    let x10 = lerp(h(0, 1, 0), h(1, 1, 0), s.x);
    let x01 = lerp(h(0, 0, 1), h(1, 0, 1), s.x);
    let x11 = lerp(h(0, 1, 1), h(1, 1, 1), s.x);
    lerp(lerp(x00, x10, s.y), lerp(x01, x11, s.y), s.z)
}

impl ClothSim {
    /// Per particle, the sheet's normal (from its neighbours).
    fn update_normals(&mut self) {
        let (w, h) = (self.cols, self.rows);
        self.normals.clear();
        for j in 0..h {
            for i in 0..w {
                let p = |i: usize, j: usize| self.bodies[j * w + i].pos;
                let dx = p((i + 1).min(w - 1), j) - p(i.saturating_sub(1), j);
                let dy = p(i, (j + 1).min(h - 1)) - p(i, j.saturating_sub(1));
                self.normals.push(dy.cross(dx).normalize_or(Vec3::Z));
            }
        }
    }

    /// Pushes `p` out of the ball and the floor; where it touches, it
    /// grips (friction: most of the sliding since `prev` is undone).
    fn collide(&self, p: Vec3, prev: Vec3) -> Vec3 {
        const GRIP: f32 = 0.8;
        let s = &self.set;
        let mut p = p;
        if s.kind == ClothKind::Drape {
            let c = Vec3::new(s.ball[0], s.ball[1], s.ball[2]);
            // A hair outside, so the sheet doesn't sink into the ball.
            let r = s.ball[3].abs() + 0.02;
            let d = p - c;
            let l = d.length();
            if l < r {
                let n = if l > 1e-6 { d / l } else { Vec3::Y };
                p = c + n * r;
                let slide = (p - prev) - n * (p - prev).dot(n);
                p -= slide * GRIP;
            }
        }
        if s.floor_on && p.y < s.floor {
            p.y = s.floor;
            let slide = Vec3::new(p.x - prev.x, 0.0, p.z - prev.z);
            p -= slide * GRIP;
        }
        p
    }
}

impl Sim for ClothSim {
    fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    fn bodies_mut(&mut self) -> &mut [Body] {
        &mut self.bodies
    }

    fn step(&mut self, dt: f32, ctx: &EvalCtx) {
        let s = &self.set;
        let speed = s.wind.eval(ctx);
        let (sd, cd) = math::sin_cos(s.wind_direction.eval(ctx).to_radians());
        let wind = Vec3::new(cd, 0.0, -sd) * speed;
        // Gusts: noise travelling a circle in noise space over the loop,
        // so they repeat every loop.
        let (sa, ca) = math::sin_cos(ctx.phase * std::f32::consts::TAU);
        let drift = Vec3::new(ca * 2.0, sa * 2.0, s.seed as f32 * 7.31);
        let gusts = s.gusts;
        let keep = 1.0 - s.damping.clamp(0.0, 1.0) * 0.5 * dt;
        self.update_normals();
        let h = dt / SUBSTEPS as f32;
        let ah = 1.0 / (h * h);
        for _ in 0..SUBSTEPS {
            self.prev.clear();
            self.prev.extend(self.bodies.iter().map(|b| b.pos));
            for (k, b) in self.bodies.iter_mut().enumerate() {
                if self.pinned[k] {
                    continue;
                }
                // Turbulence: the wind varies in strength and swirls a
                // little sideways (which is what sets a flag flapping).
                let q = b.pos * 0.35 + drift;
                let swirl = Vec3::new(
                    noise(q + Vec3::new(31.7, 0.0, 0.0)),
                    noise(q + Vec3::new(0.0, 57.3, 0.0)) * 0.5,
                    noise(q + Vec3::new(0.0, 0.0, 83.1)),
                );
                let gust = wind * (1.0 + gusts * noise(q)) + swirl * (gusts * speed.abs() * 0.6);
                let air = gust - b.vel;
                let n = self.normals[k];
                // Air pushes on the sheet across it, and drags it along
                // (standing for the drag of the flapping, which is what
                // streams a flag out), both growing with the square of the
                // speed, as air does.
                let across = air.dot(n);
                let acc = Vec3::new(0.0, -9.8, 0.0)
                    + n * (across * across.abs() * 0.4)
                    + air * (air.length() * 0.2);
                b.vel = (b.vel + acc * h) * keep;
                b.pos += b.vel * h;
            }
            // XPBD: one pass over the constraints per substep.
            for l in &self.links {
                let (a, b) = (l.a as usize, l.b as usize);
                let wa = if self.pinned[a] { 0.0 } else { 1.0 };
                let wb = if self.pinned[b] { 0.0 } else { 1.0 };
                let sum = wa + wb + l.compliance * ah;
                if sum <= 0.0 {
                    continue;
                }
                let d = self.bodies[b].pos - self.bodies[a].pos;
                let len = d.length();
                if len < 1e-9 {
                    continue;
                }
                let dl = -(len - l.rest) / sum;
                let dir = d / len;
                self.bodies[a].pos -= dir * (dl * wa);
                self.bodies[b].pos += dir * (dl * wb);
            }
            for k in 0..self.bodies.len() {
                if self.pinned[k] {
                    self.bodies[k].pos = self.rest[k];
                    self.bodies[k].vel = Vec3::ZERO;
                    continue;
                }
                let p = self.collide(self.bodies[k].pos, self.prev[k]);
                self.bodies[k].pos = p;
                self.bodies[k].vel = (p - self.prev[k]) / h;
            }
        }
    }

    /// Pinned particles stay put while the loop closes.
    fn guide(&mut self, targets: &[Body], gain: f32, dt: f32, drift: &mut [Vec3]) {
        super::pull(&mut self.bodies, targets, gain, dt, drift);
        for (k, b) in self.bodies.iter_mut().enumerate() {
            if self.pinned[k] {
                b.pos = self.rest[k];
                b.vel = Vec3::ZERO;
                drift[k] = Vec3::ZERO;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Bake, BakeJob, Frame, KeyHasher, SimClock};
    use crate::{Timing, Wave};

    fn clock() -> SimClock {
        SimClock::new(Timing {
            bpm: 120.0,
            loop_beats: 8,
        })
    }

    fn bake(c: &Cloth) -> Bake {
        BakeJob::new(Box::new(c.sim()), clock(), c.looping.clone()).finish()
    }

    fn at(b: &Bake, phase: f32) -> Vec<Body> {
        let mut f = Frame::default();
        b.sample(phase, &mut f);
        f.layers.swap_remove(0).bodies
    }

    #[test]
    fn a_flag_loops_and_keeps_its_pole() {
        let c = Cloth::default();
        let b = bake(&c);
        let (cols, rows) = c.grid();
        let (start, end) = (at(&b, 0.0), at(&b, 1.0 - 1e-5));
        for (a, z) in start.iter().zip(&end) {
            assert!(a.pos.distance(z.pos) < 1e-3);
        }
        for k in 0..32 {
            let bodies = at(&b, k as f32 / 32.0);
            for j in 0..rows {
                let (want, pinned) = c.rest(0, j);
                assert!(pinned);
                assert!(bodies[(j * cols) as usize].pos.distance(want) < 1e-5);
            }
        }
        eprintln!(
            "flag seam {:?} after {} warm-up loops",
            b.seam(),
            b.warmup_loops()
        );
    }

    #[test]
    fn cloth_stretches_little() {
        for kind in ClothKind::ALL {
            let c = Cloth {
                kind,
                floor_on: true,
                ..Cloth::default()
            };
            let sim = c.sim();
            let rest: Vec<(u32, u32, f32)> = sim
                .links
                .iter()
                .filter(|l| l.compliance == 0.0)
                .map(|l| (l.a, l.b, l.rest))
                .collect();
            let b = bake(&c);
            for k in 0..16 {
                let bodies = at(&b, k as f32 / 16.0);
                let worst = rest
                    .iter()
                    .map(|&(a, z, r)| bodies[a as usize].pos.distance(bodies[z as usize].pos) / r)
                    .fold(0.0, f32::max);
                assert!(worst < 1.15, "{kind:?} stretched {worst} at {k}/16");
            }
        }
    }

    #[test]
    fn a_drape_rests_on_its_ball_above_the_floor() {
        let c = Cloth {
            kind: ClothKind::Drape,
            size: [3.0, 3.0],
            floor_on: true,
            floor: -2.0,
            wind: Param::new(1.0),
            ..Cloth::default()
        };
        let b = bake(&c);
        let centre = Vec3::new(c.ball[0], c.ball[1], c.ball[2]);
        for k in 0..8 {
            let bodies = at(&b, k as f32 / 8.0);
            for p in &bodies {
                assert!(p.pos.y >= c.floor - 1e-3, "under the floor: {}", p.pos);
                assert!(
                    p.pos.distance(centre) >= c.ball[3] - 0.01,
                    "in the ball: {}",
                    p.pos
                );
            }
            // It hangs over the ball: the middle is on top, the corners low.
            let (cols, rows) = c.grid();
            let mid = bodies[((rows / 2) * cols + cols / 2) as usize].pos;
            assert!(mid.y > centre.y + c.ball[3] * 0.8, "middle at {mid}");
            assert!(bodies[0].pos.y < mid.y - 0.5);
        }
    }

    #[test]
    fn the_flag_flutters_downwind() {
        let turned = Cloth {
            wind_direction: Param::new(90.0),
            ..Cloth::default()
        };
        let (cols, rows) = turned.grid();
        let tip = ((rows / 2) * cols + cols - 1) as usize;
        let b = bake(&Cloth::default());
        let t = bake(&turned);
        let (a, z) = (at(&b, 0.3), at(&b, 0.55));
        assert!(
            a[tip].pos.distance(z[tip].pos) > 0.05,
            "the flag doesn't move"
        );
        // Wind along +x streams the flag out along +x; turned 90°, along -z.
        assert!(a[tip].pos.x > 1.5, "{}", a[tip].pos);
        let tt = at(&t, 0.3)[tip].pos;
        assert!(tt.z < -1.5 && tt.x.abs() < 1.5, "{tt}");
    }

    #[test]
    fn gusts_loop() {
        let drift = |phase: f32| {
            let (sa, ca) = math::sin_cos(phase * std::f32::consts::TAU);
            Vec3::new(ca * 2.0, sa * 2.0, 7.31)
        };
        for p in [Vec3::ZERO, Vec3::new(1.3, -0.4, 2.2)] {
            assert!((noise(p + drift(0.0)) - noise(p + drift(1.0))).abs() < 1e-5);
            assert!((noise(p + drift(0.0)) - noise(p + drift(0.3))).abs() > 1e-4);
        }
        let n: Vec<f32> = (0..1000)
            .map(|i| noise(Vec3::splat(i as f32 * 0.137)))
            .collect();
        assert!(n.iter().all(|v| v.abs() <= 1.0));
    }

    /// A cloth bakes the same on every platform (see `same_bake_everywhere`
    /// in `bake.rs`); update only when cloth is meant to change.
    #[test]
    fn same_cloth_everywhere() {
        let c = Cloth {
            detail: 12,
            wind: Param::new(5.0).osc(Wave::Sine, 2.0, 2),
            wind_direction: Param::new(0.0).osc(Wave::Saw, 180.0, 1),
            ..Cloth::default()
        };
        let hash = bake(&c)
            .raw()
            .iter()
            .fold(KeyHasher::new(), |h, v| h.f32(*v))
            .finish();
        assert_eq!(hash, 0x66e6_be60_8510_85a4, "cloth hash {hash:#018x}");
    }
}

#[cfg(test)]
mod saving {
    use crate::*;

    #[test]
    fn cloth_saves_and_loads() {
        let p = presets::banners();
        let json = p.to_json();
        assert!(json.contains("\"Cloth\"") && !json.contains("\"mesh\""));
        assert_eq!(Project::from_json(&json).unwrap(), p);
    }
}
