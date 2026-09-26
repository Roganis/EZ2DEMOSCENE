//! Property editors for every part of a project.

use crate::platform::{self, LayerRef, Purpose, TexSlot};
use crate::widgets::*;
use egui::{RichText, Ui};
use ez_core::palette::PaletteId;
use ez_core::*;
use ez_render::texgen;

pub const MODEL_EXTENSIONS: &[&str] = &["gltf", "glb", "obj"];
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "tga"];

/// Adds an image as a user texture and returns its name.
/// `path` is an asset path (file or `mem://`), `file_name` its display name.
pub fn add_user_texture(textures: &mut Vec<UserTexture>, path: &str, file_name: &str) -> String {
    let path_s = path.to_string();
    if let Some(t) = textures.iter().find(|t| t.path == path_s) {
        return t.name.clone();
    }
    let stem = file_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(file_name)
        .to_string();
    let stem = if stem.is_empty() {
        "image".to_string()
    } else {
        stem
    };
    let mut name = stem.clone();
    let mut k = 2;
    while textures.iter().any(|t| t.name == name) || texgen::is_builtin(&name) {
        name = format!("{stem}_{k}");
        k += 1;
    }
    textures.push(UserTexture {
        name: name.clone(),
        path: path_s,
        retro: None,
    });
    name
}

/// Texture chooser: none, built-ins, user images, or import a new one.
fn texture_picker(
    ui: &mut Ui,
    label: &str,
    tex: &mut Option<String>,
    textures: &[UserTexture],
    target: Option<(LayerRef, TexSlot)>,
) {
    row(ui, label, "Image mapped onto the surface", |ui| {
        let text = tex.clone().unwrap_or_else(|| "None".into());
        egui::ComboBox::from_id_salt(ui.id().with(label))
            .selected_text(text)
            .width(150.0)
            .height(400.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(tex, None, "None");
                ui.separator();
                ui.label(RichText::new("Built-in retro pack").weak());
                for (name, desc) in texgen::BUILTIN {
                    ui.selectable_value(tex, Some(name.to_string()), *name)
                        .on_hover_text(*desc);
                }
                if !textures.is_empty() {
                    ui.separator();
                    ui.label(RichText::new("Your images").weak());
                    for t in textures.iter() {
                        ui.selectable_value(tex, Some(t.name.clone()), &t.name);
                    }
                }
                ui.separator();
                if let Some((lref, slot)) = target {
                    if ui.button("Import image…").clicked() {
                        platform::pick(Purpose::SetTexture(lref, slot));
                    }
                }
            });
    });
}

// ---------------------------------------------------------------------------
// Scene-wide editors

pub fn timing_ui(ui: &mut Ui, p: &mut Project) {
    ui.heading("Timing & music");
    ui.label(
        RichText::new("The loop is a whole number of beats: every animation fits exactly, so it repeats seamlessly.")
            .weak(),
    );
    ui.add_space(4.0);
    slider(
        ui,
        "Tempo (BPM)",
        "Beats per minute",
        &mut p.timing.bpm,
        40.0..=220.0,
    );
    drag_u(
        ui,
        "Loop length",
        "Length of the loop in beats",
        &mut p.timing.loop_beats,
        1..=256,
    );
    ui.horizontal(|ui| {
        for b in [4, 8, 16, 32, 64] {
            if ui.small_button(format!("{b} beats")).clicked() {
                p.timing.loop_beats = b;
            }
        }
    });
    ui.label(format!("= {:.2} seconds", p.timing.loop_seconds()));
}

pub fn camera_ui(ui: &mut Ui, c: &mut Camera) {
    ui.heading("Camera");
    ui.label(RichText::new("Tip: drag in the viewport to turn the camera, scroll to zoom.").weak());
    combo(
        ui,
        "Motion",
        "How the camera moves during the loop",
        &mut c.mode,
        &CameraMode::ALL,
        |m| m.label(),
    );
    vec3(
        ui,
        "Look at",
        "Point the camera aims at",
        &mut c.target,
        0.05,
    );
    param(
        ui,
        "Distance",
        "Distance from the target",
        &mut c.distance,
        0.5..=60.0,
    );
    param(
        ui,
        "Height",
        "Height above the target",
        &mut c.height,
        -20.0..=40.0,
    );
    param(
        ui,
        "Start angle",
        "Starting direction in degrees",
        &mut c.angle,
        -180.0..=180.0,
    );
    match c.mode {
        CameraMode::Orbit => {
            drag_i(
                ui,
                "Turns per loop",
                "Full circles per loop (negative = other way)",
                &mut c.orbit_turns,
                -8..=8,
            );
        }
        CameraMode::Pendulum => {
            param(
                ui,
                "Swing",
                "Swing amplitude in degrees",
                &mut c.swing,
                0.0..=180.0,
            );
        }
        CameraMode::Static => {}
    }
    param(
        ui,
        "Field of view",
        "Zoom lens: small = telephoto, large = wide",
        &mut c.fov,
        10.0..=140.0,
    );
    param(
        ui,
        "Roll",
        "Tilt the horizon (degrees)",
        &mut c.roll,
        -180.0..=180.0,
    );
    param(
        ui,
        "Beat shake",
        "Camera kick on every beat",
        &mut c.beat_shake,
        0.0..=1.0,
    );
}

pub fn environment_ui(ui: &mut Ui, e: &mut Environment) {
    ui.heading("Light & atmosphere");
    color(
        ui,
        "Fog colour",
        "Colour distant things fade into",
        &mut e.fog_color,
    );
    param(
        ui,
        "Fog density",
        "How quickly things fade into the fog",
        &mut e.fog_density,
        0.0..=0.2,
    );
    color(
        ui,
        "Sky light",
        "Ambient light from above, also seen in reflections",
        &mut e.sky_color,
    );
    color(
        ui,
        "Ground light",
        "Ambient light from below",
        &mut e.ground_color,
    );
    param(
        ui,
        "Ambient",
        "Strength of the ambient light",
        &mut e.ambient,
        0.0..=2.0,
    );
    color(ui, "Sun colour", "", &mut e.light_color);
    param(ui, "Sun strength", "", &mut e.light_intensity, 0.0..=5.0);
    vec3(
        ui,
        "Sun direction",
        "Direction the light comes from",
        &mut e.light_dir,
        0.02,
    );
}

