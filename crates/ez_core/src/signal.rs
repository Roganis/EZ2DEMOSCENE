//! Signals: numbers that change over the loop, made by signal nodes and
//! wired into any setting of a layer with a Drive node.
//!
//! Every signal is a pure function of the evaluation context (loop phase,
//! beat and music at that moment), so graphs stay loop-safe: the frame
//! after the loop point is the first frame again.

use crate::audio::HitKind;
use crate::clock::EvalCtx;
use crate::param::{Param, Wave};
use crate::scene::Layer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MathOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    /// 1 when A > B, else 0.
    Greater,
    /// 1 when A < B, else 0.
    Less,
}

impl MathOp {
    pub const ALL: [MathOp; 8] = [
        MathOp::Add,
        MathOp::Subtract,
        MathOp::Multiply,
        MathOp::Divide,
        MathOp::Min,
        MathOp::Max,
        MathOp::Greater,
        MathOp::Less,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MathOp::Add => "A + B",
            MathOp::Subtract => "A − B",
            MathOp::Multiply => "A × B",
            MathOp::Divide => "A ÷ B",
            MathOp::Min => "smaller",
            MathOp::Max => "larger",
            MathOp::Greater => "A > B",
            MathOp::Less => "A < B",
        }
    }

    pub fn apply(self, a: f32, b: f32) -> f32 {
        match self {
            MathOp::Add => a + b,
            MathOp::Subtract => a - b,
            MathOp::Multiply => a * b,
            MathOp::Divide => {
                if b.abs() < 1e-6 {
                    0.0
                } else {
                    a / b
                }
            }
            MathOp::Min => a.min(b),
            MathOp::Max => a.max(b),
            MathOp::Greater => (a > b) as u8 as f32,
            MathOp::Less => (a < b) as u8 as f32,
        }
    }
}

/// A node that makes or shapes a signal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "signal")]
pub enum SignalNode {
    /// Any animation: a wave (whole cycles per loop), a fade on every
    /// beat, random steps, or a link to the music (follow a band or react
    /// to hits).
    Wave { param: Param },
    /// Combines two signals (unconnected inputs use `a` and `b`).
    Math { op: MathOp, a: f32, b: f32 },
    /// Maps `in_min..in_max` to `out_min..out_max`.
    Remap {
        in_min: f32,
        in_max: f32,
        out_min: f32,
        out_max: f32,
        clamp: bool,
    },
    /// Rounds to `steps` levels per unit.
    Quantize { steps: u32 },
    /// Averages the input over `beats` beats around now. Smooths waves and
    /// steps; music follows stay as they are (use their own smoothing).
    Smooth { beats: f32 },
    /// Blends A to B by T (unconnected inputs use `a`, `b` and 0.5).
    Mix { a: f32, b: f32 },
    /// Steps through `values`, one every `beats` beats (restarting with the
    /// loop). `glide` slides between them.
    Sequence {
        values: Vec<f32>,
        beats: u32,
        glide: bool,
    },
    /// Hits of a kind counted so far in the loop, wrapped at `modulo`
    /// (0, 1, … modulo−1). 0 without music.
    Counter { hit: HitKind, modulo: u32 },
}

