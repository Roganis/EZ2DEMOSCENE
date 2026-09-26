//! The project / scene description. This is what gets saved as `.ez2.json`.

use crate::clock::Timing;
use crate::color::{hex, Rgb};
use crate::graph::Graph;
use crate::palette::PaletteId;
use crate::param::Param;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Keeps project files short: optional features left at their defaults
/// are not written.
fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// 2: asset paths may be relative to the project file.
pub const PROJECT_VERSION: u32 = 2;

/// A complete loopable scene.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub version: u32,
    pub name: String,
    pub timing: Timing,
    pub camera: Camera,
    pub environment: Environment,
    pub layers: Vec<Layer>,
    pub post: PostStack,
    /// User images usable as material textures.
    pub textures: Vec<UserTexture>,
    /// Optional music file played with the loop and used for modulation.
    pub audio: Option<String>,
    /// When `Some` and `use_graph` is set, layers come from the node graph.
    pub graph: Option<Graph>,
    pub use_graph: bool,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            version: PROJECT_VERSION,
            name: "Untitled".into(),
            timing: Timing::default(),
            camera: Camera::default(),
            environment: Environment::default(),
            layers: Vec::new(),
            post: PostStack::default(),
            textures: Vec::new(),
            audio: None,
            graph: None,
            use_graph: false,
        }
    }
}

impl Project {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("project serialises")
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Layers to render: either the plain layer list or the compiled graph.
    pub fn scene_layers(&self) -> Cow<'_, [Layer]> {
        match (&self.graph, self.use_graph) {
            (Some(g), true) => Cow::Owned(g.compile()),
            _ => Cow::Borrowed(&self.layers),
        }
    }

    pub fn find_texture(&self, name: &str) -> Option<&UserTexture> {
        self.textures.iter().find(|t| t.name == name)
    }
}

// ---------------------------------------------------------------------------
// Camera & environment

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CameraMode {
    /// Circles the target `orbit_turns` times per loop.
    #[default]
    Orbit,
    /// Swings back and forth by `swing` degrees.
    Pendulum,
    /// Fixed position; distance/height params can still breathe.
    Static,
}

