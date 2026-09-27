//! Incremental export: renders one frame per [`ExportJob::step`] on the
//! caller's GPU device and hands pixels to a [`FrameSink`]. Nothing blocks,
//! so it runs inside the browser's frame loop as well as on desktop.

use anyhow::{bail, Result};
use ez_core::{AudioEnvelope, EvalCtx, Project};
use ez_render::{Readback, RenderTarget, Renderer};
use image::codecs::gif::{GifEncoder, Repeat};
use std::io::Write;

use crate::Progress;

/// Receives rendered frames (tightly packed sRGB RGBA8).
pub trait FrameSink {
    fn add_frame(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<()>;
    /// Finish and return the encoded file.
    fn finish(self: Box<Self>) -> Result<Vec<u8>>;
    /// Suggested file extension.
    fn extension(&self) -> &'static str;
}

/// Animated GIF encoded in pure Rust (NeuQuant palette per frame).
pub struct GifSink {
    encoder: GifEncoder<SharedBuf>,
    out: SharedBuf,
    fps: f32,
    index: u32,
}

/// A `Write` target we can still read after the encoder takes ownership.
#[derive(Clone, Default)]
struct SharedBuf(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl GifSink {
    pub fn new(fps: f32) -> Result<GifSink> {
        let out = SharedBuf::default();
        let mut encoder = GifEncoder::new_with_speed(out.clone(), 10);
        encoder.set_repeat(Repeat::Infinite)?;
        Ok(GifSink {
            encoder,
            out,
            fps: fps.max(1.0),
            index: 0,
        })
    }
}

impl FrameSink for GifSink {
    fn add_frame(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<()> {
        let img = image::RgbaImage::from_raw(width, height, rgba.to_vec())
            .ok_or_else(|| anyhow::anyhow!("frame size mismatch"))?;
        // GIF delays are whole centiseconds: alternate them so the total
        // length is exact (e.g. 24 fps -> 4,4,4,5,4,4,4,5… cs).
        let cs = |i: u32| (i as f64 * 100.0 / self.fps as f64).round() as u32;
        let delay = cs(self.index + 1) - cs(self.index);
        let frame =
            image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(delay * 10, 1));
        self.encoder.encode_frame(frame)?;
        self.index += 1;
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>> {
        let GifSink { encoder, out, .. } = *self;
        drop(encoder); // writes the GIF trailer
        let bytes = std::mem::take(&mut *out.0.lock().unwrap());
        Ok(bytes)
    }

    fn extension(&self) -> &'static str {
        "gif"
    }
}

/// A zip of numbered PNG frames.
pub struct PngZipSink {
    zip: zip::ZipWriter<std::io::Cursor<Vec<u8>>>,
    index: u32,
}

impl Default for PngZipSink {
    fn default() -> Self {
        PngZipSink {
            zip: zip::ZipWriter::new(std::io::Cursor::new(Vec::new())),
            index: 0,
        }
    }
}

impl FrameSink for PngZipSink {
    fn add_frame(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<()> {
        use image::ImageEncoder;
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png).write_image(
            rgba,
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )?;
        // PNGs are already compressed: store them as-is.
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        self.zip
            .start_file(format!("frame_{:05}.png", self.index), opts)?;
        self.zip.write_all(&png)?;
        self.index += 1;
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Vec<u8>> {
        Ok(self.zip.finish()?.into_inner())
    }

    fn extension(&self) -> &'static str {
        "zip"
    }
}

pub enum JobState {
    Running(Progress),
    Done(Vec<u8>),
}

/// Frame-by-frame export of one or more loops into a sink.
pub struct ExportJob {
    project: Project,
    audio: Option<AudioEnvelope>,
    target: RenderTarget,
    sink: Option<Box<dyn FrameSink>>,
    /// Frames rendered and being copied back, oldest first.
    pending: std::collections::VecDeque<Readback>,
    frames: u32,
    /// Frames are loop phases (else seconds into the song).
    looped: bool,
    fps: f32,
    total: u32,
    next: u32,
    done: u32,
    /// Motion blur (sub-frames per frame) and the sub-frame to render next.
    blur: crate::MotionBlur,
    sub: u32,
    /// Feedback warm-up frames still to render (a loop before frame 0).
    warmup: u32,
}

impl ExportJob {
    /// `repeats` repeats the loop in the output (use 1 for GIFs, which loop
    /// by themselves).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        renderer: &Renderer,
        project: Project,
        audio: Option<AudioEnvelope>,
        width: u32,
        height: u32,
        fps: f32,
        repeats: u32,
        sink: Box<dyn FrameSink>,
    ) -> ExportJob {
        let (frames, looped) = crate::export_frames(&project, audio.as_ref(), fps);
        let project_uses_feedback = project.uses_feedback();
        let repeats = if looped { repeats } else { 1 };
        ExportJob {
            target: renderer.create_target(width.max(16) & !1, height.max(16) & !1),
            project,
            audio,
            sink: Some(sink),
            pending: Default::default(),
            frames,
            looped,
            fps,
            total: frames * repeats.max(1),
            next: 0,
            done: 0,
            blur: crate::MotionBlur::new(1, 0.0),
            sub: 0,
            warmup: if looped && project_uses_feedback {
                frames
            } else {
                0
            },
        }
    }

    /// Average `subframes` sub-frames over the `shutter` per frame.
    pub fn with_motion_blur(mut self, subframes: u32, shutter: f32) -> ExportJob {
        self.blur = crate::MotionBlur::new(subframes, shutter);
        self
    }

    pub fn extension(&self) -> &'static str {
        self.sink.as_ref().map(|s| s.extension()).unwrap_or("bin")
    }

