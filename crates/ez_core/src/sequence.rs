//! Several scenes on one timeline.
//!
//! A project's own layers, camera, light and effects are the scene being
//! edited ("the current scene"); other scenes wait in
//! [`Sequence::scenes`]. Switching scenes swaps their contents, so every
//! editor panel keeps working on "the project". The clips play scenes one
//! after another; the whole sequence is the loop, and each scene keeps its
//! own loop inside its clip.

use crate::clock::EvalCtx;
use crate::graph::Graph;
use crate::scene::*;
use serde::{Deserialize, Serialize};

/// A scene waiting in the sequence (the current one lives in the project).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scene {
    pub id: u32,
    pub name: String,
    /// The scene's own loop, in beats.
    pub loop_beats: u32,
    pub camera: Camera,
    pub environment: Environment,
    pub layers: Vec<Layer>,
    pub post: PostStack,
    pub graph: Option<Graph>,
    pub use_graph: bool,
}

impl Default for Scene {
    fn default() -> Self {
        Scene {
            id: 1,
            name: "Scene".into(),
            loop_beats: 16,
            camera: Camera::default(),
            environment: Environment::default(),
            layers: Vec::new(),
            post: PostStack::default(),
            graph: None,
            use_graph: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TransitionKind {
    /// Straight cut.
    #[default]
    Cut,
    Crossfade,
    /// A soft edge sweeping across at an angle.
    Wipe,
    /// A circle opening from the middle.
    Iris,
    /// Flash to white and back.
    Flash,
    /// Blocks of the next scene glitch in.
    Glitch,
    /// Cut on the first kick of the transition (with music).
    CutOnKick,
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 7] = [
        TransitionKind::Cut,
        TransitionKind::Crossfade,
        TransitionKind::Wipe,
        TransitionKind::Iris,
        TransitionKind::Flash,
        TransitionKind::Glitch,
        TransitionKind::CutOnKick,
    ];
    pub fn label(self) -> &'static str {
        match self {
            TransitionKind::Cut => "Cut",
            TransitionKind::Crossfade => "Crossfade",
            TransitionKind::Wipe => "Wipe",
            TransitionKind::Iris => "Iris",
            TransitionKind::Flash => "Flash",
            TransitionKind::Glitch => "Glitch",
            TransitionKind::CutOnKick => "Cut on kick",
        }
    }
    /// Index used by the compositing shader.
    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0) as u32
    }
}

/// How a clip comes in (over the start of the clip).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Transition {
    pub kind: TransitionKind,
    /// Length in beats.
    pub beats: f32,
    /// Wipe direction in degrees.
    pub angle: f32,
}

impl Default for Transition {
    fn default() -> Self {
        Transition {
            kind: TransitionKind::Crossfade,
            beats: 2.0,
            angle: 0.0,
        }
    }
}

/// One stretch of the timeline showing one scene.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    /// Scene id.
    pub scene: u32,
    pub beats: u32,
    pub transition: Transition,
}

