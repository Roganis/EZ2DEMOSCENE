//! Mesh layers: shapes, deformation and copies (instancers).

use super::*;

/// Shapes: one shape (`source`) with its material, optional deformation and copies (`instancer`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct MeshLayer {
    pub source: MeshSource,
    pub material: Material,
    pub instancer: Instancer,
    pub variation: Variation,
    /// Extra geometry detail: each level splits every triangle in four
    /// (needed for smooth displacement).
    #[serde(skip_serializing_if = "is_default")]
    pub subdivide: u32,
    /// Twist, bend, taper, wobble and explode the shape.
    #[serde(skip_serializing_if = "is_default")]
    pub deform: Deform,
    /// Colours spread across the copies (and cycling through them).
    #[serde(skip_serializing_if = "is_default")]
    pub ramp: ColorRamp,
    /// Melt into another shape (raymarched while on).
    #[serde(skip_serializing_if = "is_default")]
    pub morph: ShapeMorph,
}

/// A liquid morph from the layer's shape into another. While it is on, both
/// shapes are turned into distance fields and the layer is raymarched as a
/// blend of the two (smooth, with holes opening and closing); off, the
/// shape is drawn as the usual sharp mesh.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ShapeMorph {
    pub enabled: bool,
    /// The shape it melts into.
    pub target: MeshSource,
    /// 0 = the layer's shape, 1 = the target (animatable).
    pub amount: Param,
}

impl Default for ShapeMorph {
    fn default() -> Self {
        ShapeMorph {
            enabled: false,
            target: MeshSource::Primitive(Primitive::Sphere { detail: 3 }),
            amount: Param::new(0.5),
        }
    }
}

impl Default for MeshLayer {
    fn default() -> Self {
        MeshLayer {
            source: MeshSource::Primitive(Primitive::Cube),
            material: Material::default(),
            instancer: Instancer::Single,
            variation: Variation::default(),
            subdivide: 0,
            deform: Deform::default(),
            ramp: ColorRamp::default(),
            morph: ShapeMorph::default(),
        }
    }
}

/// How a colour ramp spreads over the copies: smoothly or in steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub enum RampMode {
    /// Smooth blend from colour to colour (back to the first at the end).
    #[default]
    Gradient,
    /// Each copy takes one of the colours, in turn.
    Steps,
}

/// Colours across the copies of a shape: copy 0 at the start of the ramp,
/// the last copy near its end. It can cycle along the copies a whole
/// number of times per loop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ColorRamp {
    pub enabled: bool,
    /// 2 to 4 colours.
    pub colors: Vec<Rgb>,
    pub mode: RampMode,
    /// Times the colours travel along all the copies per loop.
    pub cycles: i32,
    /// Also colour the glow (keeping its strength).
    pub glow: bool,
}

impl Default for ColorRamp {
    fn default() -> Self {
        ColorRamp {
            enabled: false,
            colors: vec![hex(0xff2bd6), hex(0x00e5ff), hex(0xffd000)],
            mode: RampMode::Gradient,
            cycles: 1,
            glow: true,
        }
    }
}

/// Shape deformations, applied on the GPU to every copy (in the shape's
/// own space, where it fits in a unit sphere; y is "up the shape").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Deform {
    /// Turns of twist from the bottom to the top.
    pub twist: Param,
    /// Bend from the bottom to the top, in degrees.
    pub bend: Param,
    /// Top wider (+) or narrower (−) than the bottom.
    pub taper: Param,
    /// Bumpy wobble along the surface.
    pub noise: Param,
    /// Size of the wobble bumps (higher = smaller bumps).
    pub noise_scale: f32,
    /// Times the wobble flows around per loop.
    pub noise_speed: i32,
    /// Faces fly apart (clearest with flat shading).
    pub explode: Param,
}

impl Default for Deform {
    fn default() -> Self {
        Deform {
            twist: Param::new(0.0),
            bend: Param::new(0.0),
            taper: Param::new(0.0),
            noise: Param::new(0.0),
            noise_scale: 2.0,
            noise_speed: 1,
            explode: Param::new(0.0),
        }
    }
}

impl Deform {
    pub fn is_active(&self) -> bool {
        [
            &self.twist,
            &self.bend,
            &self.taper,
            &self.noise,
            &self.explode,
        ]
        .iter()
        .any(|p| p.base != 0.0 || p.is_animated())
    }

    /// How far outside its unit sphere the deformed shape can reach, as a
    /// radius multiplier (for culling), at `ctx`.
    pub fn reach(&self, ctx: &crate::EvalCtx) -> f32 {
        let taper = self.taper.eval(ctx).abs();
        let bend = self.bend.eval(ctx).abs().to_radians();
        1.0 + taper + bend * 0.5 + self.noise.eval(ctx).abs() + self.explode.eval(ctx).abs() * 1.5
    }
}

