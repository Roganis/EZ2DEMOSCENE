//! Export dialog and the background export job.

use egui::{RichText, Ui};
use ez_core::{AudioEnvelope, Project};
use ez_export::{ExportFormat, ExportSettings, Progress};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const RESOLUTIONS: &[(&str, u32, u32)] = &[
    ("720p (1280×720)", 1280, 720),
    ("1080p (1920×1080)", 1920, 1080),
    ("1440p (2560×1440)", 2560, 1440),
    ("4K (3840×2160)", 3840, 2160),
    ("Square (1080×1080)", 1080, 1080),
    ("Vertical (1080×1920)", 1080, 1920),
    ("GIF small (480×270)", 480, 270),
    ("Retro 4:3 (640×480)", 640, 480),
];

#[derive(Default)]
struct JobState {
    progress: Option<Progress>,
    result: Option<Result<PathBuf, String>>,
}

#[derive(Default)]
pub struct ExportUi {
    pub open: bool,
    pub settings: ExportSettings,
    ffmpeg_path: String,
    ffmpeg_found: Option<Option<PathBuf>>,
    job: Option<(Arc<Mutex<JobState>>, Arc<AtomicBool>)>,
    last_result: Option<Result<PathBuf, String>>,
}

impl ExportUi {
    /// Automated browser tests only.
    #[allow(dead_code)]
    pub fn start_test(
        &mut self,
        _format: &str,
        _project: &Project,
        _audio: Option<&AudioEnvelope>,
    ) {
    }

    /// Size used for "Save still image".
    pub fn still_size(&self) -> (u32, u32) {
        (self.settings.width, self.settings.height)
    }

    /// Desktop exports run on a background thread; nothing to do per frame.
    pub fn tick(&mut self, _renderer: &mut ez_render::Renderer) {}

    pub fn is_running(&self) -> bool {
        self.job.is_some()
    }

