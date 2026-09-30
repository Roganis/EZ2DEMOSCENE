//! Laser beams and electric arcs.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LaserPattern {
    /// Flat fan of beams.
    #[default]
    Fan,
    /// Beams on a rotating cone.
    Cone,
    /// Random directions that wobble.
    Scatter,
}

impl LaserPattern {
    pub const ALL: [LaserPattern; 3] =
        [LaserPattern::Fan, LaserPattern::Cone, LaserPattern::Scatter];
    pub fn label(self) -> &'static str {
        match self {
            LaserPattern::Fan => "Fan",
            LaserPattern::Cone => "Rotating cone",
            LaserPattern::Scatter => "Scatter",
        }
    }
    pub fn index(self) -> u32 {
        LaserPattern::ALL
            .iter()
            .position(|t| *t == self)
            .unwrap_or(0) as u32
    }
}

/// Beams shooting from the layer's origin along its +Y axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lasers {
    pub count: u32,
    pub pattern: LaserPattern,
    /// Opening angle in degrees.
    pub spread: Param,
    pub length: Param,
    pub width: Param,
    pub color_a: Rgb,
    pub color_b: Rgb,
    /// Brightness (animatable).
    pub intensity: Param,
    /// Sweep angle in degrees.
    pub sweep: Param,
    /// Sweeps per loop.
    pub sweep_cycles: i32,
    /// Flash on every beat (0 = steady, 1 = full strobe).
    pub strobe: Param,
    pub seed: u32,
    /// Thin laser beams or wide, hazy spotlight cones.
    #[serde(skip_serializing_if = "is_default")]
    pub style: BeamStyle,
    /// Spotlights: opening of each cone in degrees.
    #[serde(skip_serializing_if = "is_default")]
    pub cone: ConeAngle,
    /// Spotlights: pools of light where the cones hit the ground (y = 0).
    #[serde(skip_serializing_if = "is_default")]
    pub pools: bool,
}

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum BeamStyle {
        #[default]
        Laser => "Laser beams",
        Spotlight => "Spotlight cones",
    }
}

/// Opening angle of spotlight cones (degrees, animatable).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConeAngle(pub Param);

impl Default for ConeAngle {
    fn default() -> Self {
        ConeAngle(Param::new(18.0))
    }
}

impl Default for Lasers {
    fn default() -> Self {
        Lasers {
            count: 8,
            pattern: LaserPattern::Fan,
            spread: Param::new(70.0),
            length: Param::new(40.0),
            width: Param::new(0.08),
            color_a: hex(0x20ff60),
            color_b: hex(0x20a0ff),
            intensity: Param::new(3.0),
            sweep: Param::new(25.0),
            sweep_cycles: 1,
            strobe: Param::new(0.0),
            seed: 1,
            style: BeamStyle::Laser,
            cone: ConeAngle::default(),
            pools: false,
        }
    }
}

/// Where electric arcs run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode")]
pub enum ArcPath {
    /// One arc between two points (layer space).
    Points { from: [f32; 3], to: [f32; 3] },
    /// From the layer's position to the nearest copies of another shape
    /// or sprite layer: the arcs jump as the copies move.
    Nearest { target: String, count: u32 },
    /// From each copy of another layer to the next, round the ring.
    Chain { target: String },
}

impl ArcPath {
    pub fn label(&self) -> &'static str {
        match self {
            ArcPath::Points { .. } => "Between two points",
            ArcPath::Nearest { .. } => "To the nearest copies",
            ArcPath::Chain { .. } => "Copy to copy",
        }
    }

    pub fn target(&self) -> Option<&str> {
        match self {
            ArcPath::Points { .. } => None,
            ArcPath::Nearest { target, .. } | ArcPath::Chain { target } => Some(target),
        }
    }
}

/// Tesla-coil lightning: jagged arcs that crawl and re-strike a whole
/// number of times per loop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArcLayer {
    pub path: ArcPath,
    /// New shapes per loop (whole, so the loop closes).
    pub strikes: u32,
    /// How far the arc zigzags, relative to its length.
    pub jag: Param,
    /// How fast the zigzag crawls along a strike.
    pub crawl: f32,
    /// Side branches.
    pub branches: bool,
    pub width: Param,
    pub color: Rgb,
    pub glow: Param,
    /// How much each strike fades before the next (0 = steady).
    pub fade: f32,
    pub seed: u32,
}

impl Default for ArcLayer {
    fn default() -> Self {
        ArcLayer {
            path: ArcPath::Points {
                from: [-2.0, 0.0, 0.0],
                to: [2.0, 0.0, 0.0],
            },
            strikes: 16,
            jag: Param::new(0.15),
            crawl: 1.0,
            branches: true,
            width: Param::new(0.08),
            color: [0.55, 0.7, 1.0],
            glow: Param::new(2.0),
            fade: 0.6,
            seed: 1,
        }
    }
}
