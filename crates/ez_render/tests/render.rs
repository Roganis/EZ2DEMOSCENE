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
        eprintln!(
            "{:<22} seam diff {seam:.3}  motion {motion:.2}",
            preset.name
        );
        assert!(seam < 0.6, "{} does not loop: diff {seam}", preset.name);
        // The image must not be black.
        let lum: f32 = a.as_raw().iter().map(|v| *v as f32).sum::<f32>() / a.as_raw().len() as f32;
        assert!(lum > 3.0, "{} renders black", preset.name);
    }
}

/// Raymarched backgrounds must loop whatever their settings: odd twists,
/// detail and bend used to leave a seam at the loop point.
#[test]
fn raymarched_backdrops_loop_with_any_settings() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(160, 90);
    let kinds = [
        BackdropKind::Tunnel,
        BackdropKind::Fractal,
        BackdropKind::Sponge,
        BackdropKind::Rings,
    ];
    let mut failures = Vec::new();
    for kind in kinds {
        let variants = RaySettings::variants(kind).len().max(1) as u32;
        for variant in 0..variants {
            let mut p = presets::empty();
            p.layers = vec![Layer::new(
                "bg",
                LayerKind::Backdrop(Backdrop {
                    kind,
                    detail: Param::new(1.4),
                    color_b: [0.2, 0.3, 0.9],
                    color_c: [1.0, 0.4, 0.8],
                    ray: RaySettings {
                        variant,
                        pattern: 1,
                        twist: Param::new(1.3),
                        bend: Param::new(1.41),
                        glow: Param::new(0.78),
                        spin: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )];
            let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);
            let a = r.render_image(&p, &at(0.0), &target);
            let b = r.render_image(&p, &at(1.0), &target);
            let seam = mean_abs_diff(a.as_raw(), b.as_raw());
            let tiny = r.render_image(&p, &at(0.000001), &target);
            let jitter = mean_abs_diff(a.as_raw(), tiny.as_raw());
            eprintln!(
                "{:<28} variant {variant}: seam {seam:.3} (vs 1e-6 later: {jitter:.3})",
                kind.label()
            );
            // Very detailed, aliased views (the cube field) change a little
            // even between frames a millionth of a loop apart; a seam no
            // bigger than that is invisible.
            if seam > 0.6 && seam > jitter {
                failures.push(format!(
                    "{} variant {variant}: seam {seam:.2}",
                    kind.label()
                ));
            }
        }
    }
    // The ring corridor project from a user report (triangles, twist 1.3).
    let mut p = presets::empty();
    p.layers = vec![Layer::new(
        "rings",
        LayerKind::Backdrop(Backdrop {
            kind: BackdropKind::Rings,
            detail: Param::new(1.4),
            ray: RaySettings {
                variant: 2,
                size: Param::new(1.55),
                twist: Param::new(1.3),
                warp: Param::new(0.68),
                bend: Param::new(1.41),
                glow: Param::new(0.78),
                fog: Param::new(2.36),
                steps: 12,
                spin: 1,
                ..Default::default()
            },
            ..Default::default()
        }),
    )];
    let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);
    let seam = mean_abs_diff(
        r.render_image(&p, &at(0.0), &target).as_raw(),
        r.render_image(&p, &at(1.0), &target).as_raw(),
    );
    eprintln!("reported ring corridor: seam {seam:.3}");
    if seam > 0.6 {
        failures.push(format!("reported ring corridor: seam {seam:.2}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
