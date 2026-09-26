//! Property editors for every part of a project.

use crate::platform::{self, LayerRef, Purpose, TexSlot};
use crate::widgets::*;
use egui::{RichText, Ui};
use ez_core::palette::PaletteId;
use ez_core::*;
use ez_render::texgen;

pub const MODEL_EXTENSIONS: &[&str] = &["gltf", "glb", "obj"];
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "tga"];
pub const FONT_EXTENSIONS: &[&str] = &["ttf", "otf"];

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
    if p.sequence.is_active() {
        // The timeline sets the loop; the scene has its own.
        ui.label(format!(
            "Loop = the timeline: {} beats. This scene loops every {} beats (Scenes & timeline).",
            p.timing.loop_beats, p.sequence.scene_beats
        ));
    } else {
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
    }
    ui.label(format!("= {:.2} seconds", p.timing.loop_seconds()));
}

pub fn camera_ui(ui: &mut Ui, c: &mut Camera) {
    ui.heading("Camera");
    let tip = if c.mode == CameraMode::Path {
        "The camera flies through the points below; the yellow line in the viewport is its flight."
    } else {
        "Tip: drag in the viewport to turn the camera, scroll to zoom."
    };
    ui.label(RichText::new(tip).weak());
    combo(
        ui,
        "Motion",
        "How the camera moves during the loop",
        &mut c.mode,
        &CameraMode::ALL,
        |m| m.label(),
    );
    if c.mode != CameraMode::Path {
        framing_ui(ui, c);
    }
    shake_punch_ui(ui, c);
    let view: Option<PathPoint> = ui.data(|d| d.get_temp(egui::Id::new(CAMERA_VIEW)));
    ui.add_space(6.0);
    if let Some(v) = view {
        if ui
            .button("📍 Add this view to the path")
            .on_hover_text("Store the camera as it is now as a point of the Path motion")
            .clicked()
        {
            c.path.points.push(v);
        }
    }
    if c.mode == CameraMode::Path || !c.path.points.is_empty() {
        section(ui, "Path", c.mode == CameraMode::Path, |ui| {
            path_ui(ui, c, view)
        });
    }
}

/// Orbit, pendulum and static cameras: where they look from.
fn framing_ui(ui: &mut Ui, c: &mut Camera) {
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
        CameraMode::Path => {}
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
}

fn shake_punch_ui(ui: &mut Ui, c: &mut Camera) {
    param(
        ui,
        "Beat shake",
        "Camera kick on every beat",
        &mut c.beat_shake,
        0.0..=1.0,
    );
    slider(
        ui,
        "Punch-in",
        "Zoom in on every hit of the music (needs a song)",
        &mut c.punch,
        0.0..=1.0,
    );
    if c.punch > 0.0 {
        combo(ui, "On", "", &mut c.punch_on, &HIT_KINDS, hit_label);
    }
}

/// egui temp-data key: the camera's shot right now ([`PathPoint`]).
pub const CAMERA_VIEW: &str = "ez2-camera-view";

const HIT_KINDS: [ez_core::audio::HitKind; 5] = [
    ez_core::audio::HitKind::Kick,
    ez_core::audio::HitKind::Snare,
    ez_core::audio::HitKind::Hats,
    ez_core::audio::HitKind::Any,
    ez_core::audio::HitKind::Note,
];

fn hit_label(k: ez_core::audio::HitKind) -> &'static str {
    match k {
        ez_core::audio::HitKind::Kick => "Kicks",
        ez_core::audio::HitKind::Snare => "Snares / claps",
        ez_core::audio::HitKind::Hats => "Hi-hats",
        ez_core::audio::HitKind::Any => "Any hit",
        ez_core::audio::HitKind::Note => "New notes",
    }
}

