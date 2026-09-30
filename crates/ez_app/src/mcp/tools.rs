//! The MCP tools: presets, scene checking, rendering, saving and export.

use anyhow::{bail, Context, Result};
use ez_core::{presets, EvalCtx, Project};
use ez_export::{ExportFormat, ExportSettings};
use ez_render::gpu::Gpu;
use ez_render::{RenderTarget, Renderer};
use image::RgbaImage;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// Largest picture returned inline.
const MAX_SIDE: u32 = 1920;
/// Below this mean difference (0..255 per channel) the loop point is
/// invisible; the render tests use the same bound.
const SEAM_OK: f32 = 0.6;

/// What a tool call returns: content blocks for the model, optionally
/// structured data, and whether it failed.
pub struct ToolOutput {
    content: Vec<Value>,
    structured: Option<Value>,
    is_error: bool,
}

impl ToolOutput {
    fn text(text: impl Into<String>) -> ToolOutput {
        ToolOutput {
            content: vec![json!({ "type": "text", "text": text.into() })],
            structured: None,
            is_error: false,
        }
    }

    fn error(e: anyhow::Error) -> ToolOutput {
        ToolOutput {
            is_error: true,
            ..ToolOutput::text(format!("{e:#}"))
        }
    }

    fn with_image(mut self, png: &[u8]) -> ToolOutput {
        self.content.push(json!({
            "type": "image",
            "data": base64(png),
            "mimeType": "image/png",
        }));
        self
    }

    fn with_structured(mut self, v: Value) -> ToolOutput {
        self.structured = Some(v);
        self
    }

    pub fn into_json(self) -> Value {
        let mut r = json!({ "content": self.content });
        if let Some(s) = self.structured {
            r["structuredContent"] = s;
        }
        if self.is_error {
            r["isError"] = json!(true);
        }
        r
    }
}

/// The GPU, created on the first render and kept for the next ones.
struct Gpu3d {
    _gpu: Gpu,
    renderer: Renderer,
    targets: HashMap<(u32, u32), RenderTarget>,
}

pub struct Tools {
    gpu: Option<Gpu3d>,
}

const TOOL_NAMES: [&str; 7] = [
    "list_presets",
    "get_scene",
    "check_scene",
    "render_frame",
    "preview_loop",
    "save_scene",
    "export_loop",
];

fn scene_schema() -> Value {
    json!({
        "description": "The scene: the name of a built-in preset (e.g. \"Neon Arena\"), \
            the path of an .ez2.json project or .ez2pack file, or a whole project as a \
            JSON object (the format get_scene returns).",
        "oneOf": [{ "type": "string" }, { "type": "object" }]
    })
}

impl Tools {
    pub fn new() -> Tools {
        Tools { gpu: None }
    }

    pub fn exists(name: &str) -> bool {
        TOOL_NAMES.contains(&name)
    }

