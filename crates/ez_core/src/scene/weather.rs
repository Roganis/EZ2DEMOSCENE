//! Weather and waterfalls.

use super::*;

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum Precipitation {
        /// Streaks of rain with splashes on the ground.
        #[default]
        Rain => "Rain",
        /// Slowly swaying snowflakes.
        Snow => "Snow",
        /// Glowing embers rising from the ground.
        Embers => "Rising embers",
        /// A sandstorm: dust blown sideways.
        Dust => "Sandstorm",
        /// Fireflies wandering and blinking.
        Fireflies => "Fireflies",
        /// No particles (lightning only).
        None => "None (lightning only)",
    }
}

impl Precipitation {
    pub fn index(self) -> u32 {
        Precipitation::ALL
            .iter()
            .position(|t| *t == self)
            .unwrap_or(0) as u32
    }
    /// Colour and falls per loop that suit the kind.
    pub fn defaults(self) -> (Rgb, u32, f32) {
        match self {
            Precipitation::Rain => (hex(0x8fa8c8), 12, 0.05),
            Precipitation::Snow => (hex(0xf0f4ff), 2, 0.12),
            Precipitation::Embers => (hex(0xff7020), 3, 0.08),
            Precipitation::Dust => (hex(0xc09060), 4, 0.6),
            Precipitation::Fireflies => (hex(0xc0ff60), 1, 0.1),
            Precipitation::None => (hex(0xffffff), 1, 0.1),
        }
    }
}

/// Rain, snow, embers or dust filling a box that follows the camera, with
/// optional lightning. Every drop falls a whole number of times per loop.
/// The layer's height (position Y) is the ground where drops splash.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Weather {
    pub kind: Precipitation,
    pub count: u32,
    /// Half width of the box around the camera.
    pub area: f32,
    /// Height of the top of the box above the ground.
    pub height: f32,
    /// Times each drop falls (or rises) per loop.
    pub falls: u32,
    /// Sideways push in degrees of tilt (animatable: gusts).
    pub wind: Param,
    /// Wind direction in degrees around the vertical axis.
    pub wind_dir: f32,
    /// Size of drops / flakes (animatable).
    pub size: Param,
    /// Rain streak length multiplier.
    pub streak: f32,
    pub color: Rgb,
    pub intensity: Param,
    /// Splash rings where rain hits the ground (0 = none).
    pub splashes: Param,
    pub seed: u32,
    pub lightning: Lightning,
    /// Rain wets the ground (darker, glossy, puddles); snow covers
    /// upward-facing surfaces. Animate it to build up and melt.
    pub ground: Param,
}

impl Default for Weather {
    fn default() -> Self {
        Weather {
            kind: Precipitation::Rain,
            count: 6000,
            area: 18.0,
            height: 14.0,
            falls: 12,
            wind: Param::new(10.0),
            wind_dir: 30.0,
            size: Param::new(0.05),
            streak: 1.0,
            color: hex(0x8fa8c8),
            intensity: Param::new(0.6),
            splashes: Param::new(0.6),
            seed: 1,
            lightning: Lightning::default(),
            ground: Param::new(0.6),
        }
    }
}

/// Lightning strikes: a jagged bolt and a flash that lights up the scene.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lightning {
    pub enabled: bool,
    /// Moments per loop when a strike may happen.
    pub per_loop: u32,
    /// Chance of a strike at each moment.
    pub chance: f32,
    /// Brightness of the flash lighting up the scene.
    pub flash: Param,
    pub color: Rgb,
    /// Distance of the bolts from the camera.
    pub distance: f32,
    pub seed: u32,
}

impl Default for Lightning {
    fn default() -> Self {
        Lightning {
            enabled: false,
            per_loop: 8,
            chance: 0.35,
            flash: Param::new(1.5),
            color: hex(0xc8d8ff),
            distance: 30.0,
            seed: 7,
        }
    }
}

