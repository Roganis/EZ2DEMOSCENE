//! A liquid: position-based fluids (Macklin & Müller 2013). Each step the
//! droplets move, then are nudged a few times until the density around
//! each is right (neither squeezed nor, up to the cohesion setting,
//! pulled apart), then share some of their neighbours' velocity
//! (viscosity). They stay in a container: a box, a bowl or a round pool
//! on the floor, which the layer can tilt back and forth (the simulation
//! runs in the container's frame, where the pull of gravity swings).

use super::math::{self, Rot};
use super::{Body, LoopClose, Sim, SimLoop};
use crate::rng::hash2;
use crate::{EvalCtx, Param};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

/// Most droplets (the bake keeps every one at every key).
pub const FLUID_MAX: u32 = 8000;
/// Density corrections per step.
const ITERATIONS: usize = 3;
/// Most neighbours counted per droplet.
const MAX_NEIGHBOURS: usize = 64;
/// Part of a poured droplet's life spent shrinking away.
const VANISH: f32 = 0.15;
/// Droplets move at most this fast (units per second): a squeezed
/// droplet corrected at once would shoot off.
const MAX_SPEED: f32 = 30.0;

/// What holds the liquid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Container {
    /// A closed box around the layer's origin, `size` from the middle to
    /// each side (the built-in cube at size × 2).
    Box,
    /// A round bowl of radius `size` around the layer's origin (the
    /// built-in bowl at size; the whole sphere holds the liquid, so it
    /// can't slosh out).
    #[default]
    Bowl,
    /// A round pool of radius `size` on the floor: liquid spreads out.
    Floor,
}

impl Container {
    pub const ALL: [Container; 3] = [Container::Box, Container::Bowl, Container::Floor];
    pub fn label(self) -> &'static str {
        match self {
            Container::Box => "Box",
            Container::Bowl => "Bowl",
            Container::Floor => "Pool on the floor",
        }
    }
}

/// Where the liquid comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    /// All of it is in the container from the start and sloshes about.
    #[default]
    Fill,
    /// A stream pours in from above, each droplet once per loop, living
    /// `life` beats and then shrinking away.
    Pour,
}

impl Source {
    pub const ALL: [Source; 2] = [Source::Fill, Source::Pour];
    pub fn label(self) -> &'static str {
        match self {
            Source::Fill => "Filled",
            Source::Pour => "Poured in",
        }
    }
}

/// A liquid simulation's settings (the copy layout "Liquid").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Fluid {
    /// Droplets.
    pub count: u32,
    pub container: Container,
    /// The container's size (half its width; a bowl's radius).
    pub size: f32,
    /// Spacing of droplets at rest (their size), in world units.
    pub spacing: f32,
    pub source: Source,
    /// Pour: how long each droplet lives, in beats.
    pub life: f32,
    /// Pour: height of the spout above the layer's origin.
    pub spout: f32,
    /// Downward pull.
    pub gravity: f32,
    /// Sticky, syrupy flow (0 = water, 0.3 = honey).
    pub viscosity: f32,
    /// How much droplets hold together (surface tension; 0 splashes
    /// apart into spray).
    pub cohesion: f32,
    /// A swirl around the container's middle (animatable: link it to the
    /// music to stir on the beat).
    pub stir: Param,
    pub seed: u32,
    /// Draw the droplets as one smooth liquid surface (shaded with the
    /// layer's material) instead of as copies of the layer's shape.
    pub surface: bool,
    pub looping: SimLoop,
}

impl Default for Fluid {
    fn default() -> Self {
        Fluid {
            count: 1200,
            container: Container::Bowl,
            size: 1.6,
            spacing: 0.12,
            source: Source::Fill,
            life: 8.0,
            spout: 3.0,
            gravity: 9.8,
            viscosity: 0.05,
            cohesion: 0.05,
            stir: Param::new(0.0),
            seed: 1,
            surface: false,
            looping: SimLoop {
                close: LoopClose::CrossFade,
                // The liquid settles within a loop; cross-fading hides
                // the rest.
                warmup: 1,
                ..SimLoop::default()
            },
        }
    }
}

impl Fluid {
    pub fn droplets(&self) -> u32 {
        self.count.clamp(1, FLUID_MAX)
    }