fn path_ui(ui: &mut Ui, c: &mut Camera, view: Option<PathPoint>) {
    ui.label(
        RichText::new(
            "The camera flies smoothly through these points. To place one: pick Static, frame the shot \
             by dragging in the viewport, then press “Add this view”.",
        )
        .weak(),
    );
    let mut laps = c.path.laps as i32;
    drag_i(
        ui,
        "Laps / loop",
        "Trips around the path per loop",
        &mut laps,
        1..=16,
    );
    c.path.laps = laps.max(1) as u32;
    slider(
        ui,
        "Linger",
        "0 = even speed, 1 = slow down at every point",
        &mut c.path.ease,
        0.0..=1.0,
    );
    let mut cut = c.path.cut_on.is_some();
    row(
        ui,
        "Cut on hits",
        "Jump to the next point on every hit of the music (loops with the song); without music the camera flies",
        |ui| {
            if ui.checkbox(&mut cut, "").changed() {
                c.path.cut_on = cut.then_some(ez_core::audio::HitKind::Kick);
            }
        },
    );
    if let Some(k) = &mut c.path.cut_on {
        combo(ui, "On", "", k, &HIT_KINDS, hit_label);
        slider(
            ui,
            "Drift",
            "How far the camera drifts towards the next point after each cut",
            &mut c.path.drift,
            0.0..=1.0,
        );
    }
    let mut action = None;
    let n = c.path.points.len();
    for (i, p) in c.path.points.iter_mut().enumerate() {
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong(format!("Point {}", i + 1));
            if ui
                .small_button("👁")
                .on_hover_text("Look from here (switches to Static so you can adjust it)")
                .clicked()
            {
                action = Some(("look", i));
            }
            if let Some(v) = view {
                if ui
                    .small_button("⟳")
                    .on_hover_text("Replace with the current view")
                    .clicked()
                {
                    *p = v;
                }
            }
            if i > 0 && ui.small_button("⬆").clicked() {
                action = Some(("up", i));
            }
            if i + 1 < n && ui.small_button("⬇").clicked() {
                action = Some(("down", i));
            }
            if n > 2 && ui.small_button("🗑").clicked() {
                action = Some(("delete", i));
            }
        });
        ui.push_id(("path_point", i), |ui| {
            vec3(ui, "Eye", "Camera position", &mut p.eye, 0.05);
            vec3(ui, "Look at", "", &mut p.target, 0.05);
            slider(ui, "Field of view", "", &mut p.fov, 10.0..=140.0);
            slider(ui, "Roll", "", &mut p.roll, -180.0..=180.0);
        });
    }
    match action {
        Some(("look", i)) => {
            let p = c.path.points[i];
            c.look_from(&p);
        }
        Some(("up", i)) => c.path.points.swap(i, i - 1),
        Some(("down", i)) => c.path.points.swap(i, i + 1),
        Some(("delete", i)) => {
            c.path.points.remove(i);
        }
        _ => {}
    }
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
        "Direction the light comes from (with the day cycle: where the sun is at noon)",
        &mut e.light_dir,
        0.02,
    );
    ui.add_space(6.0);
    let sh = &mut e.shadows;
    toggle_section(ui, "Sun shadows", &mut sh.enabled, |ui| {
        slider(
            ui,
            "Darkness",
            "How dark the shadows are",
            &mut sh.strength,
            0.0..=1.0,
        );
        slider(
            ui,
            "Softness",
            "Blur of the shadow edges",
            &mut sh.softness,
            0.0..=4.0,
        );
        slider(
            ui,
            "Distance",
            "How far around the camera's target shadows reach (smaller = sharper)",
            &mut sh.distance,
            5.0..=150.0,
        );
    });
    slider(
        ui,
        "Contact shadows",
        "Soft dark patches under shapes standing on a mirror floor (0 = off)",
        &mut e.shadows.contact,
        0.0..=1.0,
    );
    let hf = &mut e.height_fog;
    section(ui, "Mist (height fog)", false, |ui| {
        param(
            ui,
            "Density",
            "Mist thickness at its base height (0 = off)",
            &mut hf.density,
            0.0..=0.5,
        );
        slider(ui, "Base height", "", &mut hf.height, -10.0..=20.0);
        slider(
            ui,
            "Thickness",
            "How high the mist reaches before thinning out",
            &mut hf.falloff,
            0.1..=20.0,
        );
    });
    let ca = &mut e.caustics;
    section(ui, "Underwater caustics", false, |ui| {
        param(
            ui,
            "Amount",
            "Rippling light on every surface (0 = off)",
            &mut ca.amount,
            0.0..=4.0,
        );
        color(ui, "Colour", "", &mut ca.color);
        slider(ui, "Size", "Size of the pattern", &mut ca.scale, 0.1..=8.0);
        drag_i(
            ui,
            "Ripples / loop",
            "How fast the pattern moves",
            &mut ca.speed,
            -16..=16,
        );
        slider(
            ui,
            "Only below",
            "Caustics fade out above this height (under the water line)",
            &mut ca.below,
            -10.0..=100.0,
        );
    });
    param(
        ui,
        "Rainbow",
        "A rainbow opposite the sun (needs the sun fairly low)",
        &mut e.rainbow,
        0.0..=3.0,
    );
    let d = &mut e.day_cycle;
    toggle_section(ui, "Day & night cycle", &mut d.enabled, |ui| {
        drag_i(
            ui,
            "Days / loop",
            "Whole days the sun travels per loop",
            &mut d.cycles,
            -8..=8,
        );
        slider(
            ui,
            "Start time",
            "Time of day when the loop starts: 0 midnight, 0.25 sunrise, 0.5 noon, 0.75 sunset",
            &mut d.start,
            0.0..=1.0,
        );
        slider(
            ui,
            "Noon height",
            "How high the sun climbs (degrees)",
            &mut d.noon_height,
            5.0..=90.0,
        );
        color(ui, "Sunset colour", "", &mut d.sunset_color);
        color(
            ui,
            "Night colour",
            "Sky and fog at night",
            &mut d.night_color,
        );
        color(ui, "Moonlight", "", &mut d.moon_color);
    });
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
    toggle_section(ui, "Depth of field", &mut post.dof.enabled, |ui| {
        check(
            ui,
            "Auto focus",
            "Keep the point the camera looks at sharp",
            &mut post.dof.auto_focus,
        );
        if !post.dof.auto_focus {
            param(
                ui,
                "Focus",
                "Sharp distance from the camera",
                &mut post.dof.focus,
                0.5..=100.0,
            );
        }
        param(
            ui,
            "Blur",
            "How blurry things away from the focus get (try the 🎵 row: blur on the kick)",
            &mut post.dof.blur,
            0.0..=1.5,
        );
    });
    toggle_section(ui, "Heat haze", &mut post.haze.enabled, |ui| {
        combo(
            ui,
            "Where",
            "Which parts of the picture shimmer",
            &mut post.haze.region,
            &HazeRegion::ALL,
            |r| r.label(),
        );
        param(ui, "Amount", "", &mut post.haze.amount, 0.0..=5.0);
        slider(
            ui,
            "Size",
            "Size of the ripples",
            &mut post.haze.scale,
            0.2..=4.0,
        );
        drag_i(
            ui,
            "Rises / loop",
            "How fast the shimmer rises",
            &mut post.haze.speed,
            -32..=32,
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
/// egui temp-data key: names of the project's terrain layers.
pub const TERRAIN_NAMES: &str = "ez2-terrain-names";

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
        LayerKind::Falls(f) => falls_ui(ui, f),
        LayerKind::Text(t) => text_ui(ui, t, lref),
    }
    let is_mesh_like = matches!(
        layer.kind,
        LayerKind::Mesh(_)
            | LayerKind::Particles(_)
            | LayerKind::Lasers(_)
            | LayerKind::Ribbon(_)
            | LayerKind::Falls(_)
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
            MeshSource::Text { .. } => "3D text".to_string(),
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
                    if ui
                        .selectable_label(matches!(m.source, MeshSource::Text { .. }), "3D text")
                        .on_hover_text("Solid letters: a logo with every material, relief and copy option")
                        .clicked()
                        && !matches!(m.source, MeshSource::Text { .. })
                    {
                        m.source = MeshSource::Text {
                            text: "EZ2".into(),
                            font: TextFont::Sans,
                            font_file: None,
                            depth: 0.3,
                        };
                    }
                    if ui.button("3D model file (glTF / OBJ)…").clicked() {
                        platform::pick(Purpose::SetModel(lref));
                    }
                });
        });
        match &mut m.source {
            MeshSource::Primitive(p) => primitive_params_ui(ui, p),
            MeshSource::Text {
                text,
                font,
                font_file,
                depth,
            } => {
                ui.add(
                    egui::TextEdit::multiline(text)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                row(ui, "Font", "", |ui| {
                    let label = match font_file {
                        Some(p) => ez_core::store::file_name(p).to_string(),
                        None => font.label().to_string(),
                    };
                    egui::ComboBox::from_id_salt("text_font")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            for f in TextFont::ALL {
                                if ui
                                    .selectable_label(font_file.is_none() && *font == f, f.label())
                                    .on_hover_text(if f == TextFont::Pixel {
                                        "Chunky voxel letters"
                                    } else {
                                        ""
                                    })
                                    .clicked()
                                {
                                    *font = f;
                                    *font_file = None;
                                }
                            }
                            ui.separator();
                            if ui.button("Font file (TTF / OTF)…").clicked() {
                                platform::pick(Purpose::SetFont(lref));
                            }
                        });
                });
                slider(
                    ui,
                    "Depth",
                    "Thickness, in letter heights",
                    depth,
                    0.02..=2.0,
                );
            }
            MeshSource::File { .. } => {}
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
    section(ui, "Deform", m.deform.is_active(), |ui| {
        deform_ui(ui, &mut m.deform)
    });
    section(ui, "Copies (instancing)", true, |ui| {
        instancer_ui(ui, &mut m.instancer)
    });
    if !matches!(m.instancer, Instancer::Single) {
        section(ui, "Variation", false, |ui| {
            variation_ui(ui, &mut m.variation)
        });
        let mut on = m.ramp.enabled;
        let ramp = &mut m.ramp;
        toggle_section(ui, "Colours across copies", &mut on, |ui| ramp_ui(ui, ramp));
        m.ramp.enabled = on;
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
        Instancer::Curve {
            curve,
            freq,
            size,
            count,
            laps,
            align,
        } => {
            combo(ui, "Curve", "", curve, &RibbonCurve::ALL, |c| c.label());
            row(
                ui,
                "Frequencies",
                "Loops of the curve along x, y, z",
                |ui| {
                    for f in freq.iter_mut() {
                        ui.add(egui::DragValue::new(f).range(1..=16).speed(0.05));
                    }
                },
            );
            slider(ui, "Size", "", size, 0.1..=40.0);
            drag_u(ui, "Count", "", count, 1..=4096);
            drag_i(
                ui,
                "Laps / loop",
                "Whole trips around the curve per loop",
                laps,
                -16..=16,
            );
            check(ui, "Face along", "Turn each copy along the curve", align);
        }
        Instancer::Surface {
            shape,
            size,
            count,
            seed,
            align,
            lift,
        } => {
            let label = match &*shape {
                MeshSource::Primitive(p) => p.label().to_string(),
                MeshSource::File { path } => ez_core::store::file_name(path).to_string(),
                MeshSource::Text { .. } => "3D text".to_string(),
            };
            row(
                ui,
                "On shape",
                "The shape whose surface the copies cover. In Nodes mode, wire a shape layer into “On a surface” to follow it exactly.",
                |ui| {
                    egui::ComboBox::from_id_salt("surface_shape")
                        .selected_text(label)
                        .height(400.0)
                        .show_ui(ui, |ui| {
                            for p in Primitive::all_defaults() {
                                if ui.selectable_label(false, p.label()).clicked() {
                                    *shape = MeshSource::Primitive(p);
                                }
                            }
                        });
                },
            );
            slider(
                ui,
                "Shape size",
                "Match the scale of that shape's layer",
                size,
                0.1..=40.0,
            );
            drag_u(ui, "Count", "", count, 1..=5000);
            drag_u(ui, "Seed", "", seed, 0..=9999);
            check(ui, "Stand up", "Copies stand up along the surface", align);
            slider(
                ui,
                "Lift",
                "Push copies out from the surface",
                lift,
                -2.0..=4.0,
            );
        }
        Instancer::OnTerrain {
            terrain,
            count,
            seed,
            align,
            lift,
            ground,
        } => {
            let names: Vec<String> = ui
                .data(|d| d.get_temp(egui::Id::new(TERRAIN_NAMES)))
                .unwrap_or_default();
            row(
                ui,
                "Terrain",
                "The terrain layer the copies stand on. They ride along as it scrolls.",
                |ui| {
                    egui::ComboBox::from_id_salt("on_terrain")
                        .selected_text(if terrain.is_empty() {
                            "pick a terrain"
                        } else {
                            terrain.as_str()
                        })
                        .show_ui(ui, |ui| {
                            if names.is_empty() {
                                ui.label("Add a Terrain layer first");
                            }
                            for n in &names {
                                if ui.selectable_label(terrain == n, n).clicked() {
                                    *terrain = n.clone();
                                    *ground = None;
                                }
                            }
                        });
                },
            );
            if !terrain.is_empty() && !names.is_empty() && !names.contains(terrain) {
                ui.colored_label(egui::Color32::LIGHT_RED, "No terrain layer has this name");
            }
            drag_u(ui, "Count", "", count, 1..=5000);
            drag_u(ui, "Seed", "", seed, 0..=9999);
            check(ui, "Follow the slope", "Tilt copies with the ground", align);
            slider(ui, "Lift", "Raise copies off the ground", lift, -2.0..=10.0);
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
    ui.separator();
    slider(
        ui,
        "Equalizer",
        "Each copy grows and glows with one frequency band of the music, low notes first (needs music or live input)",
        &mut v.spectrum,
        -1.0..=4.0,
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
        check(
            ui,
            "Smoke",
            "Particles cover what is behind them (and can be dark) instead of glowing",
            &mut p.smoke,
        );
        if p.smoke {
            param(ui, "Opacity", "", &mut p.intensity, 0.0..=1.0);
        } else {
            param(ui, "Brightness", "", &mut p.intensity, 0.0..=10.0);
        }
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
        combo(
            ui,
            "Resolution",
            "Render the background at a lower resolution and upscale it: much faster for clouds and raymarched styles, slightly softer",
            &mut b.resolution,
            &BgResolution::ALL,
            |r| r.label(),
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
        let max = t.max_cells();
        drag_u(
            ui,
            "Grid cells",
            "Resolution (more = smoother, slower). With level of detail, the resolution near the camera",
            &mut t.cells,
            4..=max,
        );
        check(
            ui,
            "Level of detail",
            "Full resolution near the camera, gradually coarser further away: a quarter of the triangles, so you can raise the grid cells or the size for the same cost",
            &mut t.lod,
        );
        t.cells = t.cells.min(t.max_cells());
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

fn falls_ui(ui: &mut Ui, f: &mut Falls) {
    section(ui, "Waterfall", true, |ui| {
        ui.label(
            RichText::new("Pours from the layer's position downwards and out along its Z axis. Place it on a cliff edge.")
                .weak(),
        );
        let before = f.kind;
        combo(ui, "Kind", "", &mut f.kind, &FallKind::ALL, |k| k.label());
        if f.kind != before && f.color == before.default_color() {
            f.color = f.kind.default_color();
        }
        color(ui, "Colour", "", &mut f.color);
        param(
            ui,
            if f.kind == FallKind::Water {
                "Brightness"
            } else {
                "Glow"
            },
            "",
            &mut f.glow,
            0.0..=5.0,
        );
        slider(ui, "Width", "", &mut f.width, 0.2..=40.0);
        slider(ui, "Height", "", &mut f.height, 0.5..=60.0);
        slider(
            ui,
            "Arc",
            "How far it arcs out from the edge",
            &mut f.push,
            0.0..=10.0,
        );
        drag_u(
            ui,
            "Flow / loop",
            "Times the streaks run down per loop",
            &mut f.flow,
            1..=64,
        );
        param(
            ui,
            if f.kind == FallKind::Lava {
                "Smoke"
            } else {
                "Foam & mist"
            },
            "Puffs at the foot (0 = none)",
            &mut f.foam,
            0.0..=3.0,
        );
        drag_u(ui, "Seed", "", &mut f.seed, 0..=9999);
    });
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
        match w.kind {
            Precipitation::Rain => {
                param(
                    ui,
                    "Wet ground",
                    "Darker, glossy surfaces and puddles with ripples (0 = dry)",
                    &mut w.ground,
                    0.0..=1.0,
                );
            }
            Precipitation::Snow => {
                param(
                    ui,
                    "Snow cover",
                    "Snow on everything facing up. Animate it (e.g. a slow fade in) to let it build up",
                    &mut w.ground,
                    0.0..=1.0,
                );
            }
            _ => {}
        }
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
        combo(
            ui,
            "Style",
            "Thin laser beams or wide, hazy spotlight cones",
            &mut z.style,
            &BeamStyle::ALL,
            |s| s.label(),
        );
        combo(ui, "Pattern", "", &mut z.pattern, &LaserPattern::ALL, |p| {
            p.label()
        });
        if z.style == BeamStyle::Spotlight {
            param(
                ui,
                "Cone angle",
                "Opening of each cone (degrees)",
                &mut z.cone.0,
                1.0..=90.0,
            );
            check(
                ui,
                "Light pools",
                "Pools of light where the cones hit the ground (height 0)",
                &mut z.pools,
            );
        }
        drag_u(ui, "Beams", "", &mut z.count, 1..=128);
        param(
            ui,
            "Spread",
            "Opening angle (degrees)",
            &mut z.spread,
            0.0..=180.0,
        );
        param(ui, "Length", "", &mut z.length, 1.0..=200.0);
        if z.style == BeamStyle::Laser {
            param(ui, "Width", "", &mut z.width, 0.005..=1.0);
        }
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
                // Heavy styles start at half resolution.
                let heavy = matches!(
                    k,
                    BackdropKind::Clouds | BackdropKind::Fractal | BackdropKind::Sponge
                );
                out = Some(Layer::new(
                    k.label(),
                    LayerKind::Backdrop(Backdrop {
                        kind: k,
                        resolution: if heavy {
                            BgResolution::Half
                        } else {
                            BgResolution::Full
                        },
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
    ui.menu_button("🔤 Text", |ui| {
        for (style, text) in [
            (TextStyle::Static, "EZ2DEMOSCENE"),
            (
                TextStyle::Scroller,
                "HELLO WORLD ... THIS SCROLLER LOOPS FOREVER ...",
            ),
            (TextStyle::SineScroller, "GREETINGS FROM THE SINE WAVE ..."),
            (TextStyle::Typewriter, "LOADING DEMO..."),
            (
                TextStyle::Greetings,
                "GREETINGS TO\nALL THE CREWS\nKEEP IT LOOPING",
            ),
        ] {
            if ui.button(style.label()).clicked() {
                out = Some(
                    Layer::new(
                        style.label(),
                        LayerKind::Text(TextLayer {
                            text: text.into(),
                            style,
                            ..Default::default()
                        }),
                    )
                    .at([0.0, 2.0, 0.0]),
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
    ui.menu_button("🌊 Waterfall", |ui| {
        for k in FallKind::ALL {
            if ui.button(k.label()).clicked() {
                out = Some(
                    Layer::new(
                        format!("{} fall", k.label()),
                        LayerKind::Falls(Falls {
                            kind: k,
                            color: k.default_color(),
                            glow: Param::new(if k == FallKind::Water { 1.0 } else { 1.5 }),
                            ..Default::default()
                        }),
                    )
                    .at([0.0, 8.0, 0.0]),
                );
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
        LayerKind::Falls(_) => "🌊",
        LayerKind::Text(_) => "🔤",
    }
}

pub fn deform_ui(ui: &mut Ui, d: &mut Deform) {
    param(
        ui,
        "Twist",
        "Turns of twist from the bottom of the shape to its top",
        &mut d.twist,
        -2.0..=2.0,
    );
    param(
        ui,
        "Bend",
        "Bends the shape into an arc (degrees from bottom to top)",
        &mut d.bend,
        -180.0..=180.0,
    );
    param(
        ui,
        "Taper",
        "Top wider (+) or narrower (−) than the bottom",
        &mut d.taper,
        -1.0..=1.0,
    );
    param(
        ui,
        "Wobble",
        "Bumps that flow over the surface (add Subdivide in Relief for smooth bumps on simple shapes)",
        &mut d.noise,
        0.0..=0.5,
    );
    if d.noise.base != 0.0 || d.noise.is_animated() {
        slider(
            ui,
            "Bump size",
            "Higher = smaller bumps",
            &mut d.noise_scale,
            0.5..=8.0,
        );
        drag_i(
            ui,
            "Flow / loop",
            "Times the bumps flow around per loop",
            &mut d.noise_speed,
            -8..=8,
        );
    }
    param(
        ui,
        "Explode",
        "Faces fly apart (clearest with flat shading); try ~ with a beat fade",
        &mut d.explode,
        0.0..=2.0,
    );
}

pub fn ramp_ui(ui: &mut Ui, r: &mut ColorRamp) {
    combo(
        ui,
        "Blend",
        "Gradient blends smoothly; Steps gives each copy one colour in turn",
        &mut r.mode,
        &[RampMode::Gradient, RampMode::Steps],
        |m| match m {
            RampMode::Gradient => "Gradient",
            RampMode::Steps => "Steps",
        },
    );
    let mut remove = None;
    let n = r.colors.len();
    for (i, c) in r.colors.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            color(ui, &format!("Colour {}", i + 1), "", c);
            if n > 2 && ui.small_button("🗑").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        r.colors.remove(i);
    }
    if r.colors.len() < 4 && ui.small_button("+ colour").clicked() {
        r.colors.push(r.colors.last().copied().unwrap_or([1.0; 3]));
    }
    drag_i(
        ui,
        "Travel / loop",
        "Times the colours run along all the copies per loop (0 = still)",
        &mut r.cycles,
        -16..=16,
    );
    check(
        ui,
        "Colour the glow",
        "Glowing copies glow in their colour",
        &mut r.glow,
    );
}

fn text_ui(ui: &mut Ui, t: &mut TextLayer, lref: LayerRef) {
    section(ui, "Text", true, |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut t.text)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text("Type here. Greetings: one line each."),
        );
        combo(ui, "Style", "", &mut t.style, &TextStyle::ALL, |s| {
            s.label()
        });
        match t.style {
            TextStyle::Scroller | TextStyle::SineScroller => {
                slider(
                    ui,
                    "Window",
                    "Width the text scrolls across",
                    &mut t.width,
                    1.0..=60.0,
                );
                drag_i(
                    ui,
                    "Runs / loop",
                    "Times the text scrolls past per loop (negative = to the right)",
                    &mut t.speed,
                    -16..=16,
                );
                if t.style == TextStyle::SineScroller {
                    param(
                        ui,
                        "Wave height",
                        "In letter heights",
                        &mut t.wave,
                        0.0..=3.0,
                    );
                    slider(
                        ui,
                        "Wave length",
                        "Letters per wave",
                        &mut t.wavelength,
                        1.0..=40.0,
                    );
                    drag_i(
                        ui,
                        "Wave rolls / loop",
                        "Times the wave moves along per loop",
                        &mut t.wave_cycles,
                        -16..=16,
                    );
                }
            }
            TextStyle::Typewriter => {
                drag_u(
                    ui,
                    "Letters / beat",
                    "It starts again with the loop",
                    &mut t.letters_per_beat,
                    1..=32,
                );
            }
            TextStyle::Greetings => {
                drag_u(
                    ui,
                    "Beats / line",
                    "4 = one line per bar",
                    &mut t.beats_per_line,
                    1..=32,
                );
            }
            TextStyle::Static => {}
        }
    });
    section(ui, "Font & look", true, |ui| {
        row(ui, "Font", "", |ui| {
            let label = match &t.font_file {
                Some(p) => ez_core::store::file_name(p).to_string(),
                None => t.font.label().to_string(),
            };
            egui::ComboBox::from_id_salt("font")
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for f in TextFont::ALL {
                        if ui
                            .selectable_label(t.font_file.is_none() && t.font == f, f.label())
                            .clicked()
                        {
                            t.font = f;
                            t.font_file = None;
                        }
                    }
                    ui.separator();
                    if ui.button("Font file (TTF / OTF)…").clicked() {
                        platform::pick(Purpose::SetFont(lref));
                    }
                });
        });
        slider(ui, "Size", "Letter height", &mut t.size, 0.1..=10.0);
        slider(
            ui,
            "Spacing",
            "Extra space between letters",
            &mut t.spacing,
            -0.3..=1.0,
        );
        color(ui, "Top colour", "", &mut t.color_top);
        color(ui, "Bottom colour", "", &mut t.color_bottom);
        param(
            ui,
            "Glow",
            "Brightness; above 1 the letters glow",
            &mut t.glow,
            0.0..=8.0,
        );
        slider(ui, "Outline", "", &mut t.outline, 0.0..=1.0);
        if t.outline > 0.0 {
            color(ui, "Outline colour", "", &mut t.outline_color);
        }
        slider(ui, "Drop shadow", "", &mut t.shadow, 0.0..=1.0);
        slider(
            ui,
            "Chrome",
            "Shiny bevelled letters reflecting the sky",
            &mut t.chrome,
            0.0..=1.0,
        );
        check(
            ui,
            "Face the camera",
            "Always turn the text towards the camera (otherwise it faces +z and reads backwards from behind)",
            &mut t.face_camera,
        );
    });
}

/// Scenes and the timeline playing them. Returns true when another scene
/// became the one being edited.
pub fn sequence_ui(ui: &mut Ui, p: &mut Project, audio: Option<&AudioEnvelope>) -> bool {
    use ez_core::sequence::{Clip, Transition, TransitionKind};
    ui.heading("Scenes & timeline");
    if !p.sequence.is_active() {
        ui.label(
            "Chain several scenes into one loop: each scene keeps its own layers, camera, \
             light and effects, and the timeline plays them one after another with transitions.",
        );
        ui.add_space(6.0);
        if ui
            .button("🎬 Start a timeline")
            .on_hover_text("This scene becomes the first clip; then add scenes and clips")
            .clicked()
        {
            p.start_sequence();
        }
        return false;
    }
    let mut switched = false;
    let seq_beats = p.sequence.total_beats();
    ui.label(
        RichText::new(format!(
            "The loop is the whole timeline: {} beats ({:.1} s). Pick “This scene” or “🎬 Timeline” above the preview.",
            seq_beats,
            p.timing.loop_seconds()
        ))
        .weak(),
    );
    section(ui, "Scenes", true, |ui| {
        ui.horizontal(|ui| {
            ui.label("Editing");
            ui.text_edit_singleline(&mut p.sequence.scene_name);
        });
        let mut beats = p.sequence.scene_beats;
        if drag_u(
            ui,
            "Own loop",
            "This scene's own loop inside its clips (beats)",
            &mut beats,
            1..=256,
        ) {
            p.sequence.scene_beats = beats;
        }
        ui.separator();
        let mut edit = None;
        let mut remove = None;
        for s in &p.sequence.scenes {
            ui.horizontal(|ui| {
                ui.label(format!("{}  ·  {} beats", s.name, s.loop_beats));
                if ui
                    .small_button("✏ edit")
                    .on_hover_text("Work on this scene")
                    .clicked()
                {
                    edit = Some(s.id);
                }
                if ui
                    .small_button("🗑")
                    .on_hover_text("Delete the scene and its clips")
                    .clicked()
                {
                    remove = Some(s.id);
                }
            });
        }
        if let Some(id) = edit {
            switched = p.edit_scene(id);
        }
        if let Some(id) = remove {
            p.remove_scene(id);
        }
        ui.horizontal(|ui| {
            if ui.button("+ Empty scene").clicked() {
                p.add_scene(false);
            }
            if ui.button("+ Copy of this scene").clicked() {
                p.add_scene(true);
            }
        });
    });
    section(ui, "Timeline", true, |ui| {
        let ids = p.sequence.scene_ids();
        let names: Vec<String> = ids.iter().map(|id| p.sequence.scene_name(*id)).collect();
        let n = p.sequence.clips.len();
        let mut action = None;
        let mut start = 0;
        for (i, clip) in p.sequence.clips.iter_mut().enumerate() {
            ui.push_id(("clip", i), |ui| {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.strong(format!("{}.", i + 1));
                    let name = ids
                        .iter()
                        .position(|id| *id == clip.scene)
                        .map_or("?", |k| names[k].as_str());
                    egui::ComboBox::from_id_salt("scene")
                        .selected_text(name)
                        .show_ui(ui, |ui| {
                            for (id, name) in ids.iter().zip(&names) {
                                ui.selectable_value(&mut clip.scene, *id, name);
                            }
                        });
                    ui.add(
                        egui::DragValue::new(&mut clip.beats)
                            .range(1..=1024)
                            .suffix(" beats"),
                    );
                    if i > 0 && ui.small_button("⬆").clicked() {
                        action = Some(("up", i));
                    }
                    if i + 1 < n && ui.small_button("⬇").clicked() {
                        action = Some(("down", i));
                    }
                    if n > 1 && ui.small_button("🗑").clicked() {
                        action = Some(("delete", i));
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("from beat {start}, comes in with")).weak());
                    egui::ComboBox::from_id_salt("transition")
                        .selected_text(clip.transition.kind.label())
                        .show_ui(ui, |ui| {
                            for k in TransitionKind::ALL {
                                ui.selectable_value(&mut clip.transition.kind, k, k.label());
                            }
                        });
                    if clip.transition.kind != TransitionKind::Cut {
                        ui.add(
                            egui::DragValue::new(&mut clip.transition.beats)
                                .range(0.25..=64.0)
                                .speed(0.05)
                                .suffix(" beats"),
                        );
                    }
                    if clip.transition.kind == TransitionKind::Wipe {
                        ui.add(
                            egui::DragValue::new(&mut clip.transition.angle)
                                .range(-180.0..=180.0)
                                .suffix("°"),
                        );
                    }
                });
                if clip.transition.kind == TransitionKind::CutOnKick {
                    ui.label(
                        RichText::new("Cuts on the first kick of the song in that window.")
                            .weak()
                            .small(),
                    );
                }
            });
            start += clip.beats;
        }
        match action {
            Some(("up", i)) => p.sequence.clips.swap(i, i - 1),
            Some(("down", i)) => p.sequence.clips.swap(i, i + 1),
            Some(("delete", i)) => {
                p.sequence.clips.remove(i);
            }
            _ => {}
        }
        ui.add_space(4.0);
        if let Some(env) = audio {
            if ui
                .button("🎵 Clips from the song's sections")
                .on_hover_text(
                    "Split the song where its sound changes (drops, breakdowns…) and give each part a clip, \
                     taking turns through your scenes. The loop becomes the whole song from its start.",
                )
                .clicked()
            {
                let lens = ez_core::analysis::sections(
                    env,
                    p.timing.beat_seconds(),
                    p.music.offset,
                    4,
                );
                let ids = p.sequence.scene_ids();
                p.sequence.clips = lens
                    .iter()
                    .enumerate()
                    .map(|(i, bars)| Clip {
                        scene: ids[i % ids.len()],
                        beats: bars * 4,
                        transition: Transition::default(),
                    })
                    .collect();
            }
        }
        if ui.button("+ Add clip").clicked() {
            let scene = p
                .sequence
                .clips
                .last()
                .map_or(p.sequence.scene_id, |c| c.scene);
            p.sequence.clips.push(Clip {
                scene,
                beats: p.sequence.scene_beats(scene),
                transition: Transition::default(),
            });
        }
        ui.separator();
        if ui
            .button("Stop the timeline")
            .on_hover_text(
                "Back to one looping scene (the one being edited); other scenes are kept",
            )
            .clicked()
        {
            p.stop_sequence();
        }
    });
    p.sync_sequence_length();
    switched
}
