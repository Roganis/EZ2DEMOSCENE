//! Importing animations (GIFs and videos) as frame sheets. Decoding takes
//! a moment, so it runs in the background (a thread on desktop, a task in
//! the browser) and results come back through [`take_done`].
//!
//! Desktop builds read videos with ffmpeg (bundled with release builds,
//! otherwise the one on the PATH); the browser decodes them itself (see
//! `web/ez2_clip.js`).

use crate::platform::Picked;
use ez_core::FrameSheet;
use ez_render::clip::{self, Frames};
use image::RgbaImage;
use std::sync::Mutex;

/// Video files that can be imported as animations.
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "webm", "mov", "mkv", "m4v", "avi", "ogv"];
/// Only the start of a longer video is used.
pub const MAX_SECONDS: f32 = 30.0;
/// Frames per second taken from videos (fewer for long ones).
pub const MAX_FPS: f32 = 24.0;

/// A finished import.
pub struct Done {
    pub picked: Picked,
    /// The frame sheet as a PNG file, or `None` for a GIF with a single
    /// frame (use the file as a still picture).
    pub result: Result<Option<(Vec<u8>, FrameSheet)>, String>,
}

static DONE: Mutex<Vec<Done>> = Mutex::new(Vec::new());

/// True for a file that is (or may be) an animation.
pub fn is_animation(ext: &str) -> bool {
    ext == "gif" || VIDEO_EXTENSIONS.contains(&ext)
}

/// Imports finished since the last call.
pub fn take_done() -> Vec<Done> {
    std::mem::take(&mut *DONE.lock().unwrap())
}

fn finish(
    picked: Picked,
    result: Result<Option<(RgbaImage, FrameSheet)>, String>,
    ctx: &egui::Context,
) {
    // Encoded here, off the editor's thread (a big sheet takes a moment).
    let result = result.and_then(|r| {
        r.map(|(sheet, clip)| {
            let mut png = Vec::new();
            sheet
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .map(|_| (png, clip))
                .map_err(|e| e.to_string())
        })
        .transpose()
    });
    DONE.lock().unwrap().push(Done { picked, result });
    ctx.request_repaint();
}

fn sheet(frames: Frames) -> Result<Option<(RgbaImage, FrameSheet)>, String> {
    clip::build_sheet(frames)
        .map(Some)
        .map_err(|e| format!("{e:#}"))
}

fn from_gif(path: &str) -> Result<Option<(RgbaImage, FrameSheet)>, String> {
    let bytes = ez_core::store::read(path).map_err(|e| e.to_string())?;
    match clip::decode_gif(&bytes).map_err(|e| format!("{e:#}"))? {
        Some(frames) => sheet(frames),
        None => Ok(None),
    }
}

/// Start turning a picked GIF or video into a frame sheet.
pub fn start(picked: Picked, ctx: &egui::Context) {
    let ctx = ctx.clone();
    let ext = ez_core::store::extension(&picked.path);
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(move || {
        let result = if ext == "gif" {
            from_gif(&picked.path)
        } else {
            video_ffmpeg(&picked.path).and_then(sheet)
        };
        finish(picked, result, &ctx);
    });
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(async move {
        let result = if ext == "gif" {
            from_gif(&picked.path)
        } else {
            match video_browser(&picked.path).await {
                Ok(frames) => sheet(frames),
                Err(e) => Err(e),
            }
        };
        finish(picked, result, &ctx);
    });
}

/// Frame size scaled to fit [`clip::MAX_FRAME_SIDE`], in even numbers.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn fit(w: u32, h: u32) -> (u32, u32) {
    let k = (clip::MAX_FRAME_SIDE as f32 / w.max(h) as f32).min(1.0);
    let even = |v: f32| ((v / 2.0).round() as u32 * 2).max(2);
    (even(w as f32 * k), even(h as f32 * k))
}

/// Frame rate and length to take from a video `seconds` long.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn pace(seconds: f32) -> (f32, f32) {
    let len = seconds.clamp(0.1, MAX_SECONDS);
    let fps = MAX_FPS.min(clip::MAX_FRAMES as f32 / len);
    (fps, len)
}

/// What `ffmpeg -i` says about a video: its size (as shown, after
/// rotation) and length.
#[cfg(not(target_arch = "wasm32"))]
fn probe(info: &str) -> Option<(u32, u32, f32)> {
    let video = info.lines().find(|l| l.contains("Video:"))?;
    let (w, h) = video
        .split([',', ' '])
        .filter_map(|t| {
            let (a, b) = t.split_once('x')?;
            Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?))
        })
        .find(|(a, b)| *a > 0 && *b > 0)?;
    let d = info.split("Duration: ").nth(1)?.split(',').next()?;
    let mut secs = 0.0f32;
    for part in d.trim().split(':') {
        secs = secs * 60.0 + part.parse::<f32>().ok()?;
    }
    // Phone videos turned on their side are shown upright.
    let turned = info
        .split("rotation of ")
        .nth(1)
        .and_then(|r| r.split_whitespace().next())
        .and_then(|a| a.parse::<f32>().ok())
        .is_some_and(|a| (a.abs().round() as i32) % 180 == 90);
    Some(if turned { (h, w, secs) } else { (w, h, secs) })
}

