//! Property editors for every part of a project.

use crate::widgets::*;
use egui::{RichText, Ui};
use ez_core::palette::PaletteId;
use ez_core::*;
use ez_render::texgen;

pub const MODEL_EXTENSIONS: &[&str] = &["gltf", "glb", "obj"];
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "tga"];

/// Adds an image as a user texture and returns its name.
pub fn add_user_texture(textures: &mut Vec<UserTexture>, path: &std::path::Path) -> String {
    let path_s = path.to_string_lossy().to_string();
    if let Some(t) = textures.iter().find(|t| t.path == path_s) {
        return t.name.clone();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".into());
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
    textures: &mut Vec<UserTexture>,
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
                if ui.button("Import image…").clicked() {
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Images", IMAGE_EXTENSIONS)
                        .pick_file()
                    {
                        *tex = Some(add_user_texture(textures, &p));
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
    slider(
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
            slider(
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
    slider(
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
    slider(
        ui,
        "Ambient",
        "Strength of the ambient light",
        &mut e.ambient,
        0.0..=2.0,
    );
    color(ui, "Sun colour", "", &mut e.light_color);
    slider(ui, "Sun strength", "", &mut e.light_intensity, 0.0..=5.0);
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
        slider(
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
        slider(
            ui,
            "Threshold",
            "Brightness above which things glow",
            &mut post.bloom.threshold,
            0.0..=4.0,
        );
        slider(ui, "Spread", "", &mut post.bloom.radius, 0.0..=1.0);
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
        slider(
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
        slider(
            ui,
            "Dithering",
            "Ordered (Bayer) dithering strength",
            &mut post.palette.dither,
            0.0..=1.0,
        );
        palette_swatch(ui, post.palette.palette);
    });
    toggle_section(ui, "CRT monitor", &mut post.crt.enabled, |ui| {
        slider(ui, "Scanlines", "", &mut post.crt.scanlines, 0.0..=1.0);
        slider(ui, "Curvature", "", &mut post.crt.curvature, 0.0..=1.0);
        slider(ui, "VHS wobble", "", &mut post.crt.noise, 0.0..=1.0);
    });
    section(ui, "Colour grading", true, |ui| {
        let g = &mut post.grade;
        param(ui, "Exposure", "", &mut g.exposure, 0.0..=4.0);
        slider(ui, "Contrast", "", &mut g.contrast, 0.5..=2.0);
        slider(ui, "Saturation", "", &mut g.saturation, 0.0..=2.0);
        slider(
            ui,
            "Vignette",
            "Darken the corners",
            &mut g.vignette,
            0.0..=1.5,
        );
        slider(ui, "Film grain", "", &mut g.grain, 0.0..=0.2);
        slider(
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
        if let Some(files) = rfd::FileDialog::new()
            .add_filter("Images", IMAGE_EXTENSIONS)
            .pick_files()
        {
            for f in files {
                add_user_texture(textures, &f);
            }
        }
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

pub fn layer_ui(ui: &mut Ui, layer: &mut Layer, textures: &mut Vec<UserTexture>) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut layer.enabled, "");
        ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(180.0));
        ui.label(RichText::new(layer.type_label()).weak());
    });
    ui.add_space(4.0);
    match &mut layer.kind {
        LayerKind::Mesh(m) => mesh_ui(ui, m, textures),
        LayerKind::Particles(p) => particles_ui(ui, p),
        LayerKind::Backdrop(b) => backdrop_ui(ui, b, textures),
        LayerKind::Mirror(f) => mirror_ui(ui, f, textures),
    }
    let is_mesh_like = matches!(layer.kind, LayerKind::Mesh(_) | LayerKind::Particles(_));
    let is_backdrop = matches!(layer.kind, LayerKind::Backdrop(_));
    if !is_backdrop {
        section(ui, "Placement & motion", true, |ui| {
            let t = &mut layer.transform;
            if matches!(layer.kind, LayerKind::Mirror(_)) {
                slider(ui, "Floor height", "", &mut t.position[1], -10.0..=10.0);
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
        });
    }
    if is_mesh_like {
        section(ui, "Symmetry", true, |ui| {
            symmetry_ui(ui, &mut layer.symmetry)
        });
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
        _ => {}
    }
}

fn mesh_ui(ui: &mut Ui, m: &mut MeshLayer, textures: &mut Vec<UserTexture>) {
    section(ui, "Shape", true, |ui| {
        let label = match &m.source {
            MeshSource::Primitive(p) => p.label().to_string(),
            MeshSource::File { path } => std::path::Path::new(path)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "model".into()),
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
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("3D models", MODEL_EXTENSIONS)
                            .pick_file()
                        {
                            m.source = MeshSource::File {
                                path: path.to_string_lossy().to_string(),
                            };
                        }
                    }
                });
        });
        if let MeshSource::Primitive(p) = &mut m.source {
            primitive_params_ui(ui, p);
        }
    });
    section(ui, "Material", true, |ui| {
        material_ui(ui, &mut m.material, textures)
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

fn material_ui(ui: &mut Ui, mat: &mut Material, textures: &mut Vec<UserTexture>) {
    color(ui, "Colour", "", &mut mat.base_color);
    slider(
        ui,
        "Metallic",
        "0 = plastic, 1 = metal (reflects its colour)",
        &mut mat.metallic,
        0.0..=1.0,
    );
    slider(
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
    slider(
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
    texture_picker(ui, "Texture", &mut mat.texture, textures);
    if mat.texture.is_some() {
        slider(ui, "Tiling", "", &mut mat.texture_scale, 0.1..=16.0);
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
        slider(ui, "Speed", "", &mut p.speed, 0.0..=5.0);
        slider(ui, "Area", "Emitter radius", &mut p.radius, 0.0..=40.0);
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
            slider(ui, "Trail length", "", &mut p.trail_spacing, 0.0005..=0.05);
        }
    });
}

fn backdrop_ui(ui: &mut Ui, b: &mut Backdrop, textures: &mut Vec<UserTexture>) {
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
        slider(
            ui,
            "Detail",
            "Scale of the pattern",
            &mut b.detail,
            0.1..=4.0,
        );
        if b.kind == BackdropKind::Tunnel {
            texture_picker(ui, "Wall texture", &mut b.texture, textures);
        }
    });
}

fn mirror_ui(ui: &mut Ui, f: &mut MirrorFloor, textures: &mut Vec<UserTexture>) {
    section(ui, "Mirror floor", true, |ui| {
        ui.label(RichText::new("Only the first mirror floor in the list reflects.").weak());
        slider(ui, "Size", "", &mut f.size, 1.0..=200.0);
        color(ui, "Colour", "", &mut f.base_color);
        slider(
            ui,
            "Reflection",
            "0 = matte, 1 = perfect mirror",
            &mut f.reflectivity,
            0.0..=1.0,
        );
        slider(ui, "Blur", "Frosted reflection", &mut f.blur, 0.0..=1.0);
        color(ui, "Reflection tint", "", &mut f.tint);
        texture_picker(ui, "Texture", &mut f.texture, textures);
        if f.texture.is_some() {
            slider(ui, "Tiling", "", &mut f.texture_scale, 0.05..=8.0);
        }
    });
    section(ui, "Neon grid", true, |ui| {
        param(ui, "Grid glow", "", &mut f.grid, 0.0..=10.0);
        color(ui, "Grid colour", "", &mut f.grid_color);
        slider(
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
    if ui.button("🪞 Mirror floor").clicked() {
        out = Some(Layer::new(
            "Mirror floor",
            LayerKind::Mirror(MirrorFloor::default()),
        ));
    }
    if ui.button("📦 3D model file…").clicked() {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("3D models", MODEL_EXTENSIONS)
            .pick_file()
        {
            out = Some(model_layer(&path));
        }
    }
    out
}

pub fn model_layer(path: &std::path::Path) -> Layer {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Model".into());
    Layer::new(
        name,
        LayerKind::Mesh(MeshLayer {
            source: MeshSource::File {
                path: path.to_string_lossy().to_string(),
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
        LayerKind::Mirror(_) => "🪞",
    }
}
