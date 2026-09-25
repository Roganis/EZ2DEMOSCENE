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
    pending: Option<Readback>,
    frames: u32,
    total: u32,
    next: u32,
    done: u32,
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
        let frames = project.timing.frame_count(fps);
        ExportJob {
            target: renderer.create_target(width.max(16) & !1, height.max(16) & !1),
            project,
            audio,
            sink: Some(sink),
            pending: None,
            frames,
            total: frames * repeats.max(1),
            next: 0,
            done: 0,
        }
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
        if let Some(rb) = &self.pending {
            renderer.poll();
            if !rb.is_ready() {
                return Ok(JobState::Running(self.progress()));
            }
            let rb = self.pending.take().unwrap();
            let (w, h) = (rb.width, rb.height);
            let Some(px) = rb.take() else {
                bail!("reading back a frame failed");
            };
            if let Some(sink) = &mut self.sink {
                sink.add_frame(&px, w, h)?;
            }
            self.done += 1;
        }
        if self.next < self.total {
            // Frame i sits at phase i / N: the last frame is not a copy of
            // the first, so the file loops without a seam.
            let phase = (self.next % self.frames) as f32 / self.frames as f32;
            let ctx = EvalCtx::new(&self.project.timing, phase, self.audio.as_ref());
            renderer.render(&self.project, &ctx, &self.target);
            self.pending = Some(renderer.start_readback(&self.target));
            self.next += 1;
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
}