impl SignalNode {
    pub fn title(&self) -> &'static str {
        match self {
            SignalNode::Wave { .. } => "Wave / music",
            SignalNode::Math { .. } => "Math",
            SignalNode::Remap { .. } => "Remap",
            SignalNode::Quantize { .. } => "Quantize",
            SignalNode::Smooth { .. } => "Smooth",
            SignalNode::Mix { .. } => "Mix",
            SignalNode::Sequence { .. } => "Sequence",
            SignalNode::Counter { .. } => "Hit counter",
        }
    }

    /// Names of the signal inputs.
    pub fn input_names(&self) -> &'static [&'static str] {
        match self {
            SignalNode::Wave { .. } | SignalNode::Sequence { .. } | SignalNode::Counter { .. } => {
                &[]
            }
            SignalNode::Math { .. } => &["A", "B"],
            SignalNode::Mix { .. } => &["A", "B", "T"],
            _ => &["in"],
        }
    }

    pub fn templates() -> Vec<SignalNode> {
        vec![
            SignalNode::Wave {
                param: Param::new(0.5).osc(Wave::Sine, 0.5, 1),
            },
            SignalNode::Math {
                op: MathOp::Multiply,
                a: 1.0,
                b: 1.0,
            },
            SignalNode::Remap {
                in_min: 0.0,
                in_max: 1.0,
                out_min: 0.0,
                out_max: 2.0,
                clamp: true,
            },
            SignalNode::Quantize { steps: 4 },
            SignalNode::Smooth { beats: 1.0 },
            SignalNode::Mix { a: 0.0, b: 1.0 },
            SignalNode::Sequence {
                values: vec![0.0, 1.0, 0.5, 0.25],
                beats: 4,
                glide: false,
            },
            SignalNode::Counter {
                hit: HitKind::Kick,
                modulo: 4,
            },
        ]
    }

    /// Value of a node whose inputs are already known (`None` =
    /// unconnected). `Smooth` is handled by the graph (it needs the input
    /// at other moments); here it passes its input through.
    pub fn eval(&self, inputs: &[Option<f32>], ctx: &EvalCtx) -> f32 {
        let inp = |i: usize, default: f32| inputs.get(i).copied().flatten().unwrap_or(default);
        match self {
            SignalNode::Wave { param } => param.eval(ctx),
            SignalNode::Math { op, a, b } => op.apply(inp(0, *a), inp(1, *b)),
            SignalNode::Remap {
                in_min,
                in_max,
                out_min,
                out_max,
                clamp,
            } => {
                let span = in_max - in_min;
                let mut t = if span.abs() < 1e-6 {
                    0.0
                } else {
                    (inp(0, 0.0) - in_min) / span
                };
                if *clamp {
                    t = t.clamp(0.0, 1.0);
                }
                out_min + (out_max - out_min) * t
            }
            SignalNode::Quantize { steps } => {
                let s = (*steps).max(1) as f32;
                (inp(0, 0.0) * s).floor() / s
            }
            SignalNode::Smooth { .. } => inp(0, 0.0),
            SignalNode::Mix { a, b } => {
                let t = inp(2, 0.5);
                inp(0, *a) * (1.0 - t) + inp(1, *b) * t
            }
            SignalNode::Sequence {
                values,
                beats,
                glide,
            } => {
                if values.is_empty() {
                    return 0.0;
                }
                let x =
                    ctx.beat_phase.rem_euclid(1.0) * ctx.loop_beats as f32 / (*beats).max(1) as f32;
                let n = values.len();
                let i = x.floor() as usize;
                let a = values[i % n];
                if !*glide {
                    return a;
                }
                // Glide towards the next value, and back to the first one at
                // the end of the loop so it wraps smoothly.
                let total = ctx.loop_beats as f32 / (*beats).max(1) as f32;
                let steps = total.ceil() as usize;
                let b = if i + 1 >= steps {
                    values[0]
                } else {
                    values[(i + 1) % n]
                };
                // The last step may be shorter than the others.
                let len = (total - i as f32).clamp(1e-3, 1.0);
                let f = ((x - i as f32) / len).clamp(0.0, 1.0);
                let f = f * f * (3.0 - 2.0 * f);
                a + (b - a) * f
            }
            SignalNode::Counter { hit, modulo } => {
                let h = &ctx.music.hits[*hit as usize];
                if !ctx.music.active {
                    return 0.0;
                }
                (h.count % (*modulo).max(1)) as f32
            }
        }
    }
}

/// The context with its phases in 0..1 (the loop's end is its start, to
/// the last bit, so steps and edges agree).
pub fn wrapped(ctx: &EvalCtx) -> EvalCtx {
    let mut c = *ctx;
    c.phase = ctx.phase.rem_euclid(1.0);
    c.beat_phase = ctx.beat_phase.rem_euclid(1.0);
    if c.phase >= 1.0 {
        c.phase = 0.0;
    }
    if c.beat_phase >= 1.0 {
        c.beat_phase = 0.0;
    }
    c
}

/// The context `beats` beats later (music unchanged).
pub fn shifted(ctx: &EvalCtx, beats: f32) -> EvalCtx {
    let d = beats / ctx.loop_beats.max(1) as f32;
    let mut c = *ctx;
    c.phase = ctx.phase + d;
    c.beat_phase = ctx.beat_phase + d;
    wrapped(&c)
}

/// How a Drive node writes its signal into a setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DriveMode {
    /// The setting becomes the signal.
    #[default]
    Replace,
    /// The signal is added to the setting (its own animation stays).
    Add,
    /// The setting is scaled by the signal.
    Multiply,
}

impl DriveMode {
    pub const ALL: [DriveMode; 3] = [DriveMode::Replace, DriveMode::Add, DriveMode::Multiply];

