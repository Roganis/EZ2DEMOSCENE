//! Rigid bodies: boxes and balls that fall, stack, collide and get blown
//! apart (extended position-based dynamics: small substeps, positional
//! contacts with friction, then a velocity pass for bounce).

use super::math::{self, Rot};
use super::{Body, Sim, SimLoop};
use crate::rng::hash2;
use crate::EvalCtx;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Most bodies (a bake keeps every one at every key).
pub const PHYSICS_MAX: u32 = 400;
/// Substeps per simulation step.
const SUBSTEPS: usize = 8;
/// Part of a body's life spent vanishing (shrinking or sinking).
const VANISH: f32 = 0.15;
/// Nearly still for this long (seconds), a body falls asleep: it stays
/// exactly put, and counts as fixed for the others, until something moving
/// touches it or a blast hits it. (Resting contacts solved one after
/// another otherwise leave a slow creep.)
const SLEEP_AFTER: f32 = 0.25;
const SLEEP_SPEED: f32 = 0.08;
const SLEEP_SPIN: f32 = 0.15;
/// Overlaps are pushed apart at most this fast (units per second): a deep
/// overlap corrected at once would fling bodies away.
const MAX_SEPARATION: f32 = 4.0;
/// A dead body waits this long (a fraction of the loop) where it died
/// before it moves to its next start, so the move can't be seen.
const PARK: f32 = 0.02;

/// The shape each copy collides as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Collider {
    #[default]
    Box,
    Ball,
}

impl Collider {
    pub const ALL: [Collider; 2] = [Collider::Box, Collider::Ball];
    pub fn label(self) -> &'static str {
        match self {
            Collider::Box => "Box",
            Collider::Ball => "Ball",
        }
    }
}

/// How a body in the rain leaves at the end of its life.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Vanish {
    #[default]
    Shrink,
    /// Sink through the floor.
    Sink,
}

impl Vanish {
    pub const ALL: [Vanish; 2] = [Vanish::Shrink, Vanish::Sink];
    pub fn label(self) -> &'static str {
        match self {
            Vanish::Shrink => "Shrink away",
            Vanish::Sink => "Sink into the floor",
        }
    }
}

/// What happens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Scenario {
    /// Bodies drop from above one after another, a whole number per loop,
    /// pile up and vanish after their life.
    #[default]
    Rain,
    /// Bodies start stacked in a grid on the floor (a wall, a tower) and a
    /// blast knocks them down.
    Stack,
}

impl Scenario {
    pub const ALL: [Scenario; 2] = [Scenario::Rain, Scenario::Stack];
    pub fn label(self) -> &'static str {
        match self {
            Scenario::Rain => "Rain",
            Scenario::Stack => "Stack and blast",
        }
    }
}

/// A rigid body simulation's settings (the copy layout "Physics").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Physics {
    pub scenario: Scenario,
    pub collider: Collider,
    /// Half the collider's size for a copy of size 1 (the built-in cube:
    /// 0.5; the sphere: 1).
    pub extent: f32,
    /// Rain: bodies, each dropping once per loop.
    pub count: u32,
    /// Rain: how long each lives, in beats.
    pub life: f32,
    /// Rain: radius of the area they drop into.
    pub area: f32,
    /// Rain: height they drop from, above the floor.
    pub height: f32,
    pub vanish: Vanish,
    /// Rain: spin when dropped (radians per second).
    pub spin: f32,
    /// Stack: bodies along x, y (up) and z.
    pub counts: [u32; 3],
    /// Stack: gap between bodies.
    pub gap: f32,
    /// Stack: when the blast goes off, in beats from the loop's start.
    pub blast_beat: f32,
    /// Stack: the blast's strength (speed given at its centre).
    pub blast: f32,
    /// Stack: where it goes off, from the layer's origin.
    pub blast_at: [f32; 3],
    /// Downward pull (0 = floating in space).
    pub gravity: f32,
    /// The floor's height, from the layer's origin.
    pub floor: f32,
    /// Bounciness (0..1).
    pub bounce: f32,
    /// Grip (0..1).
    pub friction: f32,
    pub seed: u32,
    /// How the loop is closed.
    pub looping: SimLoop,
}

impl Default for Physics {
    fn default() -> Self {
        Physics {
            scenario: Scenario::Rain,
            collider: Collider::Box,
            extent: 0.5,
            count: 60,
            // Short lives and a long tail: bodies dropped before the tail
            // are gone by the loop's end, and those dropped in it started
            // just like a loop earlier, so closing the loop hardly moves
            // anything (with lives of half the loop, a body could slide
            // across the floor at 20–30 units per second to its start).
            life: 4.0,
            area: 3.0,
            height: 4.0,
            vanish: Vanish::Shrink,
            spin: 3.0,
            counts: [6, 5, 1],
            gap: 0.0,
            blast_beat: 2.0,
            blast: 9.0,
            blast_at: [0.0, 1.0, 1.5],
            gravity: 9.8,
            floor: 0.0,
            bounce: 0.2,
            friction: 1.0,
            seed: 1,
            looping: SimLoop {
                blend: 0.5,
                ..SimLoop::default()
            },
        }
    }
}

