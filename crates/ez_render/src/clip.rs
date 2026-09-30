//! Animated pictures (GIFs and videos) as frame sheets: every frame in a
//! grid in one picture, which the renderer plays like a sprite sheet.

use anyhow::{bail, Context, Result};
use ez_core::FrameSheet;
use image::{imageops, RgbaImage};

/// The largest frame sheet (most devices take textures this big).
pub const MAX_SHEET: u32 = 4096;
/// At most this many frames are kept (longer clips are thinned out).
pub const MAX_FRAMES: usize = 240;
/// Frames are scaled down to at most this many pixels on their longer side.
pub const MAX_FRAME_SIDE: u32 = 320;

/// Decoded frames at an even pace.
pub struct Frames {
    pub images: Vec<RgbaImage>,
    /// Length of one pass through them.
    pub seconds: f32,
}

/// The frames of a GIF, at an even pace: frames shown for longer are
/// repeated. `None` for a GIF with a single frame (a still picture).
pub fn decode_gif(bytes: &[u8]) -> Result<Option<Frames>> {
    use image::AnimationDecoder;
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes))
        .context("reading the GIF")?;
    let mut images = Vec::new();
    let mut delays = Vec::new();
    for frame in decoder.into_frames() {
        let frame = frame.context("reading a GIF frame")?;
        let (n, d) = frame.delay().numer_denom_ms();
        // Browsers show delays under 20 ms as 100 ms.
        let ms = n as f32 / d.max(1) as f32;
        delays.push(if ms < 20.0 { 100.0 } else { ms });
        images.push(frame.into_buffer());
        if images.len() > 4 * MAX_FRAMES {
            break;
        }
    }
    if images.len() < 2 {
        return Ok(None);
    }
    let total: f32 = delays.iter().sum();
    let step = delays.iter().cloned().fold(f32::MAX, f32::min).max(20.0);
    let even = delays.iter().all(|d| (d - delays[0]).abs() < 1.0);
    let images = if even {
        images
    } else {
        // Sample the timeline at the shortest delay.
        let count = ((total / step).round() as usize).clamp(2, MAX_FRAMES * 4);
        let mut out = Vec::with_capacity(count);
        let (mut k, mut end) = (0, delays[0]);
        for i in 0..count {
            let t = (i as f32 + 0.5) * total / count as f32;
            while t >= end && k + 1 < images.len() {
                k += 1;
                end += delays[k];
            }
            out.push(images[k].clone());
        }
        out
    };
    Ok(Some(Frames {
        images,
        seconds: total / 1000.0,
    }))
}

/// Lay frames out in a grid no bigger than [`MAX_SHEET`]: thinned out to
/// [`MAX_FRAMES`] and scaled down as needed.
pub fn build_sheet(frames: Frames) -> Result<(RgbaImage, FrameSheet)> {
    let Frames { images, seconds } = frames;
    if images.is_empty() {
        bail!("no frames");
    }
    let images: Vec<RgbaImage> = if images.len() > MAX_FRAMES {
        (0..MAX_FRAMES)
            .map(|i| images[i * images.len() / MAX_FRAMES].clone())
            .collect()
    } else {
        images
    };
    let n = images.len() as u32;
    let (w0, h0) = images[0].dimensions();
    if w0 == 0 || h0 == 0 {
        bail!("empty frames");
    }
    // Shrink until the grid fits.
    let mut scale = (MAX_FRAME_SIDE as f32 / w0.max(h0) as f32).min(1.0);
    let (fw, fh, cols, rows) = loop {
        let fw = ((w0 as f32 * scale) as u32).max(1);
        let fh = ((h0 as f32 * scale) as u32).max(1);
        // Prefer a squarish sheet.
        let ideal = ((n as f32 * fh as f32 / fw as f32).sqrt().ceil() as u32).clamp(1, n);
        let cols = ideal.min(MAX_SHEET / fw).max(1);
        let rows = n.div_ceil(cols);
        if cols * fw <= MAX_SHEET && rows * fh <= MAX_SHEET {
            break (fw, fh, cols, rows);
        }
        scale *= 0.9;
    };
    let mut sheet = RgbaImage::new(cols * fw, rows * fh);
    for (i, img) in images.iter().enumerate() {
        let img = if img.dimensions() == (fw, fh) {
            img.clone()
        } else {
            imageops::resize(img, fw, fh, imageops::FilterType::Triangle)
        };
        let (c, r) = (i as u32 % cols, i as u32 / cols);
        imageops::replace(&mut sheet, &img, (c * fw) as i64, (r * fh) as i64);
    }
    Ok((
        sheet,
        FrameSheet {
            columns: cols,
            rows,
            frames: n,
            seconds: seconds.max(0.05),
        },
    ))
}