pub fn post_ui(ui: &mut Ui, post: &mut PostStack) {
    ui.heading("Post effects");
    ui.label(RichText::new("Applied to the whole picture, top to bottom.").weak());
    toggle_section(ui, "Kaleidoscope", &mut post.kaleido.enabled, |ui| {
        drag_u(
            ui,
            "Segments",
            "Number of mirrored wedges",
            &mut post.kaleido.segments,
            1..=32,
        );
        param(
            ui,
            "Angle",
            "Rotation in degrees",
            &mut post.kaleido.angle,
            -180.0..=180.0,
        );
        drag_i(
            ui,
            "Turns per loop",
            "Spin the kaleidoscope",
            &mut post.kaleido.turns,
            -8..=8,
        );
        param(ui, "Zoom", "", &mut post.kaleido.zoom, 0.2..=4.0);
        let center = &mut post.kaleido.center;
        row(ui, "Centre", "Centre of the kaleidoscope (0..1)", |ui| {
            ui.add(
                egui::DragValue::new(&mut center[0])
                    .speed(0.005)
                    .prefix("x "),
            );
            ui.add(
                egui::DragValue::new(&mut center[1])
                    .speed(0.005)
                    .prefix("y "),
            );
        });
    });
    toggle_section(ui, "Mirror split", &mut post.mirror.enabled, |ui| {
        combo(
            ui,
            "Mode",
            "Which half gets mirrored",
            &mut post.mirror.mode,
            &MirrorSplitMode::ALL,
            |m| m.label(),
        );
    });
    toggle_section(ui, "Bloom / glow", &mut post.bloom.enabled, |ui| {
        param(ui, "Intensity", "", &mut post.bloom.intensity, 0.0..=3.0);
        param(
            ui,
            "Threshold",
            "Brightness above which things glow",
            &mut post.bloom.threshold,
            0.0..=4.0,
        );
        param(ui, "Spread", "", &mut post.bloom.radius, 0.0..=1.0);
    });
    toggle_section(ui, "God rays & lens flare", &mut post.rays.enabled, |ui| {
        combo(
            ui,
            "Light",
            "Where the rays stream from",
            &mut post.rays.source,
            &RaySource::ALL,
            |s| s.label(),
        );
        param(ui, "Intensity", "", &mut post.rays.intensity, 0.0..=4.0);
        param(
            ui,
            "Length",
            "How far the rays reach",
            &mut post.rays.length,
            0.0..=1.0,
        );
        param(
            ui,
            "Threshold",
            "Brightness above which the picture casts rays",
            &mut post.rays.threshold,
            0.0..=4.0,
        );
        color(ui, "Tint", "", &mut post.rays.tint);
        param(
            ui,
            "Lens flare",
            "Ghosts and a streak when looking into the light",
            &mut post.rays.flare,
            0.0..=3.0,
        );
    });
    toggle_section(ui, "Chromatic aberration", &mut post.chroma.enabled, |ui| {
        param(
            ui,
            "Amount",
            "RGB split towards the edges",
            &mut post.chroma.amount,
            0.0..=0.03,
        );
    });
    toggle_section(ui, "Pixelate", &mut post.pixelate.enabled, |ui| {
        param(
            ui,
            "Pixel size",
            "Size of the fat pixels (at 1080p)",
            &mut post.pixelate.size,
            1.0..=16.0,
        );
    });
    toggle_section(ui, "Retro palette", &mut post.palette.enabled, |ui| {
        combo(
            ui,
            "Palette",
            "Reduce colours to a classic computer palette",
            &mut post.palette.palette,
            &PaletteId::ALL,
            |p| p.label(),
        );
        param(
            ui,
            "Dithering",
            "Ordered (Bayer) dithering strength",
            &mut post.palette.dither,
            0.0..=1.0,
        );
        palette_swatch(ui, post.palette.palette);
    });
    toggle_section(ui, "CRT monitor", &mut post.crt.enabled, |ui| {
        param(ui, "Scanlines", "", &mut post.crt.scanlines, 0.0..=1.0);
        param(ui, "Curvature", "", &mut post.crt.curvature, 0.0..=1.0);
        param(ui, "VHS wobble", "", &mut post.crt.noise, 0.0..=1.0);
    });
    section(ui, "Colour grading", true, |ui| {
        let g = &mut post.grade;
        param(ui, "Exposure", "", &mut g.exposure, 0.0..=4.0);
        param(ui, "Contrast", "", &mut g.contrast, 0.5..=2.0);
        param(ui, "Saturation", "", &mut g.saturation, 0.0..=2.0);
        param(
            ui,
            "Vignette",
            "Darken the corners",
            &mut g.vignette,
            0.0..=1.5,
        );
        param(ui, "Film grain", "", &mut g.grain, 0.0..=0.2);
        param(
            ui,
            "Beat flash",
            "White flash on every beat",
            &mut g.beat_flash,
            0.0..=1.0,
        );
    });
}

fn palette_swatch(ui: &mut Ui, p: PaletteId) {
    let cols = p.colors();
    if cols.is_empty() {
        ui.label(RichText::new("216 colours (6 levels per channel)").weak());
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for c in cols {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().rect_filled(
                rect,
                2.0,
                egui::Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, *c as u8),
            );
        }
    });
}

pub fn textures_ui(ui: &mut Ui, textures: &mut Vec<UserTexture>) {
    ui.heading("Your images");
    ui.label(RichText::new("Images you imported. 'Retro-ize' shrinks them and remaps them to an old-school palette.").weak());
    if ui.button("Import image…").clicked() {
        platform::pick(Purpose::AddImages);
    }
    let mut remove = None;
    for (i, t) in textures.iter_mut().enumerate() {
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong(&t.name);
            if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                remove = Some(i);
            }
        });
        ui.label(RichText::new(&t.path).weak().small());
        let mut retro = t.retro.is_some();
        if ui.checkbox(&mut retro, "Retro-ize").changed() {
            t.retro = retro.then(RetroProcess::default);
        }
        if let Some(r) = &mut t.retro {
            drag_u(
                ui,
                "Max size",
                "Longest side in pixels (0 = keep)",
                &mut r.max_size,
                0..=1024,
            );
            combo(ui, "Palette", "", &mut r.palette, &PaletteId::ALL, |p| {
                p.label()
            });
            slider(ui, "Dithering", "", &mut r.dither, 0.0..=1.0);
        }
    }
    if let Some(i) = remove {
        textures.remove(i);
    }
}

// ---------------------------------------------------------------------------
// Layers