    pub fn label(self) -> &'static str {
        match self {
            DriveMode::Replace => "set to",
            DriveMode::Add => "add",
            DriveMode::Multiply => "multiply by",
        }
    }
}

// ---------------------------------------------------------------------------
// Setting paths

/// Every number a Drive node can set on `layer`, as dotted paths such as
/// `kind.height` or `transform.position.1`. Animatable settings and plain
/// numbers both count; colours appear per channel.
pub fn setting_paths(layer: &Layer) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(v) = serde_json::to_value(layer) {
        walk(&v, String::new(), &mut out);
    }
    out
}

fn is_param(v: &Value) -> bool {
    v.as_object()
        .is_some_and(|o| o.contains_key("base") && o.contains_key("amp"))
}

fn walk(v: &Value, path: String, out: &mut Vec<String>) {
    let join = |k: &str| {
        if path.is_empty() {
            k.to_string()
        } else {
            format!("{path}.{k}")
        }
    };
    match v {
        Value::Number(_) => out.push(path),
        Value::Object(_) if is_param(v) => out.push(path),
        Value::Object(o) => {
            for (k, c) in o {
                walk(c, join(k), out);
            }
        }
        // Short arrays: positions, colours, spins. Longer ones are data
        // (meshes, gradients) and not offered.
        Value::Array(a) if a.len() <= 4 => {
            for (i, c) in a.iter().enumerate() {
                walk(c, join(&i.to_string()), out);
            }
        }
        _ => {}
    }
}

