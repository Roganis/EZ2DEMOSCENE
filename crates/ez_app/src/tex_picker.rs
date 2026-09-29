//! The texture library picker: a window of tiles showing the bundled
//! seamless textures, PBR materials on one tab and low-resolution
//! textures on the other.

use super::EzApp;
use crate::inspector;
use crate::platform::{LayerRef, TexSlot};
use egui::{Color32, RichText};
use ez_core::texlib::{self, Kind};
use ez_core::{LayerKind, Param, ReliefMode, Shading};
use std::collections::HashMap;

/// Kept between openings, so it comes back on the same tab, category and
/// search.
pub struct TexPicker {
    /// The layer and slot being edited while the window is open.
    target: Option<(LayerRef, TexSlot)>,
    kind: Kind,
    /// Category shown per tab (`None`: all).
    category: HashMap<&'static str, String>,
    search: String,
    /// Tile pictures by entry id.
    thumbs: HashMap<String, egui::TextureHandle>,
}

impl Default for TexPicker {
    fn default() -> Self {
        TexPicker {
            target: None,
            kind: Kind::Pbr,
            category: HashMap::new(),
            search: String::new(),
            thumbs: HashMap::new(),
        }
    }
}

const TILE_GAP: f32 = 6.0;
const LABEL_H: f32 = 30.0;
/// Pictures decoded per frame (the rest show "…" and come next frame).
const DECODES_PER_FRAME: usize = 12;

fn kind_key(kind: Kind) -> &'static str {
    match kind {
        Kind::Pbr => "pbr",
        Kind::Tile => "tile",
    }
}

impl EzApp {
    /// Open the picker when a texture chooser asked for it.
    pub(super) fn poll_tex_picker_request(&mut self, ctx: &egui::Context) {
        let asked = ctx.data_mut(|d| {
            d.remove_temp::<Option<inspector::TexRequest>>(egui::Id::new(inspector::TEX_PICKER))
                .flatten()
        });
        if let Some((lref, slot, kind)) = asked {
            if let Some(kind) = kind {
                self.tex_picker.kind = kind;
            }
            self.tex_picker.target = Some((lref, slot));
        }
    }