/// `lref` identifies the layer so file imports can be applied to it later.
pub fn layer_ui(ui: &mut Ui, layer: &mut Layer, textures: &[UserTexture], lref: LayerRef) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut layer.enabled, "");
        ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(180.0));
        ui.label(RichText::new(layer.type_label()).weak());
    });
    ui.add_space(4.0);
    match &mut layer.kind {
        LayerKind::Mesh(m) => mesh_ui(ui, m, textures, lref),
        LayerKind::Particles(p) => particles_ui(ui, p),
        LayerKind::Backdrop(b) => backdrop_ui(ui, b, textures, lref),
        LayerKind::Mirror(f) => mirror_ui(ui, f, textures, lref),
        LayerKind::Terrain(t) => terrain_ui(ui, t, textures, lref),
        LayerKind::Lasers(z) => lasers_ui(ui, z),
        LayerKind::Ribbon(r) => ribbon_ui(ui, r),
        LayerKind::Weather(w) => weather_ui(ui, w),
    }
    let is_mesh_like = matches!(
        layer.kind,
        LayerKind::Mesh(_) | LayerKind::Particles(_) | LayerKind::Lasers(_) | LayerKind::Ribbon(_)
    );
    let is_backdrop = matches!(layer.kind, LayerKind::Backdrop(_));
    if !is_backdrop {
        section(ui, "Placement & motion", true, |ui| {
            let t = &mut layer.transform;
            if matches!(layer.kind, LayerKind::Mirror(_)) {
                slider(ui, "Floor height", "", &mut t.position[1], -10.0..=10.0);
                return;
            }
            if matches!(layer.kind, LayerKind::Weather(_)) {
                slider(
                    ui,
                    "Ground height",
                    "Where rain splashes and embers start (the weather follows the camera)",
                    &mut t.position[1],
                    -10.0..=10.0,
                );
                return;
            }
            vec3(ui, "Position", "", &mut t.position, 0.05);
            vec3(ui, "Rotation", "Degrees", &mut t.rotation, 0.5);
            param(
                ui,
                "Size",
                "Size of the object (or of each copy)",
                &mut t.scale,
                0.0..=10.0,
            );
            vec3(ui, "Stretch", "Per-axis stretch", &mut t.stretch, 0.01);
            ivec3(
                ui,
                "Spin / loop",
                "Whole turns per loop around each axis",
                &mut t.spin,
                -16..=16,
            );
            param(
                ui,
                "Bob",
                "Up/down offset (animate it!)",
                &mut t.bob,
                -5.0..=5.0,
            );
            ui.add_space(4.0);
            ui.label(RichText::new("Shake").strong())
                .on_hover_text("Random jolts on a rhythm");
            shake_ui(ui, &mut t.shake);
        });
    }
    if is_mesh_like {
        section(ui, "Symmetry", true, |ui| {
            symmetry_ui(ui, &mut layer.symmetry)
        });
    }
    section(ui, "Blink / strobe", false, |ui| {
        blink_ui(ui, &mut layer.blink)
    });
}

/// Random jolts on a rhythm (also driven by the Jitter node).
fn shake_ui(ui: &mut Ui, s: &mut Shake) {
    param(
        ui,
        "Shake distance",
        "How far each jolt moves the layer. Try ~ → Exp fade out, every beat.",
        &mut s.amount,
        0.0..=3.0,
    );
    param(
        ui,
        "Shake turn",
        "How far each jolt turns the layer (degrees)",
        &mut s.turn,
        0.0..=90.0,
    );
    if s.is_active() {
        drag_u(
            ui,
            "New direction",
            "Times per loop the shake picks a new random direction (16 = every beat of a 16-beat loop)",
            &mut s.per_loop,
            1..=256,
        );
        drag_u(
            ui,
            "Seed",
            "Different random directions",
            &mut s.seed,
            0..=9999,
        );
    }
    if ui
        .small_button("⚡ Beat jolt")
        .on_hover_text("Jolt on every beat, then settle (Exp fade out)")
        .clicked()
    {
        let beats = crate::widgets::loop_beats(ui) as i32;
        s.per_loop = beats as u32;
        let dist = s.amount.base.max(s.amount.amp);
        s.amount = Param::new(0.0).osc(Wave::ExpOut, if dist > 0.0 { dist } else { 0.3 }, beats);
        let deg = s.turn.base.max(s.turn.amp);
        s.turn = Param::new(0.0).osc(Wave::ExpOut, if deg > 0.0 { deg } else { 8.0 }, beats);
    }
}

/// Blink / strobe settings (also used by the Strobe node).
pub fn blink_ui(ui: &mut Ui, b: &mut Blink) {
    combo(ui, "Mode", "", &mut b.mode, &BlinkMode::ALL, |m| m.label());
    if b.mode == BlinkMode::Off {
        return;
    }
    drag_u(
        ui,
        "Times / loop",
        "Blinks per loop (16 = every beat of a 16-beat loop)",
        &mut b.per_loop,
        1..=256,
    );
    let duty_label = match b.mode {
        BlinkMode::Blink => "On time",
        BlinkMode::Random => "Chance on",
        _ => "Dim level",
    };
    slider(ui, duty_label, "", &mut b.duty, 0.0..=1.0);
    slider(ui, "Offset", "Shift the rhythm", &mut b.offset, 0.0..=1.0);
    if b.mode == BlinkMode::Random {
        drag_u(ui, "Seed", "Different random rhythm", &mut b.seed, 0..=9999);
    }
}

fn symmetry_ui(ui: &mut Ui, s: &mut Symmetry) {
    ui.label(RichText::new("Copies the whole layer around the centre of the world.").weak());
    let options = [
        Symmetry::None,
        Symmetry::MirrorX,
        Symmetry::MirrorZ,
        Symmetry::MirrorXZ,
        Symmetry::Radial { count: 6 },
        Symmetry::Kaleido { count: 8 },
    ];
    row(ui, "Mode", "", |ui| {
        egui::ComboBox::from_id_salt("symmetry")
            .selected_text(s.label())
            .show_ui(ui, |ui| {
                for o in options {
                    if ui
                        .selectable_label(
                            std::mem::discriminant(s) == std::mem::discriminant(&o),
                            o.label(),
                        )
                        .clicked()
                        && std::mem::discriminant(s) != std::mem::discriminant(&o)
                    {
                        *s = o;
                    }
                }
            });
    });
    match s {
        Symmetry::Radial { count } | Symmetry::Kaleido { count } => {
            drag_u(ui, "Copies", "", count, 1..=64);
        }
        _ => {}
    }
}

