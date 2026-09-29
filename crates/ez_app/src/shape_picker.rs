//! The shape picker: a window of tiles with a rendered preview of every
//! built-in shape and every model of the bundled library.

use super::EzApp;
use crate::inspector::{self, ShapeSlot};
use crate::platform::{self, LayerRef, Purpose};
use egui::{Color32, RichText};
use ez_core::{LayerKind, MeshSource, Primitive, SdfShape, TextFont};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    BuiltIn,
    Library,
}

/// Kept between openings, so it comes back on the same tab, category and
/// search.
pub struct ShapePicker {
    /// The layer being edited while the window is open.
    target: Option<(LayerRef, ShapeSlot)>,
    tab: Tab,
    /// Library category shown (`None`: all).
    category: Option<String>,
    search: String,
}

/// One tile: what it sets, its name and its thumbnail key.
struct Tile {
    source: MeshSource,
    name: String,
    key: String,
    hint: Option<String>,
}

impl Default for ShapePicker {
    fn default() -> Self {
        ShapePicker {
            target: None,
            tab: Tab::BuiltIn,
            category: None,
            search: String::new(),
        }
    }
}

const TILE_GAP: f32 = 6.0;
const LABEL_H: f32 = 30.0;

fn builtin_tiles() -> Vec<Tile> {
    let mut out: Vec<Tile> = Primitive::all_defaults()
        .into_iter()
        .map(|p| Tile {
            name: p.label().to_string(),
            key: format!("p:{}", p.cache_key()),
            source: MeshSource::Primitive(p),
            hint: None,
        })
        .collect();
    for f in SdfShape::all_defaults() {
        out.push(Tile {
            name: f.label().to_string(),
            key: format!("s:{}", f.index()),
            source: MeshSource::Sdf { form: f, cycles: 1 },
            hint: Some(
                "Raymarched: smooth, organic surfaces worked out per pixel (heavier than a mesh)"
                    .into(),
            ),
        });
    }
    out.push(Tile {
        name: "3D text".into(),
        key: "t".into(),
        source: MeshSource::Text {
            text: "EZ2".into(),
            font: TextFont::Sans,
            font_file: None,
            depth: 0.3,
        },
        hint: Some("Solid letters: a logo with every material, relief and copy option".into()),
    });
    out.push(Tile {
        name: "Cloth".into(),
        key: "c".into(),
        source: MeshSource::Cloth {
            cloth: Box::new(ez_core::sim::Cloth {
                size: [1.6, 1.1],
                ..Default::default()
            }),
            mesh: None,
        },
        hint: Some(
            "A flag, curtain, banner or drape, blown by a looping wind (simulated ahead of time)"
                .into(),
        ),
    });
    out
}

/// Whether two sources are "the same shape" for highlighting the current one
/// (a primitive with other settings still counts).
fn same_shape(a: &MeshSource, b: &MeshSource) -> bool {
    match (a, b) {
        (MeshSource::Primitive(p), MeshSource::Primitive(q)) => {
            std::mem::discriminant(p) == std::mem::discriminant(q)
        }
        (MeshSource::Sdf { form: f, .. }, MeshSource::Sdf { form: g, .. }) => {
            f.index() == g.index()
        }
        (MeshSource::Text { .. }, MeshSource::Text { .. }) => true,
        (MeshSource::Cloth { .. }, MeshSource::Cloth { .. }) => true,
        _ => a == b,
    }
}

impl EzApp {
    /// Open the picker when a shape button asked for it.
    pub(super) fn poll_shape_picker_request(&mut self, ctx: &egui::Context) {
        let asked = ctx.data_mut(|d| {
            d.remove_temp::<Option<(LayerRef, ShapeSlot)>>(egui::Id::new(inspector::SHAPE_PICKER))
        });
        if let Some(target) = asked.flatten() {
            if matches!(self.current_shape(target), Some(MeshSource::Library { .. })) {
                self.shape_picker.tab = Tab::Library;
            }
            self.shape_picker.target = Some(target);
        }
    }

