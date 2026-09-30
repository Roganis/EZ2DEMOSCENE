//! Flocking: boids that keep apart, fly together and stay near a target.

use super::math::{self, Rot};
use super::{Body, Sim, SimLoop};
use crate::rng::Rng;
use crate::{EvalCtx, Param, RibbonCurve};
use glam::Vec3;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Most boids in a flock (a bake keeps every one at every key).
pub const FLOCK_MAX: u32 = 2000;
/// A guided tail may steer this many times harder than `agility`.
const GUIDE_CAP: f32 = 6.0;
/// Keeping apart may steer this many times harder than `agility` (more
/// than the guide, so closing the loop doesn't press boids together).
const APART_CAP: f32 = 8.0;

/// A closed curve the flock's target travels along.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct FlockPath {
    pub curve: RibbonCurve,
    pub freq: [u32; 3],
    /// Size of the curve (world units, like a ribbon of scale 1).
    pub size: f32,
    /// Whole trips around the curve per loop.
    pub laps: i32,
}

impl Default for FlockPath {
    fn default() -> Self {
        FlockPath {
            curve: RibbonCurve::Knot,
            freq: [2, 3, 5],
            size: 5.0,
            laps: 1,
        }
    }
}

/// A flock's settings (the copy layout "Flock").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Flock {
    /// Number of boids (up to [`FLOCK_MAX`]).
    pub count: u32,
    pub seed: u32,
    /// Cruising speed, in units per second.
    pub speed: Param,
    /// Distance boids keep from each other.
    pub spacing: f32,
    /// How far a boid sees its neighbours.
    pub sight: f32,
    /// Steering weights: keep apart, fly the same way, stay together.
    pub separation: f32,
    pub alignment: f32,
    pub cohesion: f32,
    /// How hard a boid can steer, in units per second squared.
    pub agility: f32,
    /// Boids stay within this distance of the target.
    pub radius: f32,
    /// Where the flock gathers, from the layer's origin (animatable and
    /// music-linkable: a moving target leads the flock).
    pub target: [Param; 3],
    /// The target also travels a closed curve.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<FlockPath>,
    /// Push away from the target (link it to kicks to scatter on hits).
    pub scatter: Param,
    /// Lean into turns (0 = never, 1 = like a bird).
    pub bank: f32,
    /// How firmly each boid keeps to its own place in the flock (a point
    /// circling the target a whole number of times per loop). Flocking
    /// alone never repeats; holding a formation makes the flight settle
    /// into a loop, so it closes without a visible catch-up. 0 = free.
    pub formation: f32,
    /// How the loop is closed.
    pub looping: SimLoop,
}

impl Default for Flock {
    fn default() -> Self {
        Flock {
            count: 300,
            seed: 1,
            speed: Param::new(3.0),
            spacing: 0.6,
            sight: 1.6,
            separation: 1.0,
            alignment: 1.0,
            cohesion: 1.0,
            agility: 12.0,
            radius: 5.0,
            target: [Param::new(0.0), Param::new(0.0), Param::new(0.0)],
            path: None,
            scatter: Param::new(0.0),
            bank: 0.7,
            formation: 0.8,
            // A long tail: the flock steers back to its start gently. One
            // loop of warm-up lets it gather; holding a formation, more
            // changes little.
            looping: SimLoop {
                blend: 0.5,
                warmup: 1,
                ..SimLoop::default()
            },
        }
    }
}

impl Flock {
    pub fn count(&self) -> u32 {
        self.count.min(FLOCK_MAX)
    }

    /// Whether the music changes the flight (so a new song means a new
    /// bake).
    pub fn uses_music(&self) -> bool {
        self.speed.uses_music()
            || self.scatter.uses_music()
            || self.target.iter().any(Param::uses_music)
    }

    /// The target at `ctx`.
    pub fn target_at(&self, ctx: &EvalCtx) -> Vec3 {
        let mut t = Vec3::new(
            self.target[0].eval(ctx),
            self.target[1].eval(ctx),
            self.target[2].eval(ctx),
        );
        if let Some(p) = &self.path {
            let u = (ctx.phase * p.laps as f32).rem_euclid(1.0);
            t += Vec3::from(p.curve.point_exact(p.freq, u)) * p.size;
        }
        t
    }

