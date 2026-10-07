//! Camera, lights, fog, shadows, reflections, colour scheme and day cycle.

use super::*;

labeled_enum! {
    /// How the camera moves over the loop.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum CameraMode {
        /// Circles the target `orbit_turns` times per loop.
        #[default]
        Orbit => "Orbit",
        /// Swings back and forth by `swing` degrees.
        Pendulum => "Pendulum",
        /// Fixed position; distance/height params can still breathe.
        Static => "Static",
        /// Flies through the points of `path`.
        Path => "Path",
    }
}

/// One stop of a camera path.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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

/// The camera: how it moves around its target (`mode`), how far and how high, and its lens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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

/// Lighting and atmosphere: the sun, ambient and sky colours, fog, shadows, reflections and light shafts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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
    /// Where reflections and ambient light come from: the colours above,
    /// or an environment map (a built-in studio, a photo of a real place,
    /// the scene's own sky).
    #[serde(skip_serializing_if = "is_default")]
    pub env_light: EnvLight,
    /// Shiny things reflecting the scene around them (screen-space
    /// reflections).
    #[serde(skip_serializing_if = "is_default")]
    pub reflections: Reflections,
    /// Sunbeams in the fog, cut by the sun's shadows.
    #[serde(skip_serializing_if = "is_default")]
    pub shafts: LightShafts,
    /// The sun and ambient light flicker by a Quake light style.
    #[serde(skip_serializing_if = "is_default")]
    pub light_style: crate::retro::LightStyle,
}

/// Light shafts: the fog lit by the sun where the sun reaches it, so
/// shadows cut dark bands through it. Uses the sun shadow map.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LightShafts {
    pub enabled: bool,
    pub strength: Param,
    /// How much the light scatters forward, towards someone looking at
    /// the sun (0 = the same all round, 0.9 = a tight glow around it).
    pub scattering: f32,
    /// Samples along each view ray (quality).
    pub steps: u32,
    /// How far along the view the fog is sampled, in world units.
    pub reach: f32,
}

impl Default for LightShafts {
    fn default() -> Self {
        LightShafts {
            enabled: false,
            strength: Param::new(1.0),
            scattering: 0.6,
            steps: 32,
            reach: 60.0,
        }
    }
}

/// Screen-space reflections: shiny shapes, water and wet ground reflect
/// what is on the screen (the environment elsewhere). The mirror floor
/// keeps its own exact reflection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Reflections {
    pub enabled: bool,
    /// How much of the environment reflection the scene replaces (0..1).
    pub strength: Param,
    /// How far a reflection reaches, in world units.
    pub max_distance: f32,
    /// Rougher surfaces than this keep the environment reflection.
    pub roughness_cutoff: f32,
}

impl Default for Reflections {
    fn default() -> Self {
        Reflections {
            enabled: false,
            strength: Param::new(1.0),
            max_distance: 12.0,
            roughness_cutoff: 0.6,
        }
    }
}

labeled_enum! {
    /// Built-in environment maps (generated, no files).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
    pub enum Studio {
        /// A dark studio with big soft lights: crisp highlights on shiny things.
        #[default]
        Softbox => "Softbox studio",
        /// A bright, even grey sky.
        Overcast => "Overcast",
        /// A low sun over warm clouds and a blue sky.
        Sunset => "Sunset",
        /// A dark room with magenta and cyan neon tubes.
        NeonRoom => "Neon room",
    }
}

/// Where the environment light comes from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum EnvSource {
    /// The sky and ground colours (and fake studio bands), as always.
    #[default]
    Colours,
    Studio(Studio),
    /// A panorama (`.hdr` Radiance file): light from a photo of a real
    /// place.
    Hdri(String),
    /// The scene's own background, captured all around.
    Sky,
}

/// Lighting from an environment map (image-based lighting).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct EnvLight {
    pub source: EnvSource,
    /// Turn around the vertical, in degrees (a saw of ±180° turns it a
    /// whole turn per cycle).
    pub rotation: Param,
    pub intensity: Param,
    /// Point the sun (and its shadows) at the brightest spot of the map,
    /// and take that spot out of the map so it isn't counted twice.
    pub sun_from_map: bool,
    /// From the sky: capture it once instead of every frame (for skies
    /// that don't move).
    pub sky_static: bool,
}