impl Physics {
    /// A wall knocked down and rebuilt (ping-pong).
    pub fn stack() -> Physics {
        Physics {
            scenario: Scenario::Stack,
            looping: SimLoop {
                close: super::LoopClose::PingPong,
                warmup: 0,
                ..SimLoop::default()
            },
            ..Physics::default()
        }
    }

    /// Number of bodies.
    pub fn bodies(&self) -> u32 {
        match self.scenario {
            Scenario::Rain => self.count,
            Scenario::Stack => self.counts.iter().map(|c| (*c).max(1)).product(),
        }
        .min(PHYSICS_MAX)
    }

    /// How far bodies can get from the layer's origin (roughly).
    pub fn reach(&self, scale: Vec3) -> f32 {
        let size = self.extent.abs() * scale.abs().max_element() * 2.0;
        match self.scenario {
            Scenario::Rain => self.area.abs() * 1.5 + self.height.abs() * 1.5 + size * 4.0,
            Scenario::Stack => {
                let c = self.counts.map(|c| c.max(1) as f32);
                let stack = Vec3::from(c).length() * (size + self.gap.abs());
                let fly = self.blast.abs() * 1.5;
                stack + fly + Vec3::from(self.blast_at).length()
            }
        }
    }

    /// The simulation, ready to bake; `scale` is the copies' size (the
    /// layer's scale and stretch).
    pub fn sim(&self, scale: Vec3) -> PhysicsSim {
        let n = self.bodies() as usize;
        let half = match self.collider {
            Collider::Box => scale.abs() * self.extent.abs().max(1e-3),
            Collider::Ball => Vec3::splat(scale.abs().max_element() * self.extent.abs().max(1e-3)),
        };
        let mass = 1.0;
        let inv_inertia = match self.collider {
            Collider::Box => {
                let s = half * 2.0;
                Vec3::new(
                    12.0 / (mass * (s.y * s.y + s.z * s.z)),
                    12.0 / (mass * (s.x * s.x + s.z * s.z)),
                    12.0 / (mass * (s.x * s.x + s.y * s.y)),
                )
            }
            Collider::Ball => Vec3::splat(2.5 / (mass * half.x * half.x)),
        };
        let mut bodies = Vec::with_capacity(n);
        let mut rigs = Vec::with_capacity(n);
        for i in 0..n {
            let (body, spin) = match self.scenario {
                Scenario::Rain => {
                    let (p, r, w) = self.spawn(i as u32, half);
                    // Hidden until its first drop.
                    (
                        Body {
                            pos: p,
                            vel: Vec3::ZERO,
                            rot: r,
                            size: 0.0,
                        },
                        w,
                    )
                }
                Scenario::Stack => {
                    let c = self.counts.map(|c| c.max(1) as usize);
                    let (x, y, z) = (i % c[0], (i / c[0]) % c[1], i / (c[0] * c[1]));
                    let step = half * 2.0 + Vec3::splat(self.gap.max(0.0));
                    let p = Vec3::new(
                        (x as f32 - (c[0] - 1) as f32 * 0.5) * step.x,
                        self.floor + half.y + y as f32 * step.y + 1e-4,
                        (z as f32 - (c[2] - 1) as f32 * 0.5) * step.z,
                    );
                    (Body::at(p), Vec3::ZERO)
                }
            };
            bodies.push(body);
            rigs.push(Rig {
                omega: spin,
                prev_pos: body.pos,
                prev_rot: body.rot,
                alive: self.scenario == Scenario::Stack,
                ghost: false,
                support: 0,
                // A stack starts asleep: it stands exactly until hit.
                still: if self.scenario == Scenario::Stack {
                    SLEEP_AFTER
                } else {
                    0.0
                },
            });
        }
        PhysicsSim {
            set: self.clone(),
            half,
            inv_mass: 1.0 / mass,
            inv_inertia,
            bodies,
            rigs,
            contacts: Vec::new(),
            order: Vec::new(),
            last_beat: None,
            guide: Vec::new(),
            guide_rate: 0.0,
            life_phase: 0.5,
            loop_s: 8.0,
        }
    }

    /// Where rain body `i` starts, its turn and spin (the same every loop).
    fn spawn(&self, i: u32, half: Vec3) -> (Vec3, Rot, Vec3) {
        let r = |k: u32| hash2(self.seed.wrapping_mul(0x9e37_79b9) ^ i, k);
        let a = r(0) * std::f32::consts::TAU;
        let d = r(1).sqrt() * self.area.abs();
        let (sa, ca) = math::sin_cos(a);
        let p = Vec3::new(
            ca * d,
            self.floor + half.max_element() + self.height.abs() * (1.0 + 0.3 * r(2)),
            sa * d,
        );
        let axis = Vec3::new(r(3) - 0.5, r(4) - 0.5, r(5) - 0.5).normalize_or(Vec3::Y);
        let rot = math::rot_axis_angle(axis, r(6) * std::f32::consts::TAU);
        let w = Vec3::new(r(7) - 0.5, r(8) - 0.5, r(9) - 0.5) * (2.0 * self.spin);
        (p, rot, w)
    }

