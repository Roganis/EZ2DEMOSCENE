//! Property editors for every part of a project.

use crate::platform::{self, LayerRef, Purpose, TexSlot};
use crate::widgets::*;
use egui::{RichText, Ui};
use ez_core::palette::PaletteId;
use ez_core::*;
use ez_render::texgen;

pub const MODEL_EXTENSIONS: &[&str] = ez_render::import::MESH_EXTENSIONS;
/// What "3D model file…" opens: models and Gaussian splats (a PLY file can
/// be either; its contents decide).
pub const SCENE_FILE_EXTENSIONS: &[&str] = &[
    "gltf", "glb", "obj", "stl", "ply", "off", "3mf", "spz", "splat",
];
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "tga"];
/// Images and the videos that import as animations
/// (see [`crate::clip_import::VIDEO_EXTENSIONS`]).
pub const PICTURE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "bmp", "gif", "tga", "mp4", "webm", "mov", "mkv", "m4v", "avi", "ogv",
];
pub const FONT_EXTENSIONS: &[&str] = &["ttf", "otf"];

/// Adds an image as a user texture and returns its name.
/// `path` is an asset path (file or `mem://`), `file_name` its display name.
pub fn add_user_texture(textures: &mut Vec<UserTexture>, path: &str, file_name: &str) -> String {
    add_user_clip(textures, path, file_name, None)
}

/// Adds an image, or the frame sheet of an animation (`clip`), as a user
/// texture and returns its name.
pub fn add_user_clip(
    textures: &mut Vec<UserTexture>,
    path: &str,
    file_name: &str,
    clip: Option<FrameSheet>,
) -> String {
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
        mirror: false,
        clip,
    });
    name
}

/// Temp-data key: a texture chooser asks the app to open the texture
/// library for a layer's slot (an `Option<TexRequest>`).
pub const TEX_PICKER: &str = "ez2_tex_picker";

/// The layer and slot to fill, and the tab to open on (`None`: as left).
pub type TexRequest = (LayerRef, TexSlot, Option<ez_core::texlib::Kind>);

/// Ask the app to open the texture library for a layer's slot.
fn open_tex_library(ui: &Ui, lref: LayerRef, slot: TexSlot, kind: Option<ez_core::texlib::Kind>) {
    ui.data_mut(|d| {
        d.insert_temp::<Option<TexRequest>>(egui::Id::new(TEX_PICKER), Some((lref, slot, kind)))
    });
}

/// How a texture name reads in the choosers.
fn texture_label(name: &str) -> String {
    if ez_core::texlib::is_lib(name) {
        format!("📚 {}", ez_core::texlib::display_name(name))
    } else {
        name.to_string()
    }
}

