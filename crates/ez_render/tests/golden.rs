//! Golden-image regression tests: every preset is rendered small and
//! compared with a stored reference, so renderer changes can't silently
//! change how scenes look.
//!
//! References are produced with Mesa's software rasterizer (llvmpipe), so
//! the comparison only runs on that adapter; other GPUs legitimately differ
//! in the last bits. Regenerate after an intended visual change with:
//!     EZ2_BLESS=1 cargo test -p ez_render --test golden

use ez_core::{presets, EvalCtx};
use ez_render::gpu::Gpu;
use ez_render::Renderer;
use std::path::PathBuf;

const W: u32 = 160;
const H: u32 = 90;
/// Mean absolute channel difference allowed (0..255).
const MAX_MEAN: f32 = 1.5;
/// Fraction of channels allowed to differ by more than 40.
const MAX_OUTLIERS: f32 = 0.01;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

#[test]
fn presets_match_golden_images() {
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping golden test: {e:#}");
            return;
        }
    };
    let bless = std::env::var("EZ2_BLESS").is_ok();
    if !gpu.adapter_name().contains("llvmpipe") && !bless {
        eprintln!("skipping golden test on {} (references are llvmpipe renders)", gpu.adapter_name());
        return;
    }
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = r.create_target(W, H);
    std::fs::create_dir_all(golden_dir()).unwrap();
    let mut failures = Vec::new();
    for preset in presets::all() {
        let p = &preset.project;
        let img = r.render_image(p, &EvalCtx::new(&p.timing, 0.3, None), &target);
        let file = golden_dir().join(format!("{}.png", preset.name.to_lowercase().replace(' ', "_")));
        if bless || !file.exists() {
            img.save(&file).unwrap();
            eprintln!("wrote {}", file.display());
            continue;
        }
        let reference = image::open(&file).unwrap().to_rgba8();
        let (a, b) = (img.as_raw(), reference.as_raw());
        assert_eq!(a.len(), b.len(), "{}: size changed", preset.name);
        let diffs: Vec<u32> = a.iter().zip(b).map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs()).collect();
        let mean = diffs.iter().sum::<u32>() as f32 / diffs.len() as f32;
        let outliers = diffs.iter().filter(|d| **d > 40).count() as f32 / diffs.len() as f32;
        eprintln!("{:<22} mean diff {mean:.3}, outliers {:.4}", preset.name, outliers);
        if mean > MAX_MEAN || outliers > MAX_OUTLIERS {
            let actual = file.with_extension("actual.png");
            img.save(&actual).unwrap();
            failures.push(format!(
                "{}: mean diff {mean:.2}, outliers {:.3} (see {})",
                preset.name,
                outliers,
                actual.display()
            ));
        }
    }
    assert!(failures.is_empty(), "golden images differ:\n{}", failures.join("\n"));
}
