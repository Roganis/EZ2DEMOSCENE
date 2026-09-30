//! The MCP tools: presets, scene checking, rendering, saving and export.

use anyhow::{bail, Context, Result};
use ez_core::{presets, EvalCtx, LayerKind, Project};
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
    schema: Option<Value>,
}

const TOOL_NAMES: [&str; 9] = [
    "list_presets",
    "scene_schema",
    "get_scene",
    "check_scene",
    "render_frame",
    "preview_loop",
    "locate",
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
        Tools {
            gpu: None,
            schema: None,
        }
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
                "name": "scene_schema",
                "title": "Scene format",
                "description": "Documents the scene (project JSON) format from its JSON \
                    Schema. Without arguments: the top-level fields and an index of every \
                    type with a one-line summary. With type: that type's schema (fields, \
                    allowed values, defaults and descriptions) and the types it refers to. \
                    full: true returns the whole schema (large).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "description": "A type name from the index, e.g. \"MeshLayer\" or \"Material\"." },
                        "full": { "type": "boolean", "default": false }
                    },
                    "additionalProperties": false
                },
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
                "name": "locate",
                "title": "Locate layers on screen",
                "description": "Where layers show on the picture, in the units logos use \
                    (x across and y up, fractions from the bottom-left corner): for each 3D \
                    layer the box its shapes cover and the point its position shows at, for \
                    each logo its rectangle, and which of them overlap. With sweep, over the \
                    whole loop (the camera and layers move): the area each one ever covers and \
                    the overlaps at any moment. Also projects world points. To pin a logo to a \
                    3D layer instead, set the logo's attach_to to that layer's name.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "scene": scene_schema(),
                        "phase": { "type": "number", "minimum": 0, "maximum": 1, "default": 0.25 },
                        "sweep": { "type": "integer", "minimum": 2, "maximum": 64, "description": "Check this many moments spread over the loop instead of one phase." },
                        "width": { "type": "integer", "minimum": 16, "maximum": 7680, "default": 640, "description": "Picture size: only its shape (width / height) matters; use the export's." },
                        "height": { "type": "integer", "minimum": 16, "maximum": 4320, "default": 480 },
                        "layers": { "type": "array", "items": { "type": "string" }, "description": "Only these layers (default: every layer except backgrounds, floors, landscapes and weather)." },
                        "points": { "type": "array", "items": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 }, "description": "World points [x, y, z] to project." }
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
            "scene_schema" => self.scene_schema(args),
            "get_scene" => get_scene(args),
            "check_scene" => Ok(check_scene(args)),
            "render_frame" => self.render_frame(args),
            "preview_loop" => self.preview_loop(args),
            "save_scene" => save_scene(args),
            "export_loop" => export_loop(args),
            "locate" => self.locate(args),
            _ => Err(anyhow::anyhow!("unknown tool {name}")),
        };
        result.unwrap_or_else(ToolOutput::error)
    }

    fn scene_schema(&mut self, args: &Value) -> Result<ToolOutput> {
        let schema = self.schema.get_or_insert_with(Project::json_schema);
        if args["full"].as_bool().unwrap_or(false) {
            return Ok(ToolOutput::text(serde_json::to_string_pretty(schema)?));
        }
        let defs = schema["$defs"]
            .as_object()
            .context("schema without $defs")?;
        let Some(name) = args.get("type").and_then(Value::as_str) else {
            return Ok(ToolOutput::text(schema_overview(schema, defs)));
        };
        if name.eq_ignore_ascii_case("Project") {
            return Ok(ToolOutput::text(schema_overview(schema, defs)));
        }
        let Some((name, def)) = defs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)) else {
            let lower = name.to_lowercase();
            let close: Vec<&str> = defs
                .keys()
                .filter(|k| {
                    let k = k.to_lowercase();
                    k.contains(&lower) || lower.contains(&k)
                })
                .map(String::as_str)
                .take(8)
                .collect();
            if close.is_empty() {
                bail!("no type named {name}: scene_schema without arguments lists them");
            }
            bail!("no type named {name}. Close: {}.", close.join(", "));
        };
        let mut refs = Vec::new();
        collect_refs(def, &mut refs);
        refs.retain(|r| r != name);
        refs.dedup();
        let mut text = format!("{name}:\n{}", serde_json::to_string_pretty(def)?);
        if !refs.is_empty() {
            text.push_str(&format!("\n\nRefers to: {}.", refs.join(", ")));
        }
        Ok(ToolOutput::text(text))
    }

    /// The GPU and renderer, started on first use.
    fn gpu(&mut self) -> Result<&mut Gpu3d> {
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
        Ok(self.gpu.as_mut().expect("just created"))
    }

    fn render(&mut self, project: &Project, phase: f32, w: u32, h: u32) -> Result<RgbaImage> {
        let g = self.gpu()?;
        let target = g
            .targets
            .entry((w, h))
            .or_insert_with(|| g.renderer.create_target(w, h));
        let ctx = EvalCtx::new(&project.timing, phase, None);
        Ok(g.renderer.render_image(project, &ctx, target))
    }

    fn locate(&mut self, args: &Value) -> Result<ToolOutput> {
        use ez_core::screen::{layer_box, project_point};
        let project = scene_arg(args)?;
        let w = int(args, "width", 640, 16, 7680)?;
        let h = int(args, "height", 480, 16, 4320)?;
        let aspect = w as f32 / h as f32;
        let phases: Vec<f32> = match args.get("sweep").filter(|v| !v.is_null()) {
            Some(_) => {
                let n = int(args, "sweep", 16, 2, 64)?;
                (0..n).map(|i| i as f32 / n as f32).collect()
            }
            None => {
                let phase = num(args, "phase", 0.25)?;
                if !(0.0..=1.0).contains(&phase) {
                    bail!("phase must be between 0 and 1");
                }
                vec![phase as f32]
            }
        };
        let wanted: Option<Vec<String>> = match args.get("layers") {
            Some(Value::Array(a)) => Some(
                a.iter()
                    .map(|v| v.as_str().map(String::from).context("layers are names"))
                    .collect::<Result<_>>()?,
            ),
            _ => None,
        };
        let points: Vec<glam::Vec3> = match args.get("points") {
            Some(Value::Array(a)) => a
                .iter()
                .map(|p| {
                    let v: Vec<f32> = p
                        .as_array()
                        .filter(|c| c.len() == 3)
                        .context("each point is [x, y, z]")?
                        .iter()
                        .map(|c| c.as_f64().map(|f| f as f32).context("numbers"))
                        .collect::<Result<_>>()?;
                    Ok(glam::Vec3::new(v[0], v[1], v[2]))
                })
                .collect::<Result<_>>()?,
            _ => Vec::new(),
        };
        // Per layer: kind, the area it ever covers, how many moments it
        // shows, where its middle is at the first of those.
        struct Seen {
            kind: String,
            area: Option<[f32; 4]>,
            shown: usize,
            centre: Option<[f32; 2]>,
            depth: f32,
        }
        let mut seen: Vec<(String, Seen)> = Vec::new();
        let mut overlaps: Vec<(String, String, f32, Vec<f32>)> = Vec::new();
        let mut spots: Vec<Vec<Option<[f32; 3]>>> = vec![Vec::new(); points.len()];
        let renderer = &mut self.gpu()?.renderer;
        for &phase in &phases {
            let ctx = EvalCtx::new(&project.timing, phase, None);
            let (scene, ctx) = project.shown_at(&ctx);
            let layers = scene.scene_layers(&ctx);
            let cam = scene.camera.eval(&ctx);
            let logos = renderer.logo_rects(&scene, &ctx, [w as f32, h as f32]);
            let mut now: Vec<(String, [f32; 4])> = Vec::new();
            for (i, l) in layers.iter().enumerate() {
                // Backgrounds, floors, landscapes and weather fill the view
                // rather than sit somewhere in it.
                let background = matches!(
                    l.kind,
                    LayerKind::Backdrop(_)
                        | LayerKind::Mirror(_)
                        | LayerKind::Mode7(_)
                        | LayerKind::Terrain(_)
                        | LayerKind::Weather(_)
                );
                let listed = match &wanted {
                    Some(names) => names.contains(&l.name),
                    None => l.enabled && !background,
                };
                if !listed {
                    continue;
                }
                let kind = serde_json::to_value(&l.kind).ok();
                let kind = kind
                    .as_ref()
                    .and_then(|k| k["type"].as_str())
                    .unwrap_or("?")
                    .to_string();
                let (rect, centre, depth) = if !l.enabled {
                    (None, None, 0.0)
                } else if let LayerKind::Logo(g) = &l.kind {
                    // A see-through logo (fading in later) takes no room.
                    let visible = g.opacity.eval(&ctx) > 0.02;
                    let r = logos.get(i).copied().flatten().filter(|_| visible);
                    (
                        r,
                        r.map(|r| [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0]),
                        0.0,
                    )
                } else {
                    match layer_box(l, &ctx, &cam, aspect) {
                        Some(b) => (
                            Some([b.min[0], b.min[1], b.max[0], b.max[1]]),
                            Some(b.centre),
                            b.depth,
                        ),
                        None => (None, None, 0.0),
                    }
                };
                let entry = match seen.iter().position(|(n, _)| *n == l.name) {
                    Some(k) => &mut seen[k].1,
                    None => {
                        seen.push((
                            l.name.clone(),
                            Seen {
                                kind,
                                area: None,
                                shown: 0,
                                centre: None,
                                depth: 0.0,
                            },
                        ));
                        &mut seen.last_mut().expect("just pushed").1
                    }
                };
                if let Some(r) =
                    rect.filter(|r| r[2] > 0.0 && r[0] < 1.0 && r[3] > 0.0 && r[1] < 1.0)
                {
                    entry.shown += 1;
                    entry.area = Some(match entry.area {
                        None => r,
                        Some(a) => [
                            a[0].min(r[0]),
                            a[1].min(r[1]),
                            a[2].max(r[2]),
                            a[3].max(r[3]),
                        ],
                    });
                    if entry.centre.is_none() {
                        entry.centre = centre;
                        entry.depth = depth;
                    }
                    now.push((l.name.clone(), r));
                }
            }
            // Overlaps on the picture (clipped to it), a logo involved.
            for a in 0..now.len() {
                for b in a + 1..now.len() {
                    let (ra, rb) = (now[a].1, now[b].1);
                    let x = (ra[2].min(rb[2]).min(1.0) - ra[0].max(rb[0]).max(0.0)).max(0.0);
                    let y = (ra[3].min(rb[3]).min(1.0) - ra[1].max(rb[1]).max(0.0)).max(0.0);
                    let is_logo = |n: &str| {
                        layers
                            .iter()
                            .any(|l| l.name == n && matches!(l.kind, LayerKind::Logo(_)))
                    };
                    // Less than a thousandth of the picture: touching.
                    if x * y < 1e-3 || !(is_logo(&now[a].0) || is_logo(&now[b].0)) {
                        continue;
                    }
                    let key = (now[a].0.clone(), now[b].0.clone());
                    match overlaps
                        .iter_mut()
                        .find(|o| (o.0.clone(), o.1.clone()) == key)
                    {
                        Some(o) => {
                            o.2 = o.2.max(x * y);
                            o.3.push(phase);
                        }
                        None => overlaps.push((key.0, key.1, x * y, vec![phase])),
                    }
                }
            }
            for (k, p) in points.iter().enumerate() {
                spots[k].push(project_point(&cam, aspect, *p).map(|s| [s.x, s.y, s.depth]));
            }
        }
        let sweep = phases.len() > 1;
        let f = |v: f32| format!("{v:.3}");
        let mut text = if sweep {
            format!(
                "'{}' over {} moments of the loop, on a {w}×{h} picture (x across, y up, \
                 fractions from the bottom left):\n",
                project.name,
                phases.len()
            )
        } else {
            format!(
                "'{}' at phase {}, on a {w}×{h} picture (x across, y up, fractions from the \
                 bottom left):\n",
                project.name, phases[0]
            )
        };
        let mut layers_json = Vec::new();
        for (name, s) in &seen {
            match s.area {
                Some(a) => {
                    let c = s.centre.unwrap_or([0.0; 2]);
                    text.push_str(&format!(
                        "- {name} ({}): {} x {}..{}, y {}..{}; middle ({}, {}){}{}\n",
                        s.kind,
                        if sweep { "covers" } else { "box" },
                        f(a[0]),
                        f(a[2]),
                        f(a[1]),
                        f(a[3]),
                        f(c[0]),
                        f(c[1]),
                        if s.depth > 0.0 {
                            format!(", {:.1} away", s.depth)
                        } else {
                            String::new()
                        },
                        if sweep && s.shown < phases.len() {
                            format!(", on screen at {} of {} moments", s.shown, phases.len())
                        } else {
                            String::new()
                        }
                    ));
                }
                None => text.push_str(&format!("- {name} ({}): not on screen\n", s.kind)),
            }
            layers_json.push(json!({
                "name": name,
                "kind": s.kind,
                "box": s.area.map(|a| json!({ "min": [a[0], a[1]], "max": [a[2], a[3]] })),
                "middle": s.centre,
                "depth": s.depth,
                "shown": s.shown,
            }));
        }
        if let Some(missing) = wanted.as_ref().map(|names| {
            names
                .iter()
                .filter(|n| !seen.iter().any(|(s, _)| s == *n))
                .cloned()
                .collect::<Vec<_>>()
        }) {
            for n in &missing {
                text.push_str(&format!("- {n}: no layer with this name\n"));
            }
        }
        if overlaps.is_empty() {
            text.push_str("No logo overlaps another layer.\n");
        } else {
            text.push_str("Overlaps (logo rectangles against the others' boxes):\n");
            for (a, b, area, when) in &overlaps {
                text.push_str(&format!(
                    "- {a} and {b}: up to {:.1}% of the picture{}\n",
                    area * 100.0,
                    if sweep {
                        format!(", at {} of {} moments", when.len(), phases.len())
                    } else {
                        String::new()
                    }
                ));
            }
        }
        for (k, p) in points.iter().enumerate() {
            let shown: Vec<[f32; 3]> = spots[k].iter().flatten().copied().collect();
            match shown.first() {
                Some(s) if !sweep => text.push_str(&format!(
                    "- point [{}, {}, {}]: ({}, {}), {:.1} away\n",
                    p.x,
                    p.y,
                    p.z,
                    f(s[0]),
                    f(s[1]),
                    s[2]
                )),
                Some(_) => {
                    let lo = shown
                        .iter()
                        .fold([f32::MAX; 2], |m, s| [m[0].min(s[0]), m[1].min(s[1])]);
                    let hi = shown
                        .iter()
                        .fold([f32::MIN; 2], |m, s| [m[0].max(s[0]), m[1].max(s[1])]);
                    text.push_str(&format!(
                        "- point [{}, {}, {}]: x {}..{}, y {}..{}\n",
                        p.x,
                        p.y,
                        p.z,
                        f(lo[0]),
                        f(hi[0]),
                        f(lo[1]),
                        f(hi[1])
                    ));
                }
                None => text.push_str(&format!(
                    "- point [{}, {}, {}]: behind the camera\n",
                    p.x, p.y, p.z
                )),
            }
        }
        let overlaps_json: Vec<Value> = overlaps
            .iter()
            .map(|(a, b, area, when)| json!({ "a": a, "b": b, "area": area, "phases": when }))
            .collect();
        let points_json: Vec<Value> = spots.iter().map(|s| json!(s)).collect();
        Ok(ToolOutput::text(text).with_structured(json!({
            "layers": layers_json,
            "overlaps": overlaps_json,
            "points": points_json,
        })))
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

/// The top-level fields and a one-line index of every type.
fn schema_overview(schema: &Value, defs: &serde_json::Map<String, Value>) -> String {
    let mut text = String::from(
        "The scene (project) format, from its JSON Schema. Fields that are left out \
         take their default. Ask for any type below with scene_schema {\"type\": \"Name\"}.\n\n\
         Top-level fields of a project:\n",
    );
    for (k, v) in schema["properties"].as_object().into_iter().flatten() {
        text.push_str(&format!("- {k} ({})", type_of(v)));
        if let Some(d) = v["description"].as_str() {
            text.push_str(&format!(": {}", first_sentence(d)));
        }
        text.push('\n');
    }
    text.push_str("\nTypes:\n");
    for (k, v) in defs {
        text.push_str(&format!("- {k}"));
        if let Some(d) = v["description"].as_str() {
            text.push_str(&format!(": {}", first_sentence(d)));
        }
        text.push('\n');
    }
    text
}

/// A short name for the type of a property schema.
fn type_of(v: &Value) -> String {
    if let Some(r) = v["$ref"].as_str() {
        return r.rsplit('/').next().unwrap_or(r).to_string();
    }
    if let Some(alts) = v["anyOf"].as_array().or(v["oneOf"].as_array()) {
        let names: Vec<String> = alts.iter().map(type_of).collect();
        return names.join(" or ");
    }
    match &v["type"] {
        Value::String(t) if t == "array" => format!("array of {}", type_of(&v["items"])),
        Value::String(t) => t.clone(),
        Value::Array(ts) => ts
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" or "),
        _ => "value".into(),
    }
}