/// Texture chooser: none, built-ins, user images, the texture library, or
/// import a new one.
fn texture_picker(
    ui: &mut Ui,
    label: &str,
    tex: &mut Option<String>,
    textures: &[UserTexture],
    target: Option<(LayerRef, TexSlot)>,
) {
    row(ui, label, "Image mapped onto the surface", |ui| {
        let text = tex
            .as_deref()
            .map(texture_label)
            .unwrap_or_else(|| "None".into());
        egui::ComboBox::from_id_salt(ui.id().with(label))
            .selected_text(text)
            .width(150.0)
            .height(400.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(tex, None, "None");
                if let Some((lref, slot)) = target {
                    if ui
                        .button("📚 Texture library…")
                        .on_hover_text("Seamless PBR materials and low-res retro textures")
                        .clicked()
                    {
                        open_tex_library(ui, lref, slot, None);
                    }
                }
                if let Some(name) = tex.clone().filter(|n| ez_core::texlib::is_lib(n)) {
                    ui.selectable_value(tex, Some(name.clone()), texture_label(&name));
                }
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

/// The project's colour scheme: one key colour everything harmonises with.
pub fn color_scheme_ui(ui: &mut Ui, s: &mut ColorScheme, ctx: &EvalCtx) {
    section(ui, "Colour scheme", true, |ui| {
        ui.checkbox(&mut s.enabled, "Bring every colour into one scheme");
        ui.label(
            RichText::new(
                "Each colour keeps its lightness while its hue moves to the scheme's hues. \
                 Your colours are kept: turn this off to see them again. Pictures keep theirs.",
            )
            .weak()
            .small(),
        );
        color(
            ui,
            "Key colour",
            "The colour everything harmonises with",
            &mut s.key,
        );
        combo(
            ui,
            "Harmony",
            "Which hues go with the key",
            &mut s.harmony,
            &Harmony::ALL,
            |h| h.label(),
        );
        // The scheme's hues right now.
        let offsets: Vec<f32> = s.harmony.offsets().iter().map(|d| d.to_radians()).collect();
        let h = ez_core::color::Harmoniser::new(
            s.key,
            s.key_turn.eval(ctx).to_radians(),
            &offsets,
            1.0,
            0.0,
        );
        row(ui, "Hues", "The scheme's colours", |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for c in h.swatches() {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(22.0, 16.0), egui::Sense::hover());
                let v = ez_core::color::to_hex(c);
                ui.painter().rect_filled(
                    rect,
                    3.0,
                    egui::Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8),
                );
            }
        });
        slider(
            ui,
            "Hue pull",
            "0 = colours unchanged, 1 = every hue on the scheme",
            &mut s.hue_pull,
            0.0..=1.0,
        );
        slider(
            ui,
            "Saturation match",
            "0 = colours keep their saturation, 1 = all as saturated as the key",
            &mut s.chroma_match,
            0.0..=1.0,
        );
        slider(
            ui,
            "Tint greys",
            "Greys and whites (like a new shape or model, which starts grey) take the key's hue. \
             0 = they stay neutral. Near-black stays dark either way.",
            &mut s.tint_greys,
            0.0..=1.0,
        );
        param(
            ui,
            "Turn",
            "Degrees added to the key's hue: animate it to turn the whole scheme",
            &mut s.key_turn,
            -180.0..=180.0,
        );
        if ui
            .small_button("↻ Turn once per loop")
            .on_hover_text("The whole scheme goes round the colour wheel once per loop")
            .clicked()
        {
            s.key_turn = Param::new(0.0).osc(Wave::Saw, 180.0, 1);
        }
        ui.checkbox(&mut s.environment, "Sky, fog, sun and rays too");
    });
}

/// Where reflections and ambient light come from.
fn env_light_ui(ui: &mut Ui, l: &mut EnvLight) {
    section(ui, "Environment light", false, |ui| {
        let current = match &l.source {
            EnvSource::Colours => "The colours above".to_string(),
            EnvSource::Studio(s) => s.label().to_string(),
            EnvSource::Hdri(p) => ez_core::store::file_name(p).to_string(),
            EnvSource::Sky => "From the sky".to_string(),
        };
        row(
            ui,
            "Light from",
            "Where reflections and soft light come from: the sky and ground \
             colours above, a built-in studio, a panorama photo (.hdr) of a \
             real place, or the scene's own background.",
            |ui| {
                egui::ComboBox::from_id_salt("env_light")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(l.source == EnvSource::Colours, "The colours above")
                            .clicked()
                        {
                            l.source = EnvSource::Colours;
                        }
                        for s in Studio::ALL {
                            let on = l.source == EnvSource::Studio(s);
                            if ui.selectable_label(on, s.label()).clicked() {
                                l.source = EnvSource::Studio(s);
                            }
                        }
                        if ui
                            .selectable_label(l.source == EnvSource::Sky, "From the sky")
                            .on_hover_text("The background layer, captured all around every frame")
                            .clicked()
                        {
                            l.source = EnvSource::Sky;
                        }
                        ui.separator();
                        if ui.button("Panorama photo (.hdr)…").clicked() {
                            platform::pick(Purpose::SetEnvMap);
                        }
                    });
            },
        );
        if !l.is_on() {
            return;
        }
        param(
            ui,
            "Strength",
            "How strongly the map lights the scene",
            &mut l.intensity,
            0.0..=4.0,
        );
        param(
            ui,
            "Turn",
            "Turn the map around the vertical, in degrees (a saw of 180° turns \
             it a whole turn per cycle)",
            &mut l.rotation,
            -180.0..=180.0,
        );
        match l.source {
            EnvSource::Sky => {
                check(
                    ui,
                    "Capture once",
                    "For a sky that doesn't move: capture it once instead of every frame",
                    &mut l.sky_static,
                );
            }
            _ => {
                check(
                    ui,
                    "Sun from the map",
                    "Point the sun (and its shadows) at the brightest spot of the \
                     map, and take that spot out of the map",
                    &mut l.sun_from_map,
                );
            }
        }
        ui.label(
            RichText::new(
                "Shapes and raymarched objects take the light and reflections. \
                 To see the map behind the scene, add a background of the kind \
                 Environment map.",
            )
            .weak()
            .small(),
        );
    });
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
    light_style_ui(
        ui,
        "Sun & ambient flicker (light style)",
        &mut e.light_style,
    );
    ui.add_space(6.0);
    env_light_ui(ui, &mut e.env_light);
    let rf = &mut e.reflections;
    toggle_section(ui, "Reflections", &mut rf.enabled, |ui| {
        param(
            ui,
            "Strength",
            "How much shiny shapes, water and wet ground reflect the scene around them \
             (instead of only the sky or environment map)",
            &mut rf.strength,
            0.0..=1.0,
        );
        slider(
            ui,
            "Reach",
            "How far a reflection reaches, in world units",
            &mut rf.max_distance,
            1.0..=40.0,
        );
        slider(
            ui,
            "Up to roughness",
            "Rougher surfaces keep the plain environment reflection",
            &mut rf.roughness_cutoff,
            0.05..=1.0,
        );
        ui.label(
            RichText::new(
                "Only what is on the screen can be reflected: near the edges, and behind shapes, \
                 reflections fade back to the environment. The mirror floor has its own exact reflection.",
            )
            .weak()
            .small(),
        );
    });
    let lf = &mut e.shafts;
    let switched = toggle_section(ui, "Light shafts", &mut lf.enabled, |ui| {
        param(
            ui,
            "Strength",
            "How brightly the sun lights the fog",
            &mut lf.strength,
            0.0..=4.0,
        );
        slider(
            ui,
            "Scattering",
            "0: the fog glows the same all round; towards 1: mostly when looking at the sun",
            &mut lf.scattering,
            0.0..=0.95,
        );
        slider(
            ui,
            "Reach",
            "How far into the fog the light is gathered, in world units",
            &mut lf.reach,
            5.0..=200.0,
        );
        drag_u(
            ui,
            "Quality",
            "Samples along each view ray (more = smoother, slower)",
            &mut lf.steps,
            8..=96,
        );
        ui.label(
            RichText::new(
                "Needs fog (or mist) and sun shadows: shapes and terrain cut dark bands through the light.",
            )
            .weak()
            .small(),
        );
    });
    // Shafts are made by the shadows: turning them on turns shadows on.
    if switched && e.shafts.enabled {
        e.shadows.enabled = true;
    }
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
    toggle_section(ui, "Feedback trails", &mut post.feedback.enabled, |ui| {
        param(
            ui,
            "Trail length",
            "How long the previous frames linger",
            &mut post.feedback.length,
            0.0..=1.0,
        );
        slider(
            ui,
            "Zoom / second",
            "Above 1 the trails fly outwards, below 1 they fall inwards",
            &mut post.feedback.zoom,
            0.5..=2.0,
        );
        slider(
            ui,
            "Turn / second",
            "Degrees the trails turn each second",
            &mut post.feedback.turn,
            -180.0..=180.0,
        );
        slider(
            ui,
            "Colour drift",
            "Hue turns per second: trails change colour as they fade",
            &mut post.feedback.hue,
            -2.0..=2.0,
        );
        ui.label(
            RichText::new("Exports render one loop first, so the trails are already there at the start and the loop closes.")
                .weak()
                .small(),
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
    toggle_section(ui, "Fisheye lens", &mut post.lens.enabled, |ui| {
        param(
            ui,
            "Amount",
            "Above 0: fisheye, the middle bulges out. Below 0: the middle shrinks away and the edges stretch",
            &mut post.lens.amount,
            -1.0..=1.0,
        );
    });
    toggle_section(ui, "Line wobble", &mut post.wobble.enabled, |ui| {
        combo(
            ui,
            "Mode",
            "How the lines of the picture move (retro RPG battle style)",
            &mut post.wobble.mode,
            &LineWarp::MOVING,
            |m| m.label(),
        );
        param(
            ui,
            "Amount",
            "How far the lines move",
            &mut post.wobble.amount,
            0.0..=0.2,
        );
        param(
            ui,
            "Waves",
            "Waves from top to bottom",
            &mut post.wobble.waves,
            0.0..=30.0,
        );
        drag_i(
            ui,
            "Waves / loop",
            "Times the waves roll by per loop",
            &mut post.wobble.speed,
            -64..=64,
        );
        if post.wobble.mode == LineWarp::Interlaced {
            drag_u(
                ui,
                "Lines",
                "Line pairs from top to bottom (0 = every pixel row)",
                &mut post.wobble.lines,
                0..=1080,
            );
        }
    });
    toggle_section(ui, "VHS tape", &mut post.vhs.enabled, |ui| {
        param(
            ui,
            "Amount",
            "Jittering lines, snow and faded colour",
            &mut post.vhs.amount,
            0.0..=2.0,
        );
        param(
            ui,
            "Colour bleed",
            "Colours smear sideways",
            &mut post.vhs.bleed,
            0.0..=2.0,
        );
        param(
            ui,
            "Tracking band",
            "A noisy, torn band rolling down the picture",
            &mut post.vhs.bands,
            0.0..=2.0,
        );
    });
    toggle_section(ui, "ASCII art", &mut post.ascii.enabled, |ui| {
        param(
            ui,
            "Rows",
            "Character rows from top to bottom",
            &mut post.ascii.rows,
            10.0..=200.0,
        );
        combo(
            ui,
            "Colour",
            "",
            &mut post.ascii.color,
            &AsciiColor::ALL,
            |c| c.label(),
        );
        param(
            ui,
            "Picture behind",
            "How much of the picture shows between the characters",
            &mut post.ascii.backdrop,
            0.0..=1.0,
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

/// A Quake light style: presets, the letters, and whole plays per loop
/// (shown as letters per second).
pub fn light_style_ui(ui: &mut Ui, label: &str, st: &mut LightStyle) {
    let secs = loop_seconds(ui);
    let id = ui.id().with(("light style", label));
    egui::CollapsingHeader::new(label)
        .id_salt(id)
        .default_open(st.is_on())
        .show(ui, |ui| {
            row(ui, "Style", "Ready-made flicker patterns", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for p in LightStylePreset::ALL {
                        if ui.small_button(p.label()).clicked() {
                            *st = p.style(secs);
                        }
                    }
                });
            });
            row(
                ui,
                "Letters",
                "Brightness step by step: a = dark, m = normal, z = twice as bright",
                |ui| {
                    let r =
                        ui.add(egui::TextEdit::singleline(&mut st.pattern).desired_width(170.0));
                    if r.changed() {
                        st.pattern.retain(|c| c.is_ascii_alphabetic());
                        st.pattern.make_ascii_lowercase();
                    }
                },
            );
            if st.is_on() {
                row(
                    ui,
                    "Plays / loop",
                    "Whole plays of the letters per loop, so the loop stays seamless",
                    |ui| {
                        ui.add(
                            egui::DragValue::new(&mut st.plays)
                                .range(1..=256)
                                .speed(0.2),
                        );
                        ui.label(
                            RichText::new(format!("≈ {:.1} letters/s", st.rate(secs)))
                                .weak()
                                .small(),
                        );
                        if ui
                            .small_button("Quake speed")
                            .on_hover_text(
                                "The nearest whole number of plays to 10 letters a second",
                            )
                            .clicked()
                        {
                            st.plays = st.plays_for_rate(ez_core::retro::LIGHT_STYLE_RATE, secs);
                        }
                    },
                );
            }
        });
}

/// Quake's wobbling liquid warp of a texture.
fn turbulence_ui(ui: &mut Ui, t: &mut Turbulence) {
    param(
        ui,
        "Turbulence",
        "Quake's wobbling water, lava and slime: the texture sways by a sine of itself (in tiles)",
        &mut t.amount,
        0.0..=0.5,
    );
    if t.is_on() {
        slider(
            ui,
            "Waves / tile",
            "How many wobbles across a texture tile",
            &mut t.waves,
            0.1..=4.0,
        );
        drag_i(
            ui,
            "Wobbles / loop",
            "Whole wobbles per loop",
            &mut t.cycles,
            -32..=32,
        );
    }
}

/// "Animate on steps": the layer's motion held at a lower frame rate,
/// snapped to a whole number of steps per loop (shown).
fn step_ui(ui: &mut Ui, fps: &mut f32) {
    let secs = loop_seconds(ui);
    row(
        ui,
        "Animate on",
        "Hold the layer's motion (spins, bobbing, copies moving, deform, glitch, sprite frames) \
         at a lower frame rate, like stop-motion or games animating \"on twos\"; the camera stays \
         smooth. Snapped to a whole number of steps per loop so it still loops.",
        |ui| {
            let label = |f: f32| {
                if f <= 0.0 {
                    "Smooth".to_string()
                } else {
                    format!("{f:.0} fps")
                }
            };
            egui::ComboBox::from_id_salt(ui.id().with("animate on"))
                .selected_text(label(*fps))
                .width(90.0)
                .show_ui(ui, |ui| {
                    for f in [0.0, 30.0, 24.0, 15.0, 12.0, 10.0, 8.0, 6.0, 4.0] {
                        ui.selectable_value(fps, f, label(f));
                    }
                });
            if *fps > 0.0 {
                ui.add(
                    egui::DragValue::new(fps)
                        .range(1.0..=60.0)
                        .speed(0.1)
                        .suffix(" fps"),
                );
            }
        },
    );
    if let Some(n) = ez_core::step_count(*fps, secs) {
        ui.label(
            RichText::new(format!(
                "{n} steps per loop ({:.2} fps on this loop)",
                n as f32 / secs
            ))
            .weak()
            .small(),
        );
    }
}

/// Two numbers (a size in pixels).
fn size_row(ui: &mut Ui, label: &str, tip: &str, v: &mut [u32; 2]) {
    row(ui, label, tip, |ui| {
        ui.add(egui::DragValue::new(&mut v[0]).range(8..=4096).speed(1.0));
        ui.label("×");
        ui.add(egui::DragValue::new(&mut v[1]).range(16..=4096).speed(1.0));
    });
}

/// The quirks of 5th-generation 3D for the whole scene.
pub fn retro_ui(ui: &mut Ui, r: &mut Retro3d, palette: &mut PaletteFx, out: (u32, u32)) {
    ui.heading("Retro 3D");
    ui.label(
        RichText::new(
            "Draw the 3D scene like a 90s console: chunky pixels, wobbly polygons and warped \
             textures. Pick a look, then fine-tune it.",
        )
        .weak()
        .small(),
    );
    row(ui, "Look", "Turn on a console's bundle of settings", |ui| {
        ui.horizontal_wrapped(|ui| {
            for st in RetroStyle::ALL {
                if ui.small_button(st.label()).clicked() {
                    r.apply_style(st);
                }
            }
        });
    });
    check(ui, "On", "Use the settings below", &mut r.enabled);
    ui.add_enabled_ui(r.enabled, |ui| {
        section(ui, "Whole screen (8- and 16-bit machines)", r.screen.enabled, |ui| {
            ui.label(
                RichText::new(
                    "The whole picture (text, logos and effects too) at an old machine's resolution, \
                     one colour per machine pixel, pixels as wide as they were. A machine button \
                     also turns on its palette (Post effects → Retro palette).",
                )
                .weak()
                .small(),
            );
            row(ui, "Machine", "Screen size, shape, border and palette of a machine", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for p in ScreenPreset::ALL {
                        if ui.small_button(p.label()).clicked() {
                            p.apply(r, palette);
                        }
                    }
                });
            });
            let sc = &mut r.screen;
            check(ui, "Whole screen", "Draw the whole picture at the size below", &mut sc.enabled);
            ui.add_enabled_ui(sc.enabled, |ui| {
                size_row(ui, "Pixels", "Pixels across and down", &mut sc.size);
                combo(
                    ui,
                    "Shape",
                    "Fill the output, sit on a 4:3 television (wide pixels where the machine had \
                     them), or keep pixels square (handhelds)",
                    &mut sc.frame,
                    &ScreenFrame::ALL,
                    |f| f.label(),
                );
                color(ui, "Border", "Colour around the picture", &mut sc.border);
                row(
                    ui,
                    "Border size",
                    "Border inside the frame on each side (share of its width, height): the Spectrum \
                     and C64 had wide borders all round; top and bottom only makes a letterbox, like \
                     the window Star Fox drew in",
                    |ui| {
                        ui.add(egui::Slider::new(&mut sc.inset[0], 0.0..=0.4).text("sides"));
                        ui.add(egui::Slider::new(&mut sc.inset[1], 0.0..=0.4).text("top & bottom"));
                    },
                );
                let st = &mut sc.stripes;
                combo(
                    ui,
                    "Loading stripes",
                    "A tape loading, as on the ZX Spectrum: red and cyan pilot bands, thin blue and \
                     yellow data bands, or the whole load over the loop with the picture arriving \
                     line by line in black and white, then its colours",
                    &mut st.mode,
                    &StripeMode::ALL,
                    |m| m.label(),
                );
                if st.mode != StripeMode::Off {
                    slider(ui, "Bands", "Pilot bands down the frame", &mut st.bands, 2.0..=40.0);
                    drag_u(ui, "Roll / loop", "Pairs of pilot bands rolling past per loop", &mut st.speed, 0..=128);
                }
            });
        });
        section(ui, "Resolution", true, |ui| {
            combo(
                ui,
                "Draw the scene at",
                "Draw the 3D scene small without smoothing and blow it up with square pixels: \
                 edges, shading and textures alias as on the console (unlike the Pixelate effect, \
                 which only blocks the finished picture). Sizes are for a 4:3 TV; wider outputs \
                 keep the height and the pixels' shape.",
                &mut r.resolution,
                &RetroRes::ALL,
                |x| x.label(),
            );
            if r.resolution == RetroRes::Custom {
                size_row(ui, "Size", "Pixels across and down on a 4:3 screen", &mut r.custom);
            }
            if let Some((w, h)) = r.internal_size(out) {
                ui.label(
                    RichText::new(format!("At this output: {w} × {h} pixels."))
                        .weak()
                        .small(),
                );
            }
            check(
                ui,
                "Sharp text & logos",
                "Text layers and logos stay at full resolution on top of the chunky scene",
                &mut r.sharp_overlays,
            );
        });
        section(ui, "Polygons & textures", true, |ui| {
            check(
                ui,
                "Snap vertices",
                "Corners of triangles jump to a coarse grid of screen pixels, so shapes wobble \
                 and crawl as they move",
                &mut r.snap,
            );
            ui.add_enabled_ui(r.snap, |ui| {
                size_row(
                    ui,
                    "Grid",
                    "Snapping grid (pixels across and down on a 4:3 screen)",
                    &mut r.snap_res,
                );
                param(
                    ui,
                    "Amount",
                    "How far corners move to the grid (fade the wobble in and out)",
                    &mut r.snap_amount,
                    0.0..=1.0,
                );
            });
            let mut own = r.filter.is_none();
            if check(
                ui,
                "Each material's filter",
                "Untick to give every shape and the terrain one texture filter",
                &mut own,
            ) {
                r.filter = if own { None } else { Some(TexFilter::Nearest) };
            }
            if let Some(f) = &mut r.filter {
                combo(
                    ui,
                    "Texture filter",
                    "Nearest: square pixels (PS1, Saturn, Quake). Bilinear without mipmaps: soft up \
                     close, sparkling far away. 3-point: the N64's soft, grainy blend.",
                    f,
                    &TexFilter::ALL,
                    |x| x.label(),
                );
            }
            param(
                ui,
                "Texture warp",
                "Textures stretched straight across each triangle (affine), not in perspective: \
                 they bend and swim on big polygons. Subdivide a shape to make the warp smaller, \
                 as PlayStation games did.",
                &mut r.affine,
                0.0..=1.0,
            );
            param(
                ui,
                "Near-plane culling",
                "Triangles with a corner closer to the camera than this (world units) vanish, \
                 as on the PlayStation: walls pop open when you get close. 0 = off.",
                &mut r.near_cull,
                0.0..=3.0,
            );
        });
        section(ui, "Colour", true, |ui| {
            check(
                ui,
                "15-bit colour",
                "Round every polygon's colours to 32 levels per channel as it is drawn, like the \
                 consoles' frame buffers (subtler than the Retro palette post effect)",
                &mut r.color_15bit,
            );
            ui.add_enabled_ui(r.color_15bit, |ui| {
                param(
                    ui,
                    "Dither",
                    "A fixed 4 × 4 pattern that hides the steps between the levels \
                     (PlayStation: on, Saturn: off)",
                    &mut r.dither,
                    0.0..=1.0,
                );
            });
        });
        section(ui, "Palette lighting (Quake)", false, |ui| {
            let c = &mut r.colormap;
            check(
                ui,
                "Colormap",
                "Light steps through a palette's own colours, like Quake's software renderer: each \
                 surface colour becomes its nearest palette colour, and shading picks darker or \
                 brighter palette colours for it (up to twice as bright)",
                &mut c.enabled,
            );
            ui.add_enabled_ui(c.enabled, |ui| {
                let mut opts = vec![ColormapPalette::Software256];
                // Up to 256 colours.
                opts.extend(
                    ez_core::palette::PaletteId::ALL
                        .into_iter()
                        .filter(|p| p.count() <= 256)
                        .map(ColormapPalette::Retro),
                );
                row(ui, "Palette", "The colours shading steps through", |ui| {
                    egui::ComboBox::from_id_salt("colormap palette")
                        .selected_text(c.palette.label())
                        .width(150.0)
                        .show_ui(ui, |ui| {
                            for o in opts {
                                ui.selectable_value(&mut c.palette, o, o.label());
                            }
                        });
                });
                drag_u(ui, "Light levels", "Steps from black to twice as bright (Quake: 32)", &mut c.levels, 2..=64);
                if c.palette == ColormapPalette::Software256 {
                    check(
                        ui,
                        "Fullbrights",
                        "The palette's last 32 colours (fire, lamps) glow whatever the light",
                        &mut c.fullbrights,
                    );
                }
            });
        });
        section(ui, "Nintendo 64", true, |ui| {
            let f = &mut r.fog;
            check(
                ui,
                "N64 fog",
                "Fog that starts close to the camera and thickens in a straight line to solid \
                 fog (in the scene's fog colour), instead of the scene's fog",
                &mut f.enabled,
            );
            ui.add_enabled_ui(f.enabled, |ui| {
                param(ui, "Starts at", "Distance where the fog begins", &mut f.near, 0.0..=40.0);
                param(ui, "Solid at", "Distance where nothing shows through", &mut f.far, 1.0..=200.0);
            });
            param(
                ui,
                "Video blur",
                "The N64's video output filter: smooths dither patterns away and softens the \
                 picture sideways, one console pixel wide",
                &mut r.vi_blur,
                0.0..=1.0,
            );
        });
    });
}