impl Default for Clip {
    fn default() -> Self {
        Clip {
            scene: 0,
            beats: 16,
            transition: Transition::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sequence {
    /// Id and name of the scene held by the project itself.
    pub scene_id: u32,
    pub scene_name: String,
    /// That scene's own loop, in beats.
    pub scene_beats: u32,
    /// The other scenes.
    pub scenes: Vec<Scene>,
    /// Empty = no sequence: the project is one looping scene.
    pub clips: Vec<Clip>,
}

impl Default for Sequence {
    fn default() -> Self {
        Sequence {
            scene_id: 0,
            scene_name: "Main".into(),
            scene_beats: 16,
            scenes: Vec::new(),
            clips: Vec::new(),
        }
    }
}

/// What the timeline shows at a moment: a scene, and during a transition
/// the scene it comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeqFrame {
    pub scene: u32,
    pub ctx: EvalCtx,
    /// (previous scene, its moment, progress 0..1, transition)
    pub from: Option<(u32, EvalCtx, f32, Transition)>,
}

impl Sequence {
    pub fn is_active(&self) -> bool {
        !self.clips.is_empty()
    }

    pub fn total_beats(&self) -> u32 {
        self.clips
            .iter()
            .map(|c| c.beats.max(1))
            .sum::<u32>()
            .max(1)
    }

    /// Own loop length of a scene.
    pub fn scene_beats(&self, id: u32) -> u32 {
        if id == self.scene_id {
            return self.scene_beats.max(1);
        }
        self.scenes
            .iter()
            .find(|s| s.id == id)
            .map_or(16, |s| s.loop_beats.max(1))
    }

    pub fn scene_name(&self, id: u32) -> String {
        if id == self.scene_id {
            return self.scene_name.clone();
        }
        self.scenes
            .iter()
            .find(|s| s.id == id)
            .map_or_else(|| "?".into(), |s| s.name.clone())
    }

    /// Ids of every scene, the current one first.
    pub fn scene_ids(&self) -> Vec<u32> {
        std::iter::once(self.scene_id)
            .chain(self.scenes.iter().map(|s| s.id))
            .collect()
    }

    pub fn next_id(&self) -> u32 {
        self.scene_ids().into_iter().max().unwrap_or(0) + 1
    }

    /// Where the timeline is at `ctx` (a moment of the whole sequence).
    pub fn frame_at(&self, ctx: &EvalCtx) -> Option<SeqFrame> {
        if self.clips.is_empty() {
            return None;
        }
        let total = self.total_beats() as f32;
        let beat = ctx.beat_phase.rem_euclid(1.0) * total;
        let motion = ctx.phase.rem_euclid(1.0) * total;
        // The clip playing now.
        let mut start = 0.0;
        let mut k = 0;
        for (i, c) in self.clips.iter().enumerate() {
            let len = c.beats.max(1) as f32;
            if beat < start + len || i + 1 == self.clips.len() {
                k = i;
                break;
            }
            start += len;
        }
        let clip = self.clips[k];
        // A scene's own moment, `offset` beats into its clip.
        let local = |scene: u32, beat_offset: f32, motion_offset: f32| {
            let sb = self.scene_beats(scene) as f32;
            let mut c = *ctx;
            c.loop_beats = sb as u32;
            c.beat_phase = (beat_offset / sb).rem_euclid(1.0);
            c.phase = (motion_offset / sb).rem_euclid(1.0);
            c
        };
        let into = beat - start;
        let scene_ctx = local(clip.scene, into, motion - start);
        let tr = clip.transition;
        let len = tr.beats.clamp(0.0, clip.beats.max(1) as f32);
        let mut from = None;
        if self.clips.len() > 1 && tr.kind != TransitionKind::Cut && into < len {
            let prev = self.clips[(k + self.clips.len() - 1) % self.clips.len()];
            let prev_len = prev.beats.max(1) as f32;
            let progress = match tr.kind {
                TransitionKind::CutOnKick => {
                    let h = &ctx.music.hits[crate::audio::HitKind::Kick as usize];
                    let seconds_in = into * ctx.beat_seconds;
                    if ctx.music.active && h.since <= seconds_in {
                        1.0
                    } else {
                        0.0
                    }
                }
                _ => (into / len.max(1e-3)).clamp(0.0, 1.0),
            };
            if progress < 1.0 {
                from = Some((
                    prev.scene,
                    local(prev.scene, prev_len + into, prev_len + (motion - start)),
                    progress,
                    tr,
                ));
            }
        }
        Some(SeqFrame {
            scene: clip.scene,
            ctx: scene_ctx,
            from,
        })
    }
}

impl Project {
    /// The project restricted to one scene of its sequence (with that
    /// scene's own loop), ready to render; `None` for an unknown id.
    pub fn scene_view(&self, id: u32) -> Option<Project> {
        let seq = &self.sequence;
        let mut p = Project {
            sequence: Sequence::default(),
            ..self.clone_without_sequence()
        };
        if id != seq.scene_id {
            let s = seq.scenes.iter().find(|s| s.id == id)?;
            p.layers = s.layers.clone();
            p.camera = s.camera.clone();
            p.environment = s.environment.clone();
            p.post = s.post.clone();
            p.graph = s.graph.clone();
            p.use_graph = s.use_graph;
        }
        p.timing.loop_beats = seq.scene_beats(id);
        Some(p)
    }

    fn clone_without_sequence(&self) -> Project {
        Project {
            version: self.version,
            name: self.name.clone(),
            timing: self.timing,
            camera: self.camera.clone(),
            environment: self.environment.clone(),
            layers: self.layers.clone(),
            post: self.post.clone(),
            textures: self.textures.clone(),
            audio: self.audio.clone(),
            music: self.music.clone(),
            graph: self.graph.clone(),
            use_graph: self.use_graph,
            sequence: Sequence::default(),
        }
    }

    /// True when a scene uses feedback trails, whose history needs one loop
    /// of warm-up before an export's first frame.
    pub fn uses_feedback(&self) -> bool {
        self.post.feedback.enabled || self.sequence.scenes.iter().any(|s| s.post.feedback.enabled)
    }

    /// Keep the loop as long as the sequence.
    pub fn sync_sequence_length(&mut self) {
        if self.sequence.is_active() {
            self.timing.loop_beats = self.sequence.total_beats();
        }
    }

    /// Start a sequence with the project as its first scene and clip.
    pub fn start_sequence(&mut self) {
        if self.sequence.is_active() {
            return;
        }
        let beats = self.timing.loop_beats.max(1);
        self.sequence.scene_beats = beats;
        self.sequence.clips = vec![Clip {
            scene: self.sequence.scene_id,
            beats,
            transition: Transition {
                kind: TransitionKind::Cut,
                ..Default::default()
            },
        }];
        self.sync_sequence_length();
    }

    /// Turn the sequence off: the current scene becomes the whole project
    /// (other scenes are kept for later).
    pub fn stop_sequence(&mut self) {
        if self.sequence.is_active() {
            self.timing.loop_beats = self.sequence.scene_beats.max(1);
            self.sequence.clips.clear();
        }
    }

    /// Add a scene (a copy of the current one, or empty) and return its id.
    pub fn add_scene(&mut self, copy: bool) -> u32 {
        let id = self.sequence.next_id();
        let scene = if copy {
            Scene {
                id,
                name: format!("{} copy", self.sequence.scene_name),
                loop_beats: self.sequence.scene_beats,
                camera: self.camera.clone(),
                environment: self.environment.clone(),
                layers: self.layers.clone(),
                post: self.post.clone(),
                graph: self.graph.clone(),
                use_graph: self.use_graph,
            }
        } else {
            let empty = crate::presets::empty();
            Scene {
                id,
                name: format!("Scene {id}"),
                loop_beats: self.sequence.scene_beats.max(1),
                camera: empty.camera,
                environment: empty.environment,
                layers: empty.layers,
                post: empty.post,
                graph: None,
                use_graph: false,
            }
        };
        self.sequence.scenes.push(scene);
        id
    }

    /// Make scene `id` the one the editor works on (swaps contents).
    pub fn edit_scene(&mut self, id: u32) -> bool {
        let Some(i) = self.sequence.scenes.iter().position(|s| s.id == id) else {
            return false;
        };
        let seq = &mut self.sequence;
        let s = &mut seq.scenes[i];
        std::mem::swap(&mut s.id, &mut seq.scene_id);
        std::mem::swap(&mut s.name, &mut seq.scene_name);
        std::mem::swap(&mut s.loop_beats, &mut seq.scene_beats);
        std::mem::swap(&mut s.layers, &mut self.layers);
        std::mem::swap(&mut s.camera, &mut self.camera);
        std::mem::swap(&mut s.environment, &mut self.environment);
        std::mem::swap(&mut s.post, &mut self.post);
        std::mem::swap(&mut s.graph, &mut self.graph);
        std::mem::swap(&mut s.use_graph, &mut self.use_graph);
        true
    }

    /// Remove a scene that isn't the current one, and its clips.
    pub fn remove_scene(&mut self, id: u32) {
        self.sequence.scenes.retain(|s| s.id != id);
        self.sequence.clips.retain(|c| c.scene != id);
        self.sync_sequence_length();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_scene_project() -> Project {
        let mut p = crate::presets::orbiting_solid();
        p.start_sequence();
        let b = p.add_scene(false);
        p.sequence.clips.push(Clip {
            scene: b,
            beats: 8,
            transition: Transition {
                kind: TransitionKind::Crossfade,
                beats: 2.0,
                angle: 0.0,
            },
        });
        p.sequence.clips[0].transition = Transition {
            kind: TransitionKind::Wipe,
            beats: 4.0,
            angle: 30.0,
        };
        p.sync_sequence_length();
        p
    }

    #[test]
    fn timeline_plays_scenes_and_loops() {
        let p = two_scene_project();
        let first = p.sequence.scene_id;
        let total = p.sequence.total_beats();
        assert_eq!(p.timing.loop_beats, total);
        let at = |beat: f32| {
            p.sequence
                .frame_at(&EvalCtx::new(&p.timing, beat / total as f32, None))
                .unwrap()
        };
        // Start of the loop: coming in from the last clip (a wipe).
        let f0 = at(0.0);
        assert_eq!(f0.scene, first);
        let (from, _, t, tr) = f0.from.unwrap();
        assert_ne!(from, first);
        assert_eq!((t, tr.kind), (0.0, TransitionKind::Wipe));
        // Mid first clip: no transition, the scene's own loop.
        let mid = at(6.0);
        assert!(mid.from.is_none());
        assert!((mid.ctx.phase - 6.0 / p.sequence.scene_beats as f32).abs() < 1e-4);
        // Just into the second clip: crossfading.
        let second = at(total as f32 - 7.0);
        assert_ne!(second.scene, first);
        let (_, _, t, _) = second.from.unwrap();
        assert!((t - 0.5).abs() < 1e-3, "{t}");
        // Just before the wrap: pure last clip, as the first frame begins.
        let end = at(total as f32 - 1e-3);
        assert!(end.from.is_none());
        assert_eq!(end.scene, f0.from.unwrap().0);
    }

    #[test]
    fn switching_scenes_swaps_contents() {
        let mut p = two_scene_project();
        let (a, b) = (p.sequence.scene_id, p.sequence.scenes[0].id);
        let layers_a = p.layers.clone();
        assert!(p.edit_scene(b));
        assert_eq!(p.sequence.scene_id, b);
        assert_ne!(p.layers, layers_a);
        let view = p.scene_view(a).unwrap();
        assert_eq!(view.layers, layers_a);
        assert!(!view.sequence.is_active());
        assert!(p.edit_scene(a));
        assert_eq!(p.layers, layers_a);
        // Survives saving.
        let back = Project::from_json(&p.to_json()).unwrap();
        assert_eq!(back, p);
        p.remove_scene(b);
        assert_eq!(p.sequence.clips.len(), 1);
        assert_eq!(p.timing.loop_beats, p.sequence.scene_beats);
    }
}