    pub(super) fn tex_picker_window(&mut self, ctx: &egui::Context) {
        let Some((lref, slot)) = self.tex_picker.target else {
            return;
        };
        // The layer went away (deleted, other project): nothing to pick for.
        if self.layer_for(lref).is_none() {
            self.tex_picker.target = None;
            return;
        }
        let mut picker = std::mem::take(&mut self.tex_picker);
        let screen = ctx.content_rect().size();
        let width = (screen.x - 24.0).min(760.0);
        let height = (screen.y - 90.0).clamp(240.0, 620.0);
        let mut open = true;
        let mut chosen: Option<texlib::Entry> = None;
        egui::Window::new("Texture library")
            .id(egui::Id::new("tex_picker"))
            .open(&mut open)
            .collapsible(false)
            .fixed_size([width, height])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                let Some(lib) = texlib::library() else {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Loading the texture library…");
                    });
                    return;
                };
                ui.horizontal_wrapped(|ui| {
                    let count = |k| lib.entries.iter().filter(|e| e.kind == k).count();
                    ui.selectable_value(
                        &mut picker.kind,
                        Kind::Pbr,
                        format!("PBR materials ({})", count(Kind::Pbr)),
                    )
                    .on_hover_text(
                        "Photo-real surfaces with colour, normal map and roughness: \
                         on a shape they set its whole material",
                    );
                    ui.selectable_value(
                        &mut picker.kind,
                        Kind::Tile,
                        format!("Low-res ({})", count(Kind::Tile)),
                    )
                    .on_hover_text("Small seamless textures for the retro look (at most 64 × 64)");
                    ui.add(
                        egui::TextEdit::singleline(&mut picker.search)
                            .hint_text("🔍 Search")
                            .desired_width(160.0),
                    );
                });
                ui.separator();
                let key = kind_key(picker.kind);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(!picker.category.contains_key(key), "All")
                        .clicked()
                    {
                        picker.category.remove(key);
                    }
                    for c in lib.categories(picker.kind) {
                        let on = picker.category.get(key).map(String::as_str) == Some(c);
                        if ui.selectable_label(on, c).clicked() {
                            picker.category.insert(key, c.to_string());
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
                let category = picker.category.get(key).cloned();
                let tiles: Vec<&texlib::Entry> = lib
                    .entries
                    .iter()
                    .filter(|e| e.kind == picker.kind)
                    .filter(|e| category.as_deref().is_none_or(|c| c == e.category))
                    .filter(|e| {
                        let hay = format!("{} {}", e.name, e.category).to_lowercase();
                        words.iter().all(|w| hay.contains(w.as_str()))
                    })
                    .collect();
                let current = self.current_texture(lref, slot);
                let footer = 22.0;
                let h = ui.available_height() - footer;
                if tiles.is_empty() {
                    ui.label(RichText::new("No texture matches.").weak());
                } else if let Some(e) = tile_grid(ui, &mut picker.thumbs, &lib, &tiles, &current, h)
                {
                    chosen = Some(e.clone());
                }
                ui.label(
                    RichText::new(
                        "CC0: materials by Poly Haven; low-res textures by Screaming Brain \
                         Studios and Kenney.",
                    )
                    .weak()
                    .small(),
                );
            });
        if let Some(e) = chosen {
            let note = self.apply_library_texture(lref, slot, &e);
            self.set_status(format!("{}{note}", e.name), false);
            open = false;
        }
        if !open {
            picker.target = None;
        }
        self.tex_picker = picker;
    }

    /// The texture name in a layer's slot.
    fn current_texture(&mut self, lref: LayerRef, slot: TexSlot) -> Option<String> {
        let layer = self.layer_for(lref)?;
        match (&layer.kind, slot) {
            (LayerKind::Mesh(m), TexSlot::Relief) => m.material.relief.texture.clone(),
            (LayerKind::Mesh(m), TexSlot::Orm) => m.material.pbr.orm_map.clone(),
            (LayerKind::Mesh(m), TexSlot::Emissive) => m.material.pbr.emissive_map.clone(),
            (LayerKind::Mesh(m), _) => m.material.texture.clone(),
            (LayerKind::Backdrop(b), _) => b.texture.clone(),
            (LayerKind::Mirror(f), _) => f.texture.clone(),
            (LayerKind::Terrain(t), _) => t.texture.clone(),
            (LayerKind::Mode7(f), _) => f.texture.clone(),
            (LayerKind::Sprite(sp), _) => sp.image.clone(),
            (LayerKind::Logo(g), TexSlot::Matcap) => g.matcap.clone(),
            (LayerKind::Logo(g), TexSlot::MorphImage) => g.morph_image.clone(),
            (LayerKind::Logo(g), _) => g.image.clone(),
            _ => None,
        }
    }

    /// Use a library texture in a slot. A PBR material on a shape's
    /// texture sets its whole material; in the relief or ORM slot it gives
    /// its normal or ORM map. Returns a note for the status line.
    fn apply_library_texture(
        &mut self,
        lref: LayerRef,
        slot: TexSlot,
        e: &texlib::Entry,
    ) -> String {
        let pbr = e.kind == Kind::Pbr;
        let Some(layer) = self.layer_for(lref) else {
            return String::new();
        };
        if let LayerKind::Mesh(m) = &mut layer.kind {
            let mat = &mut m.material;
            match slot {
                TexSlot::Material if pbr => {
                    mat.texture = Some(texlib::color_name(&e.id));
                    mat.relief.texture = Some(texlib::normal_name(&e.id));
                    mat.relief.mode = ReliefMode::NormalMap;
                    if mat.relief.bump.base == 0.0 && !mat.relief.bump.is_animated() {
                        mat.relief.bump = Param::new(1.0);
                    }
                    mat.pbr.orm_map = Some(texlib::orm_name(&e.id));
                    mat.pbr.shading = Shading::Physical;
                    // The maps carry the colour, roughness and metal.
                    mat.base_color = [1.0, 1.0, 1.0];
                    mat.metallic = Param::new(1.0);
                    mat.roughness = Param::new(1.0);
                    mat.rim = Param::new(0.0);
                    return " (material with its normal and ORM maps)".into();
                }
                TexSlot::Relief if pbr => {
                    mat.relief.texture = Some(texlib::normal_name(&e.id));
                    mat.relief.mode = ReliefMode::NormalMap;
                    if mat.relief.bump.base == 0.0 && !mat.relief.bump.is_animated() {
                        mat.relief.bump = Param::new(1.0);
                    }
                    return " (normal map)".into();
                }
                TexSlot::Orm if pbr => {
                    mat.pbr.orm_map = Some(texlib::orm_name(&e.id));
                    return " (occlusion / roughness / metal)".into();
                }
                // A plain texture replacing a library PBR material: its maps
                // (and the full metal and roughness they needed) go too.
                TexSlot::Material => {
                    let lib_map = |m: &Option<String>| m.as_deref().is_some_and(texlib::is_lib);
                    if lib_map(&mat.relief.texture) && mat.relief.mode == ReliefMode::NormalMap {
                        mat.relief.texture = None;
                        mat.relief.bump = Param::new(0.0);
                    }
                    if lib_map(&mat.pbr.orm_map) {
                        mat.pbr.orm_map = None;
                        mat.metallic = Param::new(0.0);
                        mat.roughness = Param::new(0.7);
                    }
                }
                _ => {}
            }
        }
        self.set_layer_texture(lref, slot, texlib::color_name(&e.id));
        String::new()
    }
}

