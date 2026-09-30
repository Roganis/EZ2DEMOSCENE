//! Layers: the common fields (transform, blink, symmetry) and every layer kind.

use super::*;

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
    /// Left out of the project's colour scheme (fire stays orange).
    #[serde(skip_serializing_if = "is_default")]
    pub keep_colors: bool,
    /// Animate on steps: motion held at this many frames per second
    /// (0 = smooth), snapped to a whole number of steps per loop.
    #[serde(skip_serializing_if = "is_default")]
    pub step_fps: f32,
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
            keep_colors: false,
            step_fps: 0.0,
            kind: LayerKind::Mesh(MeshLayer::default()),
        }
    }
}

labeled_enum! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub enum BlinkMode {
        /// Always visible.
        #[default]
        Off => "Off",
        /// On/off in a regular rhythm.
        Blink => "Blink",
        /// On/off at random moments.
        Random => "Random blink",
        /// Glow flashes and fades (the layer stays visible).
        Flash => "Flash",
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

/// Whole steps per loop for motion held at `fps` in a loop of
/// `loop_seconds` (`None`: smooth).
pub fn step_count(fps: f32, loop_seconds: f32) -> Option<u32> {
    (fps > 0.0).then(|| ((fps * loop_seconds).round() as u32).max(1))
}

impl Layer {
    /// The context its motion is evaluated at: held on whole steps when
    /// the layer animates on steps.
    pub fn motion_ctx(&self, ctx: &crate::EvalCtx) -> Option<crate::EvalCtx> {
        step_count(self.step_fps, ctx.loop_seconds()).map(|n| ctx.stepped(n))
    }

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
            LayerKind::Weather(_) => "Weather",
            LayerKind::Falls(_) => "Waterfall",
            LayerKind::Text(_) => "Text",
            LayerKind::Sprite(_) => "Sprites",
            LayerKind::Arcs(_) => "Electric arcs",
            LayerKind::Logo(_) => "Logo",
            LayerKind::Mode7(_) => "Mode 7 floor",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)] // a scene has a handful of layers
pub enum LayerKind {
    Mesh(MeshLayer),
    Particles(ParticleLayer),
    Backdrop(Backdrop),
    Mirror(MirrorFloor),
    Terrain(Terrain),
    Lasers(Lasers),
    Ribbon(Ribbon),
    Weather(Weather),
    Falls(Falls),
    Text(TextLayer),
    Sprite(SpriteLayer),
    Arcs(ArcLayer),
    Logo(LogoLayer),
    Mode7(Mode7Floor),
}

/// A SNES "Mode 7" / Saturn VDP2 floor: an endless flat picture at the
/// layer's height, drawn per pixel up to a hard horizon. It turns around
/// the layer's position and scrolls, whole turns and tiles per loop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mode7Floor {
    /// Built-in or your picture (`None`: checker).
    pub texture: Option<String>,
    /// World units per tile of the picture.
    pub tile_size: f32,
    /// Whole turns per loop around the layer's position.
    pub turns: i32,
    /// Tiles scrolled per loop (x, z), before turning.
    pub scroll: [i32; 2],
    pub tint: Rgb,
    /// Brightness (animatable).
    pub brightness: Param,
    /// Square texture pixels (the console look).
    pub pixelated: bool,
    /// Fade into the fog colour in the distance (off: the hard, bright
    /// SNES horizon).
    pub fog: bool,
}

impl Default for Mode7Floor {
    fn default() -> Self {
        Mode7Floor {
            texture: None,
            tile_size: 4.0,
            turns: 1,
            scroll: [0, 2],
            tint: [1.0, 1.0, 1.0],
            brightness: Param::new(1.0),
            pixelated: true,
            fog: false,
        }
    }
}

impl LayerKind {
    /// The copies of a shape or sprite layer.
    pub fn instancer_mut(&mut self) -> Option<&mut Instancer> {
        match self {
            LayerKind::Mesh(m) => Some(&mut m.instancer),
            LayerKind::Sprite(s) => Some(&mut s.instancer),
            _ => None,
        }
    }

