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

/// A parameter that is 0 and not animated (e.g. a switched-off effect).
fn is_off(p: &Param) -> bool {
    p.base == 0.0 && !p.is_animated()
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
    /// Which part of the song the loop uses, MIDI notes and time warp.
    #[serde(skip_serializing_if = "is_default")]
    pub music: crate::music::MusicSettings,
    /// When `Some` and `use_graph` is set, layers come from the node graph.
    pub graph: Option<Graph>,
    pub use_graph: bool,
    /// Other scenes and the timeline playing them (inactive by default).
    #[serde(skip_serializing_if = "is_default")]
    pub sequence: crate::sequence::Sequence,
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
            music: Default::default(),
            graph: None,
            use_graph: false,
            sequence: Default::default(),
        }
    }
}

impl Project {
    /// Evaluation context at a loop phase (loop-window mode).
    pub fn ctx(&self, phase: f32, audio: Option<&crate::AudioEnvelope>) -> crate::EvalCtx {
        crate::EvalCtx::with_music(&self.timing, &self.music, phase, audio)
    }

    /// Evaluation context `seconds` after playback started: wraps around
    /// the loop, or runs through the song in full-track mode.
    pub fn ctx_at(&self, seconds: f64, audio: Option<&crate::AudioEnvelope>) -> crate::EvalCtx {
        match (self.music.mode, audio) {
            (crate::MusicMode::FullTrack, Some(a)) => {
                crate::EvalCtx::song(&self.timing, &self.music, seconds as f32, Some(a))
            }
            _ => self.ctx(self.timing.phase_at(seconds), audio),
        }
    }

    /// Length of one playback cycle: the loop, or the whole song in
    /// full-track mode.
    pub fn play_seconds(&self, audio: Option<&crate::AudioEnvelope>) -> f64 {
        match (self.music.mode, audio) {
            (crate::MusicMode::FullTrack, Some(a)) => a.duration.max(0.1) as f64,
            _ => self.timing.loop_seconds() as f64,
        }
    }

    /// Where the song should be playing `seconds` after playback started.
    pub fn song_seconds(&self, seconds: f64) -> f32 {
        match self.music.mode {
            crate::MusicMode::FullTrack => seconds as f32,
            crate::MusicMode::LoopWindow => {
                self.music.offset.max(0.0)
                    + self.timing.phase_at(seconds) * self.timing.loop_seconds()
            }
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("project serialises")
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Layers to render at `ctx`: either the plain layer list or the
    /// compiled graph (with its Drive nodes applied).
    pub fn scene_layers(&self, ctx: &crate::EvalCtx) -> Cow<'_, [Layer]> {
        let mut layers = match (&self.graph, self.use_graph) {
            (Some(g), true) => Cow::Owned(g.compile_at(ctx)),
            _ => Cow::Borrowed(&self.layers[..]),
        };
        link_terrains(&mut layers);
        layers
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
    /// Flies through the points of `path`.
    Path,
}

impl CameraMode {
    pub const ALL: [CameraMode; 4] = [
        CameraMode::Orbit,
        CameraMode::Pendulum,
        CameraMode::Static,
        CameraMode::Path,
    ];
    pub fn label(self) -> &'static str {
        match self {
            CameraMode::Orbit => "Orbit",
            CameraMode::Pendulum => "Pendulum",
            CameraMode::Static => "Static",
            CameraMode::Path => "Path",
        }
    }
}

/// One stop of a camera path.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathPoint {
    pub eye: [f32; 3],
    /// Where the camera looks.
    pub target: [f32; 3],
    /// Degrees.
    pub roll: f32,
    /// Vertical field of view in degrees.
    pub fov: f32,
}

impl Default for PathPoint {
    fn default() -> Self {
        PathPoint {
            eye: [0.0, 2.0, 10.0],
            target: [0.0, 0.0, 0.0],
            roll: 0.0,
            fov: 55.0,
        }
    }
}

