//! Web export dialog. Frames are rendered a few at a time inside the
//! browser's frame loop ([`ExportUi::tick`]) and go to a GIF / PNG-zip
//! encoder written in Rust, or to the browser's WebCodecs video encoder for
//! MP4 / WebM. The finished file is downloaded.

use crate::platform::slug;
use egui::{RichText, Ui};
use ez_core::{AudioEnvelope, Project};
use ez_export::{ExportJob, FrameSink, GifSink, JobState, PngZipSink};
use ez_render::Renderer;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen]
extern "C" {
    #[derive(Clone)]
    type Ez2VideoExporter;

    #[wasm_bindgen(static_method_of = Ez2VideoExporter, js_name = create)]
    fn create(format: &str, w: u32, h: u32, fps: f32) -> js_sys::Promise;

    #[wasm_bindgen(static_method_of = Ez2VideoExporter, js_name = available)]
    fn available() -> bool;

    #[wasm_bindgen(method, catch, js_name = addFrame)]
    fn add_frame(
        this: &Ez2VideoExporter,
        rgba: &js_sys::Uint8Array,
        index: u32,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(method, js_name = queueSize)]
    fn queue_size(this: &Ez2VideoExporter) -> u32;

    #[wasm_bindgen(method, js_name = codecName)]
    fn codec_name(this: &Ez2VideoExporter) -> String;

    #[wasm_bindgen(method)]
    fn finish(this: &Ez2VideoExporter) -> js_sys::Promise;
}

fn video_encoder_available() -> bool {
    // The helper script may be missing (e.g. blocked); treat as unavailable.
    js_sys::Reflect::has(&js_sys::global(), &"Ez2VideoExporter".into()).unwrap_or(false)
        && Ez2VideoExporter::available()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WebFormat {
    Mp4,
    WebM,
    Gif,
    PngZip,
}

impl WebFormat {
    const ALL: [WebFormat; 4] = [
        WebFormat::Mp4,
        WebFormat::WebM,
        WebFormat::Gif,
        WebFormat::PngZip,
    ];

    fn label(self) -> &'static str {
        match self {
            WebFormat::Mp4 => "MP4 video",
            WebFormat::WebM => "WebM video",
            WebFormat::Gif => "Animated GIF",
            WebFormat::PngZip => "PNG frames (.zip)",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            WebFormat::Mp4 => "mp4",
            WebFormat::WebM => "webm",
            WebFormat::Gif => "gif",
            WebFormat::PngZip => "zip",
        }
    }

    fn is_video(self) -> bool {
        matches!(self, WebFormat::Mp4 | WebFormat::WebM)
    }
}

/// Feeds frames to the JS video encoder.
struct VideoSink {
    enc: Ez2VideoExporter,
    index: u32,
}

impl FrameSink for VideoSink {
    fn add_frame(&mut self, rgba: &[u8], _w: u32, _h: u32) -> anyhow::Result<()> {
        let array = js_sys::Uint8Array::from(rgba);
        self.enc
            .add_frame(&array, self.index)
            .map_err(|e| anyhow::anyhow!("video encoder: {e:?}"))?;
        self.index += 1;
        Ok(())
    }

    fn finish(self: Box<Self>) -> anyhow::Result<Vec<u8>> {
        // The encoder is flushed asynchronously by ExportUi.
        Ok(Vec::new())
    }

    fn extension(&self) -> &'static str {
        "mp4"
    }
}

type Slot<T> = Rc<RefCell<Option<Result<T, String>>>>;

enum Running {
    /// Waiting for the browser to pick a video codec.
    Starting { slot: Slot<Ez2VideoExporter> },
    Frames {
        job: Box<ExportJob>,
        video: Option<Ez2VideoExporter>,
    },
    /// Waiting for the video encoder to flush.
    Finishing { slot: Slot<Vec<u8>> },
}

pub struct ExportUi {
    pub open: bool,
    format: WebFormat,
    width: u32,
    height: u32,
    fps: f32,
    repeats: u32,
    running: Option<Running>,
    request: Option<(Project, Option<AudioEnvelope>)>,
    file_name: String,
    status: Option<Result<String, String>>,
}

impl Default for ExportUi {
    fn default() -> Self {
        let video = video_encoder_available();
        ExportUi {
            open: false,
            format: if video {
                WebFormat::Mp4
            } else {
                WebFormat::Gif
            },
            width: if video { 1280 } else { 480 },
            height: if video { 720 } else { 270 },
            fps: if video { 30.0 } else { 25.0 },
            repeats: 1,
            running: None,
            request: None,
            file_name: String::new(),
            status: None,
        }
    }
}

impl ExportUi {
    /// Size used for "Save still image".
    pub fn still_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn is_running(&self) -> bool {
        self.running.is_some() || self.request.is_some()
    }