    /// How fast the target moves at `ctx` (units per second).
    pub fn target_velocity(&self, ctx: &EvalCtx) -> Vec3 {
        const E: f32 = 1e-3;
        let shifted = |d: f32| EvalCtx {
            phase: ctx.phase + d,
            beat_phase: ctx.beat_phase + d,
            ..*ctx
        };
        let loop_s = (ctx.loop_beats as f32 * ctx.beat_seconds).max(0.05);
        (self.target_at(&shifted(E)) - self.target_at(&shifted(-E))) / (2.0 * E * loop_s)
    }

    /// How far boids can get from the layer's origin.
    pub fn reach(&self) -> f32 {
        let path = self.path.as_ref().map_or(0.0, |p| p.size.abs() * 1.6);
        let target = self
            .target
            .iter()
            .map(|p| p.base.abs() + p.amp.abs() + p.music.amount.abs())
            .fold(0.0, f32::max);
        path + target * 1.8 + self.radius.abs() * 1.6 + self.spacing
    }

    /// The simulation, ready to bake.
    pub fn sim(&self, ctx0: &EvalCtx) -> FlockSim {
        let mut rng = Rng::new(self.seed as u64 ^ 0xf10c_c0de);
        let centre = self.target_at(ctx0);
        let speed = self.speed.eval(ctx0).max(0.0);
        let bodies = (0..self.count())
            .map(|_| {
                let dir = loop {
                    let d = Vec3::new(rng.signed(), rng.signed(), rng.signed());
                    let l = d.length_squared();
                    if l > 1e-4 && l <= 1.0 {
                        break d;
                    }
                };
                let heading =
                    Vec3::new(rng.signed(), rng.signed() * 0.3, rng.signed()).normalize_or(Vec3::Z);
                let vel = heading * speed;
                Body {
                    pos: centre + dir * self.radius.abs() * 0.7,
                    vel,
                    rot: facing(vel, Vec3::Y),
                    size: 1.0,
                }
            })
            .collect();
        // Each boid's own place: a small tilted circle somewhere in the
        // flock, turned a whole number of times per loop at about the
        // cruising speed (the circle's size follows from its turns), all
        // turning the same way (as real flocks mostly fly one way, and
        // boids on circles turning opposite ways would meet head on).
        let loop_s = ctx0.loop_beats as f32 * ctx0.beat_seconds;
        let r_max = self.radius.abs() * 0.6;
        let homes = (0..self.count())
            .map(|_| {
                let n = Vec3::new(rng.signed() * 0.7, 1.0, rng.signed() * 0.7).normalize();
                let u = n.cross(Vec3::X).normalize_or(Vec3::Z);
                let w = n.cross(u);
                let want = r_max * (0.3 + 0.7 * rng.f32());
                let lap = speed.max(0.1) * loop_s;
                let turns = (lap / (std::f32::consts::TAU * want)).round().max(1.0);
                let r = (lap / (std::f32::consts::TAU * turns)).min(r_max);
                let reach = (self.radius.abs() - r).max(0.0) * 0.8;
                let centre = loop {
                    let d = Vec3::new(rng.signed(), rng.signed() * 0.6, rng.signed());
                    if d.length_squared() <= 1.0 {
                        break d * reach;
                    }
                };
                Home {
                    centre,
                    u: u * r,
                    w: w * r,
                    turns: turns as i32,
                    offset: rng.f32(),
                }
            })
            .collect();
        FlockSim {
            homes,
            set: self.clone(),
            bodies,
            acc: Vec::new(),
            order: Vec::new(),
            starts: Vec::new(),
            guide: Vec::new(),
            guide_rate: 0.0,
        }
    }
}

/// The rotation facing along `vel` (+z forward) with `up` as close to up
/// as it allows.
fn facing(vel: Vec3, up: Vec3) -> Rot {
    let fwd = vel.normalize_or(Vec3::Z);
    let side = up.cross(fwd).normalize_or(Vec3::X);
    math::rot_from_basis(side, fwd.cross(side), fwd)
}