fn palette_swatch(ui: &mut Ui, p: PaletteId) {
    let cols = p.colors();
    if let Some(n) = p.levels() {
        ui.label(RichText::new(format!("{} colours ({n} levels per channel)", n * n * n)).weak());
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
    ui.label(RichText::new("Images you imported. 'Retro-ize' shrinks them and remaps them to an old-school palette. GIFs and videos become animations: image layers play them, and as textures they play by themselves.").weak());
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
        if let Some(c) = &t.clip {
            let loop_s: f32 = ui
                .data(|d| d.get_temp(egui::Id::new(LOOP_SECONDS)))
                .unwrap_or(4.0);
            clip_note(ui, c, loop_s);
        }
        ui.checkbox(&mut t.mirror, "Mirror tiling").on_hover_text(
            "Repeat it flipped: every other copy is a mirror image, so the edges always meet and there are no seams",
        );
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
/// Temp data: the selected layer's `Option<SimStatus>` (its bake).
pub const SIM_STATUS: &str = "ez2-sim-status";
/// egui temp-data key: names of the shape and sprite layers.
pub const COPY_LAYER_NAMES: &str = "ez2-copy-layer-names";
/// Names of the logo layers (what a logo can be attached to).
pub const LOGO_NAMES: &str = "ez2-logo-names";
/// Names of the 3D layers a logo can follow.
pub const SHAPE_NAMES: &str = "ez2-shape-names";
/// Whether the project's colour scheme is on (layers offer to keep their
/// own colours).
pub const SCHEME_ON: &str = "ez2-scheme-on";

pub fn layer_ui(ui: &mut Ui, layer: &mut Layer, textures: &[UserTexture], lref: LayerRef) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut layer.enabled, "");
        ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(180.0));
        ui.label(RichText::new(layer.type_label()).weak());
    });
    if ui
        .data(|d| d.get_temp::<bool>(egui::Id::new(SCHEME_ON)))
        .unwrap_or(false)
    {
        ui.checkbox(&mut layer.keep_colors, "Keep own colours")
            .on_hover_text("Leave this layer out of the colour scheme (fire stays orange)");
    }
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
        LayerKind::Sprite(sp) => sprite_ui(ui, sp, textures, lref),
        LayerKind::Arcs(a) => arcs_ui(ui, a),
        LayerKind::Logo(g) => logo_ui(ui, g, &layer.name, textures, lref),
        LayerKind::Mode7(f) => mode7_ui(ui, f, textures),
        LayerKind::Splat(s) => splat_ui(ui, s, lref),
    }
    let is_mesh_like = matches!(
        layer.kind,
        LayerKind::Mesh(_)
            | LayerKind::Particles(_)
            | LayerKind::Lasers(_)
            | LayerKind::Ribbon(_)
            | LayerKind::Falls(_)
    );
    // Logos are placed on the screen by their own settings.
    let placed = !matches!(layer.kind, LayerKind::Backdrop(_) | LayerKind::Logo(_));
    if placed {
        section(ui, "Placement & motion", true, |ui| {
            step_ui(ui, &mut layer.step_fps);
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
            param(
                ui,
                "Tilt",
                "Rock the layer side to side, in degrees (animate it: a bowl \
                 of liquid with the same tilt sloshes)",
                &mut t.tilt,
                -90.0..=90.0,
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
        Primitive::Bowl { thickness } => {
            slider(ui, "Thickness", "", thickness, 0.005..=0.5);
        }
        _ => {}
    }
}

/// Temp-data key: a shape button asks the app to open the shape picker
/// for this layer (an `Option<(LayerRef, ShapeSlot)>`).
pub const SHAPE_PICKER: &str = "ez2_shape_picker";

/// Which shape of a layer the picker chooses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeSlot {
    /// The layer's shape.
    Main,
    /// The shape it morphs into.
    MorphTarget,
}

/// Name of a shape source, for buttons and labels.
pub fn shape_label(source: &MeshSource) -> String {
    match source {
        MeshSource::Primitive(p) => p.label().to_string(),
        MeshSource::File { path } => ez_core::store::file_name(path).to_string(),
        MeshSource::Library { id } => ez_core::models::library()
            .and_then(|lib| lib.entry(id).map(|e| e.name.clone()))
            .unwrap_or_else(|| id.rsplit('/').next().unwrap_or(id).to_string()),
        MeshSource::Text { .. } => "3D text".to_string(),
        MeshSource::Sdf { form, .. } => form.label().to_string(),
        MeshSource::Cloth { cloth, .. } => cloth.kind.label().to_string(),
    }
}

fn morph_ui(ui: &mut Ui, m: &mut MeshLayer, lref: LayerRef) {
    check(
        ui,
        "Morph",
        "Melt the shape into another, like liquid: holes open and close, parts bud off and merge. \
         Off, the shape stays the usual sharp mesh.",
        &mut m.morph.enabled,
    );
    if !m.morph.enabled {
        return;
    }
    row(ui, "Into", "", |ui| {
        if ui
            .button(format!("{}…", shape_label(&m.morph.target)))
            .on_hover_text("The shape it melts into: a built-in shape or a model")
            .clicked()
        {
            ui.data_mut(|d| {
                d.insert_temp(
                    egui::Id::new(SHAPE_PICKER),
                    Some((lref, ShapeSlot::MorphTarget)),
                )
            });
        }
    });
    param(
        ui,
        "Amount",
        "0 = this layer's shape, 1 = the other. Click ~ to animate it (loop, beat, music).",
        &mut m.morph.amount,
        0.0..=1.0,
    );
    ui.label(
        RichText::new(
            "Raymarched while on: smooth and rounded, costs per pixel it covers. \
             Textures, relief, deform and glitch don't apply. The first use of a shape takes a moment.",
        )
        .weak()
        .small(),
    );
}

fn mesh_ui(ui: &mut Ui, m: &mut MeshLayer, textures: &[UserTexture], lref: LayerRef) {
    section(ui, "Shape", true, |ui| {
        row(ui, "Shape", "", |ui| {
            if ui
                .button(format!("{}…", shape_label(&m.source)))
                .on_hover_text("Choose a built-in shape or a model from the library")
                .clicked()
            {
                ui.data_mut(|d| {
                    d.insert_temp(egui::Id::new(SHAPE_PICKER), Some((lref, ShapeSlot::Main)))
                });
            }
        });
        match &mut m.source {
            MeshSource::Primitive(p) => primitive_params_ui(ui, p),
            MeshSource::Cloth { cloth, .. } => cloth_ui(ui, cloth),
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
            MeshSource::Sdf { form, cycles } => {
                match form {
                    SdfShape::Metaballs { balls, blend } => {
                        drag_u(ui, "Balls", "", balls, 1..=8);
                        slider(
                            ui,
                            "Melt",
                            "How far the balls melt into each other",
                            blend,
                            0.0..=0.8,
                        );
                    }
                    SdfShape::Gyroid { scale, thickness } => {
                        slider(ui, "Lattice", "How fine the lattice is", scale, 2.0..=16.0);
                        slider(ui, "Thickness", "", thickness, 0.02..=0.5);
                    }
                    SdfShape::Bulb { power } => {
                        slider(
                            ui,
                            "Power",
                            "The fractal's symmetry: 8 is the classic bulb",
                            power,
                            2.0..=12.0,
                        );
                    }
                    SdfShape::SoftBox { blend, round } => {
                        slider(
                            ui,
                            "Melt",
                            "How far the ball melts into the box",
                            blend,
                            0.0..=0.6,
                        );
                        slider(ui, "Rounding", "", round, 0.0..=0.45);
                    }
                }
                drag_i(
                    ui,
                    "Motion",
                    "Whole cycles of the shape's own motion per loop (0 = still)",
                    cycles,
                    -8..=8,
                );
                ui.label(
                    RichText::new("Raymarched: costs per pixel it covers. Textures, relief, deform and glitch don't apply.")
                        .weak()
                        .small(),
                );
            }
            MeshSource::File { .. } | MeshSource::Library { .. } => {}
        }
    });
    section(ui, "Morph", m.morph.enabled, |ui| morph_ui(ui, m, lref));
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
         Works best on smooth shapes (sphere, torus, capsule, rounded cube): faceted built-in ones open at their edges. \
         3D models get the detail they need by themselves and stay closed; tick Faceted in Material for crisp bumps.",
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
    row(
        ui,
        "Presets",
        "Ready-made physical materials (keep your textures and glow)",
        |ui| {
            ui.horizontal_wrapped(|ui| {
                for p in MaterialPreset::ALL {
                    if ui.small_button(p.label()).clicked() {
                        p.apply(mat);
                    }
                }
                if ui
                    .small_button("📚 Library…")
                    .on_hover_text(
                        "Photo-real PBR materials (brick, wood, metal, rock…): colour, \
                         normal map and roughness in one go",
                    )
                    .clicked()
                {
                    open_tex_library(
                        ui,
                        lref,
                        TexSlot::Material,
                        Some(ez_core::texlib::Kind::Pbr),
                    );
                }
            });
        },
    );
    combo(
        ui,
        "Shading",
        "Physical: realistic highlights and reflections, clearcoat, sheen and glass. \
         Classic: the original look.",
        &mut mat.pbr.shading,
        &Shading::ALL,
        |s| s.label(),
    );
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
    if mat.pbr.shading == Shading::Physical {
        physical_ui(ui, &mut mat.pbr);
    }
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
    light_style_ui(ui, "Glow flicker (light style)", &mut mat.glow_style);
    param(
        ui,
        "Hue shift",
        "Rotate the colours (in turns)",
        &mut mat.hue_shift,
        -1.0..=1.0,
    );
    param(
        ui,
        "See-through (mesh)",
        "The Saturn's transparency: pixels are left out in a fixed pattern instead of \
         blending. 0.5 is its checkerboard; animate it to fade a shape in or out.",
        &mut mat.mesh,
        0.0..=1.0,
    );
    translucency_ui(ui, &mut mat.translucency);
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
        turbulence_ui(ui, &mut mat.turbulence);
        let mut filter = mat.tex_filter();
        if combo(
            ui,
            "Filter",
            "How the texture is smoothed between its pixels: Smooth (modern), Nearest (square \
             pixels, PlayStation), Bilinear without mipmaps (shimmers in the distance), \
             3-point (the Nintendo 64's softer, grainier blend)",
            &mut filter,
            &TexFilter::ALL,
            |f| f.label(),
        ) {
            mat.pixelated = filter == TexFilter::Nearest;
            mat.filter = filter;
        }
    }
    texture_picker(
        ui,
        "Occlusion/rough/metal",
        &mut mat.pbr.orm_map,
        textures,
        Some((lref, TexSlot::Orm)),
    );
    texture_picker(
        ui,
        "Glow map",
        &mut mat.pbr.emissive_map,
        textures,
        Some((lref, TexSlot::Emissive)),
    );
    if mat.pbr.orm_map.is_some() || mat.pbr.emissive_map.is_some() {
        ui.label(
            RichText::new(
                "Maps use the texture's Tiling and Scroll. The ORM map (red: occlusion, green: roughness, \
                 blue: metal, as in glTF) multiplies Roughness and Metallic; the glow map multiplies the glow colour.",
            )
            .weak()
            .small(),
        );
    }
}

/// Light shining through, and seeing through.
fn translucency_ui(ui: &mut Ui, t: &mut Translucency) {
    param(
        ui,
        "Translucency",
        "Light shines through from behind, like leaves, paper, wax, skin or a lampshade: \
         put the sun behind the shape. The shape stays solid.",
        &mut t.amount,
        0.0..=1.0,
    );
    if t.amount.base > 0.0 || t.amount.is_animated() {
        color(
            ui,
            "Light inside",
            "The colour the light takes on passing through",
            &mut t.color,
        );
    }
    param(
        ui,
        "Transparency",
        "See what is behind, smoothly blended (0 = solid, 1 = invisible). \
         Animate it to fade the shape in or out.",
        &mut t.transparency,
        0.0..=1.0,
    );
}