/// A scrolling grid of tiles filling `height`; returns the clicked one.
fn tile_grid<'a>(
    ui: &mut egui::Ui,
    thumbs: &mut HashMap<String, egui::TextureHandle>,
    lib: &texlib::Library,
    tiles: &[&'a texlib::Entry],
    current: &Option<String>,
    height: f32,
) -> Option<&'a texlib::Entry> {
    let avail = ui.available_width();
    let cols = ((avail + TILE_GAP) / (96.0 + TILE_GAP)).floor().max(2.0) as usize;
    let side = ((avail - TILE_GAP * (cols as f32 - 1.0)) / cols as f32 - 1.0).min(128.0);
    let row_h = side + LABEL_H + TILE_GAP;
    let rows = tiles.len().div_ceil(cols);
    let mut chosen = None;
    let mut budget = DECODES_PER_FRAME;
    let current_id = current.as_deref().and_then(texlib::id_of);
    egui::ScrollArea::vertical()
        .max_height(height.max(80.0))
        .auto_shrink([false, false])
        .show_rows(ui, row_h, rows, |ui, range| {
            for r in range {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = TILE_GAP;
                    for e in tiles.iter().skip(r * cols).take(cols) {
                        let selected = current_id == Some(e.id.as_str());
                        if tile(ui, thumbs, lib, e, side, selected, &mut budget) {
                            chosen = Some(*e);
                        }
                    }
                });
                ui.add_space(TILE_GAP);
            }
        });
    if budget == 0 {
        ui.ctx().request_repaint();
    }
    chosen
}

/// The tile's picture, decoded once (while `budget` lasts this frame).
fn thumb(
    ui: &egui::Ui,
    thumbs: &mut HashMap<String, egui::TextureHandle>,
    lib: &texlib::Library,
    e: &texlib::Entry,
    budget: &mut usize,
) -> Option<egui::TextureId> {
    if let Some(t) = thumbs.get(&e.id) {
        return Some(t.id());
    }
    if *budget == 0 {
        return None;
    }
    *budget -= 1;
    let bytes = lib.read(&texlib::color_name(&e.id)).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    let options = if e.kind == Kind::Tile {
        egui::TextureOptions::NEAREST_REPEAT
    } else {
        egui::TextureOptions::LINEAR_REPEAT
    };
    let handle = ui
        .ctx()
        .load_texture(format!("texlib:{}", e.id), color, options);
    let id = handle.id();
    thumbs.insert(e.id.clone(), handle);
    Some(id)
}

fn tile(
    ui: &mut egui::Ui,
    thumbs: &mut HashMap<String, egui::TextureHandle>,
    lib: &texlib::Library,
    e: &texlib::Entry,
    side: f32,
    selected: bool,
    budget: &mut usize,
) -> bool {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(side, side + LABEL_H), egui::Sense::click());
    if !ui.is_rect_visible(rect) {
        return false;
    }
    let pic = egui::Rect::from_min_size(rect.min, egui::vec2(side, side));
    let visuals = ui.visuals().clone();
    let tex = thumb(ui, thumbs, lib, e, budget);
    let painter = ui.painter_at(rect);
    painter.rect_filled(pic, 6.0, visuals.extreme_bg_color);
    match tex {
        Some(tex) => {
            // Small textures show 2 × 2 copies, so the tiling shows.
            let reps = if e.kind == Kind::Tile { 2.0 } else { 1.0 };
            painter.image(
                tex,
                pic.shrink(1.0),
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(reps, reps)),
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
        e.name.clone(),
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
    resp.on_hover_text(format!(
        "{}\n{} · {} × {} px",
        e.name, e.category, e.size, e.size
    ))
    .clicked()
}