    pub fn uses_music(&self) -> bool {
        self.stir.uses_music()
    }

    /// How far droplets can get from the layer's origin (roughly).
    pub fn reach(&self) -> f32 {
        let s = self.size.abs().max(0.1);
        let c = match self.container {
            Container::Box => s * 1.8,
            Container::Bowl => s * 1.1,
            Container::Floor => s * 1.2,
        };
        c.max(self.spout.abs() + s)
    }

    /// The simulation, ready to bake. `tilt` gives, at a moment of the
    /// loop, the rotation of the container (the layer's) in the world, so
    /// gravity can be turned into its frame.
    pub fn sim(&self, tilt: TiltFn) -> FluidSim {
        FluidSim::new(self.clone(), tilt)
    }
}

/// The container's rotation in the world at a moment of the loop.
pub type TiltFn = Box<dyn Fn(&EvalCtx) -> Rot + Send + Sync>;

/// The rotation of a layer with transform `t` (as `layer_frame` turns it,
/// without the shake), in [`math`]'s functions so a bake is the same
/// everywhere.
pub fn layer_tilt(t: &crate::Transform) -> TiltFn {
    let t = t.clone();
    Box::new(move |ctx: &EvalCtx| {
        let yxz = |y: f32, x: f32, z: f32| {
            math::rot_mul(
                math::rot_mul(
                    math::rot_axis_angle(Vec3::Y, y),
                    math::rot_axis_angle(Vec3::X, x),
                ),
                math::rot_axis_angle(Vec3::Z, z),
            )
        };
        let r = t.rotation.map(f32::to_radians);
        let base = yxz(r[1], r[0], r[2]);
        let spin = yxz(
            ctx.turns(t.spin[1] as f32),
            ctx.turns(t.spin[0] as f32),
            ctx.turns(t.spin[2] as f32),
        );
        let tilt = math::rot_axis_angle(Vec3::Z, t.tilt.eval(ctx).to_radians());
        math::rot_mul(tilt, math::rot_mul(base, spin))
    })
}

/// `v` turned by the inverse of the unit quaternion `r`.
fn unrotate(r: Rot, v: Vec3) -> Vec3 {
    let q = Vec3::new(-r[0], -r[1], -r[2]);
    let t = q.cross(v) * 2.0;
    v + t * r[3] + q.cross(t)
}

/// The smoothing kernels, for a support radius `h`.
#[derive(Clone, Copy, Debug)]
struct Kernel {
    h: f32,
    h2: f32,
    poly6: f32,
    spiky: f32,
}

impl Kernel {
    fn new(h: f32) -> Kernel {
        let h3 = h * h * h;
        Kernel {
            h,
            h2: h * h,
            poly6: 315.0 / (64.0 * PI * h3 * h3 * h3),
            spiky: -45.0 / (PI * h3 * h3),
        }
    }

    fn w(&self, r2: f32) -> f32 {
        if r2 >= self.h2 {
            return 0.0;
        }
        let d = self.h2 - r2;
        self.poly6 * d * d * d
    }

    /// Gradient of the spiky kernel at offset `d` (length `r`).
    fn grad(&self, d: Vec3, r: f32) -> Vec3 {
        if r >= self.h || r <= 1e-6 {
            return Vec3::ZERO;
        }
        let k = self.h - r;
        d * (self.spiky * k * k / r)
    }
}

/// Droplets sorted into cells of the kernel's size (a counting sort, the
/// same order everywhere).
#[derive(Default)]
struct Cells {
    origin: Vec3,
    dims: [i32; 3],
    size: f32,
    start: Vec<u32>,
    items: Vec<u32>,
}

impl Cells {
    fn cell(&self, p: Vec3) -> [i32; 3] {
        let q = (p - self.origin) / self.size;
        [
            (q.x.floor() as i32).clamp(0, self.dims[0] - 1),
            (q.y.floor() as i32).clamp(0, self.dims[1] - 1),
            (q.z.floor() as i32).clamp(0, self.dims[2] - 1),
        ]
    }

    fn index(&self, c: [i32; 3]) -> usize {
        ((c[2] * self.dims[1] + c[1]) * self.dims[0] + c[0]) as usize
    }