#[cfg(not(target_arch = "wasm32"))]
fn video_ffmpeg(path: &str) -> Result<Frames, String> {
    use std::process::{Command, Stdio};
    let ffmpeg = ez_export::find_ffmpeg(None)
        .ok_or("reading videos needs ffmpeg (install it, or use a GIF)")?;
    let info = Command::new(&ffmpeg)
        .args(["-hide_banner", "-i", path])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    let info = String::from_utf8_lossy(&info.stderr);
    let (w, h, secs) = probe(&info).ok_or("no video in this file")?;
    let (fw, fh) = fit(w, h);
    let (fps, len) = pace(if secs > 0.0 { secs } else { MAX_SECONDS });
    let out = Command::new(&ffmpeg)
        .args(["-v", "error", "-i", path, "-t"])
        .arg(format!("{len}"))
        .arg("-vf")
        .arg(format!("fps={fps},scale={fw}:{fh}:flags=area"))
        .args(["-an", "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() && out.stdout.is_empty() {
        return Err(format!(
            "ffmpeg: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let count = out.stdout.len() / (fw * fh * 4) as usize;
    if count == 0 {
        return Err("no frames in this video".into());
    }
    clip::frames_from_rgba(&out.stdout, fw, fh, count as f32 / fps).map_err(|e| format!("{e:#}"))
}

#[cfg(target_arch = "wasm32")]
async fn video_browser(path: &str) -> Result<Frames, String> {
    use wasm_bindgen::JsCast;
    let bytes = ez_core::store::read(path).map_err(|e| e.to_string())?;
    let global = js_sys::global();
    let f = js_sys::Reflect::get(&global, &"ez2DecodeVideo".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .ok_or("ez2_clip.js is missing")?;
    let args = js_sys::Array::new();
    args.push(&js_sys::Uint8Array::from(&bytes[..]).into());
    args.push(&(clip::MAX_FRAME_SIDE as f64).into());
    args.push(&(clip::MAX_FRAMES as f64).into());
    args.push(&(MAX_FPS as f64).into());
    args.push(&(MAX_SECONDS as f64).into());
    let promise = f
        .apply(&wasm_bindgen::JsValue::NULL, &args)
        .map_err(|e| format!("{e:?}"))?
        .dyn_into::<js_sys::Promise>()
        .map_err(|_| "ez2DecodeVideo returned no promise")?;
    let out = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| {
            e.dyn_ref::<js_sys::Error>()
                .map(|e| String::from(e.message()))
                .unwrap_or_else(|| format!("{e:?}"))
        })?;
    let get = |k: &str| js_sys::Reflect::get(&out, &k.into()).map_err(|e| format!("{e:?}"));
    let num = |k: &str| get(k).map(|v| v.as_f64().unwrap_or(0.0));
    let (w, h, seconds) = (num("width")? as u32, num("height")? as u32, num("seconds")?);
    let pixels = js_sys::Uint8Array::new(&get("pixels")?).to_vec();
    clip::frames_from_rgba(&pixels, w, h, seconds as f32).map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_pace_and_size() {
        assert_eq!(fit(1920, 1080), (320, 180));
        assert_eq!(fit(100, 50), (100, 50));
        let (fps, len) = pace(5.0);
        assert_eq!((fps, len), (24.0, 5.0));
        let (fps, len) = pace(120.0);
        assert_eq!(len, MAX_SECONDS);
        assert!(fps * len <= clip::MAX_FRAMES as f32 + 0.5);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn ffmpeg_info_is_read() {
        let info = "  Duration: 00:01:02.50, start: 0.000000, bitrate: 1205 kb/s\n\
            Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(tv, bt709, progressive), 1920x1080 [SAR 1:1 DAR 16:9], 1070 kb/s, 30 fps\n";
        assert_eq!(probe(info), Some((1920, 1080, 62.5)));
        let turned = format!("{info}      displaymatrix: rotation of -90.00 degrees\n");
        assert_eq!(probe(&turned), Some((1080, 1920, 62.5)));
    }

    /// A real video through ffmpeg, when it is installed.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn videos_decode_with_ffmpeg() {
        let Some(ffmpeg) = ez_export::find_ffmpeg(None) else {
            eprintln!("no ffmpeg, skipping");
            return;
        };
        let dir = std::env::temp_dir().join(format!("ez2-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("test.mp4");
        let ok = std::process::Command::new(ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg("testsrc=size=640x360:rate=25:duration=2")
            .args(["-pix_fmt", "yuv420p"])
            .arg(&file)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let frames = video_ffmpeg(&file.to_string_lossy()).unwrap();
        assert_eq!(frames.images.len(), 48, "2 s at 24 fps");
        assert_eq!(frames.images[0].dimensions(), (320, 180));
        assert!((frames.seconds - 2.0).abs() < 0.05);
        let (sheet, clip) = clip::build_sheet(frames).unwrap();
        assert_eq!(clip.frames, 48);
        assert!(sheet.width() <= clip::MAX_SHEET);
        std::fs::remove_dir_all(&dir).ok();
    }
}
