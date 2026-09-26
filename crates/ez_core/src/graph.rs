//! Node graph ("advanced mode").
//!
//! Nodes produce and transform *streams of layers*. The graph compiles down
//! to a plain `Vec<Layer>`, so the renderer never needs to know whether a
//! scene was made in simple mode or with nodes.

use crate::color::{hue_rotate, Rgb};
use crate::rng::Rng;
use crate::scene::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "node")]
#[allow(clippy::large_enum_variant)]
pub enum NodeKind {
    /// Emits one layer.
    Source { layer: Layer },
    /// Replaces the symmetry of every incoming layer.
    Symmetry { symmetry: Symmetry },
    /// Moves every incoming layer.
    Offset { offset: [f32; 3] },
    /// Adds spin (turns per loop) to every incoming layer.
    Spin { turns: [i32; 3] },
    /// Multiplies the scale of every incoming layer.
    Scale { factor: f32 },
    /// Rotates hues and scales glow of every incoming layer.
    Tint { hue: f32, glow: f32 },
    /// Repeats the incoming layers `count` times, offsetting each copy.
    Array {
        count: u32,
        step: [f32; 3],
        rotate_y: f32,
    },
    /// Randomly moves and turns every incoming layer: once (re-roll 0) or
    /// as a beat-synced shake that picks a new direction `per_loop` times.
    Jitter {
        position: crate::Param,
        /// Degrees.
        rotation: crate::Param,
        /// New random direction this many times per loop (0 = scatter once).
        #[serde(default)]
        per_loop: u32,
        seed: u32,
    },
    /// Adds a mirrored copy of every incoming layer.
    Mirror {
        /// 0 = X, 1 = Y, 2 = Z.
        axis: u8,
        /// Position of the mirror plane along the axis.
        at: f32,
    },
    /// Makes every incoming layer blink or flash.
    Strobe { blink: Blink },
    /// Overrides colour, glow, texture, wireframe look and glitch.
    Material {
        color: Option<Rgb>,
        glow: f32,
        texture: Option<String>,
        wireframe: bool,
        glitch: f32,
        glitch_style: GlitchStyle,
    },
    /// Concatenates any number of streams.
    Merge,
    /// Final output: everything connected here is rendered.
    Output,
}

impl NodeKind {
    pub fn title(&self) -> String {
        match self {
            NodeKind::Source { layer } => format!("{} ({})", layer.name, layer.type_label()),
            NodeKind::Symmetry { .. } => "Symmetry".into(),
            NodeKind::Offset { .. } => "Offset".into(),
            NodeKind::Spin { .. } => "Spin".into(),
            NodeKind::Scale { .. } => "Scale".into(),
            NodeKind::Tint { .. } => "Tint".into(),
            NodeKind::Array { .. } => "Array".into(),
            NodeKind::Jitter { .. } => "Jitter".into(),
            NodeKind::Mirror { .. } => "Mirror".into(),
            NodeKind::Strobe { .. } => "Strobe".into(),
            NodeKind::Material { .. } => "Colour / material".into(),
            NodeKind::Merge => "Merge".into(),
            NodeKind::Output => "Output".into(),
        }
    }

    /// Number of input pins.
    pub fn inputs(&self) -> usize {
        match self {
            NodeKind::Source { .. } => 0,
            NodeKind::Merge => 4,
            NodeKind::Output => 8,
            _ => 1,
        }
    }

    /// Number of output pins.
    pub fn outputs(&self) -> usize {
        match self {
            NodeKind::Output => 0,
            _ => 1,
        }
    }

    /// Templates offered in the "add node" menu.
    pub fn modifier_templates() -> Vec<NodeKind> {
        vec![
            NodeKind::Symmetry {
                symmetry: Symmetry::Radial { count: 6 },
            },
            NodeKind::Offset {
                offset: [0.0, 1.0, 0.0],
            },
            NodeKind::Spin { turns: [0, 1, 0] },
            NodeKind::Scale { factor: 1.5 },
            NodeKind::Tint {
                hue: 0.1,
                glow: 1.0,
            },
            NodeKind::Array {
                count: 3,
                step: [0.0, 2.0, 0.0],
                rotate_y: 30.0,
            },
            NodeKind::Jitter {
                position: crate::Param::new(1.0),
                rotation: crate::Param::new(30.0),
                per_loop: 0,
                seed: 1,
            },
            NodeKind::Mirror { axis: 0, at: 0.0 },
            NodeKind::Strobe {
                blink: Blink {
                    mode: BlinkMode::Flash,
                    duty: 0.3,
                    ..Default::default()
                },
            },
            NodeKind::Material {
                color: Some(crate::color::hex(0x00e5ff)),
                glow: 1.0,
                texture: None,
                wireframe: false,
                glitch: 0.0,
                glitch_style: GlitchStyle::Jitter,
            },
            NodeKind::Merge,
        ]
    }