    fn build(&mut self, pos: &[Vec3], alive: &[bool]) {
        let n = (self.dims[0] * self.dims[1] * self.dims[2]) as usize;
        self.start.clear();
        self.start.resize(n + 1, 0);
        for (p, _) in pos.iter().zip(alive).filter(|(_, a)| **a) {
            let c = self.index(self.cell(*p));
            self.start[c + 1] += 1;
        }
        for i in 0..n {
            self.start[i + 1] += self.start[i];
        }
        let mut fill = self.start.clone();
        self.items.clear();
        self.items.resize(self.start[n] as usize, 0);
        for (i, p) in pos.iter().enumerate() {
            if alive[i] {
                let c = self.index(self.cell(*p));
                self.items[fill[c] as usize] = i as u32;
                fill[c] += 1;
            }
        }
    }
}

/// A running liquid (see [`Fluid`]).
pub struct FluidSim {
    set: Fluid,
    tilt: TiltFn,
    bodies: Vec<Body>,
    kernel: Kernel,
    /// Density of droplets at rest on a lattice of the spacing.
    rest: f32,
    /// Regularisation of the density correction.
    eps: f32,
    /// The correction's gradient term for a droplet at rest (its scale).
    grad2: f32,
    radius: f32,
    /// Pour: when each droplet leaves the spout (fraction of the loop).
    born: Vec<f32>,
    /// Pour: its life (fraction of the loop).
    life: f32,
    alive: Vec<bool>,
    pred: Vec<Vec3>,
    lambda: Vec<f32>,
    delta: Vec<Vec3>,
    neigh_start: Vec<u32>,
    neigh: Vec<u32>,
    cells: Cells,
    /// Seconds per loop (set on the first step).
    loop_s: f32,
    /// The last solved state (two steps at a time), and whether the next
    /// step is the second of the two.
    base: Vec<Body>,
    half: bool,
}

impl FluidSim {
    fn new(set: Fluid, tilt: TiltFn) -> FluidSim {
        let n = set.droplets() as usize;
        let d = set.spacing.clamp(0.02, 2.0);
        let kernel = Kernel::new(d * 2.0);
        // Rest density and the correction's scale from a perfect lattice.
        let (mut rest, mut grad2, mut grad_sum) = (0.0f32, 0.0f32, Vec3::ZERO);
        for x in -3..=3 {
            for y in -3..=3 {
                for z in -3..=3 {
                    let o = Vec3::new(x as f32, y as f32, z as f32) * d;
                    rest += kernel.w(o.length_squared());
                    let g = kernel.grad(-o, o.length());
                    grad_sum += g;
                    grad2 += g.length_squared();
                }
            }
        }
        let grad2 = (grad2 + grad_sum.length_squared()) / (rest * rest);
        let s = set.size.abs().max(d * 2.0);
        let (lo, hi) = match set.container {
            Container::Box | Container::Bowl => (Vec3::splat(-s), Vec3::splat(s)),
            Container::Floor => (
                Vec3::new(-s, 0.0, -s),
                Vec3::new(s, (set.spout.abs() + d * 4.0).max(s), s),
            ),
        };
        let hi = hi.max(Vec3::new(hi.x, set.spout.abs() + d * 4.0, hi.z));
        let size = kernel.h;
        let dims = ((hi - lo) / size).ceil().as_ivec3() + 1;
        let cells = Cells {
            origin: lo,
            dims: [dims.x.max(1), dims.y.max(1), dims.z.max(1)],
            size,
            ..Cells::default()
        };
        let mut sim = FluidSim {
            born: (0..n)
                .map(|i| (i as f32 + 0.5 * hash2(set.seed, i as u32)) / n as f32)
                .collect(),
            life: 0.0,
            alive: vec![true; n],
            pred: vec![Vec3::ZERO; n],
            lambda: vec![0.0; n],
            delta: vec![Vec3::ZERO; n],
            neigh_start: Vec::with_capacity(n + 1),
            neigh: Vec::with_capacity(n * 32),
            bodies: vec![Body::default(); n],
            kernel,
            rest,
            eps: grad2 * 0.05,
            grad2,
            radius: d * 0.5,
            cells,
            loop_s: 0.0,
            base: Vec::new(),
            half: false,
            set,
            tilt,
        };
        sim.fill();
        sim.base = sim.bodies.clone();
        sim
    }