    /// Every colour setting of the layer (pictures aside).
    pub fn for_each_color_mut(&mut self, mut f: impl FnMut(&mut Rgb)) {
        match self {
            LayerKind::Mesh(m) => {
                f(&mut m.material.base_color);
                f(&mut m.material.emissive_color);
                f(&mut m.material.pbr.sheen_color);
                m.ramp.colors.iter_mut().for_each(&mut f);
            }
            LayerKind::Particles(p) => {
                f(&mut p.color_a);
                f(&mut p.color_b);
            }
            LayerKind::Backdrop(b) => {
                f(&mut b.color_a);
                f(&mut b.color_b);
                f(&mut b.color_c);
            }
            LayerKind::Mirror(m) => {
                f(&mut m.base_color);
                f(&mut m.tint);
                f(&mut m.grid_color);
            }
            LayerKind::Terrain(t) => {
                f(&mut t.line_color);
                f(&mut t.fill_color);
                f(&mut t.liquid.color);
            }
            LayerKind::Lasers(z) => {
                f(&mut z.color_a);
                f(&mut z.color_b);
            }
            LayerKind::Ribbon(r) => f(&mut r.color),
            LayerKind::Weather(w) => {
                f(&mut w.color);
                f(&mut w.lightning.color);
            }
            LayerKind::Falls(fl) => f(&mut fl.color),
            LayerKind::Text(t) => {
                f(&mut t.color_top);
                f(&mut t.color_bottom);
                f(&mut t.outline_color);
            }
            LayerKind::Sprite(sp) => f(&mut sp.tint),
            LayerKind::Arcs(a) => f(&mut a.color),
            LayerKind::Mode7(m) => f(&mut m.tint),
            LayerKind::Logo(g) => {
                for c in [
                    &mut g.tint,
                    &mut g.color_top,
                    &mut g.color_bottom,
                    &mut g.outline_color,
                    &mut g.light_color,
                    &mut g.glint_color,
                    &mut g.contour_color,
                    &mut g.stack_color_a,
                    &mut g.stack_color_b,
                    &mut g.extrude_color,
                    &mut g.burn_color,
                    &mut g.copper_a,
                    &mut g.copper_b,
                    &mut g.glass_tint,
                    &mut g.rays_tint,
                ] {
                    f(c);
                }
            }
        }
    }

    pub fn instancer(&self) -> Option<&Instancer> {
        match self {
            LayerKind::Mesh(m) => Some(&m.instancer),
            LayerKind::Sprite(s) => Some(&s.instancer),
            _ => None,
        }
    }
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
    /// Tilt around the world's z axis in degrees (animatable: rock a bowl
    /// and the liquid in it sloshes).
    #[serde(skip_serializing_if = "is_no_tilt")]
    pub tilt: Param,
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

fn is_no_tilt(p: &Param) -> bool {
    *p == Param::new(0.0)
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
            tilt: Param::new(0.0),
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

/// Fill in the terrain of copies placed "on a terrain" from the terrain
/// layer they name.
pub(super) fn link_terrains(layers: &mut Cow<'_, [Layer]>) {
    let wants = |l: &Layer| {
        matches!(l.kind.instancer(),
            Some(Instancer::OnTerrain { ground: None, terrain, .. }) if !terrain.is_empty())
    };
    if !layers.iter().any(wants) {
        return;
    }
    let terrains: Vec<(String, Terrain, Transform)> = layers
        .iter()
        .filter_map(|l| match &l.kind {
            LayerKind::Terrain(t) => Some((l.name.clone(), t.clone(), l.transform.clone())),
            _ => None,
        })
        .collect();
    for l in layers.to_mut().iter_mut() {
        {
            if let Some(Instancer::OnTerrain {
                terrain, ground, ..
            }) = l.kind.instancer_mut()
            {
                if ground.is_none() {
                    *ground = terrains
                        .iter()
                        .find(|(n, _, _)| n == terrain)
                        .map(|(_, t, tr)| Box::new((t.clone(), tr.clone())));
                }
            }
        }
    }
}
