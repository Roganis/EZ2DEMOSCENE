//! The editor application.

use crate::audio::AudioPlayer;
use crate::export_ui::{slug, ExportUi};
use crate::gizmo::{self, Gizmo, GizmoMode, Projector};
use crate::inspector::{self, IMAGE_EXTENSIONS, MODEL_EXTENSIONS};
use crate::library::Library;
use crate::nodes::NodeEditor;
use crate::viewport::Viewport;
use crate::widgets::ACCENT;
use egui::{Color32, RichText, Ui};
use ez_core::graph::Graph;
use ez_core::randomize::{randomize, RandomizeOptions};
use ez_core::*;
use std::path::{Path, PathBuf};

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "flac", "m4a", "aac"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Selection {
    Timing,
    Camera,
    Environment,
    Post,
    Textures,
    Layer(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Simple,
    Nodes,
}

struct Thumb {
    name: &'static str,
    description: &'static str,
    texture: egui::TextureId,
    _target: ez_render::RenderTarget,
}

pub struct EzApp {
    project: Project,
    path: Option<PathBuf>,
    saved: Project,
    undo: Vec<Project>,
    redo: Vec<Project>,
    committed: Project,

    selection: Selection,
    mode: Mode,
    nodes: Option<NodeEditor>,

    viewport: Viewport,
    playing: bool,
    time: f64,
    preview_scale: f32,
    aspect: (u32, u32),

    audio: Option<AudioPlayer>,
    audio_env: Option<AudioEnvelope>,

    presets_open: bool,
    thumbs: Vec<Thumb>,
    randomize_open: bool,
    rand_opts: RandomizeOptions,
    rand_seed: u64,
    export: ExportUi,
    help_open: bool,
    status: Option<(String, bool, f64)>,
    /// Wall-clock seconds (egui input time).
    now: f64,
    library: Library,
    last_autosave: f64,
    gallery_tab: usize,
    /// Name typed in the "save as my preset" dialog (Some = dialog open).
    preset_name: Option<String>,
    gizmo: Gizmo,
}

const AUTOSAVE_SECONDS: f64 = 30.0;
const PROJECT_FILTER: &[&str] = &["json", ez_core::assets::PACK_EXTENSION];

impl EzApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> EzApp {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .expect("EZ2DEMOSCENE needs the wgpu renderer");
        setup_style(&cc.egui_ctx);
        let project = presets::neon_arena();
        let mut app = EzApp {
            saved: project.clone(),
            committed: project.clone(),
            project,
            path: None,
            undo: Vec::new(),
            redo: Vec::new(),
            selection: Selection::Camera,
            mode: Mode::Simple,
            nodes: None,
            viewport: Viewport::new(rs),
            playing: true,
            time: 0.0,
            preview_scale: 1.0,
            aspect: (16, 9),
            audio: None,
            audio_env: None,
            presets_open: false,
            thumbs: Vec::new(),
            randomize_open: false,
            rand_opts: RandomizeOptions::default(),
            rand_seed: 1,
            export: ExportUi::default(),
            help_open: false,
            status: None,
            now: 0.0,
            library: Library::open(&cc.egui_ctx),
            last_autosave: 0.0,
            gallery_tab: 0,
            preset_name: None,
            gizmo: Gizmo::default(),
        };
        match initial {
            Some(p) => app.open_path(&p),
            None => app.presets_open = app.library.recovery.is_none(),
        }
        app
    }

    fn set_status(&mut self, msg: impl Into<String>, error: bool) {
        self.status = Some((msg.into(), error, self.now));
    }

    fn loop_seconds(&self) -> f64 {
        self.project.timing.loop_seconds() as f64
    }

    fn phase(&self) -> f32 {
        self.project.timing.phase_at(self.time)
    }

    // ---------------------------------------------------------------
    // Project management

    fn load_project(&mut self, p: Project, path: Option<PathBuf>) {
        self.undo.push(self.project.clone());
        self.project = p;
        self.committed = self.project.clone();
        self.saved = self.project.clone();
        self.path = path;
        self.selection = Selection::Camera;
        self.nodes = None;
        self.mode = if self.project.use_graph {
            Mode::Nodes
        } else {
            Mode::Simple
        };
        self.time = 0.0;
        self.viewport.renderer.reload_assets();
        self.reload_audio();
    }

    fn open_path(&mut self, path: &Path) {
        if ez_core::assets::is_pack(path) {
            let dest = self.library.unpack_dir(path);
            match ez_core::assets::unpack(path, &dest) {
                Ok(p) => {
                    // A pack has no editable location: "Save" asks where to put it.
                    self.load_project(p, None);
                    self.set_status(format!("Opened pack {}", path.display()), false);
                }
                Err(e) => self.set_status(format!("Could not open {}: {e}", path.display()), true),
            }
            return;
        }
        match Project::load(path) {
            Ok(p) => {
                self.load_project(p, Some(path.to_path_buf()));
                self.set_status(format!("Opened {}", path.display()), false);
            }
            Err(e) => self.set_status(format!("Could not open {}: {e}", path.display()), true),
        }
    }

    fn save_pack(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("EZ2 pack", &[ez_core::assets::PACK_EXTENSION])
            .set_file_name(format!(
                "{}.{}",
                slug(&self.project.name),
                ez_core::assets::PACK_EXTENSION
            ))
            .save_file()
        else {
            return;
        };
        match ez_core::assets::pack(&self.project, &path) {
            Ok(r) if r.missing.is_empty() => self.set_status(
                format!("Packed {} with {} asset(s)", path.display(), r.packed),
                false,
            ),
            Ok(r) => self.set_status(
                format!(
                    "Packed, but {} file(s) were missing: {}",
                    r.missing.len(),
                    r.missing.join(", ")
                ),
                true,
            ),
            Err(e) => self.set_status(format!("Pack failed: {e}"), true),
        }
    }

    fn autosave(&mut self) {
        if self.now - self.last_autosave < AUTOSAVE_SECONDS {
            return;
        }
        self.last_autosave = self.now;
        if self.project != self.saved {
            if let Err(e) = self.library.autosave(&self.project) {
                self.set_status(format!("Autosave failed: {e}"), true);
            }
        }
    }

    fn save_user_preset(&mut self, ctx: &egui::Context, name: String) {
        let mut p = self.project.clone();
        p.name = name;
        let target = self.viewport.renderer.create_target(320, 180);
        let thumb = self.viewport.renderer.render_image(
            &p,
            &EvalCtx::new(&p.timing, self.phase(), None),
            &target,
        );
        match self.library.save_preset(ctx, &p, &thumb) {
            Ok(()) => self.set_status(format!("Saved '{}' to My presets", p.name), false),
            Err(e) => self.set_status(format!("Could not save preset: {e}"), true),
        }
    }

    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter("EZ2 project or pack", PROJECT_FILTER)
            .pick_file()
        {
            self.open_path(&p);
        }
    }

    fn save(&mut self, save_as: bool) {
        let path = match (&self.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new()
                .add_filter("EZ2 project", &["json"])
                .set_file_name(format!(
                    "{}.{}",
                    slug(&self.project.name),
                    PROJECT_EXTENSION
                ))
                .save_file(),
        };
        let Some(path) = path else { return };
        match self.project.save(&path) {
            Ok(()) => {
                self.saved = self.project.clone();
                self.set_status(format!("Saved {}", path.display()), false);
                self.path = Some(path);
            }
            Err(e) => self.set_status(format!("Save failed: {e}"), true),
        }
    }

    fn set_audio(&mut self, path: Option<PathBuf>) {
        self.project.audio = path.map(|p| p.to_string_lossy().to_string());
        self.reload_audio();
    }

    fn reload_audio(&mut self) {
        self.audio = None;
        self.audio_env = None;
        let Some(path) = self.project.audio.clone() else {
            return;
        };
        let path = PathBuf::from(path);
        match ez_export::analyze_audio(&path) {
            Ok(env) => self.audio_env = Some(env),
            Err(e) => {
                self.set_status(format!("Music: {e:#}"), true);
                return;
            }
        }
        match AudioPlayer::new(&path) {
            Ok(a) => self.audio = Some(a),
            Err(e) => self.set_status(
                format!("Music loaded for sync, but no playback: {e:#}"),
                true,
            ),
        }
    }

    fn handle_dropped(&mut self, files: Vec<PathBuf>) {
        for f in files {
            let ext = f
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if ext == "json" {
                self.open_path(&f);
            } else if MODEL_EXTENSIONS.contains(&ext.as_str()) {
                self.project.layers.push(inspector::model_layer(&f));
                self.selection = Selection::Layer(self.project.layers.len() - 1);
                self.set_status(format!("Added model {}", f.display()), false);
            } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
                let name = inspector::add_user_texture(&mut self.project.textures, &f);
                if let Selection::Layer(i) = self.selection {
                    if let Some(LayerKind::Mesh(m)) =
                        self.project.layers.get_mut(i).map(|l| &mut l.kind)
                    {
                        m.material.texture = Some(name.clone());
                    }
                }
                self.set_status(format!("Added image '{name}'"), false);
            } else if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                self.set_audio(Some(f));
            } else {
                self.set_status(format!("Don't know what to do with {}", f.display()), true);
            }
        }
    }

    fn commit_history(&mut self, pointer_down: bool) {
        if pointer_down || self.project == self.committed {
            return;
        }
        self.undo
            .push(std::mem::replace(&mut self.committed, self.project.clone()));
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.project, prev));
            self.committed = self.project.clone();
            self.nodes = None;
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.project, next));
            self.committed = self.project.clone();
            self.nodes = None;
        }
    }

    // ---------------------------------------------------------------
    // UI pieces

    fn top_bar(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.label(RichText::new("EZ2DEMOSCENE").strong().color(ACCENT));
            ui.separator();
            ui.menu_button("File", |ui| {
                if ui.button("New from preset…").clicked() {
                    self.presets_open = true;
                    ui.close();
                }
                if ui.button("Open…  (Ctrl+O)").clicked() {
                    self.open_dialog();
                    ui.close();
                }
                if ui.button("Save  (Ctrl+S)").clicked() {
                    self.save(false);
                    ui.close();
                }
                if ui.button("Save as…").clicked() {
                    self.save(true);
                    ui.close();
                }
                if ui
                    .button("Save as pack (.ez2pack)…")
                    .on_hover_text("One file with the project and all its models, images and music, to share or move to another computer")
                    .clicked()
                {
                    self.save_pack();
                    ui.close();
                }
                if ui.button("Save as my preset…").clicked() {
                    self.preset_name = Some(self.project.name.clone());
                    ui.close();
                }
                ui.separator();
                if ui.button("Import 3D model…").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("3D models", MODEL_EXTENSIONS)
                        .pick_file()
                    {
                        self.handle_dropped(vec![p]);
                    }
                    ui.close();
                }
                if ui.button("Import image…").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Images", IMAGE_EXTENSIONS)
                        .pick_file()
                    {
                        self.handle_dropped(vec![p]);
                    }
                    ui.close();
                }
                if ui.button("Load music…").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Audio", AUDIO_EXTENSIONS)
                        .pick_file()
                    {
                        self.set_audio(Some(p));
                    }
                    ui.close();
                }
                ui.separator();
                if ui.button("Save still image…").clicked() {
                    self.save_still();
                    ui.close();
                }
                if ui.button("Export loop…  (Ctrl+E)").clicked() {
                    self.export.open = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui
                    .add_enabled(!self.undo.is_empty(), egui::Button::new("Undo  (Ctrl+Z)"))
                    .clicked()
                {
                    self.undo();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !self.redo.is_empty(),
                        egui::Button::new("Redo  (Ctrl+Shift+Z)"),
                    )
                    .clicked()
                {
                    self.redo();
                    ui.close();
                }
                ui.separator();
                if ui.button("Reload models & images").clicked() {
                    self.viewport.renderer.reload_assets();
                    ui.close();
                }
            });
            if ui.button("Presets").clicked() {
                self.presets_open = true;
            }
            if ui
                .button("🎲 Randomize")
                .on_hover_text("Shuffle colours, shapes and motion")
                .clicked()
            {
                self.rand_seed = self.rand_seed.wrapping_add(1);
                randomize(&mut self.project, self.rand_seed, self.rand_opts);
            }
            if ui
                .small_button("⚙")
                .on_hover_text("Randomizer settings")
                .clicked()
            {
                self.randomize_open = true;
            }
            if ui
                .button("✨ Surprise me")
                .on_hover_text("Random preset + randomize")
                .clicked()
            {
                self.surprise();
            }
            ui.separator();
            ui.selectable_value(&mut self.mode, Mode::Simple, "Simple")
                .on_hover_text("Layers & sliders");
            ui.selectable_value(&mut self.mode, Mode::Nodes, "Nodes")
                .on_hover_text("Advanced: build the scene as a node graph");
            ui.separator();
            if ui
                .button(RichText::new("⏺ Export").color(ACCENT).strong())
                .clicked()
            {
                self.export.open = true;
            }
            if ui.button("?").on_hover_text("Help").clicked() {
                self.help_open = true;
            }
        });
    }

    fn surprise(&mut self) {
        let all = presets::all();
        self.rand_seed = self
            .rand_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let pick = (self.rand_seed >> 33) as usize % (all.len() - 1);
        let mut p = all[pick].project.clone();
        randomize(
            &mut p,
            self.rand_seed,
            RandomizeOptions {
                strength: 0.8,
                ..self.rand_opts
            },
        );
        let name = p.name.clone();
        self.load_project(p, None);
        self.set_status(format!("Surprise! Based on '{name}'"), false);
    }

    fn save_still(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name(format!("{}.png", slug(&self.project.name)))
            .save_file()
        else {
            return;
        };
        let (w, h) = (self.export.settings.width, self.export.settings.height);
        match ez_export::render_still(&self.project, self.phase(), w, h, &path) {
            Ok(()) => self.set_status(format!("Saved {}", path.display()), false),
            Err(e) => self.set_status(format!("{e:#}"), true),
        }
    }

    fn scene_panel(&mut self, ui: &mut Ui) {
        let templates = self.library.template_layers();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.project.name);
        });
        ui.separator();
        let items = [
            (Selection::Timing, "⏱  Timing & music"),
            (Selection::Camera, "🎥  Camera"),
            (Selection::Environment, "☀  Light & fog"),
            (Selection::Post, "🎞  Post effects"),
            (Selection::Textures, "🖼  Your images"),
        ];
        for (sel, label) in items {
            if ui.selectable_label(self.selection == sel, label).clicked() {
                self.selection = sel;
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("Layers");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button(RichText::new("+ Add").color(ACCENT), |ui| {
                    if let Some(l) = inspector::add_layer_menu(ui, &templates) {
                        self.project.layers.push(l);
                        self.selection = Selection::Layer(self.project.layers.len() - 1);
                        ui.close();
                    }
                });
            });
        });
        if self.project.use_graph {
            ui.label(
                RichText::new("The node graph is active: layers below are not rendered.")
                    .color(Color32::YELLOW)
                    .small(),
            );
        }
        let mut action: Option<(usize, i32)> = None; // (index, op): -1 up, 1 down, 2 dup, 3 delete
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let n = self.project.layers.len();
                for i in 0..n {
                    let l = &mut self.project.layers[i];
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut l.enabled, "").on_hover_text("Show / hide");
                        let text = format!("{} {}", inspector::layer_icon(l), l.name);
                        let r = ui.add(
                            egui::Button::selectable(self.selection == Selection::Layer(i), text)
                                .truncate(),
                        );
                        if r.clicked() {
                            self.selection = Selection::Layer(i);
                        }
                        r.context_menu(|ui| {
                            if ui.button("Move up").clicked() {
                                action = Some((i, -1));
                                ui.close();
                            }
                            if ui.button("Move down").clicked() {
                                action = Some((i, 1));
                                ui.close();
                            }
                            if ui.button("Duplicate").clicked() {
                                action = Some((i, 2));
                                ui.close();
                            }
                            if ui
                                .button("Save as template")
                                .on_hover_text(
                                    "Reuse this layer in other projects (+ Add → My templates)",
                                )
                                .clicked()
                            {
                                action = Some((i, 4));
                                ui.close();
                            }
                            if ui.button("Delete").clicked() {
                                action = Some((i, 3));
                                ui.close();
                            }
                        });
                    });
                }
            });
        if let Selection::Layer(i) = self.selection {
            ui.horizontal(|ui| {
                if ui.small_button("⬆").on_hover_text("Move up").clicked() {
                    action = Some((i, -1));
                }
                if ui.small_button("⬇").on_hover_text("Move down").clicked() {
                    action = Some((i, 1));
                }
                if ui.small_button("⧉").on_hover_text("Duplicate").clicked() {
                    action = Some((i, 2));
                }
                if ui.small_button("🗑").on_hover_text("Delete").clicked() {
                    action = Some((i, 3));
                }
            });
        }
        if let Some((i, 4)) = action {
            let layer = self.project.layers[i].clone();
            match self.library.save_template(ui.ctx(), &layer) {
                Ok(()) => self.set_status(format!("Saved '{}' as a template", layer.name), false),
                Err(e) => self.set_status(format!("Could not save template: {e}"), true),
            }
            action = None;
        }
        if let Some((i, op)) = action {
            let layers = &mut self.project.layers;
            match op {
                -1 if i > 0 => {
                    layers.swap(i, i - 1);
                    self.selection = Selection::Layer(i - 1);
                }
                1 if i + 1 < layers.len() => {
                    layers.swap(i, i + 1);
                    self.selection = Selection::Layer(i + 1);
                }
                2 => {
                    let mut c = layers[i].clone();
                    c.name.push_str(" copy");
                    layers.insert(i + 1, c);
                    self.selection = Selection::Layer(i + 1);
                }
                3 => {
                    layers.remove(i);
                    self.selection = if layers.is_empty() {
                        Selection::Camera
                    } else {
                        Selection::Layer(i.min(layers.len() - 1))
                    };
                }
                _ => {}
            }
        }
    }

    fn inspector_panel(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(4.0);
            if self.mode == Mode::Nodes {
                let textures = &mut self.project.textures;
                match self.nodes.as_mut().and_then(|n| n.selected_layer_mut()) {
                    Some(layer) => inspector::layer_ui(ui, layer, textures),
                    None => {
                        ui.heading("Node graph");
                        ui.label("Right-click the canvas to add nodes. Drag from an output to an input to connect.");
                        ui.label("Click \"edit in inspector\" on a layer node to edit it here.");
                        ui.add_space(8.0);
                        ui.label(RichText::new("Sources emit layers; modifiers (Symmetry, Array, Spin…) change every layer flowing through them; everything connected to Output is rendered.").weak());
                    }
                }
                return;
            }
            match self.selection {
                Selection::Timing => {
                    inspector::timing_ui(ui, &mut self.project);
                    ui.separator();
                    self.music_ui(ui);
                }
                Selection::Camera => inspector::camera_ui(ui, &mut self.project.camera),
                Selection::Environment => inspector::environment_ui(ui, &mut self.project.environment),
                Selection::Post => inspector::post_ui(ui, &mut self.project.post),
                Selection::Textures => inspector::textures_ui(ui, &mut self.project.textures),
                Selection::Layer(i) => {
                    let Project { layers, textures, .. } = &mut self.project;
                    match layers.get_mut(i) {
                        Some(l) => inspector::layer_ui(ui, l, textures),
                        None => self.selection = Selection::Camera,
                    }
                }
            }
            let errors: Vec<String> = self.viewport.renderer.errors.values().cloned().collect();
            if !errors.is_empty() {
                ui.separator();
                for e in errors {
                    ui.label(RichText::new(format!("⚠ {e}")).color(Color32::LIGHT_RED).small());
                }
            }
        });
    }

    fn music_ui(&mut self, ui: &mut Ui) {
        ui.heading("Music");
        match self.project.audio.clone() {
            Some(p) => {
                ui.label(
                    RichText::new(
                        Path::new(&p)
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or(p),
                    )
                    .strong(),
                );
                if let Some(a) = &mut self.audio {
                    ui.add(egui::Slider::new(&mut a.volume, 0.0..=1.0).text("volume"));
                }
                if let Some(env) = &self.audio_env {
                    ui.label(RichText::new(format!("{:.1} s analysed — use the ♪ amount on any animated value to react to it.", env.duration)).weak());
                    if ui
                        .button("Detect tempo from length")
                        .on_hover_text(
                            "Sets the BPM so the whole track is one loop of the current length",
                        )
                        .clicked()
                        && env.duration > 0.5
                    {
                        self.project.timing.bpm = (self.project.timing.loop_beats as f32 * 60.0
                            / env.duration)
                            .clamp(40.0, 240.0);
                    }
                }
                if ui.button("Remove music").clicked() {
                    self.set_audio(None);
                }
            }
            None => {
                ui.label(RichText::new("Optional: drop an MP3/WAV/OGG/FLAC here or pick one. The loop plays with it and values can pulse with it.").weak());
                if ui.button("Load music…").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Audio", AUDIO_EXTENSIONS)
                        .pick_file()
                    {
                        self.set_audio(Some(p));
                    }
                }
            }
        }
    }

    fn timeline(&mut self, ui: &mut Ui) {
        let loop_s = self.loop_seconds();
        let beats = self.project.timing.loop_beats.max(1);
        ui.horizontal(|ui| {
            let icon = if self.playing { "⏸" } else { "▶" };
            if ui
                .add(
                    egui::Button::new(RichText::new(icon).size(18.0))
                        .min_size(egui::vec2(34.0, 26.0)),
                )
                .on_hover_text("Play / pause (Space)")
                .clicked()
            {
                self.playing = !self.playing;
            }
            if ui.button("⏮").on_hover_text("Back to start").clicked() {
                self.time = 0.0;
            }
            // Scrubber with beat ticks.
            let width = ui.available_width() - 330.0;
            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(width.max(100.0), 26.0),
                egui::Sense::click_and_drag(),
            );
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
            for b in 0..=beats {
                let x = rect.left() + rect.width() * b as f32 / beats as f32;
                let bar = b % 4 == 0;
                let h = if bar {
                    rect.height()
                } else {
                    rect.height() * 0.4
                };
                painter.line_segment(
                    [
                        egui::pos2(x, rect.bottom() - h),
                        egui::pos2(x, rect.bottom()),
                    ],
                    egui::Stroke::new(
                        1.0,
                        if bar {
                            Color32::from_gray(120)
                        } else {
                            Color32::from_gray(70)
                        },
                    ),
                );
            }
            if let Some(env) = &self.audio_env {
                let n = rect.width() as usize;
                for i in 0..n {
                    let t = i as f32 / n as f32 * loop_s as f32;
                    let (lv, bass) = env.sample(t);
                    let x = rect.left() + i as f32;
                    let h = lv * rect.height() * 0.45;
                    painter.line_segment(
                        [
                            egui::pos2(x, rect.center().y - h),
                            egui::pos2(x, rect.center().y + h),
                        ],
                        egui::Stroke::new(
                            1.0,
                            Color32::from_rgba_unmultiplied(
                                120,
                                180,
                                255,
                                60 + (bass * 120.0) as u8,
                            ),
                        ),
                    );
                }
            }
            let ph = self.phase();
            let x = rect.left() + rect.width() * ph;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(2.0, ACCENT),
            );
            if let Some(pos) = resp.interact_pointer_pos() {
                let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 0.9999);
                self.time = f as f64 * loop_s;
            }
            let beat = ph * beats as f32;
            ui.label(RichText::new(format!("beat {:>5.2} / {beats}", beat + 1.0)).monospace());
            ui.separator();
            ui.add(
                egui::DragValue::new(&mut self.project.timing.bpm)
                    .range(20.0..=300.0)
                    .speed(0.5)
                    .suffix(" BPM"),
            );
            ui.add(
                egui::DragValue::new(&mut self.project.timing.loop_beats)
                    .range(1..=256)
                    .speed(0.1)
                    .suffix(" beats"),
            );
            ui.label(RichText::new(format!("{:.2}s", loop_s)).weak());
            if self.project.audio.is_some() {
                ui.label(RichText::new("♪").color(ACCENT))
                    .on_hover_text("Music loaded");
            }
        });
    }

    fn viewport_ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Preview");
            egui::ComboBox::from_id_salt("aspect")
                .selected_text(format!("{}:{}", self.aspect.0, self.aspect.1))
                .width(70.0)
                .show_ui(ui, |ui| {
                    for a in [(16, 9), (4, 3), (1, 1), (9, 16), (21, 9)] {
                        ui.selectable_value(&mut self.aspect, a, format!("{}:{}", a.0, a.1));
                    }
                });
            egui::ComboBox::from_id_salt("quality")
                .selected_text(format!("{}%", (self.preview_scale * 100.0) as u32))
                .width(70.0)
                .show_ui(ui, |ui| {
                    for q in [1.0, 0.75, 0.5, 0.35] {
                        ui.selectable_value(
                            &mut self.preview_scale,
                            q,
                            format!("{}%", (q * 100.0) as u32),
                        );
                    }
                })
                .response
                .on_hover_text("Lower the preview resolution if playback stutters");
            ui.separator();
            for (mode, label, key) in [
                (GizmoMode::Move, "✥ Move", "W"),
                (GizmoMode::Rotate, "⟲ Rotate", "E"),
                (GizmoMode::Scale, "⤢ Scale", "R"),
            ] {
                ui.selectable_value(&mut self.gizmo.mode, mode, label)
                    .on_hover_text(format!(
                        "{key} — drag the handles of the selected layer. Hold Ctrl to snap."
                    ));
            }
            ui.checkbox(&mut self.gizmo.grid, "Grid")
                .on_hover_text("G — show a ground grid (1 unit squares)");
            if let Some((msg, err, t)) = &self.status {
                if self.now - t < 6.0 || (*err && self.now - t < 20.0) {
                    ui.label(RichText::new(msg).color(if *err {
                        Color32::LIGHT_RED
                    } else {
                        Color32::LIGHT_GREEN
                    }));
                }
            }
        });
        let avail = ui.available_size();
        let ar = self.aspect.0 as f32 / self.aspect.1 as f32;
        let mut size = egui::vec2(avail.x, avail.x / ar);
        if size.y > avail.y {
            size = egui::vec2(avail.y * ar, avail.y);
        }
        let ppp = ui.ctx().pixels_per_point();
        let px = [
            (size.x * ppp * self.preview_scale) as u32,
            (size.y * ppp * self.preview_scale) as u32,
        ];
        let ctx = EvalCtx::new(&self.project.timing, self.phase(), self.audio_env.as_ref());
        let tex = self.viewport.render(&self.project, &ctx, px);
        let resp = ui
            .centered_and_justified(|ui| {
                ui.add(
                    egui::Image::new(egui::load::SizedTexture::new(tex, size))
                        .sense(egui::Sense::click_and_drag())
                        .corner_radius(4.0),
                )
            })
            .inner;
        // Gizmo, picking and mouse camera control.
        let cam_state = self.project.camera.eval(&ctx);
        let proj = Projector::new(&cam_state, resp.rect);
        let painter = ui.painter_at(resp.rect);
        if self.gizmo.grid {
            gizmo::draw_grid(&painter, &proj, 0.0);
        }
        let snapping = ui.input(|i| i.modifiers.command);
        let mut on_gizmo = false;
        if self.mode == Mode::Simple && !self.project.use_graph {
            if let Selection::Layer(i) = self.selection {
                if let Some(layer) = self.project.layers.get_mut(i) {
                    on_gizmo = self.gizmo.show(&painter, &resp, &proj, layer, snapping);
                }
            }
            if resp.clicked() && !on_gizmo {
                if let Some(pos) = resp.interact_pointer_pos() {
                    if let Some(i) = gizmo::pick(&self.project.layers, &ctx, &proj, pos) {
                        self.selection = Selection::Layer(i);
                    }
                }
            }
        }
        if resp.dragged() && !on_gizmo && !self.gizmo.is_dragging() {
            let d = resp.drag_delta();
            let cam = &mut self.project.camera;
            cam.angle = (cam.angle - d.x * 0.4 + 540.0).rem_euclid(360.0) - 180.0;
            cam.height.base += d.y * 0.03;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let cam = &mut self.project.camera;
                cam.distance.base = (cam.distance.base * (1.0 - scroll * 0.002)).clamp(0.3, 200.0);
            }
        }
    }

    fn presets_window(&mut self, ctx: &egui::Context) {
        if !self.presets_open {
            return;
        }
        if self.thumbs.is_empty() {
            for p in presets::all() {
                let (texture, target) = self.viewport.thumbnail(&p.project, 0.2, [320, 180]);
                self.thumbs.push(Thumb {
                    name: p.name,
                    description: p.description,
                    texture,
                    _target: target,
                });
            }
        }
        let mut open = true;
        let mut chosen_builtin = None;
        let mut chosen_user = None;
        let mut delete_user = None;
        let mut delete_template = None;
        let card = |ui: &mut Ui, tex: Option<egui::TextureId>, name: &str, tip: &str| -> bool {
            let mut clicked = false;
            ui.vertical(|ui| {
                let size = egui::vec2(213.0, 120.0);
                let r = match tex {
                    Some(t) => ui.add(
                        egui::Image::new(egui::load::SizedTexture::new(t, size))
                            .corner_radius(4.0)
                            .sense(egui::Sense::click()),
                    ),
                    None => ui.add_sized(size, egui::Button::new("no preview")),
                };
                let r = if tip.is_empty() {
                    r
                } else {
                    r.on_hover_text(tip)
                };
                clicked = r.clicked();
                if r.hovered() {
                    ui.painter().rect_stroke(
                        r.rect,
                        4.0,
                        egui::Stroke::new(2.0, ACCENT),
                        egui::StrokeKind::Outside,
                    );
                }
                ui.strong(name);
            });
            clicked
        };
        egui::Window::new("Start from a preset")
            .open(&mut open)
            .collapsible(false)
            .default_width(720.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.gallery_tab, 0, "Built-in");
                    ui.selectable_value(&mut self.gallery_tab, 1, format!("My presets ({})", self.library.presets.len()));
                    ui.selectable_value(&mut self.gallery_tab, 2, format!("My layer templates ({})", self.library.templates.len()));
                });
                ui.separator();
                match self.gallery_tab {
                    0 => {
                        ui.label("Pick a starting point, then tweak layers on the left and values on the right. Everything loops automatically.");
                        ui.add_space(6.0);
                        egui::Grid::new("presets").spacing([10.0, 10.0]).show(ui, |ui| {
                            for (i, t) in self.thumbs.iter().enumerate() {
                                if card(ui, Some(t.texture), t.name, t.description) {
                                    chosen_builtin = Some(i);
                                }
                                if i % 3 == 2 {
                                    ui.end_row();
                                }
                            }
                        });
                    }
                    1 => {
                        if self.library.presets.is_empty() {
                            ui.label("No presets yet. Use File → Save as my preset… to add the current scene here.");
                        }
                        egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                            egui::Grid::new("user presets").spacing([10.0, 10.0]).show(ui, |ui| {
                                for (i, p) in self.library.presets.iter().enumerate() {
                                    ui.vertical(|ui| {
                                        if card(ui, p.thumb.as_ref().map(|t| t.id()), &p.name, "") {
                                            chosen_user = Some(i);
                                        }
                                        if ui.small_button("🗑 delete").clicked() {
                                            delete_user = Some(i);
                                        }
                                    });
                                    if i % 3 == 2 {
                                        ui.end_row();
                                    }
                                }
                            });
                        });
                    }
                    _ => {
                        ui.label("Layers you saved with right-click → Save as template. Add them with + Add → My templates.");
                        for (i, t) in self.library.templates.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{} {}", inspector::layer_icon(&t.layer), t.layer.name));
                                ui.label(RichText::new(t.layer.type_label()).weak());
                                if ui.small_button("🗑").on_hover_text("Delete template").clicked() {
                                    delete_template = Some(i);
                                }
                            });
                        }
                    }
                }
            });
        if let Some(i) = chosen_builtin {
            let p = presets::all().into_iter().nth(i).unwrap();
            self.load_project(p.project, None);
            self.presets_open = false;
        }
        if let Some(i) = chosen_user {
            let path = self.library.presets[i].path.clone();
            match Project::load(&path) {
                // Loaded as a new, unsaved project so the preset isn't overwritten.
                Ok(p) => {
                    self.load_project(p, None);
                    self.presets_open = false;
                }
                Err(e) => self.set_status(format!("Could not load preset: {e}"), true),
            }
        }
        if let Some(i) = delete_user {
            self.library.delete_preset(ctx, i);
        }
        if let Some(i) = delete_template {
            self.library.delete_template(ctx, i);
        }
        if !open {
            self.presets_open = false;
        }
    }

    fn recovery_window(&mut self, ctx: &egui::Context) {
        if self.library.recovery.is_none() {
            return;
        }
        let mut choice = None;
        egui::Window::new("Recover unsaved work?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let name = self.library.recovery.as_ref().map(|p| p.name.clone()).unwrap_or_default();
                ui.label(format!("EZ2DEMOSCENE did not close properly last time. An autosave of '{name}' was found."));
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Recover").strong()).clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Discard").clicked() {
                        choice = Some(false);
                    }
                });
            });
        match choice {
            Some(true) => {
                let p = self.library.recovery.take().unwrap();
                self.load_project(p, None);
                self.set_status("Recovered your unsaved work — save it with Ctrl+S", false);
            }
            Some(false) => {
                self.library.discard_recovery();
                self.presets_open = true;
            }
            None => {}
        }
    }

    fn preset_name_window(&mut self, ctx: &egui::Context) {
        let Some(mut name) = self.preset_name.take() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        egui::Window::new("Save as my preset")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("It will appear in the gallery under My presets, with a thumbnail of the current frame.");
                let r = ui.text_edit_singleline(&mut name);
                r.request_focus();
                if ui.button("Save").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    save = true;
                }
            });
        if save && !name.trim().is_empty() {
            self.save_user_preset(ctx, name.trim().to_string());
        } else if open {
            self.preset_name = Some(name);
        }
    }

    fn randomize_window(&mut self, ctx: &egui::Context) {
        let mut open = self.randomize_open;
        egui::Window::new("Randomizer")
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                let o = &mut self.rand_opts;
                ui.checkbox(&mut o.colors, "Colours");
                ui.checkbox(&mut o.shapes, "Shapes & layouts");
                ui.checkbox(&mut o.motion, "Motion");
                ui.checkbox(&mut o.post, "Post effects");
                ui.add(egui::Slider::new(&mut o.strength, 0.0..=1.0).text("strength"));
                ui.horizontal(|ui| {
                    ui.label("Seed");
                    ui.add(egui::DragValue::new(&mut self.rand_seed));
                });
                if ui.button("🎲 Randomize now").clicked() {
                    self.rand_seed = self.rand_seed.wrapping_add(1);
                    randomize(&mut self.project, self.rand_seed, self.rand_opts);
                }
                ui.label(RichText::new("Ctrl+Z undoes a randomization.").weak());
            });
        self.randomize_open = open;
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.help_open;
        egui::Window::new("How it works").open(&mut open).default_width(460.0).show(ctx, |ui| {
            ui.label(RichText::new("Everything loops.").strong());
            ui.label("A scene lasts a whole number of beats. Every motion (spins, orbits, pulses, particles, scrolling) completes an exact number of cycles per loop, so exports repeat seamlessly.");
            ui.add_space(6.0);
            ui.label(RichText::new("Building a scene").strong());
            ui.label("• Start from a preset (or Surprise me).\n• Layers are drawn together: backgrounds, a mirror floor, shapes (with copies laid out in rings, grids, walls, swarms…) and particles.\n• Symmetry duplicates a layer around the centre: mirror or kaleidoscope.\n• Click ~ next to a value to animate it: pick a wave and how many times per loop it repeats. Use the number of beats to pulse on every beat.\n• Post effects give the final look: bloom glow, kaleidoscope, retro palettes, pixels, CRT.");
            ui.add_space(6.0);
            ui.label(RichText::new("Shortcuts").strong());
            ui.label("Space play/pause · Ctrl+S save · Ctrl+O open · Ctrl+E export · Ctrl+Z / Ctrl+Shift+Z undo/redo · drag in the viewport to orbit, scroll to zoom · drop models, images, music or projects onto the window.");
        });
        self.help_open = open;
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let (space, save, open, export, undo, redo) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            (
                !typing && i.key_pressed(egui::Key::Space),
                cmd && i.key_pressed(egui::Key::S),
                cmd && i.key_pressed(egui::Key::O),
                cmd && i.key_pressed(egui::Key::E),
                cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                cmd && (i.modifiers.shift && i.key_pressed(egui::Key::Z)
                    || i.key_pressed(egui::Key::Y)),
            )
        });
        if space {
            self.playing = !self.playing;
        }
        if !typing {
            ctx.input(|i| {
                if !i.modifiers.command {
                    if i.key_pressed(egui::Key::W) {
                        self.gizmo.mode = GizmoMode::Move;
                    }
                    if i.key_pressed(egui::Key::E) {
                        self.gizmo.mode = GizmoMode::Rotate;
                    }
                    if i.key_pressed(egui::Key::R) {
                        self.gizmo.mode = GizmoMode::Scale;
                    }
                    if i.key_pressed(egui::Key::G) {
                        self.gizmo.grid = !self.gizmo.grid;
                    }
                }
            });
        }
        if save {
            self.save(false);
        }
        if open {
            self.open_dialog();
        }
        if export {
            self.export.open = true;
        }
        if undo && !typing {
            self.undo();
        }
        if redo && !typing {
            self.redo();
        }
    }
}