/// Where a shape comes from. The `type` field picks the source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum MeshSource {
    /// A built-in shape.
    Primitive(Primitive),
    /// A glTF/GLB or OBJ file.
    File { path: String },
    /// A model of the bundled library (see [`crate::models`]).
    Library { id: String },
    /// A raymarched distance-field object: drawn inside its box (it fits
    /// the unit sphere like the built-in shapes), so it intersects other
    /// shapes, casts and takes shadows and gets the usual material.
    Sdf {
        form: SdfShape,
        /// Whole cycles of its built-in motion per loop (0 = still).
        cycles: i32,
    },
    /// 3D letters: the text extruded (one line per line).
    Text {
        text: String,
        font: TextFont,
        /// A TTF/OTF file used instead of `font`.
        font_file: Option<String>,
        /// Thickness, in letter heights.
        depth: f32,
    },
    /// A sheet of cloth (a flag, curtain, banner or drape) moved by a
    /// simulation baked into a loop (see [`crate::sim::Cloth`]).
    Cloth {
        cloth: Box<crate::sim::Cloth>,
        /// The renderer's mesh of the sheet at the moment being drawn,
        /// filled in before rendering; none until the cloth is baked.
        #[serde(skip)]
        mesh: Option<String>,
    },
}

/// Raymarched distance-field shapes (see `sdf.wgsl`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind")]
pub enum SdfShape {
    /// Balls that melt into each other as they orbit.
    Metaballs { balls: u32, blend: f32 },
    /// A ball carved into a gyroid lattice that flows through it.
    Gyroid { scale: f32, thickness: f32 },
    /// The Mandelbulb fractal; its power breathes when it moves.
    Bulb { power: f32 },
    /// A rounded box with a ball passing through it, melted together.
    SoftBox { blend: f32, round: f32 },
}

impl SdfShape {
    pub fn all_defaults() -> [SdfShape; 4] {
        [
            SdfShape::Metaballs {
                balls: 5,
                blend: 0.35,
            },
            SdfShape::Gyroid {
                scale: 8.0,
                thickness: 0.08,
            },
            SdfShape::Bulb { power: 8.0 },
            SdfShape::SoftBox {
                blend: 0.25,
                round: 0.08,
            },
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            SdfShape::Metaballs { .. } => "Metaballs",
            SdfShape::Gyroid { .. } => "Gyroid",
            SdfShape::Bulb { .. } => "Fractal bulb",
            SdfShape::SoftBox { .. } => "Melting box",
        }
    }

    pub fn index(&self) -> u32 {
        match self {
            SdfShape::Metaballs { .. } => 0,
            SdfShape::Gyroid { .. } => 1,
            SdfShape::Bulb { .. } => 2,
            SdfShape::SoftBox { .. } => 3,
        }
    }

    /// Settings as the shader reads them.
    pub fn params(&self) -> [f32; 3] {
        match *self {
            SdfShape::Metaballs { balls, blend } => [balls.clamp(1, 8) as f32, blend.max(0.0), 0.0],
            SdfShape::Gyroid { scale, thickness } => [scale.max(0.5), thickness.max(0.005), 0.0],
            SdfShape::Bulb { power } => [power.clamp(2.0, 16.0), 0.0, 0.0],
            SdfShape::SoftBox { blend, round } => [blend.max(0.0), round.clamp(0.0, 0.5), 0.0],
        }
    }
}