impl Default for EnvLight {
    fn default() -> Self {
        EnvLight {
            source: EnvSource::Colours,
            rotation: Param::new(0.0),
            intensity: Param::new(1.0),
            sun_from_map: false,
            sky_static: false,
        }
    }
}

impl EnvLight {
    /// Whether a map lights the scene (not just the colours).
    pub fn is_on(&self) -> bool {
        self.source != EnvSource::Colours
    }
}

labeled_enum! {
    /// How a colour scheme's hues sit around its key colour.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
    pub enum Harmony {
        /// The key hue only.
        Mono => "One hue",
        /// The key and its neighbours (30° either side).
        Analogous => "Neighbours",
        /// The key and the opposite hue.
        #[default]
        Complementary => "Opposites",
        /// The key and the two hues beside its opposite.
        Split => "Split opposites",
        /// Three hues evenly around the wheel.
        Triadic => "Triad",
        /// Four hues: two opposite pairs.
        Tetradic => "Square",
    }
}

impl Harmony {
    /// Hues of the scheme relative to the key, in degrees.
    pub fn offsets(self) -> &'static [f32] {
        match self {
            Harmony::Mono => &[0.0],
            Harmony::Analogous => &[-30.0, 0.0, 30.0],
            Harmony::Complementary => &[0.0, 180.0],
            Harmony::Split => &[0.0, 150.0, 210.0],
            Harmony::Triadic => &[0.0, 120.0, 240.0],
            Harmony::Tetradic => &[0.0, 90.0, 180.0, 270.0],
        }
    }
}

/// A colour scheme: every colour setting keeps its lightness while its
/// hue is pulled toward the scheme's hues, worked out when drawing (the
/// stored colours never change). Pictures (textures, sprites) keep their
/// own colours.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ColorScheme {
    pub enabled: bool,
    /// The colour everything harmonises with.
    pub key: Rgb,
    /// Degrees added to the key's hue (animate it to turn the scheme).
    pub key_turn: Param,
    pub harmony: Harmony,
    /// 0 = colours unchanged, 1 = every hue on the scheme.
    pub hue_pull: f32,
    /// 0 = colours keep their saturation, 1 = all take the key's.
    pub chroma_match: f32,
    /// The sky, fog, sun and god rays too.
    pub environment: bool,
    /// Greys take the key's hue too (0 = they stay grey). A new shape's
    /// default material is grey. Missing in older projects = 0, as they
    /// looked.
    #[serde(default)]
    pub tint_greys: f32,
}

impl Default for ColorScheme {
    fn default() -> Self {
        ColorScheme {
            enabled: false,
            key: hex(0xff2bd6),
            key_turn: Param::new(0.0),
            harmony: Harmony::Complementary,
            hue_pull: 1.0,
            chroma_match: 0.0,
            environment: true,
            tint_greys: 0.6,
        }
    }
}

impl ColorScheme {
    /// The scheme at this moment, or `None` when it is off.
    pub fn harmoniser(&self, ctx: &crate::EvalCtx) -> Option<crate::color::Harmoniser> {
        self.enabled.then(|| {
            let offsets: Vec<f32> = self
                .harmony
                .offsets()
                .iter()
                .map(|d| d.to_radians())
                .collect();
            crate::color::Harmoniser::new(
                self.key,
                self.key_turn.eval(ctx).to_radians(),
                &offsets,
                self.hue_pull,
                self.chroma_match,
            )
            .with_grey_tint(self.tint_greys)
        })
    }
}

impl Environment {
    /// Every colour setting of the environment.
    pub fn for_each_color_mut(&mut self, mut f: impl FnMut(&mut Rgb)) {
        f(&mut self.fog_color);
        f(&mut self.sky_color);
        f(&mut self.ground_color);
        f(&mut self.light_color);
        f(&mut self.caustics.color);
        f(&mut self.day_cycle.sunset_color);
        f(&mut self.day_cycle.night_color);
        f(&mut self.day_cycle.moon_color);
    }
}