    pub fn list() -> Value {
        let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
        json!([
            {
                "name": "list_presets",
                "title": "List presets",
                "description": "Lists the built-in preset scenes with their gallery group and \
                    a one-line description. Presets are the best starting points and \
                    examples of the scene format: fetch one with get_scene.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
                "annotations": read_only,
            },
            {
                "name": "get_scene",
                "title": "Get a scene as JSON",
                "description": "Returns a scene as project JSON (the .ez2.json format), to \
                    read or to edit and pass back to the other tools. Fields left at their \
                    default are omitted.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "scene": scene_schema() },
                    "required": ["scene"],
                    "additionalProperties": false
                },
                "annotations": read_only,
            },
            {
                "name": "check_scene",
                "title": "Check a scene",
                "description": "Checks that a scene is valid without rendering it. Reports \
                    errors with the path of the field at fault, fields that were not kept \
                    (misspelt, or simply equal to the default), and a summary of the loop \
                    length and layers.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "scene": scene_schema() },
                    "required": ["scene"],
                    "additionalProperties": false
                },
                "annotations": read_only,
            },
            {
                "name": "render_frame",
                "title": "Render a frame",
                "description": "Renders one frame of a scene and returns it as a PNG image. \
                    phase is the position in the loop: 0 is the start, 0.5 halfway. The \
                    first render takes a few seconds while the GPU starts.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "scene": scene_schema(),
                        "phase": { "type": "number", "minimum": 0, "maximum": 1, "default": 0.25 },
                        "width": { "type": "integer", "minimum": 16, "maximum": MAX_SIDE, "default": 640 },
                        "height": { "type": "integer", "minimum": 16, "maximum": MAX_SIDE, "default": 360 },
                        "save_to": { "type": "string", "description": "Also write the PNG to this path." }
                    },
                    "required": ["scene"],
                    "additionalProperties": false
                },
                "annotations": { "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
            },
            {
                "name": "preview_loop",
                "title": "Preview the loop",
                "description": "Shows how a scene moves: renders frames spread evenly over the \
                    loop into one contact sheet (left to right, top to bottom), and checks \
                    that the loop closes by comparing its first frame with its last.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "scene": scene_schema(),
                        "frames": { "type": "integer", "minimum": 2, "maximum": 12, "default": 4 },
                        "width": { "type": "integer", "minimum": 16, "maximum": 640, "default": 320, "description": "Width of each frame." },
                        "height": { "type": "integer", "minimum": 16, "maximum": 640, "default": 180, "description": "Height of each frame." }
                    },
                    "required": ["scene"],
                    "additionalProperties": false
                },
                "annotations": read_only,
            },
            {
                "name": "save_scene",
                "title": "Save a scene",
                "description": "Saves a scene as an .ez2.json project file that the \
                    EZ2DEMOSCENE editor opens. Refuses to replace an existing file unless \
                    overwrite is true.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "scene": scene_schema(),
                        "path": { "type": "string", "description": "Where to save; .ez2.json is added when missing." },
                        "overwrite": { "type": "boolean", "default": false }
                    },
                    "required": ["scene", "path"],
                    "additionalProperties": false
                },
                "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false },
            },
            {
                "name": "export_loop",
                "title": "Export the loop",
                "description": "Renders the whole loop to a file: .mp4, .webm or .gif (these \
                    need ffmpeg installed), or a folder for a PNG sequence. Takes from \
                    seconds to minutes depending on size and length.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "scene": scene_schema(),
                        "output": { "type": "string", "description": "Output file (.mp4 .webm .gif) or folder." },
                        "width": { "type": "integer", "minimum": 16, "maximum": 7680, "default": 1280 },
                        "height": { "type": "integer", "minimum": 16, "maximum": 4320, "default": 720 },
                        "fps": { "type": "number", "minimum": 1, "maximum": 240, "default": 30 },
                        "repeats": { "type": "integer", "minimum": 1, "maximum": 100, "default": 1, "description": "How many times the loop plays in the file." },
                        "motion_blur": { "type": "integer", "minimum": 1, "maximum": 64, "default": 1, "description": "Samples per frame; above 1 blurs fast motion like film." }
                    },
                    "required": ["scene", "output"],
                    "additionalProperties": false
                },
                "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false },
            }
        ])
    }

    pub fn call(&mut self, name: &str, args: &Value) -> ToolOutput {
        let result = match name {
            "list_presets" => Ok(list_presets()),
            "get_scene" => get_scene(args),
            "check_scene" => Ok(check_scene(args)),
            "render_frame" => self.render_frame(args),
            "preview_loop" => self.preview_loop(args),
            "save_scene" => save_scene(args),
            "export_loop" => export_loop(args),
            _ => Err(anyhow::anyhow!("unknown tool {name}")),
        };
        result.unwrap_or_else(ToolOutput::error)
    }

    fn render(&mut self, project: &Project, phase: f32, w: u32, h: u32) -> Result<RgbaImage> {
        if self.gpu.is_none() {
            let gpu = Gpu::headless().context("no GPU available for rendering")?;
            let mut renderer = Renderer::new(&gpu.device, &gpu.queue, 4);
            // Simulations must finish before a frame is final.
            renderer.set_wait_for_bakes(true);
            self.gpu = Some(Gpu3d {
                _gpu: gpu,
                renderer,
                targets: HashMap::new(),
            });
        }
        let g = self.gpu.as_mut().expect("just created");
        let target = g
            .targets
            .entry((w, h))
            .or_insert_with(|| g.renderer.create_target(w, h));
        let ctx = EvalCtx::new(&project.timing, phase, None);
        Ok(g.renderer.render_image(project, &ctx, target))
    }

    fn render_frame(&mut self, args: &Value) -> Result<ToolOutput> {
        let project = scene_arg(args)?;
        let phase = num(args, "phase", 0.25)?;
        if !(0.0..=1.0).contains(&phase) {
            bail!("phase must be between 0 and 1");
        }
        let w = int(args, "width", 640, 16, MAX_SIDE)?;
        let h = int(args, "height", 360, 16, MAX_SIDE)?;
        let img = self.render(&project, phase as f32, w, h)?;
        let png = encode_png(&img)?;
        let mut note = format!("'{}' at phase {phase}, {w}×{h}.", project.name);
        if let Some(path) = args.get("save_to").and_then(Value::as_str) {
            std::fs::write(path, &png).with_context(|| format!("writing {path}"))?;
            note.push_str(&format!(
                " Saved to {}.",
                absolute(Path::new(path)).display()
            ));
        }
        Ok(ToolOutput::text(note).with_image(&png))
    }

    fn preview_loop(&mut self, args: &Value) -> Result<ToolOutput> {
        let project = scene_arg(args)?;
        let n = int(args, "frames", 4, 2, 12)?;
        let w = int(args, "width", 320, 16, 640)?;
        let h = int(args, "height", 180, 16, 640)?;
        // At most four across, rows as even as possible (6 frames: 3 × 2).
        let rows = n.div_ceil(4);
        let cols = n.div_ceil(rows);
        let mut sheet = RgbaImage::new(w * cols, h * rows);
        let mut first = None;
        for i in 0..n {
            let img = self.render(&project, i as f32 / n as f32, w, h)?;
            image::imageops::replace(
                &mut sheet,
                &img,
                ((i % cols) * w) as i64,
                ((i / cols) * h) as i64,
            );
            if i == 0 {
                first = Some(img);
            }
        }
        let end = self.render(&project, 1.0, w, h)?;
        let seam = mean_abs_diff(first.expect("n >= 2").as_raw(), end.as_raw());
        let loops = seam < SEAM_OK;
        let phases: Vec<String> = (0..n)
            .map(|i| format!("{:.3}", i as f32 / n as f32))
            .collect();
        let t = &project.timing;
        let note = format!(
            "'{}': {n} frames at phases {} ({} beats at {} BPM = {:.2} s per loop). \
             Loop point: {} (difference between the first and last frame {seam:.3}, \
             under {SEAM_OK} is invisible).",
            project.name,
            phases.join(", "),
            t.loop_beats,
            t.bpm,
            t.loop_seconds(),
            if loops { "seamless" } else { "VISIBLE SEAM" },
        );
        Ok(ToolOutput::text(note)
            .with_image(&encode_png(&sheet)?)
            .with_structured(json!({ "seam": seam, "seamless": loops })))
    }
}