    /// Rain body `i`'s age at `phase` (0..1 of the loop since its drop).
    fn age(&self, i: u32, phase: f32) -> f32 {
        let start = (i as f32 + 0.5 * hash2(self.seed ^ 0x5eed, i)) / self.count.max(1) as f32;
        (phase - start).rem_euclid(1.0)
    }
}

/// What a body carries besides its [`Body`].
#[derive(Clone, Copy)]
struct Rig {
    omega: Vec3,
    prev_pos: Vec3,
    prev_rot: Rot,
    /// Taking part (rain bodies between their drop and their end).
    alive: bool,
    /// Sinking through the floor: no collisions.
    ghost: bool,
    /// How long it has been nearly still (asleep past `SLEEP_AFTER`).
    still: f32,
    /// Contact points holding it up in the last substep (a body on an edge
    /// or a corner is balanced, not resting: it must not fall asleep).
    support: u32,
}

/// A contact: a point on `a` (in its own frame) pushed out along `n` to a
/// point on `b` (in `b`'s frame; the world for the floor).
#[derive(Clone, Copy)]
struct Contact {
    a: u32,
    b: Option<u32>,
    la: Vec3,
    lb: Vec3,
    n: Vec3,
    /// Normal speed before the substep (for bounce).
    vn: f32,
    /// Normal push applied this substep.
    lambda: f32,
}

/// Bodies being simulated.
pub struct PhysicsSim {
    set: Physics,
    half: Vec3,
    inv_mass: f32,
    /// Inverse inertia in the body's frame (diagonal).
    inv_inertia: Vec3,
    bodies: Vec<Body>,
    rigs: Vec<Rig>,
    contacts: Vec<Contact>,
    order: Vec<(f32, u32)>,
    last_beat: Option<f32>,
    /// A guided tail's targets and how fast to close in on them (per
    /// second), set by [`Sim::guide`] for the next step.
    guide: Vec<Body>,
    guide_rate: f32,
    /// Rain: a body's life as a fraction of the loop.
    life_phase: f32,
    loop_s: f32,
}

fn rotate(q: Rot, v: Vec3) -> Vec3 {
    let u = Vec3::new(q[0], q[1], q[2]);
    let t = u.cross(v) * 2.0;
    v + t * q[3] + u.cross(t)
}

fn unrotate(q: Rot, v: Vec3) -> Vec3 {
    rotate([-q[0], -q[1], -q[2], q[3]], v)
}

impl PhysicsSim {
    fn active(&self, i: usize) -> bool {
        self.rigs[i].alive && !self.rigs[i].ghost
    }

    fn asleep(&self, i: usize) -> bool {
        self.rigs[i].still >= SLEEP_AFTER
    }

    /// Inverse inertia times `v`, in the world.
    fn inv_i(&self, i: usize, v: Vec3) -> Vec3 {
        let q = self.bodies[i].rot;
        rotate(q, self.inv_inertia * unrotate(q, v))
    }

    /// How hard body `i` is to push at `r` (from its centre) along `n`.
    fn weight(&self, i: usize, r: Vec3, n: Vec3) -> f32 {
        if self.asleep(i) {
            return 0.0;
        }
        let rn = r.cross(n);
        self.inv_mass + rn.dot(self.inv_i(i, rn))
    }

    /// Moves body `i` by the push `p` applied at `r` (from its centre).
    fn push(&mut self, i: usize, r: Vec3, p: Vec3) {
        if self.asleep(i) {
            return;
        }
        self.bodies[i].pos += p * self.inv_mass;
        let w = self.inv_i(i, r.cross(p));
        let q = self.bodies[i].rot;
        let dq = math::rot_mul([w.x, w.y, w.z, 0.0], q);
        self.bodies[i].rot = math::rot_normalize([
            q[0] + 0.5 * dq[0],
            q[1] + 0.5 * dq[1],
            q[2] + 0.5 * dq[2],
            q[3] + 0.5 * dq[3],
        ]);
    }

    /// Velocity of body `i`'s point at `r`.
    fn point_vel(&self, i: usize, r: Vec3) -> Vec3 {
        self.bodies[i].vel + self.rigs[i].omega.cross(r)
    }

    /// Points of a box tested against the others: corners and face centres
    /// (in its frame).
    fn box_points(&self) -> [Vec3; 14] {
        let h = self.half;
        let mut out = [Vec3::ZERO; 14];
        for (k, o) in out.iter_mut().take(8).enumerate() {
            *o = Vec3::new(
                if k & 1 == 0 { -h.x } else { h.x },
                if k & 2 == 0 { -h.y } else { h.y },
                if k & 4 == 0 { -h.z } else { h.z },
            );
        }
        out[8] = Vec3::X * h.x;
        out[9] = -Vec3::X * h.x;
        out[10] = Vec3::Y * h.y;
        out[11] = -Vec3::Y * h.y;
        out[12] = Vec3::Z * h.z;
        out[13] = -Vec3::Z * h.z;
        out
    }

    fn radius(&self) -> f32 {
        match self.set.collider {
            Collider::Box => self.half.length(),
            Collider::Ball => self.half.x,
        }
    }

