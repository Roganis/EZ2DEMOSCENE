//! Core data model of EZ2DEMOSCENE.
//!
//! Everything here is GPU-agnostic: the project/scene description, the loop
//! clock, animatable parameters, CPU-side instancing math, presets, the
//! randomizer and the node graph. Every animation is expressed as a function
//! of the loop *phase* (0..1) with integer cycle counts, which makes every
//! scene seamlessly loopable by construction.

pub mod audio;
pub mod clock;
pub mod color;
pub mod eval;
pub mod graph;
pub mod palette;
pub mod param;
pub mod presets;
pub mod randomize;
pub mod rng;
pub mod scene;

pub use audio::AudioEnvelope;
pub use clock::{EvalCtx, Timing};
pub use param::{Param, Wave};
pub use scene::*;

/// File extension used for project files.
pub const PROJECT_EXTENSION: &str = "ez2.json";