fn first_sentence(d: &str) -> String {
    let d = d.split("\n\n").next().unwrap_or(d).replace('\n', " ");
    let end = d.find(". ").map(|i| i + 1).unwrap_or(d.len());
    let s = &d[..end];
    if s.chars().count() > 140 {
        let cut: String = s.chars().take(137).collect();
        format!("{cut}…")
    } else {
        s.to_string()
    }
}

/// Names of the types a schema refers to, in order of first mention.
fn collect_refs(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            if let Some(r) = o.get("$ref").and_then(Value::as_str) {
                let name = r.rsplit('/').next().unwrap_or(r).to_string();
                if !out.contains(&name) {
                    out.push(name);
                }
            }
            for (k, x) in o {
                // Defaults are example values, not types.
                if k != "default" {
                    collect_refs(x, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_refs(x, out)),
        _ => {}
    }
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
    fn scene_schema_overview_types_and_errors() {
        let mut tools = Tools::new();
        let out = tools.call("scene_schema", &json!({})).into_json();
        let text = out["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("- layers (array of Layer)"), "{text}");
        assert!(text.contains("- MeshLayer: "), "{text}");

        let out = tools
            .call("scene_schema", &json!({ "type": "camera" }))
            .into_json();
        assert_ne!(out["isError"], true);
        let text = out["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("Camera:"), "{text}");
        assert!(text.contains("Refers to: Param, CameraMode"), "{text}");

        let out = tools
            .call("scene_schema", &json!({ "type": "Mesh" }))
            .into_json();
        assert_eq!(out["isError"], true);
        assert!(out["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("MeshLayer"));

        let out = tools
            .call("scene_schema", &json!({ "full": true }))
            .into_json();
        // Compared as text: parsing floats back can differ in the last digit.
        let full = out["content"][0]["text"].as_str().unwrap();
        assert_eq!(
            full,
            serde_json::to_string_pretty(&Project::json_schema()).unwrap()
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
    fn locate_reports_boxes_logos_overlaps_and_points() {
        if Gpu::headless().is_err() {
            eprintln!("skipping GPU test");
            return;
        }
        let mut p = presets::named("Empty");
        p.camera.mode = ez_core::CameraMode::Static;
        p.layers.push(ez_core::Layer::new(
            "Title",
            LayerKind::Logo(ez_core::LogoLayer {
                text: "OVER THE CUBE".into(),
                ..Default::default()
            }),
        ));
        let scene = serde_json::to_value(&p).unwrap();
        let mut tools = Tools::new();
        let out = tools
            .call(
                "locate",
                &json!({ "scene": scene, "points": [[0.0, 1.0, 0.0]], "layers": ["Cube", "Title", "Nope"] }),
            )
            .into_json();
        assert_ne!(out["isError"], true, "{out}");
        let st = &out["structuredContent"];
        let names: Vec<&str> = st["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Cube", "Title"]);
        // The centred title covers the cube in the middle of the picture.
        assert_eq!(st["overlaps"][0]["a"], "Cube", "{out}");
        assert_eq!(st["overlaps"][0]["b"], "Title");
        let spot = &st["points"][0][0];
        assert!(
            spot[0].as_f64().unwrap() > 0.3 && spot[0].as_f64().unwrap() < 0.7,
            "{spot}"
        );
        let text = out["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Nope: no layer with this name"), "{text}");

        let out = tools
            .call("locate", &json!({ "scene": "Neon Arena", "sweep": 4 }))
            .into_json();
        assert_ne!(out["isError"], true, "{out}");
        assert!(out["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("over 4 moments"));
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