    pub fn progress(&self) -> Progress {
        Progress {
            frame: self.done,
            total: self.total,
        }
    }

    /// Advance the export. Call repeatedly (e.g. once or twice per UI frame).
    pub fn step(&mut self, renderer: &mut Renderer) -> Result<JobState> {
        // Up to three frames in flight: the GPU renders the next frames
        // while earlier ones are copied back.
        const IN_FLIGHT: usize = 3;
        renderer.poll();
        while self.pending.front().is_some_and(|rb| rb.is_ready()) {
            let rb = self.pending.pop_front().unwrap();
            let (w, h) = (rb.width, rb.height);
            let Some(px) = rb.take() else {
                bail!("reading back a frame failed");
            };
            let Some(frame) = self.blur.add(px) else {
                continue;
            };
            if let Some(sink) = &mut self.sink {
                sink.add_frame(&frame, w, h)?;
            }
            self.done += 1;
        }
        if self.warmup > 0 {
            // A loop of warm-up (not kept) builds the feedback history.
            let ctx = crate::export_ctx_at(
                &self.project,
                self.audio.as_ref(),
                self.fps,
                self.frames,
                self.looped,
                (self.frames - self.warmup) as f64,
            );
            renderer.render(&self.project, &ctx, &self.target);
            self.warmup -= 1;
            return Ok(JobState::Running(self.progress()));
        }
        if self.next < self.total && self.pending.len() < IN_FLIGHT {
            // Frame i sits at phase i / N: the last frame is not a copy of
            // the first, so the file loops without a seam.
            let ctx: EvalCtx = crate::export_ctx_at(
                &self.project,
                self.audio.as_ref(),
                self.fps,
                self.frames,
                self.looped,
                self.next as f64 + self.blur.offset(self.sub),
            );
            renderer.render(&self.project, &ctx, &self.target);
            self.pending
                .push_back(renderer.start_readback(&self.target));
            self.sub += 1;
            if self.sub >= self.blur.subframes() {
                self.sub = 0;
                self.next += 1;
            }
        }
        if self.done < self.total {
            return Ok(JobState::Running(self.progress()));
        }
        match self.sink.take() {
            Some(sink) => Ok(JobState::Done(sink.finish()?)),
            None => bail!("export already finished"),
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use ez_core::presets;
    use ez_render::gpu::Gpu;

    fn run(job: &mut ExportJob, r: &mut Renderer) -> Vec<u8> {
        for _ in 0..10_000 {
            if let JobState::Done(bytes) = job.step(r).unwrap() {
                return bytes;
            }
            // `step` never blocks; on some backends (e.g. Metal) a readback
            // takes longer than a few thousand spins, so wait for the GPU.
            r.wait();
        }
        panic!("export did not finish");
    }

    #[test]
    fn gif_and_png_zip_jobs() {
        let Ok(gpu) = Gpu::headless() else {
            eprintln!("no GPU, skipping");
            return;
        };
        let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
        let mut p = presets::orbiting_solid();
        p.timing.bpm = 240.0;
        p.timing.loop_beats = 2; // 0.5 s -> 6 frames at 12 fps

        let mut job = ExportJob::new(
            &r,
            p.clone(),
            None,
            64,
            36,
            12.0,
            1,
            Box::new(GifSink::new(12.0).unwrap()),
        );
        let gif = run(&mut job, &mut r);
        let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&gif)).unwrap();
        use image::AnimationDecoder;
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(frames.len(), 6);
        let total_ms: u32 = frames
            .iter()
            .map(|f| {
                let (n, d) = f.delay().numer_denom_ms();
                n / d
            })
            .sum();
        assert_eq!(total_ms, 500, "GIF length must equal the loop length");
        assert_eq!(frames[0].buffer().dimensions(), (64, 36));

        let mut job = ExportJob::new(&r, p, None, 64, 36, 12.0, 2, Box::<PngZipSink>::default());
        let zip_bytes = run(&mut job, &mut r);
        let archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).unwrap();
        assert_eq!(archive.len(), 12);
    }

    #[test]
    fn motion_blur_averages_in_linear_light() {
        let mut b = crate::MotionBlur::new(2, 1.0);
        assert!(b.add(vec![0, 0, 0, 255]).is_none());
        let out = b.add(vec![255, 255, 255, 255]).unwrap();
        // Half of full linear light is sRGB 188, not 128.
        assert_eq!(out, vec![188, 188, 188, 255]);
        assert_eq!(b.offset(0), -0.25);
        assert_eq!(b.offset(1), 0.25);
        assert_eq!(crate::MotionBlur::new(1, 1.0).offset(0), 0.0);
    }

    #[test]
    fn motion_blurred_export_keeps_its_frames() {
        let Ok(gpu) = Gpu::headless() else {
            eprintln!("no GPU, skipping");
            return;
        };
        let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
        let mut p = presets::orbiting_solid();
        p.timing.bpm = 240.0;
        p.timing.loop_beats = 2;
        p.post.grade.grain = ez_core::Param::new(0.0);
        let frames = |blur: u32, r: &mut Renderer| {
            let mut job = ExportJob::new(
                r,
                p.clone(),
                None,
                64,
                36,
                12.0,
                1,
                Box::<PngZipSink>::default(),
            )
            .with_motion_blur(blur, 1.0);
            let bytes = run(&mut job, r);
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
            (0..zip.len())
                .map(|i| {
                    let mut f = zip.by_index(i).unwrap();
                    let mut v = Vec::new();
                    std::io::Read::read_to_end(&mut f, &mut v).unwrap();
                    image::load_from_memory(&v).unwrap().to_rgba8()
                })
                .collect::<Vec<_>>()
        };
        let sharp = frames(1, &mut r);
        let blurred = frames(6, &mut r);
        assert_eq!(sharp.len(), blurred.len());
        let diff: f32 = sharp[2]
            .as_raw()
            .iter()
            .zip(blurred[2].as_raw())
            .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs() as f32)
            .sum::<f32>()
            / sharp[2].as_raw().len() as f32;
        assert!(diff > 0.5, "motion blur changed nothing ({diff})");
    }

    #[test]
    fn feedback_export_starts_with_trails_and_closes_the_loop() {
        let Ok(gpu) = Gpu::headless() else {
            eprintln!("no GPU, skipping");
            return;
        };
        let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
        let mut p = presets::orbiting_solid();
        p.timing.bpm = 240.0;
        p.timing.loop_beats = 2;
        p.post.grade.grain = ez_core::Param::new(0.0);
        p.post.feedback.enabled = true;
        p.post.feedback.length = ez_core::Param::new(0.7);
        p.post.feedback.zoom = 1.3;
        let export = |p: &ez_core::Project, r: &mut Renderer| {
            let mut job = ExportJob::new(
                r,
                p.clone(),
                None,
                64,
                36,
                12.0,
                1,
                Box::<PngZipSink>::default(),
            );
            let bytes = run(&mut job, r);
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
            (0..zip.len())
                .map(|i| {
                    let mut f = zip.by_index(i).unwrap();
                    let mut v = Vec::new();
                    std::io::Read::read_to_end(&mut f, &mut v).unwrap();
                    image::load_from_memory(&v).unwrap().to_rgba8()
                })
                .collect::<Vec<_>>()
        };
        let diff = |a: &image::RgbaImage, b: &image::RgbaImage| {
            a.as_raw()
                .iter()
                .zip(b.as_raw())
                .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs() as f32)
                .sum::<f32>()
                / a.as_raw().len() as f32
        };
        let first = export(&p, &mut r);
        // A second export from a renderer with other history gives the
        // same frames: the warm-up starts from a fixed state.
        let second = export(&p, &mut r);
        assert_eq!(first.len(), 6);
        assert!(diff(&first[0], &second[0]) < 0.01);
        let mut plain = p.clone();
        plain.post.feedback.enabled = false;
        let without = export(&plain, &mut r);
        // Frame 0 already has trails (it follows the warm-up loop).
        assert!(diff(&first[0], &without[0]) > 1.0, "no trails on frame 0");
    }
}
