//! Advanced mode: the node graph editor (egui-snarl), mirroring
//! `ez_core::graph::Graph`.

use egui::{Color32, Ui};
use egui_snarl::ui::{PinInfo, SnarlStyle, SnarlViewer};
use egui_snarl::{InPin, InPinId, NodeId, OutPin, OutPinId, Snarl};
use ez_core::graph::{Graph, GraphNode, NodeKind, Wire};
use ez_core::*;

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

    pub fn show(&mut self, ui: &mut Ui, templates: &[Layer]) {
        let mut viewer = Viewer {
            selected: &mut self.selected,
            templates,
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
}

fn pin_color(kind: &NodeKind) -> Color32 {
    match kind {
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
        if kind.inputs() > 1 {
            ui.label(format!("in {}", pin.id.input + 1));
        } else {
            ui.label("layers");
        }
        PinInfo::circle().with_fill(pin_color(kind))
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut Ui,
        snarl: &mut Snarl<NodeKind>,
    ) -> impl egui_snarl::ui::SnarlPin + 'static {
        let kind = &snarl[pin.id.node];
        ui.label("layers");
        PinInfo::circle().with_fill(pin_color(kind))
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, snarl: &mut Snarl<NodeKind>) {
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
                NodeKind::Merge | NodeKind::Output => {}
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
