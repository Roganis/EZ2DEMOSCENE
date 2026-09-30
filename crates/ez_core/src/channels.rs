//! Live channels: named values and hits that another program (or the
//! editor's preview controls) feeds in while the project plays - a VJ
//! controller, a game, OSC or MIDI glue.
//!
//! A project declares its channels by name in [`MusicSettings::channels`]
//! (crate::music); any value links to one through its 🎵 row, as it would
//! to the kick or the bass. A **value** channel is followed like a music
//! curve (snappy or smoothed); a **hit** channel plays the row's shape on
//! every hit, like the kick.
//!
//! Live input is not repeatable, so exports see every channel silent (as
//! they ignore the microphone): only a live preview, or a host program
//! rendering frames itself, feeds them.
//!
//! [`MusicSettings::channels`]: crate::music::MusicSettings::channels

use crate::music::HitState;
use serde::{Deserialize, Serialize};

/// How many channels a project can declare.
pub const MAX_CHANNELS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ChannelKind {
    /// A level, followed like a music curve (0..1 is the useful range).
    #[default]
    Value,
    /// Moments, each playing the linked value's shape.
    Hit,
}

impl ChannelKind {
    pub const ALL: [ChannelKind; 2] = [ChannelKind::Value, ChannelKind::Hit];
    pub fn label(self) -> &'static str {
        match self {
            ChannelKind::Value => "Value",
            ChannelKind::Hit => "Hits",
        }
    }
}

/// One declared channel. The host finds it by `name`, so a project and a
/// host agree on names, never on numbers.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelDef {
    pub name: String,
    pub kind: ChannelKind,
}

/// Index of the channel called `name` (case-insensitive), if declared.
pub fn find(defs: &[ChannelDef], name: &str) -> Option<usize> {
    defs.iter()
        .take(MAX_CHANNELS)
        .position(|d| d.name.eq_ignore_ascii_case(name.trim()))
}

/// Every channel at one moment (part of [`crate::MusicFrame`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelFrame {
    /// False when nothing feeds the channels: every channel link is silent.
    pub active: bool,
    pub fast: [f32; MAX_CHANNELS],
    pub smooth: [f32; MAX_CHANNELS],
    pub hits: [HitState; MAX_CHANNELS],
}

impl Default for ChannelFrame {
    fn default() -> Self {
        ChannelFrame {
            active: false,
            fast: [0.0; MAX_CHANNELS],
            smooth: [0.0; MAX_CHANNELS],
            hits: [HitState::default(); MAX_CHANNELS],
        }
    }
}

/// The live state a host feeds: set values and hits as they happen, then
/// [`advance`](Self::advance) once a frame and hand [`frame`](Self::frame)
/// to the evaluation (`EvalCtx::with_channels`).
#[derive(Clone, Debug)]
pub struct ChannelInput {
    target: [f32; MAX_CHANNELS],
    fast: [f32; MAX_CHANNELS],
    smooth: [f32; MAX_CHANNELS],
    /// Seconds since the last hit and its strength, per channel.
    since: [f32; MAX_CHANNELS],
    strength: [f32; MAX_CHANNELS],
    count: [u32; MAX_CHANNELS],
}

impl Default for ChannelInput {
    fn default() -> Self {
        ChannelInput {
            target: [0.0; MAX_CHANNELS],
            fast: [0.0; MAX_CHANNELS],
            smooth: [0.0; MAX_CHANNELS],
            since: [1e6; MAX_CHANNELS],
            strength: [0.0; MAX_CHANNELS],
            count: [0; MAX_CHANNELS],
        }
    }
}

/// The music curves' followers (crate::audio): a snappy one and a smoothed
/// one, so a channel feels like a curve of the song.
const FAST: (f32, f32) = (0.005, 0.12);
const SMOOTH: (f32, f32) = (0.08, 0.45);

fn follow(y: f32, x: f32, (attack, release): (f32, f32), dt: f32) -> f32 {
    let tau = if x > y { attack } else { release };
    y + (1.0 - (-dt / tau.max(1e-4)).exp()) * (x - y)
}

impl ChannelInput {
    /// Set a value channel; it is reached through the followers.
    pub fn set(&mut self, i: usize, value: f32) {
        if let Some(t) = self.target.get_mut(i) {
            *t = if value.is_finite() { value } else { 0.0 };
        }
    }

    /// A hit on channel `i` now, `strength` 0..1.
    pub fn hit(&mut self, i: usize, strength: f32) {
        if i < MAX_CHANNELS {
            self.since[i] = 0.0;
            self.strength[i] = strength.clamp(0.0, 1.0);
            self.count[i] = self.count[i].wrapping_add(1);
        }
    }

    /// Move the followers and the hit clocks on by `dt` seconds.
    pub fn advance(&mut self, dt: f32) {
        let dt = dt.max(0.0);
        for i in 0..MAX_CHANNELS {
            self.fast[i] = follow(self.fast[i], self.target[i], FAST, dt);
            self.smooth[i] = follow(self.smooth[i], self.target[i], SMOOTH, dt);
            self.since[i] = (self.since[i] + dt).min(1e6);
        }
    }

