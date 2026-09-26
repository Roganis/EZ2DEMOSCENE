//! Offline export of a project loop.
//!
//! Frames are rendered at exact loop phases `i / N` (the last frame is *not*
//! a duplicate of the first), so the exported file loops without a seam.
//! Video and GIF encoding pipes raw RGBA frames into `ffmpeg`; a PNG sequence
//! needs no external tool.

mod job;
pub use job::{ExportJob, FrameSink, GifSink, JobState, PngZipSink};

use anyhow::{bail, Context, Result};
use ez_core::AudioEnvelope;
#[cfg(not(target_arch = "wasm32"))]
use ez_core::{EvalCtx, Project};
#[cfg(not(target_arch = "wasm32"))]
use ez_render::gpu::Gpu;
#[cfg(not(target_arch = "wasm32"))]
use ez_render::Renderer;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::process::{Command, Stdio};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Mp4,
    WebM,
    Gif,
    PngSequence,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 4] = [
        ExportFormat::Mp4,
        ExportFormat::WebM,
        ExportFormat::Gif,
        ExportFormat::PngSequence,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Mp4 => "MP4 video (H.264)",
            ExportFormat::WebM => "WebM video (VP9)",
            ExportFormat::Gif => "Animated GIF",
            ExportFormat::PngSequence => "PNG sequence (no ffmpeg needed)",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Mp4 => "mp4",
            ExportFormat::WebM => "webm",
            ExportFormat::Gif => "gif",
            ExportFormat::PngSequence => "",
        }
    }

    pub fn needs_ffmpeg(self) -> bool {
        self != ExportFormat::PngSequence
    }

    /// Guess the format from a file name.
    pub fn from_path(path: &Path) -> ExportFormat {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref()
        {
            Some("mp4") | Some("mov") => ExportFormat::Mp4,
            Some("webm") => ExportFormat::WebM,
            Some("gif") => ExportFormat::Gif,
            _ => ExportFormat::PngSequence,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportSettings {
    pub format: ExportFormat,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    /// How many times the loop is repeated in the file (videos/GIFs).
    pub repeats: u32,
    /// File path, or a directory for PNG sequences.
    pub output: PathBuf,
    /// Explicit ffmpeg binary; otherwise `ffmpeg` on the PATH.
    pub ffmpeg: Option<PathBuf>,
    /// Mux the project's audio track into video exports.
    pub include_audio: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        ExportSettings {
            format: ExportFormat::Mp4,
            width: 1920,
            height: 1080,
            fps: 60.0,
            repeats: 1,
            output: PathBuf::from("loop.mp4"),
            ffmpeg: None,
            include_audio: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Progress {
    pub frame: u32,
    pub total: u32,
}

/// Finds a working ffmpeg: the explicit path first, then `ffmpeg` on PATH.
#[cfg(not(target_arch = "wasm32"))]
pub fn find_ffmpeg(explicit: Option<&Path>) -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let mut candidates: Vec<PathBuf> = explicit.map(|p| vec![p.to_path_buf()]).unwrap_or_default();
    // Release builds ship ffmpeg next to the program (or in ./ffmpeg/).
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join(exe_name));
        candidates.push(dir.join("ffmpeg").join(exe_name));
    }
    candidates.push(PathBuf::from("ffmpeg"));
    candidates.into_iter().find(|c| {
        (c.components().count() == 1 || c.exists())
            && Command::new(c)
                .arg("-version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
    })
}

/// Decode an audio file and analyse it (bands, hits, tempo, pitch).
pub fn analyze_audio(path: &Path) -> Result<AudioEnvelope> {
    analyze_audio_asset(&path.to_string_lossy())
}

/// [`analyze_audio`] for an asset path (file or in-memory `mem://` asset).
pub fn analyze_audio_asset(path: &str) -> Result<AudioEnvelope> {
    let bytes = ez_core::store::read(path).with_context(|| format!("opening {path}"))?;
    analyze_audio_bytes(bytes.to_vec()).with_context(|| format!("decoding {path}"))
}

/// Decode audio held in memory and analyse it.
pub fn analyze_audio_bytes(bytes: Vec<u8>) -> Result<AudioEnvelope> {
    let mut d = Decoding::new(bytes)?;
    while !d.step(usize::MAX) {}
    Ok(ez_core::analysis::analyze(&d.finish()?, d.rate))
}

/// The music of a project: the analysed audio file and/or MIDI notes
/// (MIDI replaces the detected hits and pitch). `None` without either.
pub fn load_music(project: &ez_core::Project) -> Result<Option<AudioEnvelope>> {
    Ok(MusicJob::new(project, None)?.run()?.music)
}

/// Audio decoding to mono, a slice at a time.
struct Decoding {
    dec: rodio::Decoder<std::io::Cursor<Vec<u8>>>,
    channels: usize,
    rate: f32,
    /// Expected mono samples, when the file says.
    expected: Option<usize>,
    mono: Vec<f32>,
    sum: f32,
    ch: usize,
}

impl Decoding {
    fn new(bytes: Vec<u8>) -> Result<Decoding> {
        use rodio::Source;
        let dec = rodio::Decoder::new(std::io::Cursor::new(bytes))?;
        let channels = dec.channels().get() as usize;
        let rate = dec.sample_rate().get() as f32;
        let expected = dec
            .total_duration()
            .map(|d| (d.as_secs_f64() * rate as f64) as usize);
        Ok(Decoding {
            dec,
            channels,
            rate,
            expected,
            mono: Vec::with_capacity(expected.unwrap_or(0)),
            sum: 0.0,
            ch: 0,
        })
    }

    /// Decode up to `max` mono samples; `true` at the end of the file.
    fn step(&mut self, max: usize) -> bool {
        let end = self.mono.len().saturating_add(max);
        while self.mono.len() < end {
            let Some(s) = self.dec.next() else {
                return true;
            };
            self.sum += s;
            self.ch += 1;
            if self.ch == self.channels {
                self.mono.push(self.sum / self.channels as f32);
                self.sum = 0.0;
                self.ch = 0;
            }
        }
        false
    }

    fn progress(&self) -> f32 {
        match self.expected {
            Some(n) if n > 0 => (self.mono.len() as f32 / n as f32).min(1.0),
            _ => 0.5,
        }
    }

    fn finish(&mut self) -> Result<Vec<f32>> {
        if self.mono.is_empty() {
            bail!("the file contains no audio");
        }
        Ok(std::mem::take(&mut self.mono))
    }
}

#[allow(clippy::large_enum_variant)] // one short-lived value
enum Stage {
    Done(Option<AudioEnvelope>),
    Decoding(String, Box<Decoding>),
    Analysing(Box<ez_core::analysis::Analysis>),
}

/// What [`MusicJob`] produces.
pub struct LoadedMusic {
    /// The analysed audio file alone (before MIDI), worth keeping so a
    /// MIDI change doesn't re-analyse the song.
    pub audio: Option<AudioEnvelope>,
    /// What the project plays to: audio and/or MIDI.
    pub music: Option<AudioEnvelope>,
}

/// Loads a project's music (decode, analyse, apply MIDI) in slices, so it
/// can run on a thread or spread over frames in a browser.
pub struct MusicJob {
    stage: Stage,
    midi: Option<(ez_core::midi::MidiData, f32)>,
}

impl MusicJob {
    /// Reads the files and parses the MIDI. `cached` is the audio file's
    /// analysis from an earlier [`LoadedMusic::audio`], if the file is
    /// unchanged.
    pub fn new(project: &ez_core::Project, cached: Option<AudioEnvelope>) -> Result<MusicJob> {
        let stage = match (&project.audio, cached) {
            (None, _) => Stage::Done(None),
            (Some(_), Some(env)) => Stage::Done(Some(env)),
            (Some(a), None) => {
                let bytes = ez_core::store::read(a).with_context(|| format!("opening {a}"))?;
                let d = Decoding::new(bytes.to_vec()).with_context(|| format!("decoding {a}"))?;
                Stage::Decoding(a.clone(), Box::new(d))
            }
        };
        let midi = match &project.music.midi {
            Some(m) => {
                let bytes = ez_core::store::read(m).with_context(|| format!("opening {m}"))?;
                let midi = ez_core::midi::parse(&bytes).map_err(|e| anyhow::anyhow!("{m}: {e}"))?;
                Some((midi, project.music.midi_offset))
            }
            None => None,
        };
        Ok(MusicJob { stage, midi })
    }

    /// Do a slice of work (about `units` analysis frames' worth); `true`
    /// once [`MusicJob::finish`] has nothing left to do.
    pub fn step(&mut self, units: usize) -> Result<bool> {
        match &mut self.stage {
            Stage::Done(_) => return Ok(true),
            Stage::Decoding(path, d) => {
                if d.step(units.saturating_mul(1024)) {
                    let mono = d.finish().with_context(|| format!("decoding {path}"))?;
                    self.stage =
                        Stage::Analysing(Box::new(ez_core::analysis::Analysis::new(mono, d.rate)));
                }
            }
            Stage::Analysing(a) => {
                if a.step(units) {
                    let Stage::Analysing(a) = std::mem::replace(&mut self.stage, Stage::Done(None))
                    else {
                        unreachable!()
                    };
                    self.stage = Stage::Done(Some(a.finish()));
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// 0..1 (decoding is the first fifth).
    pub fn progress(&self) -> f32 {
        match &self.stage {
            Stage::Done(_) => 1.0,
            Stage::Decoding(_, d) => d.progress() * 0.2,
            Stage::Analysing(a) => 0.2 + a.progress() * 0.8,
        }
    }

    /// Finish (doing any remaining work) and apply the MIDI.
    pub fn run(mut self) -> Result<LoadedMusic> {
        while !self.step(usize::MAX)? {}
        let Stage::Done(audio) = self.stage else {
            unreachable!()
        };
        let mut music = audio.clone();
        if let Some((midi, offset)) = &self.midi {
            match &mut music {
                Some(e) => e.apply_midi(midi, *offset),
                None => music = Some(AudioEnvelope::from_midi(midi)),
            }
        }
        Ok(LoadedMusic { audio, music })
    }
}

/// Frames of one export pass: one loop, or the whole song in full-track
/// mode. Returns (frames, whether frames are loop phases).
pub fn export_frames(
    project: &ez_core::Project,
    audio: Option<&AudioEnvelope>,
    fps: f32,
) -> (u32, bool) {
    match (project.music.mode, audio) {
        (ez_core::MusicMode::FullTrack, Some(a)) => {
            (((a.duration * fps).round() as u32).max(1), false)
        }
        _ => (project.timing.frame_count(fps), true),
    }
}

/// Evaluation context of frame `i` of an export pass.
pub fn export_ctx(
    project: &ez_core::Project,
    audio: Option<&AudioEnvelope>,
    fps: f32,
    frames: u32,
    looped: bool,
    i: u32,
) -> ez_core::EvalCtx {
    if looped {
        project.ctx((i % frames) as f32 / frames as f32, audio)
    } else {
        project.ctx_at(i as f64 / fps as f64, audio)
    }
}

/// Render the loop and write it out. `progress` is called after every frame;
/// setting `cancel` aborts the export.
#[cfg(not(target_arch = "wasm32"))]
pub fn export(
    project: &Project,
    settings: &ExportSettings,
    audio: Option<&AudioEnvelope>,
    mut progress: impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    let (w, h) = (settings.width.max(16) & !1, settings.height.max(16) & !1);
    let (frames, looped) = export_frames(project, audio, settings.fps);
    let repeats = if settings.format == ExportFormat::PngSequence || !looped {
        1
    } else {
        settings.repeats.max(1)
    };
    let total = frames * repeats;

    let gpu = Gpu::headless()?;
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = renderer.create_target(w, h);

    let ctx_of =
        |i: u32| -> EvalCtx { export_ctx(project, audio, settings.fps, frames, looped, i) };

    match settings.format {
        ExportFormat::PngSequence => {
            let dir = &settings.output;
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            // PNG encoding runs on a writer thread while the GPU renders.
            let (tx, rx) = std::sync::mpsc::sync_channel::<(u32, Vec<u8>)>(4);
            let out_dir = dir.clone();
            let writer = std::thread::spawn(move || -> Result<()> {
                for (i, px) in rx {
                    let path = out_dir.join(format!("frame_{i:05}.png"));
                    image::save_buffer(&path, &px, w, h, image::ExtendedColorType::Rgba8)
                        .with_context(|| format!("writing {}", path.display()))?;
                }
                Ok(())
            });
            let rendered = render_pipelined(
                &mut renderer,
                &target,
                project,
                frames,
                ctx_of,
                cancel,
                |i, px| {
                    tx.send((i, px))
                        .map_err(|_| anyhow::anyhow!("the PNG writer stopped"))?;
                    progress(Progress {
                        frame: i + 1,
                        total,
                    });
                    Ok(())
                },
            );
            drop(tx);
            let written = writer
                .join()
                .map_err(|_| anyhow::anyhow!("the PNG writer crashed"))?;
            rendered?;
            written?;
            Ok(dir.clone())
        }
        fmt => {
            let ffmpeg = find_ffmpeg(settings.ffmpeg.as_deref()).context(
                "ffmpeg was not found. Install it (https://ffmpeg.org) or pick 'PNG sequence'.",
            )?;
            let loop_secs = frames as f32 / settings.fps;
            let audio_path = project
                .audio
                .as_ref()
                .filter(|_| settings.include_audio && fmt != ExportFormat::Gif)
                .filter(|p| Path::new(p).exists());
            let mut cmd = Command::new(&ffmpeg);
            cmd.args(["-y", "-hide_banner", "-loglevel", "error"])
                .args(["-f", "rawvideo", "-pix_fmt", "rgba"])
                .args(["-s", &format!("{w}x{h}")])
                .args(["-r", &format!("{}", settings.fps)])
                .args(["-i", "-"]);
            if let Some(a) = audio_path {
                if looped {
                    // The loop window of the song, repeated with the video.
                    let sr = audio.map(|e| e.sample_rate).unwrap_or(44100).max(1);
                    let start = project.music.offset.max(0.0);
                    let len = project.timing.loop_seconds();
                    cmd.args(["-i", a]).args([
                        "-filter_complex",
                        &format!(
                            "[1:a]atrim=start={start}:duration={len},asetpts=PTS-STARTPTS,aloop=loop={}:size={}[aud]",
                            repeats.saturating_sub(1),
                            (len * sr as f32).round() as u64
                        ),
                    ]);
                } else {
                    cmd.args(["-i", a]);
                }
            }
            match fmt {
                ExportFormat::Mp4 => {
                    cmd.args([
                        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "16", "-preset", "slow",
                    ])
                    .args(["-movflags", "+faststart"]);
                }
                ExportFormat::WebM => {
                    cmd.args([
                        "-c:v",
                        "libvpx-vp9",
                        "-pix_fmt",
                        "yuv420p",
                        "-crf",
                        "24",
                        "-b:v",
                        "0",
                    ]);
                }
                ExportFormat::Gif => {
                    cmd.args([
                        "-filter_complex",
                        "[0:v]split[a][b];[a]palettegen=stats_mode=full[p];[b][p]paletteuse=dither=bayer:bayer_scale=3",
                        "-loop",
                        "0",
                    ]);
                }
                ExportFormat::PngSequence => unreachable!(),
            }
            if audio_path.is_some() {
                let track = if looped { "[aud]" } else { "1:a" };
                cmd.args(["-map", "0:v", "-map", track, "-c:a"]);
                cmd.arg(if fmt == ExportFormat::WebM {
                    "libopus"
                } else {
                    "aac"
                });
                cmd.args(["-t", &format!("{}", loop_secs * repeats as f32)]);
                cmd.arg("-shortest");
            }
            cmd.arg(&settings.output);
            cmd.stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            let mut child = cmd
                .spawn()
                .with_context(|| format!("starting {}", ffmpeg.display()))?;
            let mut stdin = child.stdin.take().expect("piped stdin");
            // Frames are identical across repeats: render one loop, reuse it
            // when it fits in memory (< 2 GiB), otherwise re-render. A writer
            // thread feeds ffmpeg while the GPU renders the next frames.
            let frame_bytes = (w * h * 4) as u64;
            let cache_ok = repeats > 1 && frame_bytes * frames as u64 <= 2 << 30;
            let (tx, rx) = std::sync::mpsc::sync_channel::<Arc<Vec<u8>>>(4);
            let writer = std::thread::spawn(move || -> Result<()> {
                for px in rx {
                    stdin
                        .write_all(&px)
                        .map_err(|e| anyhow::anyhow!("ffmpeg stopped accepting frames: {e}"))?;
                }
                Ok(())
            });
            let mut cache: Vec<Arc<Vec<u8>>> = Vec::new();
            let to_render = if cache_ok { frames } else { total };
            let mut result = render_pipelined(
                &mut renderer,
                &target,
                project,
                to_render,
                ctx_of,
                cancel,
                |i, px| {
                    let px = Arc::new(px);
                    if cache_ok {
                        cache.push(px.clone());
                    }
                    tx.send(px)
                        .map_err(|_| anyhow::anyhow!("ffmpeg stopped accepting frames"))?;
                    progress(Progress {
                        frame: i + 1,
                        total,
                    });
                    Ok(())
                },
            );
            if cache_ok && result.is_ok() {
                for i in frames..total {
                    if cancel.load(Ordering::Relaxed) {
                        result = Err(anyhow::anyhow!("export cancelled"));
                        break;
                    }
                    if tx.send(cache[(i % frames) as usize].clone()).is_err() {
                        break;
                    }
                    progress(Progress {
                        frame: i + 1,
                        total,
                    });
                }
            }
            drop(tx);
            let written = writer
                .join()
                .map_err(|_| anyhow::anyhow!("the ffmpeg writer crashed"))?;
            let result = result.and(written);
            let out = child.wait_with_output()?;
            result?;
            if !out.status.success() {
                bail!(
                    "ffmpeg failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            Ok(settings.output.clone())
        }
    }
}

/// Renders frames `0..count` with up to three frames in flight on the GPU
/// (frame n + 2 renders while frame n is copied back) and hands each one to
/// `sink` in order.
#[cfg(not(target_arch = "wasm32"))]
fn render_pipelined(
    renderer: &mut Renderer,
    target: &ez_render::RenderTarget,
    project: &Project,
    count: u32,
    ctx_of: impl Fn(u32) -> EvalCtx,
    cancel: &AtomicBool,
    mut sink: impl FnMut(u32, Vec<u8>) -> Result<()>,
) -> Result<()> {
    const IN_FLIGHT: usize = 3;
    let mut pending = std::collections::VecDeque::with_capacity(IN_FLIGHT);
    let mut finish_one = |renderer: &Renderer,
                          pending: &mut std::collections::VecDeque<(u32, ez_render::Readback)>|
     -> Result<()> {
        if let Some((j, rb)) = pending.pop_front() {
            renderer.wait_for(&rb);
            let px = rb.take().context("reading back a frame failed")?;
            sink(j, px)?;
        }
        Ok(())
    };
    for i in 0..count {
        if cancel.load(Ordering::Relaxed) {
            bail!("export cancelled");
        }
        renderer.render(project, &ctx_of(i), target);
        pending.push_back((i, renderer.start_readback(target)));
        if pending.len() >= IN_FLIGHT {
            finish_one(renderer, &mut pending)?;
        }
    }
    while !pending.is_empty() {
        finish_one(renderer, &mut pending)?;
    }
    Ok(())
}

/// Render a single still frame to a PNG.
#[cfg(not(target_arch = "wasm32"))]
pub fn render_still(
    project: &Project,
    phase: f32,
    width: u32,
    height: u32,
    path: &Path,
) -> Result<()> {
    let gpu = Gpu::headless()?;
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = renderer.create_target(width, height);
    let ctx = EvalCtx::new(&project.timing, phase, None);
    let img = renderer.render_image(project, &ctx, &target);
    img.save(path)
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use ez_core::presets;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join("ez2_export_tests");
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    #[test]
    fn format_from_path() {
        assert_eq!(
            ExportFormat::from_path(Path::new("a.GIF")),
            ExportFormat::Gif
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("out")),
            ExportFormat::PngSequence
        );
    }

    #[test]
    fn exports_png_and_video() {
        if Gpu::headless().is_err() {
            eprintln!("no GPU, skipping");
            return;
        }
        let mut p = presets::orbiting_solid();
        p.timing.bpm = 240.0;
        p.timing.loop_beats = 2; // 0.5 s
        let cancel = AtomicBool::new(false);
        let mut s = ExportSettings {
            format: ExportFormat::PngSequence,
            width: 96,
            height: 54,
            fps: 12.0,
            repeats: 2,
            output: tmp("pngseq"),
            ..Default::default()
        };
        export(&p, &s, None, |_| {}, &cancel).unwrap();
        let n = std::fs::read_dir(&s.output).unwrap().count();
        assert_eq!(n, 6);
        if find_ffmpeg(None).is_none() {
            eprintln!("no ffmpeg, skipping video");
            return;
        }
        for fmt in [ExportFormat::Mp4, ExportFormat::Gif] {
            s.format = fmt;
            s.output = tmp(&format!("loop.{}", fmt.extension()));
            let mut last = 0;
            export(&p, &s, None, |pr| last = pr.frame, &cancel).unwrap();
            assert_eq!(last, 12);
            assert!(std::fs::metadata(&s.output).unwrap().len() > 100);
        }
    }

    #[test]
    fn audio_envelope_from_wav() {
        // 1 s of 440 Hz tone with a 60 Hz "kick" in the second half.
        let path = tmp("tone.wav");
        let rate = 22050u32;
        let samples: Vec<f32> = (0..rate)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let mut s = (t * 440.0 * std::f32::consts::TAU).sin() * 0.3;
                if t > 0.5 {
                    s += (t * 60.0 * std::f32::consts::TAU).sin() * 0.6;
                }
                s
            })
            .collect();
        write_wav(&path, &samples, rate);
        let env = analyze_audio(&path).unwrap();
        assert!((env.duration - 1.0).abs() < 0.05);
        let (_, bass_early) = env.sample(0.25);
        let (_, bass_late) = env.sample(0.75);
        assert!(bass_late > bass_early * 2.0, "{bass_early} vs {bass_late}");
    }

    /// Full-track exports run through the whole song; loop-window exports
    /// mux the window of the song, repeated.
    #[test]
    fn music_modes_export() {
        if Gpu::headless().is_err() {
            eprintln!("no GPU, skipping");
            return;
        }
        let rate = 22050u32;
        let samples: Vec<f32> = (0..rate * 3)
            .map(|i| {
                let t = i as f32 / rate as f32;
                let bt = (t * 2.0).fract() / 2.0;
                (std::f32::consts::TAU * 60.0 * bt).sin() * (-bt * 30.0).exp() * 0.8
            })
            .collect();
        let wav = tmp("beat.wav");
        write_wav(&wav, &samples, rate);
        let mut p = presets::orbiting_solid();
        p.timing.bpm = 240.0;
        p.timing.loop_beats = 2; // 0.5 s
        p.audio = Some(wav.to_string_lossy().to_string());
        p.music.offset = 0.7;
        let env = load_music(&p).unwrap().expect("music");
        let cancel = AtomicBool::new(false);
        let mut s = ExportSettings {
            format: ExportFormat::PngSequence,
            width: 64,
            height: 36,
            fps: 10.0,
            output: tmp("fulltrack"),
            ..Default::default()
        };
        p.music.mode = ez_core::MusicMode::FullTrack;
        let _ = std::fs::remove_dir_all(&s.output);
        export(&p, &s, Some(&env), |_| {}, &cancel).unwrap();
        assert_eq!(std::fs::read_dir(&s.output).unwrap().count(), 30);
        if find_ffmpeg(None).is_none() {
            eprintln!("no ffmpeg, skipping video");
            return;
        }
        p.music.mode = ez_core::MusicMode::LoopWindow;
        s.format = ExportFormat::Mp4;
        s.repeats = 3;
        s.output = tmp("window.mp4");
        export(&p, &s, Some(&env), |_| {}, &cancel).unwrap();
        // The file has a video and an audio stream of 3 loops (1.5 s).
        let probe = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,duration",
                "-of",
                "csv=p=0",
            ])
            .arg(&s.output)
            .output();
        if let Ok(out) = probe {
            let text = String::from_utf8_lossy(&out.stdout).to_string();
            assert!(text.contains("audio"), "no audio stream: {text}");
            for line in text.lines() {
                if let Some(d) = line.split(',').nth(1).and_then(|d| d.parse::<f32>().ok()) {
                    assert!((d - 1.5).abs() < 0.15, "stream length {d}: {text}");
                }
            }
        }
    }

    #[test]
    fn music_job_in_slices_matches_and_caches() {
        let rate = 22050;
        let samples: Vec<f32> = (0..rate * 3)
            .map(|i| {
                let t = (i % (rate / 2)) as f32 / rate as f32;
                (t * 60.0 * std::f32::consts::TAU * 10.0).sin() * (-t * 30.0).exp()
            })
            .collect();
        let path = tmp("job.wav");
        write_wav(&path, &samples, rate);
        let mut project = presets::orbiting_solid();
        project.audio = Some(path.to_string_lossy().to_string());
        let whole = load_music(&project).unwrap().unwrap();

        let mut job = MusicJob::new(&project, None).unwrap();
        let (mut last, mut steps) = (0.0, 0);
        while !job.step(10).unwrap() {
            let p = job.progress();
            assert!((last..=1.0).contains(&p), "{last} -> {p}");
            last = p;
            steps += 1;
        }
        assert!(steps > 20, "{steps}");
        let loaded = job.run().unwrap();
        assert_eq!(loaded.music.as_ref(), Some(&whole));

        // A cached analysis skips the file entirely.
        project.audio = Some("/nonexistent/song.wav".into());
        let mut job = MusicJob::new(&project, loaded.audio).unwrap();
        assert!(job.step(0).unwrap());
        assert_eq!(job.run().unwrap().music, Some(whole));
    }

    fn write_wav(path: &Path, samples: &[f32], rate: u32) {
        let mut data = Vec::new();
        for s in samples {
            data.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);
        std::fs::write(path, wav).unwrap();
    }
}