    pub fn show(&mut self, ctx: &egui::Context, project: &Project, audio: Option<&AudioEnvelope>) {
        let mut open = self.open;
        egui::Window::new("Export loop")
            .open(&mut open)
            .resizable(false)
            .default_width(360.0)
            .show(ctx, |ui| self.contents(ui, project, audio));
        self.open = open;
    }

    fn contents(&mut self, ui: &mut Ui, project: &Project, audio: Option<&AudioEnvelope>) {
        let (frames, looped) = ez_export::export_frames(project, audio, self.fps);
        ui.label(
            RichText::new(if looped {
                format!(
                    "One loop = {:.2} s = {} frames, rendered at exact loop positions so the file loops seamlessly.",
                    project.timing.loop_seconds(),
                    frames
                )
            } else {
                format!("Whole song = {:.1} s = {} frames.", frames as f32 / self.fps, frames)
            })
            .weak(),
        );
        let video_ok = video_encoder_available();
        let busy = self.is_running();
        ui.add_enabled_ui(!busy, |ui| {
            egui::Grid::new("web export")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Format");
                    egui::ComboBox::from_id_salt("webfmt")
                        .selected_text(self.format.label())
                        .show_ui(ui, |ui| {
                            for f in WebFormat::ALL {
                                let enabled = !f.is_video() || video_ok;
                                let r = ui.add_enabled(
                                    enabled,
                                    egui::Button::selectable(self.format == f, f.label()),
                                );
                                if r.clicked() {
                                    self.format = f;
                                    if f == WebFormat::Gif && self.width > 800 {
                                        (self.width, self.height, self.fps) = (480, 270, 25.0);
                                    }
                                }
                                if !enabled {
                                    r.on_disabled_hover_text(
                                        "This browser has no WebCodecs video encoder",
                                    );
                                }
                            }
                        });
                    ui.end_row();
                    ui.label("Size");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.width).range(16..=3840));
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut self.height).range(16..=2160));
                        ui.menu_button("Presets", |ui| {
                            for (n, w, h) in [
                                ("720p", 1280, 720),
                                ("1080p", 1920, 1080),
                                ("Square 1080", 1080, 1080),
                                ("Vertical 1080×1920", 1080, 1920),
                                ("GIF small 480×270", 480, 270),
                            ] {
                                if ui.button(n).clicked() {
                                    (self.width, self.height) = (w, h);
                                    ui.close();
                                }
                            }
                        });
                    });
                    ui.end_row();
                    ui.label("Frame rate");
                    ui.horizontal(|ui| {
                        for f in [24.0, 25.0, 30.0, 60.0] {
                            ui.selectable_value(&mut self.fps, f, format!("{f}"));
                        }
                    });
                    ui.end_row();
                    if self.format != WebFormat::Gif && looped {
                        ui.label("Repeat loop");
                        ui.add(
                            egui::DragValue::new(&mut self.repeats)
                                .range(1..=20)
                                .suffix(" ×"),
                        );
                        ui.end_row();
                    }
                });
        });
        if !video_ok {
            ui.label(RichText::new("Video needs a browser with WebCodecs (Chrome/Edge 94+, Android Chrome). GIF and PNG always work.").weak().small());
        } else if self.format.is_video() && project.audio.is_some() {
            ui.label(
                RichText::new("Note: the web version exports video without the music track.")
                    .weak()
                    .small(),
            );
        }
        ui.add_space(6.0);
        if busy {
            let (done, total, label) = match &self.running {
                Some(Running::Frames { job, .. }) => {
                    let p = job.progress();
                    (p.frame, p.total, format!("frame {} / {}", p.frame, p.total))
                }
                Some(Running::Finishing { .. }) => (1, 1, "finishing the video…".into()),
                _ => (0, 1, "starting…".into()),
            };
            ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(label));
            if ui.button("Cancel").clicked() {
                self.running = None;
                self.request = None;
                self.status = Some(Err("export cancelled".into()));
            }
            ui.ctx().request_repaint();
        } else if ui
            .add(
                egui::Button::new(RichText::new("⏺  Export & download").strong())
                    .min_size(egui::vec2(160.0, 30.0)),
            )
            .clicked()
        {
            self.status = None;
            self.file_name = format!("{}.{}", slug(&project.name), self.format.extension());
            self.request = Some((project.clone(), audio.cloned()));
        }
        match &self.status {
            Some(Ok(m)) => {
                ui.label(RichText::new(format!("✔ {m}")).color(egui::Color32::LIGHT_GREEN));
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("✖ {e}")).color(egui::Color32::LIGHT_RED));
            }
            None => {}
        }
    }

    /// Start an export right away (automated tests).
    pub fn start_test(&mut self, format: &str, project: &Project, audio: Option<&AudioEnvelope>) {
        self.format = match format {
            "mp4" => WebFormat::Mp4,
            "webm" => WebFormat::WebM,
            "zip" => WebFormat::PngZip,
            _ => WebFormat::Gif,
        };
        (self.width, self.height, self.fps, self.repeats) = (320, 180, 24.0, 1);
        self.open = true;
        self.file_name = format!("{}.{}", slug(&project.name), self.format.extension());
        self.request = Some((project.clone(), audio.cloned()));
    }

    /// Advance a running export; call once per frame.
    pub fn tick(&mut self, renderer: &mut Renderer) {
        let (w, h) = (self.width.max(16) & !1, self.height.max(16) & !1);
        // A video export keeps its request while the codec is being chosen,
        // so only start something new when nothing is running.
        let idle = self.running.is_none();
        if let Some((project, audio)) = self.request.take_if(|_| idle) {
            if self.format.is_video() {
                let slot: Slot<Ez2VideoExporter> = Rc::default();
                let s = slot.clone();
                let fmt = if self.format == WebFormat::Mp4 {
                    "mp4"
                } else {
                    "webm"
                };
                let promise = Ez2VideoExporter::create(fmt, w, h, self.fps);
                wasm_bindgen_futures::spawn_local(async move {
                    let r = JsFuture::from(promise)
                        .await
                        .map(|v| v.unchecked_into::<Ez2VideoExporter>())
                        .map_err(js_error);
                    *s.borrow_mut() = Some(r);
                });
                self.request = Some((project, audio));
                self.running = Some(Running::Starting { slot });
                return;
            }
            let sink: Box<dyn FrameSink> = match self.format {
                WebFormat::Gif => match GifSink::new(self.fps) {
                    Ok(s) => Box::new(s),
                    Err(e) => {
                        self.status = Some(Err(e.to_string()));
                        return;
                    }
                },
                _ => Box::<PngZipSink>::default(),
            };
            let repeats = if self.format == WebFormat::Gif {
                1
            } else {
                self.repeats
            };
            let job = Box::new(ExportJob::new(
                renderer, project, audio, w, h, self.fps, repeats, sink,
            ));
            self.running = Some(Running::Frames { job, video: None });
        }

        match self.running.take() {
            None => {}
            Some(Running::Starting { slot }) => {
                let result = slot.borrow_mut().take();
                match result {
                    None => self.running = Some(Running::Starting { slot }),
                    Some(Err(e)) => {
                        self.request = None;
                        self.status = Some(Err(e));
                    }
                    Some(Ok(enc)) => {
                        let Some((project, audio)) = self.request.take() else {
                            return;
                        };
                        log::info!("video codec: {}", enc.codec_name());
                        let sink = Box::new(VideoSink {
                            enc: enc.clone(),
                            index: 0,
                        });
                        let job = Box::new(ExportJob::new(
                            renderer,
                            project,
                            audio,
                            w,
                            h,
                            self.fps,
                            self.repeats,
                            sink,
                        ));
                        self.running = Some(Running::Frames {
                            job,
                            video: Some(enc),
                        });
                    }
                }
            }
            Some(Running::Frames { mut job, video }) => {
                // Don't outrun the hardware encoder.
                if video.as_ref().is_some_and(|v| v.queue_size() > 6) {
                    self.running = Some(Running::Frames { job, video });
                    return;
                }
                for _ in 0..3 {
                    match job.step(renderer) {
                        Ok(JobState::Running(_)) => {}
                        Ok(JobState::Done(bytes)) => {
                            match video {
                                Some(enc) => {
                                    let slot: Slot<Vec<u8>> = Rc::default();
                                    let s = slot.clone();
                                    let promise = enc.finish();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        let r = JsFuture::from(promise)
                                            .await
                                            .map(|v| js_sys::Uint8Array::new(&v).to_vec())
                                            .map_err(js_error);
                                        *s.borrow_mut() = Some(r);
                                    });
                                    self.running = Some(Running::Finishing { slot });
                                }
                                None => self.deliver(bytes),
                            }
                            return;
                        }
                        Err(e) => {
                            self.status = Some(Err(format!("{e:#}")));
                            return;
                        }
                    }
                }
                self.running = Some(Running::Frames { job, video });
            }
            Some(Running::Finishing { slot }) => {
                let result = slot.borrow_mut().take();
                match result {
                    None => self.running = Some(Running::Finishing { slot }),
                    Some(Ok(bytes)) => self.deliver(bytes),
                    Some(Err(e)) => self.status = Some(Err(e)),
                }
            }
        }
    }

    fn deliver(&mut self, bytes: Vec<u8>) {
        let size = bytes.len() as f64 / 1e6;
        self.status = Some(
            crate::platform::download(&self.file_name, &bytes)
                .map(|_| format!("Downloaded {} ({size:.1} MB)", self.file_name)),
        );
    }
}

fn js_error(e: JsValue) -> String {
    e.dyn_ref::<js_sys::Error>()
        .map(|e| String::from(e.message()))
        .or_else(|| e.as_string())
        .unwrap_or_else(|| format!("{e:?}"))
}
