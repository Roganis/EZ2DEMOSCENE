//! Node graph ("advanced mode").
//!
//! Nodes produce and transform *streams of layers*. The graph compiles down
//! to a plain `Vec<Layer>`, so the renderer never needs to know whether a
//! scene was made in simple mode or with nodes.

use crate::color::hue_rotate;
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
        }
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
