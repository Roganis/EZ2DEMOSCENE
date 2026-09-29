//! Running a simulation into a looping bake, and reading it at a phase.

use super::math::{self, Rot};
use super::{Body, LoopClose, Sim, SimLoop};
use crate::{AudioEnvelope, EvalCtx, MusicSettings, Timing};
use glam::Vec3;
use std::sync::Arc;

/// Simulation steps per second of loop time (whatever the export rate).
pub const STEPS_PER_SECOND: f64 = 120.0;
/// Keys kept per second of loop time...
pub const KEYS_PER_SECOND: f64 = 60.0;
/// ...but at most this many per loop (long loops get sparser keys)...
pub const MAX_KEYS: usize = 480;
/// ...and at least this many.
pub const MIN_KEYS: usize = 16;
/// How a guided tail closes: the gap left shrinks like the time left to
/// this power.
const GUIDE_RATE: f64 = 3.0;

/// What a simulation's settings may depend on: the loop and its music.
#[derive(Clone, Debug)]
pub struct SimClock {
    pub timing: Timing,
    pub music: MusicSettings,
    pub audio: Option<Arc<AudioEnvelope>>,
}

impl SimClock {
    /// A loop without music.
    pub fn new(timing: Timing) -> SimClock {
        SimClock {
            timing,
            music: MusicSettings::default(),
            audio: None,
        }
    }

    /// A loop with its music (the loop window of the song).
    pub fn with_music(
        timing: Timing,
        music: MusicSettings,
        audio: Option<Arc<AudioEnvelope>>,
    ) -> SimClock {
        SimClock {
            timing,
            music,
            audio,
        }
    }

    /// The loop moment at `phase`.
    pub fn ctx(&self, phase: f32) -> EvalCtx {
        EvalCtx::with_music(&self.timing, &self.music, phase, self.audio.as_deref())
    }
}

/// The fixed time grid of a bake: keys per loop, steps per key, and which
/// keys are recorded.
#[derive(Clone, Copy, Debug)]
struct Grid {
    close: LoopClose,
    /// Keys per loop (even).
    keys: usize,
    /// Simulation steps per key.
    sub: usize,
    loop_s: f64,
    /// Keys recorded before the loop starts (the blended tail's pre-roll).
    pre: usize,
    /// Keys recorded.
    count: usize,
}

impl Grid {
    fn new(loop_s: f32, set: &SimLoop) -> Grid {
        let loop_s = loop_s.max(0.05) as f64;
        let mut keys = ((loop_s * KEYS_PER_SECOND).round() as usize).clamp(MIN_KEYS, MAX_KEYS);
        keys += keys % 2;
        let sub = ((loop_s * STEPS_PER_SECOND / keys as f64).ceil() as usize).max(1);
        let (pre, count) = match set.close {
            LoopClose::CrossFade => (0, keys + 1),
            LoopClose::BlendTail => {
                let pre = ((set.blend.clamp(0.02, 0.5) as f64 * keys as f64).round() as usize)
                    .clamp(1, keys / 2);
                (pre, pre + keys + 1)
            }
            LoopClose::PingPong => (0, keys / 2 + 1),
        };
        Grid {
            close: set.close,
            keys,
            sub,
            loop_s,
            pre,
            count,
        }
    }

    fn key_s(&self) -> f64 {
        self.loop_s / self.keys as f64
    }

    fn dt(&self) -> f64 {
        self.key_s() / self.sub as f64
    }

    fn loop_steps(&self) -> i64 {
        (self.keys * self.sub) as i64
    }

    /// Step at which recording starts (negative with a pre-roll).
    fn first_step(&self) -> i64 {
        -((self.pre * self.sub) as i64)
    }

    /// Loop phase at the start of step `s` (step 0 is the loop's start),
    /// exact for any number of loops.
    fn phase(&self, s: i64) -> f32 {
        (s.rem_euclid(self.loop_steps()) as f64 / self.loop_steps() as f64) as f32
    }

    /// Length of the blended tail in seconds.
    fn tail_s(&self) -> f64 {
        self.pre as f64 * self.key_s()
    }
}

