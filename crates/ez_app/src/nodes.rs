//! Advanced mode: the node graph editor (egui-snarl), mirroring
//! `ez_core::graph::Graph`.

use egui::{Color32, Ui};
use egui_snarl::ui::{PinInfo, SnarlStyle, SnarlViewer};
use egui_snarl::{InPin, InPinId, NodeId, OutPin, OutPinId, Snarl};
use ez_core::graph::{Graph, GraphNode, NodeKind, PinKind, Wire};
use ez_core::signal::{DriveMode, MathOp, SignalNode};
use ez_core::*;

const SIGNAL_COLOR: Color32 = Color32::from_rgb(120, 240, 140);

pub struct NodeEditor {
    pub snarl: Snarl<NodeKind>,
    pub selected: Option<NodeId>,
    style: SnarlStyle,
}

impl NodeEditor {
    pub fn from_graph(g: &Graph) -> NodeEditor {
        let mut snarl = Snarl::new();
        let mut map = std::collections::HashMap::new();
        for n in &g.nodes {
            let id = snarl.insert_node(egui::pos2(n.pos[0], n.pos[1]), n.kind.clone());
            map.insert(n.id, id);
        }
        for w in &g.wires {
            if let (Some(a), Some(b)) = (map.get(&w.from), map.get(&w.to)) {
                snarl.connect(
                    OutPinId {
                        node: *a,
                        output: w.from_pin,
                    },
                    InPinId {
                        node: *b,
                        input: w.to_pin,
                    },
                );
            }
        }
        NodeEditor {
            snarl,
            selected: None,
            style: SnarlStyle::new(),
        }
    }

    pub fn to_graph(&self) -> Graph {
        let nodes = self
            .snarl
            .nodes_pos_ids()
            .map(|(id, pos, kind)| GraphNode {
                id: id.0 as u32,
                pos: [pos.x, pos.y],
                kind: kind.clone(),
            })
            .collect();
        let wires = self
            .snarl
            .wires()
            .map(|(o, i)| Wire {
                from: o.node.0 as u32,
                from_pin: o.output,
                to: i.node.0 as u32,
                to_pin: i.input,
            })
            .collect();
        Graph { nodes, wires }
    }

    /// `ctx_at` gives the moment at a loop phase (for the signal
    /// previews); `now` is the current phase.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        templates: &[Layer],
        ctx_at: &dyn Fn(f32) -> EvalCtx,
        now: f32,
    ) {
        let graph = self.to_graph();
        let mut viewer = Viewer {
            selected: &mut self.selected,
            templates,
            graph,
            ctx_at,
            now,
        };
        self.snarl.show(&mut viewer, &self.style, "ez2-graph", ui);
    }

    /// Layer of the selected source node, for the inspector.
    pub fn selected_layer_mut(&mut self) -> Option<(u32, &mut Layer)> {
        let id = self.selected?;
        match self.snarl.get_node_mut(id)? {
            NodeKind::Source { layer } => Some((id.0 as u32, layer)),
            _ => None,
        }
    }

    /// Layer of source node `id` (for applying imported files).
    pub fn layer_mut(&mut self, id: u32) -> Option<&mut Layer> {
        match self.snarl.get_node_mut(NodeId(id as usize))? {
            NodeKind::Source { layer } => Some(layer),
            _ => None,
        }
    }
}

struct Viewer<'a> {
    selected: &'a mut Option<NodeId>,
    templates: &'a [Layer],
    /// The graph as it was at the start of this frame.
    graph: Graph,
    ctx_at: &'a dyn Fn(f32) -> EvalCtx,
    now: f32,
}

fn pin_color(kind: &NodeKind) -> Color32 {
    match kind {
        NodeKind::Signal { .. } => SIGNAL_COLOR,
        NodeKind::Source { .. } => Color32::from_rgb(90, 200, 255),
        NodeKind::Output => Color32::from_rgb(255, 70, 140),
        _ => Color32::from_rgb(255, 190, 60),
    }
}

