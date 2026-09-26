//! Frame-time benchmark: `cargo run --release -p ez_render --example bench`
//! Measures CPU time spent in `Renderer::render` (scene evaluation, instance
//! building, uploads, command encoding) and the total including GPU wait.
//!
//! `bench [preset name] [full|half|quarter]` times a preset instead of the
//! stress scene, optionally forcing its background resolution.
//! `EZ2_LOD=1` turns terrain level of detail on; `EZ2_LOD=2` also doubles
//! the terrain's cells (same triangle count, twice the near detail).

use ez_core::*;
use ez_render::gpu::Gpu;
use ez_render::Renderer;
use std::time::Instant;

fn stress_scene() -> Project {
    let mut p = presets::gold_room();
    // A big static wall: 6000 copies that never change over the loop.
    p.layers.push(
        Layer::new(
            "Big static wall",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Sphere { detail: 1 }),
                instancer: Instancer::Wall {
                    cols: 100,
                    rows: 60,
                    spacing: 0.4,
                    curve: 90.0,
                },
                variation: Variation {
                    rotation: 30.0,
                    scale: 0.4,
                    hue: 0.2,
                    ..Default::default()
                },
                ..Default::default()
            }),
        )
        .scaled(0.15)
        .at([0.0, 0.0, -12.0]),
    );
    p
}

fn main() -> anyhow::Result<()> {
    let gpu = Gpu::headless()?;
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = r.create_target(640, 360);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut p = match args.first() {
        Some(name) => presets::all()
            .into_iter()
            .find(|q| q.name.to_lowercase().contains(&name.to_lowercase()))
            .map(|q| q.project)
            .ok_or_else(|| anyhow::anyhow!("no preset matching {name}"))?,
        None => stress_scene(),
    };
    if let Some(res) = args.get(1) {
        let res = match res.as_str() {
            "half" => BgResolution::Half,
            "quarter" => BgResolution::Quarter,
            _ => BgResolution::Full,
        };
        for l in &mut p.layers {
            if let LayerKind::Backdrop(b) = &mut l.kind {
                b.resolution = res;
            }
        }
    }
    if let Ok(lod) = std::env::var("EZ2_LOD") {
        for l in &mut p.layers {
            if let LayerKind::Terrain(t) = &mut l.kind {
                t.lod = lod != "0";
                if lod == "2" {
                    t.cells *= 2;
                }
            }
        }
    }
    let frames = 60;
    // warm-up
    r.render(&p, &EvalCtx::at(0.0), &target);
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let mut cpu = 0.0;
    let t0 = Instant::now();
    for i in 0..frames {
        let ctx = EvalCtx::new(&p.timing, i as f32 / frames as f32, None);
        let t = Instant::now();
        r.render(&p, &ctx, &target);
        cpu += t.elapsed().as_secs_f64();
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    }
    let total = t0.elapsed().as_secs_f64();
    let stats = r.stats();
    println!(
        "{}: {frames} frames: render() CPU {:.2} ms/frame, total {:.1} ms/frame ({} tris, {} particles, {} draws)",
        p.name,
        cpu * 1000.0 / frames as f64,
        total * 1000.0 / frames as f64,
        stats.triangles,
        stats.particles,
        stats.draw_calls
    );
    Ok(())
}