/// Recorded keys: per key and body, position, velocity, size and (if
/// kept) rotation.
#[derive(Clone, Debug)]
struct Track {
    bodies: usize,
    rot: bool,
    /// Time of the first key, in seconds from the loop's start.
    start_s: f64,
    key_s: f64,
    /// Keys recorded (counted apart from `data`, which a simulation
    /// without bodies leaves empty).
    keys: usize,
    data: Vec<f32>,
}

impl Track {
    fn stride(&self) -> usize {
        if self.rot {
            11
        } else {
            7
        }
    }

    fn len(&self) -> usize {
        self.keys
    }

    /// Records a key; `drift` is movement the simulation's velocities
    /// don't include (a guided tail's pull), per body, or empty.
    fn push(&mut self, bodies: &[Body], drift: &[Vec3]) {
        debug_assert_eq!(
            bodies.len(),
            self.bodies,
            "a simulation changed its number of bodies"
        );
        self.keys += 1;
        for (i, b) in bodies.iter().enumerate() {
            let vel = b.vel + drift.get(i).copied().unwrap_or(Vec3::ZERO);
            self.data
                .extend_from_slice(&[b.pos.x, b.pos.y, b.pos.z, vel.x, vel.y, vel.z, b.size]);
            if self.rot {
                self.data.extend_from_slice(&b.rot);
            }
        }
    }

    /// Body `i` at key `k`: position, velocity, size, rotation.
    fn key(&self, k: usize, i: usize) -> (Vec3, Vec3, f32, Rot) {
        let s = self.stride();
        let d = &self.data[(k * self.bodies + i) * s..][..s];
        let rot = if self.rot {
            [d[7], d[8], d[9], d[10]]
        } else {
            math::ROT_IDENTITY
        };
        (
            Vec3::new(d[0], d[1], d[2]),
            Vec3::new(d[3], d[4], d[5]),
            d[6],
            rot,
        )
    }

    /// Every body at `t` seconds from the loop's start (clamped to what is
    /// recorded): cubic Hermite through the keys' positions and velocities.
    fn state_at(&self, t: f64, out: &mut Vec<Body>) {
        out.resize(self.bodies, Body::default());
        let n = self.len();
        if n == 0 {
            return;
        }
        let kf = (t - self.start_s) / self.key_s;
        let k = (kf.floor().max(0.0) as usize).min(n.saturating_sub(2));
        let u = ((kf - k as f64) as f32).clamp(0.0, 1.0);
        let k1 = (k + 1).min(n - 1);
        let d = self.key_s as f32;
        let (u2, u3) = (u * u, u * u * u);
        let (h00, h10, h01, h11) = (
            2.0 * u3 - 3.0 * u2 + 1.0,
            u3 - 2.0 * u2 + u,
            -2.0 * u3 + 3.0 * u2,
            u3 - u2,
        );
        let (g00, g10, g01, g11) = (
            6.0 * u2 - 6.0 * u,
            3.0 * u2 - 4.0 * u + 1.0,
            -6.0 * u2 + 6.0 * u,
            3.0 * u2 - 2.0 * u,
        );
        for (i, b) in out.iter_mut().enumerate() {
            let (p0, v0, s0, r0) = self.key(k, i);
            let (p1, v1, s1, r1) = self.key(k1, i);
            b.pos = p0 * h00 + v0 * (d * h10) + p1 * h01 + v1 * (d * h11);
            b.vel = (p0 * g00 + p1 * g01) / d + v0 * g10 + v1 * g11;
            b.size = s0 + (s1 - s0) * u;
            b.rot = if self.rot {
                math::rot_nlerp(r0, r1, u)
            } else {
                r0
            };
        }
    }
}

fn smootherstep(u: f32) -> f32 {
    u * u * u * (u * (u * 6.0 - 15.0) + 10.0)
}

/// Slope of [`smootherstep`].
fn smootherstep_d(u: f32) -> f32 {
    30.0 * u * u * (1.0 - u) * (1.0 - u)
}

/// `a` moved towards `b` by `w`, velocities included; `dw` is how fast `w`
/// changes (per second), which moves the bodies too.
fn blend(a: &Body, b: &Body, w: f32, dw: f32) -> Body {
    Body {
        pos: a.pos + (b.pos - a.pos) * w,
        vel: a.vel + (b.vel - a.vel) * w + (b.pos - a.pos) * dw,
        rot: math::rot_nlerp(a.rot, b.rot, w),
        size: a.size + (b.size - a.size) * w,
    }
}