fn primitive_params_ui(ui: &mut Ui, p: &mut Primitive) {
    match p {
        Primitive::Sphere { detail } => {
            drag_u(ui, "Detail", "Subdivisions (0 = faceted)", detail, 0..=5);
        }
        Primitive::Torus {
            thickness,
            segments,
        } => {
            slider(ui, "Thickness", "", thickness, 0.02..=0.9);
            drag_u(ui, "Segments", "", segments, 6..=128);
        }
        Primitive::Cylinder { segments } => {
            drag_u(ui, "Sides", "", segments, 3..=64);
        }
        Primitive::Shard { seed } => {
            drag_u(ui, "Seed", "Different random shape", seed, 0..=9999);
        }
        Primitive::Crystal { spikes, seed } => {
            drag_u(ui, "Spikes", "", spikes, 1..=32);
            drag_u(ui, "Seed", "Different random shape", seed, 0..=9999);
        }
        Primitive::Panel { bevel } => {
            slider(ui, "Bevel", "", bevel, 0.0..=0.45);
        }
        Primitive::Ring {
            arc,
            width,
            height,
            segments,
        } => {
            slider(ui, "Arc", "Degrees of the circle covered", arc, 1.0..=360.0);
            slider(
                ui,
                "Width",
                "Band width (fraction of the radius)",
                width,
                0.005..=1.0,
            );
            slider(ui, "Height", "", height, 0.005..=1.0);
            drag_u(ui, "Segments", "", segments, 2..=256);
        }
        Primitive::Cone { segments } => {
            drag_u(ui, "Sides", "", segments, 3..=64);
        }
        Primitive::Capsule { length, segments } => {
            slider(
                ui,
                "Length",
                "Straight middle part (0 = sphere)",
                length,
                0.0..=4.0,
            );
            drag_u(ui, "Segments", "", segments, 6..=64);
        }
        Primitive::TorusKnot { p, q, thickness } => {
            drag_u(ui, "Loops (p)", "Times around the ring", p, 1..=12);
            drag_u(ui, "Twists (q)", "Times through the hole", q, 1..=12);
            slider(ui, "Thickness", "", thickness, 0.01..=0.3);
        }
        Primitive::Star {
            points,
            inner,
            depth,
        } => {
            drag_u(ui, "Points", "", points, 3..=32);
            slider(ui, "Inner radius", "", inner, 0.05..=0.95);
            slider(ui, "Depth", "Thickness of the extrusion", depth, 0.01..=2.0);
        }
        Primitive::Gear { teeth, depth } => {
            drag_u(ui, "Teeth", "", teeth, 4..=64);
            slider(ui, "Depth", "Thickness of the extrusion", depth, 0.01..=2.0);
        }
        Primitive::Spring { turns, thickness } => {
            slider(ui, "Turns", "", turns, 0.5..=20.0);
            slider(ui, "Thickness", "", thickness, 0.01..=0.3);
        }
        Primitive::Menger { level } => {
            drag_u(ui, "Level", "Fractal depth (3 = 8000 cubes)", level, 0..=3);
        }
        Primitive::RoundedCube { radius } => {
            slider(ui, "Roundness", "", radius, 0.0..=0.5);
        }
        Primitive::Gem { facets } => {
            drag_u(ui, "Facets", "", facets, 4..=32);
        }
        Primitive::Heart { depth } => {
            slider(ui, "Depth", "Thickness of the extrusion", depth, 0.01..=2.0);
        }
        Primitive::Mobius { width } => {
            slider(ui, "Width", "", width, 0.05..=0.9);
        }
        _ => {}
    }
}

fn mesh_ui(ui: &mut Ui, m: &mut MeshLayer, textures: &[UserTexture], lref: LayerRef) {
    section(ui, "Shape", true, |ui| {
        let label = match &m.source {
            MeshSource::Primitive(p) => p.label().to_string(),
            MeshSource::File { path } => ez_core::store::file_name(path).to_string(),
        };
        row(ui, "Shape", "", |ui| {
            egui::ComboBox::from_id_salt("shape")
                .selected_text(label)
                .width(150.0)
                .height(400.0)
                .show_ui(ui, |ui| {
                    for p in Primitive::all_defaults() {
                        let same = matches!(&m.source, MeshSource::Primitive(q) if std::mem::discriminant(q) == std::mem::discriminant(&p));
                        if ui.selectable_label(same, p.label()).clicked() && !same {
                            m.source = MeshSource::Primitive(p);
                        }
                    }
                    ui.separator();
                    if ui.button("3D model file (glTF / OBJ)…").clicked() {
                        platform::pick(Purpose::SetModel(lref));
                    }
                });
        });
        if let MeshSource::Primitive(p) = &mut m.source {
            primitive_params_ui(ui, p);
        }
    });
    section(ui, "Material", true, |ui| {
        material_ui(ui, &mut m.material, textures, lref)
    });
    section(ui, "Relief (bump / normal / displacement)", false, |ui| {
        relief_ui(
            ui,
            &mut m.material.relief,
            m.material.texture.is_some(),
            &mut m.subdivide,
            textures,
            lref,
        )
    });
    section(ui, "Glitch", false, |ui| {
        glitch_ui(ui, &mut m.material.glitch)
    });
    section(ui, "Copies (instancing)", true, |ui| {
        instancer_ui(ui, &mut m.instancer)
    });
    if !matches!(m.instancer, Instancer::Single) {
        section(ui, "Variation", false, |ui| {
            variation_ui(ui, &mut m.variation)
        });
    }
}

fn relief_ui(
    ui: &mut Ui,
    r: &mut Relief,
    has_colour_texture: bool,
    subdivide: &mut u32,
    textures: &[UserTexture],
    lref: LayerRef,
) {
    ui.label(
        RichText::new(
            "Uses the material's Tiling and Scroll. Without a relief texture the colour texture is used.",
        )
        .weak(),
    );
    texture_picker(
        ui,
        "Relief texture",
        &mut r.texture,
        textures,
        Some((lref, TexSlot::Relief)),
    );
    if r.texture.is_none() && !has_colour_texture {
        ui.label(
            RichText::new("Pick a relief texture (or a colour texture) to see it.")
                .color(egui::Color32::LIGHT_YELLOW),
        );
    }
    combo(
        ui,
        "Mode",
        "How the texture is read",
        &mut r.mode,
        &ReliefMode::ALL,
        |m| m.label(),
    );
    let tip = match r.mode {
        ReliefMode::Bump => "Bright = raised, dark = sunken (lighting only)",
        ReliefMode::NormalMap => "Strength of a normal-map image (the purple-blue kind)",
    };
    param(ui, "Relief", tip, &mut r.bump, 0.0..=4.0);
    param(
        ui,
        "Displacement",
        "Really moves the surface out by the texture brightness. Raise Subdivide for detail. \
         Works best on smooth shapes (sphere, torus, capsule, rounded cube): faceted ones open at their edges.",
        &mut r.displace,
        -1.0..=1.0,
    );
    drag_u(
        ui,
        "Subdivide",
        "Extra geometry detail for displacement (each level = 4× triangles)",
        subdivide,
        0..=4,
    );
}

fn glitch_ui(ui: &mut Ui, g: &mut Glitch) {
    param(
        ui,
        "Amount",
        "Corrupt the shape (0 = off). Animate it for bursts.",
        &mut g.amount,
        0.0..=2.0,
    );
    combo(ui, "Style", "", &mut g.style, &GlitchStyle::ALL, |s| {
        s.label()
    });
    drag_u(
        ui,
        "Changes / loop",
        "How many new random patterns per loop",
        &mut g.rate,
        1..=128,
    );
    param(
        ui,
        "Chance",
        "Fraction of those moments that glitch (1 = always)",
        &mut g.chance,
        0.0..=1.0,
    );
    drag_u(
        ui,
        "Seed",
        "Different random pattern",
        &mut g.seed,
        0..=9999,
    );
}