/// A closed flight through points, travelled a whole number of times per
/// loop at an even speed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraPath {
    pub points: Vec<PathPoint>,
    /// Trips around the path per loop.
    pub laps: u32,
    /// 0 = constant speed, 1 = slow down and linger at every point.
    pub ease: f32,
    /// Jump to the next point on every hit of this kind (with music);
    /// without music the camera flies as usual.
    pub cut_on: Option<crate::audio::HitKind>,
    /// After a cut, how far the camera drifts towards the next point
    /// during one beat (0..1).
    pub drift: f32,
}

impl Default for CameraPath {
    fn default() -> Self {
        CameraPath {
            points: vec![
                PathPoint {
                    eye: [0.0, 2.0, 10.0],
                    ..Default::default()
                },
                PathPoint {
                    eye: [10.0, 4.0, 0.0],
                    ..Default::default()
                },
                PathPoint {
                    eye: [0.0, 1.0, -10.0],
                    ..Default::default()
                },
                PathPoint {
                    eye: [-10.0, 5.0, 0.0],
                    ..Default::default()
                },
            ],
            laps: 1,
            ease: 0.0,
            cut_on: None,
            drift: 0.2,
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
    /// Zoom in on every hit of `punch_on` (0 = none, 1 = strong).
    #[serde(skip_serializing_if = "is_default")]
    pub punch: f32,
    #[serde(skip_serializing_if = "is_default")]
    pub punch_on: crate::audio::HitKind,
    /// Points for the Path mode.
    #[serde(skip_serializing_if = "is_default")]
    pub path: CameraPath,
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
            punch: 0.0,
            punch_on: crate::audio::HitKind::Kick,
            path: CameraPath::default(),
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
    /// Mist that is thick near the ground and thins out higher up.
    #[serde(skip_serializing_if = "is_default")]
    pub height_fog: HeightFog,
    /// Rippling underwater light patterns on surfaces.
    #[serde(skip_serializing_if = "is_default")]
    pub caustics: Caustics,
    /// Rainbow opposite the sun (0 = none).
    #[serde(skip_serializing_if = "is_off")]
    pub rainbow: Param,
    /// The sun travels across the sky; night falls with stars and a moon.
    #[serde(skip_serializing_if = "is_default")]
    pub day_cycle: DayCycle,
    /// Shadows cast by the sun, and soft contact shadows on floors.
    #[serde(skip_serializing_if = "is_default")]
    pub shadows: Shadows,
}

/// Sun shadows (a shadow map around the camera's target) and contact
/// shadows under shapes standing on a mirror floor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shadows {
    pub enabled: bool,
    /// How dark shadows are (0..1).
    pub strength: f32,
    /// Blur of the shadow edge.
    pub softness: f32,
    /// Radius around the camera's target that gets shadows.
    pub distance: f32,
    /// Soft dark patches under shapes on a mirror floor (0 = none).
    pub contact: f32,
}

impl Default for Shadows {
    fn default() -> Self {
        Shadows {
            enabled: false,
            strength: 0.85,
            softness: 1.5,
            distance: 40.0,
            contact: 0.0,
        }
    }
}

/// Fog that pools in valleys: `density` at `height`, halving every
/// `falloff` × 0.7 units above it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeightFog {
    pub density: Param,
    pub height: f32,
    pub falloff: f32,
}

