//! Command-line mode (no window): rendering, exporting and asset dumps.

use anyhow::{bail, Context, Result};
use ez_core::{presets, Project};
use ez_export::{ExportFormat, ExportSettings};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

const HELP: &str = "\
EZ2DEMOSCENE — loopable demoscene-style 3D scenes

USAGE:
    ez2demoscene [PROJECT.ez2.json]              open the editor
    ez2demoscene --render SCENE OUT.png [--phase 0.25] [--size 1920x1080]
    ez2demoscene --export SCENE OUT [--size WxH] [--fps 60] [--repeats 1]
                 (OUT: .mp4 .webm .gif, or a folder for a PNG sequence)
    ez2demoscene --list-presets
    ez2demoscene --write-presets DIR              save built-in presets as projects
    ez2demoscene --write-textures DIR             save the built-in texture pack as PNGs

SCENE is a project file or the name of a built-in preset (e.g. \"Neon Arena\").
";

fn load_scene(s: &str) -> Result<Project> {
    let p = Path::new(s);
    if p.exists() {
        let txt = std::fs::read_to_string(p).with_context(|| format!("reading {s}"))?;
        return Project::from_json(&txt).with_context(|| format!("parsing {s}"));
    }
    presets::all()
        .into_iter()
        .find(|pr| pr.name.eq_ignore_ascii_case(s))
        .map(|pr| pr.project)
        .with_context(|| format!("'{s}' is neither a file nor a preset (see --list-presets)"))
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
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
        "--write-textures" => {
            let dir = PathBuf::from(args.get(1).context("missing DIR")?);
            std::fs::create_dir_all(&dir)?;
            for (name, _) in ez_render::texgen::BUILTIN {
                let file = dir.join(format!("{name}.png"));
                ez_render::texgen::generate(name).save(&file)?;
                println!("wrote {}", file.display());
            }
        }
        "--render" => {
            let scene = load_scene(args.get(1).context("missing SCENE")?)?;
            let out = PathBuf::from(args.get(2).context("missing OUT.png")?);
            let phase: f32 = flag(args, "--phase")
                .map(|s| s.parse())
                .transpose()?
                .unwrap_or(0.0);
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
                fps: flag(args, "--fps")
                    .map(|s| s.parse())
                    .transpose()?
                    .unwrap_or(60.0),
                repeats: flag(args, "--repeats")
                    .map(|s| s.parse())
                    .transpose()?
                    .unwrap_or(1),
                output: out,
                ..Default::default()
            };
            let audio = scene
                .audio
                .as_ref()
                .and_then(|a| ez_export::analyze_audio(Path::new(a)).ok());
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
