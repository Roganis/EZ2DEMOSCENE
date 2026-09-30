//! Command-line mode (no window): rendering, exporting and asset dumps.

use anyhow::{anyhow, bail, Context, Result};
use ez_core::{presets, Project};
use ez_export::{ExportFormat, ExportSettings};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

const HELP: &str = "\
EZ2DEMOSCENE — loopable demoscene-style 3D scenes

USAGE:
    ez2demoscene [PROJECT.ez2.json]              open the editor
    ez2demoscene --render SCENE OUT.png [--phase 0.25] [--size 1920x1080]
    ez2demoscene --export SCENE OUT [--size WxH] [--fps 60] [--repeats 1]
                 [--motion-blur 8] [--shutter 0.5]
                 (OUT: .mp4 .webm .gif, or a folder for a PNG sequence)
    ez2demoscene --list-presets
    ez2demoscene --mcp                           serve the Model Context Protocol on stdin/stdout
    ez2demoscene --write-presets DIR              save built-in presets as projects
    ez2demoscene --write-schema OUT.json          save the JSON Schema of project files
    ez2demoscene --write-textures DIR             save the built-in texture pack as PNGs
    ez2demoscene --write-preset-thumbs OUT.zip    render the gallery pictures of the built-in presets

SCENE is a project file, an .ez2pack, or the name of a built-in preset (e.g. \"Neon Arena\").
";

fn load_scene(s: &str) -> Result<Project> {
    ez_export::load_scene(s).map_err(|e| anyhow!("{e:#} (see --list-presets)"))
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
}

/// The value of `name` parsed, or `default` when the flag is absent.
fn flag_or<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    flag(args, name)
        .map(|s| {
            s.parse()
                .with_context(|| format!("bad value for {name}: {s}"))
        })
        .transpose()
        .map(|v| v.unwrap_or(default))
}

fn size(args: &[String], default: (u32, u32)) -> Result<(u32, u32)> {
    match flag(args, "--size") {
        None => Ok(default),
        Some(s) => {
            let (w, h) = s
                .split_once('x')
                .context("--size must look like 1920x1080")?;
            Ok((w.parse()?, h.parse()?))
        }
    }
}

/// Returns `Some(exit_code)` when a CLI command ran, `None` to start the GUI.
pub fn run(args: &[String]) -> Result<Option<i32>> {
    let Some(cmd) = args.first() else {
        return Ok(None);
    };
    match cmd.as_str() {
        "-h" | "--help" => {
            print!("{HELP}");
        }
        "--mcp" => crate::mcp::serve()?,
        "--list-presets" => {
            for p in presets::all() {
                println!("{:<22} {}", p.name, p.description);
            }
        }
        "--write-presets" => {
            let dir = PathBuf::from(args.get(1).context("missing DIR")?);
            std::fs::create_dir_all(&dir)?;
            for p in presets::all() {
                let file = dir.join(format!(
                    "{}.{}",
                    crate::export_ui::slug(p.name),
                    ez_core::PROJECT_EXTENSION
                ));
                std::fs::write(&file, p.project.to_json())?;
                println!("wrote {}", file.display());
            }
        }
        "--write-schema" => {
            let out = PathBuf::from(args.get(1).context("missing OUT.json")?);
            std::fs::write(&out, serde_json::to_string_pretty(&Project::json_schema())?)?;
            println!("wrote {}", out.display());
        }
        "--write-textures" => {
            let dir = PathBuf::from(args.get(1).context("missing DIR")?);
            std::fs::create_dir_all(&dir)?;
            for (name, _) in ez_render::texgen::BUILTIN {
                let file = dir.join(format!("{name}.png"));
                ez_render::texgen::generate(name).save(&file)?;
                println!("wrote {}", file.display());
            }
        }
        "--write-preset-thumbs" => {
            let out = PathBuf::from(args.get(1).context("missing OUT.zip")?);
            crate::preset_thumbs::write(&out)?;
        }
        "--render" => {
            let scene = load_scene(args.get(1).context("missing SCENE")?)?;
            let out = PathBuf::from(args.get(2).context("missing OUT.png")?);
            let phase: f32 = flag_or(args, "--phase", 0.0)?;
            let (w, h) = size(args, (1920, 1080))?;
            ez_export::render_still(&scene, phase.rem_euclid(1.0), w, h, &out)?;
            println!("wrote {}", out.display());
        }
        "--export" => {
            let scene = load_scene(args.get(1).context("missing SCENE")?)?;
            let out = PathBuf::from(args.get(2).context("missing OUT")?);
            let (w, h) = size(args, (1920, 1080))?;
            let settings = ExportSettings {
                format: ExportFormat::from_path(&out),
                width: w,
                height: h,
                fps: flag_or(args, "--fps", 60.0)?,
                repeats: flag_or(args, "--repeats", 1)?,
                motion_blur: flag_or(args, "--motion-blur", 1)?,
                shutter: flag_or(args, "--shutter", 0.5)?,
                output: out,
                ..Default::default()
            };
            let audio = match ez_export::load_music(&scene) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("warning: music ignored: {e:#}");
                    None
                }
            };
            let cancel = AtomicBool::new(false);
            let mut last = 0;
            let path = ez_export::export(
                &scene,
                &settings,
                audio.as_ref(),
                |p| {
                    let pct = p.frame * 100 / p.total.max(1);
                    if pct != last {
                        last = pct;
                        eprint!("\rrendering… {pct:3}% ({}/{})", p.frame, p.total);
                    }
                },
                &cancel,
            )?;
            eprintln!();
            println!("wrote {}", path.display());
        }
        other if other.starts_with('-') => bail!("unknown option {other}\n\n{HELP}"),
        _ => return Ok(None),
    }
    Ok(Some(0))
}