    fn apply(&self, input: Vec<Layer>) -> Vec<Layer> {
        match self {
            NodeKind::Source { layer } => vec![layer.clone()],
            NodeKind::Merge | NodeKind::Output => input,
            NodeKind::Symmetry { symmetry } => input
                .into_iter()
                .map(|mut l| {
                    l.symmetry = *symmetry;
                    l
                })
                .collect(),
            NodeKind::Offset { offset } => input
                .into_iter()
                .map(|mut l| {
                    for (p, o) in l.transform.position.iter_mut().zip(offset) {
                        *p += o;
                    }
                    l
                })
                .collect(),
            NodeKind::Spin { turns } => input
                .into_iter()
                .map(|mut l| {
                    for (s, t) in l.transform.spin.iter_mut().zip(turns) {
                        *s += t;
                    }
                    l
                })
                .collect(),
            NodeKind::Scale { factor } => input
                .into_iter()
                .map(|mut l| {
                    l.transform.scale.base *= factor;
                    l.transform.scale.amp *= factor;
                    l
                })
                .collect(),
            NodeKind::Tint { hue, glow } => input
                .into_iter()
                .map(|mut l| {
                    tint_layer(&mut l, *hue, *glow);
                    l
                })
                .collect(),
            NodeKind::Array {
                count,
                step,
                rotate_y,
            } => {
                let mut out = Vec::new();
                for k in 0..(*count).clamp(1, 64) {
                    for l in &input {
                        let mut l = l.clone();
                        for (p, s) in l.transform.position.iter_mut().zip(step) {
                            *p += s * k as f32;
                        }
                        l.transform.rotation[1] += rotate_y * k as f32;
                        out.push(l);
                    }
                }
                out
            }
            NodeKind::Jitter {
                position,
                rotation,
                per_loop,
                seed,
            } => input
                .into_iter()
                .enumerate()
                .map(|(i, mut l)| {
                    if *per_loop > 0 {
                        // Beat-synced: every layer shakes in its own directions.
                        l.transform.shake = Shake {
                            amount: *position,
                            turn: *rotation,
                            per_loop: *per_loop,
                            seed: seed.wrapping_mul(31).wrapping_add(i as u32 * 7919 + 1),
                        };
                        return l;
                    }
                    let mut rng = Rng::new(((*seed as u64) << 20) ^ (i as u64 * 7919 + 17));
                    for p in &mut l.transform.position {
                        *p += rng.signed() * position.base;
                    }
                    for r in &mut l.transform.rotation {
                        *r += rng.signed() * rotation.base;
                    }
                    l
                })
                .collect(),
            NodeKind::Mirror { axis, at } => {
                let mut out = input.clone();
                out.extend(
                    input
                        .into_iter()
                        .map(|l| mirror_layer(l, *axis as usize, *at)),
                );
                out
            }
            NodeKind::Strobe { blink } => input
                .into_iter()
                .map(|mut l| {
                    l.blink = *blink;
                    l
                })
                .collect(),
            NodeKind::Material {
                color,
                glow,
                texture,
                wireframe,
                glitch,
                glitch_style,
            } => input
                .into_iter()
                .map(|mut l| {
                    if let Some(c) = color {
                        set_layer_color(&mut l, *c);
                    }
                    tint_layer(&mut l, 0.0, *glow);
                    if let (LayerKind::Terrain(t), Some(_)) = (&mut l.kind, texture) {
                        t.texture = texture.clone();
                    }
                    if let LayerKind::Mesh(m) = &mut l.kind {
                        if texture.is_some() {
                            m.material.texture = texture.clone();
                        }
                        if *wireframe {
                            m.material.emissive_mode = EmissiveMode::Edges;
                            m.material.flat_shading = true;
                            m.material.base_color = [0.01, 0.01, 0.012];
                            if m.material.emissive.base <= 0.0 {
                                m.material.emissive.base = 1.5 * glow;
                            }
                        }
                        if *glitch > 0.0 {
                            m.material.glitch.amount = crate::Param::new(*glitch);
                            m.material.glitch.style = *glitch_style;
                        }
                    }
                    l
                })
                .collect(),
        }
    }
}

/// A copy of `l` reflected across the plane `axis = at`.
pub fn mirror_layer(mut l: Layer, axis: usize, at: f32) -> Layer {
    let axis = axis.min(2);
    let t = &mut l.transform;
    t.position[axis] = 2.0 * at - t.position[axis];
    t.stretch[axis] = -t.stretch[axis];
    // Reflecting a rotation negates its components about the other axes.
    for a in 0..3 {
        if a != axis {
            t.rotation[a] = -t.rotation[a];
            t.spin[a] = -t.spin[a];
        }
    }
    if axis == 1 {
        t.bob.base = -t.bob.base;
        t.bob.amp = -t.bob.amp;
    }
    l.name = format!("{} (mirror)", l.name);
    l
}