/// How far apart the end of the simulated loop and its start are, before
/// the loop is closed: what the closing has to hide.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Seam {
    /// Largest distance of any body, in world units.
    pub max: f32,
    /// Root mean square distance over the bodies.
    pub rms: f32,
}

/// The bodies at one moment, in one or two layers: a cross-faded bake is
/// two copies half a loop apart, each drawn with its weight (opacity or
/// brightness); the weights add up to 1. Reuse one `Frame` from frame to
/// frame: sampling doesn't allocate once it has grown.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub layers: Vec<FrameLayer>,
    scratch: Vec<Body>,
}

#[derive(Clone, Debug, Default)]
pub struct FrameLayer {
    pub weight: f32,
    pub bodies: Vec<Body>,
}

/// A simulation run into keys that loop. Read it with [`Bake::sample`].
#[derive(Clone, Debug)]
pub struct Bake {
    grid: Grid,
    track: Track,
    seam: Seam,
    warmup_loops: u32,
}

impl Bake {
    /// Bodies per layer.
    pub fn len(&self) -> usize {
        self.track.bodies
    }

    pub fn is_empty(&self) -> bool {
        self.track.bodies == 0
    }

    pub fn close(&self) -> LoopClose {
        self.grid.close
    }

    /// The gap the closing hides (see [`Seam`]); none for ping-pong.
    pub fn seam(&self) -> Seam {
        self.seam
    }

    /// Loops simulated before recording (fewer than asked once settled).
    pub fn warmup_loops(&self) -> u32 {
        self.warmup_loops
    }

    /// Keys per loop.
    pub fn keys_per_loop(&self) -> usize {
        self.grid.keys
    }

    /// Memory used by the keys, in bytes.
    pub fn bytes(&self) -> usize {
        self.track.data.len() * 4
    }

    /// The raw keys (for tests that the bake is the same everywhere).
    pub fn raw(&self) -> &[f32] {
        &self.track.data
    }

    /// The bodies at loop phase `phase` (wrapped into 0..1). Sample with
    /// the real loop phase (`EvalCtx::beat_phase`): the simulation ran in
    /// real time, without the music's time warp.
    pub fn sample(&self, phase: f32, out: &mut Frame) {
        let x = phase.rem_euclid(1.0) as f64;
        let l = self.grid.loop_s;
        let layers = if self.grid.close == LoopClose::CrossFade {
            2
        } else {
            1
        };
        out.layers.resize_with(layers, FrameLayer::default);
        match self.grid.close {
            LoopClose::CrossFade => {
                // Weight 0 exactly where each copy jumps back to the start.
                let wa = 1.0 - (2.0 * x - 1.0).abs() as f32;
                let xb = (x + 0.5).fract();
                self.track.state_at(x * l, &mut out.layers[0].bodies);
                self.track.state_at(xb * l, &mut out.layers[1].bodies);
                out.layers[0].weight = wa;
                out.layers[1].weight = 1.0 - wa;
            }
            LoopClose::BlendTail => {
                self.tail_at(x * l, &mut out.layers[0].bodies, &mut out.scratch);
                out.layers[0].weight = 1.0;
            }
            LoopClose::PingPong => {
                let u = 1.0 - (2.0 * x - 1.0).abs();
                let bodies = &mut out.layers[0].bodies;
                self.track.state_at(u * l * 0.5, bodies);
                if x > 0.5 {
                    bodies.iter_mut().for_each(|b| b.vel = -b.vel);
                }
                out.layers[0].weight = 1.0;
            }
        }
    }

    /// A blended-tail bake at `t` seconds into the loop.
    fn tail_at(&self, t: f64, out: &mut Vec<Body>, scratch: &mut Vec<Body>) {
        let l = self.grid.loop_s;
        let b = self.grid.tail_s();
        self.track.state_at(t, out);
        if t < l - b {
            return;
        }
        let u = ((t - (l - b)) / b).clamp(0.0, 1.0) as f32;
        let (w, dw) = (smootherstep(u), smootherstep_d(u) / b as f32);
        self.track.state_at(t - l, scratch);
        for (a, pre) in out.iter_mut().zip(scratch.iter()) {
            *a = blend(a, pre, w, dw);
        }
    }
}

