//! Headless rendering tests. Skipped (with a message) when no GPU adapter is
//! available. Snapshots are written to `target/ez2-snapshots/` for review.

use ez_core::{presets, EvalCtx};
use ez_render::gpu::Gpu;
use ez_render::Renderer;
use std::path::PathBuf;

fn snapshot_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ez2-snapshots");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn mean_abs_diff(a: &[u8], b: &[u8]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs() as f32)
        .sum::<f32>()
        / a.len() as f32
}

#[test]
fn presets_render_and_loop_seamlessly() {
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    eprintln!("adapter: {}", gpu.adapter_name());
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = r.create_target(320, 180);
    let dir = snapshot_dir();
    for preset in presets::all() {
        let p = &preset.project;
        let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);
        let a = r.render_image(p, &at(0.0), &target);
        let b = r.render_image(p, &at(1.0), &target);
        let mid = r.render_image(p, &at(0.37), &target);
        let slug = preset.name.to_lowercase().replace(' ', "_");
        a.save(dir.join(format!("{slug}_0.png"))).unwrap();
        mid.save(dir.join(format!("{slug}_mid.png"))).unwrap();
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let motion = mean_abs_diff(a.as_raw(), mid.as_raw());
        eprintln!("{:<22} seam diff {seam:.3}  motion {motion:.2}", preset.name);
        assert!(seam < 0.6, "{} does not loop: diff {seam}", preset.name);
        // The image must not be black.
        let lum: f32 = a.as_raw().iter().map(|v| *v as f32).sum::<f32>() / a.as_raw().len() as f32;
        assert!(lum > 3.0, "{} renders black", preset.name);
    }
}
