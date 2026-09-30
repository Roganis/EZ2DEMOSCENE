//! The project / scene description. This is what gets saved as `.ez2.json`.

use crate::clock::Timing;
use crate::color::{hex, Rgb};
use crate::graph::Graph;
use crate::palette::PaletteId;
use crate::param::Param;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

mod backdrop;
mod environment;
mod lasers;
mod layer;
mod logo;
mod material;
mod mesh;
mod particles;
mod post;
mod ribbon;
mod sprite;
mod terrain;
mod text;
mod textures;
mod weather;

pub use backdrop::*;
pub use environment::*;
pub use lasers::*;
pub use layer::*;
pub use logo::*;
pub use material::*;
pub use mesh::*;
pub use particles::*;
pub use post::*;
pub use ribbon::*;
pub use sprite::*;
pub use terrain::*;
pub use text::*;
pub use textures::*;
pub use weather::*;

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
    /// Every colour brought into harmony with one key colour (off by
    /// default).
    #[serde(skip_serializing_if = "is_default")]
    pub color_scheme: ColorScheme,
    /// The quirks of 5th-generation consoles for the whole 3D scene:
    /// chunky low resolution, wobbly vertices, warped textures.
    #[serde(skip_serializing_if = "is_default")]
    pub retro: crate::retro::Retro3d,
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
            color_scheme: ColorScheme::default(),
            retro: Default::default(),
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
        // The colour scheme, worked out on copies (the stored colours stay).
        if let Some(h) = self.color_scheme.harmoniser(ctx) {
            for l in layers.to_mut().iter_mut().filter(|l| !l.keep_colors) {
                l.kind.for_each_color_mut(|c| *c = h.apply(*c));
            }
        }
        layers
    }

    /// The environment as drawn: in the colour scheme when it covers it.
    pub fn scene_environment(&self, ctx: &crate::EvalCtx) -> Cow<'_, Environment> {
        match self.environment_harmoniser(ctx) {
            Some(h) => {
                let mut env = self.environment.clone();
                env.for_each_color_mut(|c| *c = h.apply(*c));
                Cow::Owned(env)
            }
            None => Cow::Borrowed(&self.environment),
        }
    }

    /// A colour of the environment or the post effects as drawn.
    pub fn scene_color(&self, c: Rgb, ctx: &crate::EvalCtx) -> Rgb {
        match self.environment_harmoniser(ctx) {
            Some(h) => h.apply(c),
            None => c,
        }
    }

    fn environment_harmoniser(&self, ctx: &crate::EvalCtx) -> Option<crate::color::Harmoniser> {
        if self.color_scheme.environment {
            self.color_scheme.harmoniser(ctx)
        } else {
            None
        }
    }

    pub fn find_texture(&self, name: &str) -> Option<&UserTexture> {
        self.textures.iter().find(|t| t.name == name)
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
