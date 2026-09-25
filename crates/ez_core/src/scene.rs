//! The project / scene description. This is what gets saved as `.ez2.json`.

use crate::clock::Timing;
use crate::color::{hex, Rgb};
use crate::graph::Graph;
use crate::palette::PaletteId;
use crate::param::Param;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

pub const PROJECT_VERSION: u32 = 1;

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
    pub angle: f32,
    pub orbit_turns: i32,
    /// Pendulum amplitude in degrees.
    pub swing: f32,
    /// Vertical field of view in degrees.
    pub fov: Param,
    /// Roll in degrees.
    pub roll: Param,
    /// Camera shake on every beat (0 = none).
    pub beat_shake: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            mode: CameraMode::Orbit,
            target: [0.0, 0.0, 0.0],
            distance: Param::new(10.0),
            height: Param::new(3.0),
            angle: 0.0,
            orbit_turns: 1,
            swing: 30.0,
            fov: Param::new(55.0),
            roll: Param::new(0.0),
            beat_shake: 0.0,
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
    pub light_intensity: f32,
    pub ambient: f32,
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
            light_intensity: 1.5,
            ambient: 0.3,
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
    pub kind: LayerKind,
}

impl Default for Layer {
    fn default() -> Self {
        Layer {
            name: "Layer".into(),
            enabled: true,
            transform: Transform::default(),
            symmetry: Symmetry::None,
            kind: LayerKind::Mesh(MeshLayer::default()),
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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transform {
    pub position: [f32; 3],
    /// Euler rotation in degrees (applied Y, X, Z).
    pub rotation: [f32; 3],
    /// Uniform scale (animatable).
    pub scale: Param,
    /// Per-axis stretch multiplied with `scale`.
    pub stretch: [f32; 3],
    /// Whole turns per loop around X, Y, Z.
    pub spin: [i32; 3],
    /// Vertical offset (animatable, e.g. bobbing).
    pub bob: Param,
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
    File { path: String },
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
    Sphere { detail: u32 },
    Torus { thickness: f32, segments: u32 },
    Cylinder { segments: u32 },
    /// Tall faceted spike.
    Shard { seed: u32 },
    /// Cluster of spikes growing from a base.
    Crystal { spikes: u32, seed: u32 },
    /// Beveled slab (UVs make "edges"/"stripes" emissive modes look like light strips).
    Panel { bevel: f32 },
    /// Curved band segment (neon rings, arena walls).
    Ring { arc: f32, width: f32, height: f32, segments: u32 },
    /// Flat square in XZ.
    Plane,
    /// Pyramid with a square base.
    Pyramid,
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
        ]
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
    Radial { count: u32, radius: f32 },
    /// Random points inside a sphere (or on its shell).
    Scatter { count: u32, radius: f32, shell: bool, seed: u32 },
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
    pub metallic: f32,
    pub roughness: f32,
    pub emissive_color: Rgb,
    /// Glow strength (animatable: pulse it on the beat!).
    pub emissive: Param,
    pub emissive_mode: EmissiveMode,
    /// Built-in texture name or a user texture name.
    pub texture: Option<String>,
    pub texture_scale: f32,
    /// Texture tiles scrolled per loop (U, V).
    pub scroll: [i32; 2],
    /// Nearest-neighbour texture sampling for chunky pixels.
    pub pixelated: bool,
    /// Faceted look (normals from the triangle faces).
    pub flat_shading: bool,
    /// Rim / fresnel light strength.
    pub rim: f32,
    /// Hue rotation over the loop (animatable, in turns).
    pub hue_shift: Param,
}

impl Default for Material {
    fn default() -> Self {
        Material {
            base_color: hex(0xb0b0b8),
            metallic: 0.2,
            roughness: 0.4,
            emissive_color: hex(0xff3020),
            emissive: Param::new(0.0),
            emissive_mode: EmissiveMode::Full,
            texture: None,
            texture_scale: 1.0,
            scroll: [0, 0],
            pixelated: false,
            flat_shading: false,
            rim: 0.3,
            hue_shift: Param::new(0.0),
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
    pub speed: f32,
    /// Emitter size / spread.
    pub radius: f32,
    pub color_a: Rgb,
    pub color_b: Rgb,
    pub intensity: Param,
    /// Ghost copies behind each particle (0 = none).
    pub trail: u32,
    pub trail_spacing: f32,
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
            speed: 1.0,
            radius: 4.0,
            color_a: hex(0xffd080),
            color_b: hex(0xff3010),
            intensity: Param::new(2.0),
            trail: 0,
            trail_spacing: 0.01,
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
        BackdropKind::ALL.iter().position(|e| *e == self).unwrap_or(0) as u32
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
    pub detail: f32,
    /// Texture used by the tunnel walls.
    pub texture: Option<String>,
}

impl Default for Backdrop {
    fn default() -> Self {
        Backdrop {
            kind: BackdropKind::Gradient,
            color_a: hex(0x05050a),
            color_b: hex(0x302040),
            color_c: hex(0xff4060),
            speed: 1,
            intensity: Param::new(1.0),
            detail: 1.0,
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
    pub reflectivity: f32,
    /// Blur of the reflection (0..1).
    pub blur: f32,
    pub tint: Rgb,
    pub texture: Option<String>,
    pub texture_scale: f32,
    /// Glowing grid line intensity (0 = off).
    pub grid: Param,
    pub grid_color: Rgb,
    pub grid_scale: f32,
    /// Grid scroll (cells per loop).
    pub grid_scroll: i32,
}

impl Default for MirrorFloor {
    fn default() -> Self {
        MirrorFloor {
            size: 40.0,
            base_color: hex(0x080808),
            reflectivity: 0.6,
            blur: 0.2,
            tint: [1.0, 1.0, 1.0],
            texture: None,
            texture_scale: 1.0,
            grid: Param::new(0.0),
            grid_color: hex(0xff2040),
            grid_scale: 1.0,
            grid_scroll: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Post-processing

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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

impl Default for PostStack {
    fn default() -> Self {
        PostStack {
            bloom: Bloom::default(),
            kaleido: Kaleido::default(),
            mirror: MirrorSplit::default(),
            chroma: Chroma::default(),
            pixelate: Pixelate::default(),
            palette: PaletteFx::default(),
            crt: Crt::default(),
            grade: Grade::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bloom {
    pub enabled: bool,
    pub intensity: Param,
    pub threshold: f32,
    /// Spread (0..1).
    pub radius: f32,
}

impl Default for Bloom {
    fn default() -> Self {
        Bloom {
            enabled: true,
            intensity: Param::new(0.8),
            threshold: 0.8,
            radius: 0.7,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kaleido {
    pub enabled: bool,
    pub segments: u32,
    /// Base rotation in degrees.
    pub angle: f32,
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
            angle: 0.0,
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
    pub size: f32,
}

impl Default for Pixelate {
    fn default() -> Self {
        Pixelate {
            enabled: false,
            size: 4.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaletteFx {
    pub enabled: bool,
    pub palette: PaletteId,
    /// Ordered dithering strength (0..1).
    pub dither: f32,
}

impl Default for PaletteFx {
    fn default() -> Self {
        PaletteFx {
            enabled: false,
            palette: PaletteId::Ega,
            dither: 0.6,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crt {
    pub enabled: bool,
    pub scanlines: f32,
    pub curvature: f32,
    /// Horizontal wobble / VHS noise.
    pub noise: f32,
}

impl Default for Crt {
    fn default() -> Self {
        Crt {
            enabled: false,
            scanlines: 0.5,
            curvature: 0.15,
            noise: 0.1,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    pub exposure: Param,
    pub contrast: f32,
    pub saturation: f32,
    pub vignette: f32,
    pub grain: f32,
    /// White flash on every beat (0 = none).
    pub beat_flash: f32,
}

impl Default for Grade {
    fn default() -> Self {
        Grade {
            exposure: Param::new(1.0),
            contrast: 1.05,
            saturation: 1.1,
            vignette: 0.35,
            grain: 0.03,
            beat_flash: 0.0,
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
        let p = Project::from_json(r#"{"name":"x","layers":[{"name":"a","kind":{"type":"Particles"}}]}"#)
            .unwrap();
        assert_eq!(p.layers.len(), 1);
        assert!(matches!(p.layers[0].kind, LayerKind::Particles(_)));
        assert_eq!(p.timing, Timing::default());
    }
}
