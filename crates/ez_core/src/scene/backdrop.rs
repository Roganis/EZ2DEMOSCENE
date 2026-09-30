//! Backdrops (full-screen procedural and raymarched backgrounds) and the mirror floor.

use super::*;

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum BackdropKind {
        #[default]
        Gradient => "Gradient sky",
        Nebula => "Nebula clouds",
        Starfield => "Starfield",
        Tunnel => "Raymarched tunnel",
        Fractal => "Raymarched fractal",
        Plasma => "Oldschool plasma",
        SynthGrid => "Synthwave sun & grid",
        /// Flight through an infinite raymarched Menger sponge.
        Sponge => "Raymarched sponge flight",
        /// Flight through a corridor of glowing rings.
        Rings => "Raymarched ring corridor",
        /// Sky with raymarched volumetric clouds and a sun.
        Clouds => "Volumetric clouds",
        /// Night sky with rippling aurora curtains.
        Aurora => "Aurora night sky",
        /// EarthBound-style battle background: flat patterns whose lines
        /// wobble, with cycling colours (see [`Battle`]).
        Battle => "Battle background (retro RPG)",
        /// The environment map (Light & fog → Environment light) all around,
        /// turned with it; *Detail* blurs it.
        Environment => "Environment map",
        /// Quake's sky: a far and a near cloud layer (with see-through holes)
        /// scrolling at their own speeds (see [`LayeredSky`]).
        LayeredSky => "Two-layer sky (Quake)",
    }
}

impl BackdropKind {
    /// Kinds that fly through raymarched space (the view can roll).
    pub fn is_flight(self) -> bool {
        matches!(
            self,
            BackdropKind::Tunnel
                | BackdropKind::Fractal
                | BackdropKind::Sponge
                | BackdropKind::Rings
        )
    }
    pub fn index(self) -> u32 {
        BackdropKind::ALL
            .iter()
            .position(|e| *e == self)
            .unwrap_or(0) as u32
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Backdrop {
    pub kind: BackdropKind,
    pub color_a: Rgb,
    pub color_b: Rgb,
    pub color_c: Rgb,
    /// Motion cycles per loop.
    pub speed: i32,
    pub intensity: Param,
    /// Kind-specific detail / scale.
    pub detail: Param,
    /// Texture used by the tunnel walls.
    pub texture: Option<String>,
    /// Settings of the raymarched kinds (tunnel, fractal, sponge, rings).
    #[serde(skip_serializing_if = "is_default")]
    pub ray: RaySettings,
    /// Render at a lower resolution and upscale (much faster for the
    /// raymarched kinds and clouds, slightly softer).
    #[serde(skip_serializing_if = "is_default")]
    pub resolution: BgResolution,
    /// Settings of the battle background.
    #[serde(skip_serializing_if = "is_default")]
    pub battle: Battle,
    /// Settings of the two-layer sky (its far layer is `texture`).
    #[serde(skip_serializing_if = "is_default")]
    pub sky: LayeredSky,
}

/// Quake's sky: two pictures projected on a flattened dome, the near one
/// drawn over the far one except where it has the see-through colour.
/// Each scrolls a whole number of tiles per loop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LayeredSky {
    /// The near layer (built-in or your image); `None`: built-in clouds.
    pub near_texture: Option<String>,
    /// Colour of the near layer that is see-through.
    pub cutout: Rgb,
    /// How close to the see-through colour counts (0..1).
    pub tolerance: f32,
    /// Tiles scrolled per loop (x, z) by the far and near layers.
    pub far_scroll: [i32; 2],
    pub near_scroll: [i32; 2],
    /// Tiles across the dome.
    pub tiles: f32,
    /// How flat the dome is (Quake: 3; higher = flatter, more tiles near
    /// the horizon).
    pub flatten: f32,
}

impl Default for LayeredSky {
    fn default() -> Self {
        LayeredSky {
            near_texture: None,
            cutout: [0.0, 0.0, 0.0],
            tolerance: 0.1,
            far_scroll: [1, 1],
            near_scroll: [2, 2],
            tiles: 3.0,
            flatten: 3.0,
        }
    }
}

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum BgResolution {
        #[default]
        Full => "Full",
        Half => "Half (4× faster)",
        Quarter => "Quarter (16× faster)",
    }
}

impl BgResolution {
    /// Pixel size divisor.
    pub fn divisor(self) -> u32 {
        match self {
            BgResolution::Full => 1,
            BgResolution::Half => 2,
            BgResolution::Quarter => 4,
        }
    }
}