/// Clearcoat, sheen and glass (physical shading only).
fn physical_ui(ui: &mut Ui, p: &mut Pbr) {
    param(
        ui,
        "Clearcoat",
        "A clear lacquer layer on top (car paint, varnish)",
        &mut p.clearcoat,
        0.0..=1.0,
    );
    if p.clearcoat.base > 0.0 || p.clearcoat.is_animated() {
        param(
            ui,
            "Coat roughness",
            "",
            &mut p.clearcoat_roughness,
            0.0..=1.0,
        );
    }
    param(
        ui,
        "Sheen",
        "Soft light at grazing angles, like velvet",
        &mut p.sheen,
        0.0..=1.0,
    );
    if p.sheen.base > 0.0 || p.sheen.is_animated() {
        color(ui, "Sheen colour", "", &mut p.sheen_color);
    }
    param(
        ui,
        "Glass",
        "Light passing through (transmission): 1 = clear glass tinted by the colour",
        &mut p.transmission,
        0.0..=1.0,
    );
    if p.transmission.base > 0.0 || p.transmission.is_animated() {
        slider(
            ui,
            "Refraction",
            "Index of refraction: 1 = none, 1.33 water, 1.5 glass, 2.4 diamond",
            &mut p.ior,
            1.0..=2.5,
        );
        ui.label(
            RichText::new("Glass shows the environment behind it, not other shapes.")
                .weak()
                .small(),
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
        Instancer::Swarm {
            form,
            count,
            radius,
            spread,
            speed,
            seed,
        } => {
            combo(ui, "Form", "", form, &SwarmForm::ALL, |f| f.label());
            row(
                ui,
                "Count",
                "Up to 250,000. Placed by the graphics card on desktop and WebGPU; in WebGL2 browsers the processor does it, so keep it smaller there.",
                |ui| {
                    ui.add(
                        egui::DragValue::new(count)
                            .range(1..=SWARM_MAX)
                            .speed(100.0),
                    )
                    .changed()
                },
            );
            slider(ui, "Radius", "", radius, 0.0..=60.0);
            slider(ui, "Spread", "", spread, 0.0..=20.0);
            drag_i(ui, "Turns / loop", "", speed, -8..=8);
            drag_u(ui, "Seed", "", seed, 0..=9999);
            ui.label(
                RichText::new("Use a simple shape (cube, tetrahedron, shard) for huge counts.")
                    .weak()
                    .small(),
            );
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
            let label = shape_label(shape);
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
        Instancer::Flock { flock, .. } => flock_ui(ui, flock),
        Instancer::Physics { physics, .. } => physics_ui(ui, physics),
        Instancer::Fluid { fluid, .. } => fluid_ui(ui, fluid),
    }
}

fn flock_ui(ui: &mut Ui, f: &mut ez_core::sim::Flock) {
    use ez_core::sim::{FlockPath, FLOCK_MAX};
    drag_u(
        ui,
        "Count",
        "Boids in the flock",
        &mut f.count,
        1..=FLOCK_MAX,
    );
    drag_u(ui, "Seed", "", &mut f.seed, 0..=9999);
    param(
        ui,
        "Speed",
        "Cruising speed, in units per second",
        &mut f.speed,
        0.0..=20.0,
    );
    slider(
        ui,
        "Area",
        "How far from the target the boids fly",
        &mut f.radius,
        0.5..=40.0,
    );
    slider(
        ui,
        "Formation",
        "How firmly each boid keeps its own place in the flock. Free flocking \
         never repeats, so closing the loop has to catch boids up; holding a \
         formation makes the flight nearly repeat by itself. 0 = free.",
        &mut f.formation,
        0.0..=2.0,
    );
    ui.separator();
    ui.label(RichText::new("Flocking").weak());
    slider(
        ui,
        "Spacing",
        "Distance boids keep from each other",
        &mut f.spacing,
        0.05..=5.0,
    );
    slider(
        ui,
        "Sight",
        "How far a boid sees its neighbours",
        &mut f.sight,
        0.1..=10.0,
    );
    slider(ui, "Keep apart", "", &mut f.separation, 0.0..=3.0);
    slider(
        ui,
        "Fly together",
        "Match the neighbours' heading",
        &mut f.alignment,
        0.0..=3.0,
    );
    slider(
        ui,
        "Stay close",
        "Move to the neighbours' middle",
        &mut f.cohesion,
        0.0..=3.0,
    );
    slider(
        ui,
        "Agility",
        "How hard a boid can steer",
        &mut f.agility,
        1.0..=60.0,
    );
    slider(
        ui,
        "Bank",
        "Lean into turns (0 = never)",
        &mut f.bank,
        0.0..=1.5,
    );
    ui.separator();
    ui.label(RichText::new("Target").weak());
    for (axis, p) in ["Target x", "Target y", "Target z"]
        .into_iter()
        .zip(&mut f.target)
    {
        param(
            ui,
            axis,
            "Where the flock gathers, from the layer's origin (animate it or link \
             it to the music to lead the flock)",
            p,
            -20.0..=20.0,
        );
    }
    let mut on_path = f.path.is_some();
    if check(
        ui,
        "Along a curve",
        "The target also travels a closed curve",
        &mut on_path,
    ) {
        f.path = on_path.then(FlockPath::default);
    }
    if let Some(path) = &mut f.path {
        combo(ui, "Curve", "", &mut path.curve, &RibbonCurve::ALL, |c| {
            c.label()
        });
        row(
            ui,
            "Frequencies",
            "Loops of the curve along x, y, z",
            |ui| {
                for v in path.freq.iter_mut() {
                    ui.add(egui::DragValue::new(v).range(1..=16).speed(0.05));
                }
            },
        );
        slider(ui, "Curve size", "", &mut path.size, 0.1..=40.0);
        drag_i(
            ui,
            "Laps / loop",
            "Whole trips around the curve per loop",
            &mut path.laps,
            -8..=8,
        );
    }
    param(
        ui,
        "Scatter",
        "Push the boids away from the target (link it to kicks to scatter on hits)",
        &mut f.scatter,
        -1.0..=2.0,
    );
    ui.separator();
    sim_loop_ui(
        ui,
        &mut f.looping,
        "Blend the tail: the flock steers back to where it started. \
         Cross-fade: two copies of the flight half a loop apart, each \
         shrinking away before it jumps back. Ping-pong: forward, then backward.",
    );
}

/// How a simulation closes its loop, and how its bake is doing.
fn sim_loop_ui(ui: &mut Ui, l: &mut ez_core::sim::SimLoop, close_tip: &str) {
    use ez_core::sim::LoopClose;
    ui.separator();
    ui.label(RichText::new("Closing the loop").weak());
    combo(
        ui,
        "Close by",
        close_tip,
        &mut l.close,
        &LoopClose::ALL,
        |c| c.label(),
    );
    if l.close == LoopClose::BlendTail {
        slider(
            ui,
            "Tail",
            "Part of the loop spent steering back to the start",
            &mut l.blend,
            0.05..=0.5,
        );
        check(
            ui,
            "Steer back",
            "Steer to the start during the tail (off: only blend)",
            &mut l.guide,
        );
    }
    if l.close != LoopClose::PingPong {
        drag_u(
            ui,
            "Warm-up loops",
            "Loops simulated before the one you see (fewer once it settles)",
            &mut l.warmup,
            0..=6,
        );
    }
    let status: Option<ez_render::SimStatus> =
        ui.data(|d| d.get_temp(egui::Id::new(SIM_STATUS))).flatten();
    if let Some(st) = status {
        let text = if !st.ready {
            "Simulating…".to_string()
        } else if l.close == LoopClose::PingPong {
            format!("Baked · {:.1} MB", st.bytes as f32 / 1e6)
        } else {
            format!(
                "Baked · seam {:.2} (largest {:.2}) · {:.1} MB",
                st.seam.rms,
                st.seam.max,
                st.bytes as f32 / 1e6
            )
        };
        ui.label(RichText::new(text).weak()).on_hover_text(
            "The seam is how far the simulation still is from where it started \
             when the final blend takes over (average and largest, in world \
             units): smaller means a smoother loop.",
        );
    }
}

fn physics_ui(ui: &mut Ui, p: &mut ez_core::sim::Physics) {
    use ez_core::sim::{Collider, LoopClose, Scenario, Vanish, PHYSICS_MAX};
    let before = p.scenario;
    combo(
        ui,
        "Scene",
        "Rain: copies drop one after another, pile up and vanish. Stack and \
         blast: copies start stacked (a wall, a tower) and a blast knocks \
         them down.",
        &mut p.scenario,
        &Scenario::ALL,
        |s| s.label(),
    );
    if p.scenario != before {
        // Each scene closes its loop its own way.
        let fresh = match p.scenario {
            Scenario::Rain => ez_core::sim::Physics::default(),
            Scenario::Stack => ez_core::sim::Physics::stack(),
        };
        p.looping = fresh.looping;
    }
    combo(
        ui,
        "Collide as",
        "The shape each copy bumps as",
        &mut p.collider,
        &Collider::ALL,
        |c| c.label(),
    );
    slider(
        ui,
        "Collider size",
        "Half the collider's size for a copy of size 1: 0.5 fits the \
         built-in cube, 1 the sphere",
        &mut p.extent,
        0.05..=2.0,
    );
    match p.scenario {
        Scenario::Rain => {
            drag_u(
                ui,
                "Count",
                "Copies, each dropping once per loop",
                &mut p.count,
                1..=PHYSICS_MAX,
            );
            slider(
                ui,
                "Life",
                "How long each stays, in beats. Keep it under half the tail, \
                 so closing the loop hardly moves anything.",
                &mut p.life,
                0.5..=32.0,
            );
            slider(ui, "Area", "Radius they drop into", &mut p.area, 0.0..=20.0);
            slider(
                ui,
                "Height",
                "Height they drop from",
                &mut p.height,
                0.0..=30.0,
            );
            slider(
                ui,
                "Spin",
                "How fast they tumble as they drop",
                &mut p.spin,
                0.0..=15.0,
            );
            combo(ui, "Leave by", "", &mut p.vanish, &Vanish::ALL, |v| {
                v.label()
            });
        }
        Scenario::Stack => {
            row(
                ui,
                "Stack",
                "Copies along x, up and z (a wall: 8, 5, 1)",
                |ui| {
                    for (v, a) in p.counts.iter_mut().zip(["x ", "y ", "z "]) {
                        ui.add(egui::DragValue::new(v).range(1..=40).speed(0.1).prefix(a));
                    }
                },
            );
            slider(
                ui,
                "Gap",
                "Space between the stacked copies",
                &mut p.gap,
                0.0..=1.0,
            );
            slider(
                ui,
                "Blast at beat",
                "When the blast goes off (ping-pong rebuilds the stack in \
                 the second half of the loop)",
                &mut p.blast_beat,
                0.0..=64.0,
            );
            slider(ui, "Blast", "Its strength", &mut p.blast, 0.0..=40.0);
            vec3(
                ui,
                "Blast from",
                "From the layer's origin",
                &mut p.blast_at,
                0.05,
            );
        }
    }
    ui.separator();
    ui.label(RichText::new("World").weak());
    slider(
        ui,
        "Gravity",
        "0 = floating in space",
        &mut p.gravity,
        0.0..=30.0,
    );
    slider(
        ui,
        "Floor",
        "Floor height, from the layer's origin",
        &mut p.floor,
        -20.0..=20.0,
    );
    slider(ui, "Bounce", "", &mut p.bounce, 0.0..=1.0);
    slider(ui, "Grip", "Friction", &mut p.friction, 0.0..=2.0);
    drag_u(ui, "Seed", "", &mut p.seed, 0..=9999);
    ui.label(
        RichText::new(
            "Random tilt and size (Variation) turn and size the copies but \
             not how they collide: keep them at 0.",
        )
        .weak()
        .small(),
    );
    let tip = if p.scenario == Scenario::Stack && p.looping.close == LoopClose::PingPong {
        "Ping-pong: the stack falls, then builds itself back up."
    } else {
        "Blend the tail: the bodies steer back to where they started \
         while still bumping into each other and the floor."
    };
    sim_loop_ui(ui, &mut p.looping, tip);
}

fn fluid_ui(ui: &mut Ui, f: &mut ez_core::sim::Fluid) {
    use ez_core::sim::{Container, Source, FLUID_MAX};
    drag_u(
        ui,
        "Droplets",
        "More makes a smoother liquid but takes longer to simulate \
         (a few thousand bake in seconds)",
        &mut f.count,
        1..=FLUID_MAX,
    );
    combo(
        ui,
        "Container",
        "What holds it. Tilt the layer (Transform → Tilt) to rock it.",
        &mut f.container,
        &Container::ALL,
        |c| c.label(),
    );
    slider(
        ui,
        "Container size",
        "Half its width (a bowl's radius)",
        &mut f.size,
        0.2..=10.0,
    );
    slider(
        ui,
        "Droplet size",
        "Their spacing at rest: smaller needs more droplets for the same amount",
        &mut f.spacing,
        0.03..=1.0,
    );
    combo(ui, "Source", "", &mut f.source, &Source::ALL, |s| s.label());
    if f.source == Source::Pour {
        slider(
            ui,
            "Life",
            "How long each droplet stays, in beats",
            &mut f.life,
            0.5..=64.0,
        );
        slider(
            ui,
            "Spout height",
            "Where it pours from, above the layer's origin",
            &mut f.spout,
            0.0..=20.0,
        );
    }
    ui.separator();
    slider(ui, "Gravity", "", &mut f.gravity, 0.0..=30.0);
    slider(
        ui,
        "Viscosity",
        "0 water, 0.3 honey",
        &mut f.viscosity,
        0.0..=1.0,
    );
    slider(
        ui,
        "Cohesion",
        "How much droplets hold together (surface tension)",
        &mut f.cohesion,
        0.0..=0.5,
    );
    param(
        ui,
        "Stir",
        "A swirl around the middle (link it to the music to stir on the beat)",
        &mut f.stir,
        -20.0..=20.0,
    );
    drag_u(ui, "Seed", "", &mut f.seed, 0..=9999);
    check(
        ui,
        "Liquid surface",
        "Draw the droplets as one smooth surface in the layer's material \
         (molten metal, water with Glass) instead of as copies of its shape",
        &mut f.surface,
    );
    let tip = if f.surface {
        "One liquid layer per scene is drawn as a surface (the last); \
         the floor's reflection shows its droplets."
    } else {
        "Each copy is a droplet: small spheres, or glowing sprites. \
         The layer's Size sets how big each one is drawn."
    };
    ui.label(RichText::new(tip).weak().small());
    sim_loop_ui(
        ui,
        &mut f.looping,
        "Cross-fade halves: two copies of the liquid half a loop apart \
         fade into each other, so the loop closes exactly.",
    );
}

fn cloth_ui(ui: &mut Ui, c: &mut ez_core::sim::Cloth) {
    use ez_core::sim::{ClothKind, CLOTH_MAX_DETAIL};
    combo(
        ui,
        "Kind",
        "What it is and where it hangs from",
        &mut c.kind,
        &ClothKind::ALL,
        |k| k.label(),
    );
    row(
        ui,
        "Size",
        "Width and height (a drape: width and depth)",
        |ui| {
            for (v, p) in c.size.iter_mut().zip(["w ", "h "]) {
                ui.add(
                    egui::DragValue::new(v)
                        .range(0.1..=40.0)
                        .speed(0.02)
                        .prefix(p),
                );
            }
        },
    );
    drag_u(
        ui,
        "Detail",
        "Particles across: finer folds, slower to simulate",
        &mut c.detail,
        4..=CLOTH_MAX_DETAIL,
    );
    slider(
        ui,
        "Stiffness",
        "Resistance to folding: 0 silk, 1 canvas",
        &mut c.stiffness,
        0.0..=1.0,
    );
    slider(
        ui,
        "Damping",
        "How fast motion dies down",
        &mut c.damping,
        0.0..=1.0,
    );
    ui.separator();
    ui.label(RichText::new("Wind").weak());
    param(
        ui,
        "Wind",
        "Wind speed (link it to the music for gusts on the beat)",
        &mut c.wind,
        0.0..=30.0,
    );
    param(
        ui,
        "Direction",
        "Where the wind blows to, in degrees around the vertical (0 = along x). \
         An oscillator of 180° turns it round.",
        &mut c.wind_direction,
        -180.0..=180.0,
    );
    slider(
        ui,
        "Gusts",
        "How much the wind varies over the cloth and the loop",
        &mut c.gusts,
        0.0..=2.0,
    );
    ui.separator();
    ui.label(RichText::new("Bumping into").weak());
    check(ui, "Floor", "Stop at a floor", &mut c.floor_on);
    if c.floor_on {
        slider(
            ui,
            "Floor height",
            "From the layer's origin",
            &mut c.floor,
            -20.0..=5.0,
        );
    }
    if c.kind == ClothKind::Drape {
        row(
            ui,
            "Ball",
            "Where the ball is (from the layer's origin) and its radius",
            |ui| {
                for (v, p) in c.ball.iter_mut().zip(["x ", "y ", "z ", "r "]) {
                    ui.add(egui::DragValue::new(v).speed(0.02).prefix(p));
                }
            },
        );
    }
    drag_u(
        ui,
        "Seed",
        "Another pattern of gusts",
        &mut c.seed,
        0..=9999,
    );
    sim_loop_ui(
        ui,
        &mut c.looping,
        "Blend the tail: the cloth steers back to where it started. \
         Cross-fade: the loop mixed with itself half a loop apart. \
         Ping-pong: forward, then backward.",
    );
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
        light_style_ui(ui, "Flicker (light style)", &mut p.glow_style);
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
        let battle = b.kind == BackdropKind::Battle;
        if !battle {
            drag_i(
                ui,
                "Motion / loop",
                "Animation cycles per loop",
                &mut b.speed,
                -16..=16,
            );
        }
        param(ui, "Brightness", "", &mut b.intensity, 0.0..=4.0);
        if b.kind == BackdropKind::Environment {
            param(
                ui,
                "Sharpness",
                "1 = the map as it is, 0 = fully blurred",
                &mut b.detail,
                0.0..=1.0,
            );
            ui.label(
                RichText::new(
                    "Shows the environment light's map (Light & fog → Environment light), \
                     turned with it.",
                )
                .weak()
                .small(),
            );
        } else if !battle {
            param(
                ui,
                "Detail",
                "Scale of the pattern",
                &mut b.detail,
                0.1..=4.0,
            );
        }
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
    if b.kind == BackdropKind::LayeredSky {
        let k = &mut b.sky;
        section(ui, "Two-layer sky", true, |ui| {
            ui.label(
                RichText::new(
                    "Quake's sky: a far layer and a near layer scrolling over it; the near layer's \
                     see-through colour shows the far one. None = the built-in cloud layers.",
                )
                .weak()
                .small(),
            );
            texture_picker(
                ui,
                "Far layer",
                &mut b.texture,
                textures,
                Some((lref, TexSlot::Backdrop)),
            );
            texture_picker(ui, "Near layer", &mut k.near_texture, textures, None);
            color(
                ui,
                "See-through colour",
                "Colour of the near layer that shows the far one",
                &mut k.cutout,
            );
            slider(
                ui,
                "Tolerance",
                "How close to that colour counts",
                &mut k.tolerance,
                0.0..=1.0,
            );
            let scroll = |ui: &mut Ui, label: &str, v: &mut [i32; 2]| {
                row(ui, label, "Tiles scrolled per loop (x, z)", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut v[0])
                            .range(-32..=32)
                            .speed(0.1)
                            .prefix("x "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut v[1])
                            .range(-32..=32)
                            .speed(0.1)
                            .prefix("z "),
                    );
                });
            };
            scroll(ui, "Far scroll / loop", &mut k.far_scroll);
            scroll(ui, "Near scroll / loop", &mut k.near_scroll);
            slider(
                ui,
                "Tiles",
                "Tiles across the dome",
                &mut k.tiles,
                0.5..=12.0,
            );
            slider(
                ui,
                "Flatten",
                "How flat the dome is (Quake: 3): flatter squeezes more tiles towards the horizon",
                &mut k.flatten,
                1.0..=8.0,
            );
        });
    }
    if b.kind == BackdropKind::Battle {
        let bt = &mut b.battle;
        section(ui, "Battle background", true, |ui| {
            ui.label(
                RichText::new("The colours A, B and C cycle through the patterns.")
                    .weak()
                    .small(),
            );
            drag_u(
                ui,
                "Lines",
                "Pixel rows from top to bottom: fat retro pixels (0 = full resolution)",
                &mut bt.lines,
                0..=1080,
            );
            drag_u(
                ui,
                "Colours",
                "Colours in the cycling palette: few give hard retro bands (0 = smooth)",
                &mut bt.steps,
                0..=64,
            );
        });
        section(ui, "Back layer", true, |ui| {
            ui.push_id("battle back", |ui| battle_layer_ui(ui, &mut bt.back, false));
        });
        section(ui, "Front layer", false, |ui| {
            ui.push_id("battle front", |ui| {
                check(
                    ui,
                    "Show",
                    "A second pattern over the first",
                    &mut bt.front.enabled,
                );
                if bt.front.enabled {
                    combo(
                        ui,
                        "Blend",
                        "How it goes over the back layer",
                        &mut bt.blend,
                        &BattleBlend::ALL,
                        |m| m.label(),
                    );
                    battle_layer_ui(ui, &mut bt.front, true);
                }
            });
        });
        if [&bt.back, &bt.front]
            .iter()
            .any(|l| l.enabled && l.pattern == BattlePattern::Picture)
        {
            texture_picker(
                ui,
                "Picture",
                &mut b.texture,
                textures,
                Some((lref, TexSlot::Backdrop)),
            );
        }
        return;
    }
    let labels = RaySettings::labels(b.kind);
    if labels.iter().any(|l| l.is_some()) {
        section(ui, "Raymarching", true, |ui| {
            ray_ui(ui, b.kind, &mut b.ray, b.texture.is_some())
        });
    }
}