fn lookup<'a>(v: &'a mut Value, path: &str) -> Option<&'a mut Value> {
    let mut cur = v;
    for key in path.split('.') {
        cur = match cur {
            Value::Object(o) => o.get_mut(key)?,
            Value::Array(a) => a.get_mut(key.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}

fn number(x: f32, like: &serde_json::Number) -> Value {
    if like.is_u64() {
        Value::from(x.round().max(0.0) as u64)
    } else if like.is_i64() {
        Value::from(x.round() as i64)
    } else {
        serde_json::Number::from_f64(x as f64)
            .map(Value::Number)
            .unwrap_or(Value::from(0.0))
    }
}

/// Write `value` into the setting at `path` of `layer`. Unknown paths, or
/// values the setting can't take, leave the layer unchanged (returns false).
pub fn drive(layer: &mut Layer, path: &str, mode: DriveMode, value: f32) -> bool {
    drive_all(layer, &[(path, mode, value)])
}

/// Several [`drive`]s at once (one conversion of the layer).
pub fn drive_all(layer: &mut Layer, drives: &[(&str, DriveMode, f32)]) -> bool {
    if drives.is_empty() || drives.iter().any(|d| !d.2.is_finite()) {
        return false;
    }
    let Ok(mut v) = serde_json::to_value(&*layer) else {
        return false;
    };
    let mut changed = false;
    for &(path, mode, x) in drives {
        let Some(target) = lookup(&mut v, path) else {
            continue;
        };
        let new = match (&*target, mode) {
            (Value::Number(n), DriveMode::Replace) => number(x, n),
            (Value::Number(n), DriveMode::Add) => number(n.as_f64().unwrap_or(0.0) as f32 + x, n),
            (Value::Number(n), DriveMode::Multiply) => {
                number(n.as_f64().unwrap_or(0.0) as f32 * x, n)
            }
            // A static animatable setting is written as a plain number.
            (Value::Object(_), DriveMode::Replace) if is_param(target) => Value::from(x as f64),
            (Value::Object(o), m) if is_param(target) => {
                let mut o = o.clone();
                let get = |o: &serde_json::Map<String, Value>, k: &str| {
                    o.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32
                };
                let f = |x: f32| Value::from(x as f64);
                match m {
                    DriveMode::Add => {
                        let b = get(&o, "base") + x;
                        o.insert("base".into(), f(b));
                    }
                    _ => {
                        let (b, a) = (get(&o, "base") * x, get(&o, "amp") * x);
                        o.insert("base".into(), f(b));
                        o.insert("amp".into(), f(a));
                    }
                }
                Value::Object(o)
            }
            _ => continue,
        };
        *target = new;
        changed = true;
    }
    if !changed {
        return false;
    }
    match serde_json::from_value::<Layer>(v) {
        Ok(l) => {
            *layer = l;
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Graph, NodeKind};
    use crate::scene::*;

    fn terrain() -> Layer {
        Layer::new("T", LayerKind::Terrain(Terrain::default()))
    }

    #[test]
    fn paths_cover_params_numbers_and_colours() {
        let paths = setting_paths(&terrain());
        for p in [
            "kind.height",
            "kind.size",
            "kind.cells",
            "transform.position.1",
            "transform.scale",
            "kind.line_color.0",
        ] {
            assert!(paths.iter().any(|x| x == p), "{p} missing: {paths:?}");
        }
    }

    #[test]
    fn drive_modes() {
        let mut l = terrain();
        assert!(drive(&mut l, "kind.height", DriveMode::Replace, 9.5));
        let LayerKind::Terrain(t) = &l.kind else {
            unreachable!()
        };
        assert_eq!(t.height, Param::new(9.5));
        // Integers round; animated params keep their animation on Add.
        assert!(drive(&mut l, "kind.cells", DriveMode::Replace, 99.6));
        let mut glow = Param::new(1.0).osc(Wave::Sine, 0.5, 2);
        if let LayerKind::Terrain(t) = &mut l.kind {
            t.glow = glow;
        }
        assert!(drive(&mut l, "kind.glow", DriveMode::Add, 1.0));
        assert!(drive(&mut l, "transform.position.1", DriveMode::Add, 2.0));
        let LayerKind::Terrain(t) = &l.kind else {
            unreachable!()
        };
        assert_eq!(t.cells, 100);
        glow.base = 2.0;
        assert_eq!(t.glow, glow);
        assert_eq!(l.transform.position[1], 2.0);
        assert!(drive(&mut l, "kind.glow", DriveMode::Multiply, 2.0));
        let LayerKind::Terrain(t) = &l.kind else {
            unreachable!()
        };
        assert_eq!((t.glow.base, t.glow.amp), (4.0, 1.0));
        // Nonsense is refused and changes nothing.
        let before = l.clone();
        assert!(!drive(&mut l, "kind.nope", DriveMode::Replace, 1.0));
        assert!(!drive(&mut l, "kind.style", DriveMode::Replace, 1.0));
        assert!(!drive(&mut l, "kind.height", DriveMode::Replace, f32::NAN));
        assert_eq!(l, before);
    }

    #[test]
    fn sequence_and_quantize() {
        let s = SignalNode::Sequence {
            values: vec![1.0, 2.0, 3.0],
            beats: 4,
            glide: false,
        };
        let at = |beat: f32| EvalCtx::at(beat / 16.0);
        assert_eq!(s.eval(&[], &at(0.5)), 1.0);
        assert_eq!(s.eval(&[], &at(5.0)), 2.0);
        assert_eq!(s.eval(&[], &at(13.0)), 1.0);
        let q = SignalNode::Quantize { steps: 4 };
        assert_eq!(q.eval(&[Some(0.6)], &at(0.0)), 0.5);
    }

    /// A graph: wave → smooth → remap → drive(height), plus a glide
    /// sequence into a multiply drive. Must loop and move.
    #[test]
    fn driven_graph_loops_and_moves() {
        let mut g = Graph::default();
        let out = g.add(NodeKind::Output, [0.0; 2]);
        let src = g.add(NodeKind::Source { layer: terrain() }, [0.0; 2]);
        let wave = g.add(
            NodeKind::Signal {
                sig: SignalNode::Wave {
                    param: Param::new(0.0).osc(Wave::Random, 1.0, 8),
                },
            },
            [0.0; 2],
        );
        let smooth = g.add(
            NodeKind::Signal {
                sig: SignalNode::Smooth { beats: 2.0 },
            },
            [0.0; 2],
        );
        let remap = g.add(
            NodeKind::Signal {
                sig: SignalNode::Remap {
                    in_min: -1.0,
                    in_max: 1.0,
                    out_min: 2.0,
                    out_max: 10.0,
                    clamp: true,
                },
            },
            [0.0; 2],
        );
        let drive1 = g.add(
            NodeKind::Drive {
                path: "kind.height".into(),
                mode: DriveMode::Replace,
            },
            [0.0; 2],
        );
        let seq = g.add(
            NodeKind::Signal {
                sig: SignalNode::Sequence {
                    values: vec![1.0, 2.0],
                    beats: 3,
                    glide: true,
                },
            },
            [0.0; 2],
        );
        let drive2 = g.add(
            NodeKind::Drive {
                path: "transform.scale".into(),
                mode: DriveMode::Multiply,
            },
            [0.0; 2],
        );
        g.connect(wave, 0, smooth, 0);
        g.connect(smooth, 0, remap, 0);
        g.connect(src, 0, drive1, 0);
        g.connect(remap, 0, drive1, 1);
        g.connect(drive1, 0, drive2, 0);
        g.connect(seq, 0, drive2, 1);
        g.connect(drive2, 0, out, 0);
        // A signal wired into a layer pin is ignored.
        g.wires.push(crate::graph::Wire {
            from: seq,
            from_pin: 0,
            to: out,
            to_pin: 1,
        });

        let look = |phase: f32| {
            let ls = g.compile_at(&EvalCtx::at(phase));
            assert_eq!(ls.len(), 1);
            let LayerKind::Terrain(t) = &ls[0].kind else {
                unreachable!()
            };
            (
                t.height.eval(&EvalCtx::at(phase)),
                ls[0].transform.scale.base,
            )
        };
        let (h0, s0) = look(0.0);
        let (h1, s1) = look(1.0);
        assert!((h0 - h1).abs() < 1e-4 && (s0 - s1).abs() < 1e-4);
        // Continuous through the loop point (smooth + glide).
        let (hb, sb) = look(1.0 - 1e-4);
        assert!((h0 - hb).abs() < 0.01, "{h0} {hb}");
        assert!((s0 - sb).abs() < 0.01, "{s0} {sb}");
        let heights: Vec<f32> = (0..32).map(|i| look(i as f32 / 32.0).0).collect();
        let spread = heights.iter().cloned().fold(f32::MIN, f32::max)
            - heights.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread > 1.0, "height barely moves: {heights:?}");
        assert!(heights.iter().all(|h| (2.0..=10.0).contains(h)));
        // Without a moment, drives are left out.
        assert_eq!(g.compile()[0], terrain());
    }

    #[test]
    fn signal_flow_preset_drives_its_centrepiece() {
        let p = crate::presets::signal_flow();
        let g = p.graph.as_ref().unwrap();
        assert_eq!(
            g.compile().len(),
            crate::presets::orbiting_solid().layers.len()
        );
        let at = |phase: f32| {
            let ls = p.scene_layers(&EvalCtx::at(phase)).into_owned();
            let d = ls.into_iter().find(|l| l.name == "Dodecahedron").unwrap();
            let LayerKind::Mesh(m) = &d.kind else {
                unreachable!()
            };
            (m.material.emissive.base, d.transform.scale.base)
        };
        let (a, b) = (at(0.05), at(0.3));
        assert_ne!(a.0, b.0, "glow not driven");
        assert_ne!(a.1, b.1, "scale not driven");
        assert_eq!(at(0.0), at(1.0));
    }

    /// Random signal graphs (every node kind, random wiring) give the same
    /// value at the loop's start and end.
    #[test]
    fn random_signal_graphs_loop() {
        let mut rng = crate::rng::Rng::new(7);
        let templates = SignalNode::templates();
        for round in 0..200 {
            let mut g = Graph::default();
            let mut ids = Vec::new();
            for _ in 0..6 {
                let mut sig = templates
                    [(rng.f32() * templates.len() as f32) as usize % templates.len()]
                .clone();
                if let SignalNode::Wave { param } = &mut sig {
                    let waves = Wave::ALL;
                    param.wave = waves[(rng.f32() * waves.len() as f32) as usize % waves.len()];
                    param.cycles = 1 + (rng.f32() * 8.0) as i32;
                }
                let id = g.add(NodeKind::Signal { sig: sig.clone() }, [0.0; 2]);
                // Wire each input to a random earlier node.
                for pin in 0..sig.input_names().len() {
                    if !ids.is_empty() && rng.f32() < 0.8 {
                        let from = ids[(rng.f32() * ids.len() as f32) as usize % ids.len()];
                        g.connect(from, 0, id, pin);
                    }
                }
                ids.push(id);
            }
            let timing = crate::Timing {
                bpm: 120.0,
                loop_beats: [4, 8, 12, 16][round % 4],
            };
            let at = |phase: f32| EvalCtx::new(&timing, phase, None);
            for id in &ids {
                let (a, b) = (g.signal_at(*id, &at(0.0)), g.signal_at(*id, &at(1.0)));
                let (a, b) = (a.unwrap(), b.unwrap());
                assert!(
                    (a - b).abs() < 1e-3,
                    "round {round} node {id}: {a} vs {b}\n{g:?}"
                );
            }
        }
    }
}