fn material_ui(ui: &mut Ui, mat: &mut Material, textures: &[UserTexture], lref: LayerRef) {
    color(ui, "Colour", "", &mut mat.base_color);
    param(
        ui,
        "Metallic",
        "0 = plastic, 1 = metal (reflects its colour)",
        &mut mat.metallic,
        0.0..=1.0,
    );
    param(
        ui,
        "Roughness",
        "0 = mirror-glossy, 1 = matte",
        &mut mat.roughness,
        0.0..=1.0,
    );
    check(
        ui,
        "Faceted",
        "Flat-shaded low-poly look",
        &mut mat.flat_shading,
    );
    param(
        ui,
        "Rim light",
        "Glow along the silhouette",
        &mut mat.rim,
        0.0..=2.0,
    );
    ui.separator();
    color(ui, "Glow colour", "", &mut mat.emissive_color);
    param(
        ui,
        "Glow",
        "Self-illumination strength (pulse it on the beat!)",
        &mut mat.emissive,
        0.0..=10.0,
    );
    combo(
        ui,
        "Glow where",
        "Which part of the surface glows",
        &mut mat.emissive_mode,
        &EmissiveMode::ALL,
        |m| m.label(),
    );
    param(
        ui,
        "Hue shift",
        "Rotate the colours (in turns)",
        &mut mat.hue_shift,
        -1.0..=1.0,
    );
    ui.separator();
    texture_picker(
        ui,
        "Texture",
        &mut mat.texture,
        textures,
        Some((lref, TexSlot::Material)),
    );
    if mat.texture.is_some() {
        param(ui, "Tiling", "", &mut mat.texture_scale, 0.1..=16.0);
        row(
            ui,
            "Scroll / loop",
            "Tiles scrolled per loop (U, V)",
            |ui| {
                ui.add(
                    egui::DragValue::new(&mut mat.scroll[0])
                        .range(-16..=16)
                        .speed(0.1)
                        .prefix("u "),
                );
                ui.add(
                    egui::DragValue::new(&mut mat.scroll[1])
                        .range(-16..=16)
                        .speed(0.1)
                        .prefix("v "),
                );
            },
        );
        check(
            ui,
            "Chunky pixels",
            "Nearest-neighbour sampling",
            &mut mat.pixelated,
        );
    }
}

fn instancer_ui(ui: &mut Ui, inst: &mut Instancer) {
    row(ui, "Layout", "How copies are arranged", |ui| {
        egui::ComboBox::from_id_salt("instancer")
            .selected_text(inst.label())
            .show_ui(ui, |ui| {
                for o in Instancer::defaults() {
                    let same = std::mem::discriminant(inst) == std::mem::discriminant(&o);
                    if ui.selectable_label(same, o.label()).clicked() && !same {
                        *inst = o;
                    }
                }
            });
    });
    match inst {
        Instancer::Single => {}
        Instancer::Grid { counts, spacing } => {
            row(ui, "Count", "", |ui| {
                for (i, a) in ["x", "y", "z"].iter().enumerate() {
                    ui.add(
                        egui::DragValue::new(&mut counts[i])
                            .range(1..=64)
                            .speed(0.1)
                            .prefix(format!("{a} ")),
                    );
                }
            });
            vec3(ui, "Spacing", "", spacing, 0.02);
        }
        Instancer::Radial { count, radius } => {
            drag_u(ui, "Count", "", count, 1..=1024);
            slider(ui, "Radius", "", radius, 0.0..=60.0);
        }
        Instancer::Scatter {
            count,
            radius,
            shell,
            seed,
        } => {
            drag_u(ui, "Count", "", count, 1..=5000);
            slider(ui, "Radius", "", radius, 0.0..=60.0);
            check(
                ui,
                "Only surface",
                "Place copies on the sphere surface",
                shell,
            );
            drag_u(ui, "Seed", "", seed, 0..=9999);
        }
        Instancer::Orbit {
            count,
            radius,
            spread,
            speed,
            seed,
        } => {
            drag_u(ui, "Count", "", count, 1..=5000);
            slider(ui, "Radius", "", radius, 0.0..=40.0);
            slider(ui, "Spread", "", spread, 0.0..=10.0);
            drag_i(ui, "Orbits / loop", "", speed, -8..=8);
            drag_u(ui, "Seed", "", seed, 0..=9999);
        }
        Instancer::Wall {
            cols,
            rows,
            spacing,
            curve,
        } => {
            drag_u(ui, "Columns", "", cols, 1..=128);
            drag_u(ui, "Rows", "", rows, 1..=128);
            slider(ui, "Spacing", "", spacing, 0.05..=10.0);
            slider(
                ui,
                "Curve",
                "Bend the wall into an arc (degrees)",
                curve,
                -360.0..=360.0,
            );
        }
        Instancer::Spiral {
            count,
            radius,
            height,
            turns,
        } => {
            drag_u(ui, "Count", "", count, 1..=4096);
            slider(ui, "Radius", "", radius, 0.0..=30.0);
            slider(ui, "Height", "", height, -30.0..=30.0);
            slider(ui, "Turns", "", turns, 0.0..=20.0);
        }
    }
}

fn variation_ui(ui: &mut Ui, v: &mut Variation) {
    drag_u(ui, "Seed", "", &mut v.seed, 0..=9999);
    slider(ui, "Random tilt", "Degrees", &mut v.rotation, 0.0..=180.0);
    slider(ui, "Random size", "", &mut v.scale, 0.0..=0.95);
    slider(ui, "Random hue", "", &mut v.hue, 0.0..=1.0);
    drag_u(
        ui,
        "Random spin",
        "Up to this many turns per loop",
        &mut v.spin,
        0..=8,
    );
    ui.separator();
    ui.label(RichText::new("Waves travelling across the copies").weak());
    slider(ui, "Size wave", "", &mut v.ripple, 0.0..=1.0);
    slider(
        ui,
        "Light chase",
        "Glow running along the copies",
        &mut v.chase,
        0.0..=2.0,
    );
    drag_i(ui, "Waves / loop", "", &mut v.ripple_cycles, -32..=32);
    slider(
        ui,
        "Wavelengths",
        "How many waves across all copies",
        &mut v.ripple_spread,
        0.0..=8.0,
    );
}

fn particles_ui(ui: &mut Ui, p: &mut ParticleLayer) {
    section(ui, "Particles", true, |ui| {
        combo(
            ui,
            "Emitter",
            "How particles move",
            &mut p.emitter,
            &Emitter::ALL,
            |e| e.label(),
        );
        drag_u(ui, "Count", "", &mut p.count, 1..=100_000);
        drag_u(
            ui,
            "Lives / loop",
            "How often each particle is reborn per loop",
            &mut p.lifetimes,
            1..=32,
        );
        param(ui, "Speed", "", &mut p.speed, 0.0..=5.0);
        param(ui, "Area", "Emitter radius", &mut p.radius, 0.0..=40.0);
        drag_u(ui, "Seed", "", &mut p.seed, 0..=9999);
    });
    section(ui, "Look", true, |ui| {
        combo(ui, "Sprite", "", &mut p.sprite, &Sprite::ALL, |s| s.label());
        param(ui, "Size", "", &mut p.size, 0.0..=1.0);
        color(ui, "Colour (young)", "", &mut p.color_a);
        color(ui, "Colour (old)", "", &mut p.color_b);
        param(ui, "Brightness", "", &mut p.intensity, 0.0..=10.0);
        drag_u(
            ui,
            "Trail",
            "Ghost copies behind each particle",
            &mut p.trail,
            0..=16,
        );
        if p.trail > 0 {
            param(ui, "Trail length", "", &mut p.trail_spacing, 0.0005..=0.05);
        }
    });
}