/// Frame `k` cut out of a frame sheet.
pub fn frame(sheet: &RgbaImage, clip: &FrameSheet, k: u32) -> RgbaImage {
    let fw = (sheet.width() / clip.columns.max(1)).max(1);
    let fh = (sheet.height() / clip.rows.max(1)).max(1);
    let k = k.min(clip.frames.max(1) - 1);
    let (c, r) = (k % clip.columns.max(1), k / clip.columns.max(1));
    imageops::crop_imm(sheet, c * fw, r * fh, fw, fh).to_image()
}

/// Raw RGBA frames (`count` of `w` × `h`, one after the other) as
/// [`Frames`] lasting `seconds`.
pub fn frames_from_rgba(pixels: &[u8], w: u32, h: u32, seconds: f32) -> Result<Frames> {
    let size = (w * h * 4) as usize;
    if size == 0 || pixels.len() < size {
        bail!("no frames");
    }
    let images = pixels
        .chunks_exact(size)
        .filter_map(|c| RgbaImage::from_raw(w, h, c.to_vec()))
        .collect();
    Ok(Frames { images, seconds })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gif(frames: &[(u8, u16)]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            for (shade, ms) in frames {
                let img = RgbaImage::from_pixel(8, 6, image::Rgba([*shade, 0, 0, 255]));
                let delay = image::Delay::from_numer_denom_ms(*ms as u32, 1);
                enc.encode_frame(image::Frame::from_parts(img, 0, 0, delay))
                    .unwrap();
            }
        }
        out
    }

    #[test]
    fn gif_frames_become_a_sheet() {
        let f = decode_gif(&gif(&[(0, 100), (80, 100), (160, 100), (240, 100)]))
            .unwrap()
            .unwrap();
        assert_eq!(f.images.len(), 4);
        assert!((f.seconds - 0.4).abs() < 1e-3);
        let (sheet, clip) = build_sheet(f).unwrap();
        assert_eq!(clip.frames, 4);
        assert_eq!(sheet.width(), clip.columns * 8);
        assert_eq!(sheet.height(), clip.rows * 6);
        assert_eq!(frame(&sheet, &clip, 2).get_pixel(3, 3)[0], 160);
        // A still GIF is a picture.
        assert!(decode_gif(&gif(&[(10, 100)])).unwrap().is_none());
    }

    #[test]
    fn uneven_gif_delays_are_evened_out() {
        let f = decode_gif(&gif(&[(0, 100), (200, 300)])).unwrap().unwrap();
        assert_eq!(f.images.len(), 4, "shown 1 + 3 steps of 100 ms");
        assert_eq!(f.images[3].get_pixel(0, 0)[0], 200);
    }

    #[test]
    fn big_clips_fit_the_sheet() {
        // More frames than are kept, and tall ones.
        let img = RgbaImage::new(90, 320);
        let f = Frames {
            images: vec![img; 300],
            seconds: 10.0,
        };
        let (sheet, clip) = build_sheet(f).unwrap();
        assert_eq!(clip.frames as usize, MAX_FRAMES);
        assert!(sheet.width() <= MAX_SHEET && sheet.height() <= MAX_SHEET);
        assert!(clip.columns * clip.rows >= clip.frames);
    }
}