fn list_presets() -> ToolOutput {
    let mut text = String::new();
    let mut list = Vec::new();
    for group in presets::CATEGORIES {
        text.push_str(&format!("{group}:\n"));
        for p in presets::INDEX.iter().filter(|p| p.category == group) {
            text.push_str(&format!("- {}: {}\n", p.name, p.description));
            list.push(json!({
                "name": p.name,
                "category": p.category,
                "description": p.description,
            }));
        }
    }
    ToolOutput::text(text).with_structured(json!({ "presets": list }))
}

fn get_scene(args: &Value) -> Result<ToolOutput> {
    Ok(ToolOutput::text(scene_arg(args)?.to_json()))
}

fn check_scene(args: &Value) -> ToolOutput {
    let project = match scene_arg(args) {
        Ok(p) => p,
        Err(e) => return ToolOutput::error(e),
    };
    let mut report = format!("'{}' is valid.\n", project.name);
    let t = &project.timing;
    report.push_str(&format!(
        "Loop: {} beats at {} BPM = {:.2} s.\n",
        t.loop_beats,
        t.bpm,
        t.loop_seconds()
    ));
    let layers = serde_json::to_value(&project.layers).unwrap_or_default();
    report.push_str(&format!("Layers ({}):\n", project.layers.len()));
    for l in layers.as_array().into_iter().flatten() {
        let kind = l["kind"]["type"].as_str().unwrap_or("?");
        report.push_str(&format!(
            "- {} ({kind})\n",
            l["name"].as_str().unwrap_or("?")
        ));
    }
    let mut dropped = Vec::new();
    if let Some(input) = args.get("scene").filter(|s| s.is_object()) {
        let kept = serde_json::to_value(&project).unwrap_or_default();
        not_kept(input, &kept, String::new(), &mut dropped);
    }
    if !dropped.is_empty() {
        report.push_str(
            "Not kept (misspelt or unknown fields are ignored, and fields equal to \
             their default are left out):\n",
        );
        for d in &dropped {
            report.push_str(&format!("- {d}\n"));
        }
    }
    ToolOutput::text(report).with_structured(json!({ "valid": true, "not_kept": dropped }))
}