    fn current_shape(&mut self, (layer, slot): (LayerRef, ShapeSlot)) -> Option<MeshSource> {
        match self.layer_for(layer).map(|l| &l.kind) {
            Some(LayerKind::Mesh(m)) => Some(match slot {
                ShapeSlot::Main => m.source.clone(),
                ShapeSlot::MorphTarget => m.morph.target.clone(),
            }),
            _ => None,
        }
    }

    pub(super) fn shape_picker_window(&mut self, ctx: &egui::Context) {
        let Some(target) = self.shape_picker.target else {
            self.viewport.clear_shape_thumb_queue();
            return;
        };
        // The layer went away (deleted, other project): nothing to pick for.
        let Some(current) = self.current_shape(target) else {
            self.shape_picker.target = None;
            return;
        };
        let mut picker = std::mem::take(&mut self.shape_picker);
        let screen = ctx.content_rect().size();
        let width = (screen.x - 24.0).min(760.0);
        let height = (screen.y - 90.0).clamp(240.0, 620.0);
        let mut open = true;
        let mut chosen: Option<MeshSource> = None;
        let mut pick_file = false;
        let title = match target.1 {
            ShapeSlot::Main => "Choose a shape",
            ShapeSlot::MorphTarget => "Morph into…",
        };
        egui::Window::new(title)
            .id(egui::Id::new("shape_picker"))
            .open(&mut open)
            .collapsible(false)
            .fixed_size([width, height])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut picker.tab, Tab::BuiltIn, "Built-in");
                    // Only the Library tab loads the library (a download on the web).
                    let count = ez_core::models::loaded().map(|l| l.entries.len());
                    let label = match count {
                        Some(n) => format!("Library ({n})"),
                        None => "Library".into(),
                    };
                    ui.selectable_value(&mut picker.tab, Tab::Library, label);
                    if picker.tab == Tab::Library {
                        ui.add(
                            egui::TextEdit::singleline(&mut picker.search)
                                .hint_text("🔍 Search")
                                .desired_width(160.0),
                        );
                    }
                });
                ui.separator();
                match picker.tab {
                    Tab::BuiltIn => {
                        let tiles = builtin_tiles();
                        let footer = 34.0;
                        let h = ui.available_height() - footer;
                        if let Some(s) = self.tile_grid(ui, &tiles, &current, h) {
                            chosen = Some(s);
                        }
                        ui.separator();
                        if ui
                            .button("📂 3D model file (glTF / OBJ)…")
                            .on_hover_text("Use a model of your own")
                            .clicked()
                        {
                            pick_file = true;
                        }
                    }
                    Tab::Library => match ez_core::models::library() {
                        None => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Loading the model library…");
                            });
                        }
                        Some(lib) => {
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .selectable_label(picker.category.is_none(), "All")
                                    .clicked()
                                {
                                    picker.category = None;
                                }
                                for c in lib.categories() {
                                    let on = picker.category.as_deref() == Some(c);
                                    if ui.selectable_label(on, c).clicked() {
                                        picker.category = Some(c.to_string());
                                    }
                                }
                            });
                            ui.add_space(4.0);
                            let words: Vec<String> = picker
                                .search
                                .to_lowercase()
                                .split_whitespace()
                                .map(String::from)
                                .collect();
                            let gen = ez_core::models::generation();
                            let tiles: Vec<Tile> = lib
                                .entries
                                .iter()
                                .filter(|e| {
                                    picker.category.as_deref().is_none_or(|c| c == e.category)
                                })
                                .filter(|e| {
                                    let hay = format!("{} {}", e.name, e.category).to_lowercase();
                                    words.iter().all(|w| hay.contains(w.as_str()))
                                })
                                .map(|e| Tile {
                                    source: MeshSource::Library { id: e.id.clone() },
                                    name: e.name.clone(),
                                    key: format!("l:{}:{gen}", e.id),
                                    hint: Some(format!("{} · {} triangles", e.category, e.tris)),
                                })
                                .collect();
                            if tiles.is_empty() {
                                ui.label(RichText::new("No model matches.").weak());
                            } else {
                                let h = ui.available_height() - 18.0;
                                if let Some(s) = self.tile_grid(ui, &tiles, &current, h) {
                                    chosen = Some(s);
                                }
                            }
                            ui.label(
                                RichText::new("Models by Kenney (kenney.nl), CC0.")
                                    .weak()
                                    .small(),
                            );
                        }
                    },
                }
            });
        if self.viewport.render_shape_thumbs() {
            ctx.request_repaint();
        }
        let (layer, slot) = target;
        if pick_file {
            platform::pick(match slot {
                ShapeSlot::Main => Purpose::SetModel(layer),
                ShapeSlot::MorphTarget => Purpose::SetMorphModel(layer),
            });
        }
        if let Some(source) = chosen {
            if let Some(LayerKind::Mesh(m)) = self.layer_for(layer).map(|l| &mut l.kind) {
                match slot {
                    ShapeSlot::Main => m.source = source,
                    ShapeSlot::MorphTarget => m.morph.target = source,
                }
            }
            open = false;
        }
        if !open {
            picker.target = None;
            self.viewport.clear_shape_thumb_queue();
        }
        self.shape_picker = picker;
    }

    /// A scrolling grid of tiles filling `height`; returns the clicked one.
    fn tile_grid(
        &mut self,
        ui: &mut egui::Ui,
        tiles: &[Tile],
        current: &MeshSource,
        height: f32,
    ) -> Option<MeshSource> {
        let avail = ui.available_width();
        // Tiles between 84 and 128 px, as many columns as fit.
        let cols = ((avail + TILE_GAP) / (96.0 + TILE_GAP)).floor().max(2.0) as usize;
        let side = ((avail - TILE_GAP * (cols as f32 - 1.0)) / cols as f32 - 1.0).min(128.0);
        let row_h = side + LABEL_H + TILE_GAP;
        let rows = tiles.len().div_ceil(cols);
        let mut chosen = None;
        egui::ScrollArea::vertical()
            .max_height(height.max(80.0))
            .auto_shrink([false, false])
            .show_rows(ui, row_h, rows, |ui, range| {
                for r in range {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = TILE_GAP;
                        for t in tiles.iter().skip(r * cols).take(cols) {
                            if self.tile(ui, t, side, same_shape(&t.source, current)) {
                                chosen = Some(t.source.clone());
                            }
                        }
                    });
                    ui.add_space(TILE_GAP);
                }
            });
        chosen
    }

    fn tile(&mut self, ui: &mut egui::Ui, t: &Tile, side: f32, selected: bool) -> bool {
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(side, side + LABEL_H), egui::Sense::click());
        if !ui.is_rect_visible(rect) {
            return false;
        }
        let pic = egui::Rect::from_min_size(rect.min, egui::vec2(side, side));
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        painter.rect_filled(pic, 6.0, visuals.extreme_bg_color);
        match self.viewport.shape_thumb(&t.key, &t.source) {
            Some(tex) => {
                painter.image(
                    tex,
                    pic.shrink(1.0),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                painter.text(
                    pic.center(),
                    egui::Align2::CENTER_CENTER,
                    "…",
                    egui::FontId::proportional(18.0),
                    visuals.weak_text_color(),
                );
            }
        }
        let stroke = if selected {
            egui::Stroke::new(2.0, crate::widgets::ACCENT)
        } else if resp.hovered() {
            egui::Stroke::new(1.0, visuals.strong_text_color())
        } else {
            egui::Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color)
        };
        painter.rect_stroke(pic, 6.0, stroke, egui::StrokeKind::Inside);
        let galley = painter.layout(
            t.name.clone(),
            egui::FontId::proportional(12.0),
            if selected {
                crate::widgets::ACCENT
            } else {
                visuals.text_color()
            },
            side,
        );
        let text_pos = egui::pos2(rect.center().x - galley.size().x / 2.0, pic.bottom() + 3.0);
        painter.galley(text_pos, galley, visuals.text_color());
        let resp = match &t.hint {
            Some(h) => resp.on_hover_text(format!("{}\n{h}", t.name)),
            None => resp.on_hover_text(&t.name),
        };
        resp.clicked()
    }
}