/// Settings shared by the raymarched backgrounds. What each one does
/// depends on the kind (see [`RaySettings::labels`]); the defaults keep the
/// original look.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RaySettings {
    /// Style: tunnel shape, fractal formula, sponge type, ring shape.
    pub variant: u32,
    /// Tunnel wall pattern when there is no texture.
    pub pattern: u32,
    /// Radius / zoom / cell size (×).
    pub size: Param,
    /// Twist of the space along the flight.
    pub twist: Param,
    /// Wall wobble / fractal fold (×).
    pub warp: Param,
    /// How much the flight path bends (×).
    pub bend: Param,
    /// Glowing lights / edge glow.
    pub glow: Param,
    /// Depth fog (×).
    pub fog: Param,
    /// Quality: raymarch steps or fractal iterations (0 = default).
    pub steps: u32,
    /// Whole rolls of the view per loop.
    pub spin: i32,
}

impl Default for RaySettings {
    fn default() -> Self {
        RaySettings {
            variant: 0,
            pattern: 0,
            size: Param::new(1.0),
            twist: Param::new(0.0),
            warp: Param::new(1.0),
            bend: Param::new(1.0),
            glow: Param::new(0.0),
            fog: Param::new(1.0),
            steps: 0,
            spin: 0,
        }
    }
}

impl RaySettings {
    /// Names of the style choices for a kind (empty = no styles).
    pub fn variants(kind: BackdropKind) -> &'static [&'static str] {
        match kind {
            BackdropKind::Tunnel => &["Round", "Square", "Hexagon", "Triangle", "Flower"],
            BackdropKind::Fractal => &["Kaliset", "Crystal", "Nebula"],
            BackdropKind::Sponge => &["Menger sponge", "Beam lattice", "Cube field"],
            BackdropKind::Rings => &["Rings", "Squares", "Triangles"],
            BackdropKind::Clouds => &["Fluffy", "Overcast", "Storm"],
            BackdropKind::Aurora => &["Curtains", "Rings", "Spiral"],
            _ => &[],
        }
    }

    /// Labels for (size, twist, warp, bend, glow) for a kind; `None` hides
    /// the setting.
    pub fn labels(kind: BackdropKind) -> [Option<&'static str>; 5] {
        match kind {
            BackdropKind::Tunnel => [
                Some("Radius ×"),
                Some("Twist"),
                Some("Wall wobble ×"),
                Some("Path bend ×"),
                Some("Light rings"),
            ],
            BackdropKind::Fractal => [
                Some("Zoom ×"),
                None,
                Some("Fold ×"),
                Some("Drift ×"),
                Some("Brightness"),
            ],
            BackdropKind::Sponge => [
                Some("Cell size ×"),
                Some("Twist"),
                None,
                Some("Path bend ×"),
                Some("Edge glow"),
            ],
            BackdropKind::Rings => [
                Some("Ring size ×"),
                Some("Twist"),
                Some("Thickness ×"),
                Some("Path bend ×"),
                Some("Glow"),
            ],
            BackdropKind::Clouds => [
                Some("Cloud scale ×"),
                None,
                Some("Coverage ×"),
                Some("Thickness ×"),
                Some("Sun glow"),
            ],
            BackdropKind::Aurora => [
                Some("Height ×"),
                Some("Sway"),
                Some("Ripples ×"),
                None,
                Some("Brightness"),
            ],
            _ => [None; 5],
        }
    }
}

impl Default for Backdrop {
    fn default() -> Self {
        Backdrop {
            kind: BackdropKind::Gradient,
            color_a: hex(0x05050a),
            color_b: hex(0x302040),
            color_c: hex(0x5060a0),
            speed: 1,
            intensity: Param::new(1.0),
            detail: Param::new(1.0),
            texture: None,
            ray: RaySettings::default(),
            resolution: BgResolution::Full,
            battle: Battle::default(),
            sky: LayeredSky::default(),
        }
    }
}

/// How the lines of a picture are pushed about, as in the battle
/// backgrounds of 16-bit RPGs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LineWarp {
    /// Still.
    None,
    /// Each line slides left and right along a wave.
    #[default]
    Wave,
    /// Odd and even lines slide opposite ways.
    Interlaced,
    /// Lines bunch up and spread out vertically.
    Compression,
}

impl LineWarp {
    pub const ALL: [LineWarp; 4] = [
        LineWarp::None,
        LineWarp::Wave,
        LineWarp::Interlaced,
        LineWarp::Compression,
    ];
    /// The moving ones (for the line wobble effect).
    pub const MOVING: [LineWarp; 3] = [LineWarp::Wave, LineWarp::Interlaced, LineWarp::Compression];
    pub fn label(self) -> &'static str {
        match self {
            LineWarp::None => "None",
            LineWarp::Wave => "Wave (lines slide)",
            LineWarp::Interlaced => "Interlaced (odd/even opposite)",
            LineWarp::Compression => "Compression (lines squeeze)",
        }
    }
    pub fn index(self) -> u32 {
        LineWarp::ALL.iter().position(|e| *e == self).unwrap_or(0) as u32
    }
}