/// A boid's place in the formation: `centre + u cos a + w sin a` from the
/// target, `a` turning `turns` times per loop.
struct Home {
    centre: Vec3,
    u: Vec3,
    w: Vec3,
    turns: i32,
    offset: f32,
}

/// A flock being simulated.
pub struct FlockSim {
    set: Flock,
    homes: Vec<Home>,
    bodies: Vec<Body>,
    // Scratch: steering per boid, and the neighbour grid (boids sorted by
    // hash cell, `starts[c]..starts[c + 1]` in cell `c`).
    acc: Vec<Vec3>,
    order: Vec<u32>,
    starts: Vec<u32>,
    /// A guided tail's targets and how fast to close in on them (per
    /// second), set by [`Sim::guide`] for the next step.
    guide: Vec<Body>,
    guide_rate: f32,
}

impl FlockSim {
    fn cell(p: Vec3, size: f32) -> [i32; 3] {
        [
            (p.x / size).floor() as i32,
            (p.y / size).floor() as i32,
            (p.z / size).floor() as i32,
        ]
    }

    fn slot(c: [i32; 3], mask: u32) -> u32 {
        let h = (c[0] as u32).wrapping_mul(0x8da6_b343)
            ^ (c[1] as u32).wrapping_mul(0xd816_3841)
            ^ (c[2] as u32).wrapping_mul(0xcb1a_b31f);
        crate::rng::hash_u32(h) & mask
    }

    /// Sorts the boids into hash cells of `size` (a counting sort, so the
    /// order is the same everywhere).
    fn build_grid(&mut self, size: f32) -> u32 {
        let n = self.bodies.len();
        let slots = (n * 2).next_power_of_two().max(16) as u32;
        let mask = slots - 1;
        self.starts.clear();
        self.starts.resize(slots as usize + 1, 0);
        let keys: Vec<u32> = self
            .bodies
            .iter()
            .map(|b| Self::slot(Self::cell(b.pos, size), mask))
            .collect();
        for &k in &keys {
            self.starts[k as usize + 1] += 1;
        }
        for i in 0..slots as usize {
            self.starts[i + 1] += self.starts[i];
        }
        self.order.clear();
        self.order.resize(n, 0);
        let mut fill = self.starts.clone();
        for (i, &k) in keys.iter().enumerate() {
            self.order[fill[k as usize] as usize] = i as u32;
            fill[k as usize] += 1;
        }
        mask
    }
}

impl Sim for FlockSim {
    fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    fn bodies_mut(&mut self) -> &mut [Body] {
        &mut self.bodies
    }

    fn rotations(&self) -> bool {
        true
    }

    /// Steers to the start instead of dragging there, so boids keep apart
    /// while the loop closes.
    fn guide(&mut self, targets: &[Body], gain: f32, dt: f32, drift: &mut [Vec3]) {
        self.guide.clear();
        self.guide.extend_from_slice(targets);
        // `gain` is the fraction to close this step: as a rate, capped so
        // the (explicit) steering stays stable.
        self.guide_rate = (gain / dt).min(0.4 / dt);
        drift.fill(Vec3::ZERO);
    }