enum Stage {
    Warm,
    Record,
    Done,
}

/// Runs a simulation into a [`Bake`], a slice at a time: [`BakeJob::step`]
/// until it returns `true`, then [`BakeJob::finish`] (which also runs
/// whatever is left). The result doesn't depend on the slicing.
pub struct BakeJob {
    sim: Box<dyn Sim>,
    clock: SimClock,
    set: SimLoop,
    grid: Grid,
    track: Track,
    stage: Stage,
    /// Next step to simulate (0 = the loop's start).
    step: i64,
    warmup_loops: u32,
    /// Steps into the current warm-up loop.
    warm_step: i64,
    /// The bodies at the start of the current warm-up loop.
    loop_start: Vec<Body>,
    targets: Vec<Body>,
    /// The guided tail's pull in the last step, per second.
    drift: Vec<Vec3>,
    done_steps: u64,
}

impl BakeJob {
    pub fn new(sim: Box<dyn Sim>, clock: SimClock, set: SimLoop) -> BakeJob {
        let grid = Grid::new(clock.timing.loop_seconds(), &set);
        let bodies = sim.bodies().len();
        let track = Track {
            bodies,
            rot: sim.rotations(),
            start_s: grid.first_step() as f64 * grid.dt(),
            key_s: grid.key_s(),
            keys: 0,
            data: Vec::with_capacity(bodies * grid.count * if sim.rotations() { 11 } else { 7 }),
        };
        let mut job = BakeJob {
            loop_start: sim.bodies().to_vec(),
            sim,
            clock,
            grid,
            track,
            stage: Stage::Warm,
            step: grid.first_step(),
            warmup_loops: 0,
            warm_step: 0,
            targets: Vec::new(),
            drift: Vec::new(),
            done_steps: 0,
            set,
        };
        if job.set.warmup == 0 {
            job.start_recording();
        }
        job
    }

    fn record_steps(&self) -> u64 {
        ((self.grid.count - 1) * self.grid.sub) as u64
    }

    /// 0..1 (an estimate while warming up: warm-up may stop early).
    pub fn progress(&self) -> f32 {
        let warm = match self.stage {
            Stage::Warm => self.set.warmup as u64 * self.grid.loop_steps() as u64,
            _ => self.warmup_loops as u64 * self.grid.loop_steps() as u64,
        };
        let total = warm + self.record_steps();
        if total == 0 {
            1.0
        } else {
            (self.done_steps as f64 / total as f64).min(1.0) as f32
        }
    }

    fn start_recording(&mut self) {
        self.stage = Stage::Record;
        self.track.push(self.sim.bodies(), &[]);
        if self.track.len() == self.grid.count {
            self.stage = Stage::Done;
        }
    }

    /// Simulate one step starting at step `s`.
    fn advance(&mut self, s: i64) {
        let ctx = self.clock.ctx(self.grid.phase(s));
        self.sim.step(self.grid.dt() as f32, &ctx);
        self.done_steps += 1;
    }

    /// Simulate up to `max_steps` more steps; `true` once the bake is done.
    pub fn step(&mut self, max_steps: usize) -> bool {
        let mut budget = max_steps;
        while budget > 0 {
            match self.stage {
                Stage::Done => break,
                Stage::Warm => {
                    // Warm-up loops cover the same phases as recording.
                    self.advance(self.grid.first_step() + self.warm_step);
                    self.warm_step += 1;
                    if self.warm_step == self.grid.loop_steps() {
                        self.warm_step = 0;
                        self.warmup_loops += 1;
                        if self.settled() || self.warmup_loops >= self.set.warmup {
                            self.start_recording();
                        } else {
                            self.loop_start.clear();
                            self.loop_start.extend_from_slice(self.sim.bodies());
                        }
                    }
                }
                Stage::Record => {
                    self.advance(self.step);
                    self.step += 1;
                    self.guide();
                    if (self.step - self.grid.first_step()) % self.grid.sub as i64 == 0 {
                        self.track.push(self.sim.bodies(), &self.drift);
                        if self.track.len() == self.grid.count {
                            self.stage = Stage::Done;
                        }
                    }
                }
            }
            budget -= 1;
        }
        matches!(self.stage, Stage::Done)
    }