impl Default for HeightFog {
    fn default() -> Self {
        HeightFog {
            density: Param::new(0.0),
            height: 0.0,
            falloff: 2.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Caustics {
    pub amount: Param,
    /// Size of the pattern.
    pub scale: f32,
    /// Ripple cycles per loop.
    pub speed: i32,
    pub color: Rgb,
    /// Caustics only below this height (fading out just above it).
    pub below: f32,
}

impl Default for Caustics {
    fn default() -> Self {
        Caustics {
            amount: Param::new(0.0),
            scale: 1.0,
            speed: 1,
            color: hex(0x80d0ff),
            below: 100.0,
        }
    }
}

/// Day and night: the sun turns around the sky `cycles` times per loop.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DayCycle {
    pub enabled: bool,
    /// Whole days per loop.
    pub cycles: i32,
    /// Time of day at the start of the loop (0 midnight, 0.25 sunrise,
    /// 0.5 noon, 0.75 sunset).
    pub start: f32,
    /// Height of the sun at noon in degrees.
    pub noon_height: f32,
    pub sunset_color: Rgb,
    pub night_color: Rgb,
    pub moon_color: Rgb,
}

impl Default for DayCycle {
    fn default() -> Self {
        DayCycle {
            enabled: false,
            cycles: 1,
            start: 0.3,
            noon_height: 60.0,
            sunset_color: hex(0xff7a40),
            night_color: hex(0x060a1c),
            moon_color: hex(0x8098d0),
        }
    }
}

/// The environment's lighting at one moment (after the day cycle).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnvState {
    pub fog_color: Rgb,
    pub fog_density: f32,
    pub sky_color: Rgb,
    pub ground_color: Rgb,
    /// Unit vector towards the main light (the sun, or the moon at night).
    pub light_dir: [f32; 3],
    pub light_color: Rgb,
    pub light_intensity: f32,
    pub ambient: f32,
    /// Unit vector towards the sun (also below the horizon).
    pub sun_dir: [f32; 3],
    /// 0 in daylight .. 1 at night.
    pub night: f32,
    /// 0 .. 1 around sunrise and sunset.
    pub dusk: f32,
}

fn norm3(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l < 1e-6 {
        [0.0, 1.0, 0.0]
    } else {
        [v[0] / l, v[1] / l, v[2] / l]
    }
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Environment {
    /// Lighting at `ctx`, with the day cycle applied.
    pub fn eval(&self, ctx: &crate::EvalCtx) -> EnvState {
        use crate::color::lerp;
        let ld = norm3(self.light_dir);
        let mut st = EnvState {
            fog_color: self.fog_color,
            fog_density: self.fog_density.eval(ctx).max(0.0),
            sky_color: self.sky_color,
            ground_color: self.ground_color,
            light_dir: ld,
            light_color: self.light_color,
            light_intensity: self.light_intensity.eval(ctx),
            ambient: self.ambient.eval(ctx),
            sun_dir: ld,
            night: 0.0,
            dusk: 0.0,
        };
        let d = &self.day_cycle;
        if !d.enabled {
            return st;
        }
        // The sun rises opposite its noon side... on a tilted great circle
        // facing the light direction's azimuth. Whole days per loop.
        let t = (d.start + ctx.phase * d.cycles as f32).rem_euclid(1.0);
        let a = std::f32::consts::TAU * (t - 0.25);
        let mut south = [ld[0], 0.0, ld[2]];
        if south[0].abs() + south[2].abs() < 1e-4 {
            south = [0.0, 0.0, -1.0];
        }
        let south = norm3(south);
        let east = [-south[2], 0.0, south[0]];
        let noon = d.noon_height.clamp(5.0, 90.0).to_radians();
        let up = [south[0] * noon.cos(), noon.sin(), south[2] * noon.cos()];
        let sun = norm3([
            east[0] * a.cos() + up[0] * a.sin(),
            east[1] * a.cos() + up[1] * a.sin(),
            east[2] * a.cos() + up[2] * a.sin(),
        ]);
        let day = smooth(-0.12, 0.12, sun[1]);
        let dusk = (-(sun[1] / 0.18).powi(2)).exp();
        let night = 1.0 - day;
        let warm = smooth(0.0, 0.45, sun[1]);
        let sun_col = lerp(d.sunset_color, self.light_color, warm);
        st.sun_dir = sun;
        st.night = night;
        st.dusk = dusk;
        if sun[1] > -0.05 {
            st.light_dir = [sun[0], sun[1].max(0.02), sun[2]];
            st.light_dir = norm3(st.light_dir);
            st.light_color = sun_col;
            st.light_intensity *= smooth(-0.05, 0.1, sun[1]).max(0.15);
        } else {
            // Moonlight from the other side.
            st.light_dir = norm3([-sun[0], (-sun[1]).max(0.05), -sun[2]]);
            st.light_color = d.moon_color;
            st.light_intensity *= 0.5;
        }
        let tint = |c: Rgb| {
            let c = lerp(
                c,
                crate::color::scale(lerp(c, d.sunset_color, 0.6), 0.9),
                dusk * 0.8,
            );
            lerp(c, d.night_color, night * 0.92)
        };
        st.fog_color = tint(self.fog_color);
        st.sky_color = tint(self.sky_color);
        st.ground_color = lerp(
            self.ground_color,
            crate::color::scale(self.ground_color, 0.3),
            night,
        );
        st.ambient *= 1.0 - night * 0.55;
        st
    }
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
            height_fog: HeightFog::default(),
            caustics: Caustics::default(),
            rainbow: Param::new(0.0),
            day_cycle: DayCycle::default(),
            shadows: Shadows::default(),
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
            LayerKind::Weather(_) => "Weather",
            LayerKind::Falls(_) => "Waterfall",
            LayerKind::Text(_) => "Text",
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

/// Fill in the terrain of copies placed "on a terrain" from the terrain
/// layer they name.
fn link_terrains(layers: &mut Cow<'_, [Layer]>) {
    let wants = |l: &Layer| {
        matches!(&l.kind, LayerKind::Mesh(m) if matches!(&m.instancer,
            Instancer::OnTerrain { ground: None, terrain, .. } if !terrain.is_empty()))
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
        if let LayerKind::Mesh(m) = &mut l.kind {
            if let Instancer::OnTerrain {
                terrain, ground, ..
            } = &mut m.instancer
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

// ---------------------------------------------------------------------------
// Mesh layer

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MeshSource {
    Primitive(Primitive),
    /// A glTF/GLB or OBJ file.
    File {
        path: String,
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
            Instancer::Curve { .. } => "Along a curve",
            Instancer::Surface { .. } => "On a shape's surface",
            Instancer::OnTerrain { .. } => "On a terrain",
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
    /// Surface relief: bump / normal map / displacement.
    #[serde(skip_serializing_if = "is_default")]
    pub relief: Relief,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ReliefMode {
    /// Brightness of the texture is height.
    #[default]
    Bump,
    /// The texture is a (tangent-space) normal map.
    NormalMap,
}

impl ReliefMode {
    pub const ALL: [ReliefMode; 2] = [ReliefMode::Bump, ReliefMode::NormalMap];
    pub fn label(self) -> &'static str {
        match self {
            ReliefMode::Bump => "Bump (brightness = height)",
            ReliefMode::NormalMap => "Normal map",
        }
    }
}

/// Makes a surface look (bump / normal map) or be (displacement) uneven.
/// Uses the material's tiling and scrolling.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Relief {
    /// Relief texture (built-in or user image); `None` uses the colour texture.
    pub texture: Option<String>,
    pub mode: ReliefMode,
    /// Strength of the lighting relief (animatable). 0 = off.
    pub bump: Param,
    /// Moves the surface outwards by the texture brightness (animatable).
    /// Needs detailed geometry (Subdivide).
    pub displace: Param,
}

impl Default for Relief {
    fn default() -> Self {
        Relief {
            texture: None,
            mode: ReliefMode::Bump,
            bump: Param::new(0.0),
            displace: Param::new(0.0),
        }
    }
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
            relief: Relief::default(),
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
    /// A twisting funnel (tornado / water spout) with debris at its foot.
    Tornado,
}

impl Emitter {
    pub const ALL: [Emitter; 8] = [
        Emitter::Burst,
        Emitter::Sphere,
        Emitter::Ring,
        Emitter::Fountain,
        Emitter::Warp,
        Emitter::Vortex,
        Emitter::Snow,
        Emitter::Tornado,
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
            Emitter::Tornado => "Tornado",
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
    /// Smoke: particles cover what is behind them (and can be dark)
    /// instead of adding light. Brightness becomes opacity.
    #[serde(skip_serializing_if = "is_default")]
    pub smoke: bool,
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
    /// Flight through an infinite raymarched Menger sponge.
    Sponge,
    /// Flight through a corridor of glowing rings.
    Rings,
    /// Sky with raymarched volumetric clouds and a sun.
    Clouds,
    /// Night sky with rippling aurora curtains.
    Aurora,
}

impl BackdropKind {
    pub const ALL: [BackdropKind; 11] = [
        BackdropKind::Gradient,
        BackdropKind::Nebula,
        BackdropKind::Starfield,
        BackdropKind::Tunnel,
        BackdropKind::Fractal,
        BackdropKind::Plasma,
        BackdropKind::SynthGrid,
        BackdropKind::Sponge,
        BackdropKind::Rings,
        BackdropKind::Clouds,
        BackdropKind::Aurora,
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
            BackdropKind::Sponge => "Raymarched sponge flight",
            BackdropKind::Rings => "Raymarched ring corridor",
            BackdropKind::Clouds => "Volumetric clouds",
            BackdropKind::Aurora => "Aurora night sky",
        }
    }

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BgResolution {
    #[default]
    Full,
    Half,
    Quarter,
}

impl BgResolution {
    pub const ALL: [BgResolution; 3] = [
        BgResolution::Full,
        BgResolution::Half,
        BgResolution::Quarter,
    ];
    pub fn label(self) -> &'static str {
        match self {
            BgResolution::Full => "Full",
            BgResolution::Half => "Half (4× faster)",
            BgResolution::Quarter => "Quarter (16× faster)",
        }
    }
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
    /// Light shafts streaming from the sun, plus lens flare.
    #[serde(skip_serializing_if = "is_default")]
    pub rays: GodRays,
    /// Shimmering heat distortion.
    #[serde(skip_serializing_if = "is_default")]
    pub haze: HeatHaze,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HazeRegion {
    /// Above bright, hot things (lava, fire, the sun).
    #[default]
    HotSpots,
    /// Strongest at the bottom of the picture (hot ground).
    Ground,
    /// Everywhere.
    Everywhere,
}

impl HazeRegion {
    pub const ALL: [HazeRegion; 3] = [
        HazeRegion::HotSpots,
        HazeRegion::Ground,
        HazeRegion::Everywhere,
    ];
    pub fn label(self) -> &'static str {
        match self {
            HazeRegion::HotSpots => "Above hot spots",
            HazeRegion::Ground => "Near the ground",
            HazeRegion::Everywhere => "Everywhere",
        }
    }
}

/// Heat shimmer: the picture wobbles as if seen through rising hot air.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HeatHaze {
    pub enabled: bool,
    pub region: HazeRegion,
    pub amount: Param,
    /// Size of the ripples.
    pub scale: f32,
    /// Times the shimmer rises through the picture per loop.
    pub speed: i32,
}

impl Default for HeatHaze {
    fn default() -> Self {
        HeatHaze {
            enabled: false,
            region: HazeRegion::HotSpots,
            amount: Param::new(1.0),
            scale: 1.0,
            speed: 4,
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RaySource {
    /// From the sun (the light direction).
    #[default]
    Sun,
    /// From the middle of the picture (light at the end of a tunnel).
    Centre,
}

impl RaySource {
    pub const ALL: [RaySource; 2] = [RaySource::Sun, RaySource::Centre];
    pub fn label(self) -> &'static str {
        match self {
            RaySource::Sun => "Sun",
            RaySource::Centre => "Picture centre",
        }
    }
}

/// Screen-space light shafts ("god rays"): bright parts of the picture near
/// the light are smeared towards it, so anything dark in front casts
/// streaks of shadow through the haze.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GodRays {
    pub enabled: bool,
    pub source: RaySource,
    pub intensity: Param,
    /// Length of the shafts (0..1).
    pub length: Param,
    /// Brightness above which the picture casts rays.
    pub threshold: Param,
    pub tint: Rgb,
    /// Lens flare ghosts and halo (0 = none).
    pub flare: Param,
}

impl Default for GodRays {
    fn default() -> Self {
        GodRays {
            enabled: false,
            source: RaySource::Sun,
            intensity: Param::new(1.0),
            length: Param::new(0.7),
            threshold: Param::new(0.5),
            tint: [1.0, 0.95, 0.85],
            flare: Param::new(0.0),
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
    fn lod_grading_spans_the_terrain() {
        for (cells, drawn) in [(128u32, 64u32), (512, 256), (64, 64)] {
            for focus in [0.0f32, 0.2, 0.5, 0.93, 1.0] {
                let (k, s0) = lod_grading(cells, drawn, focus);
                let at = |i: f32| lod_position(i, cells, focus, k, s0);
                assert!(at(0.0).abs() < 1e-4, "{cells} {focus}: {}", at(0.0));
                assert!(
                    (at(drawn as f32) - 1.0).abs() < 1e-4,
                    "{}",
                    at(drawn as f32)
                );
                let mut last = -1.0;
                for i in 0..=drawn {
                    let t = at(i as f32);
                    assert!(t >= last, "not monotonic at {i}");
                    last = t;
                }
                // Full resolution next to the focus.
                let near = (at(s0 + 0.5) - at(s0 - 0.5)) * cells as f32;
                if focus > 0.01 && focus < 0.99 {
                    assert!((near - 1.0).abs() < 0.05, "{cells} {focus}: {near}");
                }
            }
        }
    }

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TerrainShape {
    /// Rolling hills.
    #[default]
    Hills,
    /// Sharp ridged mountains.
    Mountains,
    /// Flat-topped mesas in steps.
    Mesas,
    /// Wind-blown sand dunes.
    Dunes,
    /// Deep winding canyons in a plateau.
    Canyons,
    /// Round craters, like the moon.
    Craters,
}

impl TerrainShape {
    pub const ALL: [TerrainShape; 6] = [
        TerrainShape::Hills,
        TerrainShape::Mountains,
        TerrainShape::Mesas,
        TerrainShape::Dunes,
        TerrainShape::Canyons,
        TerrainShape::Craters,
    ];
    pub fn label(self) -> &'static str {
        match self {
            TerrainShape::Hills => "Hills",
            TerrainShape::Mountains => "Ridged mountains",
            TerrainShape::Mesas => "Mesas (terraces)",
            TerrainShape::Dunes => "Sand dunes",
            TerrainShape::Canyons => "Canyons",
            TerrainShape::Craters => "Craters",
        }
    }
    pub fn index(self) -> u32 {
        TerrainShape::ALL
            .iter()
            .position(|t| *t == self)
            .unwrap_or(0) as u32
    }
}

/// Height and slope based colouring of solid terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Biome {
    /// One ground colour.
    #[default]
    Plain,
    /// Grass, rock and snowy peaks.
    Alpine,
    /// Sand and red rock.
    Desert,
    /// Black basalt with glowing cracks.
    Volcanic,
    /// Snow and blue ice.
    Arctic,
    /// Purple moss and teal crystal.
    Alien,
}

impl Biome {
    pub const ALL: [Biome; 6] = [
        Biome::Plain,
        Biome::Alpine,
        Biome::Desert,
        Biome::Volcanic,
        Biome::Arctic,
        Biome::Alien,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Biome::Plain => "Plain (ground colour)",
            Biome::Alpine => "Alpine",
            Biome::Desert => "Desert",
            Biome::Volcanic => "Volcanic",
            Biome::Arctic => "Arctic",
            Biome::Alien => "Alien",
        }
    }
    pub fn index(self) -> u32 {
        Biome::ALL.iter().position(|t| *t == self).unwrap_or(0) as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LiquidKind {
    #[default]
    None,
    /// Reflective water with ripples, foam and a sun glint.
    Water,
    /// Glowing, slowly churning lava with a dark crust.
    Lava,
    /// Radioactive green goo with bubbles.
    Toxic,
    /// Frozen, cracked ice.
    Ice,
}

impl LiquidKind {
    pub const ALL: [LiquidKind; 5] = [
        LiquidKind::None,
        LiquidKind::Water,
        LiquidKind::Lava,
        LiquidKind::Toxic,
        LiquidKind::Ice,
    ];
    pub fn label(self) -> &'static str {
        match self {
            LiquidKind::None => "None",
            LiquidKind::Water => "Water",
            LiquidKind::Lava => "Lava",
            LiquidKind::Toxic => "Toxic goo",
            LiquidKind::Ice => "Ice",
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BeamStyle {
    #[default]
    Laser,
    Spotlight,
}

impl BeamStyle {
    pub const ALL: [BeamStyle; 2] = [BeamStyle::Laser, BeamStyle::Spotlight];
    pub fn label(self) -> &'static str {
        match self {
            BeamStyle::Laser => "Laser beams",
            BeamStyle::Spotlight => "Spotlight cones",
        }
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

    /// Point at `t` (0..1 around the closed curve), fitting a unit sphere.
    pub fn point(self, freq: [u32; 3], t: f32) -> [f32; 3] {
        use std::f32::consts::{PI, TAU};
        let [a, b, c] = freq.map(|f| f.clamp(1, 16) as f32);
        let x = t * TAU;
        match self {
            RibbonCurve::Lissajous => [
                (a * x + 0.5 * PI).sin(),
                (b * x).sin() * 0.6,
                (c * x + 0.25 * PI).sin(),
            ],
            RibbonCurve::Knot => {
                let rr = 0.62 + 0.28 * (b * x).cos();
                [rr * (a * x).cos(), 0.28 * (b * x).sin(), rr * (a * x).sin()]
            }
            RibbonCurve::Infinity => [x.sin(), 0.15 * (a * x).sin(), x.sin() * x.cos()],
            RibbonCurve::Wave => [x.cos(), 0.3 * (a * x).sin(), x.sin()],
            RibbonCurve::Rose => {
                let rr = (a * x).cos();
                [rr * x.cos(), 0.1 * (b * x).sin(), rr * x.sin()]
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TextFont {
    /// Chunky pixel letters, demoscene style.
    #[default]
    Pixel,
    /// Clean monospaced letters (Hack).
    Mono,
    /// Light rounded letters (Ubuntu Light).
    Sans,
}

impl TextFont {
    pub const ALL: [TextFont; 3] = [TextFont::Pixel, TextFont::Mono, TextFont::Sans];
    pub fn label(self) -> &'static str {
        match self {
            TextFont::Pixel => "Pixel",
            TextFont::Mono => "Mono",
            TextFont::Sans => "Sans",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextStyle {
    /// Still text, centred.
    #[default]
    Static,
    /// Runs across a window a whole number of times per loop.
    Scroller,
    /// A scroller whose letters ride a sine wave.
    SineScroller,
    /// Letters appear one by one on the beat.
    Typewriter,
    /// One line at a time, a new line every few beats.
    Greetings,
}

impl TextStyle {
    pub const ALL: [TextStyle; 5] = [
        TextStyle::Static,
        TextStyle::Scroller,
        TextStyle::SineScroller,
        TextStyle::Typewriter,
        TextStyle::Greetings,
    ];
    pub fn label(self) -> &'static str {
        match self {
            TextStyle::Static => "Still",
            TextStyle::Scroller => "Scroller",
            TextStyle::SineScroller => "Sine scroller",
            TextStyle::Typewriter => "Typewriter",
            TextStyle::Greetings => "Greetings list",
        }
    }
}

/// Glowing letters in the scene (a flat sign facing +z; place and turn it
/// like any layer).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextLayer {
    /// One line per line for Greetings; Scrollers run it as one line.
    pub text: String,
    pub font: TextFont,
    /// A TTF/OTF file used instead of `font`.
    pub font_file: Option<String>,
    pub style: TextStyle,
    /// Letter height in world units.
    pub size: f32,
    /// Extra space between letters (fraction of the size).
    pub spacing: f32,
    /// Width of the scroller window in world units.
    pub width: f32,
    /// Times the scroller runs past per loop (negative = the other way).
    pub speed: i32,
    /// Sine scroller: wave height (fraction of the size).
    pub wave: Param,
    /// Letters per wave.
    pub wavelength: f32,
    /// Times the wave rolls per loop.
    pub wave_cycles: i32,
    /// Typewriter: letters per beat.
    pub letters_per_beat: u32,
    /// Greetings: beats per line.
    pub beats_per_line: u32,
    /// Colour at the top and bottom of the letters.
    pub color_top: Rgb,
    pub color_bottom: Rgb,
    pub glow: Param,
    /// Outline width (0 = none, 1 = thick).
    pub outline: f32,
    pub outline_color: Rgb,
    /// Drop shadow strength.
    pub shadow: f32,
    /// Chrome: shiny bevelled letters reflecting the sky.
    pub chrome: f32,
    /// Always turn the text towards the camera.
    pub face_camera: bool,
}

impl Default for TextLayer {
    fn default() -> Self {
        TextLayer {
            text: "EZ2DEMOSCENE".into(),
            font: TextFont::Pixel,
            font_file: None,
            style: TextStyle::Static,
            size: 1.0,
            spacing: 0.0,
            width: 12.0,
            speed: 1,
            wave: Param::new(0.4),
            wavelength: 8.0,
            wave_cycles: 2,
            letters_per_beat: 2,
            beats_per_line: 4,
            color_top: hex(0xffffff),
            color_bottom: hex(0xff2bd6),
            glow: Param::new(1.0),
            outline: 0.0,
            outline_color: hex(0x000000),
            shadow: 0.0,
            chrome: 0.0,
            face_camera: false,
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

// ---------------------------------------------------------------------------
// Weather

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Precipitation {
    /// Streaks of rain with splashes on the ground.
    #[default]
    Rain,
    /// Slowly swaying snowflakes.
    Snow,
    /// Glowing embers rising from the ground.
    Embers,
    /// A sandstorm: dust blown sideways.
    Dust,
    /// Fireflies wandering and blinking.
    Fireflies,
    /// No particles (lightning only).
    None,
}

impl Precipitation {
    pub const ALL: [Precipitation; 6] = [
        Precipitation::Rain,
        Precipitation::Snow,
        Precipitation::Embers,
        Precipitation::Dust,
        Precipitation::Fireflies,
        Precipitation::None,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Precipitation::Rain => "Rain",
            Precipitation::Snow => "Snow",
            Precipitation::Embers => "Rising embers",
            Precipitation::Dust => "Sandstorm",
            Precipitation::Fireflies => "Fireflies",
            Precipitation::None => "None (lightning only)",
        }
    }
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

// ---------------------------------------------------------------------------
// Waterfall

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FallKind {
    #[default]
    Water,
    Lava,
    Toxic,
}

impl FallKind {
    pub const ALL: [FallKind; 3] = [FallKind::Water, FallKind::Lava, FallKind::Toxic];
    pub fn label(self) -> &'static str {
        match self {
            FallKind::Water => "Water",
            FallKind::Lava => "Lava",
            FallKind::Toxic => "Toxic goo",
        }
    }
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
        }
    }
}

#[cfg(test)]
mod env_tests {
    use super::*;

    #[test]
    fn day_cycle_loops_and_has_night() {
        let mut e = Environment::default();
        e.day_cycle.enabled = true;
        e.day_cycle.cycles = 2;
        let a = e.eval(&crate::EvalCtx::at(0.0));
        let b = e.eval(&crate::EvalCtx::at(1.0));
        assert!((a.sun_dir[1] - b.sun_dir[1]).abs() < 1e-4);
        let ys: Vec<f32> = (0..100)
            .map(|i| e.eval(&crate::EvalCtx::at(i as f32 / 100.0)).sun_dir[1])
            .collect();
        assert!(ys.iter().any(|y| *y > 0.5), "no noon");
        assert!(ys.iter().any(|y| *y < -0.5), "no midnight");
        // Without the cycle nothing changes.
        let plain = Environment::default().eval(&crate::EvalCtx::at(0.4));
        assert_eq!(plain.night, 0.0);
        assert_eq!(plain.fog_color, Environment::default().fog_color);
    }
}