/// Paths of fields in `input` that are missing from `kept` (the project as
/// saved): unknown names, or values equal to the default.
fn not_kept(input: &Value, kept: &Value, path: String, out: &mut Vec<String>) {
    match (input, kept) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in a {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match b.get(k) {
                    Some(w) => not_kept(v, w, p, out),
                    None => out.push(p),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (v, w)) in a.iter().zip(b).enumerate() {
                not_kept(v, w, format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

fn save_scene(args: &Value) -> Result<ToolOutput> {
    let project = scene_arg(args)?;
    let mut path = PathBuf::from(str_arg(args, "path")?);
    let ext = format!(".{}", ez_core::PROJECT_EXTENSION);
    if !path.to_string_lossy().ends_with(&ext) {
        path = PathBuf::from(format!("{}{ext}", path.display()));
    }
    if path.exists() && !args["overwrite"].as_bool().unwrap_or(false) {
        bail!(
            "{} already exists: pass overwrite: true to replace it",
            path.display()
        );
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    project
        .save(&path)
        .with_context(|| format!("saving {}", path.display()))?;
    Ok(ToolOutput::text(format!(
        "Saved '{}' to {}.",
        project.name,
        absolute(&path).display()
    )))
}

fn export_loop(args: &Value) -> Result<ToolOutput> {
    let project = scene_arg(args)?;
    let output = PathBuf::from(str_arg(args, "output")?);
    let settings = ExportSettings {
        format: ExportFormat::from_path(&output),
        width: int(args, "width", 1280, 16, 7680)?,
        height: int(args, "height", 720, 16, 4320)?,
        fps: num(args, "fps", 30.0)? as f32,
        repeats: int(args, "repeats", 1, 1, 100)?,
        motion_blur: int(args, "motion_blur", 1, 1, 64)?,
        output,
        ..Default::default()
    };
    let mut notes = Vec::new();
    let audio = match ez_export::load_music(&project) {
        Ok(a) => a,
        Err(e) => {
            notes.push(format!("The music was left out: {e:#}."));
            None
        }
    };
    let start = std::time::Instant::now();
    let mut frames = 0;
    let path = ez_export::export(
        &project,
        &settings,
        audio.as_ref(),
        |p| frames = p.total,
        &AtomicBool::new(false),
    )?;
    notes.insert(
        0,
        format!(
            "Exported '{}' to {} ({frames} frames, {}×{}, {:.1} s).",
            project.name,
            absolute(&path).display(),
            settings.width,
            settings.height,
            start.elapsed().as_secs_f32()
        ),
    );
    Ok(ToolOutput::text(notes.join(" ")))
}

/// The `scene` argument: a preset name, a file, or an inline project.
fn scene_arg(args: &Value) -> Result<Project> {
    match args.get("scene") {
        Some(Value::String(s)) => ez_export::load_scene(s).map_err(|e| match suggest(s) {
            Some(names) => anyhow::anyhow!("{e:#}. Did you mean {names}?"),
            None => anyhow::anyhow!("{e:#}. list_presets lists the presets."),
        }),
        Some(v @ Value::Object(_)) => {
            let mut p: Project = serde_path_to_error::deserialize(v)
                .map_err(|e| anyhow::anyhow!("invalid scene at {}: {}", e.path(), e.inner()))?;
            p.migrate();
            Ok(p)
        }
        Some(_) => bail!("scene must be a preset name, a file path or a project object"),
        None => bail!("missing argument: scene"),
    }
}

/// Presets whose name shares a word with `name`, quoted.
fn suggest(name: &str) -> Option<String> {
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 3)
        .map(str::to_lowercase)
        .collect();
    let close: Vec<String> = presets::INDEX
        .iter()
        .filter(|p| {
            let n = p.name.to_lowercase();
            words.iter().any(|w| n.contains(w.as_str()))
        })
        .map(|p| format!("'{}'", p.name))
        .take(5)
        .collect();
    (!close.is_empty()).then(|| close.join(" or "))
}

fn str_arg<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .with_context(|| format!("missing argument: {name}"))
}

fn num(args: &Value, name: &str, default: f64) -> Result<f64> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_f64()
            .with_context(|| format!("{name} must be a number")),
    }
}

fn int(args: &Value, name: &str, default: u32, min: u32, max: u32) -> Result<u32> {
    let v = match args.get(name) {
        None | Some(Value::Null) => return Ok(default),
        Some(v) => v
            .as_u64()
            .with_context(|| format!("{name} must be a whole number"))?,
    };
    if v < min as u64 || v > max as u64 {
        bail!("{name} must be between {min} and {max}");
    }
    Ok(v as u32)
}

fn absolute(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

fn encode_png(img: &RgbaImage) -> Result<Vec<u8>> {
    let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    rgb.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}

fn mean_abs_diff(a: &[u8], b: &[u8]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y) as f32)
        .sum::<f32>()
        / a.len().max(1) as f32
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                s.push(ABC[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn scenes_by_name_and_inline() {
        let by_name = scene_arg(&json!({ "scene": "neon arena" })).unwrap();
        let json: Value = serde_json::from_str(&by_name.to_json()).unwrap();
        let inline = scene_arg(&json!({ "scene": json })).unwrap();
        assert_eq!(inline, by_name);
        let e = scene_arg(&json!({ "scene": "Synthwave Sunset" })).unwrap_err();
        assert!(format!("{e:#}").contains("'Synth Sunset'"), "{e:#}");
        let e = scene_arg(&json!({ "scene": "Qqq" })).unwrap_err();
        assert!(format!("{e:#}").contains("list_presets"), "{e:#}");
        assert!(scene_arg(&json!({})).is_err());
    }

    #[test]
    fn check_scene_names_the_field_at_fault_and_ignored_fields() {
        let mut scene: Value =
            serde_json::from_str(&presets::named("Neon Arena").to_json()).unwrap();
        scene["timing"]["bpm"] = json!("fast");
        let out = check_scene(&json!({ "scene": scene })).into_json();
        assert_eq!(out["isError"], true);
        let text = out["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("timing.bpm"), "{text}");

        scene["timing"]["bpm"] = json!(120.0);
        scene["camera"]["zooom"] = json!(2.0);
        let out = check_scene(&json!({ "scene": scene })).into_json();
        assert_ne!(out["isError"], true);
        assert_eq!(
            out["structuredContent"]["not_kept"],
            json!(["camera.zooom"])
        );
    }

    #[test]
    fn save_scene_refuses_to_overwrite() {
        let dir = std::env::temp_dir().join(format!("ez2-mcp-{}", std::process::id()));
        let path = dir.join("test");
        let args = json!({ "scene": "Empty", "path": path.to_string_lossy() });
        assert!(save_scene(&args).is_ok());
        let saved = dir.join("test.ez2.json");
        assert!(Project::load(&saved).is_ok());
        assert!(save_scene(&args).is_err());
        let mut again = args.clone();
        again["overwrite"] = json!(true);
        assert!(save_scene(&again).is_ok());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn renders_frames_and_checks_the_loop() {
        if Gpu::headless().is_err() {
            eprintln!("skipping GPU test");
            return;
        }
        let mut tools = Tools::new();
        let out = tools
            .call(
                "render_frame",
                &json!({ "scene": "Neon Arena", "width": 64, "height": 36 }),
            )
            .into_json();
        assert_ne!(out["isError"], true, "{out}");
        assert_eq!(out["content"][1]["mimeType"], "image/png");
        assert!(out["content"][1]["data"]
            .as_str()
            .unwrap()
            .starts_with("iVBORw0KGgo"));

        let out = tools
            .call(
                "preview_loop",
                &json!({ "scene": "Neon Arena", "frames": 3, "width": 64, "height": 36 }),
            )
            .into_json();
        assert_ne!(out["isError"], true, "{out}");
        assert_eq!(out["structuredContent"]["seamless"], true, "{out}");
    }
}
