//! Simulations that loop: flocks, cloth, rigid bodies, fluids.
//!
//! Everything else in a project is a formula of the loop phase. A
//! simulation depends on its own past instead, so it is run ahead of time
//! into a [`Bake`] (a fixed number of keys per loop), and the bake is made
//! to loop: drawing reads it at the phase, and the picture at a phase still
//! depends only on the settings, the loop length and the loop-window music.
//!
//! - A simulation implements [`Sim`]: a constant number of [`Body`]s moved
//!   by [`Sim::step`] at a fixed rate ([`STEPS_PER_SECOND`]). Its settings
//!   are evaluated at the simulated moment (the [`EvalCtx`](crate::EvalCtx)
//!   passed in), so beats, music links and signal nodes drive it.
//! - A [`BakeJob`] runs it (a slice at a time if needed) into a [`Bake`];
//!   [`SimLoop`] says how the loop is closed ([`LoopClose`]).
//! - [`BakeCache`] runs bakes on a thread (a slice per frame in the
//!   browser) and keeps the previous bake of a layer until the new one is
//!   ready.
//! - Simulations use [`math`] instead of the platform's `sin` / `cos` /
//!   `exp`, so a bake is the same on desktop, web and Android.

mod bake;
mod cache;
mod cloth;
mod flock;
mod fluid;
pub mod math;
mod physics;

pub use bake::*;
pub use cache::*;
pub use cloth::*;
pub use flock::*;
pub use fluid::*;
pub use physics::*;

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// One simulated thing: a boid, a cloth vertex, a rigid body, a droplet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub pos: Vec3,
    /// Velocity (units per second): the bake interpolates positions with it
    /// between keys, so it must be the true motion.
    pub vel: Vec3,
    /// Orientation (kept only when [`Sim::rotations`] is true).
    pub rot: math::Rot,
    /// Scale; 0 hides the body. A body that jumps somewhere else (respawns)
    /// should be hidden for at least one key before and after, so the
    /// interpolation's swoop between the two places can't be seen.
    pub size: f32,
}

impl Default for Body {
    fn default() -> Self {
        Body {
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            rot: math::ROT_IDENTITY,
            size: 1.0,
        }
    }
}

impl Body {
    pub fn at(pos: Vec3) -> Body {
        Body {
            pos,
            ..Body::default()
        }
    }
}

/// A simulation. Keep all of its changing state in [`Sim::bodies`]: when
/// the loop is closed by a guided tail, the bake pulls the bodies towards
/// the start between steps and the simulation must carry on from there.
pub trait Sim: Send {
    fn bodies(&self) -> &[Body];
    fn bodies_mut(&mut self) -> &mut [Body];
    /// Move everything on by `dt` seconds. `ctx` is the moment in the loop
    /// at the start of the step: evaluate the settings with it.
    fn step(&mut self, dt: f32, ctx: &crate::EvalCtx);
    /// Whether orientations matter (stored, interpolated and blended).
    fn rotations(&self) -> bool {
        false
    }

    /// A guided tail: after each step near the loop's end, move the bodies
    /// the fraction `gain` of the way to `targets` (where they were one
    /// loop earlier; `gain` reaches 1 at the end). `drift` receives, per
    /// body, any movement the velocities don't include (per second). The
    /// default moves them directly ([`pull`]); a simulation can instead
    /// steer towards the targets, so its own rules (keeping apart) still
    /// act; the final blend then covers what is left.
    fn guide(&mut self, targets: &[Body], gain: f32, dt: f32, drift: &mut [Vec3]) {
        pull(self.bodies_mut(), targets, gain, dt, drift);
    }
}

/// Moves `bodies` the fraction `gain` of the way to `targets`, recording
/// the movement per second in `drift` (the default [`Sim::guide`]).
pub fn pull(bodies: &mut [Body], targets: &[Body], gain: f32, dt: f32, drift: &mut [Vec3]) {
    for ((body, to), drift) in bodies.iter_mut().zip(targets).zip(drift) {
        let moved = (to.pos - body.pos) * gain;
        body.pos += moved;
        *drift = moved / dt;
        body.vel += (to.vel - body.vel) * gain;
        body.rot = math::rot_nlerp(body.rot, to.rot, gain);
        body.size += (to.size - body.size) * gain;
    }
}

/// How a simulation's end joins its start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoopClose {
    /// The volumetric clouds' trick: the loop is drawn twice, half a loop
    /// apart, each fading out before it jumps back. Exact, for anything
    /// drawn with opacity or added light (glowing particles, smoke, fluid).
    CrossFade,
    /// Over the end of the loop every body blends into where it was one
    /// loop earlier, so it arrives exactly at the start. For solid things
    /// that must not show a second, ghost copy (flocks, cloth, bodies).
    #[default]
    BlendTail,
    /// Forward for half the loop, backward for the other half. Exact:
    /// collapse and rebuild.
    PingPong,
}

impl LoopClose {
    pub const ALL: [LoopClose; 3] = [
        LoopClose::CrossFade,
        LoopClose::BlendTail,
        LoopClose::PingPong,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LoopClose::CrossFade => "Cross-fade halves",
            LoopClose::BlendTail => "Blend the tail",
            LoopClose::PingPong => "Ping-pong",
        }
    }
}

/// How a simulation is baked into a loop (saved with its layer).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SimLoop {
    pub close: LoopClose,
    /// Length of the blended tail, as a fraction of the loop (0.02–0.5).
    pub blend: f32,
    /// While the tail is simulated, pull the bodies towards the start, so
    /// most of the joining is physical and the final blend moves little.
    pub guide: bool,
    /// Most loops simulated before recording starts (fewer once settled).
    pub warmup: u32,
    /// Settled: the bodies moved less than this (RMS, in world units)
    /// between the same moment of two loops.
    pub settle: f32,
}

impl Default for SimLoop {
    fn default() -> Self {
        SimLoop {
            close: LoopClose::BlendTail,
            blend: 0.25,
            guide: true,
            warmup: 2,
            settle: 0.001,
        }
    }
}

/// A stable 64-bit hash (FNV-1a) for bake cache keys: the same on every
/// platform and run, unlike `std`'s hasher.
#[derive(Clone, Copy, Debug)]
pub struct KeyHasher(u64);

impl Default for KeyHasher {
    fn default() -> Self {
        KeyHasher(0xcbf2_9ce4_8422_2325)
    }
}

impl KeyHasher {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bytes(mut self, bytes: &[u8]) -> Self {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
        self
    }

    pub fn u64(self, v: u64) -> Self {
        self.bytes(&v.to_le_bytes())
    }

    pub fn f32(self, v: f32) -> Self {
        self.bytes(&v.to_bits().to_le_bytes())
    }

    /// Anything serialisable, through its JSON (don't use it on hash maps,
    /// whose order changes between runs).
    pub fn json(self, v: &impl Serialize) -> Self {
        self.bytes(serde_json::to_string(v).unwrap_or_default().as_bytes())
    }

    pub fn finish(self) -> u64 {
        self.0
    }
}