impl Lightning {
    /// The strike visible at `phase`: (brightness 0..1, strike number,
    /// time since it started as a fraction of a slot). Loop-safe: slots
    /// wrap with the loop.
    pub fn strike(&self, phase: f32) -> Option<(f32, u32, f32)> {
        if !self.enabled {
            return None;
        }
        let n = self.per_loop.max(1) as f32;
        let x = phase.rem_euclid(1.0) * n;
        let slot = (x.floor() as u32) % self.per_loop.max(1);
        let r = crate::rng::hash_u32(
            slot.wrapping_mul(0x9e37_79b9) ^ self.seed.wrapping_mul(0x85eb_ca6b),
        );
        let roll = (r >> 8) as f32 / (1u32 << 24) as f32;
        if roll >= self.chance.clamp(0.0, 1.0) {
            return None;
        }
        // Each strike starts somewhere in the first half of its slot and
        // flickers two or three times before fading.
        let start = ((r & 0xff) as f32 / 255.0) * 0.5;
        let t = x.fract() - start;
        if t < 0.0 {
            return None;
        }
        let flicker = if t < 0.03 {
            1.0
        } else if t < 0.05 {
            0.25
        } else if t < 0.09 {
            0.9
        } else {
            (-(t - 0.09) * 20.0).exp()
        };
        let b = flicker.clamp(0.0, 1.0);
        (b > 0.003).then_some((b, slot, t))
    }

    /// Brightness of the lightning flash at `phase` (0 = none).
    pub fn flash_at(&self, phase: f32) -> f32 {
        self.strike(phase).map(|s| s.0).unwrap_or(0.0)
    }
}

#[cfg(test)]
mod weather_tests {
    use super::*;

    #[test]
    fn lightning_loops_and_strikes() {
        let l = Lightning {
            enabled: true,
            chance: 1.0,
            ..Default::default()
        };
        assert_eq!(l.flash_at(0.0), l.flash_at(1.0));
        let lit = (0..1000)
            .filter(|i| l.flash_at(*i as f32 / 1000.0) > 0.5)
            .count();
        assert!(lit > 0, "no strike");
        assert!(lit < 500, "always lit");
        let off = Lightning::default();
        assert_eq!(off.flash_at(0.3), 0.0);
    }

    #[test]
    fn new_features_round_trip() {
        let mut p = Project::default();
        let mut t = Terrain::default();
        t.liquid.kind = LiquidKind::Lava;
        t.shape = TerrainShape::Canyons;
        t.biome = Biome::Volcanic;
        p.layers.push(Layer::new("t", LayerKind::Terrain(t)));
        p.layers
            .push(Layer::new("w", LayerKind::Weather(Weather::default())));
        p.post.rays.enabled = true;
        let back = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(p, back);
        // Defaults are not written.
        let plain = Project::default().to_json();
        assert!(!plain.contains("rays"));
    }
}

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum FallKind {
        #[default]
        Water => "Water",
        Lava => "Lava",
        Toxic => "Toxic goo",
    }
}

impl FallKind {
    pub fn index(self) -> u32 {
        FallKind::ALL.iter().position(|t| *t == self).unwrap_or(0) as u32
    }
    pub fn default_color(self) -> Rgb {
        match self {
            FallKind::Water => hex(0xb8dcf0),
            FallKind::Lava => hex(0xff5a10),
            FallKind::Toxic => hex(0x40ff30),
        }
    }
}

/// A curtain of water (or lava) pouring over an edge, from the layer's
/// position downwards and away along its +Z axis, with foam or smoke at
/// the foot. Streaks scroll a whole number of times per loop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Falls {
    pub kind: FallKind,
    pub width: f32,
    pub height: f32,
    /// How far the curtain arcs out from the edge.
    pub push: f32,
    pub color: Rgb,
    /// Glow of lava / goo, brightness of water.
    pub glow: Param,
    /// Times the streaks run down per loop.
    pub flow: u32,
    /// Foam, spray and mist at the foot (0 = none).
    pub foam: Param,
    pub seed: u32,
    /// Quake's wobbling warp of the streaks (0 = off; animatable), a
    /// whole number of wobbles per loop (`flow`).
    #[serde(skip_serializing_if = "is_off")]
    pub turbulence: Param,
}

impl Default for Falls {
    fn default() -> Self {
        Falls {
            kind: FallKind::Water,
            width: 4.0,
            height: 8.0,
            push: 1.0,
            color: FallKind::Water.default_color(),
            glow: Param::new(1.0),
            flow: 4,
            foam: Param::new(1.0),
            seed: 1,
            turbulence: Param::new(0.0),
        }
    }
}