impl CameraMode {
    pub const ALL: [CameraMode; 3] = [CameraMode::Orbit, CameraMode::Pendulum, CameraMode::Static];
    pub fn label(self) -> &'static str {
        match self {
            CameraMode::Orbit => "Orbit",
            CameraMode::Pendulum => "Pendulum",
            CameraMode::Static => "Static",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Camera {
    pub mode: CameraMode,
    pub target: [f32; 3],
    pub distance: Param,
    pub height: Param,
    /// Starting azimuth in degrees.
    pub angle: Param,
    pub orbit_turns: i32,
    /// Pendulum amplitude in degrees.
    pub swing: Param,
    /// Vertical field of view in degrees.
    pub fov: Param,
    /// Roll in degrees.
    pub roll: Param,
    /// Camera shake on every beat (0 = none).
    pub beat_shake: Param,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            mode: CameraMode::Orbit,
            target: [0.0, 0.0, 0.0],
            distance: Param::new(10.0),
            height: Param::new(3.0),
            angle: Param::new(0.0),
            orbit_turns: 1,
            swing: Param::new(30.0),
            fov: Param::new(55.0),
            roll: Param::new(0.0),
            beat_shake: Param::new(0.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Environment {
    pub fog_color: Rgb,
    pub fog_density: Param,
    /// Colours used for ambient light and fake reflections.
    pub sky_color: Rgb,
    pub ground_color: Rgb,
    pub light_dir: [f32; 3],
    pub light_color: Rgb,
    pub light_intensity: Param,
    pub ambient: Param,
}

impl Default for Environment {
    fn default() -> Self {
        Environment {
            fog_color: hex(0x101018),
            fog_density: Param::new(0.02),
            sky_color: hex(0x8090b0),
            ground_color: hex(0x202020),
            light_dir: [0.4, 1.0, 0.3],
            light_color: [1.0, 1.0, 1.0],
            light_intensity: Param::new(1.5),
            ambient: Param::new(0.3),
        }
    }
}

// ---------------------------------------------------------------------------
// Layers

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub name: String,
    pub enabled: bool,
    pub transform: Transform,
    pub symmetry: Symmetry,
    /// Blinking / flashing over the loop.
    #[serde(skip_serializing_if = "is_default")]
    pub blink: Blink,
    pub kind: LayerKind,
}

impl Default for Layer {
    fn default() -> Self {
        Layer {
            name: "Layer".into(),
            enabled: true,
            transform: Transform::default(),
            symmetry: Symmetry::None,
            blink: Blink::default(),
            kind: LayerKind::Mesh(MeshLayer::default()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlinkMode {
    /// Always visible.
    #[default]
    Off,
    /// On/off in a regular rhythm.
    Blink,
    /// On/off at random moments.
    Random,
    /// Glow flashes and fades (the layer stays visible).
    Flash,
}

impl BlinkMode {
    pub const ALL: [BlinkMode; 4] = [
        BlinkMode::Off,
        BlinkMode::Blink,
        BlinkMode::Random,
        BlinkMode::Flash,
    ];
    pub fn label(self) -> &'static str {
        match self {
            BlinkMode::Off => "Off",
            BlinkMode::Blink => "Blink",
            BlinkMode::Random => "Random blink",
            BlinkMode::Flash => "Flash",
        }
    }
}

/// Loop-safe strobe: the rhythm repeats `per_loop` times per loop.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Blink {
    pub mode: BlinkMode,
    /// Blinks / flashes per loop (e.g. 16 = every beat of a 16-beat loop).
    pub per_loop: u32,
    /// Blink: fraction of the time on. Random: chance of being on.
    /// Flash: brightness between flashes.
    pub duty: f32,
    /// Shift of the rhythm (fraction of one blink).
    pub offset: f32,
    pub seed: u32,
}

impl Default for Blink {
    fn default() -> Self {
        Blink {
            mode: BlinkMode::Off,
            per_loop: 16,
            duty: 0.5,
            offset: 0.0,
            seed: 1,
        }
    }
}

impl Blink {
    /// `None` when the layer is hidden at this moment, otherwise a glow
    /// multiplier.
    pub fn eval(&self, phase: f32) -> Option<f32> {
        let n = self.per_loop.max(1) as f32;
        let x = phase.rem_euclid(1.0) * n + self.offset;
        let duty = self.duty.clamp(0.0, 1.0);
        match self.mode {
            BlinkMode::Off => Some(1.0),
            BlinkMode::Blink => (x.rem_euclid(1.0) < duty).then_some(1.0),
            BlinkMode::Random => {
                let step = x.floor().rem_euclid(n) as u32;
                let r = crate::rng::hash_u32(
                    step.wrapping_mul(0x9e37_79b9) ^ self.seed.wrapping_mul(0x85eb_ca6b),
                );
                let r = (r >> 8) as f32 / (1u32 << 24) as f32;
                (r < duty).then_some(1.0)
            }
            BlinkMode::Flash => {
                let t = 1.0 - x.rem_euclid(1.0);
                Some(duty + (1.0 - duty) * t.powi(4) + t.powi(12) * 2.0)
            }
        }
    }
}

impl Layer {
    pub fn new(name: impl Into<String>, kind: LayerKind) -> Self {
        Layer {
            name: name.into(),
            kind,
            ..Default::default()
        }
    }

    pub fn at(mut self, pos: [f32; 3]) -> Self {
        self.transform.position = pos;
        self
    }

    pub fn rotated(mut self, deg: [f32; 3]) -> Self {
        self.transform.rotation = deg;
        self
    }

    pub fn scaled(mut self, s: f32) -> Self {
        self.transform.scale = Param::new(s);
        self
    }

    pub fn stretched(mut self, s: [f32; 3]) -> Self {
        self.transform.stretch = s;
        self
    }

    pub fn spin(mut self, turns: [i32; 3]) -> Self {
        self.transform.spin = turns;
        self
    }

    pub fn sym(mut self, s: Symmetry) -> Self {
        self.symmetry = s;
        self
    }

    pub fn type_label(&self) -> &'static str {
        match &self.kind {
            LayerKind::Mesh(_) => "Mesh",
            LayerKind::Particles(_) => "Particles",
            LayerKind::Backdrop(_) => "Backdrop",
            LayerKind::Mirror(_) => "Mirror floor",
            LayerKind::Terrain(_) => "Terrain",
            LayerKind::Lasers(_) => "Laser beams",
            LayerKind::Ribbon(_) => "Neon ribbon",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LayerKind {
    Mesh(MeshLayer),
    Particles(ParticleLayer),
    Backdrop(Backdrop),
    Mirror(MirrorFloor),
    Terrain(Terrain),
    Lasers(Lasers),
    Ribbon(Ribbon),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform {
    pub position: [f32; 3],
    /// Euler rotation in degrees (applied Y, X, Z).
    pub rotation: [f32; 3],
    /// Size of the layer / of each copy (animatable). Instancer distances
    /// are not scaled.
    pub scale: Param,
    /// Per-axis stretch multiplied with `scale`.
    pub stretch: [f32; 3],
    /// Whole turns per loop around X, Y, Z.
    pub spin: [i32; 3],
    /// Vertical offset (animatable, e.g. bobbing).
    pub bob: Param,
    /// Random jolts on a rhythm.
    #[serde(skip_serializing_if = "is_default")]
    pub shake: Shake,
}

/// Loop-safe random jolts: a new random direction `per_loop` times per
/// loop, scaled by `amount` / `turn`. Give those a fade (e.g. Exp fade out,
/// every beat) for a hit that settles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shake {
    /// Distance of the jolt (animatable).
    pub amount: Param,
    /// Rotation of the jolt in degrees (animatable).
    pub turn: Param,
    /// New random direction this many times per loop.
    pub per_loop: u32,
    pub seed: u32,
}

impl Default for Shake {
    fn default() -> Self {
        Shake {
            amount: Param::new(0.0),
            turn: Param::new(0.0),
            per_loop: 16,
            seed: 1,
        }
    }
}

impl Shake {
    pub fn is_active(&self) -> bool {
        self.amount.base != 0.0
            || self.amount.is_animated()
            || self.turn.base != 0.0
            || self.turn.is_animated()
    }
}

impl Default for Transform {
    fn default() -> Self {
        Transform {
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: Param::new(1.0),
            stretch: [1.0; 3],
            spin: [0; 3],
            bob: Param::new(0.0),
            shake: Shake::default(),
        }
    }
}

/// World-space duplication of a whole layer around the origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Symmetry {
    #[default]
    None,
    /// Mirror across the YZ plane (left/right).
    MirrorX,
    /// Mirror across the XY plane (front/back).
    MirrorZ,
    /// Mirror across both: four copies.
    MirrorXZ,
    /// `count` copies rotated around the Y axis.
    Radial { count: u32 },
    /// `count` rotated copies, every other one mirrored (kaleidoscope-like).
    Kaleido { count: u32 },
}

impl Symmetry {
    pub fn label(&self) -> &'static str {
        match self {
            Symmetry::None => "None",
            Symmetry::MirrorX => "Mirror X",
            Symmetry::MirrorZ => "Mirror Z",
            Symmetry::MirrorXZ => "Mirror XZ",
            Symmetry::Radial { .. } => "Radial",
            Symmetry::Kaleido { .. } => "Kaleido",
        }
    }
}

// ---------------------------------------------------------------------------
// Mesh layer

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeshLayer {
    pub source: MeshSource,
    pub material: Material,
    pub instancer: Instancer,
    pub variation: Variation,
}

impl Default for MeshLayer {
    fn default() -> Self {
        MeshLayer {
            source: MeshSource::Primitive(Primitive::Cube),
            material: Material::default(),
            instancer: Instancer::Single,
            variation: Variation::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MeshSource {
    Primitive(Primitive),
    /// A glTF/GLB or OBJ file.
    File {
        path: String,
    },
}

/// Built-in procedural meshes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape")]
pub enum Primitive {
    Cube,
    Tetrahedron,
    Octahedron,
    Dodecahedron,
    Icosahedron,
    Sphere {
        detail: u32,
    },
    Torus {
        thickness: f32,
        segments: u32,
    },
    Cylinder {
        segments: u32,
    },
    /// Tall faceted spike.
    Shard {
        seed: u32,
    },
    /// Cluster of spikes growing from a base.
    Crystal {
        spikes: u32,
        seed: u32,
    },
    /// Beveled slab (UVs make "edges"/"stripes" emissive modes look like light strips).
    Panel {
        bevel: f32,
    },
    /// Curved band segment (neon rings, arena walls).
    Ring {
        arc: f32,
        width: f32,
        height: f32,
        segments: u32,
    },
    /// Flat square in XZ.
    Plane,
    /// Pyramid with a square base.
    Pyramid,
    Cone {
        segments: u32,
    },
    /// Cylinder with round caps.
    Capsule {
        /// Length of the straight middle part (0 = sphere).
        length: f32,
        segments: u32,
    },
    /// Tube winding `p` times around and `q` times through a torus.
    TorusKnot {
        p: u32,
        q: u32,
        thickness: f32,
    },
    /// Flat extruded star.
    Star {
        points: u32,
        /// Inner radius (fraction of the outer one).
        inner: f32,
        depth: f32,
    },
    /// Flat extruded cog wheel.
    Gear {
        teeth: u32,
        depth: f32,
    },
    /// Coil spring (helical tube).
    Spring {
        turns: f32,
        thickness: f32,
    },
    /// Menger sponge fractal cube.
    Menger {
        level: u32,
    },
    /// Cube with rounded edges.
    RoundedCube {
        radius: f32,
    },
    /// Brilliant-cut diamond.
    Gem {
        facets: u32,
    },
    /// Flat extruded heart.
    Heart {
        depth: f32,
    },
    /// Band with a half twist.
    Mobius {
        width: f32,
    },
}

impl Primitive {
    pub fn all_defaults() -> Vec<Primitive> {
        vec![
            Primitive::Cube,
            Primitive::Tetrahedron,
            Primitive::Octahedron,
            Primitive::Dodecahedron,
            Primitive::Icosahedron,
            Primitive::Sphere { detail: 3 },
            Primitive::Torus {
                thickness: 0.3,
                segments: 32,
            },
            Primitive::Cylinder { segments: 24 },
            Primitive::Shard { seed: 1 },
            Primitive::Crystal { spikes: 7, seed: 1 },
            Primitive::Panel { bevel: 0.15 },
            Primitive::Ring {
                arc: 360.0,
                width: 0.15,
                height: 0.1,
                segments: 64,
            },
            Primitive::Plane,
            Primitive::Pyramid,
            Primitive::Cone { segments: 24 },
            Primitive::Capsule {
                length: 1.0,
                segments: 24,
            },
            Primitive::TorusKnot {
                p: 2,
                q: 3,
                thickness: 0.12,
            },
            Primitive::Star {
                points: 5,
                inner: 0.45,
                depth: 0.25,
            },
            Primitive::Gear {
                teeth: 12,
                depth: 0.25,
            },
            Primitive::Spring {
                turns: 5.0,
                thickness: 0.08,
            },
            Primitive::Menger { level: 2 },
            Primitive::RoundedCube { radius: 0.15 },
            Primitive::Gem { facets: 8 },
            Primitive::Heart { depth: 0.3 },
            Primitive::Mobius { width: 0.35 },
        ]
    }

    /// Shapes the randomizer picks from (solid, recognisable ones).
    pub fn random_pool() -> Vec<Primitive> {
        Primitive::all_defaults()
            .into_iter()
            .filter(|p| {
                !matches!(
                    p,
                    Primitive::Plane | Primitive::Panel { .. } | Primitive::Ring { .. }
                )
            })
            .collect()
    }

    pub fn label(&self) -> &'static str {
        match self {
            Primitive::Cube => "Cube",
            Primitive::Tetrahedron => "Tetrahedron",
            Primitive::Octahedron => "Octahedron",
            Primitive::Dodecahedron => "Dodecahedron",
            Primitive::Icosahedron => "Icosahedron",
            Primitive::Sphere { .. } => "Sphere",
            Primitive::Torus { .. } => "Torus",
            Primitive::Cylinder { .. } => "Cylinder",
            Primitive::Shard { .. } => "Shard",
            Primitive::Crystal { .. } => "Crystal cluster",
            Primitive::Panel { .. } => "Panel",
            Primitive::Ring { .. } => "Ring band",
            Primitive::Plane => "Plane",
            Primitive::Pyramid => "Pyramid",
            Primitive::Cone { .. } => "Cone",
            Primitive::Capsule { .. } => "Capsule",
            Primitive::TorusKnot { .. } => "Torus knot",
            Primitive::Star { .. } => "Star",
            Primitive::Gear { .. } => "Gear",
            Primitive::Spring { .. } => "Spring",
            Primitive::Menger { .. } => "Menger sponge",
            Primitive::RoundedCube { .. } => "Rounded cube",
            Primitive::Gem { .. } => "Gem",
            Primitive::Heart { .. } => "Heart",
            Primitive::Mobius { .. } => "Möbius strip",
        }
    }

    /// Stable cache key for generated geometry.
    pub fn cache_key(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// How copies of the mesh are laid out inside the layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Instancer {
    Single,
    Grid {
        counts: [u32; 3],
        spacing: [f32; 3],
    },
    /// Copies on a circle around Y, facing outward.
    Radial {
        count: u32,
        radius: f32,
    },
    /// Random points inside a sphere (or on its shell).
    Scatter {
        count: u32,
        radius: f32,
        shell: bool,
        seed: u32,
    },
    /// Debris swarm: each copy orbits on its own tilted circle.
    Orbit {
        count: u32,
        radius: f32,
        spread: f32,
        /// Whole revolutions per loop.
        speed: i32,
        seed: u32,
    },
    /// A wall of copies (optionally curved into an arc).
    Wall {
        cols: u32,
        rows: u32,
        spacing: f32,
        /// Arc angle in degrees (0 = flat wall).
        curve: f32,
    },
    /// Helix.
    Spiral {
        count: u32,
        radius: f32,
        height: f32,
        turns: f32,
    },
}

impl Instancer {
    pub fn label(&self) -> &'static str {
        match self {
            Instancer::Single => "Single",
            Instancer::Grid { .. } => "Grid",
            Instancer::Radial { .. } => "Radial",
            Instancer::Scatter { .. } => "Scatter",
            Instancer::Orbit { .. } => "Orbit swarm",
            Instancer::Wall { .. } => "Wall",
            Instancer::Spiral { .. } => "Spiral",
        }
    }

    pub fn defaults() -> Vec<Instancer> {
        vec![
            Instancer::Single,
            Instancer::Grid {
                counts: [5, 1, 5],
                spacing: [2.0, 2.0, 2.0],
            },
            Instancer::Radial {
                count: 12,
                radius: 5.0,
            },
            Instancer::Scatter {
                count: 60,
                radius: 8.0,
                shell: false,
                seed: 1,
            },
            Instancer::Orbit {
                count: 40,
                radius: 4.0,
                spread: 1.5,
                speed: 1,
                seed: 1,
            },
            Instancer::Wall {
                cols: 12,
                rows: 4,
                spacing: 1.2,
                curve: 0.0,
            },
            Instancer::Spiral {
                count: 48,
                radius: 3.0,
                height: 6.0,
                turns: 3.0,
            },
        ]
    }
}

/// Per-instance randomness and travelling waves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Variation {
    pub seed: u32,
    /// Random rotation in degrees.
    pub rotation: f32,
    /// Random scale amount (0..1).
    pub scale: f32,
    /// Random hue shift (0..1 turns).
    pub hue: f32,
    /// Random extra spins per loop (0 = none, n = up to ±n turns).
    pub spin: u32,
    /// Travelling scale wave across instances.
    pub ripple: f32,
    /// Cycles of the ripple per loop.
    pub ripple_cycles: i32,
    /// How many wavelengths across all instances.
    pub ripple_spread: f32,
    /// Travelling emissive wave (lights chasing along the instances).
    pub chase: f32,
}

impl Default for Variation {
    fn default() -> Self {
        Variation {
            seed: 1,
            rotation: 0.0,
            scale: 0.0,
            hue: 0.0,
            spin: 0,
            ripple: 0.0,
            ripple_cycles: 1,
            ripple_spread: 1.0,
            chase: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EmissiveMode {
    /// Whole surface glows.
    #[default]
    Full,
    /// Glowing outlines along UV borders (Tron look).
    Edges,
    /// Glowing stripes across the surface.
    Stripes,
    /// Glow where the texture is bright.
    Texture,
}

impl EmissiveMode {
    pub const ALL: [EmissiveMode; 4] = [
        EmissiveMode::Full,
        EmissiveMode::Edges,
        EmissiveMode::Stripes,
        EmissiveMode::Texture,
    ];
    pub fn label(self) -> &'static str {
        match self {
            EmissiveMode::Full => "Full",
            EmissiveMode::Edges => "Edges",
            EmissiveMode::Stripes => "Stripes",
            EmissiveMode::Texture => "Texture",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Material {
    pub base_color: Rgb,
    pub metallic: Param,
    pub roughness: Param,
    pub emissive_color: Rgb,
    /// Glow strength (animatable: pulse it on the beat!).
    pub emissive: Param,
    pub emissive_mode: EmissiveMode,
    /// Built-in texture name or a user texture name.
    pub texture: Option<String>,
    pub texture_scale: Param,
    /// Texture tiles scrolled per loop (U, V).
    pub scroll: [i32; 2],
    /// Nearest-neighbour texture sampling for chunky pixels.
    pub pixelated: bool,
    /// Faceted look (normals from the triangle faces).
    pub flat_shading: bool,
    /// Rim / fresnel light strength.
    pub rim: Param,
    /// Hue rotation over the loop (animatable, in turns).
    pub hue_shift: Param,
    /// Geometry corruption (off when the amount is 0).
    #[serde(skip_serializing_if = "is_default")]
    pub glitch: Glitch,
}

/// How a glitched mesh is corrupted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GlitchStyle {
    /// Vertices shake to random positions.
    #[default]
    Jitter,
    /// Horizontal bands shift sideways (VHS tearing).
    Slices,
    /// Faces fly apart along their normals.
    Shatter,
}

impl GlitchStyle {
    pub const ALL: [GlitchStyle; 3] = [
        GlitchStyle::Jitter,
        GlitchStyle::Slices,
        GlitchStyle::Shatter,
    ];
    pub fn label(self) -> &'static str {
        match self {
            GlitchStyle::Jitter => "Jitter",
            GlitchStyle::Slices => "Slices (VHS)",
            GlitchStyle::Shatter => "Shatter",
        }
    }
    pub fn index(self) -> u32 {
        GlitchStyle::ALL
            .iter()
            .position(|g| *g == self)
            .unwrap_or(0) as u32
    }
}

/// Loop-safe geometry corruption: the pattern changes `rate` times per
/// loop, and only `chance` of those steps are glitched.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Glitch {
    /// Strength (animatable). 0 = off.
    pub amount: Param,
    pub style: GlitchStyle,
    /// New random pattern this many times per loop.
    pub rate: u32,
    /// Fraction of the steps that glitch (1 = always).
    pub chance: Param,
    pub seed: u32,
}

impl Default for Glitch {
    fn default() -> Self {
        Glitch {
            amount: Param::new(0.0),
            style: GlitchStyle::Jitter,
            rate: 16,
            chance: Param::new(0.5),
            seed: 1,
        }
    }
}

impl Default for Material {
    fn default() -> Self {
        Material {
            base_color: hex(0xb0b0b8),
            metallic: Param::new(0.2),
            roughness: Param::new(0.4),
            emissive_color: hex(0xff3020),
            emissive: Param::new(0.0),
            emissive_mode: EmissiveMode::Full,
            texture: None,
            texture_scale: Param::new(1.0),
            scroll: [0, 0],
            pixelated: false,
            flat_shading: false,
            rim: Param::new(0.3),
            hue_shift: Param::new(0.0),
            glitch: Glitch::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Particles

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Emitter {
    /// Explodes outward from the centre.
    #[default]
    Burst,
    /// Drifts inside a sphere.
    Sphere,
    /// Circles in a flat ring.
    Ring,
    /// Shoots up and falls down.
    Fountain,
    /// Streams towards the camera (hyperspace).
    Warp,
    /// Spirals upward in a vortex.
    Vortex,
    /// Falls slowly like snow / glitter.
    Snow,
}

impl Emitter {
    pub const ALL: [Emitter; 7] = [
        Emitter::Burst,
        Emitter::Sphere,
        Emitter::Ring,
        Emitter::Fountain,
        Emitter::Warp,
        Emitter::Vortex,
        Emitter::Snow,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Emitter::Burst => "Burst",
            Emitter::Sphere => "Sphere drift",
            Emitter::Ring => "Ring orbit",
            Emitter::Fountain => "Fountain",
            Emitter::Warp => "Warp stars",
            Emitter::Vortex => "Vortex",
            Emitter::Snow => "Snow / glitter",
        }
    }
    pub fn index(self) -> u32 {
        Emitter::ALL.iter().position(|e| *e == self).unwrap_or(0) as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Sprite {
    #[default]
    Glow,
    Square,
    Star,
    Ring,
}

impl Sprite {
    pub const ALL: [Sprite; 4] = [Sprite::Glow, Sprite::Square, Sprite::Star, Sprite::Ring];
    pub fn label(self) -> &'static str {
        match self {
            Sprite::Glow => "Soft glow",
            Sprite::Square => "Pixel square",
            Sprite::Star => "Star",
            Sprite::Ring => "Ring",
        }
    }
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
        }
    }
}

// ---------------------------------------------------------------------------
// Backdrop (fullscreen raymarched / procedural background)

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BackdropKind {
    #[default]
    Gradient,
    Nebula,
    Starfield,
    Tunnel,
    Fractal,
    Plasma,
    SynthGrid,
}

impl BackdropKind {
    pub const ALL: [BackdropKind; 7] = [
        BackdropKind::Gradient,
        BackdropKind::Nebula,
        BackdropKind::Starfield,
        BackdropKind::Tunnel,
        BackdropKind::Fractal,
        BackdropKind::Plasma,
        BackdropKind::SynthGrid,
    ];
    pub fn label(self) -> &'static str {
        match self {
            BackdropKind::Gradient => "Gradient sky",
            BackdropKind::Nebula => "Nebula clouds",
            BackdropKind::Starfield => "Starfield",
            BackdropKind::Tunnel => "Raymarched tunnel",
            BackdropKind::Fractal => "Raymarched fractal",
            BackdropKind::Plasma => "Oldschool plasma",
            BackdropKind::SynthGrid => "Synthwave sun & grid",
        }
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
        }
    }
}

// ---------------------------------------------------------------------------
// Mirror floor

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MirrorFloor {
    /// Half size of the floor square.
    pub size: f32,
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

// ---------------------------------------------------------------------------
// Post-processing

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct PostStack {
    pub bloom: Bloom,
    pub kaleido: Kaleido,
    pub mirror: MirrorSplit,
    pub chroma: Chroma,
    pub pixelate: Pixelate,
    pub palette: PaletteFx,
    pub crt: Crt,
    pub grade: Grade,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bloom {
    pub enabled: bool,
    pub intensity: Param,
    pub threshold: Param,
    /// Spread (0..1).
    pub radius: Param,
}

impl Default for Bloom {
    fn default() -> Self {
        Bloom {
            enabled: true,
            intensity: Param::new(0.8),
            threshold: Param::new(0.8),
            radius: Param::new(0.7),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kaleido {
    pub enabled: bool,
    pub segments: u32,
    /// Base rotation in degrees.
    pub angle: Param,
    /// Whole rotations per loop.
    pub turns: i32,
    pub zoom: Param,
    pub center: [f32; 2],
}

impl Default for Kaleido {
    fn default() -> Self {
        Kaleido {
            enabled: false,
            segments: 6,
            angle: Param::new(0.0),
            turns: 0,
            zoom: Param::new(1.0),
            center: [0.5, 0.5],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MirrorSplitMode {
    #[default]
    LeftToRight,
    TopToBottom,
    Quad,
}

impl MirrorSplitMode {
    pub const ALL: [MirrorSplitMode; 3] = [
        MirrorSplitMode::LeftToRight,
        MirrorSplitMode::TopToBottom,
        MirrorSplitMode::Quad,
    ];
    pub fn label(self) -> &'static str {
        match self {
            MirrorSplitMode::LeftToRight => "Left → right",
            MirrorSplitMode::TopToBottom => "Top → bottom",
            MirrorSplitMode::Quad => "Quad",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MirrorSplit {
    pub enabled: bool,
    pub mode: MirrorSplitMode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chroma {
    pub enabled: bool,
    pub amount: Param,
}

impl Default for Chroma {
    fn default() -> Self {
        Chroma {
            enabled: false,
            amount: Param::new(0.004),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pixelate {
    pub enabled: bool,
    /// Size of a "fat pixel" in output pixels.
    pub size: Param,
}

impl Default for Pixelate {
    fn default() -> Self {
        Pixelate {
            enabled: false,
            size: Param::new(4.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaletteFx {
    pub enabled: bool,
    pub palette: PaletteId,
    /// Ordered dithering strength (0..1).
    pub dither: Param,
}

impl Default for PaletteFx {
    fn default() -> Self {
        PaletteFx {
            enabled: false,
            palette: PaletteId::Ega,
            dither: Param::new(0.6),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crt {
    pub enabled: bool,
    pub scanlines: Param,
    pub curvature: Param,
    /// Horizontal wobble / VHS noise.
    pub noise: Param,
}

impl Default for Crt {
    fn default() -> Self {
        Crt {
            enabled: false,
            scanlines: Param::new(0.5),
            curvature: Param::new(0.15),
            noise: Param::new(0.1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    pub exposure: Param,
    pub contrast: Param,
    pub saturation: Param,
    pub vignette: Param,
    pub grain: Param,
    /// White flash on every beat (0 = none).
    pub beat_flash: Param,
}

impl Default for Grade {
    fn default() -> Self {
        Grade {
            exposure: Param::new(1.0),
            contrast: Param::new(1.05),
            saturation: Param::new(1.1),
            vignette: Param::new(0.35),
            grain: Param::new(0.03),
            beat_flash: Param::new(0.0),
        }
    }
}

// ---------------------------------------------------------------------------
// User textures

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserTexture {
    pub name: String,
    pub path: String,
    /// Optional "retro-ize" processing applied on load.
    pub retro: Option<RetroProcess>,
}

impl Default for UserTexture {
    fn default() -> Self {
        UserTexture {
            name: "texture".into(),
            path: String::new(),
            retro: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RetroProcess {
    /// Downscale to this many pixels on the longest side (0 = keep).
    pub max_size: u32,
    pub palette: PaletteId,
    pub dither: f32,
}

impl Default for RetroProcess {
    fn default() -> Self {
        RetroProcess {
            max_size: 128,
            palette: PaletteId::Vga,
            dither: 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_project_roundtrip() {
        let p = Project::default();
        let back = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let p = Project::from_json(
            r#"{"name":"x","layers":[{"name":"a","kind":{"type":"Particles"}}]}"#,
        )
        .unwrap();
        assert_eq!(p.layers.len(), 1);
        assert!(matches!(p.layers[0].kind, LayerKind::Particles(_)));
        assert_eq!(p.timing, Timing::default());
    }
}

// ---------------------------------------------------------------------------
// Terrain (scrolling landscape)

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TerrainStyle {
    /// Glowing grid lines only.
    #[default]
    Wireframe,
    /// Lit solid ground.
    Solid,
    /// Solid ground with glowing grid lines.
    Both,
}

impl TerrainStyle {
    pub const ALL: [TerrainStyle; 3] = [
        TerrainStyle::Wireframe,
        TerrainStyle::Solid,
        TerrainStyle::Both,
    ];
    pub fn label(self) -> &'static str {
        match self {
            TerrainStyle::Wireframe => "Wireframe",
            TerrainStyle::Solid => "Solid",
            TerrainStyle::Both => "Solid + lines",
        }
    }
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
}

impl Default for Terrain {
    fn default() -> Self {
        Terrain {
            size: 60.0,
            cells: 64,
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
        }
    }
}

// ---------------------------------------------------------------------------
// Laser beams

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
        }
    }
}

// ---------------------------------------------------------------------------
// Neon ribbon

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RibbonCurve {
    /// 3D Lissajous figure (uses the three frequencies).
    #[default]
    Lissajous,
    /// Torus knot (uses the first two frequencies).
    Knot,
    /// Figure eight.
    Infinity,
    /// Circle that waves up and down (first frequency = waves).
    Wave,
    /// Flower / rose curve (first frequency = petals).
    Rose,
}

impl RibbonCurve {
    pub const ALL: [RibbonCurve; 5] = [
        RibbonCurve::Lissajous,
        RibbonCurve::Knot,
        RibbonCurve::Infinity,
        RibbonCurve::Wave,
        RibbonCurve::Rose,
    ];
    pub fn label(self) -> &'static str {
        match self {
            RibbonCurve::Lissajous => "Lissajous",
            RibbonCurve::Knot => "Knot",
            RibbonCurve::Infinity => "Figure eight",
            RibbonCurve::Wave => "Wavy ring",
            RibbonCurve::Rose => "Rose",
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