    /// Droplets in a block in the container, settling from there (Fill),
    /// or all waiting at the spout (Pour).
    fn fill(&mut self) {
        let d = self.set.spacing.clamp(0.02, 2.0);
        let s = self.set.size.abs().max(d * 2.0);
        // A square column over the middle, as wide as fits.
        let side = match self.set.container {
            Container::Box => s * 1.8,
            Container::Bowl => s * 1.1,
            Container::Floor => s * 1.0,
        };
        let per = ((side / d).floor() as usize).max(1);
        let base = match self.set.container {
            Container::Box => -s + d,
            Container::Bowl => -s * 0.65,
            Container::Floor => d,
        };
        for (i, b) in self.bodies.iter_mut().enumerate() {
            let (x, z, y) = (i % per, (i / per) % per, i / (per * per));
            let jitter = Vec3::new(
                hash2(self.set.seed ^ 0x51, i as u32),
                hash2(self.set.seed ^ 0x52, i as u32),
                hash2(self.set.seed ^ 0x53, i as u32),
            ) - 0.5;
            let p = Vec3::new(
                (x as f32 - (per - 1) as f32 * 0.5) * d,
                base + y as f32 * d,
                (z as f32 - (per - 1) as f32 * 0.5) * d,
            ) + jitter * d * 0.1;
            *b = Body {
                pos: p,
                vel: Vec3::ZERO,
                rot: math::ROT_IDENTITY,
                size: 1.0,
            };
        }
        if self.set.source == Source::Pour {
            let spout = self.spout();
            for b in &mut self.bodies {
                b.pos = spout;
                b.size = 0.0;
            }
            self.alive.iter_mut().for_each(|a| *a = false);
        }
    }

    fn spout(&self) -> Vec3 {
        Vec3::new(0.0, self.set.spout.abs(), 0.0)
    }

    /// Keeps `p` inside the container (a droplet's radius in).
    fn contain(&self, p: Vec3) -> Vec3 {
        let r = self.radius;
        let s = self.set.size.abs().max(r * 4.0);
        let pour = self.set.source == Source::Pour;
        match self.set.container {
            // Poured liquid comes in over the top: the box is open there,
            // and the bowl has straight sides above its rim.
            Container::Box => {
                let top = if pour { f32::MAX } else { s - r };
                Vec3::new(
                    p.x.clamp(-s + r, s - r),
                    p.y.clamp(-s + r, top),
                    p.z.clamp(-s + r, s - r),
                )
            }
            Container::Bowl if pour && p.y > 0.0 => {
                let rr = (p.x * p.x + p.z * p.z).sqrt();
                if rr > s - r {
                    Vec3::new(p.x * (s - r) / rr, p.y, p.z * (s - r) / rr)
                } else {
                    p
                }
            }
            Container::Bowl => {
                let len = p.length();
                if len > s - r {
                    p * ((s - r) / len)
                } else {
                    p
                }
            }
            Container::Floor => {
                let mut q = Vec3::new(p.x, p.y.max(r), p.z);
                let rr = (q.x * q.x + q.z * q.z).sqrt();
                if rr > s - r {
                    let k = (s - r) / rr;
                    q.x *= k;
                    q.z *= k;
                }
                q
            }
        }
    }

    /// Pour: whether droplet `i` is alive at loop phase `phase`, and how
    /// far into its life (0..1).
    fn age(&self, i: usize, phase: f32) -> Option<f32> {
        let a = (phase - self.born[i]).rem_euclid(1.0);
        (a < self.life).then(|| a / self.life)
    }

    fn around(&self, i: usize) -> &[u32] {
        &self.neigh[self.neigh_start[i] as usize..self.neigh_start[i + 1] as usize]
    }

    /// Density of droplet `i` over the rest density (1 = at rest), from
    /// the predicted positions.
    pub fn density(&self, i: usize) -> f32 {
        let p = self.pred[i];
        let mut rho = self.kernel.w(0.0);
        for &j in self.around(i) {
            rho += self.kernel.w((self.pred[j as usize] - p).length_squared());
        }
        rho / self.rest
    }
}

/// One droplet's neighbours.
#[derive(Clone, Copy)]
struct Found {
    list: [u32; MAX_NEIGHBOURS],
    count: usize,
}

impl Default for Found {
    fn default() -> Self {
        Found {
            list: [0; MAX_NEIGHBOURS],
            count: 0,
        }
    }
}