    fn contact(&self, a: usize, b: Option<usize>, pa: Vec3, pb: Vec3, n: Vec3) -> Contact {
        let ra = pa - self.bodies[a].pos;
        let mut rel = self.point_vel(a, ra);
        let lb = match b {
            Some(b) => {
                let rb = pb - self.bodies[b].pos;
                rel -= self.point_vel(b, rb);
                unrotate(self.bodies[b].rot, rb)
            }
            None => pb,
        };
        Contact {
            a: a as u32,
            b: b.map(|b| b as u32),
            la: unrotate(self.bodies[a].rot, ra),
            lb,
            n,
            vn: rel.dot(n),
            lambda: 0.0,
        }
    }

    /// Point `p` (on body `a`) inside box `b`: the contact pushing it out
    /// through the nearest face.
    fn point_in_box(&self, a: usize, b: usize, p: Vec3) -> Option<Contact> {
        let q = self.bodies[b].rot;
        let l = unrotate(q, p - self.bodies[b].pos);
        let depth = self.half - l.abs();
        if depth.min_element() <= 0.0 {
            return None;
        }
        let (axis, _) =
            [depth.x, depth.y, depth.z]
                .iter()
                .enumerate()
                .fold(
                    (0, f32::MAX),
                    |m, (k, d)| if *d < m.1 { (k, *d) } else { m },
                );
        let mut face = l;
        let mut n = Vec3::ZERO;
        let s = if l[axis] >= 0.0 { 1.0 } else { -1.0 };
        face[axis] = self.half[axis] * s;
        n[axis] = s;
        let pb = self.bodies[b].pos + rotate(q, face);
        Some(self.contact(a, Some(b), p, pb, rotate(q, n)))
    }