/// One layer of a battle background.
fn battle_layer_ui(ui: &mut Ui, l: &mut BattleLayer, front: bool) {
    combo(
        ui,
        "Pattern",
        "",
        &mut l.pattern,
        &BattlePattern::ALL,
        |p| p.label(),
    );
    param(
        ui,
        "Tiles",
        "Pattern repeats from top to bottom",
        &mut l.tiles,
        0.5..=16.0,
    );
    row(
        ui,
        "Scroll / loop",
        "Tiles scrolled per loop: sideways, up",
        |ui| {
            ui.add(
                egui::DragValue::new(&mut l.scroll[0])
                    .range(-32..=32)
                    .speed(0.1)
                    .prefix("x "),
            );
            ui.add(
                egui::DragValue::new(&mut l.scroll[1])
                    .range(-32..=32)
                    .speed(0.1)
                    .prefix("y "),
            );
        },
    );
    combo(
        ui,
        "Line warp",
        "How the lines wobble",
        &mut l.warp,
        &LineWarp::ALL,
        |w| w.label(),
    );
    if l.warp != LineWarp::None {
        param(
            ui,
            "Amount",
            "How far the lines move",
            &mut l.amount,
            0.0..=0.3,
        );
        param(
            ui,
            "Waves",
            "Waves from top to bottom",
            &mut l.waves,
            0.0..=20.0,
        );
        drag_i(
            ui,
            "Waves / loop",
            "Times the waves roll by per loop",
            &mut l.wave_speed,
            -32..=32,
        );
    }
    param(
        ui,
        "Bands",
        "Times the colours go round across one tile",
        &mut l.bands,
        0.0..=8.0,
    );
    drag_i(
        ui,
        "Colour cycles",
        "Times the colours cycle through the pattern per loop",
        &mut l.cycles,
        -32..=32,
    );
    if front {
        param(ui, "Opacity", "", &mut l.opacity, 0.0..=1.0);
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
    param(
        ui,
        "Turbulence",
        "Quake's wobbling liquid: the surface patterns sway by a sine of themselves (in pattern cells)",
        &mut l.turbulence,
        0.0..=0.5,
    );
    if l.turbulence.base != 0.0 || l.turbulence.is_animated() {
        drag_i(
            ui,
            "Wobbles / loop",
            "Whole wobbles per loop",
            &mut l.turb_cycles,
            -32..=32,
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
        param(
            ui,
            "Turbulence",
            "Quake's wobbling liquid: the streaks sway, a wobble per streak run",
            &mut f.turbulence,
            0.0..=0.5,
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
        check(
            ui,
            "Infinite",
            "Stretch to the horizon and fade into the sky there, with no visible edge",
            &mut f.infinite,
        );
        if !f.infinite {
            slider(ui, "Size", "", &mut f.size, 1.0..=200.0);
        }
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
    ui.menu_button("🖼 Sprites", |ui| {
        let sprites: [(&str, &str, SpriteLayer); 4] = [
            (
                "Glow dots",
                "A swarm of soft glowing dots",
                SpriteLayer {
                    blend: SpriteBlend::Additive,
                    size: Param::new(0.4),
                    tint: [0.5, 0.8, 1.0],
                    glow: Param::new(2.0),
                    instancer: Instancer::Orbit {
                        count: 60,
                        radius: 3.0,
                        spread: 1.0,
                        speed: 1,
                        seed: 1,
                    },
                    ..Default::default()
                },
            ),
            (
                "Flames",
                "Flickering flames standing up (sprite sheet)",
                SpriteLayer {
                    image: Some("sheet_flame".into()),
                    columns: 4,
                    rows: 4,
                    cycles: 4,
                    random_start: true,
                    facing: SpriteFacing::Upright,
                    blend: SpriteBlend::Additive,
                    glow: Param::new(1.5),
                    instancer: Instancer::Radial {
                        count: 8,
                        radius: 2.5,
                    },
                    ..Default::default()
                },
            ),
            (
                "Explosion",
                "A fireball, once per loop (sprite sheet)",
                SpriteLayer {
                    image: Some("sheet_explosion".into()),
                    columns: 4,
                    rows: 4,
                    size: Param::new(3.0),
                    glow: Param::new(1.4),
                    ..Default::default()
                },
            ),
            (
                "Image plane",
                "A flat picture in the scene (choose your image)",
                SpriteLayer {
                    image: Some("win9x".into()),
                    facing: SpriteFacing::Fixed,
                    blend: SpriteBlend::Cutout,
                    size: Param::new(2.0),
                    ..Default::default()
                },
            ),
        ];
        for (name, tip, sp) in sprites {
            if ui.button(name).on_hover_text(tip).clicked() {
                out = Some(Layer::new(name, LayerKind::Sprite(sp)).at([0.0, 1.5, 0.0]));
            }
        }
    });
    if ui
        .button("⚡ Electric arcs")
        .on_hover_text("Tesla-coil lightning between two points, or to the copies of a shape")
        .clicked()
    {
        out = Some(
            Layer::new("Electric arcs", LayerKind::Arcs(ArcLayer::default())).at([0.0, 1.5, 0.0]),
        );
        ui.close();
    }
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
    ui.menu_button("🏷 Logo", |ui| {
        let logos: [(&str, &str, LogoLayer); 5] = [
            (
                "Title",
                "Big text in the middle of the screen",
                LogoLayer::default(),
            ),
            (
                "Chrome logo",
                "Shiny bevelled letters with an outline and a shadow",
                LogoLayer {
                    text: "CHROME".into(),
                    size: Param::new(0.25),
                    color_top: ez_core::color::hex(0xfff2c0),
                    color_bottom: ez_core::color::hex(0xff8a3d),
                    outline: Param::new(0.35),
                    outline_color: ez_core::color::hex(0x1a0830),
                    shadow: Param::new(0.8),
                    chrome: Param::new(0.7),
                    ..Default::default()
                },
            ),
            (
                "Gold logo",
                "Bevelled gold letters with a glint sweeping across on every bar",
                LogoLayer {
                    text: "GOLD".into(),
                    size: Param::new(0.25),
                    bevel: LogoBevel::Round,
                    bevel_width: Param::new(0.6),
                    matcap: Some("matcap_gold".into()),
                    shine: Param::new(0.8),
                    outline: Param::new(0.25),
                    outline_color: ez_core::color::hex(0x2a1400),
                    shadow: Param::new(0.8),
                    glint: Param::new(1.5),
                    glint_cycles: 4,
                    ..Default::default()
                },
            ),
            (
                "Corner tag",
                "Small pixel letters in the bottom right corner",
                LogoLayer {
                    text: "EZ2".into(),
                    font: TextFont::Pixel,
                    x: Param::new(0.97),
                    y: Param::new(0.04),
                    anchor: LogoAnchor::BottomRight,
                    size: Param::new(0.08),
                    shadow: Param::new(0.8),
                    ..Default::default()
                },
            ),
            (
                "Image logo",
                "Your picture as a logo (choose the image; transparent parts are cut away)",
                LogoLayer {
                    source: LogoSource::Image,
                    image: Some("sheet_coin".into()),
                    colors: LogoColors::Image,
                    size: Param::new(0.3),
                    ..Default::default()
                },
            ),
        ];
        for (name, tip, mut g) in logos {
            if ui.button(name).on_hover_text(tip).clicked() {
                measure_from_anchor(&mut g);
                out = Some(Layer::new(name, LayerKind::Logo(g)));
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
    if ui.button("🏁 Mode 7 floor").clicked() {
        out = Some(Layer::new(
            "Mode 7 floor",
            LayerKind::Mode7(Mode7Floor::default()),
        ));
    }
    if ui.button("⊞ Mirror floor").clicked() {
        out = Some(Layer::new(
            "Mirror floor",
            LayerKind::Mirror(MirrorFloor::default()),
        ));
    }
    if ui
        .button("☁ Gaussian splats")
        .on_hover_text(
            "A captured object or place made of soft coloured blobs, from a .ply, .spz or \
             .splat file (choose it in the layer)",
        )
        .clicked()
    {
        out = Some(splat_layer(None));
        ui.close();
    }
    if ui
        .button("📦 3D model or splat file…")
        .on_hover_text("glTF / GLB, OBJ, STL, PLY, OFF or 3MF models; PLY, SPZ or .splat splats")
        .clicked()
    {
        platform::pick(Purpose::AddModelLayer);
        ui.close();
    }
    out
}

/// A new splat layer showing the file at asset `path` (none: the built-in
/// cloud).
pub fn splat_layer(path: Option<&str>) -> Layer {
    let name = path
        .map(ez_core::store::file_name)
        .map(|f| f.rsplit_once('.').map(|(s, _)| s).unwrap_or(f).to_string())
        .unwrap_or_else(|| "Gaussian splats".into());
    Layer::new(
        name,
        LayerKind::Splat(SplatLayer {
            file: path.map(str::to_string),
            ..Default::default()
        }),
    )
    .at([0.0, 1.5, 0.0])
    .scaled(2.0)
}

fn splat_ui(ui: &mut Ui, s: &mut SplatLayer, lref: LayerRef) {
    section(ui, "Gaussian splats", true, |ui| {
        ui.label(
            RichText::new(
                "A captured object or place made of millions of soft blobs (3D Gaussian \
                 splatting), from a .ply, .spz or .splat file. Place, turn and spin it with \
                 Placement & motion.",
            )
            .weak()
            .small(),
        );
        row(ui, "File", "The splat file (.ply, .spz or .splat)", |ui| {
            let shown = s
                .file
                .as_deref()
                .map(ez_core::store::file_name)
                .unwrap_or("Built-in galaxy");
            ui.label(shown);
            if ui.button("Choose…").clicked() {
                platform::pick(Purpose::SetSplats(lref));
            }
            if s.file.is_some() && ui.button("Built-in").clicked() {
                s.file = None;
            }
        });
        combo(
            ui,
            "Up",
            "Which way is up in the file. Change it if the splats lie on their side or upside \
             down.",
            &mut s.up,
            &SplatUp::ALL,
            SplatUp::label,
        );
        check(
            ui,
            "Fit to size",
            "Centre the splats and fit them into the layer's size (strays far out ignored); off \
             keeps the file's own units and origin",
            &mut s.fit,
        );
        color(ui, "Tint", "Multiplies the colours", &mut s.tint);
        param(
            ui,
            "Brightness",
            "Above 1 glows",
            &mut s.brightness,
            0.0..=3.0,
        );
        param(ui, "Opacity", "", &mut s.opacity, 0.0..=1.0);
        param(
            ui,
            "Splat size",
            "Size of every splat: 1 as captured, smaller turns the scene into dots",
            &mut s.splat_size,
            0.0..=3.0,
        );
        param(
            ui,
            "Scatter",
            "Moves every splat out from the centre: animate it for a scene that bursts apart \
             and comes back together",
            &mut s.scatter,
            0.0..=3.0,
        );
        row(
            ui,
            "Most splats",
            "The most splats drawn; above it the faintest and smallest are left out (fewer is \
             faster)",
            |ui| {
                ui.add(
                    egui::DragValue::new(&mut s.max_splats)
                        .range(10_000..=ez_render::splats::MAX_SPLATS)
                        .speed(10_000.0),
                )
            },
        );
    });
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
        LayerKind::Sprite(_) => "🖼",
        LayerKind::Logo(_) => "🏷",
        LayerKind::Arcs(_) => "⚡",
        LayerKind::Mode7(_) => "🏁",
        LayerKind::Splat(_) => "☁",
    }
}

fn mode7_ui(ui: &mut Ui, f: &mut Mode7Floor, textures: &[UserTexture]) {
    section(ui, "Mode 7 floor", true, |ui| {
        ui.label(
            RichText::new(
                "An endless flat picture at the layer's height, up to a hard horizon, like SNES \
                 racing games and Saturn floors. It turns around the layer's position. Set the \
                 height with Placement → Position y; Size scales the tiles.",
            )
            .weak()
            .small(),
        );
        texture_picker(ui, "Picture", &mut f.texture, textures, None);
        slider(
            ui,
            "Tile size",
            "World units per tile of the picture",
            &mut f.tile_size,
            0.25..=40.0,
        );
        drag_i(
            ui,
            "Turns / loop",
            "Whole turns around the layer's position per loop",
            &mut f.turns,
            -16..=16,
        );
        row(
            ui,
            "Scroll / loop",
            "Tiles scrolled per loop (x, z)",
            |ui| {
                ui.add(
                    egui::DragValue::new(&mut f.scroll[0])
                        .range(-64..=64)
                        .speed(0.1)
                        .prefix("x "),
                );
                ui.add(
                    egui::DragValue::new(&mut f.scroll[1])
                        .range(-64..=64)
                        .speed(0.1)
                        .prefix("z "),
                );
            },
        );
        color(ui, "Tint", "Multiplies the picture", &mut f.tint);
        param(ui, "Brightness", "", &mut f.brightness, 0.0..=3.0);
        check(
            ui,
            "Square pixels",
            "No smoothing between the picture's pixels (the console look)",
            &mut f.pixelated,
        );
        check(
            ui,
            "Fade into fog",
            "Fade into the fog colour in the distance; off keeps the hard, bright horizon",
            &mut f.fog,
        );
    });
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

/// Temp-data key: the project's loop length in seconds.
pub const LOOP_SECONDS: &str = "ez2-loop-seconds";

/// Play an animation's frames in a sprite layer, at its own speed as near
/// as whole plays per loop allow.
pub fn fit_sprite_to_clip(sp: &mut SpriteLayer, clip: &FrameSheet, loop_seconds: f32) {
    sp.columns = clip.columns;
    sp.rows = clip.rows;
    sp.frames = clip.frames;
    sp.cycles = clip.cycles_per_loop(loop_seconds);
}

/// What an animation is and how it fits the loop.
fn clip_note(ui: &mut Ui, clip: &FrameSheet, loop_seconds: f32) {
    let plays = clip.cycles_per_loop(loop_seconds);
    let speed = plays as f32 * clip.seconds / loop_seconds.max(1e-3);
    let mut text = format!(
        "Animation: {} frames, {:.1} s. Plays {plays}× per loop",
        clip.frames, clip.seconds
    );
    if (speed - 1.0).abs() > 0.05 {
        text += &format!(" ({:.0}% of its speed)", speed * 100.0);
    }
    text.push('.');
    ui.label(RichText::new(text).weak().small());
}

fn sprite_ui(ui: &mut Ui, sp: &mut SpriteLayer, textures: &[UserTexture], lref: LayerRef) {
    section(ui, "Image", true, |ui| {
        let before = sp.image.clone();
        texture_picker(
            ui,
            "Image",
            &mut sp.image,
            textures,
            Some((lref, platform::TexSlot::Sprite)),
        );
        let clip = sp
            .image
            .as_deref()
            .and_then(|n| textures.iter().find(|t| t.name == n))
            .and_then(|t| t.clip.clone());
        let loop_s: f32 = ui
            .data(|d| d.get_temp(egui::Id::new(LOOP_SECONDS)))
            .unwrap_or(4.0);
        // Built-in sheets and animations set their own grid.
        if sp.image != before {
            if let Some(c) = &clip {
                fit_sprite_to_clip(sp, c, loop_s);
            } else if let Some((c, r)) = sp.image.as_deref().and_then(texgen::sheet_grid) {
                sp.columns = c;
                sp.rows = r;
                sp.frames = 0;
            } else if before.as_deref().and_then(texgen::sheet_grid).is_some()
                || sp.columns * sp.rows > 1
            {
                sp.columns = 1;
                sp.rows = 1;
                sp.frames = 0;
            }
        }
        if let Some(c) = &clip {
            clip_note(ui, c, loop_s);
            if ui
                .small_button("Play at its own speed")
                .on_hover_text("Plays per loop that keep it nearest the speed it was made at")
                .clicked()
            {
                fit_sprite_to_clip(sp, c, loop_s);
            }
        }
        if sp.image.is_none() {
            ui.label(
                RichText::new("No image: a soft glowing dot.")
                    .weak()
                    .small(),
            );
        }
        combo(ui, "Facing", "", &mut sp.facing, &SpriteFacing::ALL, |f| {
            f.label()
        });
        combo(
            ui,
            "Blend",
            "Alpha: soft edges. Additive: light adds up. Cutout: hard edges, solid.",
            &mut sp.blend,
            &SpriteBlend::ALL,
            |b| b.label(),
        );
        param(
            ui,
            "Size",
            "Height; the width follows the image",
            &mut sp.size,
            0.0..=10.0,
        );
        param(ui, "Opacity", "", &mut sp.opacity, 0.0..=1.0);
        color(ui, "Tint", "Multiplies the image", &mut sp.tint);
        param(
            ui,
            "Glow",
            "Brightness: above 1 blooms",
            &mut sp.glow,
            0.0..=5.0,
        );
        light_style_ui(ui, "Flicker (light style)", &mut sp.glow_style);
        ui.checkbox(&mut sp.pixelated, "Pixelated (sharp pixel art)");
    });
    section(
        ui,
        "Animation (sprite sheet)",
        sp.columns * sp.rows > 1,
        |ui| {
            drag_u(
                ui,
                "Columns",
                "Frames across the sheet",
                &mut sp.columns,
                1..=64,
            );
            drag_u(ui, "Rows", "Frames down the sheet", &mut sp.rows, 1..=64);
            drag_u(
                ui,
                "Frames",
                "Frames used, left to right, top to bottom (0 = all)",
                &mut sp.frames,
                0..=4096,
            );
            drag_i(
                ui,
                "Plays / loop",
                "Whole passes through the frames per loop (0 = first frame)",
                &mut sp.cycles,
                -32..=32,
            );
            ui.checkbox(&mut sp.random_start, "Each copy starts on its own frame");
        },
    );
    section(ui, "Copies (instancing)", true, |ui| {
        instancer_ui(ui, &mut sp.instancer)
    });
    if !matches!(sp.instancer, Instancer::Single) {
        section(ui, "Variation", false, |ui| {
            variation_ui(ui, &mut sp.variation)
        });
    }
}

fn arcs_ui(ui: &mut Ui, a: &mut ArcLayer) {
    section(ui, "Arcs", true, |ui| {
        let names: Vec<String> = ui
            .data(|d| d.get_temp(egui::Id::new(COPY_LAYER_NAMES)))
            .unwrap_or_default();
        let first = names.first().cloned().unwrap_or_default();
        row(ui, "Path", "", |ui| {
            egui::ComboBox::from_id_salt("arc_path")
                .selected_text(a.path.label())
                .show_ui(ui, |ui| {
                    let target = a.path.target().map(String::from).unwrap_or(first.clone());
                    for p in [
                        ArcPath::Points {
                            from: [-2.0, 0.0, 0.0],
                            to: [2.0, 0.0, 0.0],
                        },
                        ArcPath::Nearest {
                            target: target.clone(),
                            count: 3,
                        },
                        ArcPath::Chain { target },
                    ] {
                        let same = std::mem::discriminant(&p) == std::mem::discriminant(&a.path);
                        if ui.selectable_label(same, p.label()).clicked() && !same {
                            a.path = p;
                        }
                    }
                });
        });
        match &mut a.path {
            ArcPath::Points { from, to } => {
                vec3(ui, "From", "In the layer's space", from, 0.05);
                vec3(ui, "To", "", to, 0.05);
            }
            ArcPath::Nearest { target, count } => {
                arc_target_ui(ui, target, &names);
                drag_u(
                    ui,
                    "Arcs",
                    "How many of the nearest copies are struck",
                    count,
                    1..=64,
                );
            }
            ArcPath::Chain { target } => arc_target_ui(ui, target, &names),
        }
        drag_u(
            ui,
            "Strikes / loop",
            "New shapes per loop",
            &mut a.strikes,
            1..=128,
        );
        param(
            ui,
            "Jaggedness",
            "How far the arc zigzags",
            &mut a.jag,
            0.0..=0.6,
        );
        slider(
            ui,
            "Crawl",
            "How far the zigzag slides during a strike",
            &mut a.crawl,
            0.0..=4.0,
        );
        slider(
            ui,
            "Fade",
            "How much a strike fades before the next (0 = steady)",
            &mut a.fade,
            0.0..=1.0,
        );
        ui.checkbox(&mut a.branches, "Branches");
        param(ui, "Width", "", &mut a.width, 0.0..=0.5);
        color(ui, "Colour", "", &mut a.color);
        param(
            ui,
            "Glow",
            "Brightness (above 1 blooms)",
            &mut a.glow,
            0.0..=8.0,
        );
        drag_u(ui, "Seed", "Different zigzags", &mut a.seed, 0..=9999);
    });
}

fn arc_target_ui(ui: &mut Ui, target: &mut String, names: &[String]) {
    row(
        ui,
        "Target",
        "The shape or sprite layer whose copies the arcs reach; the arcs start at this layer's position",
        |ui| {
            egui::ComboBox::from_id_salt("arc_target")
                .selected_text(if target.is_empty() { "pick a layer" } else { target.as_str() })
                .show_ui(ui, |ui| {
                    if names.is_empty() {
                        ui.label("Add a shape or sprite layer first");
                    }
                    for n in names {
                        if ui.selectable_label(target == n, n).clicked() {
                            *target = n.clone();
                        }
                    }
                });
        },
    );
    if !target.is_empty() && !names.contains(target) {
        ui.colored_label(
            egui::Color32::LIGHT_RED,
            "No shape or sprite layer has this name",
        );
    }
}

fn logo_ui(
    ui: &mut Ui,
    g: &mut LogoLayer,
    own_name: &str,
    textures: &[UserTexture],
    lref: LayerRef,
) {
    section(ui, "Logo", true, |ui| {
        combo(ui, "Made of", "", &mut g.source, &LogoSource::ALL, |s| {
            s.label()
        });
        match g.source {
            LogoSource::Text => {
                ui.add(
                    egui::TextEdit::multiline(&mut g.text)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .hint_text("Your logo text"),
                );
                font_picker(ui, &mut g.font, &mut g.font_file, lref);
                text_values_ui(ui, &mut g.values);
            }
            LogoSource::Image => {
                texture_picker(
                    ui,
                    "Image",
                    &mut g.image,
                    textures,
                    Some((lref, TexSlot::Logo)),
                );
                combo(
                    ui,
                    "Shape from",
                    "Which parts of the image are the logo",
                    &mut g.mask,
                    &LogoMask::ALL,
                    |m| m.label(),
                );
            }
        }
    });
    section(ui, "On screen", true, |ui| {
        let names: Vec<String> = ui
            .data(|d| d.get_temp::<Vec<String>>(egui::Id::new(LOGO_NAMES)))
            .unwrap_or_default()
            .into_iter()
            .filter(|n| n != own_name)
            .collect();
        let shapes: Vec<String> = ui
            .data(|d| d.get_temp::<Vec<String>>(egui::Id::new(SHAPE_NAMES)))
            .unwrap_or_default();
        row(
            ui,
            "Attach to",
            "Place the logo on the screen, against another logo, or where a 3D layer \
             shows (it follows it as the camera moves)",
            |ui| {
                let label = if g.attach_to.is_empty() {
                    "The screen".to_string()
                } else {
                    g.attach_to.clone()
                };
                let before = g.attach_to.clone();
                egui::ComboBox::from_id_salt("logo_attach")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut g.attach_to, String::new(), "The screen");
                        if !names.is_empty() {
                            ui.label(RichText::new("Logos").small().weak());
                        }
                        for n in &names {
                            ui.selectable_value(&mut g.attach_to, n.clone(), n);
                        }
                        if !shapes.is_empty() {
                            ui.label(RichText::new("3D layers").small().weak());
                        }
                        for n in &shapes {
                            ui.selectable_value(&mut g.attach_to, n.clone(), format!("🧊 {n}"));
                        }
                    });
                if g.attach_to != before {
                    // Start from a sensible spot: the middle of the screen,
                    // or just under the other logo or the 3D layer.
                    let place = if g.attach_to.is_empty() {
                        LogoAnchor::Centre
                    } else {
                        LogoAnchor::Bottom
                    };
                    snap_logo(g, place);
                }
            },
        );
        if !g.attach_to.is_empty()
            && !names.contains(&g.attach_to)
            && !shapes.contains(&g.attach_to)
        {
            ui.label(
                RichText::new("No layer with that name: placed on the screen.")
                    .weak()
                    .small(),
            );
        }
        let tip = if g.attach_to.is_empty() {
            "Snap into a part of the screen (a corner, an edge or the middle)"
        } else {
            "Snap against what it is attached to (the other logo, or the area the 3D layer \
             covers on the screen): below, above, beside, at a corner or on top of it"
        };
        row(ui, "Place", tip, |ui| {
            if let Some(a) = anchor_grid(ui, "logo_place", g.attach_point) {
                snap_logo(g, a);
            }
        });
        param(
            ui,
            "Offset across",
            "From the point it is placed at, as a fraction of the screen width",
            &mut g.x,
            -1.0..=1.0,
        );
        param(
            ui,
            "Offset up",
            "From the point it is placed at, as a fraction of the screen height",
            &mut g.y,
            -1.0..=1.0,
        );
        let from = g.attach_point;
        combo(
            ui,
            "Measured from",
            "The point of the screen (or of the other logo) the offsets start from",
            &mut g.attach_point,
            &LogoAnchor::ALL,
            |a| a.label(),
        );
        if g.attach_point != from && g.attach_to.is_empty() {
            // On the screen the logo stays where it is.
            let (a, b) = (from.point(), g.attach_point.point());
            g.x.base += a[0] - b[0];
            g.y.base += a[1] - b[1];
        }
        combo(
            ui,
            "Anchor",
            "The point of the logo at that position (it turns around it too)",
            &mut g.anchor,
            &LogoAnchor::ALL,
            |a| a.label(),
        );
        param(
            ui,
            "Size",
            "Height, as a fraction of the screen height",
            &mut g.size,
            0.0..=1.0,
        );
        param(ui, "Turn", "Degrees", &mut g.rotation, -180.0..=180.0);
        param(ui, "Opacity", "", &mut g.opacity, 0.0..=1.0);
    });
    section(ui, "Look", true, |ui| {
        combo(ui, "Colours", "", &mut g.colors, &LogoColors::ALL, |c| {
            c.label()
        });
        match g.colors {
            LogoColors::Image => {
                color(ui, "Tint", "Multiplies the colours", &mut g.tint);
            }
            LogoColors::Gradient => {
                color(ui, "Top colour", "", &mut g.color_top);
                color(ui, "Bottom colour", "", &mut g.color_bottom);
            }
        }
        param(
            ui,
            "Glow",
            "Brightness; above 1 it glows",
            &mut g.glow,
            0.0..=8.0,
        );
        param(ui, "Outline", "", &mut g.outline, 0.0..=1.0);
        if g.outline.is_animated() || g.outline.base > 0.0 {
            color(ui, "Outline colour", "", &mut g.outline_color);
        }
        param(ui, "Drop shadow", "", &mut g.shadow, 0.0..=1.0);
        param(
            ui,
            "Chrome",
            "Shiny bevelled edges reflecting the sky",
            &mut g.chrome,
            0.0..=1.0,
        );
    });
    section(ui, "Lighting", g.bevel != LogoBevel::Off, |ui| {
        combo(
            ui,
            "Bevel",
            "The shape of the edges, lit by a light on the screen",
            &mut g.bevel,
            &LogoBevel::ALL,
            |b| b.label(),
        );
        if g.bevel != LogoBevel::Off {
            if g.bevel != LogoBevel::Pillow {
                param(
                    ui,
                    "Bevel width",
                    "How far in from the edge it reaches",
                    &mut g.bevel_width,
                    0.05..=2.0,
                );
            }
            param(
                ui,
                "Depth",
                "How steep it is",
                &mut g.bevel_depth,
                0.0..=3.0,
            );
            if g.bevel == LogoBevel::Stepped {
                drag_u(ui, "Steps", "Terraces", &mut g.steps, 1..=12);
            }
            param(
                ui,
                "Light from",
                "Degrees: 0 = from the right, 90 = from above",
                &mut g.light_angle,
                -180.0..=180.0,
            );
            if ui
                .small_button("↻ Circle the light")
                .on_hover_text("The light goes round once per loop")
                .clicked()
            {
                let from = g.light_angle.base;
                g.light_angle = Param::new(from).osc(Wave::Saw, 180.0, 1);
            }
            param(
                ui,
                "Light height",
                "Degrees above the logo: low lights rake across the bevel",
                &mut g.light_height,
                0.0..=90.0,
            );
            color(ui, "Light colour", "", &mut g.light_color);
            param(
                ui,
                "Shading",
                "How much the light shades the colour (0 = flat)",
                &mut g.lighting,
                0.0..=2.0,
            );
            param(ui, "Shine", "Highlights", &mut g.shine, 0.0..=3.0);
            slider(
                ui,
                "Gloss",
                "Small sharp highlights (1) or broad soft ones (0)",
                &mut g.gloss,
                0.0..=1.0,
            );
        }
        matcap_picker(ui, &mut g.matcap, textures, lref);
        if g.matcap.is_some() {
            param(
                ui,
                "Material",
                "How much of the material shows",
                &mut g.matcap_amount,
                0.0..=1.0,
            );
        }
    });
    section(
        ui,
        "Glint",
        g.glint.is_animated() || g.glint.base > 0.0,
        |ui| {
            param(
                ui,
                "Glint",
                "A bright band sweeping across the logo",
                &mut g.glint,
                0.0..=4.0,
            );
            drag_i(
                ui,
                "Sweeps / loop",
                "Whole sweeps per loop (negative = the other way)",
                &mut g.glint_cycles,
                -32..=32,
            );
            slider(
                ui,
                "Width",
                "In logo heights",
                &mut g.glint_width,
                0.01..=1.0,
            );
            slider(
                ui,
                "Direction",
                "Degrees: 0 = to the right, 90 = upwards",
                &mut g.glint_angle,
                -180.0..=180.0,
            );
            color(ui, "Colour", "", &mut g.glint_color);
        },
    );
    logo_effects_ui(ui, g, textures, lref);
    logo_raster_ui(ui, g);
    logo_retro_ui(ui, g);
    logo_scene_ui(ui, g);
}

/// How a logo meets the scene: glass, rays, echoes.
fn logo_scene_ui(ui: &mut Ui, g: &mut LogoLayer) {
    let on = |p: &Param| p.is_animated() || p.base > 0.0;
    section(
        ui,
        "Glass, rays & echoes",
        on(&g.glass) || on(&g.rays) || g.echoes > 0,
        |ui| {
            param(
                ui,
                "Glass",
                "Letters of glass: the scene behind shows through, bent by their edges",
                &mut g.glass,
                0.0..=1.0,
            );
            if on(&g.glass) {
                slider(
                    ui,
                    "Bend",
                    "How far the glass bends the scene, in logo heights",
                    &mut g.refraction,
                    0.0..=0.5,
                );
                slider(
                    ui,
                    "Dispersion",
                    "Colours bent by different amounts (rainbow edges)",
                    &mut g.dispersion,
                    0.0..=1.0,
                );
                color(ui, "Glass tint", "", &mut g.glass_tint);
            }
            ui.separator();
            param(
                ui,
                "Rays",
                "Light streaming out from the logo",
                &mut g.rays,
                0.0..=4.0,
            );
            if on(&g.rays) {
                slider(ui, "Length", "", &mut g.rays_length, 0.05..=1.0);
                slider(
                    ui,
                    "Threshold",
                    "Only light brighter than this streams",
                    &mut g.rays_threshold,
                    0.0..=2.0,
                );
                ui.checkbox(
                    &mut g.rays_shadow,
                    "Shadow: rays of the light behind, the logo blocking them",
                );
                color(ui, "Ray tint", "", &mut g.rays_tint);
            }
            ui.separator();
            drag_u(
                ui,
                "Echoes",
                "Fading copies where the logo was a moment ago (animate it to see them)",
                &mut g.echoes,
                0..=16,
            );
            if g.echoes > 0 {
                slider(
                    ui,
                    "Spacing",
                    "Time between copies, as a fraction of the loop",
                    &mut g.echo_spacing,
                    0.002..=0.25,
                );
                slider(
                    ui,
                    "Fade",
                    "Each copy's opacity relative to the next",
                    &mut g.echo_fade,
                    0.0..=1.0,
                );
            }
        },
    );
}

/// Retro looks of a logo.
fn logo_retro_ui(ui: &mut Ui, g: &mut LogoLayer) {
    let on = |p: &Param| p.is_animated() || p.base > 0.0;
    let open = on(&g.pixelate)
        || g.palette.is_some()
        || on(&g.halftone)
        || on(&g.scanlines)
        || on(&g.moire);
    section(ui, "Retro looks", open, |ui| {
        param(
            ui,
            "Pixel blocks",
            "Block size in logo heights (0 = sharp). Animate it to pixelate in or out",
            &mut g.pixelate,
            0.0..=0.3,
        );
        row(ui, "Palette", "Only the colours of a retro machine", |ui| {
            let text = g.palette.map(|p| p.label()).unwrap_or("Any colours");
            egui::ComboBox::from_id_salt("logo_palette")
                .selected_text(text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut g.palette, None, "Any colours");
                    // Logos take up to 16 colours (and the VGA cube).
                    let fits =
                        |p: &PaletteId| p.levels().is_none_or(|n| n == 6) && p.colors().len() <= 16;
                    for p in PaletteId::ALL.into_iter().filter(fits) {
                        ui.selectable_value(&mut g.palette, Some(p), p.label());
                    }
                });
        });
        if let Some(pal) = g.palette {
            slider(
                ui,
                "Dither",
                "Ordered dither between palette colours",
                &mut g.dither,
                0.0..=1.0,
            );
            if pal != PaletteId::Vga {
                ui.checkbox(
                    &mut g.palette_by_brightness,
                    "By brightness (the palette as a dark-to-light ramp)",
                );
                drag_i(
                    ui,
                    "Cycles / loop",
                    "Palette colours rotating, whole turns per loop",
                    &mut g.palette_cycles,
                    -16..=16,
                );
            }
        }
        ui.separator();
        param(
            ui,
            "Halftone",
            "Dots as big as the colour is bright (0 = none, 1 = only dots)",
            &mut g.halftone,
            0.0..=1.0,
        );
        if on(&g.halftone) {
            slider(
                ui,
                "Dot spacing",
                "In logo heights",
                &mut g.halftone_size,
                0.005..=0.2,
            );
            slider(
                ui,
                "Screen angle",
                "Degrees",
                &mut g.halftone_angle,
                -90.0..=90.0,
            );
        }
        ui.separator();
        param(
            ui,
            "Scanlines",
            "Dark gaps between the lines",
            &mut g.scanlines,
            0.0..=1.0,
        );
        slider(
            ui,
            "Lines",
            "Per logo height",
            &mut g.scanline_count,
            2.0..=200.0,
        );
        slider(
            ui,
            "Phosphor stripes",
            "Red, green and blue stripes like a CRT's mask",
            &mut g.crt_mask,
            0.0..=1.0,
        );
        param(
            ui,
            "Line glow",
            "Extra brightness in the lines (it blooms)",
            &mut g.crt_glow,
            0.0..=3.0,
        );
        ui.separator();
        param(
            ui,
            "Moiré",
            "Two turning line patterns beating against each other",
            &mut g.moire,
            0.0..=1.0,
        );
        if on(&g.moire) {
            slider(
                ui,
                "Lines",
                "Per logo height",
                &mut g.moire_lines,
                2.0..=120.0,
            );
            drag_i(
                ui,
                "Turns / loop",
                "The patterns turn opposite ways",
                &mut g.moire_cycles,
                -8..=8,
            );
        }
    });
}

/// Distance-field effects of a logo (distances in logo heights).
fn logo_effects_ui(ui: &mut Ui, g: &mut LogoLayer, textures: &[UserTexture], lref: LayerRef) {
    let on = |p: &Param| p.is_animated() || p.base > 0.0;
    section(
        ui,
        "Rings & outlines",
        on(&g.contours) || g.stack > 0,
        |ui| {
            param(
                ui,
                "Rings",
                "Brightness of rings rippling out from the edges",
                &mut g.contours,
                0.0..=4.0,
            );
            if on(&g.contours) {
                slider(
                    ui,
                    "Spacing",
                    "Between rings, in logo heights",
                    &mut g.contour_spacing,
                    0.01..=0.5,
                );
                drag_i(
                    ui,
                    "Rings / loop",
                    "Rings passing per loop (negative = inward)",
                    &mut g.contour_cycles,
                    -32..=32,
                );
                slider(
                    ui,
                    "Reach",
                    "How far out they fade, in logo heights",
                    &mut g.contour_reach,
                    0.05..=1.0,
                );
                slider(
                    ui,
                    "Line width",
                    "Fraction of the spacing",
                    &mut g.contour_width,
                    0.02..=1.0,
                );
                color(ui, "Ring colour", "", &mut g.contour_color);
                ui.checkbox(&mut g.contour_inside, "Inside the letters too");
            }
            ui.separator();
            drag_u(
                ui,
                "Stacked outlines",
                "Solid outlines around the logo, one outside the other",
                &mut g.stack,
                0..=16,
            );
            if g.stack > 0 {
                param(
                    ui,
                    "Width",
                    "Of each outline, in logo heights",
                    &mut g.stack_width,
                    0.0..=0.2,
                );
                slider(
                    ui,
                    "Gap",
                    "Between outlines, in logo heights",
                    &mut g.stack_gap,
                    0.0..=0.2,
                );
                color(ui, "Inner colour", "", &mut g.stack_color_a);
                color(ui, "Outer colour", "", &mut g.stack_color_b);
            }
        },
    );
    section(ui, "Extrude", on(&g.extrude), |ui| {
        param(
            ui,
            "Depth",
            "Fake 3D: the logo repeated behind itself, in logo heights",
            &mut g.extrude,
            0.0..=1.0,
        );
        slider(
            ui,
            "Direction",
            "Degrees: 0 = to the right, -90 = down",
            &mut g.extrude_angle,
            -180.0..=180.0,
        );
        color(ui, "Side colour", "", &mut g.extrude_color);
    });
    section(ui, "Dissolve", on(&g.dissolve), |ui| {
        param(
            ui,
            "Dissolve",
            "0 = whole, 1 = burnt away. Animate it: ~ with Linear in",
            &mut g.dissolve,
            0.0..=1.0,
        );
        slider(
            ui,
            "Patches",
            "Burnt patches per logo height",
            &mut g.dissolve_scale,
            0.5..=30.0,
        );
        slider(
            ui,
            "From the edges",
            "0: patches anywhere; 1: eaten from the edges inward",
            &mut g.dissolve_edges,
            0.0..=1.0,
        );
        slider(
            ui,
            "Burn width",
            "The glowing front",
            &mut g.burn_width,
            0.0..=0.4,
        );
        color(ui, "Burn colour", "", &mut g.burn_color);
        drag_u(
            ui,
            "Seed",
            "Different patches",
            &mut g.dissolve_seed,
            0..=999,
        );
    });
    section(
        ui,
        "Reveal",
        g.reveal_amount.is_animated() || g.reveal_amount.base < 1.0,
        |ui| {
            combo(ui, "How", "", &mut g.reveal, &LogoReveal::ALL, |r| {
                r.label()
            });
            param(
                ui,
                "Shown",
                "0 = hidden, 1 = whole. Animate it to bring the logo in",
                &mut g.reveal_amount,
                0.0..=1.0,
            );
            if matches!(g.reveal, LogoReveal::Wipe | LogoReveal::Radial) {
                if g.reveal == LogoReveal::Wipe {
                    slider(
                        ui,
                        "Direction",
                        "Degrees: 0 = left to right, 90 = bottom to top",
                        &mut g.reveal_angle,
                        -180.0..=180.0,
                    );
                }
                slider(ui, "Softness", "Of the edge", &mut g.reveal_soft, 0.0..=1.0);
            }
        },
    );
    section(ui, "Morph", on(&g.morph), |ui| {
        param(
            ui,
            "Morph",
            "Blend into another logo: 0 = this one, 1 = the other",
            &mut g.morph,
            0.0..=1.0,
        );
        combo(ui, "Into", "", &mut g.morph_source, &LogoSource::ALL, |s| {
            s.label()
        });
        match g.morph_source {
            LogoSource::Text => {
                ui.add(
                    egui::TextEdit::multiline(&mut g.morph_text)
                        .desired_rows(1)
                        .desired_width(f32::INFINITY)
                        .hint_text("The other text (same font)"),
                );
            }
            LogoSource::Image => {
                texture_picker(
                    ui,
                    "Image",
                    &mut g.morph_image,
                    textures,
                    Some((lref, TexSlot::MorphImage)),
                );
                let mut mask = g.morph_mask.unwrap_or(g.mask);
                combo(
                    ui,
                    "Shape from",
                    "Which parts of this image are the logo",
                    &mut mask,
                    &LogoMask::ALL,
                    |m| m.label(),
                );
                if mask != g.morph_mask.unwrap_or(g.mask) {
                    g.morph_mask = Some(mask);
                }
            }
        }
    });
}

/// Rasters and distortion of a logo (distances in logo heights).
fn logo_raster_ui(ui: &mut Ui, g: &mut LogoLayer) {
    let on = |p: &Param| p.is_animated() || p.base > 0.0;
    section(ui, "Copper bars", on(&g.copper), |ui| {
        param(
            ui,
            "Copper",
            "Scrolling colour bars through the letters (0 = none, 1 = all bars)",
            &mut g.copper,
            0.0..=1.0,
        );
        slider(
            ui,
            "Bars",
            "Per logo height",
            &mut g.copper_bars,
            0.5..=16.0,
        );
        drag_i(
            ui,
            "Scrolls / loop",
            "Pairs of bars passing per loop (negative = upward)",
            &mut g.copper_cycles,
            -32..=32,
        );
        color(ui, "Bar colour 1", "", &mut g.copper_a);
        color(ui, "Bar colour 2", "", &mut g.copper_b);
    });
    section(
        ui,
        "Wobble & glitch",
        on(&g.wobble_x) || on(&g.wobble_y) || on(&g.glitch) || on(&g.chroma),
        |ui| {
            param(
                ui,
                "Sway",
                "Rows swaying sideways, in logo heights",
                &mut g.wobble_x,
                0.0..=0.5,
            );
            param(
                ui,
                "Bob",
                "Columns bobbing up and down, in logo heights",
                &mut g.wobble_y,
                0.0..=0.5,
            );
            if on(&g.wobble_x) || on(&g.wobble_y) {
                slider(
                    ui,
                    "Waves",
                    "Per logo height",
                    &mut g.wobble_waves,
                    0.1..=8.0,
                );
                drag_i(
                    ui,
                    "Rolls / loop",
                    "Times the waves roll past per loop",
                    &mut g.wobble_cycles,
                    -32..=32,
                );
            }
            ui.separator();
            param(
                ui,
                "Glitch",
                "Slices jumping sideways (the largest jump, in logo heights). Try 🎵 on the kick",
                &mut g.glitch,
                0.0..=0.5,
            );
            if on(&g.glitch) {
                slider(
                    ui,
                    "Slices",
                    "Per logo height",
                    &mut g.glitch_slices,
                    1.0..=60.0,
                );
                slider(ui, "Share jumping", "", &mut g.glitch_chance, 0.0..=1.0);
                drag_u(
                    ui,
                    "New jumps / loop",
                    "16 = every beat of a 16-beat loop",
                    &mut g.glitch_per_loop,
                    1..=256,
                );
                slider(
                    ui,
                    "Colour split",
                    "Of a jumping slice, in logo heights",
                    &mut g.glitch_split,
                    0.0..=0.2,
                );
            }
            ui.separator();
            param(
                ui,
                "Chromatic split",
                "Red and blue pulled apart, in logo heights",
                &mut g.chroma,
                0.0..=0.2,
            );
            if on(&g.chroma) {
                slider(
                    ui,
                    "Direction",
                    "Degrees red moves: 0 = to the right",
                    &mut g.chroma_angle,
                    -180.0..=180.0,
                );
            }
        },
    );
}

/// A material sphere for a lit logo: built-in or one of your images.
fn matcap_picker(
    ui: &mut Ui,
    matcap: &mut Option<String>,
    textures: &[UserTexture],
    lref: LayerRef,
) {
    row(
        ui,
        "Material",
        "A picture of a lit sphere; the bevel picks the colour facing each way",
        |ui| {
            let text = matcap.clone().unwrap_or_else(|| "None".into());
            egui::ComboBox::from_id_salt(ui.id().with("matcap"))
                .selected_text(text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(matcap, None, "None");
                    for (name, desc) in texgen::BUILTIN.iter().filter(|(n, _)| texgen::is_matcap(n))
                    {
                        ui.selectable_value(matcap, Some(name.to_string()), *name)
                            .on_hover_text(*desc);
                    }
                    if !textures.is_empty() {
                        ui.separator();
                        ui.label(RichText::new("Your images").weak());
                        for t in textures.iter() {
                            ui.selectable_value(matcap, Some(t.name.clone()), &t.name);
                        }
                    }
                    ui.separator();
                    if ui.button("Import image…").clicked() {
                        platform::pick(Purpose::SetTexture(lref, TexSlot::Matcap));
                    }
                });
        },
    );
}

/// A 3×3 grid of anchor points, the current one lit; returns the one
/// clicked.
fn anchor_grid(ui: &mut Ui, id: &str, current: LogoAnchor) -> Option<LogoAnchor> {
    let mut picked = None;
    egui::Grid::new(ui.id().with(id))
        .spacing([2.0, 2.0])
        .show(ui, |ui| {
            for (i, a) in LogoAnchor::ALL.into_iter().enumerate() {
                let on = a == current;
                let b = ui
                    .add(
                        egui::Button::new(if on { "■" } else { "□" })
                            .selected(on)
                            .min_size(egui::vec2(22.0, 18.0)),
                    )
                    .on_hover_text(a.label());
                if b.clicked() {
                    picked = Some(a);
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
    picked
}

/// Measure a screen-placed logo's position from its own anchor point of
/// the screen (the same place on screen), so the Place grid shows where
/// it is.
fn measure_from_anchor(g: &mut LogoLayer) {
    if g.attach_to.is_empty() {
        let (a, b) = (g.attach_point.point(), g.anchor.point());
        g.x.base += a[0] - b[0];
        g.y.base += a[1] - b[1];
        g.attach_point = g.anchor;
    }
}

/// Snap a logo to point `at` of what it is attached to. On the screen it
/// sits inside, a small margin from the edges; against another logo it
/// sits outside, touching with a small gap (the middle: on top of it).
fn snap_logo(g: &mut LogoLayer, at: LogoAnchor) {
    let [px, py] = at.point();
    // -1, 0 or 1: which side of the middle.
    let (sx, sy) = ((px - 0.5) * 2.0, (py - 0.5) * 2.0);
    g.attach_point = at;
    if g.attach_to.is_empty() {
        const MARGIN: f32 = 0.04;
        g.anchor = at;
        g.x = Param::new(-sx * MARGIN);
        g.y = Param::new(-sy * MARGIN);
    } else {
        const GAP: f32 = 0.02;
        g.anchor = at.opposite();
        g.x = Param::new(sx * GAP);
        g.y = Param::new(sy * GAP);
    }
}

/// Built-in fonts, or a TTF / OTF file.
fn font_picker(ui: &mut Ui, font: &mut TextFont, file: &mut Option<String>, lref: LayerRef) {
    row(ui, "Font", "", |ui| {
        let label = match &*file {
            Some(p) => ez_core::store::file_name(p).to_string(),
            None => font.label().to_string(),
        };
        egui::ComboBox::from_id_salt("font")
            .selected_text(label)
            .show_ui(ui, |ui| {
                for f in TextFont::ALL {
                    if ui
                        .selectable_label(file.is_none() && *font == f, f.label())
                        .clicked()
                    {
                        *font = f;
                        *file = None;
                    }
                }
                ui.separator();
                if ui.button("Font file (TTF / OTF)…").clicked() {
                    platform::pick(Purpose::SetFont(lref));
                }
            });
    });
}

/// The numbers shown by `{0}`, `{1}`… in a text: each an animatable
/// value with its digits, decimals and thousands separators.
fn text_values_ui(ui: &mut Ui, values: &mut Vec<TextValue>) {
    let mut remove = None;
    for (i, v) in values.iter_mut().enumerate() {
        ui.push_id(("text value", i), |ui| {
            param(
                ui,
                &format!("{{{i}}}"),
                "Shown where the text says {n}. Animate it: count down with a ramp \
                 (~ then once), count up to a score, follow the music",
                &mut v.value,
                0.0..=1000.0,
            );
            ui.horizontal(|ui| {
                ui.add_space(114.0);
                ui.add(
                    egui::DragValue::new(&mut v.digits)
                        .range(0..=12)
                        .prefix("digits "),
                )
                .on_hover_text("Fewest digits, padded with zeros (0 = as needed)");
                ui.add(
                    egui::DragValue::new(&mut v.decimals)
                        .range(0..=6)
                        .prefix("decimals "),
                );
                ui.checkbox(&mut v.group, "1,000")
                    .on_hover_text("Separate thousands with commas");
                if ui
                    .small_button("✕")
                    .on_hover_text("Remove this number")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        values.remove(i);
    }
    let next = values.len();
    if ui
        .button(format!("+ Number {{{next}}}"))
        .on_hover_text("A number shown in the text where it says {n}: a timer, a score, a combo…")
        .clicked()
    {
        values.push(TextValue::default());
    }
}

fn text_ui(ui: &mut Ui, t: &mut TextLayer, lref: LayerRef) {
    section(ui, "Text", true, |ui| {
        ui.add(
            egui::TextEdit::multiline(&mut t.text)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text("Type here. Greetings: one line each."),
        );
        text_values_ui(ui, &mut t.values);
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
        font_picker(ui, &mut t.font, &mut t.font_file, lref);
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