    /// Whether the last warm-up loop ended within `settle` of where it
    /// started.
    fn settled(&self) -> bool {
        let n = self.loop_start.len();
        if n == 0 {
            return true;
        }
        let sum: f64 = self
            .sim
            .bodies()
            .iter()
            .zip(&self.loop_start)
            .map(|(a, b)| a.pos.distance_squared(b.pos) as f64)
            .sum();
        ((sum / n as f64).sqrt() as f32) < self.set.settle
    }

    /// In a guided tail, pull the bodies towards the pre-roll (where they
    /// were one loop earlier). Each step closes the fraction `GUIDE_RATE ×
    /// step / time left` of the gap (eased in at the tail's start), so the
    /// gap shrinks like the time left to the power `GUIDE_RATE` and is gone
    /// a few steps before the end; the simulation carries on from the
    /// pulled state, so collisions and neighbours react to it.
    fn guide(&mut self) {
        if self.grid.close != LoopClose::BlendTail || !self.set.guide {
            return;
        }
        let (l, b, dt) = (self.grid.loop_s, self.grid.tail_s(), self.grid.dt());
        let t = self.step as f64 * dt;
        if t <= l - b {
            self.drift.clear();
            return;
        }
        let u = ((t - (l - b)) / b).clamp(0.0, 1.0) as f32;
        let ramp = u * u * (3.0 - 2.0 * u);
        let left = l - t;
        let gain = if left > 0.0 {
            ramp * (GUIDE_RATE * dt / left).min(1.0) as f32
        } else {
            1.0
        };
        self.track.state_at(t - l, &mut self.targets);
        self.drift.resize(self.targets.len(), Vec3::ZERO);
        let bodies = self.sim.bodies_mut();
        for ((body, to), drift) in bodies.iter_mut().zip(&self.targets).zip(&mut self.drift) {
            let moved = (to.pos - body.pos) * gain;
            body.pos += moved;
            *drift = moved / dt as f32;
            body.vel += (to.vel - body.vel) * gain;
            body.rot = math::rot_nlerp(body.rot, to.rot, gain);
            body.size += (to.size - body.size) * gain;
        }
    }