/// Sun shadows (a shadow map around the camera's target) and contact
/// shadows under shapes standing on a mirror floor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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

/// Rippling light patterns (as under water) cast on the scene.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
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
            light_intensity: self.light_intensity.eval(ctx) * self.light_style.eval(ctx.phase),
            ambient: self.ambient.eval(ctx) * self.light_style.eval(ctx.phase),
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
            env_light: EnvLight::default(),
            reflections: Reflections::default(),
            shafts: LightShafts::default(),
            light_style: Default::default(),
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

#[cfg(test)]
mod color_scheme_tests {
    use super::*;
    use crate::color::to_oklch;

    fn layers() -> Vec<Layer> {
        [
            LayerKind::Mesh(MeshLayer::default()),
            LayerKind::Particles(ParticleLayer::default()),
            LayerKind::Backdrop(Backdrop::default()),
            LayerKind::Mirror(MirrorFloor::default()),
            LayerKind::Terrain(Terrain::default()),
            LayerKind::Lasers(Lasers::default()),
            LayerKind::Ribbon(Ribbon::default()),
            LayerKind::Weather(Weather::default()),
            LayerKind::Falls(Falls::default()),
            LayerKind::Text(TextLayer::default()),
            LayerKind::Sprite(SpriteLayer::default()),
            LayerKind::Arcs(ArcLayer::default()),
            LayerKind::Logo(LogoLayer::default()),
            LayerKind::Splat(SplatLayer::default()),
        ]
        .into_iter()
        .map(|k| Layer::new("L", k))
        .collect()
    }

    /// The visitor reaches real, distinct colour fields: a colour written
    /// through it shows up that many times in the saved layer.
    #[test]
    fn visitor_reaches_every_color_once() {
        let mark = [0.123_456_78f32, 0.234_567_8, 0.345_678_9];
        for mut l in layers() {
            let mut n = 0;
            l.kind.for_each_color_mut(|c| {
                *c = mark;
                n += 1;
            });
            let json = serde_json::to_string(&l).unwrap();
            let found = json.matches("0.12345678").count();
            assert!(n > 0, "{}: no colours", l.type_label());
            assert_eq!(found, n, "{}: {n} visited, {found} saved", l.type_label());
        }
        let mut env = Environment::default();
        let mut n = 0;
        env.for_each_color_mut(|_| n += 1);
        assert_eq!(n, 8);
    }

    #[test]
    fn scheme_recolours_only_when_on_and_not_kept_layers() {
        let mut p = Project {
            layers: layers(),
            ..Default::default()
        };
        let ctx = p.ctx(0.3, None);
        // Off: the very same layers.
        assert!(matches!(p.scene_layers(&ctx), Cow::Borrowed(_)));
        p.color_scheme.enabled = true;
        p.color_scheme.key = crate::color::hex(0x2060ff);
        p.color_scheme.harmony = Harmony::Mono;
        p.layers[0].keep_colors = true;
        let drawn = p.scene_layers(&ctx);
        assert_eq!(drawn[0], p.layers[0], "a kept layer changed");
        let mut changed = 0;
        for (a, b) in p.layers.iter().zip(drawn.iter()).skip(1) {
            let (mut ca, mut cb) = (a.clone(), b.clone());
            let mut va = Vec::new();
            let mut vb = Vec::new();
            ca.kind.for_each_color_mut(|c| va.push(*c));
            cb.kind.for_each_color_mut(|c| vb.push(*c));
            for (x, y) in va.iter().zip(&vb) {
                // Lightness kept, colourful ones moved.
                assert!((to_oklch(*x)[0] - to_oklch(*y)[0]).abs() < 5e-3);
                if x != y {
                    changed += 1;
                }
            }
        }
        assert!(changed > 5, "only {changed} colours changed");
        // The environment follows unless told not to.
        assert_ne!(*p.scene_environment(&ctx), p.environment);
        p.color_scheme.environment = false;
        assert!(matches!(p.scene_environment(&ctx), Cow::Borrowed(_)));
    }
}