impl SnarlViewer<NodeKind> for Viewer<'_> {
    fn title(&mut self, node: &NodeKind) -> String {
        node.title()
    }

    fn inputs(&mut self, node: &NodeKind) -> usize {
        node.inputs()
    }

    fn outputs(&mut self, node: &NodeKind) -> usize {
        node.outputs()
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut Ui,
        snarl: &mut Snarl<NodeKind>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let kind = &snarl[pin.id.node];
        let i = pin.id.input;
        match kind {
            NodeKind::Merge | NodeKind::Output => {
                ui.label(format!("in {}", i + 1));
            }
            _ => {
                ui.label(kind.input_name(i));
            }
        }
        let fill = match kind.input_kind(i) {
            PinKind::Signal => SIGNAL_COLOR,
            PinKind::Layers => pin_color(kind),
        };
        PinInfo::circle().with_fill(fill)
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut Ui,
        snarl: &mut Snarl<NodeKind>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let kind = &snarl[pin.id.node];
        match kind.output_kind() {
            PinKind::Signal => {
                ui.label("signal");
                PinInfo::square().with_fill(SIGNAL_COLOR)
            }
            PinKind::Layers => {
                ui.label("layers");
                PinInfo::circle().with_fill(pin_color(kind))
            }
        }
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<NodeKind>) {
        // Signals only go into signal pins, layers into layer pins.
        if snarl[from.id.node].output_kind() != snarl[to.id.node].input_kind(to.id.input) {
            return;
        }
        // One wire per input pin.
        snarl.drop_inputs(to.id);
        snarl.connect(from.id, to.id);
    }

    fn has_body(&mut self, node: &NodeKind) -> bool {
        !matches!(node, NodeKind::Merge | NodeKind::Output)
    }

    fn show_body(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<NodeKind>,
    ) {
        let is_selected = *self.selected == Some(node);
        ui.vertical(|ui| {
            ui.set_max_width(190.0);
            match &mut snarl[node] {
                NodeKind::Source { layer } => {
                    ui.checkbox(&mut layer.enabled, "visible");
                    if ui
                        .selectable_label(is_selected, "✏ edit in inspector")
                        .clicked()
                    {
                        *self.selected = Some(node);
                    }
                }
                NodeKind::Symmetry { symmetry } => {
                    let mut count = match symmetry {
                        Symmetry::Radial { count } | Symmetry::Kaleido { count } => *count,
                        _ => 0,
                    };
                    egui::ComboBox::from_id_salt(("sym", node.0))
                        .selected_text(symmetry.label())
                        .show_ui(ui, |ui| {
                            for o in [
                                Symmetry::None,
                                Symmetry::MirrorX,
                                Symmetry::MirrorZ,
                                Symmetry::MirrorXZ,
                                Symmetry::Radial {
                                    count: count.max(2),
                                },
                                Symmetry::Kaleido {
                                    count: count.max(2),
                                },
                            ] {
                                if ui.selectable_label(*symmetry == o, o.label()).clicked() {
                                    *symmetry = o;
                                }
                            }
                        });
                    if matches!(symmetry, Symmetry::Radial { .. } | Symmetry::Kaleido { .. })
                        && ui
                            .add(
                                egui::DragValue::new(&mut count)
                                    .range(1..=64)
                                    .prefix("copies "),
                            )
                            .changed()
                    {
                        *symmetry = match symmetry {
                            Symmetry::Radial { .. } => Symmetry::Radial { count },
                            _ => Symmetry::Kaleido { count },
                        };
                    }
                }
                NodeKind::Offset { offset } => {
                    for (i, a) in ["x", "y", "z"].iter().enumerate() {
                        ui.add(
                            egui::DragValue::new(&mut offset[i])
                                .speed(0.05)
                                .prefix(format!("{a} ")),
                        );
                    }
                }
                NodeKind::Spin { turns } => {
                    for (i, a) in ["x", "y", "z"].iter().enumerate() {
                        ui.add(
                            egui::DragValue::new(&mut turns[i])
                                .range(-16..=16)
                                .speed(0.1)
                                .prefix(format!("{a} turns ")),
                        );
                    }
                }
                NodeKind::Scale { factor } => {
                    ui.add(
                        egui::DragValue::new(factor)
                            .speed(0.01)
                            .range(0.0..=100.0)
                            .prefix("× "),
                    );
                }
                NodeKind::Tint { hue, glow } => {
                    ui.add(egui::Slider::new(hue, -0.5..=0.5).text("hue"));
                    ui.add(egui::Slider::new(glow, 0.0..=4.0).text("glow"));
                }
                NodeKind::Array {
                    count,
                    step,
                    rotate_y,
                } => {
                    ui.add(egui::DragValue::new(count).range(1..=64).prefix("copies "));
                    for (i, a) in ["x", "y", "z"].iter().enumerate() {
                        ui.add(
                            egui::DragValue::new(&mut step[i])
                                .speed(0.05)
                                .prefix(format!("step {a} ")),
                        );
                    }
                    ui.add(egui::DragValue::new(rotate_y).speed(0.5).suffix("° each"));
                }
                NodeKind::Jitter {
                    position,
                    rotation,
                    per_loop,
                    seed,
                } => {
                    ui.push_id(("jitter", node.0), |ui| {
                        crate::widgets::param(ui, "Move", "", position, 0.0..=10.0);
                        crate::widgets::param(ui, "Turn °", "", rotation, 0.0..=180.0);
                    });
                    ui.add(
                        egui::DragValue::new(per_loop)
                            .range(0..=256)
                            .prefix("re-roll ")
                            .suffix(" ×/loop"),
                    )
                    .on_hover_text(
                        "0 = scatter once. Above 0 = shake: a new random direction this many \
                         times per loop (use ~ on Move/Turn for fades, e.g. Exp fade out every beat)",
                    );
                    ui.add(egui::DragValue::new(seed).range(0..=9999).prefix("seed "));
                }
                NodeKind::Mirror { axis, at } => {
                    ui.horizontal(|ui| {
                        for (i, a) in ["X", "Y", "Z"].iter().enumerate() {
                            ui.selectable_value(axis, i as u8, *a);
                        }
                    });
                    ui.add(egui::DragValue::new(at).speed(0.05).prefix("plane at "));
                }
                NodeKind::Strobe { blink } => {
                    ui.push_id(("strobe", node.0), |ui| {
                        crate::inspector::blink_ui(ui, blink)
                    });
                }
                NodeKind::Material {
                    color,
                    glow,
                    texture,
                    wireframe,
                    glitch,
                    glitch_style,
                } => {
                    ui.horizontal(|ui| {
                        let mut on = color.is_some();
                        if ui.checkbox(&mut on, "colour").changed() {
                            *color = on.then_some([0.0, 0.9, 1.0]);
                        }
                        if let Some(c) = color {
                            ui.color_edit_button_rgb(c);
                        }
                    });
                    ui.add(egui::Slider::new(glow, 0.0..=4.0).text("glow ×"));
                    egui::ComboBox::from_id_salt(("mat_tex", node.0))
                        .selected_text(texture.as_deref().unwrap_or("texture: keep"))
                        .height(300.0)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(texture.is_none(), "keep").clicked() {
                                *texture = None;
                            }
                            for (name, desc) in ez_render::texgen::BUILTIN {
                                if ui
                                    .selectable_label(texture.as_deref() == Some(*name), *name)
                                    .on_hover_text(*desc)
                                    .clicked()
                                {
                                    *texture = Some(name.to_string());
                                }
                            }
                        });
                    ui.checkbox(wireframe, "neon wireframe");
                    ui.add(egui::Slider::new(glitch, 0.0..=2.0).text("glitch"));
                    if *glitch > 0.0 {
                        egui::ComboBox::from_id_salt(("mat_glitch", node.0))
                            .selected_text(glitch_style.label())
                            .show_ui(ui, |ui| {
                                for g in GlitchStyle::ALL {
                                    ui.selectable_value(glitch_style, g, g.label());
                                }
                            });
                    }
                }
                NodeKind::Signal { sig } => {
                    ui.push_id(("signal", node.0), |ui| signal_ui(ui, sig));
                }
                NodeKind::Colors { ramp } => {
                    ui.push_id(("ramp", node.0), |ui| {
                        ui.checkbox(&mut ramp.enabled, "on");
                        crate::inspector::ramp_ui(ui, ramp)
                    });
                }
                NodeKind::FollowCurve {
                    curve,
                    freq,
                    size,
                    count,
                    laps,
                    align,
                } => {
                    ui.add(egui::DragValue::new(count).range(1..=4096).prefix("copies "));
                    ui.add(
                        egui::DragValue::new(laps)
                            .range(-16..=16)
                            .prefix("laps ")
                            .suffix(" / loop"),
                    );
                    ui.checkbox(align, "face along");
                    ui.label(
                        egui::RichText::new("Wire a ribbon into “ribbon” to follow it, or:")
                            .small()
                            .weak(),
                    );
                    egui::ComboBox::from_id_salt(("curve", node.0))
                        .selected_text(curve.label())
                        .show_ui(ui, |ui| {
                            for c in RibbonCurve::ALL {
                                ui.selectable_value(curve, c, c.label());
                            }
                        });
                    ui.horizontal(|ui| {
                        for f in freq.iter_mut() {
                            ui.add(egui::DragValue::new(f).range(1..=16).speed(0.05));
                        }
                    });
                    ui.add(egui::DragValue::new(size).range(0.1..=40.0).speed(0.05).prefix("size "));
                }
                NodeKind::OnSurface {
                    count,
                    seed,
                    align,
                    lift,
                } => {
                    ui.add(egui::DragValue::new(count).range(1..=5000).prefix("copies "));
                    ui.add(egui::DragValue::new(seed).range(0..=9999).prefix("seed "));
                    ui.checkbox(align, "stand up");
                    ui.add(egui::Slider::new(lift, -2.0..=4.0).text("lift"));
                    ui.label(
                        egui::RichText::new("Wire the shape to cover into “surface”")
                            .small()
                            .weak(),
                    );
                }
                NodeKind::OnTerrain {
                    count,
                    seed,
                    align,
                    lift,
                } => {
                    ui.add(egui::DragValue::new(count).range(1..=5000).prefix("copies "));
                    ui.add(egui::DragValue::new(seed).range(0..=9999).prefix("seed "));
                    ui.checkbox(align, "follow the slope");
                    ui.add(egui::Slider::new(lift, -2.0..=10.0).text("lift"));
                    ui.label(
                        egui::RichText::new("Wire the terrain into “terrain”")
                            .small()
                            .weak(),
                    );
                }
                NodeKind::Deform { deform } => {
                    ui.push_id(("deform", node.0), |ui| {
                        crate::inspector::deform_ui(ui, deform)
                    });
                }
                NodeKind::Drive { path, mode } => {
                    let layers = self.graph.upstream_layers(node.0 as u32, 0);
                    let mut paths: Vec<String> = layers
                        .iter()
                        .flat_map(ez_core::signal::setting_paths)
                        .collect();
                    paths.sort();
                    paths.dedup();
                    egui::ComboBox::from_id_salt(("drive_path", node.0))
                        .selected_text(if path.is_empty() {
                            "pick a setting"
                        } else {
                            path.as_str()
                        })
                        .height(400.0)
                        .show_ui(ui, |ui| {
                            if paths.is_empty() {
                                ui.label("Connect layers first");
                            }
                            for p in &paths {
                                if ui.selectable_label(path == p, p).clicked() {
                                    *path = p.clone();
                                }
                            }
                        })
                        .response
                        .on_hover_text(
                            "The setting of every incoming layer that the signal sets. \
                             kind.… are the layer's own settings, transform.… its placement \
                             (.0 .1 .2 = x y z or red green blue)",
                        );
                    egui::ComboBox::from_id_salt(("drive_mode", node.0))
                        .selected_text(mode.label())
                        .show_ui(ui, |ui| {
                            for m in DriveMode::ALL {
                                ui.selectable_value(mode, m, m.label());
                            }
                        });
                    if !path.is_empty() && !paths.is_empty() && !paths.contains(path) {
                        ui.colored_label(Color32::LIGHT_RED, "no incoming layer has this setting");
                    }
                }
                NodeKind::Merge | NodeKind::Output => {}
            }
            if matches!(snarl[node], NodeKind::Signal { .. }) {
                sparkline(ui, &self.graph, node.0 as u32, self.ctx_at, self.now);
            }
        });
    }

    fn has_graph_menu(&mut self, _pos: egui::Pos2, _snarl: &mut Snarl<NodeKind>) -> bool {
        true
    }

    fn show_graph_menu(&mut self, pos: egui::Pos2, ui: &mut Ui, snarl: &mut Snarl<NodeKind>) {
        ui.label("Add node");
        if let Some(layer) = crate::inspector::add_layer_menu(ui, self.templates) {
            let id = snarl.insert_node(pos, NodeKind::Source { layer });
            *self.selected = Some(id);
            ui.close();
        }
        ui.menu_button("Signal", |ui| {
            for sig in SignalNode::templates() {
                if ui
                    .button(sig.title())
                    .on_hover_text(signal_help(&sig))
                    .clicked()
                {
                    snarl.insert_node(pos, NodeKind::Signal { sig });
                    ui.close();
                }
            }
        });
        ui.separator();
        for m in NodeKind::modifier_templates() {
            if ui.button(m.title()).clicked() {
                snarl.insert_node(pos, m);
                ui.close();
            }
        }
        if !snarl
            .nodes_ids_data()
            .any(|(_, n)| matches!(n.value, NodeKind::Output))
            && ui.button("Output").clicked()
        {
            snarl.insert_node(pos, NodeKind::Output);
            ui.close();
        }
    }

    fn has_node_menu(&mut self, _node: &NodeKind) -> bool {
        true
    }

    fn show_node_menu(
        &mut self,
        node: NodeId,
        _inputs: &[InPin],
        _outputs: &[OutPin],
        ui: &mut Ui,
        snarl: &mut Snarl<NodeKind>,
    ) {
        if ui.button("Duplicate").clicked() {
            let kind = snarl[node].clone();
            let pos = snarl
                .get_node_info(node)
                .map(|n| n.pos + egui::vec2(30.0, 30.0))
                .unwrap_or_default();
            snarl.insert_node(pos, kind);
            ui.close();
        }
        if ui.button("Delete").clicked() {
            snarl.remove_node(node);
            if *self.selected == Some(node) {
                *self.selected = None;
            }
            ui.close();
        }
    }
}