labeled_enum! {
    /// The pattern of a battle background layer. Each is a ramp of values
    /// that the colours cycle through.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum BattlePattern {
        #[default]
        Rings => "Rings",
        Diamonds => "Diamonds",
        Checker => "Checker",
        Stripes => "Diagonal stripes",
        Zigzag => "Zigzag",
        Dots => "Dots",
        Swirl => "Swirl",
        Bricks => "Bricks",
        Plasma => "Plasma",
        /// The backdrop's picture (its brightness picks the colour).
        Picture => "Picture (the texture)",
    }
}

impl BattlePattern {
    pub fn index(self) -> u32 {
        BattlePattern::ALL
            .iter()
            .position(|e| *e == self)
            .unwrap_or(0) as u32
    }
}

labeled_enum! {
    /// How the front battle layer goes over the back one.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum BattleBlend {
        /// See-through, by its opacity.
        #[default]
        Mix => "Mix",
        /// Adds light.
        Add => "Add",
        /// Lightens, softer than add.
        Screen => "Screen",
        /// Colours flip where both are bright.
        Difference => "Difference",
    }
}

impl BattleBlend {
    pub fn index(self) -> u32 {
        BattleBlend::ALL
            .iter()
            .position(|e| *e == self)
            .unwrap_or(0) as u32
    }
}

/// One layer of a battle background: a tiled pattern that scrolls, whose
/// lines wobble, coloured by the backdrop's colours cycling through it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BattleLayer {
    pub enabled: bool,
    pub pattern: BattlePattern,
    /// Tiles from the top of the picture to the bottom.
    pub tiles: Param,
    /// Tiles scrolled per loop, sideways and up.
    pub scroll: [i32; 2],
    pub warp: LineWarp,
    /// How far lines move (fraction of the picture's height).
    pub amount: Param,
    /// Waves from the top of the picture to the bottom.
    pub waves: Param,
    /// Times the waves roll by per loop.
    pub wave_speed: i32,
    /// Colour bands along the pattern's ramp.
    pub bands: Param,
    /// Times the colours cycle through the pattern per loop.
    pub cycles: i32,
    /// How much of the layer shows (the front one, over the back).
    pub opacity: Param,
}

impl Default for BattleLayer {
    fn default() -> Self {
        BattleLayer {
            enabled: true,
            pattern: BattlePattern::Rings,
            tiles: Param::new(3.0),
            scroll: [1, 0],
            warp: LineWarp::Wave,
            amount: Param::new(0.04),
            waves: Param::new(3.0),
            wave_speed: 2,
            bands: Param::new(2.0),
            cycles: 4,
            opacity: Param::new(1.0),
        }
    }
}

/// An EarthBound-style battle background: a back layer and an optional
/// front one over it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Battle {
    pub back: BattleLayer,
    pub front: BattleLayer,
    pub blend: BattleBlend,
    /// Lines from top to bottom: fat retro pixels (0 = full resolution).
    pub lines: u32,
    /// Colours in the cycling palette (0 = smooth).
    pub steps: u32,
}

impl Default for Battle {
    fn default() -> Self {
        Battle {
            back: BattleLayer::default(),
            front: BattleLayer {
                enabled: false,
                pattern: BattlePattern::Diamonds,
                tiles: Param::new(5.0),
                scroll: [0, -1],
                warp: LineWarp::Interlaced,
                amount: Param::new(0.03),
                waves: Param::new(6.0),
                wave_speed: -3,
                bands: Param::new(1.0),
                cycles: -2,
                opacity: Param::new(0.5),
            },
            blend: BattleBlend::Mix,
            lines: 224,
            steps: 8,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MirrorFloor {
    /// Half size of the floor square.
    pub size: f32,
    /// Endless: the floor follows the camera to the horizon and fades into
    /// the sky there (`size` is not used).
    #[serde(skip_serializing_if = "is_default")]
    pub infinite: bool,
    pub base_color: Rgb,
    /// 0 = matte, 1 = perfect mirror.
    pub reflectivity: Param,
    /// Blur of the reflection (0..1).
    pub blur: Param,
    pub tint: Rgb,
    pub texture: Option<String>,
    pub texture_scale: f32,
    /// Glowing grid line intensity (0 = off).
    pub grid: Param,
    pub grid_color: Rgb,
    pub grid_scale: Param,
    /// Grid scroll (cells per loop).
    pub grid_scroll: i32,
}

impl Default for MirrorFloor {
    fn default() -> Self {
        MirrorFloor {
            size: 40.0,
            infinite: false,
            base_color: hex(0x080808),
            reflectivity: Param::new(0.6),
            blur: Param::new(0.2),
            tint: [1.0, 1.0, 1.0],
            texture: None,
            texture_scale: 1.0,
            grid: Param::new(0.0),
            grid_color: hex(0xff2040),
            grid_scale: Param::new(1.0),
            grid_scroll: 0,
        }
    }
}