/// Built-in procedural meshes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// The lower half of a sphere as a shell, open at the top (holds a
    /// liquid layer's Bowl at the same place and size).
    Bowl {
        thickness: f32,
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
            Primitive::Bowl { thickness: 0.06 },
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
            Primitive::Bowl { .. } => "Bowl",
        }
    }

    /// Stable cache key for generated geometry.
    pub fn cache_key(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// How copies of the mesh are laid out inside the layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Instancer {
    Single,
    /// A huge swarm (up to 250,000 copies), computed on the graphics card
    /// where it can (desktop, WebGPU): every copy is a function of its
    /// number, so the fallback on the CPU gives the same picture.
    Swarm {
        form: SwarmForm,
        count: u32,
        radius: f32,
        spread: f32,
        /// Whole turns per loop.
        speed: i32,
        seed: u32,
    },
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
    /// Copies travelling along a closed curve (the neon ribbon's shapes).
    Curve {
        curve: RibbonCurve,
        freq: [u32; 3],
        /// Size of the curve (1 = a ribbon layer of the same scale).
        size: f32,
        count: u32,
        /// Whole trips around the curve per loop (0 = still).
        laps: i32,
        /// Turn each copy to face along the curve.
        align: bool,
    },
    /// Copies scattered over the surface of a shape (evenly by area).
    Surface {
        shape: MeshSource,
        /// Size of that shape (1 = a shape layer of scale 1).
        size: f32,
        count: u32,
        seed: u32,
        /// Stand each copy up along the surface.
        align: bool,
        /// Push copies out from the surface.
        lift: f32,
    },
    /// Copies standing on a terrain layer (by name), riding along as it
    /// scrolls.
    OnTerrain {
        /// Name of the terrain layer.
        terrain: String,
        count: u32,
        seed: u32,
        /// Tilt copies with the slope.
        align: bool,
        /// Lift copies off the ground.
        lift: f32,
        /// The terrain and its placement, filled in before rendering.
        #[serde(skip)]
        ground: Option<Box<(Terrain, Transform)>>,
    },
    /// A flock: copies that fly together, simulated ahead of time into a
    /// loop (see [`crate::sim::Flock`]).
    Flock {
        flock: Box<crate::sim::Flock>,
        /// Where every copy is at the moment being drawn (from the bake),
        /// filled in before rendering; none until the flock is baked.
        #[serde(skip)]
        placed: Option<std::sync::Arc<Vec<glam::Mat4>>>,
    },
    /// Rigid bodies: copies that fall, stack, collide and get blown apart,
    /// simulated ahead of time into a loop (see [`crate::sim::Physics`]).
    Physics {
        physics: Box<crate::sim::Physics>,
        /// Where every copy is at the moment being drawn (from the bake),
        /// filled in before rendering; none until it is baked.
        #[serde(skip)]
        placed: Option<std::sync::Arc<Vec<glam::Mat4>>>,
    },
    /// A liquid: copies are its droplets, sloshing in a container,
    /// simulated ahead of time into a loop (see [`crate::sim::Fluid`]).
    Fluid {
        fluid: Box<crate::sim::Fluid>,
        /// Where every droplet is at the moment being drawn (from the
        /// bake), filled in before rendering; none until it is baked.
        #[serde(skip)]
        placed: Option<std::sync::Arc<Vec<glam::Mat4>>>,
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
            Instancer::Swarm { .. } => "Big swarm",
            Instancer::Wall { .. } => "Wall",
            Instancer::Spiral { .. } => "Spiral",
            Instancer::Curve { .. } => "Along a curve",
            Instancer::Surface { .. } => "On a shape's surface",
            Instancer::OnTerrain { .. } => "On a terrain",
            Instancer::Flock { .. } => "Flock",
            Instancer::Physics { .. } => "Physics",
            Instancer::Fluid { .. } => "Liquid",
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
            Instancer::Swarm {
                form: SwarmForm::Orbit,
                count: 20_000,
                radius: 5.0,
                spread: 2.0,
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
            Instancer::Curve {
                curve: RibbonCurve::Knot,
                freq: [2, 3, 5],
                size: 4.0,
                count: 24,
                laps: 1,
                align: true,
            },
            Instancer::Surface {
                shape: MeshSource::Primitive(Primitive::Sphere { detail: 3 }),
                size: 3.0,
                count: 80,
                seed: 1,
                align: true,
                lift: 0.0,
            },
            Instancer::OnTerrain {
                terrain: String::new(),
                count: 60,
                seed: 1,
                align: false,
                lift: 0.0,
                ground: None,
            },
            Instancer::Flock {
                flock: Box::default(),
                placed: None,
            },
            Instancer::Physics {
                physics: Box::default(),
                placed: None,
            },
            Instancer::Fluid {
                fluid: Box::default(),
                placed: None,
            },
        ]
    }
}

/// Per-instance randomness and travelling waves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// Equalizer: copy i grows upwards and glows with frequency band i of
    /// the music (low notes first).
    #[serde(skip_serializing_if = "is_default")]
    pub spectrum: f32,
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
            spectrum: 0.0,
        }
    }
}

/// Most copies in a swarm.
pub const SWARM_MAX: u32 = 250_000;

labeled_enum! {
    /// Layouts of a big swarm.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum SwarmForm {
        /// Each copy on its own tilted circle (like Orbit).
        #[default]
        Orbit => "Orbits",
        /// Filling a ball that turns, each copy bobbing.
        Cloud => "Cloud",
        /// On the surface of a ball that turns.
        Shell => "Shell",
        /// A spiral galaxy: three arms, the middle turning faster.
        Galaxy => "Galaxy",
    }
}

impl SwarmForm {
    pub fn index(self) -> u32 {
        match self {
            SwarmForm::Orbit => 0,
            SwarmForm::Cloud => 1,
            SwarmForm::Shell => 2,
            SwarmForm::Galaxy => 3,
        }
    }
}
