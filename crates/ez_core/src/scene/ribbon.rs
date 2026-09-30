//! Neon ribbons.

use super::*;

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum RibbonCurve {
        /// 3D Lissajous figure (uses the three frequencies).
        #[default]
        Lissajous => "Lissajous",
        /// Torus knot (uses the first two frequencies).
        Knot => "Knot",
        /// Figure eight.
        Infinity => "Figure eight",
        /// Circle that waves up and down (first frequency = waves).
        Wave => "Wavy ring",
        /// Flower / rose curve (first frequency = petals).
        Rose => "Rose",
    }
}

impl RibbonCurve {
    /// Point at `t` (0..1 around the closed curve), fitting a unit sphere.
    pub fn point(self, freq: [u32; 3], t: f32) -> [f32; 3] {
        self.point_by(freq, t, |x| x.sin(), |x| x.cos())
    }

    /// [`RibbonCurve::point`] with the same result on every platform (for
    /// simulations, see [`crate::sim::math`]).
    pub fn point_exact(self, freq: [u32; 3], t: f32) -> [f32; 3] {
        use crate::sim::math;
        self.point_by(freq, t, math::sin, math::cos)
    }

    fn point_by(
        self,
        freq: [u32; 3],
        t: f32,
        sin: impl Fn(f32) -> f32,
        cos: impl Fn(f32) -> f32,
    ) -> [f32; 3] {
        use std::f32::consts::{PI, TAU};
        let [a, b, c] = freq.map(|f| f.clamp(1, 16) as f32);
        let x = t * TAU;
        match self {
            RibbonCurve::Lissajous => [
                sin(a * x + 0.5 * PI),
                sin(b * x) * 0.6,
                sin(c * x + 0.25 * PI),
            ],
            RibbonCurve::Knot => {
                let rr = 0.62 + 0.28 * cos(b * x);
                [rr * cos(a * x), 0.28 * sin(b * x), rr * sin(a * x)]
            }
            RibbonCurve::Infinity => [sin(x), 0.15 * sin(a * x), sin(x) * cos(x)],
            RibbonCurve::Wave => [cos(x), 0.3 * sin(a * x), sin(x)],
            RibbonCurve::Rose => {
                let rr = cos(a * x);
                [rr * cos(x), 0.1 * sin(b * x), rr * sin(x)]
            }
        }
    }
}

/// A glowing tube along a closed curve with light pulses running along it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ribbon {
    pub curve: RibbonCurve,
    pub freq: [u32; 3],
    pub thickness: f32,
    pub color: Rgb,
    /// Glow of the whole tube (animatable).
    pub glow: Param,
    /// Light pulses travelling along the tube.
    pub pulses: u32,
    /// Laps each pulse makes per loop (negative = backwards).
    pub pulse_speed: i32,
    /// Pulse length (fraction of the tube).
    pub pulse_length: Param,
    /// Extra brightness of the pulses.
    pub pulse_glow: Param,
}

impl Default for Ribbon {
    fn default() -> Self {
        Ribbon {
            curve: RibbonCurve::Lissajous,
            freq: [3, 2, 5],
            thickness: 0.04,
            color: hex(0x00e5ff),
            glow: Param::new(1.0),
            pulses: 3,
            pulse_speed: 1,
            pulse_length: Param::new(0.08),
            pulse_glow: Param::new(6.0),
        }
    }
}
