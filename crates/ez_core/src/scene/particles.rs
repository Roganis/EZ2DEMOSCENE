//! Particle layers.

use super::*;

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum Emitter {
        /// Explodes outward from the centre.
        #[default]
        Burst => "Burst",
        /// Drifts inside a sphere.
        Sphere => "Sphere drift",
        /// Circles in a flat ring.
        Ring => "Ring orbit",
        /// Shoots up and falls down.
        Fountain => "Fountain",
        /// Streams towards the camera (hyperspace).
        Warp => "Warp stars",
        /// Spirals upward in a vortex.
        Vortex => "Vortex",
        /// Falls slowly like snow / glitter.
        Snow => "Snow / glitter",
        /// A twisting funnel (tornado / water spout) with debris at its foot.
        Tornado => "Tornado",
    }
}

impl Emitter {
    pub fn index(self) -> u32 {
        Emitter::ALL.iter().position(|e| *e == self).unwrap_or(0) as u32
    }
}

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum Sprite {
        #[default]
        Glow => "Soft glow",
        Square => "Pixel square",
        Star => "Star",
        Ring => "Ring",
        /// Solid, unsmoothed single-colour squares (Quake).
        SolidSquare => "Solid square (Quake)",
    }
}

impl Sprite {
    pub fn index(self) -> u32 {
        Sprite::ALL.iter().position(|e| *e == self).unwrap_or(0) as u32
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParticleLayer {
    pub emitter: Emitter,
    pub count: u32,
    /// How many times each particle is reborn per loop (integer: loops!).
    pub lifetimes: u32,
    pub size: Param,
    pub speed: Param,
    /// Emitter size / spread.
    pub radius: Param,
    pub color_a: Rgb,
    pub color_b: Rgb,
    pub intensity: Param,
    /// Ghost copies behind each particle (0 = none).
    pub trail: u32,
    pub trail_spacing: Param,
    pub sprite: Sprite,
    pub seed: u32,
    /// Smoke: particles cover what is behind them (and can be dark)
    /// instead of adding light. Brightness becomes opacity.
    #[serde(skip_serializing_if = "is_default")]
    pub smoke: bool,
    /// The brightness flickers by a Quake light style.
    #[serde(skip_serializing_if = "is_default")]
    pub glow_style: crate::retro::LightStyle,
}

impl Default for ParticleLayer {
    fn default() -> Self {
        ParticleLayer {
            emitter: Emitter::Burst,
            count: 800,
            lifetimes: 2,
            size: Param::new(0.08),
            speed: Param::new(1.0),
            radius: Param::new(4.0),
            color_a: hex(0xffd080),
            color_b: hex(0xff3010),
            intensity: Param::new(2.0),
            trail: 0,
            trail_spacing: Param::new(0.01),
            sprite: Sprite::Glow,
            seed: 1,
            smoke: false,
            glow_style: Default::default(),
        }
    }
}
