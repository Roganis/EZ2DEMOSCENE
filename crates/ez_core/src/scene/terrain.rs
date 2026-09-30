//! Terrain layers (a scrolling landscape) and their liquids.

use super::*;

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum TerrainStyle {
        /// Glowing grid lines only.
        #[default]
        Wireframe => "Wireframe",
        /// Lit solid ground.
        Solid => "Solid",
        /// Solid ground with glowing grid lines.
        Both => "Solid + lines",
    }
}

impl TerrainStyle {
    pub fn index(self) -> u32 {
        TerrainStyle::ALL
            .iter()
            .position(|t| *t == self)
            .unwrap_or(0) as u32
    }
}

/// An endless landscape that scrolls towards the camera and repeats
/// exactly once per `scroll` unit, so the loop is seamless.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Terrain {
    /// Width and depth in world units.
    pub size: f32,
    /// Grid cells along each side.
    pub cells: u32,
    /// Level of detail: `cells` is the resolution near the camera and the
    /// grid gets gradually coarser away from it (a quarter of the
    /// triangles).
    #[serde(skip_serializing_if = "is_default")]
    pub lod: bool,
    /// Mountain height (animatable).
    pub height: Param,
    /// Hills across the terrain (whole number, keeps it tileable).
    pub hills: u32,
    /// Sharpness: more octaves of detail.
    pub roughness: Param,
    /// How many times the landscape scrolls past per loop.
    pub scroll: i32,
    /// Flat valley down the middle (0 = none, 1 = wide).
    pub valley: Param,
    pub style: TerrainStyle,
    pub line_color: Rgb,
    /// Line glow (animatable).
    pub glow: Param,
    pub fill_color: Rgb,
    pub seed: u32,
    /// Built-in texture name or a user texture name.
    pub texture: Option<String>,
    /// Texture repeats across the terrain (whole number keeps loops seamless).
    pub tiles: u32,
    /// Also colour the grid lines with the texture.
    pub texture_lines: bool,
    /// Nearest-neighbour sampling for chunky pixels.
    pub pixelated: bool,
    /// Kind of landscape.
    #[serde(skip_serializing_if = "is_default")]
    pub shape: TerrainShape,
    /// Colours by height and slope (snowy peaks, sandy shores…).
    #[serde(skip_serializing_if = "is_default")]
    pub biome: Biome,
    /// Water, lava… filling the low ground.
    #[serde(skip_serializing_if = "is_default")]
    pub liquid: Liquid,
}

impl Terrain {
    /// Highest `cells`: 256, or 512 with level of detail.
    pub fn max_cells(&self) -> u32 {
        if self.lod {
            512
        } else {
            256
        }
    }

    /// Grid cells along each side actually drawn.
    pub fn drawn_cells(&self) -> u32 {
        let cells = self.cells.clamp(4, self.max_cells());
        if self.lod {
            (cells / 2).max(4)
        } else {
            cells
        }
    }
}