fn backdrop_ui(ui: &mut Ui, b: &mut Backdrop, textures: &[UserTexture], lref: LayerRef) {
    section(ui, "Background", true, |ui| {
        combo(ui, "Style", "", &mut b.kind, &BackdropKind::ALL, |k| {
            k.label()
        });
        color(ui, "Colour A", "Darkest / base colour", &mut b.color_a);
        color(ui, "Colour B", "", &mut b.color_b);
        color(ui, "Colour C", "Highlight colour", &mut b.color_c);
        drag_i(
            ui,
            "Motion / loop",
            "Animation cycles per loop",
            &mut b.speed,
            -16..=16,
        );
        param(ui, "Brightness", "", &mut b.intensity, 0.0..=4.0);
        param(
            ui,
            "Detail",
            "Scale of the pattern",
            &mut b.detail,
            0.1..=4.0,
        );
        if b.kind == BackdropKind::Tunnel {
            texture_picker(
                ui,
                "Wall texture",
                &mut b.texture,
                textures,
                Some((lref, TexSlot::Backdrop)),
            );
        }
    });
    let labels = RaySettings::labels(b.kind);
    if labels.iter().any(|l| l.is_some()) {
        section(ui, "Raymarching", true, |ui| {
            ray_ui(ui, b.kind, &mut b.ray, b.texture.is_some())
        });
    }
}

/// Settings of the raymarched backgrounds; labels depend on the kind.
fn ray_ui(ui: &mut Ui, kind: BackdropKind, r: &mut RaySettings, has_texture: bool) {
    let variants = RaySettings::variants(kind);
    if !variants.is_empty() {
        row(ui, "Variant", "Shape or formula", |ui| {
            egui::ComboBox::from_id_salt("ray_variant")
                .selected_text(variants.get(r.variant as usize).copied().unwrap_or("?"))
                .show_ui(ui, |ui| {
                    for (i, v) in variants.iter().enumerate() {
                        ui.selectable_value(&mut r.variant, i as u32, *v);
                    }
                });
        });
    }
    if kind == BackdropKind::Tunnel && !has_texture {
        const PATTERNS: [&str; 4] = ["XOR", "Checker", "Rings", "Stripes"];
        row(
            ui,
            "Wall pattern",
            "Used when there is no wall texture",
            |ui| {
                egui::ComboBox::from_id_salt("ray_pattern")
                    .selected_text(PATTERNS.get(r.pattern as usize).copied().unwrap_or("?"))
                    .show_ui(ui, |ui| {
                        for (i, v) in PATTERNS.iter().enumerate() {
                            ui.selectable_value(&mut r.pattern, i as u32, *v);
                        }
                    });
            },
        );
    }
    let [size, twist, warp, bend, glow] = RaySettings::labels(kind);
    if let Some(l) = size {
        param(ui, l, "", &mut r.size, 0.2..=3.0);
    }
    if let Some(l) = twist {
        param(
            ui,
            l,
            "How much the space twists along the flight",
            &mut r.twist,
            -2.0..=2.0,
        );
    }
    if let Some(l) = warp {
        param(ui, l, "", &mut r.warp, 0.0..=3.0);
    }
    if let Some(l) = bend {
        param(ui, l, "", &mut r.bend, 0.0..=3.0);
    }
    if let Some(l) = glow {
        param(ui, l, "", &mut r.glow, 0.0..=4.0);
    }
    match kind {
        BackdropKind::Aurora => {}
        BackdropKind::Clouds => {
            param(
                ui,
                "Haze ×",
                "How much distant clouds melt into the horizon",
                &mut r.fog,
                0.0..=4.0,
            );
        }
        _ => {
            param(ui, "Fog ×", "Depth fog (0 = none)", &mut r.fog, 0.0..=4.0);
        }
    }
    if kind.is_flight() {
        drag_i(
            ui,
            "Roll / loop",
            "Whole turns of the view per loop",
            &mut r.spin,
            -8..=8,
        );
    }
    let (default_steps, what) = match kind {
        BackdropKind::Aurora => return,
        BackdropKind::Fractal => (13, "Fractal iterations"),
        BackdropKind::Sponge => (80, "Raymarch steps"),
        BackdropKind::Clouds => (28, "Raymarch steps"),
        _ => (64, "Raymarch steps"),
    };
    row(
        ui,
        "Quality",
        &format!("{what} (0 = default {default_steps}); lower is faster on phones"),
        |ui| {
            ui.add(egui::DragValue::new(&mut r.steps).range(0..=256).speed(0.3));
        },
    );
}

fn terrain_ui(ui: &mut Ui, t: &mut Terrain, textures: &[UserTexture], lref: LayerRef) {
    section(ui, "Landscape", true, |ui| {
        combo(ui, "Style", "", &mut t.style, &TerrainStyle::ALL, |s| {
            s.label()
        });
        combo(
            ui,
            "Shape",
            "Kind of landscape",
            &mut t.shape,
            &TerrainShape::ALL,
            |s| s.label(),
        );
        param(ui, "Height", "Mountain height", &mut t.height, 0.0..=20.0);
        drag_u(
            ui,
            "Hills",
            "Hills across the terrain",
            &mut t.hills,
            1..=32,
        );
        param(
            ui,
            "Roughness",
            "More small bumps",
            &mut t.roughness,
            0.0..=1.0,
        );
        param(
            ui,
            "Valley",
            "Flat road down the middle (0 = none)",
            &mut t.valley,
            0.0..=1.0,
        );
        drag_i(
            ui,
            "Scroll / loop",
            "Times the landscape scrolls past per loop (0 = still)",
            &mut t.scroll,
            -8..=8,
        );
        drag_u(ui, "Seed", "Different landscape", &mut t.seed, 0..=9999);
    });
    section(ui, "Look", true, |ui| {
        if t.style != TerrainStyle::Solid {
            color(ui, "Line colour", "", &mut t.line_color);
            param(ui, "Line glow", "", &mut t.glow, 0.0..=10.0);
        }
        if t.style != TerrainStyle::Wireframe {
            combo(
                ui,
                "Biome",
                "Colours the ground by height and steepness",
                &mut t.biome,
                &Biome::ALL,
                |b| b.label(),
            );
            if t.biome == Biome::Plain {
                color(ui, "Ground colour", "", &mut t.fill_color);
            }
        }
        texture_picker(
            ui,
            "Texture",
            &mut t.texture,
            textures,
            Some((lref, TexSlot::Terrain)),
        );
        if t.texture.is_some() {
            drag_u(
                ui,
                "Tiles",
                "Times the texture repeats across the terrain",
                &mut t.tiles,
                1..=64,
            );
            check(
                ui,
                "Texture on lines",
                "Colour the grid lines with the texture too",
                &mut t.texture_lines,
            );
            check(
                ui,
                "Chunky pixels",
                "Keep texture pixels sharp",
                &mut t.pixelated,
            );
        }
        slider(ui, "Size", "Width and depth", &mut t.size, 5.0..=200.0);
        drag_u(
            ui,
            "Grid cells",
            "Resolution (more = smoother, slower)",
            &mut t.cells,
            4..=256,
        );
    });
    section(ui, "Water & lava", true, |ui| liquid_ui(ui, &mut t.liquid));
}