    fn step(&mut self, dt: f32, ctx: &EvalCtx) {
        let s = &self.set;
        let target = s.target_at(ctx);
        let speed = s.speed.eval(ctx).max(0.0);
        let scatter = s.scatter.eval(ctx);
        let spacing = s.spacing.max(0.01);
        let sight = s.sight.max(spacing);
        let radius = s.radius.abs().max(0.1);
        let agility = s.agility.max(0.0);
        let (sep_w, ali_w, coh_w) = (s.separation, s.alignment, s.cohesion);
        let bank = s.bank;
        // A scatter hit loosens the formation while it lasts.
        let form = s.formation.max(0.0) * (1.0 - scatter.clamp(0.0, 1.0));
        let loop_s = (ctx.loop_beats as f32 * ctx.beat_seconds).max(0.05);
        // The flock flies relative to its target: the target's own motion
        // is added on top, so the flock keeps up with it exactly and
        // steering goes into flocking only.
        let target_vel = s.target_velocity(ctx);
        let next = EvalCtx {
            phase: ctx.phase + dt / loop_s,
            beat_phase: ctx.beat_phase + dt / loop_s,
            ..*ctx
        };
        let (target_next, target_vel_next) = (s.target_at(&next), s.target_velocity(&next));
        let mask = self.build_grid(sight);
        let (bodies, order, starts) = (&self.bodies, &self.order, &self.starts);
        let homes = &self.homes;
        let (guide, rate) = (&self.guide, self.guide_rate);
        self.acc.clear();
        self.acc.extend(bodies.iter().enumerate().map(|(i, b)| {
            // The cells around the boid (a cell's slot can repeat: visit
            // each slot once).
            let c = Self::cell(b.pos, sight);
            let mut slots = [0u32; 27];
            let mut k = 0;
            for dz in -1..=1 {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        slots[k] = Self::slot([c[0] + dx, c[1] + dy, c[2] + dz], mask);
                        k += 1;
                    }
                }
            }
            slots.sort_unstable();
            let (mut sep, mut vel_sum, mut pos_sum, mut seen) =
                (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 0u32);
            let mut last = u32::MAX;
            for &slot in &slots {
                if slot == last {
                    continue;
                }
                last = slot;
                let range = starts[slot as usize] as usize..starts[slot as usize + 1] as usize;
                for &j in &order[range] {
                    let j = j as usize;
                    if j == i {
                        continue;
                    }
                    let d = bodies[j].pos - b.pos;
                    let r2 = d.length_squared();
                    if r2 >= sight * sight {
                        continue;
                    }
                    seen += 1;
                    vel_sum += bodies[j].vel;
                    pos_sum += bodies[j].pos;
                    if r2 < spacing * spacing {
                        let r = r2.sqrt();
                        let away = if r > 1e-5 {
                            -d / r
                        } else {
                            // On top of each other: split by number.
                            Vec3::new(1.0, 0.3, -0.5) * if i < j { 1.0 } else { -1.0 }
                        };
                        // Gentle at the spacing, sharp up close.
                        sep += away * (spacing / r.max(spacing * 0.1) - 1.0);
                    }
                }
            }
            let rel = b.vel - target_vel;
            let mut acc = Vec3::ZERO;
            if form > 0.0 {
                let h = &homes[i];
                let a = std::f32::consts::TAU * (ctx.phase * h.turns as f32 + h.offset);
                let (sa, ca) = math::sin_cos(a);
                let place = target + h.centre + h.u * ca + h.w * sa;
                let place_vel =
                    (h.w * ca - h.u * sa) * (std::f32::consts::TAU * h.turns as f32 / loop_s);
                acc += (place - b.pos) * (form * 6.0) + (place_vel - rel) * (form * 3.0);
            }
            if seen > 0 {
                let n = seen as f32;
                acc += (vel_sum / n - b.vel) * (ali_w * 2.0);
                acc += (pos_sum / n - b.pos) * (coh_w * 1.5);
            }
            // Home: a gentle pull towards the target, and a firm one
            // beyond the radius.
            let home = target - b.pos;
            let dist = home.length();
            acc += home * 0.15;
            if dist > radius {
                acc += home / dist * ((dist - radius) / radius) * agility * 3.0;
            }
            if scatter != 0.0 && dist > 1e-4 {
                acc -= home / dist * scatter * agility * 6.0;
            }
            // Keep to the cruising speed.
            let v = rel.length();
            if v > 1e-4 {
                acc += rel / v * (speed - v) * 2.0;
            }
            // Steering is limited; keeping apart has a limit of its own, so
            // the pull to the formation can't press boids together.
            let limit = agility * (1.0 + scatter.abs() * 3.0);
            let l = acc.length();
            if l > limit {
                acc *= limit / l;
            }
            // A guided tail: critically damped steering to the start,
            // beyond the usual limit (it has to arrive).
            if let Some(to) = guide.get(i) {
                let mut g = (to.pos - b.pos) * (rate * rate) + (to.vel - b.vel) * (2.0 * rate);
                let l = g.length();
                if l > GUIDE_CAP * agility {
                    g *= GUIDE_CAP * agility / l;
                }
                acc += g;
            }
            let mut apart = sep * (sep_w * agility * 3.0);
            let l = apart.length();
            if l > agility * APART_CAP {
                apart *= agility * APART_CAP / l;
            }
            acc + apart
        }));
        self.guide.clear();
        let turn = 1.0 - math::exp(-14.0 * dt);
        let carried = target_next - target;
        let target_acc = (target_vel_next - target_vel) / dt;
        for (b, &acc) in self.bodies.iter_mut().zip(&self.acc) {
            let mut rel = b.vel - target_vel + acc * dt;
            let v = rel.length();
            let top = speed * 2.5 + 0.5;
            if v > top {
                rel *= top / v;
            }
            b.pos += rel * dt + carried;
            b.vel = rel + target_vel_next;
            // Bank: lift leans into the turn, like a plane's.
            let fwd = b.vel.normalize_or(Vec3::Z);
            let acc = acc + target_acc;
            let side_acc = acc - fwd * acc.dot(fwd);
            let up = (Vec3::Y * 9.8 + side_acc * bank).normalize_or(Vec3::Y);
            b.rot = math::rot_nlerp(b.rot, facing(b.vel, up), turn);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{BakeJob, Frame, KeyHasher, SimClock};
    use crate::{Timing, Wave};

    fn clock() -> SimClock {
        SimClock::new(Timing {
            bpm: 120.0,
            loop_beats: 8,
        })
    }

    fn bake(f: &Flock) -> crate::sim::Bake {
        let c = clock();
        BakeJob::new(Box::new(f.sim(&c.ctx(0.0))), c, f.looping.clone()).finish()
    }

    fn at(b: &crate::sim::Bake, phase: f32) -> Vec<Body> {
        let mut f = Frame::default();
        b.sample(phase, &mut f);
        f.layers.swap_remove(0).bodies
    }

    fn nearest(bodies: &[Body], i: usize) -> f32 {
        bodies
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, b)| b.pos.distance(bodies[i].pos))
            .fold(f32::MAX, f32::min)
    }

    #[test]
    fn loops() {
        let f = Flock::default();
        let b = bake(&f);
        let (start, end) = (at(&b, 0.0), at(&b, 1.0 - 1e-5));
        for (a, z) in start.iter().zip(&end) {
            assert!(a.pos.distance(z.pos) < 1e-3, "{} vs {}", a.pos, z.pos);
            let d: f32 = a.rot.iter().zip(z.rot).map(|(x, y)| x * y).sum();
            assert!(d.abs() > 0.999);
        }
    }

    #[test]
    fn boids_keep_apart_and_stay_home() {
        let f = Flock::default();
        let b = bake(&f);
        let n = 64;
        for k in 0..n {
            let phase = k as f32 / n as f32;
            let bodies = at(&b, phase);
            let crowded = (0..bodies.len())
                .filter(|&i| nearest(&bodies, i) < f.spacing * 0.3)
                .count();
            // While the tail steers back to the start (and the drawn blend
            // mops up), boids with some way to go may brush past others.
            let closing = phase > 1.0 - f.looping.blend;
            let allowed = if closing { 0.15 } else { 0.02 };
            assert!(
                crowded as f32 <= bodies.len() as f32 * allowed,
                "{crowded} crowded at {phase}"
            );
            let far = bodies.iter().map(|b| b.pos.length()).fold(0.0, f32::max);
            assert!(far < f.radius * 1.6, "{far} from home at {phase}");
            // They fly at about the cruising speed, also while the loop
            // closes (a free flock would race to its start there).
            let mean = bodies.iter().map(|b| b.vel.length()).sum::<f32>() / bodies.len() as f32;
            assert!(mean > 2.0 && mean < 4.5, "mean speed {mean} at {phase}");
        }
        // Holding a formation, the loop nearly closes by itself.
        assert!(b.seam().rms < 0.3, "{:?}", b.seam());
    }

    #[test]
    fn a_free_flock_still_loops() {
        let f = Flock {
            formation: 0.0,
            count: 150,
            ..Flock::default()
        };
        let b = bake(&f);
        let (start, end) = (at(&b, 0.0), at(&b, 1.0 - 1e-5));
        for (a, z) in start.iter().zip(&end) {
            assert!(a.pos.distance(z.pos) < 1e-3);
        }
    }

    #[test]
    fn copies_face_where_they_fly() {
        let b = bake(&Flock::default());
        for phase in [0.1, 0.4, 0.7] {
            let bodies = at(&b, phase);
            // Turning is smoothed, so a boid in a sharp turn lags a little.
            let facing = bodies
                .iter()
                .filter(|body| {
                    let fwd = glam::Quat::from_array(body.rot) * Vec3::Z;
                    fwd.dot(body.vel.normalize()) > 0.8
                })
                .count();
            assert!(facing * 10 >= bodies.len() * 9, "{facing} at {phase}");
        }
    }

    #[test]
    fn the_flock_follows_its_path() {
        // The target moves at ~4.7 units per second, the flock cruises at 5.
        let f = Flock {
            radius: 4.0,
            speed: Param::new(5.0),
            path: Some(FlockPath {
                curve: RibbonCurve::Wave,
                freq: [1, 1, 1],
                size: 3.0,
                laps: 1,
            }),
            ..Flock::default()
        };
        let b = bake(&f);
        let c = clock();
        for k in 0..8 {
            let phase = k as f32 / 8.0;
            let bodies = at(&b, phase);
            let centre = bodies.iter().map(|b| b.pos).sum::<Vec3>() / bodies.len() as f32;
            let target = f.target_at(&c.ctx(phase));
            assert!(
                centre.distance(target) < 1.5,
                "{centre} vs {target} at {phase}"
            );
        }
    }

    #[test]
    fn scatter_pushes_them_out() {
        // A scatter hit at every bar (a sharp pulse, 4 per loop).
        let f = Flock {
            scatter: Param::new(0.0).osc(Wave::Pulse, 1.0, 4),
            ..Flock::default()
        };
        let calm = bake(&Flock::default());
        let hit = bake(&f);
        let outward = |b: &[Body]| {
            b.iter()
                .map(|b| b.vel.dot(b.pos.normalize_or_zero()))
                .sum::<f32>()
                / b.len() as f32
        };
        // Right after a hit (at 0.25) they turn outward...
        let (before, after) = (outward(&at(&hit, 0.245)), outward(&at(&hit, 0.26)));
        assert!(after > before + 1.5, "{after} vs {before}");
        // ...and the flock spreads out.
        let spread = |b: &[Body]| b.iter().map(|b| b.pos.length()).sum::<f32>() / b.len() as f32;
        let (wide, calm_wide) = (spread(&at(&hit, 0.36)), spread(&at(&calm, 0.36)));
        assert!(wide > calm_wide * 1.2, "{wide} vs {calm_wide}");
    }

    /// A flock bakes the same on every platform (see `same_bake_everywhere`
    /// in `bake.rs`); update only when flocks are meant to change.
    #[test]
    fn same_flock_everywhere() {
        let f = Flock {
            count: 120,
            speed: Param::new(3.0).osc(Wave::Sine, 1.0, 2),
            ..Flock::default()
        };
        let b = bake(&f);
        let hash = b
            .raw()
            .iter()
            .fold(KeyHasher::new(), |h, v| h.f32(*v))
            .finish();
        assert_eq!(hash, 0xe95b_166f_ce7c_2f1e, "flock hash {hash:#018x}");
    }
}

#[cfg(test)]
mod saving {
    use crate::*;

    #[test]
    fn flocks_save_and_load() {
        let p = presets::named("Starling Dusk");
        let json = p.to_json();
        assert!(!json.contains("placed"));
        let back = Project::from_json(&json).unwrap();
        assert_eq!(back, p);
        // Settings added later default when missing from a file.
        let old = json.replace("\"formation\": 0.8,", "");
        let back = Project::from_json(&old).unwrap();
        assert!(matches!(
            back.layers[1].kind.instancer(),
            Some(Instancer::Flock { flock, .. }) if flock.formation == 0.8
        ));
    }
}