fn signal_help(sig: &SignalNode) -> &'static str {
    match sig {
        SignalNode::Wave { .. } => {
            "A value that moves: use ~ for waves, beat fades, random steps or the music"
        }
        SignalNode::Math { .. } => "Adds, multiplies or compares two signals",
        SignalNode::Remap { .. } => "Maps a range of values to another range",
        SignalNode::Quantize { .. } => "Rounds to steps",
        SignalNode::Smooth { .. } => "Averages over a few beats (waves and steps)",
        SignalNode::Mix { .. } => "Blends A to B by T",
        SignalNode::Sequence { .. } => "Steps through a list of values every few beats",
        SignalNode::Counter { .. } => "Counts kicks (or other hits) in the loop: 0, 1, 2…",
    }
}

fn signal_ui(ui: &mut Ui, sig: &mut SignalNode) {
    let num = |ui: &mut Ui, v: &mut f32, label: &str| {
        ui.add(
            egui::DragValue::new(v)
                .speed(0.01)
                .prefix(format!("{label} ")),
        );
    };
    match sig {
        SignalNode::Wave { param } => {
            crate::widgets::param(ui, "Value", "", param, -1.0..=1.0);
        }
        SignalNode::Math { op, a, b } => {
            egui::ComboBox::from_id_salt("op")
                .selected_text(op.label())
                .show_ui(ui, |ui| {
                    for o in MathOp::ALL {
                        ui.selectable_value(op, o, o.label());
                    }
                });
            ui.horizontal(|ui| {
                num(ui, a, "A");
                num(ui, b, "B");
            })
            .response
            .on_hover_text("Used when the pin isn't connected");
        }
        SignalNode::Remap {
            in_min,
            in_max,
            out_min,
            out_max,
            clamp,
        } => {
            ui.horizontal(|ui| {
                num(ui, in_min, "from");
                num(ui, in_max, "…");
            });
            ui.horizontal(|ui| {
                num(ui, out_min, "to");
                num(ui, out_max, "…");
            });
            ui.checkbox(clamp, "stay in range");
        }
        SignalNode::Quantize { steps } => {
            ui.add(
                egui::DragValue::new(steps)
                    .range(1..=64)
                    .suffix(" steps per 1"),
            );
        }
        SignalNode::Smooth { beats } => {
            ui.add(
                egui::DragValue::new(beats)
                    .range(0.0..=16.0)
                    .speed(0.05)
                    .suffix(" beats"),
            );
        }
        SignalNode::Mix { a, b } => {
            ui.horizontal(|ui| {
                num(ui, a, "A");
                num(ui, b, "B");
            });
        }
        SignalNode::Sequence {
            values,
            beats,
            glide,
        } => {
            ui.add(
                egui::DragValue::new(beats)
                    .range(1..=64)
                    .prefix("every ")
                    .suffix(" beats"),
            );
            let mut remove = None;
            for (i, v) in values.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    num(ui, v, &format!("{}", i + 1));
                    if ui.small_button("✕").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                values.remove(i);
            }
            if values.len() < 16 && ui.small_button("+ value").clicked() {
                values.push(values.last().copied().unwrap_or(0.0));
            }
            ui.checkbox(glide, "glide");
        }
        SignalNode::Counter { hit, modulo } => {
            let kinds = [
                (ez_core::audio::HitKind::Kick, "kicks"),
                (ez_core::audio::HitKind::Snare, "snares"),
                (ez_core::audio::HitKind::Hats, "hi-hats"),
                (ez_core::audio::HitKind::Any, "any hit"),
                (ez_core::audio::HitKind::Note, "notes"),
            ];
            egui::ComboBox::from_id_salt("hit")
                .selected_text(kinds.iter().find(|k| k.0 == *hit).map_or("", |k| k.1))
                .show_ui(ui, |ui| {
                    for (k, label) in kinds {
                        ui.selectable_value(hit, k, label);
                    }
                });
            ui.add(
                egui::DragValue::new(modulo)
                    .range(1..=64)
                    .prefix("count to "),
            );
        }
    }
}