    /// Finds the contacts of the predicted poses.
    fn find_contacts(&mut self) {
        self.contacts.clear();
        let n = self.bodies.len();
        let floor = self.set.floor;
        let points = self.box_points();
        // The floor.
        for i in 0..n {
            if !self.active(i) || self.asleep(i) {
                continue;
            }
            let b = self.bodies[i];
            match self.set.collider {
                Collider::Box => {
                    for lp in points.iter().take(8) {
                        let p = b.pos + rotate(b.rot, *lp);
                        if p.y < floor {
                            let c = self.contact(i, None, p, Vec3::new(p.x, floor, p.z), Vec3::Y);
                            self.contacts.push(c);
                        }
                    }
                }
                Collider::Ball => {
                    let p = b.pos - Vec3::Y * self.half.x;
                    if p.y < floor {
                        let c = self.contact(i, None, p, Vec3::new(p.x, floor, p.z), Vec3::Y);
                        self.contacts.push(c);
                    }
                }
            }
        }
        // Each other: sweep along x (sorted by the left edge, then number,
        // so the order is the same everywhere).
        let r = self.radius();
        self.order.clear();
        for i in 0..n {
            if self.active(i) {
                self.order.push((self.bodies[i].pos.x - r, i as u32));
            }
        }
        self.order
            .sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for s in 0..self.order.len() {
            let (left, i) = self.order[s];
            let i = i as usize;
            for t in s + 1..self.order.len() {
                let (left_j, j) = self.order[t];
                if left_j > left + 2.0 * r {
                    break;
                }
                let j = j as usize;
                let (pi, pj) = (self.bodies[i].pos, self.bodies[j].pos);
                if pi.distance_squared(pj) > 4.0 * r * r {
                    continue;
                }
                let (a, b) = (i.min(j), i.max(j));
                // Two sleepers stay as they are; something moving wakes a
                // sleeper it touches.
                match (self.asleep(a), self.asleep(b)) {
                    (true, true) => continue,
                    (true, false) | (false, true) => {
                        let (sleeper, mover) = if self.asleep(a) { (a, b) } else { (b, a) };
                        if self.bodies[mover].vel.length() > SLEEP_SPEED * 4.0
                            || self.rigs[mover].omega.length() > SLEEP_SPIN * 4.0
                        {
                            self.rigs[sleeper].still = 0.0;
                        }
                    }
                    _ => {}
                }
                match self.set.collider {
                    Collider::Ball => {
                        let d = self.bodies[a].pos - self.bodies[b].pos;
                        let l = d.length();
                        if l < 2.0 * r {
                            let nrm = if l > 1e-6 { d / l } else { Vec3::Y };
                            let pa = self.bodies[a].pos - nrm * r;
                            let pb = self.bodies[b].pos + nrm * r;
                            let c = self.contact(a, Some(b), pa, pb, nrm);
                            self.contacts.push(c);
                        }
                    }
                    Collider::Box => {
                        for (x, y) in [(a, b), (b, a)] {
                            let bx = self.bodies[x];
                            for lp in &points {
                                let p = bx.pos + rotate(bx.rot, *lp);
                                if let Some(c) = self.point_in_box(x, y, p) {
                                    self.contacts.push(c);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn solve_contacts(&mut self, h: f32) {
        let mu = self.set.friction.clamp(0.0, 1.5);
        let max_depth = MAX_SEPARATION * h;
        for k in 0..self.contacts.len() {
            let c = self.contacts[k];
            let a = c.a as usize;
            let pa = self.bodies[a].pos + rotate(self.bodies[a].rot, c.la);
            let pb = match c.b {
                Some(b) => self.bodies[b as usize].pos + rotate(self.bodies[b as usize].rot, c.lb),
                None => c.lb,
            };
            let depth = (pb - pa).dot(c.n).min(max_depth);
            if depth <= 0.0 {
                continue;
            }
            let ra = pa - self.bodies[a].pos;
            let wa = self.weight(a, ra, c.n);
            let rb = c.b.map(|b| pb - self.bodies[b as usize].pos);
            let wb = match (c.b, rb) {
                (Some(b), Some(rb)) => self.weight(b as usize, rb, c.n),
                _ => 0.0,
            };
            if wa + wb <= 0.0 {
                continue;
            }
            let lambda = depth / (wa + wb);
            self.contacts[k].lambda = lambda;
            // Holding up: a contact pushing upwards-ish.
            if c.n.y > 0.5 {
                self.rigs[a].support += 1;
            } else if c.n.y < -0.5 {
                if let Some(b) = c.b {
                    self.rigs[b as usize].support += 1;
                }
            }
            let p = c.n * lambda;
            self.push(a, ra, p);
            if let (Some(b), Some(rb)) = (c.b, rb) {
                self.push(b as usize, rb, -p);
            }
            // Static friction: undo the sliding of the touching points
            // this substep, as long as it isn't pushed too hard.
            let a_prev = self.rigs[a].prev_pos + rotate(self.rigs[a].prev_rot, c.la);
            let pa = self.bodies[a].pos + rotate(self.bodies[a].rot, c.la);
            let mut slide = pa - a_prev;
            if let Some(b) = c.b {
                let b = b as usize;
                let b_prev = self.rigs[b].prev_pos + rotate(self.rigs[b].prev_rot, c.lb);
                let pb = self.bodies[b].pos + rotate(self.bodies[b].rot, c.lb);
                slide -= pb - b_prev;
            }
            let tangent = slide - c.n * slide.dot(c.n);
            let tl = tangent.length();
            if tl < 1e-9 {
                continue;
            }
            let t = tangent / tl;
            let ra = pa - self.bodies[a].pos;
            let wa = self.weight(a, ra, t);
            let rb = c.b.map(|b| {
                self.bodies[b as usize].pos + rotate(self.bodies[b as usize].rot, c.lb)
                    - self.bodies[b as usize].pos
            });
            let wb = match (c.b, rb) {
                (Some(b), Some(rb)) => self.weight(b as usize, rb, t),
                _ => 0.0,
            };
            if wa + wb <= 0.0 {
                continue;
            }
            let lt = tl / (wa + wb);
            if lt < mu * 1.2 * lambda {
                let p = -t * lt;
                self.push(a, ra, p);
                if let (Some(b), Some(rb)) = (c.b, rb) {
                    self.push(b as usize, rb, -p);
                }
            }
        }
    }

    /// Velocities from the substep's movement, then bounce and sliding
    /// friction.
    fn update_velocities(&mut self, h: f32) {
        for (b, r) in self.bodies.iter_mut().zip(&mut self.rigs) {
            if !r.alive || r.ghost || r.still >= SLEEP_AFTER {
                continue;
            }
            b.vel = (b.pos - r.prev_pos) / h;
            let q = b.rot;
            let p = r.prev_rot;
            let d = math::rot_mul(q, [-p[0], -p[1], -p[2], p[3]]);
            let s = if d[3] < 0.0 { -2.0 } else { 2.0 };
            r.omega = Vec3::new(d[0], d[1], d[2]) * (s / h);
        }
        let (e, mu) = (
            self.set.bounce.clamp(0.0, 1.0),
            self.set.friction.clamp(0.0, 1.5),
        );
        let g = self.set.gravity.abs();
        for k in 0..self.contacts.len() {
            let c = self.contacts[k];
            if c.lambda <= 0.0 {
                continue;
            }
            let a = c.a as usize;
            let ra = rotate(self.bodies[a].rot, c.la);
            let rb = c.b.map(|b| rotate(self.bodies[b as usize].rot, c.lb));
            let mut v = self.point_vel(a, ra);
            if let (Some(b), Some(rb)) = (c.b, rb) {
                v -= self.point_vel(b as usize, rb);
            }
            let vn = v.dot(c.n);
            let vt = v - c.n * vn;
            let mut dv = Vec3::ZERO;
            // Sliding friction, as strong as the contact's push.
            let vtl = vt.length();
            if vtl > 1e-6 {
                dv -= vt / vtl * (mu * c.lambda / h).min(vtl);
            }
            // Bounce, except for slow contacts (so resting bodies rest).
            let bounce = if c.vn.abs() > 2.0 * g * h { e } else { 0.0 };
            dv += c.n * (-vn + (-bounce * c.vn).max(0.0));
            let l = dv.length();
            if l < 1e-9 {
                continue;
            }
            let dir = dv / l;
            let wa = self.weight(a, ra, dir);
            let wb = match (c.b, rb) {
                (Some(b), Some(rb)) => self.weight(b as usize, rb, dir),
                _ => 0.0,
            };
            if wa + wb <= 0.0 {
                continue;
            }
            let p = dv / (wa + wb);
            if !self.asleep(a) {
                self.bodies[a].vel += p * self.inv_mass;
                let w = self.inv_i(a, ra.cross(p));
                self.rigs[a].omega += w;
            }
            if let (Some(b), Some(rb)) = (c.b, rb) {
                let b = b as usize;
                if !self.asleep(b) {
                    self.bodies[b].vel -= p * self.inv_mass;
                    let w = self.inv_i(b, rb.cross(p));
                    self.rigs[b].omega -= w;
                }
            }
        }
    }

    /// Rain: drops, vanishing and waiting, from each body's age.
    fn rain_life(&mut self, phase: f32, dt: f32) {
        let s = &self.set;
        for i in 0..self.bodies.len() {
            let age = s.age(i as u32, phase);
            let life = self.life_phase;
            let (b, r) = (&mut self.bodies[i], &mut self.rigs[i]);
            if age < life {
                if !r.alive {
                    // Its drop: from its start, the same every loop.
                    let (p, rot, w) = s.spawn(i as u32, self.half);
                    *b = Body {
                        pos: p,
                        vel: Vec3::ZERO,
                        rot,
                        size: 1.0,
                    };
                    r.omega = w;
                    r.alive = true;
                    r.ghost = false;
                    r.still = 0.0;
                }
                let left = (life - age) / (life * VANISH).max(1e-4);
                if left < 1.0 {
                    match s.vanish {
                        Vanish::Shrink => b.size = left.max(0.0),
                        Vanish::Sink => {
                            // Sink its own height through the floor.
                            r.ghost = true;
                            r.still = 0.0;
                            let depth = self.half.max_element() * 2.2;
                            let speed = depth / (life * VANISH * self.loop_s);
                            b.vel = Vec3::new(0.0, -speed, 0.0);
                            b.pos += b.vel * dt;
                            r.omega = Vec3::ZERO;
                        }
                    }
                }
            } else {
                // Dead: hidden, then parked at its start.
                r.alive = false;
                r.still = 0.0;
                r.ghost = false;
                b.size = 0.0;
                b.vel = Vec3::ZERO;
                r.omega = Vec3::ZERO;
                if age > life + PARK {
                    let (p, rot, _) = s.spawn(i as u32, self.half);
                    b.pos = p;
                    b.rot = rot;
                }
            }
        }
    }
}

impl Sim for PhysicsSim {
    fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    fn bodies_mut(&mut self) -> &mut [Body] {
        &mut self.bodies
    }

    fn rotations(&self) -> bool {
        true
    }

    fn step(&mut self, dt: f32, ctx: &EvalCtx) {
        let loop_beats = ctx.loop_beats.max(1) as f32;
        self.loop_s = (loop_beats * ctx.beat_seconds).max(0.05);
        // Schedules follow the real clock (the bake runs in real time).
        let phase = ctx.beat_phase.rem_euclid(1.0);
        match self.set.scenario {
            Scenario::Rain => {
                self.life_phase = (self.set.life / loop_beats).clamp(0.02, 1.0 - 3.0 * PARK);
                self.rain_life(phase, dt);
            }
            Scenario::Stack => {
                let beat = phase * loop_beats;
                let at = self.set.blast_beat.rem_euclid(loop_beats);
                let crossed = match self.last_beat {
                    Some(last) if last <= beat => last < at && at <= beat,
                    Some(last) => at > last || at <= beat,
                    None => false,
                };
                self.last_beat = Some(beat);
                if crossed && self.set.blast != 0.0 {
                    let centre = Vec3::from(self.set.blast_at);
                    for i in 0..self.bodies.len() {
                        let d = self.bodies[i].pos - centre;
                        let dist = d.length().max(1e-3);
                        let kick = self.set.blast / (1.0 + dist * dist * 0.15);
                        self.bodies[i].vel += d / dist * kick;
                        self.rigs[i].still = 0.0;
                        let r = |k: u32| hash2(self.set.seed ^ 0xb1a5, i as u32 * 8 + k) - 0.5;
                        self.rigs[i].omega += Vec3::new(r(0), r(1), r(2)) * (kick * 0.8);
                    }
                }
            }
        }
        let g = Vec3::new(0.0, -self.set.gravity, 0.0);
        let h = dt / SUBSTEPS as f32;
        for _ in 0..SUBSTEPS {
            for (b, r) in self.bodies.iter_mut().zip(&mut self.rigs) {
                r.prev_pos = b.pos;
                r.prev_rot = b.rot;
                if !r.alive || r.ghost || r.still >= SLEEP_AFTER {
                    continue;
                }
                b.vel += g * h;
                b.pos += b.vel * h;
                b.rot = math::rot_integrate(b.rot, r.omega, h);
            }
            // A guided tail: pull towards the start inside the substep,
            // before the contacts, so floors and neighbours still push
            // back (the velocities then include the pull).
            if !self.guide.is_empty() {
                let f = (self.guide_rate * h).min(0.5);
                for (k, to) in self.guide.iter().enumerate() {
                    let (b, r) = (&mut self.bodies[k], &mut self.rigs[k]);
                    // The dead wait for their next drop, the same every
                    // loop: nothing to pull.
                    if !r.alive {
                        continue;
                    }
                    r.still = 0.0;
                    b.pos += (to.pos - b.pos) * f;
                    b.rot = math::rot_nlerp(b.rot, to.rot, f);
                    b.size += (to.size - b.size) * f;
                }
            }
            for r in &mut self.rigs {
                r.support = 0;
            }
            self.find_contacts();
            self.solve_contacts(h);
            self.update_velocities(h);
        }
        self.guide.clear();
        for (b, r) in self.bodies.iter_mut().zip(&mut self.rigs) {
            if r.still >= SLEEP_AFTER {
                continue;
            }
            let calm = b.vel.length() < SLEEP_SPEED
                && r.omega.length() < SLEEP_SPIN
                && (r.support >= 3 || self.set.collider == Collider::Ball);
            r.still = if calm && r.alive && !r.ghost {
                r.still + dt
            } else {
                0.0
            };
            if r.still >= SLEEP_AFTER {
                b.vel = Vec3::ZERO;
                r.omega = Vec3::ZERO;
            }
        }
    }

    /// Steers back to the start inside the next step's substeps (see
    /// `step`), so bodies keep out of the floor and each other while the
    /// loop closes.
    fn guide(&mut self, targets: &[Body], gain: f32, dt: f32, drift: &mut [Vec3]) {
        drift.fill(Vec3::ZERO);
        self.guide.clear();
        self.guide.extend_from_slice(targets);
        self.guide_rate = gain / dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Bake, BakeJob, Frame, KeyHasher, SimClock};
    use crate::Timing;

    fn clock() -> SimClock {
        SimClock::new(Timing {
            bpm: 120.0,
            loop_beats: 16,
        })
    }

    fn bake(p: &Physics) -> Bake {
        BakeJob::new(Box::new(p.sim(Vec3::ONE)), clock(), p.looping.clone()).finish()
    }

    fn at(b: &Bake, phase: f32) -> Vec<Body> {
        let mut f = Frame::default();
        b.sample(phase, &mut f);
        f.layers.swap_remove(0).bodies
    }

    fn lowest(p: &Physics, b: &Body) -> f32 {
        let s = PhysicsSim { ..p.sim(Vec3::ONE) };
        s.box_points()
            .iter()
            .take(8)
            .map(|lp| (b.pos + rotate(b.rot, *lp)).y)
            .fold(f32::MAX, f32::min)
    }

    #[test]
    fn a_resting_wall_stays_put() {
        let p = Physics {
            blast: 0.0,
            ..Physics::stack()
        };
        let mut sim = p.sim(Vec3::ONE);
        let start: Vec<Vec3> = sim.bodies().iter().map(|b| b.pos).collect();
        let c = clock();
        for s in 0..240 {
            sim.step(1.0 / 120.0, &c.ctx(s as f32 / 960.0));
        }
        let moved = sim
            .bodies()
            .iter()
            .zip(&start)
            .map(|(b, s)| b.pos.distance(*s))
            .fold(0.0, f32::max);
        assert!(moved < 1e-3, "a resting wall moved {moved} in 2 s");
    }

    #[test]
    fn a_blast_knocks_the_wall_down_and_ping_pong_rebuilds_it() {
        let p = Physics::stack();
        let b = bake(&p);
        let start = at(&b, 0.0);
        let fresh = p.sim(Vec3::ONE);
        for (a, s) in start.iter().zip(fresh.bodies()) {
            assert!(a.pos.distance(s.pos) < 1e-3, "not intact at the start");
        }
        let (early, down) = (at(&b, 0.1), at(&b, 0.5));
        assert!(early
            .iter()
            .zip(&start)
            .all(|(a, s)| a.pos.distance(s.pos) < 1e-3));
        let fallen = down
            .iter()
            .zip(&start)
            .filter(|(a, s)| a.pos.distance(s.pos) > 0.5)
            .count();
        assert!(fallen * 2 > down.len(), "only {fallen} fell");
        // Rebuilt at the end, mirrored in the middle.
        let end = at(&b, 1.0 - 1e-5);
        for (a, s) in end.iter().zip(&start) {
            assert!(a.pos.distance(s.pos) < 1e-3);
        }
        for (a, z) in at(&b, 0.3).iter().zip(&at(&b, 0.7)) {
            assert!(a.pos.distance(z.pos) < 1e-3);
        }
    }

    #[test]
    fn nothing_falls_through_the_floor() {
        for p in [
            Physics::default(),
            Physics::stack(),
            Physics {
                collider: Collider::Ball,
                ..Physics::default()
            },
        ] {
            let b = bake(&p);
            for k in 0..32 {
                for body in at(&b, k as f32 / 32.0) {
                    if body.size < 0.5
                        || (p.vanish == Vanish::Sink
                            && p.scenario == Scenario::Rain
                            && body.size < 1.0)
                    {
                        continue;
                    }
                    let low = match p.collider {
                        Collider::Box => lowest(&p, &body),
                        Collider::Ball => body.pos.y - p.extent,
                    };
                    assert!(low > p.floor - 0.05, "{:?}: {low} at {k}/32", p.scenario);
                }
            }
        }
    }

    #[test]
    fn rain_drops_piles_and_loops() {
        let p = Physics::default();
        let b = bake(&p);
        let (start, end) = (at(&b, 0.0), at(&b, 1.0 - 1e-5));
        for (a, z) in start.iter().zip(&end) {
            assert!((a.size - z.size).abs() < 1e-3);
            // Hidden bodies may move to their next drop unseen.
            if a.size > 0.05 {
                assert!(
                    a.pos.distance(z.pos) < 1e-3,
                    "size {} vs {}: {} vs {}",
                    a.size,
                    z.size,
                    a.pos,
                    z.pos
                );
            }
        }
        // Some are falling, some resting on the floor or each other.
        let mid = at(&b, 0.5);
        let shown = mid.iter().filter(|b| b.size > 0.5).count();
        // Closing the loop doesn't drag bodies over the floor: they slide
        // at most about twice as fast in the tail as the rest of the loop
        // (measured: see the roadmap, 10.4).
        let slide = |from: f32, to: f32| {
            (0..400)
                .map(|k| from + (to - from) * k as f32 / 400.0)
                .flat_map(|ph| at(&b, ph))
                .filter(|b| b.size > 0.3 && b.pos.y < 1.0)
                .map(|b| Vec3::new(b.vel.x, 0.0, b.vel.z).length())
                .fold(0.0, f32::max)
        };
        let (body, tail) = (slide(0.0, 0.5), slide(0.5, 1.0));
        eprintln!(
            "rain slides at {tail:.2} while closing, {body:.2} otherwise; seam {:?}",
            b.seam()
        );
        assert!(
            tail < body * 2.0 + 1.0,
            "slides at {tail} while closing, {body} otherwise"
        );
        let over_loop = |f: &dyn Fn(&Body) -> bool| {
            (0..8)
                .flat_map(|k| at(&b, k as f32 / 8.0))
                .filter(|b| b.size > 0.5 && f(b))
                .count()
        };
        // Landed: on the floor, or on top of others (a pile). New drops
        // keep the pile stirring, so few are still.
        let landed = over_loop(&|b| b.pos.y < 0.8 && b.vel.y > -2.0);
        let piled = over_loop(&|b| b.pos.y > 1.1 && b.pos.y < 2.5 && b.vel.length() < 1.5);
        let falling = over_loop(&|b| b.vel.y < -2.0);
        assert!(
            shown > 12 && landed > 30 && piled > 0 && falling > 8,
            "{shown} shown, {landed} landed, {piled} piled, {falling} falling"
        );
        eprintln!(
            "rain seam {:?} after {} warm-up loops",
            b.seam(),
            b.warmup_loops()
        );
    }

    #[test]
    fn rain_bodies_sink_through_the_floor() {
        let p = Physics {
            vanish: Vanish::Sink,
            ..Physics::default()
        };
        let b = bake(&p);
        let mut sunk = 0;
        for k in 0..64 {
            for body in at(&b, k as f32 / 64.0) {
                if body.size > 0.0 && body.pos.y < p.floor {
                    sunk += 1;
                }
            }
        }
        assert!(sunk > 10, "{sunk}");
    }

    /// Rigid bodies bake the same on every platform; update only when
    /// physics is meant to change.
    #[test]
    fn same_physics_everywhere() {
        let p = Physics {
            count: 30,
            ..Physics::default()
        };
        let hash = bake(&p)
            .raw()
            .iter()
            .fold(KeyHasher::new(), |h, v| h.f32(*v))
            .finish();
        assert_eq!(hash, 0x6c37_7e57_11d5_4fa8, "physics hash {hash:#018x}");
    }
}

#[cfg(test)]
mod saving {
    use crate::*;

    #[test]
    fn physics_saves_and_loads() {
        let p = presets::beat_demolition();
        let json = p.to_json();
        assert!(json.contains("\"Physics\"") && !json.contains("placed"));
        assert_eq!(Project::from_json(&json).unwrap(), p);
    }

    #[test]
    #[ignore]
    fn time_bakes() {
        use crate::sim::*;
        let clock = SimClock::new(Timing {
            bpm: 120.0,
            loop_beats: 16,
        });
        for (name, p) in [
            ("rain 60", Physics::default()),
            (
                "rain 200",
                Physics {
                    count: 200,
                    area: 6.0,
                    ..Physics::default()
                },
            ),
            (
                "wall 8x6",
                Physics {
                    counts: [8, 6, 1],
                    ..Physics::stack()
                },
            ),
            (
                "tower 5x16x5",
                Physics {
                    counts: [5, 16, 5],
                    ..Physics::stack()
                },
            ),
        ] {
            let t = std::time::Instant::now();
            let b = BakeJob::new(
                Box::new(p.sim(glam::Vec3::ONE)),
                clock.clone(),
                p.looping.clone(),
            )
            .finish();
            println!(
                "{name}: {:.2}s {:.1}MB seam {:?}",
                t.elapsed().as_secs_f32(),
                b.bytes() as f32 / 1e6,
                b.seam()
            );
        }
    }
}