fn liquid_ui(ui: &mut Ui, l: &mut Liquid) {
    let before = l.kind;
    combo(
        ui,
        "Liquid",
        "Fills the low ground with a flat surface",
        &mut l.kind,
        &LiquidKind::ALL,
        |k| k.label(),
    );
    if l.kind != before && l.color == before.default_color() {
        l.color = l.kind.default_color();
    }
    if l.kind == LiquidKind::None {
        return;
    }
    param(
        ui,
        "Level",
        "Surface height (fraction of the mountain height). Animate it for tides or rising lava",
        &mut l.level,
        0.0..=1.0,
    );
    color(ui, "Colour", "", &mut l.color);
    let (glow, waves) = match l.kind {
        LiquidKind::Water => ("Shine", "Ripples"),
        LiquidKind::Lava => ("Glow", "Crust"),
        LiquidKind::Toxic => ("Glow", "Bubbles"),
        _ => ("Shine", "Cracks"),
    };
    param(ui, glow, "", &mut l.glow, 0.0..=5.0);
    param(ui, waves, "", &mut l.waves, 0.0..=3.0);
    if l.kind != LiquidKind::Ice {
        drag_i(
            ui,
            "Current / loop",
            "Whole drifts of the surface along the terrain per loop",
            &mut l.flow,
            -8..=8,
        );
    }
}

fn weather_ui(ui: &mut Ui, w: &mut Weather) {
    section(ui, "Weather", true, |ui| {
        let before = w.kind;
        combo(ui, "Kind", "", &mut w.kind, &Precipitation::ALL, |k| {
            k.label()
        });
        if w.kind != before {
            // Start from settings that suit the new kind.
            let (c, falls, size) = w.kind.defaults();
            w.color = c;
            w.falls = falls;
            w.size = Param::new(size);
        }
        if w.kind == Precipitation::None {
            return;
        }
        drag_u(ui, "Amount", "Number of drops", &mut w.count, 0..=100_000);
        color(ui, "Colour", "", &mut w.color);
        param(ui, "Brightness", "", &mut w.intensity, 0.0..=5.0);
        param(ui, "Size", "", &mut w.size, 0.005..=2.0);
        if w.kind == Precipitation::Rain {
            slider(ui, "Streak length", "", &mut w.streak, 0.0..=4.0);
            param(
                ui,
                "Splashes",
                "Share of drops that splash on the ground",
                &mut w.splashes,
                0.0..=1.0,
            );
        }
        let what = match w.kind {
            Precipitation::Fireflies => ("Blinks / loop", "Blinks per loop (×4)"),
            Precipitation::Embers => ("Rises / loop", "Times each ember rises per loop"),
            Precipitation::Dust => ("Gusts / loop", "Times each puff crosses the area per loop"),
            _ => (
                "Falls / loop",
                "Times each drop falls per loop (higher = faster)",
            ),
        };
        drag_u(ui, what.0, what.1, &mut w.falls, 1..=64);
        if w.kind != Precipitation::Dust {
            param(
                ui,
                "Wind",
                "Sideways push in degrees (animate it for gusts)",
                &mut w.wind,
                -60.0..=60.0,
            );
        }
        slider(
            ui,
            "Wind direction",
            "Degrees around the vertical",
            &mut w.wind_dir,
            0.0..=360.0,
        );
        slider(
            ui,
            "Area",
            "Half width of the box around the camera",
            &mut w.area,
            2.0..=60.0,
        );
        slider(ui, "Height", "Height of the box", &mut w.height, 1.0..=60.0);
        drag_u(ui, "Seed", "", &mut w.seed, 0..=9999);
    });
    let l = &mut w.lightning;
    toggle_section(ui, "Lightning", &mut l.enabled, |ui| {
        drag_u(
            ui,
            "Chances / loop",
            "Moments per loop when a strike may happen",
            &mut l.per_loop,
            1..=64,
        );
        slider(
            ui,
            "Chance",
            "Chance of a strike at each moment",
            &mut l.chance,
            0.0..=1.0,
        );
        param(
            ui,
            "Flash",
            "How much the flash lights up the scene",
            &mut l.flash,
            0.0..=5.0,
        );
        color(ui, "Colour", "", &mut l.color);
        slider(
            ui,
            "Distance",
            "How far away the bolts strike",
            &mut l.distance,
            5.0..=150.0,
        );
        drag_u(ui, "Seed", "Different strikes", &mut l.seed, 0..=9999);
    });
}

fn lasers_ui(ui: &mut Ui, z: &mut Lasers) {
    section(ui, "Beams", true, |ui| {
        combo(ui, "Pattern", "", &mut z.pattern, &LaserPattern::ALL, |p| {
            p.label()
        });
        drag_u(ui, "Beams", "", &mut z.count, 1..=128);
        param(
            ui,
            "Spread",
            "Opening angle (degrees)",
            &mut z.spread,
            0.0..=180.0,
        );
        param(ui, "Length", "", &mut z.length, 1.0..=200.0);
        param(ui, "Width", "", &mut z.width, 0.005..=1.0);
        if z.pattern == LaserPattern::Scatter {
            drag_u(ui, "Seed", "Different directions", &mut z.seed, 0..=9999);
        }
    });
    section(ui, "Look & motion", true, |ui| {
        color(ui, "Colour (first)", "", &mut z.color_a);
        color(
            ui,
            "Colour (last)",
            "Beams blend between the two colours",
            &mut z.color_b,
        );
        param(ui, "Brightness", "", &mut z.intensity, 0.0..=20.0);
        param(
            ui,
            "Sweep",
            "How far the beams swing (degrees)",
            &mut z.sweep,
            0.0..=90.0,
        );
        drag_i(
            ui,
            "Sweeps / loop",
            "Swings (and cone turns) per loop",
            &mut z.sweep_cycles,
            -16..=16,
        );
        param(
            ui,
            "Beat strobe",
            "Flash on every beat (0 = steady)",
            &mut z.strobe,
            0.0..=1.0,
        );
    });
}

