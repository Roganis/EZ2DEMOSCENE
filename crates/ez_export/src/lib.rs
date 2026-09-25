//! Offline export of a project loop.
//!
//! Frames are rendered at exact loop phases `i / N` (the last frame is *not*
//! a duplicate of the first), so the exported file loops without a seam.
//! Video and GIF encoding pipes raw RGBA frames into `ffmpeg`; a PNG sequence
//! needs no external tool.

use anyhow::{bail, Context, Result};
use ez_core::{AudioEnvelope, EvalCtx, Project};
use ez_render::gpu::Gpu;
use ez_render::Renderer;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

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

/// Decode an audio file and build loudness envelopes (100 samples/s).
pub fn analyze_audio(path: &Path) -> Result<AudioEnvelope> {
    use rodio::Source;
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let dec =
        rodio::Decoder::try_from(file).with_context(|| format!("decoding {}", path.display()))?;
    let channels = dec.channels().get() as usize;
    let rate = dec.sample_rate().get() as f32;
    let env_rate = 100.0;
    let hop = ((rate / env_rate) as usize).max(1) * channels;
    let mut level = Vec::new();
    let mut bass = Vec::new();
    let (mut acc, mut acc_low, mut n) = (0.0f32, 0.0f32, 0usize);
    let mut low = 0.0f32;
    // One-pole low-pass at ~150 Hz for the "kick" band.
    let k = 1.0 - (-2.0 * std::f32::consts::PI * 150.0 / rate).exp();
    let mut frame_sum = 0.0f32;
    let mut ch = 0usize;
    for s in dec {
        acc += s * s;
        frame_sum += s;
        ch += 1;
        if ch == channels {
            let mono = frame_sum / channels as f32;
            low += k * (mono - low);
            acc_low += low * low;
            frame_sum = 0.0;
            ch = 0;
        }
        n += 1;
        if n == hop {
            level.push((acc / n as f32).sqrt());
            bass.push((acc_low / (n / channels).max(1) as f32).sqrt());
            acc = 0.0;
            acc_low = 0.0;
            n = 0;
        }
    }
    if level.is_empty() {
        bail!("{} contains no audio", path.display());
    }
    let norm = |v: &mut Vec<f32>| {
        let mut sorted = v.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let peak = sorted[(sorted.len() as f32 * 0.98) as usize].max(1e-6);
        for x in v.iter_mut() {
            *x = (*x / peak).min(1.0);
        }
    };
    norm(&mut level);
    norm(&mut bass);
    let duration = level.len() as f32 / env_rate;
    Ok(AudioEnvelope {
        rate: env_rate,
        level,
        bass,
        duration,
    })
}

/// Render the loop and write it out. `progress` is called after every frame;
/// setting `cancel` aborts the export.
pub fn export(
    project: &Project,
    settings: &ExportSettings,
    audio: Option<&AudioEnvelope>,
    mut progress: impl FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    let (w, h) = (settings.width.max(16) & !1, settings.height.max(16) & !1);
    let frames = project.timing.frame_count(settings.fps);
    let repeats = if settings.format == ExportFormat::PngSequence {
        1
    } else {
        settings.repeats.max(1)
    };
    let total = frames * repeats;

    let gpu = Gpu::headless()?;
    let mut renderer = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = renderer.create_target(w, h);

    let render_frame = |renderer: &mut Renderer, i: u32| -> Vec<u8> {
        let phase = (i % frames) as f32 / frames as f32;
        let ctx = EvalCtx::new(&project.timing, phase, audio);
        renderer.render(project, &ctx, &target);
        renderer.read_pixels(&target)
    };

    match settings.format {
        ExportFormat::PngSequence => {
            let dir = &settings.output;
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            for i in 0..frames {
                if cancel.load(Ordering::Relaxed) {
                    bail!("export cancelled");
                }
                let px = render_frame(&mut renderer, i);
                let path = dir.join(format!("frame_{i:05}.png"));
                image::save_buffer(&path, &px, w, h, image::ExtendedColorType::Rgba8)
                    .with_context(|| format!("writing {}", path.display()))?;
                progress(Progress {
                    frame: i + 1,
                    total,
                });
            }
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
                cmd.args(["-stream_loop", "-1", "-i", a]);
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
                cmd.args(["-map", "0:v", "-map", "1:a", "-c:a"]);
                cmd.arg(if fmt == ExportFormat::WebM {
                    "libopus"
                } else {
                    "aac"
                });
                cmd.args(["-t", &format!("{}", loop_secs * repeats as f32)]);
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
            // when it fits in memory (< 2 GiB), otherwise re-render.
            let frame_bytes = (w * h * 4) as u64;
            let cache_ok = repeats > 1 && frame_bytes * frames as u64 <= 2 << 30;
            let mut cache: Vec<Vec<u8>> = Vec::new();
            let mut result = Ok(());
            for i in 0..total {
                if cancel.load(Ordering::Relaxed) {
                    result = Err(anyhow::anyhow!("export cancelled"));
                    break;
                }
                let idx = (i % frames) as usize;
                let px = if cache_ok && idx < cache.len() {
                    None
                } else {
                    Some(render_frame(&mut renderer, i))
                };
                let data = match &px {
                    Some(p) => p.as_slice(),
                    None => cache[idx].as_slice(),
                };
                if let Err(e) = stdin.write_all(data) {
                    result = Err(anyhow::anyhow!("ffmpeg stopped accepting frames: {e}"));
                    break;
                }
                if let (true, Some(p)) = (cache_ok, px) {
                    cache.push(p);
                }
                progress(Progress {
                    frame: i + 1,
                    total,
                });
            }
            drop(stdin);
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

/// Render a single still frame to a PNG.
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

#[cfg(test)]
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
        let mut data = Vec::new();
        for i in 0..rate {
            let t = i as f32 / rate as f32;
            let mut s = (t * 440.0 * std::f32::consts::TAU).sin() * 0.3;
            if t > 0.5 {
                s += (t * 60.0 * std::f32::consts::TAU).sin() * 0.6;
            }
            data.extend_from_slice(&((s * 32767.0) as i16).to_le_bytes());
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
        std::fs::write(&path, wav).unwrap();
        let env = analyze_audio(&path).unwrap();
        assert!((env.duration - 1.0).abs() < 0.05);
        let (_, bass_early) = env.sample(0.25);
        let (_, bass_late) = env.sample(0.75);
        assert!(bass_late > bass_early * 2.0, "{bass_early} vs {bass_late}");
    }
}