/// Graded grid for terrain level of detail, along one axis: `drawn` grid
/// lines spread over 0..1 so the spacing is `1 / cells` at `focus` and
/// grows linearly with distance from it (`1 + k·d` times). Returns `k` and
/// the (fractional) grid index that lands on the focus. `k` = 0 is uniform.
pub fn lod_grading(cells: u32, drawn: u32, focus: f32) -> (f32, f32) {
    let (n, m, c) = (cells as f64, drawn as f64, focus.clamp(0.0, 1.0) as f64);
    if drawn >= cells {
        return (0.0, (c * m) as f32);
    }
    let total = |k: f64| n / k * ((1.0 + k * c).ln() + (1.0 + k * (1.0 - c)).ln());
    // total() falls from n (k -> 0) towards 0: bisect for total(k) = m.
    let (mut lo, mut hi) = (1e-6f64, 1e6f64);
    for _ in 0..100 {
        let mid = (lo * hi).sqrt();
        if total(mid) > m {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let k = (lo * hi).sqrt();
    (k as f32, (n / k * (1.0 + k * c).ln()) as f32)
}

/// Where grid line `i` of a [`lod_grading`] lands (0..1).
pub fn lod_position(i: f32, cells: u32, focus: f32, k: f32, s0: f32) -> f32 {
    let x = i - s0;
    let d = if k < 1e-4 {
        x.abs() / cells as f32
    } else {
        ((x.abs() * k / cells as f32).exp() - 1.0) / k
    };
    (focus + x.signum() * d).clamp(0.0, 1.0)
}

impl Default for Terrain {
    fn default() -> Self {
        Terrain {
            size: 60.0,
            cells: 64,
            lod: false,
            height: Param::new(4.0),
            hills: 4,
            roughness: Param::new(0.5),
            scroll: 1,
            valley: Param::new(0.3),
            style: TerrainStyle::Wireframe,
            line_color: hex(0xff2bd6),
            glow: Param::new(1.0),
            fill_color: hex(0x0a0418),
            seed: 1,
            texture: None,
            tiles: 8,
            texture_lines: false,
            pixelated: false,
            shape: TerrainShape::Hills,
            biome: Biome::Plain,
            liquid: Liquid::default(),
        }
    }
}

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum TerrainShape {
        /// Rolling hills.
        #[default]
        Hills => "Hills",
        /// Sharp ridged mountains.
        Mountains => "Ridged mountains",
        /// Flat-topped mesas in steps.
        Mesas => "Mesas (terraces)",
        /// Wind-blown sand dunes.
        Dunes => "Sand dunes",
        /// Deep winding canyons in a plateau.
        Canyons => "Canyons",
        /// Round craters, like the moon.
        Craters => "Craters",
    }
}

impl TerrainShape {
    pub fn index(self) -> u32 {
        TerrainShape::ALL
            .iter()
            .position(|t| *t == self)
            .unwrap_or(0) as u32
    }
}

labeled_enum! {
    /// Height and slope based colouring of solid terrain.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum Biome {
        /// One ground colour.
        #[default]
        Plain => "Plain (ground colour)",
        /// Grass, rock and snowy peaks.
        Alpine => "Alpine",
        /// Sand and red rock.
        Desert => "Desert",
        /// Black basalt with glowing cracks.
        Volcanic => "Volcanic",
        /// Snow and blue ice.
        Arctic => "Arctic",
        /// Purple moss and teal crystal.
        Alien => "Alien",
    }
}

impl Biome {
    pub fn index(self) -> u32 {
        Biome::ALL.iter().position(|t| *t == self).unwrap_or(0) as u32
    }
}

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum LiquidKind {
        #[default]
        None => "None",
        /// Reflective water with ripples, foam and a sun glint.
        Water => "Water",
        /// Glowing, slowly churning lava with a dark crust.
        Lava => "Lava",
        /// Radioactive green goo with bubbles.
        Toxic => "Toxic goo",
        /// Frozen, cracked ice.
        Ice => "Ice",
    }
}

impl LiquidKind {
    pub fn index(self) -> u32 {
        LiquidKind::ALL.iter().position(|t| *t == self).unwrap_or(0) as u32
    }
    /// A good colour to start from.
    pub fn default_color(self) -> Rgb {
        match self {
            LiquidKind::None | LiquidKind::Water => hex(0x0b3d5c),
            LiquidKind::Lava => hex(0xff5a10),
            LiquidKind::Toxic => hex(0x40ff30),
            LiquidKind::Ice => hex(0x9fd8f0),
        }
    }
}

/// A liquid filling the terrain up to `level`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Liquid {
    pub kind: LiquidKind,
    /// Surface height as a fraction of the mountain height (animatable:
    /// tides, rising lava).
    pub level: Param,
    pub color: Rgb,
    /// Glow of lava / goo, shine of water and ice.
    pub glow: Param,
    /// Size of waves, ripples and crust (0 = still).
    pub waves: Param,
    /// Current: whole drifts of the surface pattern per loop.
    pub flow: i32,
    /// Quake's wobbling warp of the surface patterns, in pattern cells
    /// (0 = off; animatable), `flow`-independent: whole turns per loop
    /// set by `turb_cycles`.
    #[serde(skip_serializing_if = "is_off")]
    pub turbulence: Param,
    #[serde(skip_serializing_if = "is_two", default = "two")]
    pub turb_cycles: i32,
}

impl Default for Liquid {
    fn default() -> Self {
        Liquid {
            kind: LiquidKind::None,
            level: Param::new(0.25),
            color: hex(0x0b3d5c),
            glow: Param::new(1.0),
            waves: Param::new(1.0),
            flow: 1,
            turbulence: Param::new(0.0),
            turb_cycles: 2,
        }
    }
}

fn two() -> i32 {
    2
}

fn is_two(v: &i32) -> bool {
    *v == 2
}