    /// The bake (simulating whatever is left first).
    pub fn finish(mut self) -> Bake {
        self.step(usize::MAX);
        let seam = match self.grid.close {
            LoopClose::PingPong => Seam::default(),
            _ => {
                let (mut end, mut start) = (Vec::new(), Vec::new());
                self.track.state_at(self.grid.loop_s, &mut end);
                self.track.state_at(0.0, &mut start);
                let d: Vec<f32> = end
                    .iter()
                    .zip(&start)
                    .map(|(a, b)| a.pos.distance(b.pos))
                    .collect();
                let n = d.len().max(1) as f32;
                Seam {
                    max: d.iter().fold(0.0, |m: f32, &v| m.max(v)),
                    rms: (d.iter().map(|v| v * v).sum::<f32>() / n).sqrt(),
                }
            }
        };
        Bake {
            grid: self.grid,
            track: self.track,
            seam,
            warmup_loops: self.warmup_loops,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;
    use crate::sim::KeyHasher;

    /// Damped springs pushed by a force that turns whole cycles per loop:
    /// they settle into a cycle that repeats.
    struct Springs {
        bodies: Vec<Body>,
        anchors: Vec<Vec3>,
    }

    impl Springs {
        fn new(n: usize) -> Springs {
            let mut rng = Rng::new(7);
            let anchors: Vec<Vec3> = (0..n)
                .map(|_| Vec3::new(rng.signed(), rng.signed(), rng.signed()) * 3.0)
                .collect();
            Springs {
                bodies: anchors.iter().map(|&a| Body::at(a + Vec3::X)).collect(),
                anchors,
            }
        }
    }

    impl Sim for Springs {
        fn bodies(&self) -> &[Body] {
            &self.bodies
        }
        fn bodies_mut(&mut self) -> &mut [Body] {
            &mut self.bodies
        }
        fn step(&mut self, dt: f32, ctx: &EvalCtx) {
            let a = ctx.phase * std::f32::consts::TAU * 3.0;
            let push = Vec3::new(math::cos(a), math::sin(a * 2.0) * 0.5, math::sin(a)) * 20.0;
            for (b, &anchor) in self.bodies.iter_mut().zip(&self.anchors) {
                let acc = (anchor - b.pos) * 40.0 - b.vel * 1.6 + push;
                b.vel += acc * dt;
                b.pos += b.vel * dt;
            }
        }
    }

    /// Balls bouncing in a box, pushing each other apart, stirred by a
    /// turning force and spinning as they roll: chaotic, never repeats.
    struct Balls {
        bodies: Vec<Body>,
    }

    impl Balls {
        fn new(n: usize) -> Balls {
            let mut rng = Rng::new(3);
            Balls {
                bodies: (0..n)
                    .map(|_| Body {
                        vel: Vec3::new(rng.signed(), rng.signed(), rng.signed()),
                        ..Body::at(Vec3::new(rng.signed(), rng.signed(), rng.signed()) * 1.8)
                    })
                    .collect(),
            }
        }
    }

    impl Sim for Balls {
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
            let a = ctx.phase * std::f32::consts::TAU * 2.0;
            let stir = Vec3::new(math::cos(a), 0.0, math::sin(a)) * 6.0;
            let n = self.bodies.len();
            let mut acc = vec![Vec3::new(0.0, -9.8, 0.0) + stir; n];
            for i in 0..n {
                for j in i + 1..n {
                    let d = self.bodies[i].pos - self.bodies[j].pos;
                    let r = d.length();
                    if r < 0.6 && r > 1e-6 {
                        let f = d / r * (0.6 - r) * 400.0;
                        acc[i] += f;
                        acc[j] -= f;
                    }
                }
            }
            for (b, a) in self.bodies.iter_mut().zip(acc) {
                b.vel += a * dt;
                b.vel *= 1.0 - 0.2 * dt;
                b.pos += b.vel * dt;
                for k in 0..3 {
                    if b.pos[k].abs() > 2.0 {
                        b.pos[k] = b.pos[k].clamp(-2.0, 2.0);
                        b.vel[k] = -b.vel[k] * 0.8;
                    }
                }
                let spin = Vec3::Y.cross(b.vel) * 2.0;
                b.rot = math::rot_integrate(b.rot, spin, dt);
            }
        }
    }

    fn clock() -> SimClock {
        SimClock::new(Timing {
            bpm: 120.0,
            loop_beats: 8,
        })
    }

    fn bake(sim: Box<dyn Sim>, set: SimLoop) -> Bake {
        BakeJob::new(sim, clock(), set).finish()
    }

    fn set(close: LoopClose) -> SimLoop {
        SimLoop {
            close,
            ..SimLoop::default()
        }
    }

    /// Samples the loop densely; checks every layer moves no further between
    /// samples than its velocity allows (so positions are continuous and the
    /// velocities are their true slope), except where its weight is ~0
    /// (and the velocity where ping-pong turns back).
    fn check_continuous(b: &Bake) {
        let n = 6000;
        let l = clock().timing.loop_seconds();
        let dt = l / n as f32;
        let (mut prev, mut cur) = (Frame::default(), Frame::default());
        b.sample(0.0, &mut prev);
        for i in 1..=n {
            b.sample(i as f32 / n as f32, &mut cur);
            for (pl, cl) in prev.layers.iter().zip(&cur.layers) {
                if pl.weight.min(cl.weight) < 1e-2 {
                    continue;
                }
                for (p, c) in pl.bodies.iter().zip(&cl.bodies) {
                    let moved = p.pos.distance(c.pos);
                    let allowed = p.vel.length().max(c.vel.length()) * dt * 1.3 + 1e-4;
                    assert!(
                        moved <= allowed,
                        "jump at sample {i}: moved {moved}, allowed {allowed}"
                    );
                    // The velocity is the slope of the position.
                    if b.close() == LoopClose::PingPong && [1, n / 2, n / 2 + 1, n].contains(&i) {
                        continue;
                    }
                    let slope = (c.pos - p.pos) / dt;
                    let mid = (p.vel + c.vel) * 0.5;
                    assert!(
                        slope.distance(mid) <= 0.02 * mid.length().max(slope.length()) + 0.05,
                        "velocity off at sample {i}: {slope} vs {mid}"
                    );
                }
            }
            std::mem::swap(&mut prev, &mut cur);
        }
    }

    #[test]
    fn grid() {
        let g = Grid::new(4.0, &SimLoop::default());
        assert_eq!(g.keys, 240);
        assert_eq!(g.sub, 2);
        assert_eq!(g.pre, 60);
        assert_eq!(g.count, 301);
        assert!((g.dt() - 1.0 / 120.0).abs() < 1e-12);
        assert_eq!(g.phase(g.first_step()), 0.75);
        assert_eq!(g.phase(g.loop_steps() * 5), 0.0);
        // Long loops get sparser keys, short ones a minimum.
        assert_eq!(Grid::new(64.0, &SimLoop::default()).keys, MAX_KEYS);
        assert_eq!(Grid::new(0.1, &SimLoop::default()).keys, MIN_KEYS);
        let odd = Grid::new(1.0 / 60.0 * 17.0, &SimLoop::default());
        assert_eq!(odd.keys % 2, 0);
    }

    #[test]
    fn blend_tail_arrives_at_the_start() {
        for guide in [false, true] {
            let b = bake(
                Box::new(Balls::new(40)),
                SimLoop {
                    guide,
                    ..SimLoop::default()
                },
            );
            let l = b.grid.loop_s;
            let (mut end, mut start, mut scratch) = (Vec::new(), Vec::new(), Vec::new());
            b.tail_at(l, &mut end, &mut scratch);
            b.tail_at(0.0, &mut start, &mut scratch);
            for (e, s) in end.iter().zip(&start) {
                assert!(e.pos.distance(s.pos) < 1e-5);
                assert!(e.vel.distance(s.vel) < 1e-4);
                let dot: f32 = e.rot.iter().zip(s.rot).map(|(a, b)| a * b).sum();
                assert!(dot.abs() > 1.0 - 1e-5);
            }
            // The loop genuinely didn't close by itself; guided, the
            // simulation itself arrived at the start.
            if guide {
                assert!(b.seam().max < 1e-5, "{:?}", b.seam());
            } else {
                assert!(b.seam().max > 0.1, "{:?}", b.seam());
            }
            check_continuous(&b);
        }
    }

    #[test]
    fn guiding_the_tail_leaves_less_to_blend() {
        let free = bake(
            Box::new(Balls::new(40)),
            SimLoop {
                guide: false,
                ..SimLoop::default()
            },
        );
        let guided = bake(Box::new(Balls::new(40)), SimLoop::default());
        assert!(
            guided.seam().rms < free.seam().rms * 0.1,
            "{:?} vs {:?}",
            guided.seam(),
            free.seam()
        );
    }

    #[test]
    fn cross_fade_hides_each_jump() {
        let b = bake(Box::new(Balls::new(30)), set(LoopClose::CrossFade));
        let mut f = Frame::default();
        for (x, a, w) in [
            (0.0, 0.0, 1.0),
            (0.25, 0.5, 0.5),
            (0.5, 1.0, 0.0),
            (0.75, 0.5, 0.5),
        ] {
            b.sample(x, &mut f);
            assert_eq!(f.layers.len(), 2);
            assert!((f.layers[0].weight - a).abs() < 1e-6 && (f.layers[1].weight - w).abs() < 1e-6);
        }
        // Each copy is the same simulation half a loop apart.
        let (mut g, mut h) = (Frame::default(), Frame::default());
        b.sample(0.1, &mut g);
        b.sample(0.6, &mut h);
        for (a, b) in g.layers[0].bodies.iter().zip(&h.layers[1].bodies) {
            assert!(a.pos.distance(b.pos) < 1e-4 && a.vel.distance(b.vel) < 1e-3);
        }
        check_continuous(&b);
    }

    #[test]
    fn ping_pong_turns_back() {
        let b = bake(Box::new(Balls::new(30)), set(LoopClose::PingPong));
        assert_eq!(b.seam(), Seam::default());
        let (mut f, mut g) = (Frame::default(), Frame::default());
        b.sample(0.2, &mut f);
        b.sample(0.8, &mut g);
        for (a, b) in f.layers[0].bodies.iter().zip(&g.layers[0].bodies) {
            assert!(a.pos.distance(b.pos) < 1e-4);
            assert!(a.vel.distance(-b.vel) < 1e-3);
        }
        check_continuous(&b);
    }

    #[test]
    fn warm_up_settles_a_repeating_system() {
        let cold = bake(
            Box::new(Springs::new(20)),
            SimLoop {
                warmup: 0,
                guide: false,
                ..SimLoop::default()
            },
        );
        let warm = bake(
            Box::new(Springs::new(20)),
            SimLoop {
                warmup: 6,
                settle: 1e-3,
                guide: false,
                ..SimLoop::default()
            },
        );
        assert!(cold.seam().max > 0.05, "{:?}", cold.seam());
        assert!(warm.seam().max < 1e-3, "{:?}", warm.seam());
        // It stopped as soon as it settled.
        assert!(
            warm.warmup_loops() >= 2 && warm.warmup_loops() < 6,
            "{}",
            warm.warmup_loops()
        );
        check_continuous(&warm);
    }

    #[test]
    fn slicing_does_not_change_the_bake() {
        let whole = bake(Box::new(Balls::new(25)), SimLoop::default());
        let mut job = BakeJob::new(Box::new(Balls::new(25)), clock(), SimLoop::default());
        let mut last = 0.0;
        while !job.step(7) {
            let p = job.progress();
            assert!(p >= last && p < 1.0);
            last = p;
        }
        assert_eq!(job.progress(), 1.0);
        let sliced = job.finish();
        assert_eq!(whole.raw(), sliced.raw());
        assert_eq!(whole.bytes(), 25 * 301 * 11 * 4);
    }

    #[test]
    fn music_drives_the_simulation_at_the_simulated_moment() {
        // A force from the settings' Param, evaluated at each step's
        // moment: a beat fade must change the bake.
        struct Kicked(Vec<Body>, crate::Param);
        impl Sim for Kicked {
            fn bodies(&self) -> &[Body] {
                &self.0
            }
            fn bodies_mut(&mut self) -> &mut [Body] {
                &mut self.0
            }
            fn step(&mut self, dt: f32, ctx: &EvalCtx) {
                let f = self.1.eval(ctx);
                for b in &mut self.0 {
                    b.vel += (Vec3::Y * f - b.pos * 10.0 - b.vel) * dt;
                    b.pos += b.vel * dt;
                }
            }
        }
        let still = crate::Param::new(0.0);
        let kicked = crate::Param::new(0.0).osc(crate::Wave::Saw, 50.0, 4);
        let a = bake(
            Box::new(Kicked(vec![Body::default()], still)),
            set(LoopClose::PingPong),
        );
        let b = bake(
            Box::new(Kicked(vec![Body::default()], kicked)),
            set(LoopClose::PingPong),
        );
        assert!(a.raw().iter().all(|&v| v == 0.0 || v == 1.0));
        assert!(b.raw().iter().any(|&v| v.abs() > 0.01));
    }

    #[test]
    fn nothing_to_simulate() {
        for close in LoopClose::ALL {
            let b = bake(Box::new(Balls::new(0)), set(close));
            assert!(b.is_empty() && b.raw().is_empty());
            let mut f = Frame::default();
            b.sample(0.3, &mut f);
            assert!(f.layers.iter().all(|l| l.bodies.is_empty()));
        }
    }

    /// The same bake on every platform: the keys of a chaotic simulation
    /// (any different rounding would have grown into a different state).
    /// CI runs this on Linux, Windows and macOS; update the value only when
    /// a simulation change is meant to change bakes.
    #[test]
    fn same_bake_everywhere() {
        let b = bake(Box::new(Balls::new(40)), SimLoop::default());
        let hash = b
            .raw()
            .iter()
            .fold(KeyHasher::new(), |h, v| h.f32(*v))
            .finish();
        assert_eq!(hash, 0x2276_c75f_ada8_74f6, "bake hash {hash:#018x}");
    }
}