    /// Forget every value and hit.
    pub fn reset(&mut self) {
        *self = ChannelInput::default();
    }

    /// The raw value last set on channel `i`.
    pub fn value(&self, i: usize) -> f32 {
        self.target.get(i).copied().unwrap_or(0.0)
    }

    pub fn frame(&self) -> ChannelFrame {
        ChannelFrame {
            active: true,
            fast: self.fast,
            smooth: self.smooth,
            hits: std::array::from_fn(|i| HitState {
                since: self.since[i],
                strength: self.strength[i],
                count: self.count[i],
            }),
        }
    }
}

/// Remove channel `k` from `project`: every link to a later channel moves
/// down one, and every link to `k` itself is switched off. Links name
/// channels by position, so this is the only safe way to delete one.
///
/// It walks the project's own JSON form, so it reaches every value in
/// every layer, node and scene without a list of them to keep up to date.
pub fn remove_channel(project: &mut crate::scene::Project, k: usize) {
    if k >= project.music.channels.len() {
        return;
    }
    project.music.channels.remove(k);
    let Ok(mut v) = serde_json::to_value(&*project) else {
        return;
    };
    fn walk(v: &mut serde_json::Value, k: u64) {
        match v {
            serde_json::Value::Object(map) => {
                let mut off = false;
                if let Some(serde_json::Value::Object(src)) = map.get_mut("source") {
                    for key in ["Channel", "ChannelHit"] {
                        if let Some(n) = src.get(key).and_then(|n| n.as_u64()) {
                            if n == k {
                                off = true;
                            } else if n > k {
                                src.insert(key.into(), (n - 1).into());
                            }
                        }
                    }
                }
                if off {
                    let kick = serde_json::to_value(crate::music::AudioSource::default());
                    map.insert("source".into(), kick.unwrap_or_default());
                    map.insert("amount".into(), 0.0.into());
                }
                for (_, child) in map.iter_mut() {
                    walk(child, k);
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(|c| walk(c, k)),
            _ => {}
        }
    }
    walk(&mut v, k as u64);
    if let Ok(p) = serde_json::from_value(v) {
        *project = p;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_follow_and_hits_ring() {
        let mut c = ChannelInput::default();
        c.set(3, 1.0);
        c.hit(5, 0.8);
        c.advance(0.05);
        let f = c.frame();
        assert!(f.active);
        assert!(f.fast[3] > 0.99, "the snappy follower is there at once");
        assert!(
            f.smooth[3] > 0.3 && f.smooth[3] < 0.9,
            "the smooth one eases"
        );
        assert!((f.hits[5].since - 0.05).abs() < 1e-6);
        assert_eq!(f.hits[5].count, 1);
        assert!((f.hits[5].strength - 0.8).abs() < 1e-6);
        assert!(f.hits[4].since > 1e5, "no hit yet");
        c.set(3, 0.0);
        c.advance(1.0);
        assert!(c.frame().fast[3] < 0.01, "and lets go");
    }

    #[test]
    fn found_by_name_not_number() {
        let defs = vec![
            ChannelDef {
                name: "Combo".into(),
                kind: ChannelKind::Value,
            },
            ChannelDef {
                name: "Perfect".into(),
                kind: ChannelKind::Hit,
            },
        ];
        assert_eq!(find(&defs, "perfect "), Some(1));
        assert_eq!(find(&defs, "gauge"), None);
    }

    #[test]
    fn removing_a_channel_renumbers_the_links() {
        use crate::music::AudioSource;
        let mut p = crate::presets::by_name("Neon Arena").unwrap();
        for name in ["A", "B", "C"] {
            p.music.channels.push(ChannelDef {
                name: name.into(),
                kind: ChannelKind::Value,
            });
        }
        assert!(p.layers.len() >= 3, "the preset has three layers");
        for i in 0..3 {
            let m = &mut p.layers[i].transform.scale.music;
            m.source = AudioSource::Channel(i as u8);
            m.amount = 1.0;
        }
        remove_channel(&mut p, 1);
        assert_eq!(p.music.channels.len(), 2);
        assert_eq!(p.music.channels[1].name, "C");
        let m = |i: usize| p.layers[i].transform.scale.music;
        assert_eq!(m(0).source, AudioSource::Channel(0));
        assert_eq!(m(1).amount, 0.0, "the link to the removed channel is off");
        assert_eq!(
            m(2).source,
            AudioSource::Channel(1),
            "the later one moved down"
        );
    }

    #[test]
    fn out_of_range_is_ignored() {
        let mut c = ChannelInput::default();
        c.set(MAX_CHANNELS, 1.0);
        c.hit(MAX_CHANNELS + 3, 1.0);
        c.set(0, f32::NAN);
        c.advance(0.1);
        assert_eq!(c.frame().fast[0], 0.0);
    }
}