/// Set the main glow colour of a layer.
pub fn set_layer_color(l: &mut Layer, c: Rgb) {
    match &mut l.kind {
        LayerKind::Mesh(m) => m.material.emissive_color = c,
        LayerKind::Particles(p) => {
            p.color_a = c;
            p.color_b = c;
        }
        LayerKind::Backdrop(b) => b.color_b = c,
        LayerKind::Mirror(m) => m.grid_color = c,
        LayerKind::Terrain(t) => t.line_color = c,
        LayerKind::Lasers(z) => {
            z.color_a = c;
            z.color_b = c;
        }
        LayerKind::Ribbon(r) => r.color = c,
    }
}

/// Rotate all colours of a layer and scale its glow.
pub fn tint_layer(l: &mut Layer, hue: f32, glow: f32) {
    match &mut l.kind {
        LayerKind::Mesh(m) => {
            m.material.base_color = hue_rotate(m.material.base_color, hue);
            m.material.emissive_color = hue_rotate(m.material.emissive_color, hue);
            m.material.emissive.base *= glow;
            m.material.emissive.amp *= glow;
        }
        LayerKind::Particles(p) => {
            p.color_a = hue_rotate(p.color_a, hue);
            p.color_b = hue_rotate(p.color_b, hue);
            p.intensity.base *= glow;
        }
        LayerKind::Backdrop(b) => {
            b.color_a = hue_rotate(b.color_a, hue);
            b.color_b = hue_rotate(b.color_b, hue);
            b.color_c = hue_rotate(b.color_c, hue);
            b.intensity.base *= glow;
        }
        LayerKind::Mirror(m) => {
            m.base_color = hue_rotate(m.base_color, hue);
            m.tint = hue_rotate(m.tint, hue);
            m.grid_color = hue_rotate(m.grid_color, hue);
            m.grid.base *= glow;
        }
        LayerKind::Terrain(t) => {
            t.line_color = hue_rotate(t.line_color, hue);
            t.fill_color = hue_rotate(t.fill_color, hue);
            t.glow.base *= glow;
            t.glow.amp *= glow;
        }
        LayerKind::Lasers(z) => {
            z.color_a = hue_rotate(z.color_a, hue);
            z.color_b = hue_rotate(z.color_b, hue);
            z.intensity.base *= glow;
            z.intensity.amp *= glow;
        }
        LayerKind::Ribbon(r) => {
            r.color = hue_rotate(r.color, hue);
            r.glow.base *= glow;
            r.glow.amp *= glow;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: u32,
    pub pos: [f32; 2],
    pub kind: NodeKind,
}

/// Connection from `from` node's output pin to `to` node's input pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wire {
    pub from: u32,
    pub from_pin: usize,
    pub to: u32,
    pub to_pin: usize,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Graph {
    pub nodes: Vec<GraphNode>,
    pub wires: Vec<Wire>,
}

impl Graph {
    /// Builds a graph with one source node per layer, all wired into a merge
    /// feeding the output.
    pub fn from_layers(layers: &[Layer]) -> Graph {
        let mut g = Graph::default();
        let out = g.add(NodeKind::Output, [620.0, 120.0]);
        let per_merge = 4;
        for (chunk_i, chunk) in layers.chunks(per_merge).enumerate() {
            let merge = g.add(NodeKind::Merge, [380.0, 40.0 + chunk_i as f32 * 440.0]);
            g.connect(merge, 0, out, chunk_i.min(7));
            for (j, l) in chunk.iter().enumerate() {
                let y = 20.0 + (chunk_i * per_merge + j) as f32 * 110.0;
                let s = g.add(NodeKind::Source { layer: l.clone() }, [80.0, y]);
                g.connect(s, 0, merge, j);
            }
        }
        g
    }

    pub fn next_id(&self) -> u32 {
        self.nodes.iter().map(|n| n.id + 1).max().unwrap_or(0)
    }

    pub fn add(&mut self, kind: NodeKind, pos: [f32; 2]) -> u32 {
        let id = self.next_id();
        self.nodes.push(GraphNode { id, pos, kind });
        id
    }

    pub fn connect(&mut self, from: u32, from_pin: usize, to: u32, to_pin: usize) {
        self.wires.retain(|w| !(w.to == to && w.to_pin == to_pin));
        self.wires.push(Wire {
            from,
            from_pin,
            to,
            to_pin,
        });
    }

    pub fn node(&self, id: u32) -> Option<&GraphNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Compile the graph to a layer list (cycles are ignored).
    pub fn compile(&self) -> Vec<Layer> {
        let Some(out) = self
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Output))
        else {
            return Vec::new();
        };
        let mut visiting = Vec::new();
        self.eval_node(out.id, &mut visiting)
    }

    fn eval_node(&self, id: u32, visiting: &mut Vec<u32>) -> Vec<Layer> {
        if visiting.contains(&id) || visiting.len() > 256 {
            return Vec::new();
        }
        let Some(node) = self.node(id) else {
            return Vec::new();
        };
        visiting.push(id);
        let mut inputs: Vec<&Wire> = self.wires.iter().filter(|w| w.to == id).collect();
        inputs.sort_by_key(|w| w.to_pin);
        let mut stream = Vec::new();
        for w in inputs {
            stream.extend(self.eval_node(w.from, visiting));
        }
        visiting.pop();
        node.kind.apply(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;

    #[test]
    fn from_layers_compiles_back() {
        for p in presets::all() {
            let g = Graph::from_layers(&p.project.layers);
            assert_eq!(g.compile(), p.project.layers, "{}", p.name);
        }
    }

    #[test]
    fn modifiers_apply() {
        let mut g = Graph::default();
        let out = g.add(NodeKind::Output, [0.0; 2]);
        let src = g.add(
            NodeKind::Source {
                layer: Layer::default(),
            },
            [0.0; 2],
        );
        let arr = g.add(
            NodeKind::Array {
                count: 3,
                step: [1.0, 0.0, 0.0],
                rotate_y: 0.0,
            },
            [0.0; 2],
        );
        let sym = g.add(
            NodeKind::Symmetry {
                symmetry: Symmetry::MirrorX,
            },
            [0.0; 2],
        );
        g.connect(src, 0, arr, 0);
        g.connect(arr, 0, sym, 0);
        g.connect(sym, 0, out, 0);
        let layers = g.compile();
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[2].transform.position[0], 2.0);
        assert!(layers.iter().all(|l| l.symmetry == Symmetry::MirrorX));
    }

    #[test]
    fn mirror_and_strobe_nodes() {
        let mut g = Graph::default();
        let out = g.add(NodeKind::Output, [0.0; 2]);
        let mut layer = Layer::default()
            .at([2.0, 1.0, 0.0])
            .rotated([10.0, 20.0, 30.0]);
        layer.transform.spin = [1, 2, 3];
        let src = g.add(NodeKind::Source { layer }, [0.0; 2]);
        let mir = g.add(NodeKind::Mirror { axis: 0, at: 0.5 }, [0.0; 2]);
        let strobe = g.add(
            NodeKind::Strobe {
                blink: Blink {
                    mode: BlinkMode::Blink,
                    ..Default::default()
                },
            },
            [0.0; 2],
        );
        g.connect(src, 0, mir, 0);
        g.connect(mir, 0, strobe, 0);
        g.connect(strobe, 0, out, 0);
        let layers = g.compile();
        assert_eq!(layers.len(), 2);
        let m = &layers[1].transform;
        assert_eq!(m.position, [-1.0, 1.0, 0.0]);
        assert_eq!(m.stretch, [-1.0, 1.0, 1.0]);
        assert_eq!(m.rotation, [10.0, -20.0, -30.0]);
        assert_eq!(m.spin, [1, -2, -3]);
        assert!(layers.iter().all(|l| l.blink.mode == BlinkMode::Blink));
    }

    #[test]
    fn mirror_matches_world_reflection() {
        use crate::eval::layer_matrix;
        use glam::{Mat4, Vec3};
        let mut l = Layer::default()
            .at([1.0, 2.0, 3.0])
            .rotated([15.0, 40.0, -25.0]);
        l.transform.spin = [1, 1, 2];
        let ctx = crate::EvalCtx::new(&Default::default(), 0.3, None);
        for axis in 0..3 {
            let mut s = Vec3::ONE;
            s[axis] = -1.0;
            let want = Mat4::from_scale(s) * layer_matrix(&l.transform, &ctx);
            let got = layer_matrix(&mirror_layer(l.clone(), axis, 0.0).transform, &ctx);
            assert!(want.abs_diff_eq(got, 1e-4), "axis {axis}");
        }
    }

    #[test]
    fn cycles_do_not_hang() {
        let mut g = Graph::default();
        let out = g.add(NodeKind::Output, [0.0; 2]);
        let a = g.add(NodeKind::Merge, [0.0; 2]);
        let b = g.add(NodeKind::Merge, [0.0; 2]);
        g.connect(a, 0, b, 0);
        g.connect(b, 0, a, 0);
        g.connect(a, 0, out, 0);
        assert!(g.compile().is_empty());
    }
}