    pub fn show(&mut self, ctx: &egui::Context, project: &Project, audio: Option<&AudioEnvelope>) {
        let mut open = self.open;
        egui::Window::new("Export loop")
            .open(&mut open)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| self.contents(ui, project, audio));
        self.open = open;
    }

    fn contents(&mut self, ui: &mut Ui, project: &Project, audio: Option<&AudioEnvelope>) {
        let s = &mut self.settings;
        let frames = project.timing.frame_count(s.fps);
        ui.label(
            RichText::new(format!(
                "One loop = {:.2} s = {} frames. Frames are rendered at exact loop positions, so the file loops seamlessly.",
                project.timing.loop_seconds(),
                frames
            ))
            .weak(),
        );
        ui.add_space(6.0);
        egui::Grid::new("export grid")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                ui.label("Format");
                egui::ComboBox::from_id_salt("fmt")
                    .selected_text(s.format.label())
                    .width(220.0)
                    .show_ui(ui, |ui| {
                        for f in ExportFormat::ALL {
                            if ui.selectable_value(&mut s.format, f, f.label()).changed() {
                                if f == ExportFormat::PngSequence {
                                    s.output = s.output.with_extension("");
                                } else {
                                    s.output = s.output.with_extension(f.extension());
                                }
                                if f == ExportFormat::Gif && s.width > 800 {
                                    s.width = 480;
                                    s.height = 270;
                                    s.fps = 30.0;
                                }
                            }
                        }
                    });
                ui.end_row();

                ui.label("Size");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut s.width).range(16..=7680));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut s.height).range(16..=4320));
                    ui.menu_button("Presets", |ui| {
                        for (name, w, h) in RESOLUTIONS {
                            if ui.button(*name).clicked() {
                                s.width = *w;
                                s.height = *h;
                                ui.close();
                            }
                        }
                    });
                });
                ui.end_row();

                ui.label("Frame rate");
                ui.horizontal(|ui| {
                    for f in [24.0, 25.0, 30.0, 50.0, 60.0] {
                        ui.selectable_value(&mut s.fps, f, format!("{f}"));
                    }
                });
                ui.end_row();

                if s.format != ExportFormat::PngSequence {
                    ui.label("Repeat loop");
                    ui.add(
                        egui::DragValue::new(&mut s.repeats)
                            .range(1..=100)
                            .suffix(" ×"),
                    )
                    .on_hover_text("Make the file longer by repeating the loop");
                    ui.end_row();
                }
                if project.audio.is_some()
                    && matches!(s.format, ExportFormat::Mp4 | ExportFormat::WebM)
                {
                    ui.label("Music");
                    ui.checkbox(&mut s.include_audio, "include the music track");
                    ui.end_row();
                }

                ui.label(if s.format == ExportFormat::PngSequence {
                    "Folder"
                } else {
                    "File"
                });
                ui.horizontal(|ui| {
                    let mut text = s.output.to_string_lossy().to_string();
                    if ui
                        .add(egui::TextEdit::singleline(&mut text).desired_width(200.0))
                        .changed()
                    {
                        s.output = PathBuf::from(text);
                    }
                    if ui.button("Browse…").clicked() {
                        let picked = if s.format == ExportFormat::PngSequence {
                            rfd::FileDialog::new().pick_folder()
                        } else {
                            rfd::FileDialog::new()
                                .add_filter(s.format.label(), &[s.format.extension()])
                                .set_file_name(format!(
                                    "{}.{}",
                                    slug(&project.name),
                                    s.format.extension()
                                ))
                                .save_file()
                        };
                        if let Some(p) = picked {
                            s.output = p;
                        }
                    }
                });
                ui.end_row();
            });

        if s.format.needs_ffmpeg() {
            let found = self.ffmpeg_found.get_or_insert_with(|| {
                ez_export::find_ffmpeg(
                    (!self.ffmpeg_path.is_empty()).then(|| std::path::Path::new(&self.ffmpeg_path)),
                )
            });
            ui.add_space(4.0);
            match found {
                Some(p) => {
                    ui.label(
                        RichText::new(format!("✔ ffmpeg found: {}", p.display()))
                            .color(egui::Color32::LIGHT_GREEN),
                    );
                }
                None => {
                    ui.label(
                        RichText::new("⚠ ffmpeg not found — needed for video and GIF. Install it from ffmpeg.org or point to it:")
                            .color(egui::Color32::YELLOW),
                    );
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.ffmpeg_path);
                        if ui.button("Browse…").clicked() {
                            if let Some(p) = rfd::FileDialog::new().pick_file() {
                                self.ffmpeg_path = p.to_string_lossy().to_string();
                            }
                        }
                        if ui.button("Check").clicked() {
                            self.ffmpeg_found = None;
                        }
                    });
                }
            }
            s.ffmpeg = (!self.ffmpeg_path.is_empty()).then(|| PathBuf::from(&self.ffmpeg_path));
        }

        ui.add_space(8.0);
        // Poll the running job.
        if let Some((state, cancel)) = &self.job {
            let st = state.lock().unwrap();
            if let Some(res) = &st.result {
                self.last_result = Some(res.clone());
                drop(st);
                self.job = None;
            } else {
                let (done, total) = st.progress.map(|p| (p.frame, p.total)).unwrap_or((0, 1));
                drop(st);
                ui.add(
                    egui::ProgressBar::new(done as f32 / total.max(1) as f32)
                        .text(format!("frame {done} / {total}")),
                );
                if ui.button("Cancel").clicked() {
                    cancel.store(true, Ordering::Relaxed);
                }
                ui.ctx().request_repaint();
                return;
            }
        }
        if ui
            .add(
                egui::Button::new(RichText::new("⏺  Export").strong().size(16.0))
                    .min_size(egui::vec2(120.0, 30.0)),
            )
            .clicked()
        {
            self.start(ui.ctx().clone(), project.clone(), audio.cloned());
        }
        match &self.last_result {
            Some(Ok(p)) => {
                ui.label(
                    RichText::new(format!("✔ Saved {}", p.display()))
                        .color(egui::Color32::LIGHT_GREEN),
                );
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("✖ {e}")).color(egui::Color32::LIGHT_RED));
            }
            None => {}
        }
    }

    fn start(&mut self, ctx: egui::Context, project: Project, audio: Option<AudioEnvelope>) {
        let state = Arc::new(Mutex::new(JobState::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let settings = self.settings.clone();
        let (st, c) = (state.clone(), cancel.clone());
        self.last_result = None;
        std::thread::spawn(move || {
            let res = ez_export::export(
                &project,
                &settings,
                audio.as_ref(),
                |p| {
                    st.lock().unwrap().progress = Some(p);
                    ctx.request_repaint();
                },
                &c,
            );
            st.lock().unwrap().result = Some(res.map_err(|e| format!("{e:#}")));
            ctx.request_repaint();
        });
        self.job = Some((state, cancel));
    }
}

pub use crate::platform::slug;