fn ribbon_ui(ui: &mut Ui, r: &mut Ribbon) {
    section(ui, "Curve", true, |ui| {
        combo(ui, "Curve", "", &mut r.curve, &RibbonCurve::ALL, |c| {
            c.label()
        });
        let names: &[&str] = match r.curve {
            RibbonCurve::Lissajous => &["X waves", "Y waves", "Z waves"],
            RibbonCurve::Knot => &["Loops", "Twists"],
            RibbonCurve::Infinity => &["Height waves"],
            RibbonCurve::Wave => &["Waves"],
            RibbonCurve::Rose => &["Petals", "Height waves"],
        };
        for (name, f) in names.iter().zip(r.freq.iter_mut()) {
            drag_u(ui, name, "", f, 1..=16);
        }
        slider(ui, "Thickness", "", &mut r.thickness, 0.002..=0.3);
    });
    section(ui, "Glow & pulses", true, |ui| {
        color(ui, "Colour", "", &mut r.color);
        param(
            ui,
            "Glow",
            "Glow of the whole tube",
            &mut r.glow,
            0.0..=10.0,
        );
        drag_u(
            ui,
            "Pulses",
            "Light pulses running along the tube",
            &mut r.pulses,
            0..=32,
        );
        drag_i(
            ui,
            "Laps / loop",
            "How fast the pulses run (negative = backwards)",
            &mut r.pulse_speed,
            -16..=16,
        );
        param(ui, "Pulse length", "", &mut r.pulse_length, 0.005..=0.5);
        param(ui, "Pulse glow", "", &mut r.pulse_glow, 0.0..=20.0);
    });
}

fn mirror_ui(ui: &mut Ui, f: &mut MirrorFloor, textures: &[UserTexture], lref: LayerRef) {
    section(ui, "Mirror floor", true, |ui| {
        ui.label(RichText::new("Only the first mirror floor in the list reflects.").weak());
        slider(ui, "Size", "", &mut f.size, 1.0..=200.0);
        color(ui, "Colour", "", &mut f.base_color);
        param(
            ui,
            "Reflection",
            "0 = matte, 1 = perfect mirror",
            &mut f.reflectivity,
            0.0..=1.0,
        );
        param(ui, "Blur", "Frosted reflection", &mut f.blur, 0.0..=1.0);
        color(ui, "Reflection tint", "", &mut f.tint);
        texture_picker(
            ui,
            "Texture",
            &mut f.texture,
            textures,
            Some((lref, TexSlot::Mirror)),
        );
        if f.texture.is_some() {
            slider(ui, "Tiling", "", &mut f.texture_scale, 0.05..=8.0);
        }
    });
    section(ui, "Neon grid", true, |ui| {
        param(ui, "Grid glow", "", &mut f.grid, 0.0..=10.0);
        color(ui, "Grid colour", "", &mut f.grid_color);
        param(
            ui,
            "Grid scale",
            "Lines per unit",
            &mut f.grid_scale,
            0.05..=4.0,
        );
        drag_i(
            ui,
            "Scroll / loop",
            "Cells scrolled per loop",
            &mut f.grid_scroll,
            -64..=64,
        );
    });
}

/// Templates offered in the "Add layer" menu, plus the user's own saved
/// layer templates.
pub fn add_layer_menu(ui: &mut Ui, templates: &[Layer]) -> Option<Layer> {
    let mut out = None;
    if !templates.is_empty() {
        ui.menu_button("⭐ My templates", |ui| {
            for t in templates {
                if ui.button(format!("{} {}", layer_icon(t), t.name)).clicked() {
                    out = Some(t.clone());
                }
            }
        });
        ui.separator();
    }
    ui.menu_button("🔷 Shape", |ui| {
        for p in Primitive::all_defaults() {
            if ui.button(p.label()).clicked() {
                out = Some(
                    Layer::new(
                        p.label(),
                        LayerKind::Mesh(MeshLayer {
                            source: MeshSource::Primitive(p),
                            material: Material {
                                emissive: Param::new(1.0),
                                emissive_mode: EmissiveMode::Edges,
                                flat_shading: true,
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                    )
                    .at([0.0, 1.0, 0.0]),
                );
            }
        }
    });
    ui.menu_button("✨ Particles", |ui| {
        for e in Emitter::ALL {
            if ui.button(e.label()).clicked() {
                out = Some(Layer::new(
                    e.label(),
                    LayerKind::Particles(ParticleLayer {
                        emitter: e,
                        ..Default::default()
                    }),
                ));
            }
        }
    });
    ui.menu_button("🌌 Background", |ui| {
        for k in BackdropKind::ALL {
            if ui.button(k.label()).clicked() {
                out = Some(Layer::new(
                    k.label(),
                    LayerKind::Backdrop(Backdrop {
                        kind: k,
                        ..Default::default()
                    }),
                ));
            }
        }
    });
    if ui.button("🗻 Terrain").clicked() {
        out = Some(Layer::new(
            "Terrain",
            LayerKind::Terrain(Terrain::default()),
        ));
    }
    ui.menu_button("🔦 Laser beams", |ui| {
        for p in LaserPattern::ALL {
            if ui.button(p.label()).clicked() {
                out = Some(Layer::new(
                    "Lasers",
                    LayerKind::Lasers(Lasers {
                        pattern: p,
                        ..Default::default()
                    }),
                ));
            }
        }
    });
    ui.menu_button("〰 Neon ribbon", |ui| {
        for c in RibbonCurve::ALL {
            if ui.button(c.label()).clicked() {
                out = Some(
                    Layer::new(
                        c.label(),
                        LayerKind::Ribbon(Ribbon {
                            curve: c,
                            ..Default::default()
                        }),
                    )
                    .at([0.0, 2.0, 0.0])
                    .scaled(3.0),
                );
            }
        }
    });
    ui.menu_button("☔ Weather", |ui| {
        for k in Precipitation::ALL {
            if ui.button(k.label()).clicked() {
                let (color, falls, size) = k.defaults();
                out = Some(Layer::new(
                    k.label(),
                    LayerKind::Weather(Weather {
                        kind: k,
                        color,
                        falls,
                        size: Param::new(size),
                        lightning: Lightning {
                            enabled: k == Precipitation::None,
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                ));
            }
        }
    });
    if ui.button("⊞ Mirror floor").clicked() {
        out = Some(Layer::new(
            "Mirror floor",
            LayerKind::Mirror(MirrorFloor::default()),
        ));
    }
    if ui.button("📦 3D model file…").clicked() {
        platform::pick(Purpose::AddModelLayer);
        ui.close();
    }
    out
}

/// A new mesh layer showing the model at asset `path`.
pub fn model_layer(path: &str) -> Layer {
    let file = ez_core::store::file_name(path);
    let name = file
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(file)
        .to_string();
    Layer::new(
        name,
        LayerKind::Mesh(MeshLayer {
            source: MeshSource::File {
                path: path.to_string(),
            },
            ..Default::default()
        }),
    )
    .at([0.0, 1.0, 0.0])
    .scaled(1.5)
}

pub fn layer_icon(l: &Layer) -> &'static str {
    match l.kind {
        LayerKind::Mesh(_) => "🔷",
        LayerKind::Particles(_) => "✨",
        LayerKind::Backdrop(_) => "🌌",
        LayerKind::Mirror(_) => "⊞",
        LayerKind::Terrain(_) => "🗻",
        LayerKind::Lasers(_) => "🔦",
        LayerKind::Ribbon(_) => "〰",
        LayerKind::Weather(_) => "☔",
    }
}
