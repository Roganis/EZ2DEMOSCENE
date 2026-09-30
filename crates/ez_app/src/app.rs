//! The editor application.

use crate::audio::AudioPlayer;
use crate::export_ui::ExportUi;
use crate::gizmo::{self, Gizmo, GizmoMode, Projector};
use crate::inspector::{self, IMAGE_EXTENSIONS, MODEL_EXTENSIONS};
use crate::library::Library;
use crate::nodes::NodeEditor;
use crate::platform::slug;
use crate::platform::{self, LayerRef, Purpose};
use crate::viewport::Viewport;
use crate::widgets::{self, ACCENT};

#[path = "shape_picker.rs"]
mod shape_picker;
#[path = "tex_picker.rs"]
mod tex_picker;
use egui::{Color32, RichText, Ui};
use ez_core::graph::Graph;
use ez_core::randomize::{randomize, RandomizeOptions};
use ez_core::*;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::path::PathBuf;

pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "ogg", "flac", "m4a", "aac"];
pub const MIDI_EXTENSIONS: &[&str] = &["mid", "midi"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Selection {
    Timing,
    Sequence,
    Camera,
    Environment,
    Colors,
    Post,
    Retro,
    Textures,
    Layer(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    View,
    Layers,
    Edit,
}

/// Below this width (in points) the phone layout is used.
const NARROW_WIDTH: f32 = 820.0;

enum Capture {
    PresetThumb,
    Still,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Simple,
    Nodes,
}

struct Thumb {
    name: &'static str,
    category: &'static str,
    description: &'static str,
    texture: Option<egui::TextureHandle>,
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
    fps_cap: platform::FpsCap,
    /// When the next frame is due while playing under an fps limit.
    next_frame: f64,
    aspect: (u32, u32),

    audio: Option<AudioPlayer>,
    audio_env: Option<std::sync::Arc<AudioEnvelope>>,
    /// What `audio_env` was built from (audio, MIDI, MIDI offset).
    music_key: (Option<String>, Option<String>, u32),
    /// Music being decoded and analysed.
    music_task: Option<crate::music_task::MusicTask>,
    /// With a sequence: preview only the scene being edited.
    solo_scene: bool,
    /// The last analysed audio file (path, analysis before MIDI).
    audio_cache: Option<(String, AudioEnvelope)>,
    /// Microphone / line-in driving the preview.
    live: Option<crate::live::LiveInput>,
    live_frame: Option<ez_core::MusicFrame>,
    /// The live channels as the preview's controls set them (never saved;
    /// exports see the channels silent).
    channels_preview: ez_core::ChannelInput,

    presets_open: bool,
    /// Open the preset gallery when the app starts.
    presets_on_startup: bool,
    thumbs: Vec<Thumb>,
    /// The name of a scene just loaded, while it is covered by the loading
    /// bar (see [`Self::loading_overlay`]).
    loading: Option<String>,
    randomize_open: bool,
    rand_opts: RandomizeOptions,
    rand_seed: u64,
    export: ExportUi,
    help_open: bool,
    graphics_open: bool,
    backend_pref: platform::GpuBackendPref,
    /// The backend choice the app was started with.
    backend_started: platform::GpuBackendPref,
    status: Option<(String, bool, f64)>,
    /// Wall-clock seconds (egui input time).
    now: f64,
    library: Library,
    last_autosave: f64,
    gallery_tab: usize,
    /// Name typed in the "save as my preset" dialog (Some = dialog open).
    preset_name: Option<String>,
    gizmo: Gizmo,
    /// Pending GPU captures (preset thumbnails, stills).
    captures: Vec<(ez_render::Readback, Project, Capture)>,
    /// Pending automated test (`?ez2test=…` in the browser).
    test: Option<String>,
    title: String,
    /// Phone layout active this frame.
    narrow: bool,
    tab: Tab,
    last_node_selection: Option<usize>,
    /// The text field being edited and where it was last seen, so the
    /// on-screen keyboard stays up while it is scrolled out of view.
    last_ime: Option<(egui::Id, egui::output::IMEOutput)>,
    /// Scroll the inspector to this text field on the next frame.
    scroll_to_field: Option<egui::Rect>,
    /// Screen height last frame (the on-screen keyboard shrinks it).
    last_screen_h: f32,
    shape_picker: shape_picker::ShapePicker,
    tex_picker: tex_picker::TexPicker,
    /// The clock was moved by hand (scrub, rewind, load): the music jumps
    /// there. Otherwise, while it plays, the music leads the clock.
    music_seek: bool,
    /// The last layer copied (Ctrl+C), in case the system clipboard can't
    /// be read back.
    layer_clipboard: Option<Layer>,
    /// Frames drawn since start (the desktop app notes a working graphics
    /// backend once a few have been drawn).
    frames_drawn: u32,
    /// Desktop graphics backend chosen in the Graphics window.
    #[cfg(not(target_arch = "wasm32"))]
    native_backend: crate::gpu_choice::Backend,
    #[cfg(not(target_arch = "wasm32"))]
    native_backend_started: crate::gpu_choice::Backend,
    /// Smoothed frame time in milliseconds.
    frame_ms: f32,
}

const AUTOSAVE_SECONDS: f64 = 30.0;
/// Bring the loop clock `time` to `want` (where the music is), the short
/// way round a loop of `period` seconds: a quarter of the way when they are
/// close (smooth, whatever the audio's update steps), straight there when
/// the picture has fallen behind (slow frames).
fn follow_clock(time: f64, want: f64, period: f64) -> f64 {
    let d = (want - time).rem_euclid(period);
    let d = if d > period / 2.0 { d - period } else { d };
    if d.abs() < 0.1 {
        (time + d * 0.25).rem_euclid(period)
    } else {
        want.rem_euclid(period)
    }
}

/// Whether `key` went down this frame with Ctrl/Cmd held (`command`) and
/// Shift held (`shift`), as they were at that moment (`None`: either way).
/// The modifiers come with the key press, so a quick tap that is over
/// within one frame still counts. Holding a key down doesn't repeat it.
fn pressed(
    i: &egui::InputState,
    key: egui::Key,
    command: Option<bool>,
    shift: Option<bool>,
) -> bool {
    i.events.iter().any(|e| match e {
        egui::Event::Key {
            key: k,
            pressed: true,
            repeat: false,
            modifiers,
            ..
        } => {
            *k == key
                && command.is_none_or(|c| modifiers.command == c)
                && shift.is_none_or(|s| modifiers.shift == s)
        }
        _ => false,
    })
}

/// First line of a layer copied to the clipboard (the layer follows as
/// JSON).
const LAYER_CLIPBOARD_TAG: &str = "EZ2DEMOSCENE layer";
/// Setting: "no" keeps the preset gallery closed at start-up.
const PRESETS_ON_STARTUP: &str = "presets_on_startup";
/// Layer load above which the layer list shows a warning.
const HEAVY_LAYER: f32 = 1.0;

fn human(n: u64) -> String {
    match n {
        0..=9_999 => n.to_string(),
        10_000..=999_999 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.2}M", n as f64 / 1e6),
    }
}

impl EzApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> EzApp {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .expect("EZ2DEMOSCENE needs the wgpu renderer");
        setup_style(&cc.egui_ctx);
        let touch = platform::is_touch_device();
        if touch {
            // Finger-sized controls.
            cc.egui_ctx.global_style_mut(|s| {
                s.spacing.interact_size.y = 30.0;
                s.spacing.button_padding = egui::vec2(8.0, 6.0);
                s.spacing.item_spacing = egui::vec2(8.0, 8.0);
                s.spacing.slider_width = 150.0;
            });
        }
        // A blank stage, paused: nothing heavy to render at start-up.
        let project = presets::empty();
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
            playing: false,
            time: 0.0,
            preview_scale: 1.0,
            fps_cap: platform::FpsCap::load(),
            next_frame: 0.0,
            aspect: (16, 9),
            audio: None,
            audio_env: None,
            music_key: (None, None, 0),
            music_task: None,
            solo_scene: true,
            audio_cache: None,
            live: None,
            live_frame: None,
            channels_preview: ez_core::ChannelInput::default(),
            presets_open: false,
            presets_on_startup: platform::load_setting(PRESETS_ON_STARTUP).as_deref() != Some("no"),
            thumbs: Vec::new(),
            loading: None,
            randomize_open: false,
            rand_opts: RandomizeOptions::default(),
            rand_seed: 1,
            export: ExportUi::default(),
            help_open: false,
            graphics_open: false,
            backend_pref: platform::GpuBackendPref::load(),
            backend_started: platform::GpuBackendPref::load(),
            status: None,
            now: 0.0,
            library: Library::open(&cc.egui_ctx),
            last_autosave: 0.0,
            gallery_tab: 0,
            preset_name: None,
            gizmo: Gizmo::new(touch),
            captures: Vec::new(),
            test: None,
            title: String::new(),
            narrow: false,
            tab: Tab::View,
            last_node_selection: None,
            last_ime: None,
            scroll_to_field: None,
            last_screen_h: 0.0,
            shape_picker: Default::default(),
            tex_picker: Default::default(),
            music_seek: true,
            layer_clipboard: None,
            frames_drawn: 0,
            #[cfg(not(target_arch = "wasm32"))]
            native_backend: crate::gpu_choice::load_pref(),
            #[cfg(not(target_arch = "wasm32"))]
            native_backend_started: crate::gpu_choice::load_pref(),
            frame_ms: 16.0,
        };
        match initial {
            Some(p) => {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                app.open_asset(&p.to_string_lossy(), &name)
            }
            None => app.presets_open = app.presets_on_startup && app.library.recovery.is_none(),
        }
        app.test = platform::query_param("ez2test");
        app
    }

    /// Automated browser test hook: `?ez2test=gif|zip|mp4|webm` loads a
    /// preset and exports it at a small size (see web/tests).
    fn run_test_hook(&mut self) {
        let Some(format) = self.test.take() else {
            return;
        };
        if !self.library.is_loaded() || self.music_task.is_some() {
            self.test = Some(format);
            return;
        }
        let preset = platform::query_param("preset").unwrap_or_else(|| "Orbiting Solid".into());
        if let Some(p) = presets::by_name(&preset) {
            self.load_project(p, None);
        }
        self.presets_open = false;
        self.library.discard_recovery();
        self.export
            .start_test(&format, &self.project, self.audio_env.as_deref());
    }

    fn set_status(&mut self, msg: impl Into<String>, error: bool) {
        self.status = Some((msg.into(), error, self.now));
    }

    fn loop_seconds(&self) -> f64 {
        self.project.timing.loop_seconds() as f64
    }

    /// Length of one playback cycle: the loop, or the song in full-track
    /// mode.
    fn play_seconds(&self) -> f64 {
        self.project.play_seconds(self.audio_env.as_deref())
    }

    /// Everything animated values depend on right now.
    fn eval_ctx(&self) -> EvalCtx {
        let ctx = self.project.ctx_at(self.time, self.audio_env.as_deref());
        let ctx = match self.live_frame {
            Some(f) => ctx.with_frame(f),
            None => ctx,
        };
        if self.project.music.channels.is_empty() {
            ctx
        } else {
            ctx.with_channels(self.channels_preview.frame())
        }
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
        self.music_seek = true;
        self.viewport.renderer.reload_assets();
        self.reload_audio();
        self.loading = Some(self.project.name.clone());
    }

    /// Open a project or pack by asset path (a file on desktop, a `mem://`
    /// asset in the browser).
    fn open_asset(&mut self, path: &str, name: &str) {
        let is_pack = ez_core::store::extension(path) == ez_core::assets::PACK_EXTENSION;
        #[cfg(not(target_arch = "wasm32"))]
        if !ez_core::store::is_mem(path) {
            let p = Path::new(path);
            if is_pack {
                let dest = self.library.unpack_dir(p);
                match ez_core::assets::unpack(p, &dest) {
                    // A pack has no editable location: "Save" asks where to put it.
                    Ok(pr) => {
                        self.load_project(pr, None);
                        self.set_status(format!("Opened pack {name}"), false);
                    }
                    Err(e) => self.set_status(format!("Could not open {name}: {e}"), true),
                }
            } else {
                match Project::load(p) {
                    Ok(pr) => {
                        self.load_project(pr, Some(p.to_path_buf()));
                        self.set_status(format!("Opened {name}"), false);
                    }
                    Err(e) => self.set_status(format!("Could not open {name}: {e}"), true),
                }
            }
            return;
        }
        // In memory (browser): the file's bytes are in the asset store.
        let result = ez_core::store::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                if is_pack {
                    let prefix = format!("pack-{}", slug(name));
                    ez_core::assets::unpack_bytes(&bytes, &prefix).map_err(|e| e.to_string())
                } else {
                    let mut p = Project::from_json(&String::from_utf8_lossy(&bytes))
                        .map_err(|e| e.to_string())?;
                    p.migrate();
                    Ok(p)
                }
            });
        ez_core::store::remove(path);
        match result {
            Ok(p) => {
                // Keep unpacked assets across page reloads.
                for a in p.asset_paths() {
                    crate::library::persist_asset(&a);
                }
                self.load_project(p, None);
                self.set_status(format!("Opened {name}"), false);
            }
            Err(e) => self.set_status(format!("Could not open {name}: {e}"), true),
        }
    }

    fn save_pack(&mut self) {
        let name = format!(
            "{}.{}",
            slug(&self.project.name),
            ez_core::assets::PACK_EXTENSION
        );
        match ez_core::assets::pack_to_bytes(&self.project) {
            Ok((bytes, report)) => {
                match platform::save_file(
                    &name,
                    ("EZ2 pack", &[ez_core::assets::PACK_EXTENSION]),
                    &bytes,
                ) {
                    Ok(Some(where_)) if report.missing.is_empty() => {
                        if platform::IS_WEB {
                            self.saved = self.project.clone();
                            self.library.discard_recovery();
                        }
                        self.set_status(
                            format!("Saved pack ({} asset(s)): {where_}", report.packed),
                            false,
                        )
                    }
                    Ok(Some(_)) => self.set_status(
                        format!(
                            "Packed, but {} file(s) were missing: {}",
                            report.missing.len(),
                            report.missing.join(", ")
                        ),
                        true,
                    ),
                    Ok(None) => {}
                    Err(e) => self.set_status(format!("Save failed: {e}"), true),
                }
            }
            Err(e) => self.set_status(format!("Pack failed: {e}"), true),
        }
    }

    fn autosave(&mut self) {
        if self.now - self.last_autosave < AUTOSAVE_SECONDS || !self.library.is_loaded() {
            return;
        }
        self.last_autosave = self.now;
        if self.project != self.saved {
            if let Err(e) = self.library.autosave(&self.project) {
                self.set_status(format!("Autosave failed: {e}"), true);
            }
        }
    }

    /// Render `project` at `(w, h)` and read it back without blocking; the
    /// result is handled by [`Self::finish_captures`].
    fn capture(&mut self, project: Project, w: u32, h: u32, what: Capture) {
        let target = self.viewport.renderer.create_target(w, h);
        let ctx = project.ctx_at(self.time, self.audio_env.as_deref());
        self.viewport.renderer.render(&project, &ctx, &target);
        let rb = self.viewport.renderer.start_readback(&target);
        self.captures.push((rb, project, what));
    }

    fn finish_captures(&mut self, ctx: &egui::Context) {
        if self.captures.is_empty() {
            return;
        }
        self.viewport.renderer.poll();
        let pending = std::mem::take(&mut self.captures);
        for (rb, project, what) in pending {
            if !rb.is_ready() {
                self.captures.push((rb, project, what));
                continue;
            }
            let (w, h) = (rb.width, rb.height);
            let Some(img) = rb
                .take()
                .and_then(|px| image::RgbaImage::from_raw(w, h, px))
            else {
                self.set_status("Capturing the image failed", true);
                continue;
            };
            match what {
                Capture::PresetThumb => match self.library.save_preset(ctx, &project, &img) {
                    Ok(()) => {
                        self.set_status(format!("Saved '{}' to My presets", project.name), false)
                    }
                    Err(e) => self.set_status(format!("Could not save preset: {e}"), true),
                },
                Capture::Still => {
                    let mut png = Vec::new();
                    let r = img
                        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                        .map_err(|e| e.to_string())
                        .and_then(|_| {
                            platform::save_file(
                                &format!("{}.png", slug(&project.name)),
                                ("PNG", &["png"]),
                                &png,
                            )
                        });
                    match r {
                        Ok(Some(where_)) => {
                            self.set_status(format!("Saved still: {where_}"), false)
                        }
                        Ok(None) => {}
                        Err(e) => self.set_status(format!("Could not save the still: {e}"), true),
                    }
                }
            }
        }
        ctx.request_repaint();
    }

    /// egui only reports a text field as being edited while it is visible,
    /// and the web backend hides the on-screen keyboard whenever none is.
    /// On a phone the keyboard itself shrinks the screen, which can push the
    /// field out of view: the keyboard then closed and reopened every frame.
    /// Keep reporting the field while it still has focus, and scroll it back
    /// into view when the screen shrinks or the user types into it.
    fn keep_keyboard(&mut self, ctx: &egui::Context) {
        let focused = ctx.memory(|m| m.focused());
        let screen_h = ctx.content_rect().height();
        let shrank = screen_h < self.last_screen_h - 1.0;
        self.last_screen_h = screen_h;
        match (ctx.output(|o| o.ime), focused) {
            (Some(ime), Some(id)) => {
                self.last_ime = Some((id, ime));
                if shrank {
                    self.scroll_to_field = Some(ime.rect);
                }
            }
            (None, Some(id)) if self.last_ime.is_some_and(|(last, _)| last == id) => {
                let ime = self.last_ime.unwrap().1;
                ctx.output_mut(|o| o.ime = Some(ime));
                let typed = ctx.input(|i| {
                    i.events
                        .iter()
                        .any(|e| matches!(e, egui::Event::Text(_) | egui::Event::Ime(_)))
                });
                if shrank || typed {
                    self.scroll_to_field = Some(ime.rect);
                }
            }
            _ => self.last_ime = None,
        }
        if self.scroll_to_field.is_some() {
            ctx.request_repaint();
        }
    }

    /// Ask for the next playback frame, no sooner than the fps limit
    /// allows. Frames drawn early (input) do not move the schedule.
    fn schedule_next_frame(&mut self, ctx: &egui::Context) {
        let Some(fps) = self.fps_cap.fps() else {
            ctx.request_repaint();
            return;
        };
        let period = 1.0 / fps;
        // Wake a little early: the picture is drawn at the next screen
        // refresh after the wake-up anyway.
        const SLACK: f64 = 0.003;
        if self.now >= self.next_frame - SLACK {
            self.next_frame += period;
            if self.next_frame < self.now {
                // Fell behind (slow frame, or playback just resumed).
                self.next_frame = self.now + period;
            }
        }
        let wait = (self.next_frame - self.now - SLACK).max(0.0);
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
    }

    fn save_user_preset(&mut self, name: String) {
        let mut p = self.project.clone();
        p.name = name;
        self.capture(p, 320, 180, Capture::PresetThumb);
    }

    fn open_dialog(&mut self) {
        platform::pick(Purpose::OpenProject);
    }

    fn save(&mut self, save_as: bool) {
        // In the browser a project is saved as a pack download: in-memory
        // assets have no file path a plain project could point to.
        if platform::IS_WEB {
            self.save_pack();
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
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
        let _ = save_as;
    }

    fn set_audio(&mut self, path: Option<String>) {
        self.project.audio = path;
        self.reload_audio();
    }

    fn music_key(&self) -> (Option<String>, Option<String>, u32) {
        (
            self.project.audio.clone(),
            self.project.music.midi.clone(),
            self.project.music.midi_offset.to_bits(),
        )
    }

    fn music_loaded(&mut self, res: anyhow::Result<ez_export::LoadedMusic>) {
        match res {
            Ok(m) => {
                if let (Some(path), Some(audio)) = (&self.project.audio, m.audio) {
                    self.audio_cache = Some((path.clone(), audio));
                }
                self.audio_env = m.music.map(std::sync::Arc::new);
            }
            Err(e) => self.set_status(format!("Music: {e:#}"), true),
        }
    }

    fn reload_audio(&mut self) {
        let audio_changed = self.music_key.0 != self.project.audio;
        self.music_key = self.music_key();
        if audio_changed {
            self.audio = None;
        }
        self.audio_env = None;
        self.music_task = None;
        let cached = self
            .audio_cache
            .as_ref()
            .filter(|(p, _)| Some(p) == self.project.audio.as_ref())
            .map(|(_, env)| env.clone());
        match ez_export::MusicJob::new(&self.project, cached) {
            Ok(job) => match crate::music_task::MusicTask::start(job) {
                crate::music_task::Started::Done(res) => self.music_loaded(res),
                crate::music_task::Started::Working(t) => self.music_task = Some(t),
            },
            Err(e) => {
                self.set_status(format!("Music: {e:#}"), true);
                return;
            }
        }
        let Some(path) = self.project.audio.clone() else {
            self.audio = None;
            return;
        };
        if self.audio.is_some() {
            return;
        }
        match AudioPlayer::new(&path) {
            Ok(a) => self.audio = Some(a),
            Err(e) => self.set_status(
                format!("Music loaded for sync, but no playback: {e:#}"),
                true,
            ),
        }
    }

    /// Keep the music playing where the loop is. The music loops its region
    /// (the loop window, or the whole song) by itself, seamlessly, and leads
    /// the clock while it plays: the picture follows it (so slow frames
    /// can't hold the music back and nothing jumps at the loop point). It
    /// only jumps when the clock is moved by hand.
    fn sync_music(&mut self) {
        let Some(a) = &mut self.audio else {
            return;
        };
        let (start, end) = match self.project.music.mode {
            MusicMode::LoopWindow => {
                let start = self.project.music.offset.max(0.0) as f64;
                (start, start + self.project.timing.loop_seconds() as f64)
            }
            MusicMode::FullTrack => (0.0, a.duration()),
        };
        let target = self.project.song_seconds(self.time) as f64;
        let playing = self.playing && self.live.is_none();
        let seek = std::mem::take(&mut self.music_seek);
        let Some(heard) = a.sync(playing, target, start, end, seek) else {
            return;
        };
        // Where the clock should be for what is playing, and the short way
        // there round the loop.
        let period = self.play_seconds().max(0.01);
        let want = match self.project.music.mode {
            MusicMode::LoopWindow => heard - start,
            MusicMode::FullTrack => heard,
        };
        self.time = follow_clock(self.time, want, period);
    }

    /// Mutable access to the layer an import was meant for.
    fn layer_for(&mut self, lref: LayerRef) -> Option<&mut Layer> {
        match lref {
            LayerRef::Layer(i) => self.project.layers.get_mut(i),
            LayerRef::Node(id) => self.nodes.as_mut().and_then(|n| n.layer_mut(id)),
        }
    }

    /// Put the texture `name` in a texture slot of a layer.
    fn set_layer_texture(&mut self, lref: LayerRef, slot: platform::TexSlot, name: String) {
        let Some(layer) = self.layer_for(lref) else {
            return;
        };
        let name = Some(name);
        match (&mut layer.kind, slot) {
            (LayerKind::Mesh(m), platform::TexSlot::Relief) => m.material.relief.texture = name,
            (LayerKind::Mesh(m), platform::TexSlot::Orm) => m.material.pbr.orm_map = name,
            (LayerKind::Mesh(m), platform::TexSlot::Emissive) => m.material.pbr.emissive_map = name,
            (LayerKind::Mesh(m), _) => m.material.texture = name,
            (LayerKind::Backdrop(b), _) => b.texture = name,
            (LayerKind::Mirror(f), _) => f.texture = name,
            (LayerKind::Terrain(t), _) => t.texture = name,
            (LayerKind::Mode7(f), _) => f.texture = name,
            (LayerKind::Sprite(sp), _) => sp.image = name,
            (LayerKind::Logo(g), platform::TexSlot::Matcap) => g.matcap = name,
            (LayerKind::Logo(g), platform::TexSlot::MorphImage) => g.morph_image = name,
            (LayerKind::Logo(g), _) => g.image = name,
            _ => {}
        }
    }

    /// Give a model layer the physical material of its glTF file: factors,
    /// and its pictures added as images. Returns a note for the status
    /// line ("" when the file has no material).
    fn apply_model_material(&mut self, lref: LayerRef, path: &str, name: &str) -> String {
        let mm = match ez_render::import::load_model_material(path) {
            Ok(Some(mm)) => mm,
            Ok(None) => return String::new(),
            Err(e) => {
                log::warn!("reading the material of {name}: {e:#}");
                return String::new();
            }
        };
        let stem = name
            .rsplit_once('.')
            .map(|(s, _)| s)
            .unwrap_or(name)
            .to_string();
        let mut picture = |img: &Option<image::RgbaImage>, kind: &str| -> Option<String> {
            let file = format!("{stem}_{kind}.png");
            let path = self.store_picture(img.as_ref()?, &file)?;
            Some(inspector::add_user_texture(
                &mut self.project.textures,
                &path,
                &file,
            ))
        };
        let color = picture(&mm.color_map, "colour");
        let orm = picture(&mm.orm_map, "orm");
        let glow = picture(&mm.emissive_map, "glow");
        let Some(LayerKind::Mesh(m)) = self.layer_for(lref).map(|l| &mut l.kind) else {
            return String::new();
        };
        let mat = &mut m.material;
        mat.base_color = mm.base_color;
        mat.metallic = Param::new(mm.metallic);
        mat.roughness = Param::new(mm.roughness);
        mat.rim = Param::new(0.0);
        mat.texture = color;
        mat.texture_scale = Param::new(1.0);
        let e = mm.emissive;
        let strength = e[0].max(e[1]).max(e[2]);
        if strength > 0.0 {
            mat.emissive_color = [e[0] / strength, e[1] / strength, e[2] / strength];
            mat.emissive = Param::new(strength);
            mat.emissive_mode = EmissiveMode::Full;
        }
        mat.pbr = Pbr {
            shading: Shading::Physical,
            orm_map: orm,
            emissive_map: glow,
            transmission: Param::new(mm.transmission),
            ior: mm.ior,
            ..Default::default()
        };
        " with its material".into()
    }

    /// Keep a picture made by the app (taken out of a model) as a PNG
    /// asset: a file in the app's data folder, or in memory on the web.
    fn store_picture(&self, img: &image::RgbaImage, file: &str) -> Option<String> {
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .ok()?;
        self.store_png(png, file)
    }

    /// Keep PNG bytes as an asset (see [`Self::store_picture`]).
    fn store_png(&self, png: Vec<u8>, file: &str) -> Option<String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let dir = self.library.imported_dir();
            std::fs::create_dir_all(&dir).ok()?;
            let mut path = dir.join(file);
            let mut k = 2;
            while path.exists() {
                path = dir.join(format!("{k}_{file}"));
                k += 1;
            }
            std::fs::write(&path, png).ok()?;
            Some(path.to_string_lossy().to_string())
        }
        #[cfg(target_arch = "wasm32")]
        {
            Some(ez_core::store::insert_new(file, png))
        }
    }

    /// Apply files picked in a dialog or dropped on the window.
    fn handle_picked(&mut self, ctx: &egui::Context) {
        for p in platform::take_picked() {
            let ext = ez_core::store::extension(&p.path);
            let purpose = match p.purpose {
                Purpose::Dropped => {
                    if ext == "json" || ext == ez_core::assets::PACK_EXTENSION {
                        Purpose::OpenProject
                    } else if MODEL_EXTENSIONS.contains(&ext.as_str()) {
                        Purpose::AddModelLayer
                    } else if IMAGE_EXTENSIONS.contains(&ext.as_str())
                        || crate::clip_import::is_animation(&ext)
                    {
                        match self.selection {
                            Selection::Layer(i) if self.mode == Mode::Simple => {
                                Purpose::SetTexture(LayerRef::Layer(i), platform::TexSlot::Material)
                            }
                            _ => Purpose::AddImages,
                        }
                    } else if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
                        Purpose::LoadMusic
                    } else if MIDI_EXTENSIONS.contains(&ext.as_str()) {
                        Purpose::LoadMidi
                    } else if ext == "hdr" {
                        Purpose::SetEnvMap
                    } else {
                        self.set_status(format!("Don't know what to do with {}", p.name), true);
                        continue;
                    }
                }
                other => other,
            };
            // GIFs and videos become frame sheets in the background (see
            // handle_clips).
            if matches!(purpose, Purpose::AddImages | Purpose::SetTexture(..))
                && crate::clip_import::is_animation(&ext)
            {
                self.set_status(format!("Reading the frames of {}…", p.name), false);
                crate::clip_import::start(
                    platform::Picked {
                        purpose,
                        ..p.clone()
                    },
                    ctx,
                );
                continue;
            }
            match purpose {
                Purpose::OpenProject => self.open_asset(&p.path, &p.name),
                Purpose::AddModelLayer => {
                    self.project.layers.push(inspector::model_layer(&p.path));
                    let i = self.project.layers.len() - 1;
                    self.selection = Selection::Layer(i);
                    let with = self.apply_model_material(LayerRef::Layer(i), &p.path, &p.name);
                    self.set_status(format!("Added model {}{with}", p.name), false);
                }
                Purpose::SetModel(lref) => {
                    if let Some(LayerKind::Mesh(m)) = self.layer_for(lref).map(|l| &mut l.kind) {
                        m.source = MeshSource::File {
                            path: p.path.clone(),
                        };
                    }
                    let with = self.apply_model_material(lref, &p.path, &p.name);
                    if !with.is_empty() {
                        self.set_status(format!("Loaded {}{with}", p.name), false);
                    }
                }
                Purpose::SetMorphModel(lref) => {
                    if let Some(LayerKind::Mesh(m)) = self.layer_for(lref).map(|l| &mut l.kind) {
                        m.morph.target = MeshSource::File {
                            path: p.path.clone(),
                        };
                    }
                }
                Purpose::AddImages => {
                    let name =
                        inspector::add_user_texture(&mut self.project.textures, &p.path, &p.name);
                    self.set_status(format!("Added image '{name}'"), false);
                }
                Purpose::SetTexture(lref, slot) => {
                    let name =
                        inspector::add_user_texture(&mut self.project.textures, &p.path, &p.name);
                    self.set_layer_texture(lref, slot, name.clone());
                    self.set_status(format!("Added image '{name}'"), false);
                }
                Purpose::LoadMusic => self.set_audio(Some(p.path.clone())),
                Purpose::SetEnvMap => {
                    let light = &mut self.project.environment.env_light;
                    light.source = EnvSource::Hdri(p.path.clone());
                    self.set_status(format!("{} lights the scene", p.name), false);
                }
                Purpose::LoadMidi => {
                    self.project.music.midi = Some(p.path.clone());
                    self.reload_audio();
                    self.set_status(
                        format!("MIDI notes from {} drive the hits and pitch", p.name),
                        false,
                    );
                }
                Purpose::SetFont(lref) => {
                    match self.layer_for(lref).map(|l| &mut l.kind) {
                        Some(LayerKind::Text(t)) => t.font_file = Some(p.path.clone()),
                        Some(LayerKind::Logo(g)) => g.font_file = Some(p.path.clone()),
                        Some(LayerKind::Mesh(MeshLayer {
                            source: MeshSource::Text { font_file, .. },
                            ..
                        })) => *font_file = Some(p.path.clone()),
                        _ => {}
                    }
                    self.set_status(format!("Font {}", p.name), false);
                }
                Purpose::Dropped => {}
            }
        }
    }

    /// Apply GIFs and videos whose frames are ready: their frame sheet
    /// becomes an image of the project (and goes where it was picked for).
    fn handle_clips(&mut self) {
        for done in crate::clip_import::take_done() {
            let p = done.picked;
            let (name, clip) = match done.result {
                Err(e) => {
                    self.set_status(format!("Could not read {}: {e}", p.name), true);
                    continue;
                }
                // A GIF that doesn't move: the file itself.
                Ok(None) => (
                    inspector::add_user_texture(&mut self.project.textures, &p.path, &p.name),
                    None,
                ),
                Ok(Some((png, clip))) => {
                    let stem = p.name.rsplit_once('.').map_or(&p.name[..], |(s, _)| s);
                    let Some(path) = self.store_png(png, &format!("{stem}.png")) else {
                        self.set_status(format!("Could not keep the frames of {}", p.name), true);
                        continue;
                    };
                    crate::library::persist_asset(&path);
                    let name = inspector::add_user_clip(
                        &mut self.project.textures,
                        &path,
                        &p.name,
                        Some(clip.clone()),
                    );
                    (name, Some(clip))
                }
            };
            if let Purpose::SetTexture(lref, slot) = p.purpose {
                self.set_layer_texture(lref, slot, name.clone());
                let loop_s = self.project.timing.loop_seconds();
                if let (Some(c), Some(LayerKind::Sprite(sp))) =
                    (&clip, self.layer_for(lref).map(|l| &mut l.kind))
                {
                    inspector::fit_sprite_to_clip(sp, c, loop_s);
                }
            }
            let what = match &clip {
                Some(c) => format!(
                    "Added animation '{name}' ({} frames, {:.1} s)",
                    c.frames, c.seconds
                ),
                None => format!("Added image '{name}'"),
            };
            self.set_status(what, false);
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

    fn file_menu(&mut self, ui: &mut Ui) {
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
        if !platform::IS_WEB && ui.button("Save as…").clicked() {
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
            platform::pick(Purpose::AddModelLayer);
            ui.close();
        }
        if ui.button("Import image…").clicked() {
            platform::pick(Purpose::AddImages);
            ui.close();
        }
        if ui.button("Load music…").clicked() {
            platform::pick(Purpose::LoadMusic);
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
        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.separator();
            if ui.button("Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn edit_menu(&mut self, ui: &mut Ui) {
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
    }

    fn randomize_now(&mut self) {
        self.rand_seed = self.rand_seed.wrapping_add(1);
        randomize(&mut self.project, self.rand_seed, self.rand_opts);
    }

    fn top_bar(&mut self, ui: &mut Ui, narrow: bool) {
        if narrow {
            // Phone layout: one menu button, play, export.
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(RichText::new("☰").size(20.0), |ui| {
                    ui.menu_button("File", |ui| self.file_menu(ui));
                    ui.menu_button("Edit", |ui| self.edit_menu(ui));
                    if ui.button("Presets").clicked() {
                        self.presets_open = true;
                        ui.close();
                    }
                    if ui.button("🎲 Randomize").clicked() {
                        self.randomize_now();
                        ui.close();
                    }
                    if ui.button("⚙ Randomizer settings").clicked() {
                        self.randomize_open = true;
                        ui.close();
                    }
                    if ui.button("✨ Surprise me").clicked() {
                        self.surprise();
                        ui.close();
                    }
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.mode, Mode::Simple, "Simple");
                        ui.selectable_value(&mut self.mode, Mode::Nodes, "Nodes");
                    });
                    ui.separator();
                    if ui.button("🖥 Graphics").clicked() {
                        self.graphics_open = true;
                        ui.close();
                    }
                    if ui.button("? Help").clicked() {
                        self.help_open = true;
                        ui.close();
                    }
                });
                ui.label(RichText::new("EZ2").strong().color(ACCENT));
                let icon = if self.playing { "⏸" } else { "▶" };
                if ui.button(RichText::new(icon).size(18.0)).clicked() {
                    self.playing = !self.playing;
                }
                if ui.button("🎲").clicked() {
                    self.randomize_now();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(RichText::new("⏺ Export").color(ACCENT).strong())
                        .clicked()
                    {
                        self.export.open = true;
                    }
                });
            });
            return;
        }
        egui::MenuBar::new().ui(ui, |ui| {
            ui.label(RichText::new("EZ2DEMOSCENE").strong().color(ACCENT));
            ui.separator();
            ui.menu_button("File", |ui| self.file_menu(ui));
            ui.menu_button("Edit", |ui| self.edit_menu(ui));
            if ui.button("Presets").clicked() {
                self.presets_open = true;
            }
            if ui
                .button("🎲 Randomize")
                .on_hover_text("Shuffle colours, shapes and motion")
                .clicked()
            {
                self.randomize_now();
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
            if ui.button("🖥").on_hover_text("Graphics").clicked() {
                self.graphics_open = true;
            }
            if ui.button("?").on_hover_text("Help").clicked() {
                self.help_open = true;
            }
        });
    }

    fn graph_panel(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.strong("Node graph");
            ui.checkbox(&mut self.project.use_graph, "render the graph");
            if ui
                .button("Rebuild from layers")
                .on_hover_text("Replace the graph with one node per layer")
                .clicked()
            {
                self.nodes = Some(NodeEditor::from_graph(&Graph::from_layers(
                    &self.project.layers,
                )));
            }
            if ui
                .button("Bake to layers")
                .on_hover_text(
                    "Turn the graph result into plain layers and go back to Simple mode \
                     (Drive nodes are dropped: settings keep their own values)",
                )
                .clicked()
            {
                if let Some(g) = &self.project.graph {
                    self.project.layers = g.compile();
                }
                self.project.use_graph = false;
                self.mode = Mode::Simple;
            }
        });
        let templates = self.library.template_layers();
        let project = &self.project;
        let audio = self.audio_env.clone();
        let ctx_at = |phase: f32| project.ctx(phase, audio.as_deref());
        let now = self
            .project
            .ctx_at(self.time, self.audio_env.as_deref())
            .phase;
        if let Some(n) = &mut self.nodes {
            n.show(ui, &templates, &ctx_at, now);
        }
    }

    fn tab_bar(&mut self, ui: &mut Ui) {
        let layers_label = if self.mode == Mode::Nodes {
            "🕸\nGraph"
        } else {
            "☰\nLayers"
        };
        ui.columns(3, |cols| {
            for (col, (tab, label)) in cols.iter_mut().zip([
                (Tab::View, "🎬\nView"),
                (Tab::Layers, layers_label),
                (Tab::Edit, "✏\nEdit"),
            ]) {
                let selected = self.tab == tab;
                let text = RichText::new(label).size(15.0);
                let text = if selected {
                    text.color(ACCENT).strong()
                } else {
                    text
                };
                let r = col.add_sized(
                    [col.available_width(), 46.0],
                    egui::Button::selectable(selected, text),
                );
                if r.clicked() {
                    self.tab = tab;
                }
            }
        });
    }

    fn surprise(&mut self) {
        self.rand_seed = self
            .rand_seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        // Any preset but the last (Empty).
        let pick = (self.rand_seed >> 33) as usize % (presets::INDEX.len() - 1);
        let mut p = presets::INDEX[pick].project();
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
        let (w, h) = self.export.still_size();
        self.capture(self.project.clone(), w, h, Capture::Still);
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
            (Selection::Colors, "🎨  Colour scheme"),
            (Selection::Post, "🎞  Post effects"),
            (Selection::Retro, "🕹  Retro 3D"),
            (Selection::Textures, "🖼  Your images"),
            (Selection::Sequence, "🎬  Scenes & timeline"),
        ];
        for (sel, label) in items {
            if ui.selectable_label(self.selection == sel, label).clicked() {
                self.selection = sel;
                if self.narrow {
                    self.tab = Tab::Edit;
                }
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
                            if self.narrow {
                                self.tab = Tab::Edit;
                            }
                        }
                        if !self.project.use_graph {
                            if let Some(st) = self.viewport.renderer.stats().layers.iter().find(|s| s.index == i) {
                                if st.load > HEAVY_LAYER {
                                    ui.label(RichText::new("⚠").color(Color32::from_rgb(255, 170, 60)))
                                        .on_hover_text(format!(
                                            "Heavy layer: {} triangles, {} particles. Lower the count, detail or trail if playback stutters.",
                                            human(st.triangles),
                                            human(st.particles)
                                        ));
                                }
                            }
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
                2 => self.duplicate_layer(i),
                3 => self.delete_layer(i),
                _ => {}
            }
        }
    }

    fn duplicate_layer(&mut self, i: usize) {
        if let Some(l) = self.project.layers.get(i) {
            let mut c = l.clone();
            c.name.push_str(" copy");
            self.project.layers.insert(i + 1, c);
            self.selection = Selection::Layer(i + 1);
        }
    }

    fn delete_layer(&mut self, i: usize) {
        let layers = &mut self.project.layers;
        if i >= layers.len() {
            return;
        }
        layers.remove(i);
        self.selection = if layers.is_empty() {
            Selection::Camera
        } else {
            Selection::Layer(i.min(layers.len() - 1))
        };
    }

    /// Put a copy of layer `i` on the clipboard (as text, so it can be
    /// pasted into another project or another window of the app).
    fn copy_layer(&mut self, ctx: &egui::Context, i: usize) {
        let Some(l) = self.project.layers.get(i).cloned() else {
            return;
        };
        if let Ok(json) = serde_json::to_string(&l) {
            ctx.copy_text(format!("{LAYER_CLIPBOARD_TAG}\n{json}"));
        }
        self.set_status(format!("Copied '{}'", l.name), false);
        self.layer_clipboard = Some(l);
    }

    /// Paste a copied layer after the selected one (or at the end).
    /// `text` is the system clipboard, when there is one.
    fn paste_layer(&mut self, text: Option<&str>) {
        let from_text = text
            .and_then(|t| t.strip_prefix(LAYER_CLIPBOARD_TAG))
            .and_then(|json| serde_json::from_str::<Layer>(json.trim()).ok());
        let Some(layer) = from_text.or_else(|| self.layer_clipboard.clone()) else {
            return;
        };
        let at = match self.selection {
            Selection::Layer(i) if i < self.project.layers.len() => i + 1,
            _ => self.project.layers.len(),
        };
        self.set_status(format!("Pasted '{}'", layer.name), false);
        self.project.layers.insert(at, layer);
        self.selection = Selection::Layer(at);
    }

    fn inspector_panel(&mut self, ui: &mut Ui) {
        // Terrain layers that copies can stand on.
        let terrains: Vec<String> = self
            .project
            .layers
            .iter()
            .filter(|l| matches!(l.kind, LayerKind::Terrain(_)))
            .map(|l| l.name.clone())
            .collect();
        ui.data_mut(|d| d.insert_temp(egui::Id::new(inspector::TERRAIN_NAMES), terrains));
        // How the selected layer's simulation bake is doing.
        let sim = match self.selection {
            Selection::Layer(i) => self
                .project
                .layers
                .get(i)
                .and_then(|l| self.viewport.renderer.sim_status(&l.name)),
            _ => None,
        };
        ui.data_mut(|d| d.insert_temp(egui::Id::new(inspector::SIM_STATUS), sim));
        // Shape and sprite layers that arcs can reach for.
        let copy_layers: Vec<String> = self
            .project
            .layers
            .iter()
            .filter(|l| matches!(l.kind, LayerKind::Mesh(_) | LayerKind::Sprite(_)))
            .map(|l| l.name.clone())
            .collect();
        ui.data_mut(|d| d.insert_temp(egui::Id::new(inspector::COPY_LAYER_NAMES), copy_layers));
        let logos: Vec<String> = self
            .project
            .layers
            .iter()
            .filter(|l| matches!(l.kind, LayerKind::Logo(_)))
            .map(|l| l.name.clone())
            .collect();
        ui.data_mut(|d| d.insert_temp(egui::Id::new(inspector::LOGO_NAMES), logos));
        // 3D layers a logo can be attached to (follow on the screen).
        let shapes: Vec<String> = self
            .project
            .layers
            .iter()
            .filter(|l| {
                !matches!(
                    l.kind,
                    LayerKind::Logo(_)
                        | LayerKind::Backdrop(_)
                        | LayerKind::Mirror(_)
                        | LayerKind::Mode7(_)
                )
            })
            .map(|l| l.name.clone())
            .collect();
        ui.data_mut(|d| d.insert_temp(egui::Id::new(inspector::SHAPE_NAMES), shapes));
        let scheme_on = self.project.color_scheme.enabled;
        ui.data_mut(|d| d.insert_temp(egui::Id::new(inspector::SCHEME_ON), scheme_on));
        let loop_s = self.project.timing.loop_seconds();
        ui.data_mut(|d| d.insert_temp(egui::Id::new(inspector::LOOP_SECONDS), loop_s));
        let ctx = self.project.ctx_at(self.time, self.audio_env.as_deref());
        let view = self.project.camera.view_point(&ctx);
        ui.data_mut(|d| d.insert_temp(egui::Id::new(inspector::CAMERA_VIEW), view));
        let scroll_to_field = self.scroll_to_field.take();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if let Some(r) = scroll_to_field {
                ui.scroll_to_rect(r, Some(egui::Align::Center));
            }
            ui.add_space(4.0);
            if self.mode == Mode::Nodes {
                let textures = &mut self.project.textures;
                match self.nodes.as_mut().and_then(|n| n.selected_layer_mut()) {
                    Some((id, layer)) => inspector::layer_ui(ui, layer, textures, LayerRef::Node(id)),
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
                Selection::Colors => {
                    let ctx = self.project.ctx_at(self.time, self.audio_env.as_deref());
                    inspector::color_scheme_ui(ui, &mut self.project.color_scheme, &ctx);
                }
                Selection::Post => inspector::post_ui(ui, &mut self.project.post),
                Selection::Retro => {
                    let out = self.export.still_size();
                    inspector::retro_ui(ui, &mut self.project.retro, &mut self.project.post.palette, out)
                }
                Selection::Textures => inspector::textures_ui(ui, &mut self.project.textures),
                Selection::Sequence => {
                    let audio = self.audio_env.clone();
                    if inspector::sequence_ui(ui, &mut self.project, audio.as_deref()) {
                        // Another scene is now the one being edited.
                        self.selection = Selection::Sequence;
                        self.nodes = None;
                    }
                }
                Selection::Layer(i) => {
                    let Project { layers, textures, .. } = &mut self.project;
                    match layers.get_mut(i) {
                        Some(l) => inspector::layer_ui(ui, l, textures, LayerRef::Layer(i)),
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
                ui.horizontal(|ui| {
                    ui.label(RichText::new(ez_core::store::file_name(&p).to_string()).strong());
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Remove the music")
                        .clicked()
                    {
                        self.set_audio(None);
                    }
                });
                if let Some(a) = &mut self.audio {
                    ui.add(egui::Slider::new(&mut a.volume, 0.0..=1.0).text("volume"));
                }
            }
            None => {
                ui.label(RichText::new("Optional: drop an MP3/WAV/OGG/FLAC here or pick one. The loop plays with it, and any value can react to its kicks, bass, hits or melody.").weak());
                if ui.button("Load music…").clicked() {
                    platform::pick(Purpose::LoadMusic);
                }
            }
        }
        ui.horizontal(|ui| {
            match self.project.music.midi.clone() {
                Some(m) => {
                    ui.label(format!("MIDI: {}", ez_core::store::file_name(&m)));
                    if ui.small_button("🗑").on_hover_text("Remove the MIDI file").clicked() {
                        self.project.music.midi = None;
                    }
                }
                None => {
                    if ui
                        .button("Load MIDI notes…")
                        .on_hover_text("Exact hits (drums on channel 10) and melody from a .mid file, instead of guessing them from the audio")
                        .clicked()
                    {
                        platform::pick(Purpose::LoadMidi);
                    }
                }
            }
        });
        if self.project.music.midi.is_some() && self.project.audio.is_some() {
            widgets::slider(
                ui,
                "MIDI offset",
                "Shift the notes against the audio (seconds)",
                &mut self.project.music.midi_offset,
                -5.0..=5.0,
            );
        }
        let Some(env) = self.audio_env.clone() else {
            self.live_ui(ui);
            return;
        };
        let tempo = env.tempo;
        ui.label(
            RichText::new(match tempo {
                Some((bpm, _)) => format!("{:.1} s analysed · about {bpm:.1} BPM", env.duration),
                None => format!("{:.1} s analysed", env.duration),
            })
            .weak(),
        );
        ui.add_space(4.0);
        let m = &mut self.project.music;
        widgets::combo(
            ui,
            "Plays",
            "Loop a part of the song (seamless) or play the whole song (exports last as long as the song)",
            &mut m.mode,
            &ez_core::MusicMode::ALL,
            |m| m.label(),
        );
        let loop_s = self.project.timing.loop_seconds();
        if m.mode == ez_core::MusicMode::LoopWindow {
            let max = (env.duration - loop_s).max(0.0);
            widgets::slider(
                ui,
                "Starts at",
                "Where in the song the loop window begins (seconds)",
                &mut m.offset,
                0.0..=max.max(0.01),
            );
            ui.horizontal(|ui| {
                if let Some((bpm, downbeat)) = tempo {
                    if ui
                        .button("Use song tempo")
                        .on_hover_text(format!(
                            "Set the tempo to {bpm:.1} BPM and snap the window to a bar"
                        ))
                        .clicked()
                    {
                        self.project.timing.bpm = bpm;
                        let bar = 240.0 / bpm;
                        let m = &mut self.project.music;
                        m.offset =
                            (downbeat + ((m.offset - downbeat) / bar).round() * bar).max(0.0);
                    }
                    if ui
                        .button("Snap to bar")
                        .on_hover_text("Move the window start onto the nearest bar of the song")
                        .clicked()
                    {
                        let bar = 240.0 / self.project.timing.bpm.max(1.0);
                        let m = &mut self.project.music;
                        m.offset =
                            (downbeat + ((m.offset - downbeat) / bar).round() * bar).max(0.0);
                    }
                }
                if ui
                    .button("Fit whole song")
                    .on_hover_text(
                        "Set the BPM so the whole track is one loop of the current length",
                    )
                    .clicked()
                    && env.duration > 0.5
                {
                    self.project.timing.bpm = (self.project.timing.loop_beats as f32 * 60.0
                        / env.duration)
                        .clamp(20.0, 300.0);
                    self.project.music.offset = 0.0;
                }
            });
        } else {
            ui.label(
                RichText::new(format!(
                    "The preview and exports run through the whole song ({:.1} s); loop animations keep cycling underneath.",
                    env.duration
                ))
                .weak(),
            );
        }
        ui.add_space(6.0);
        // The music's curves, then the project's value channels.
        let settings = self.project.music.clone();
        let mut sources = ez_core::AudioSource::FOLLOW[..6].to_vec();
        sources.extend(ez_core::AudioSource::channels(&settings).0);
        let w = &mut self.project.music.warp;
        widgets::section(ui, "Time warp", true, |ui| {
            ui.label(RichText::new("Motion (spins, orbits, scrolling, LFOs) speeds up with the music, and the loop still ends where it started. Beat fades, strobes and blinks stay on the beat. Following a live channel, the clock speeds up as it plays (exports aren't warped).").weak().small());
            widgets::row(ui, "Follows", "", |ui| {
                egui::ComboBox::from_id_salt(ui.id().with("warp source"))
                    .selected_text(w.source.label_in(&settings))
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for src in &sources {
                            ui.selectable_value(&mut w.source, *src, src.label_in(&settings))
                                .on_hover_text(src.description());
                        }
                    });
            });
            widgets::slider(
                ui,
                "Amount",
                "0 = off; 2 = up to 3× as fast on full hits; negative slows down on hits",
                &mut w.amount,
                -0.9..=6.0,
            );
        });
        ui.add_space(6.0);
        self.channels_ui(ui);
        ui.add_space(6.0);
        let frame = self.eval_ctx().music;
        widgets::section(ui, "Meters", true, |ui| music_meters(ui, &frame));
        self.live_ui(ui);
    }

    /// The project's live channels: declare them, and poke them to see the
    /// preview react (a host program feeds them for real).
    fn channels_ui(&mut self, ui: &mut Ui) {
        let mut remove = None;
        let mut new_kind = None;
        let preview = &mut self.channels_preview;
        let channels = &mut self.project.music.channels;
        let open = !channels.is_empty();
        widgets::section(ui, "Live channels", open, |ui| {
            ui.label(
                RichText::new(
                    "Named values and hits another program feeds while the project plays \
                     (a controller, a game, OSC…). Link any value to one in its 🎵 row. \
                     The controls here only try them in the preview; exports see them silent.",
                )
                .weak()
                .small(),
            );
            for (i, d) in channels.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut d.name)
                            .desired_width(96.0)
                            .hint_text("name"),
                    )
                    .on_hover_text("The name the feeding program uses");
                    // Changed below, with the links to the channel.
                    let mut kind = d.kind;
                    egui::ComboBox::from_id_salt(("channel kind", i))
                        .selected_text(d.kind.label())
                        .width(56.0)
                        .show_ui(ui, |ui| {
                            for k in ez_core::ChannelKind::ALL {
                                ui.selectable_value(&mut kind, k, k.label());
                            }
                        });
                    if kind != d.kind {
                        new_kind = Some((i, kind));
                    }
                    match d.kind {
                        ez_core::ChannelKind::Value => {
                            let mut v = preview.value(i);
                            if ui
                                .add(egui::Slider::new(&mut v, 0.0..=1.0).show_value(false))
                                .on_hover_text("Try a value in the preview")
                                .changed()
                            {
                                preview.set(i, v);
                            }
                        }
                        ez_core::ChannelKind::Hit => {
                            if ui
                                .button("hit")
                                .on_hover_text("Try a hit in the preview")
                                .clicked()
                            {
                                preview.hit(i, 1.0);
                            }
                        }
                    }
                    if ui
                        .small_button("🗑")
                        .on_hover_text("Remove the channel (values linked to it stop following)")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            }
            let full = channels.len() >= ez_core::channels::MAX_CHANNELS;
            if ui
                .add_enabled(!full, egui::Button::new("+ Add channel"))
                .clicked()
            {
                channels.push(ez_core::ChannelDef {
                    name: format!("Channel {}", channels.len() + 1),
                    kind: ez_core::ChannelKind::Value,
                });
            }
        });
        if let Some((i, kind)) = new_kind {
            // Links to the channel turn with it (following ↔ each hit).
            ez_core::channels::set_kind(&mut self.project, i, kind);
        }
        if let Some(i) = remove {
            ez_core::channels::remove_channel(&mut self.project, i);
            // The preview's values were by position too.
            self.channels_preview.reset();
        }
    }

    fn live_ui(&mut self, ui: &mut Ui) {
        ui.add_space(6.0);
        let mut on = self.live.is_some();
        if ui
            .checkbox(&mut on, "Live input (microphone / line-in)")
            .on_hover_text("React to live sound in the preview, e.g. for VJ sets. Exports always use the music file, which is repeatable.")
            .changed()
        {
            if on {
                match crate::live::LiveInput::start() {
                    Ok(l) => {
                        self.set_status(format!("Listening to {}", l.name), false);
                        self.live = Some(l);
                    }
                    Err(e) => self.set_status(format!("Live input: {e:#}"), true),
                }
            } else {
                self.live = None;
            }
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(e) = self.live.as_ref().and_then(|l| l.error()) {
            ui.label(
                RichText::new(format!("⚠ {e}"))
                    .color(Color32::LIGHT_RED)
                    .small(),
            );
        }
        if self.live.is_some() {
            ui.label(
                RichText::new(
                    "Live: the preview reacts to what the microphone hears (music file paused).",
                )
                .weak()
                .small(),
            );
            if self.audio_env.is_none() {
                if let Some(f) = self.live_frame {
                    music_meters(ui, &f);
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
                self.music_seek = true;
            }
            // Scrubber with beat ticks.
            let reserve = if self.narrow { 110.0 } else { 330.0 };
            let width = ui.available_width() - reserve;
            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(width.max(100.0), 26.0),
                egui::Sense::click_and_drag(),
            );
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
            let play_s = self.play_seconds();
            let full = play_s > loop_s + 1e-6;
            // Beat ticks (bars only when showing the whole song).
            let beat_s = loop_s / beats as f64;
            let ticks = (play_s / beat_s).round().max(1.0) as u32;
            let step = if ticks > 64 { 4 } else { 1 };
            for b in (0..=ticks).step_by(step) {
                let x = rect.left() + rect.width() * (b as f64 * beat_s / play_s) as f32;
                let bar = b % 4 == 0;
                let loop_start = full && b % beats == 0;
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
                        if loop_start {
                            Color32::from_gray(170)
                        } else if bar {
                            Color32::from_gray(120)
                        } else {
                            Color32::from_gray(70)
                        },
                    ),
                );
            }
            if let Some(env) = &self.audio_env {
                let song_at = |f: f64| self.project.song_seconds(f * play_s);
                let n = rect.width() as usize;
                for i in 0..n {
                    let ts = song_at(i as f64 / n as f64);
                    let lv = env.value(ez_core::audio::Curve::Level, false, ts);
                    let bass = env.value(ez_core::audio::Curve::Kick, false, ts);
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
                // Detected (or MIDI) hits: kicks red, snares yellow.
                let (a, b) = (song_at(0.0), song_at(0.0) + play_s as f32);
                for (kind, col, y0) in [
                    (
                        ez_core::audio::HitKind::Kick,
                        Color32::from_rgb(255, 80, 80),
                        0.0,
                    ),
                    (
                        ez_core::audio::HitKind::Snare,
                        Color32::from_rgb(255, 220, 80),
                        0.3,
                    ),
                ] {
                    for &(t, strength) in &env.hits[kind as usize] {
                        if t < a || t >= b {
                            continue;
                        }
                        let x = rect.left() + rect.width() * ((t - a) as f64 / play_s) as f32;
                        let y = rect.top() + rect.height() * y0;
                        painter.line_segment(
                            [egui::pos2(x, y), egui::pos2(x, y + rect.height() * 0.28)],
                            egui::Stroke::new(1.0, col.gamma_multiply(0.4 + 0.6 * strength)),
                        );
                    }
                }
            }
            let ph = self.phase();
            let x = rect.left() + rect.width() * (self.time / play_s) as f32;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(2.0, ACCENT),
            );
            if let Some(pos) = resp.interact_pointer_pos() {
                let f = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 0.9999);
                self.time = f as f64 * play_s;
                self.music_seek = true;
            }
            let beat = ph * beats as f32;
            if self.narrow {
                // Tempo and length live in "Timing & music" on phones.
                ui.label(RichText::new(format!("{:>4.1}/{beats}", beat + 1.0)).monospace());
                return;
            }
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
        // Wrap rather than overflow: an overflowing row widens the ui, and
        // the picture below would then be sized past the screen edge (phones
        // in portrait).
        ui.horizontal_wrapped(|ui| {
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
            let cap = self.fps_cap;
            egui::ComboBox::from_id_salt("fps_cap")
                .selected_text(cap.label())
                .width(70.0)
                .show_ui(ui, |ui| {
                    for c in platform::FpsCap::ALL {
                        ui.selectable_value(&mut self.fps_cap, c, c.label());
                    }
                })
                .response
                .on_hover_text(
                    "Frame rate limit of the preview. 30 or 60 saves battery and heat; \
                     Display draws as fast as the screen refreshes. Export is not affected.",
                );
            if self.fps_cap != cap {
                self.fps_cap.save();
            }
            {
                let st = self.viewport.renderer.stats();
                let fps = 1000.0 / self.frame_ms.max(0.1);
                let color = if fps < 24.0 {
                    Color32::from_rgb(255, 170, 60)
                } else {
                    Color32::GRAY
                };
                ui.label(RichText::new(format!("{fps:.0} fps")).color(color).small())
                    .on_hover_ui(|ui| {
                        ui.label(format!("Frame time: {:.1} ms", self.frame_ms));
                        ui.label(format!("Triangles: {}", human(st.triangles)));
                        ui.label(format!("Particles: {}", human(st.particles)));
                        ui.label(format!("Draw calls: {}", st.draw_calls));
                        ui.label(format!(
                            "Mirror reflection pass: {}",
                            if st.reflection {
                                "yes (scene drawn twice)"
                            } else {
                                "no"
                            }
                        ));
                        ui.label(format!(
                            "Estimated load: {:.2} (above ~3 may stutter on a laptop GPU)",
                            st.load
                        ));
                        ui.separator();
                        let mut layers = st.layers.clone();
                        layers.sort_by(|a, b| b.load.partial_cmp(&a.load).unwrap());
                        for l in layers.iter().take(5) {
                            ui.label(format!("{:>5.2}  {}", l.load, l.name));
                        }
                    });
            }
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
            if self.project.sequence.is_active() {
                ui.separator();
                ui.selectable_value(&mut self.solo_scene, true, "This scene")
                    .on_hover_text("Preview only the scene you are editing, looping on its own");
                ui.selectable_value(&mut self.solo_scene, false, "🎬 Timeline")
                    .on_hover_text(
                        "Preview the whole sequence with its transitions (what gets exported)",
                    );
            }
            if let Some(task) = &self.music_task {
                ui.spinner();
                ui.label(format!("Analysing music… {:.0}%", task.progress() * 100.0))
                    .on_hover_text("Music-driven settings react once this finishes.");
            }
            if let Some(p) = self.viewport.renderer.bake_progress() {
                ui.spinner();
                ui.label(format!("Simulating… {:.0}%", p * 100.0))
                    .on_hover_text(
                        "Flocks are simulated ahead of time into a loop. Until this \
                     finishes they show their previous version (or nothing); \
                     exports wait for it.",
                    );
            }
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
        // Never size the picture beyond what is actually visible.
        let mut avail = ui.available_size();
        let visible = ui.clip_rect().intersect(ui.ctx().content_rect());
        avail.x = avail.x.min(visible.max.x - ui.cursor().min.x).max(1.0);
        avail.y = avail.y.min(visible.max.y - ui.cursor().min.y).max(1.0);
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
        // With a sequence, preview the whole timeline or only the scene
        // being edited.
        let seq_on = self.project.sequence.is_active();
        let solo = if seq_on && self.solo_scene {
            self.project.scene_view(self.project.sequence.scene_id)
        } else {
            None
        };
        let ctx = match &solo {
            Some(v) => {
                let c = v.ctx_at(self.time, self.audio_env.as_deref());
                match self.live_frame {
                    Some(f) => c.with_frame(f),
                    None => c,
                }
            }
            None => self.eval_ctx(),
        };
        // The gizmo belongs to the edited scene: hide it while the timeline
        // shows another one.
        let (gizmo_ok, ctx) = match (seq_on && solo.is_none())
            .then(|| self.project.sequence.frame_at(&ctx))
            .flatten()
        {
            Some(f) if f.scene == self.project.sequence.scene_id && f.from.is_none() => {
                (true, f.ctx)
            }
            Some(_) => (false, ctx),
            None => (true, ctx),
        };
        // Simulations: their music, and some baking (in the browser, which
        // has no threads, a slice of each frame).
        self.viewport.renderer.set_audio(self.audio_env.clone());
        self.viewport.renderer.poll_bakes(bake_budget());
        // While exporting, keep showing the last picture: the GPU time goes
        // to the export instead (this matters a lot on phones).
        let tex = match self.viewport.last_texture() {
            Some(t) if self.export.is_running() => t,
            _ => match &solo {
                Some(v) => self.viewport.render(v, &ctx, px),
                None => {
                    let global = self.eval_ctx();
                    self.viewport.render(&self.project, &global, px)
                }
            },
        };
        let resp = ui
            .allocate_ui_with_layout(
                avail,
                egui::Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    ui.add(
                        egui::Image::new(egui::load::SizedTexture::new(tex, size))
                            .sense(egui::Sense::click_and_drag())
                            .corner_radius(4.0),
                    )
                },
            )
            .inner;
        // Gizmo, picking and mouse camera control.
        let cam_state = self.project.camera.eval(&ctx);
        // The response covers the whole centred area; the picture itself is
        // `size`, centred in it.
        let image_rect = egui::Rect::from_center_size(resp.rect.center(), size);
        let mut proj = Projector::new(&cam_state, image_rect);
        // Logos are placed on the picture, some against each other.
        if self
            .project
            .layers
            .iter()
            .any(|l| matches!(l.kind, LayerKind::Logo(_)))
        {
            proj.logos = self.viewport.renderer.logo_anchors(
                &self.project,
                &ctx,
                [image_rect.width(), image_rect.height()],
            );
        }
        let painter = ui.painter_at(image_rect);
        if self.gizmo.grid {
            gizmo::draw_grid(&painter, &proj, 0.0);
        }
        let snapping = ui.input(|i| i.modifiers.command);
        let mut on_gizmo = false;
        if gizmo_ok && self.mode == Mode::Simple && !self.project.use_graph {
            if let Selection::Layer(i) = self.selection {
                if let Some(layer) = self.project.layers.get_mut(i) {
                    on_gizmo = self.gizmo.show(&painter, &resp, &proj, layer, i, snapping);
                }
            }
            if resp.clicked() && !on_gizmo {
                if let Some(pos) = resp
                    .interact_pointer_pos()
                    .filter(|p| image_rect.contains(*p))
                {
                    if let Some(i) = gizmo::pick(&self.project.layers, &ctx, &proj, pos) {
                        self.selection = Selection::Layer(i);
                    }
                }
            }
        }
        // The flight line of a camera path, while the camera is selected.
        let cam = &self.project.camera;
        if cam.mode == CameraMode::Path && self.selection == Selection::Camera {
            let line = cam.path.line(240);
            let stroke = egui::Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 190, 60, 200));
            for w in line.windows(2) {
                if let (Some(a), Some(b)) = (proj.to_screen(w[0]), proj.to_screen(w[1])) {
                    painter.line_segment([a, b], stroke);
                }
            }
            for (i, p) in cam.path.points.iter().enumerate() {
                if let Some(q) = proj.to_screen(glam::Vec3::from(p.eye)) {
                    painter.circle_filled(q, 5.0, Color32::from_rgb(255, 190, 60));
                    painter.text(
                        q + egui::vec2(7.0, -7.0),
                        egui::Align2::LEFT_BOTTOM,
                        format!("{}", i + 1),
                        egui::FontId::proportional(13.0),
                        Color32::WHITE,
                    );
                }
            }
        }
        // A path camera flies by itself: dragging doesn't steer it.
        let steerable = self.project.camera.mode != CameraMode::Path;
        let multi = ui.input(|i| i.multi_touch().is_some());
        if steerable && resp.dragged() && !on_gizmo && !self.gizmo.is_dragging() && !multi {
            let d = resp.drag_delta();
            let cam = &mut self.project.camera;
            cam.angle.base = (cam.angle.base - d.x * 0.4 + 540.0).rem_euclid(360.0) - 180.0;
            cam.height.base += d.y * 0.03;
        }
        // Two fingers: pinch to zoom, twist to turn, drag up/down for height.
        if let Some(mt) = ui.input(|i| i.multi_touch()).filter(|_| steerable) {
            if resp.rect.contains(mt.center_pos) {
                let cam = &mut self.project.camera;
                if mt.zoom_delta > 0.0 {
                    cam.distance.base = (cam.distance.base / mt.zoom_delta).clamp(0.3, 200.0);
                }
                cam.angle.base = (cam.angle.base - mt.rotation_delta.to_degrees() + 540.0)
                    .rem_euclid(360.0)
                    - 180.0;
                cam.height.base += mt.translation_delta.y * 0.03;
            }
        }
        if steerable && resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let cam = &mut self.project.camera;
                cam.distance.base = (cam.distance.base * (1.0 - scroll * 0.002)).clamp(0.3, 200.0);
            }
        }
        self.loading_overlay(ui, image_rect);
    }

    /// Right after a scene is loaded, cover the picture with a progress bar
    /// until its simulations are baked and its music analysed, so it
    /// appears whole instead of with parts missing. Call after rendering:
    /// the render asks for the bakes.
    fn loading_overlay(&mut self, ui: &mut Ui, rect: egui::Rect) {
        let Some(name) = &self.loading else {
            return;
        };
        if self.export.is_running() {
            return;
        }
        let bake = self.viewport.renderer.scene_bake_progress();
        let music = self.music_task.as_ref().map(|t| t.progress());
        let (progress, what) = match (bake, music) {
            (None, None) => {
                self.loading = None;
                return;
            }
            (Some(b), None) => (b, "Simulating ahead of time, so the loop is seamless"),
            (None, Some(m)) => (m, "Analysing the music"),
            (Some(b), Some(m)) => ((b + m) * 0.5, "Simulating and analysing the music"),
        };
        let title = if name.is_empty() {
            "Preparing the scene…".to_string()
        } else {
            format!("Preparing {name}…")
        };
        ui.painter_at(rect)
            .rect_filled(rect, 4.0, Color32::from_black_alpha(235));
        let w = (rect.width() * 0.6).clamp(120.0, 360.0);
        let inner = egui::Rect::from_center_size(rect.center(), egui::vec2(w, 100.0));
        let mut skip = false;
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::top_down(egui::Align::Center)),
            |ui| {
                ui.label(RichText::new(title).strong());
                ui.add(
                    egui::ProgressBar::new(progress)
                        .show_percentage()
                        .desired_width(w),
                );
                ui.label(RichText::new(what).small().weak());
                skip = ui
                    .small_button("Show now")
                    .on_hover_text("Show the scene now: its simulated parts appear when ready.")
                    .clicked();
            },
        );
        if skip {
            self.loading = None;
        }
        // Bakes finishing on their thread don't wake the app up.
        ui.ctx().request_repaint();
    }

    fn presets_window(&mut self, ctx: &egui::Context) {
        if !self.presets_open {
            return;
        }
        if self.thumbs.is_empty() {
            let mut images = crate::preset_thumbs::bundled();
            self.thumbs = presets::INDEX
                .iter()
                .map(|p| {
                    let file = crate::preset_thumbs::file_name(p.name);
                    Thumb {
                        name: p.name,
                        category: p.category,
                        description: p.description,
                        texture: images
                            .remove(&file)
                            .map(|img| ctx.load_texture(file, img, egui::TextureOptions::LINEAR)),
                    }
                })
                .collect();
        }
        let mut open = true;
        let mut chosen_builtin = None;
        let mut chosen_user = None;
        let mut delete_user = None;
        let mut delete_template = None;
        // Three columns on desktop; on phones, as many as fit the screen.
        let screen = ctx.content_rect().size();
        let win_w = (screen.x - 24.0).min(720.0);
        let cols = if win_w >= 680.0 {
            3
        } else if win_w >= 330.0 {
            2
        } else {
            1
        };
        let card_w = if cols == 3 {
            213.0
        } else {
            ((win_w - 24.0 - 10.0 * (cols as f32 - 1.0)) / cols as f32).floor()
        };
        let card = |ui: &mut Ui, tex: Option<egui::TextureId>, name: &str, tip: &str| -> bool {
            let mut clicked = false;
            ui.vertical(|ui| {
                let size = egui::vec2(card_w, card_w * 9.0 / 16.0);
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
                // The name is part of the card too (easier to hit on phones).
                if ui
                    .add(egui::Label::new(RichText::new(name).strong()).sense(egui::Sense::click()))
                    .clicked()
                {
                    clicked = true;
                }
            });
            clicked
        };
        let mut window = egui::Window::new("Start from a preset")
            .open(&mut open)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0));
        window = if cols == 3 {
            window.default_width(720.0)
        } else {
            window.fixed_size(egui::vec2(win_w - 12.0, screen.y - 140.0))
        };
        let list_height = if cols == 3 { 520.0 } else { screen.y - 290.0 };
        window.show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut self.gallery_tab, 0, "Built-in");
                    ui.selectable_value(&mut self.gallery_tab, 1, format!("My presets ({})", self.library.presets.len()));
                    ui.selectable_value(&mut self.gallery_tab, 2, format!("My layer templates ({})", self.library.templates.len()));
                });
                ui.separator();
                match self.gallery_tab {
                    0 => {
                        ui.label("Pick a starting point, then tweak layers on the left and values on the right. Everything loops automatically.");
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical().max_height(list_height).show(ui, |ui| {
                            // One grid per group (PlayStation, Saturn, N64
                            // and Quake looks under "Retro console").
                            for cat in presets::CATEGORIES {
                                ui.heading(cat);
                                egui::Grid::new(("presets", cat)).spacing([10.0, 10.0]).show(ui, |ui| {
                                    let group = self.thumbs.iter().enumerate().filter(|(_, t)| t.category == cat);
                                    for (k, (i, t)) in group.enumerate() {
                                        if card(ui, t.texture.as_ref().map(|t| t.id()), t.name, t.description) {
                                            chosen_builtin = Some(i);
                                        }
                                        if k % cols == cols - 1 {
                                            ui.end_row();
                                        }
                                    }
                                });
                                ui.add_space(8.0);
                            }
                        });
                    }
                    1 => {
                        if self.library.presets.is_empty() {
                            ui.label("No presets yet. Use File → Save as my preset… to add the current scene here.");
                        }
                        egui::ScrollArea::vertical().max_height(list_height).show(ui, |ui| {
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
                                    if i % cols == cols - 1 {
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
                ui.separator();
                let mut hide = !self.presets_on_startup;
                if ui
                    .checkbox(&mut hide, "Don't show on startup")
                    .on_hover_text("Open this window from Presets in the menu bar")
                    .changed()
                {
                    self.presets_on_startup = !hide;
                    platform::save_setting(PRESETS_ON_STARTUP, if hide { "no" } else { "yes" });
                }
            });
        // A chosen preset starts playing, to show how it moves.
        if let Some(i) = chosen_builtin {
            self.load_project(presets::INDEX[i].project(), None);
            self.presets_open = false;
            self.playing = true;
        }
        if let Some(i) = chosen_user {
            match self.library.load_preset(i) {
                // Loaded as a new, unsaved project so the preset isn't overwritten.
                Ok(p) => {
                    self.load_project(p, None);
                    self.presets_open = false;
                    self.playing = true;
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
                if platform::IS_WEB {
                    ui.label(format!("Unsaved changes to '{name}' from your last visit were found."));
                } else {
                    ui.label(format!("EZ2DEMOSCENE did not close properly last time. An autosave of '{name}' was found."));
                }
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
            self.save_user_preset(name.trim().to_string());
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

    /// Which GPU and graphics backend the app runs on, plus (web only) a
    /// choice between WebGL2 and WebGPU for devices whose driver misbehaves.
    fn graphics_window(&mut self, ctx: &egui::Context) {
        let mut open = self.graphics_open;
        let info = self.viewport.adapter_info();
        let backend = match info.backend {
            wgpu::Backend::BrowserWebGpu => "WebGPU".to_string(),
            wgpu::Backend::Gl if platform::IS_WEB => "WebGL2".to_string(),
            b => format!("{b:?}"),
        };
        let msaa = self.viewport.renderer.msaa();
        let lines = [
            ("Backend", backend),
            ("GPU", info.name.clone()),
            (
                "Driver",
                format!("{} {}", info.driver, info.driver_info)
                    .trim()
                    .to_string(),
            ),
            ("Type", format!("{:?}", info.device_type)),
            (
                "Anti-aliasing",
                if msaa > 1 {
                    format!("{msaa}× MSAA")
                } else {
                    "off".into()
                },
            ),
        ];
        let max_w = (ctx.content_rect().width() - 24.0).clamp(200.0, 420.0);
        egui::Window::new("Graphics")
            .open(&mut open)
            .default_width(max_w)
            .max_width(max_w)
            .show(ctx, |ui| {
                for (k, v) in &lines {
                    ui.horizontal_top(|ui| {
                        ui.add_sized([110.0, 18.0], egui::Label::new(RichText::new(*k).weak()));
                        ui.add(
                            egui::Label::new(if v.is_empty() { "unknown" } else { v.as_str() })
                                .wrap(),
                        );
                    });
                }
                if ui.button("📋 Copy").on_hover_text("Copy these details for a bug report").clicked() {
                    let text: Vec<String> = lines.iter().map(|(k, v)| format!("{k}: {v}")).collect();
                    ctx.copy_text(text.join("\n"));
                    self.set_status("Graphics details copied", false);
                }
                #[cfg(not(target_arch = "wasm32"))]
                {
                    use crate::gpu_choice::Backend as B;
                    ui.separator();
                    ui.label(RichText::new("Graphics backend").strong());
                    ui.label("If the app crashes, hangs or glitches, try another backend. Automatic moves on by itself when one fails to start.");
                    let before = self.native_backend;
                    ui.horizontal_wrapped(|ui| {
                        for b in std::iter::once(B::Auto).chain(B::available().iter().copied()) {
                            ui.selectable_value(&mut self.native_backend, b, b.label());
                        }
                    });
                    if self.native_backend != before {
                        crate::gpu_choice::save_pref(self.native_backend);
                    }
                    if self.native_backend != self.native_backend_started {
                        ui.label(RichText::new("Used from the next start").color(Color32::LIGHT_YELLOW));
                    }
                }
                if platform::IS_WEB {
                    ui.separator();
                    ui.label(RichText::new("Graphics backend").strong());
                    ui.label("If the picture glitches, try the other backend. The app restarts to switch.");
                    if platform::GpuBackendPref::auto_uses_webgl() {
                        ui.label(
                            RichText::new("Automatic uses WebGL2 on Android: WebGPU glitches with some phone GPUs.")
                                .weak(),
                        );
                    }
                    let before = self.backend_pref;
                    ui.horizontal(|ui| {
                        use platform::GpuBackendPref as P;
                        for p in [P::Auto, P::WebGl, P::WebGpu] {
                            ui.selectable_value(&mut self.backend_pref, p, p.label());
                        }
                    });
                    if self.backend_pref != before {
                        self.backend_pref.save();
                    }
                    if self.backend_pref != self.backend_started {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Restart to apply").color(Color32::LIGHT_YELLOW));
                            if ui.button("⟳ Restart now").clicked() {
                                platform::reload();
                            }
                        });
                    }
                }
            });
        self.graphics_open = open;
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
            ui.label("Space play/pause · Ctrl+S save · Ctrl+O open · Ctrl+E export · Ctrl+Z / Ctrl+Shift+Z undo/redo · with a layer selected: Ctrl+C copy, Ctrl+X cut, Ctrl+V paste, Ctrl+D duplicate, Delete remove · W/E/R move/rotate/scale, G grid · drag in the viewport to orbit, scroll to zoom · drop models, images, music or projects onto the window.");
        });
        self.help_open = open;
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let (space, save, open, export, undo, redo) = ctx.input(|i| {
            (
                !typing && i.key_pressed(egui::Key::Space),
                pressed(i, egui::Key::S, Some(true), None),
                pressed(i, egui::Key::O, Some(true), None),
                pressed(i, egui::Key::E, Some(true), None),
                pressed(i, egui::Key::Z, Some(true), Some(false)),
                pressed(i, egui::Key::Z, Some(true), Some(true))
                    || pressed(i, egui::Key::Y, Some(true), None),
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
        // Layers: copy, cut, paste, duplicate and delete. Copy and paste
        // arrive as clipboard events (the browser sends them that way too).
        if !typing && self.mode == Mode::Simple {
            let selected = match self.selection {
                Selection::Layer(i) if i < self.project.layers.len() => Some(i),
                _ => None,
            };
            let (copy, cut, paste, duplicate, delete) = ctx.input(|i| {
                let mut copy = false;
                let mut cut = false;
                let mut paste = None;
                for e in &i.events {
                    match e {
                        egui::Event::Copy => copy = true,
                        egui::Event::Cut => cut = true,
                        egui::Event::Paste(t) => paste = Some(t.clone()),
                        _ => {}
                    }
                }
                (
                    copy,
                    cut,
                    paste,
                    pressed(i, egui::Key::D, Some(true), None),
                    pressed(i, egui::Key::Delete, Some(false), None)
                        || pressed(i, egui::Key::Backspace, Some(false), None),
                )
            });
            if let Some(i) = selected {
                if copy || cut {
                    self.copy_layer(ctx, i);
                }
                if cut {
                    self.delete_layer(i);
                } else if duplicate {
                    self.duplicate_layer(i);
                } else if delete {
                    self.delete_layer(i);
                }
            }
            if let Some(text) = paste {
                self.paste_layer(Some(&text));
            }
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
        let raw_dt = ctx.input(|i| i.unstable_dt) * 1000.0;
        self.frame_ms += (raw_dt.clamp(0.0, 1000.0) - self.frame_ms) * 0.1;
        self.frames_drawn = self.frames_drawn.saturating_add(1);
        #[cfg(not(target_arch = "wasm32"))]
        {
            if self.frames_drawn == 1 {
                log::info!("first frame");
                if let Some(b) = crate::gpu_choice::failed_last_time() {
                    self.set_status(
                        format!(
                            "{} did not start last time: now using {}. Change it in Graphics.",
                            b.label(),
                            self.viewport.adapter_info().backend
                        ),
                        true,
                    );
                }
            }
            if self.frames_drawn < 10 {
                // Get there even when idle: the start isn't confirmed
                // (see below) until then.
                ctx.request_repaint();
            }
            if self.frames_drawn == 10 {
                log::info!(
                    "graphics started: {:?}",
                    self.viewport.adapter_info().backend
                );
                // Build the pipelines not used yet in the background. Only
                // then is the backend known to work: some drivers crash or
                // hang compiling shaders, which Automatic steps past.
                self.viewport
                    .renderer
                    .warm_up_in_background(crate::gpu_choice::started_ok);
            }
            if self.viewport.renderer.warm_up_step() {
                ctx.request_repaint();
            }
        }
        self.project.sync_sequence_length();
        if self.music_key != self.music_key() {
            self.reload_audio();
        }
        if let Some(task) = &mut self.music_task {
            if let Some(res) = task.poll() {
                self.music_task = None;
                self.music_loaded(res);
            } else {
                ctx.request_repaint();
            }
        }
        self.channels_preview.advance(dt as f32);
        // Live input: analyse.
        self.live_frame = self.live.as_mut().map(|live| live.frame(dt as f32));
        // A time warp following something live (the live input, a live
        // channel) speeds up the clock.
        let mut live = self.live_frame.unwrap_or_default();
        if !self.project.music.channels.is_empty() {
            live.channels = self.channels_preview.frame();
        }
        let speed = self
            .project
            .music
            .warp
            .live_speed(&live, self.audio_env.is_some()) as f64;
        if self.playing {
            self.time += dt * (speed - 1.0);
        }
        if self.playing {
            self.time = (self.time + dt).rem_euclid(self.play_seconds().max(0.01));
        }
        widgets::set_clock(&ctx, self.phase(), self.project.timing.loop_beats);
        widgets::set_music(
            &ctx,
            widgets::MusicPreview {
                env: self.audio_env.clone(),
                settings: self.project.music.clone(),
                timing: self.project.timing,
                live: self.live_frame,
            },
        );
        self.sync_music();

        let dropped: Vec<egui::DroppedFileHandle> = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            platform::handle_drop(f);
        }
        self.handle_picked(&ctx);
        self.handle_clips();
        let was_loaded = self.library.is_loaded();
        self.library.poll(&ctx);
        if !was_loaded && self.library.is_loaded() && self.library.recovery.is_some() {
            self.presets_open = false;
        }
        self.run_test_hook();
        self.finish_captures(&ctx);
        self.export.tick(&mut self.viewport.renderer);
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

        self.narrow = ctx.content_rect().width() < NARROW_WIDTH;
        let narrow = self.narrow;
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui, narrow));
        if narrow {
            // Phone layout: one panel at a time, chosen with a bottom tab bar.
            egui::Panel::bottom("tabs")
                .exact_size(52.0)
                .show(ui, |ui| self.tab_bar(ui));
            egui::Panel::bottom("timeline")
                .exact_size(40.0)
                .show(ui, |ui| {
                    ui.add_space(6.0);
                    self.timeline(ui)
                });
            egui::CentralPanel::default().show(ui, |ui| match (self.tab, self.mode) {
                (Tab::View, _) => self.viewport_ui(ui),
                (Tab::Layers, Mode::Simple) => self.scene_panel(ui),
                (Tab::Layers, Mode::Nodes) => self.graph_panel(ui),
                (Tab::Edit, _) => self.inspector_panel(ui),
            });
        } else {
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
            } else {
                egui::Panel::bottom("graph")
                    .resizable(true)
                    .min_size(220.0)
                    .default_size(380.0)
                    .show(ui, |ui| self.graph_panel(ui));
            }
            egui::CentralPanel::default().show(ui, |ui| self.viewport_ui(ui));
        }
        if self.mode == Mode::Nodes {
            if let Some(n) = &self.nodes {
                let g = n.to_graph();
                if self.project.graph.as_ref() != Some(&g) {
                    self.project.graph = Some(g);
                }
                // Choosing "edit in inspector" on a phone jumps to the Edit tab.
                let sel = n.selected.map(|id| id.0);
                if sel != self.last_node_selection {
                    self.last_node_selection = sel;
                    if sel.is_some() && self.narrow {
                        self.tab = Tab::Edit;
                    }
                }
            }
        }

        platform::fetch_libraries(&ctx);
        self.poll_shape_picker_request(&ctx);
        self.shape_picker_window(&ctx);
        self.poll_tex_picker_request(&ctx);
        self.tex_picker_window(&ctx);
        self.presets_window(&ctx);
        self.recovery_window(&ctx);
        self.preset_name_window(&ctx);
        self.autosave();
        self.randomize_window(&ctx);
        self.help_window(&ctx);
        self.graphics_window(&ctx);
        self.export.music_loading = self.music_task.is_some();
        self.export
            .show(&ctx, &self.project, self.audio_env.as_deref());

        let pointer_down = ctx.input(|i| i.pointer.any_down());
        self.commit_history(pointer_down);

        let dirty = if self.project != self.saved {
            "•"
        } else {
            ""
        };
        let title = format!("EZ2DEMOSCENE — {}{dirty}", self.project.name);
        if title != self.title {
            platform::set_title(&ctx, &title);
            self.title = title;
        }
        self.keep_keyboard(&ctx);
        if self.export.is_running() {
            ctx.request_repaint();
        } else if self.playing {
            self.schedule_next_frame(&ctx);
        }
    }
}

fn setup_style(ctx: &egui::Context) {
    // Always dark, whatever the OS/browser theme.
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::Dark);
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

/// Live bars for every music source, hit flashes and the spectrum.
fn music_meters(ui: &mut Ui, m: &ez_core::MusicFrame) {
    use ez_core::AudioSource;
    for src in AudioSource::FOLLOW {
        let c = src.curve().unwrap() as usize;
        ui.horizontal(|ui| {
            ui.add_sized(
                [120.0, 14.0],
                egui::Label::new(RichText::new(src.label()).small()),
            );
            ui.add(
                egui::ProgressBar::new(m.fast[c])
                    .desired_width(130.0)
                    .desired_height(8.0),
            );
        });
    }
    ui.horizontal(|ui| {
        for src in AudioSource::HITS {
            let h = m.hits[src.hit().unwrap() as usize];
            let k = (1.0 - h.since / 0.25).clamp(0.0, 1.0);
            let col = Color32::from_rgb(
                (60.0 + 195.0 * k) as u8,
                (60.0 + 60.0 * k) as u8,
                (70.0 + 30.0 * k) as u8,
            );
            let name = src.label().trim_start_matches("Each ").to_string();
            ui.label(RichText::new(format!("• {name}")).color(col).small());
        }
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(250.0, 36.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, ui.visuals().extreme_bg_color);
    let n = m.spectrum.len();
    let w = rect.width() / n as f32;
    for (i, v) in m.spectrum.iter().enumerate() {
        let h = v * (rect.height() - 4.0);
        let x = rect.left() + i as f32 * w;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x + 1.0, rect.bottom() - 2.0 - h),
                egui::pos2(x + w - 1.0, rect.bottom() - 2.0),
            ),
            1.0,
            ACCENT,
        );
    }
}

/// How long simulations may bake per frame: in the browser (no threads)
/// about 8 ms; elsewhere bakes run on threads and polling only collects
/// them.
fn bake_budget() -> impl FnMut() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let start = js_sys::Date::now();
        move || js_sys::Date::now() - start < 8.0
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        || false
    }
}

#[cfg(test)]
mod clock_tests {
    use super::follow_clock;

    #[test]
    fn follows_the_music_across_the_loop_point() {
        let period = 16.0 * 60.0 / 140.0;
        // The music has wrapped, the clock not yet: step forward over the
        // loop point, not back through the whole loop.
        let t = follow_clock(period - 0.01, 0.01, period);
        assert!(t > period - 0.01 || t < 0.01, "{t}");
        // And the other way round.
        let t = follow_clock(0.01, period - 0.01, period);
        assert!(t < 0.01 || t > period - 0.01, "{t}");
        // Converges.
        let mut t = 1.0;
        for _ in 0..60 {
            t = follow_clock(t, 1.05, period);
        }
        assert!((t - 1.05).abs() < 1e-4);
        // Far behind (slow frames): straight to the music.
        assert_eq!(follow_clock(1.0, 3.0, period), 3.0);
    }
}