/// The node's value over the loop, with the current moment marked.
fn sparkline(ui: &mut Ui, graph: &Graph, id: u32, ctx_at: &dyn Fn(f32) -> EvalCtx, now: f32) {
    const N: usize = 96;
    let vals: Vec<f32> = (0..N)
        .map(|i| {
            graph
                .signal_at(id, &ctx_at(i as f32 / N as f32))
                .unwrap_or(0.0)
        })
        .collect();
    let current = graph.signal_at(id, &ctx_at(now)).unwrap_or(0.0);
    let (lo, hi) = vals
        .iter()
        .fold((current, current), |(a, b), v| (a.min(*v), b.max(*v)));
    let (w, h) = (180.0, 36.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 3.0, Color32::from_gray(24));
    let span = (hi - lo).max(1e-6);
    let y = |v: f32| rect.bottom() - 3.0 - (v - lo) / span * (h - 6.0);
    let pts: Vec<egui::Pos2> = vals
        .iter()
        .enumerate()
        .map(|(i, v)| egui::pos2(rect.left() + i as f32 / (N - 1) as f32 * w, y(*v)))
        .collect();
    p.add(egui::Shape::line(pts, egui::Stroke::new(1.5, SIGNAL_COLOR)));
    let x = rect.left() + now.rem_euclid(1.0) * w;
    p.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        egui::Stroke::new(1.0, Color32::from_rgb(255, 70, 140)),
    );
    ui.label(
        egui::RichText::new(format!("now {current:.2}   range {lo:.2} … {hi:.2}"))
            .small()
            .weak(),
    );
}