/// `f(i)` for every `i` below `n`, on threads where there are any. Each
/// value depends only on `i`, so the result is the same however the work
/// is split (and on one thread in the browser).
fn par_map<T: Send + Copy + Default>(n: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let mut out = vec![T::default(); n];
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = std::thread::available_parallelism()
            .map_or(1, |t| t.get())
            .min(8);
        if threads > 1 && n >= 256 {
            let chunk = n.div_ceil(threads);
            std::thread::scope(|scope| {
                for (k, part) in out.chunks_mut(chunk).enumerate() {
                    let f = &f;
                    scope.spawn(move || {
                        for (j, v) in part.iter_mut().enumerate() {
                            *v = f(k * chunk + j);
                        }
                    });
                }
            });
            return out;
        }
    }
    for (i, v) in out.iter_mut().enumerate() {
        *v = f(i);
    }
    out
}

impl FluidSim {
    /// Droplets within the kernel of droplet `i` (at most
    /// [`MAX_NEIGHBOURS`], in the cells' fixed order).
    fn find(&self, i: usize, out: &mut [u32; MAX_NEIGHBOURS]) -> usize {
        let p = self.pred[i];
        let c = self.cells.cell(p);
        let mut found = 0;
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let q = [c[0] + dx, c[1] + dy, c[2] + dz];
                    if (0..3).any(|k| q[k] < 0 || q[k] >= self.cells.dims[k]) {
                        continue;
                    }
                    let ci = self.cells.index(q);
                    let (a, b) = (self.cells.start[ci], self.cells.start[ci + 1]);
                    for &j in &self.cells.items[a as usize..b as usize] {
                        if j as usize != i
                            && (self.pred[j as usize] - p).length_squared() < self.kernel.h2
                        {
                            out[found] = j;
                            found += 1;
                            if found == MAX_NEIGHBOURS {
                                return found;
                            }
                        }
                    }
                }
            }
        }
        found
    }

    fn lambda_of(&self, i: usize) -> f32 {
        if !self.alive[i] {
            return 0.0;
        }
        let k = self.kernel;
        let p = self.pred[i];
        let mut rho = k.w(0.0);
        let mut sum2 = 0.0;
        let mut gi = Vec3::ZERO;
        for &j in self.around(i) {
            let d = p - self.pred[j as usize];
            let r2 = d.length_squared();
            rho += k.w(r2);
            let g = k.grad(d, r2.sqrt()) / self.rest;
            sum2 += g.length_squared();
            gi += g;
        }
        // Only squeezing is corrected (pulling apart made the surface
        // jitter); cohesion is a gentle force of its own.
        let c = (rho / self.rest - 1.0).max(0.0);
        -c / (sum2 + gi.length_squared() + self.eps)
    }

    fn delta_of(&self, i: usize, wq: f32) -> Vec3 {
        if !self.alive[i] {
            return Vec3::ZERO;
        }
        let k = self.kernel;
        let p = self.pred[i];
        let mut dp = Vec3::ZERO;
        for &j in self.around(i) {
            let j = j as usize;
            let d = p - self.pred[j];
            let r2 = d.length_squared();
            let ratio = k.w(r2) / wq;
            // Artificial pressure, as if a 1% density deficit pushed apart
            // at close range (keeps droplets from clumping).
            let s = -0.01 * ratio * ratio * ratio * ratio / self.grad2;
            dp += k.grad(d, r2.sqrt()) * (self.lambda[i] + self.lambda[j] + s);
        }
        dp / self.rest
    }

    /// Droplet `i`'s velocity after viscosity and cohesion (`vel` holds
    /// every droplet's velocity from the positions).
    fn smoothed(&self, i: usize, vel: &[Vec3], dt: f32) -> Vec3 {
        let k = self.kernel;
        let (p, v) = (self.pred[i], vel[i]);
        let c = self.set.viscosity.clamp(0.0, 1.0);
        // Cohesion: droplets a little further apart than at rest pull
        // together (strongest halfway between the spacing and the
        // kernel's reach), which rounds drops and holds the surface.
        let rest_d = k.h * 0.5;
        let pull = self.set.cohesion.max(0.0) * self.set.gravity.abs().max(1.0) * 4.0;
        let mut acc = Vec3::ZERO;
        let mut wsum = 0.0;
        let mut attract = Vec3::ZERO;
        for &j in self.around(i) {
            let j = j as usize;
            let d = self.pred[j] - p;
            let r2 = d.length_squared();
            let w = k.w(r2);
            acc += (vel[j] - v) * w;
            wsum += w;
            let r = r2.sqrt();
            if r > rest_d && r < k.h {
                let x = (r - rest_d) / (k.h - rest_d);
                attract += d / r * (x * (1.0 - x) * 4.0);
            }
        }
        let mut v = if wsum > 0.0 {
            v + acc / wsum.max(k.w(0.0)) * c
        } else {
            v
        };
        v += attract * (pull * dt / 16.0);
        v
    }

    /// One full step of `dt` seconds from the droplets' current state.
    fn solve(&mut self, dt: f32, ctx: &EvalCtx) {
        let n = self.bodies.len();
        // Gravity in the container's frame, and the stir.
        let g = unrotate((self.tilt)(ctx), Vec3::new(0.0, -self.set.gravity, 0.0));
        let stir = self.set.stir.eval(ctx);
        for i in 0..n {
            let b = &mut self.bodies[i];
            if !self.alive[i] {
                self.pred[i] = b.pos;
                continue;
            }
            let mut f = g;
            if stir != 0.0 {
                let r = Vec3::new(b.pos.x, 0.0, b.pos.z);
                let len = r.length();
                if len > 1e-4 {
                    f += Vec3::new(-r.z, 0.0, r.x) / len * stir;
                }
            }
            b.vel += f * dt;
            self.pred[i] = b.pos + b.vel * dt;
        }
        for i in 0..n {
            if self.alive[i] {
                self.pred[i] = self.contain(self.pred[i]);
            }
        }
        // Neighbours, found on threads and joined in order.
        self.cells.build(&self.pred, &self.alive);
        let lists = par_map(n, |i| {
            let mut out = Found::default();
            if self.alive[i] {
                out.count = self.find(i, &mut out.list);
            }
            out
        });
        self.neigh_start.clear();
        self.neigh.clear();
        for f in &lists {
            self.neigh_start.push(self.neigh.len() as u32);
            self.neigh.extend_from_slice(&f.list[..f.count]);
        }
        self.neigh_start.push(self.neigh.len() as u32);
        let k = self.kernel;
        let wq = k.w((0.2 * k.h) * (0.2 * k.h));
        for _ in 0..ITERATIONS {
            self.lambda = par_map(n, |i| self.lambda_of(i));
            self.delta = par_map(n, |i| self.delta_of(i, wq));
            for i in 0..n {
                if self.alive[i] {
                    self.pred[i] = self.contain(self.pred[i] + self.delta[i]);
                }
            }
        }
        // New velocities from the movement, then viscosity and cohesion.
        let vel: Vec<Vec3> = (0..n)
            .map(|i| {
                if !self.alive[i] {
                    return Vec3::ZERO;
                }
                let v = (self.pred[i] - self.bodies[i].pos) / dt;
                let s = v.length();
                if s > MAX_SPEED {
                    v * (MAX_SPEED / s)
                } else {
                    v
                }
            })
            .collect();
        let smooth = par_map(n, |i| {
            if self.alive[i] {
                self.smoothed(i, &vel, dt)
            } else {
                Vec3::ZERO
            }
        });
        for (i, (b, v)) in self.bodies.iter_mut().zip(smooth).enumerate() {
            if self.alive[i] {
                b.vel = v;
                b.pos = self.pred[i];
            }
        }
    }
}