impl eframe::App for EzApp {
    fn on_exit(&mut self) {
        self.library.clean_exit();
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dt = ctx.input(|i| i.stable_dt).min(0.1) as f64;
        self.now = ctx.input(|i| i.time);
        if self.playing {
            self.time = (self.time + dt).rem_euclid(self.loop_seconds().max(0.01));
        }
        let loop_s = self.loop_seconds() as f32;
        let t = (self.phase() * loop_s).max(0.0);
        if let Some(a) = &mut self.audio {
            a.sync(self.playing, t);
        }

        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            self.handle_dropped(dropped);
        }
        self.shortcuts(&ctx);

        // Node mode: keep graph and editor in sync.
        if self.mode == Mode::Nodes {
            if self.nodes.is_none() {
                let g = self
                    .project
                    .graph
                    .clone()
                    .unwrap_or_else(|| Graph::from_layers(&self.project.layers));
                self.nodes = Some(NodeEditor::from_graph(&g));
            }
            self.project.use_graph = true;
        }

        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("timeline")
            .exact_size(40.0)
            .show(ui, |ui| {
                ui.add_space(6.0);
                self.timeline(ui)
            });
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(340.0)
            .show(ui, |ui| self.inspector_panel(ui));
        if self.mode == Mode::Simple {
            egui::Panel::left("scene")
                .resizable(true)
                .default_size(230.0)
                .show(ui, |ui| self.scene_panel(ui));
            egui::CentralPanel::default().show(ui, |ui| self.viewport_ui(ui));
        } else {
            egui::Panel::bottom("graph")
                .resizable(true)
                .min_size(220.0)
                .default_size(380.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("Node graph");
                        ui.checkbox(&mut self.project.use_graph, "render the graph");
                        if ui.button("Rebuild from layers").on_hover_text("Replace the graph with one node per layer").clicked() {
                            self.nodes = Some(NodeEditor::from_graph(&Graph::from_layers(&self.project.layers)));
                        }
                        if ui.button("Bake to layers").on_hover_text("Turn the graph result into plain layers and go back to Simple mode").clicked() {
                            if let Some(g) = &self.project.graph {
                                self.project.layers = g.compile();
                            }
                            self.project.use_graph = false;
                            self.mode = Mode::Simple;
                        }
                    });
                    if let Some(n) = &mut self.nodes {
                        n.show(ui, &self.library.template_layers());
                    }
                });
            egui::CentralPanel::default().show(ui, |ui| self.viewport_ui(ui));
            if let Some(n) = &self.nodes {
                let g = n.to_graph();
                if self.project.graph.as_ref() != Some(&g) {
                    self.project.graph = Some(g);
                }
            }
        }

        self.presets_window(&ctx);
        self.recovery_window(&ctx);
        self.preset_name_window(&ctx);
        self.autosave();
        self.randomize_window(&ctx);
        self.help_window(&ctx);
        self.export
            .show(&ctx, &self.project, self.audio_env.as_ref());

        let pointer_down = ctx.input(|i| i.pointer.any_down());
        self.commit_history(pointer_down);

        let dirty = if self.project != self.saved {
            "•"
        } else {
            ""
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "EZ2DEMOSCENE — {}{dirty}",
            self.project.name
        )));
        if self.playing || self.export.is_running() {
            ctx.request_repaint();
        }
    }
}

fn setup_style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.selection.bg_fill = Color32::from_rgb(150, 30, 90);
    v.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    v.panel_fill = Color32::from_rgb(18, 18, 24);
    v.window_fill = Color32::from_rgb(24, 24, 32);
    v.extreme_bg_color = Color32::from_rgb(10, 10, 14);
    v.widgets.noninteractive.bg_fill = Color32::from_rgb(22, 22, 30);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.slider_width = 130.0;
    });
}