impl Sim for FluidSim {
    fn bodies(&self) -> &[Body] {
        &self.bodies
    }

    fn bodies_mut(&mut self) -> &mut [Body] {
        &mut self.bodies
    }

    /// The liquid is solved at half the bake's rate (two steps at a
    /// time); the step in between moves the droplets on at their speed.
    fn step(&mut self, dt: f32, ctx: &EvalCtx) {
        if self.loop_s == 0.0 {
            self.loop_s = (ctx.beat_seconds * ctx.loop_beats.max(1) as f32).max(0.05);
            self.life = (self.set.life.max(0.1) / ctx.loop_beats.max(1) as f32).min(0.95);
        }
        let n = self.bodies.len();
        let phase = ctx.beat_phase.rem_euclid(1.0);
        // Pour: droplets come and go on their schedule.
        if self.set.source == Source::Pour {
            let spout = self.spout();
            for i in 0..n {
                match self.age(i, phase) {
                    Some(a) => {
                        if !self.alive[i] {
                            let j = Vec3::new(
                                hash2(self.set.seed ^ 0x61, i as u32) - 0.5,
                                0.0,
                                hash2(self.set.seed ^ 0x62, i as u32) - 0.5,
                            );
                            let b = &mut self.bodies[i];
                            b.pos = spout + j * self.set.spacing;
                            b.vel = Vec3::new(0.0, -2.0, 0.0);
                            self.alive[i] = true;
                            self.base[i] = *b;
                        }
                        let fade = ((1.0 - a) / VANISH).min(1.0);
                        self.bodies[i].size = fade.max(0.0);
                        self.base[i].size = self.bodies[i].size;
                    }
                    None => {
                        self.alive[i] = false;
                        let b = &mut self.bodies[i];
                        b.size = 0.0;
                        b.vel = Vec3::ZERO;
                        self.base[i] = *b;
                    }
                }
            }
        }
        if self.half {
            // The second half: arrive at the solved state.
            for i in 0..n {
                if self.alive[i] {
                    self.bodies[i] = self.base[i];
                }
            }
        } else {
            // Solve two steps ahead from here, and show the first one:
            // back from there at its speed.
            self.solve(dt * 2.0, ctx);
            self.base.copy_from_slice(&self.bodies);
            for i in 0..n {
                if self.alive[i] {
                    let b = self.base[i];
                    self.bodies[i].pos = self.contain(b.pos - b.vel * dt);
                }
            }
        }
        self.half = !self.half;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{BakeJob, Frame, SimClock};
    use crate::Timing;

    fn level() -> TiltFn {
        Box::new(|_| math::ROT_IDENTITY)
    }

    fn rocking(deg: f32) -> TiltFn {
        Box::new(move |ctx: &EvalCtx| {
            let a = (deg * math::sin(ctx.beat_phase * std::f32::consts::TAU)).to_radians();
            math::rot_axis_angle(Vec3::Z, a)
        })
    }

    fn small() -> Fluid {
        Fluid {
            count: 400,
            size: 1.0,
            spacing: 0.14,
            ..Fluid::default()
        }
    }

    fn timing() -> Timing {
        Timing {
            bpm: 120.0,
            loop_beats: 8,
        }
    }

    fn inside(set: &Fluid, p: Vec3) -> bool {
        let s = set.size + 1e-3;
        match set.container {
            Container::Box => p.abs().max_element() <= s,
            Container::Bowl => p.length() <= s,
            Container::Floor => p.y >= -1e-3 && (p.x * p.x + p.z * p.z).sqrt() <= s,
        }
    }

    #[test]
    fn settles_near_rest_density() {
        for container in Container::ALL {
            let set = Fluid {
                container,
                ..small()
            };
            let mut sim = set.sim(level());
            let ctx = EvalCtx::at(0.0);
            for _ in 0..360 {
                sim.step(1.0 / 120.0, &ctx);
            }
            // Droplets with a full set of neighbours (inside the liquid,
            // not at its surface or the walls).
            let dens: Vec<f32> = (0..sim.bodies.len())
                .filter(|&i| sim.around(i).len() >= 24)
                .map(|i| sim.density(i))
                .collect();
            assert!(dens.len() > 30, "{container:?}: too few inner droplets");
            let mean = dens.iter().sum::<f32>() / dens.len() as f32;
            eprintln!(
                "{container:?}: {} inner droplets, mean density {mean:.3}",
                dens.len()
            );
            assert!((mean - 1.0).abs() < 0.1, "{container:?}: density {mean}");
            let speeds: Vec<f32> = sim.bodies.iter().map(|b| b.vel.length()).collect();
            let fastest = speeds.iter().copied().fold(0.0, f32::max);
            let mean_speed = speeds.iter().sum::<f32>() / speeds.len() as f32;
            eprintln!("{container:?}: speed mean {mean_speed:.3}, fastest {fastest:.3}");
            assert!(
                mean_speed < 0.1,
                "{container:?}: still moving at {mean_speed} on average"
            );
            for b in &sim.bodies {
                assert!(inside(&set, b.pos), "{container:?}: left at {}", b.pos);
            }
        }
    }

    #[test]
    fn loops_and_stays_in_a_rocking_bowl() {
        let set = small();
        let tilt = || rocking(25.0);
        let job = BakeJob::new(
            Box::new(set.sim(tilt())),
            SimClock::new(timing()),
            set.looping.clone(),
        );
        let bake = job.finish();
        let mut a = Frame::default();
        let mut b = Frame::default();
        bake.sample(0.0, &mut a);
        bake.sample(1.0, &mut b);
        for (la, lb) in a.layers.iter().zip(&b.layers) {
            assert_eq!(la.weight, lb.weight);
            for (x, y) in la.bodies.iter().zip(&lb.bodies) {
                assert_eq!(x.pos, y.pos);
            }
        }
        let mut f = Frame::default();
        for k in 0..64 {
            bake.sample(k as f32 / 64.0, &mut f);
            for l in &f.layers {
                assert_eq!(l.bodies.len(), set.droplets() as usize);
                for body in &l.bodies {
                    assert!(inside(&set, body.pos), "left the bowl at {}", body.pos);
                }
            }
        }
        // The liquid really sloshes: its middle moves sideways.
        let mid = |phase: f32, f: &mut Frame| {
            bake.sample(phase, f);
            let l = &f.layers[0];
            l.bodies.iter().map(|b| b.pos).sum::<Vec3>() / l.bodies.len() as f32
        };
        let (left, right) = (mid(0.25, &mut f), mid(0.75, &mut f));
        eprintln!("slosh: middle at x {:.3} and {:.3}", left.x, right.x);
        assert!((left.x - right.x).abs() > 0.05);
    }

    #[test]
    fn pouring_keeps_the_count_and_fills() {
        let set = Fluid {
            source: Source::Pour,
            container: Container::Box,
            life: 6.0,
            ..small()
        };
        let job = BakeJob::new(
            Box::new(set.sim(level())),
            SimClock::new(timing()),
            set.looping.clone(),
        );
        let bake = job.finish();
        let mut f = Frame::default();
        for k in 0..32 {
            bake.sample(k as f32 / 32.0, &mut f);
            for l in &f.layers {
                assert_eq!(l.bodies.len(), set.droplets() as usize);
                let shown = l.bodies.iter().filter(|b| b.size > 0.0).count();
                assert!(shown > 0 && shown < l.bodies.len());
                for b in l.bodies.iter().filter(|b| b.size > 0.0) {
                    assert!(inside(&set, b.pos) || b.pos.y > set.size - 0.01);
                }
            }
        }
    }

    #[test]
    fn same_fluid_everywhere() {
        // Enough droplets for threads natively (one thread in the browser).
        let set = Fluid {
            count: 300,
            ..small()
        };
        let mut looping = set.looping.clone();
        looping.warmup = 0;
        let job = BakeJob::new(
            Box::new(set.sim(rocking(20.0))),
            SimClock::new(timing()),
            looping,
        );
        let bake = job.finish();
        let hash = bake
            .raw()
            .iter()
            .fold(crate::sim::KeyHasher::new(), |h, v| h.f32(*v))
            .finish();
        eprintln!("fluid bake hash {hash:#018x}");
        assert_eq!(hash, 0x7317_e757_334b_3792, "fluid bake hash {hash:#018x}");
    }

    #[test]
    #[ignore = "timing: run with --release -- --ignored --nocapture"]
    fn bake_time() {
        for count in [1200u32, 3000, 8000] {
            let set = Fluid {
                count,
                ..Fluid::default()
            };
            let t = Timing {
                bpm: 120.0,
                loop_beats: 16,
            };
            let start = std::time::Instant::now();
            let job = BakeJob::new(
                Box::new(set.sim(rocking(20.0))),
                SimClock::new(t),
                set.looping.clone(),
            );
            let bake = job.finish();
            eprintln!(
                "{count} droplets, 8 s loop: {:.2} s ({} warm-up loops, {:.1} MB)",
                start.elapsed().as_secs_f32(),
                bake.warmup_loops(),
                bake.bytes() as f32 / 1e6
            );
        }
    }
}
