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

/// Weather, liquids, terrain shapes and the new skies must not jump at the
/// loop point, nor where a whole-number motion wraps inside the loop (a
/// current flowing twice per loop wraps at the half). Each frame is compared
/// with one a hair earlier (film grain, which changes every frame, is
/// turned off); the same step elsewhere in the loop is the baseline.
#[test]
fn weather_liquids_and_skies_are_continuous() {
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
    let mut scenes: Vec<(String, Project)> = Vec::new();
    for liquid in LiquidKind::ALL {
        for (i, shape) in TerrainShape::ALL.iter().enumerate() {
            let mut p = presets::stormy_lake();
            p.layers
                .retain(|l| !matches!(l.kind, LayerKind::Weather(_)));
            for l in &mut p.layers {
                if let LayerKind::Terrain(t) = &mut l.kind {
                    t.shape = *shape;
                    t.biome = Biome::ALL[i % Biome::ALL.len()];
                    t.scroll = 1;
                    t.liquid.kind = liquid;
                    t.liquid.level = Param::new(0.3);
                    t.liquid.flow = 2;
                }
            }
            scenes.push((format!("{} / {}", liquid.label(), shape.label()), p));
        }
    }
    for kind in [BackdropKind::Clouds, BackdropKind::Aurora] {
        for variant in 0..RaySettings::variants(kind).len() as u32 {
            let mut p = presets::sunbeam_peaks();
            p.layers
                .retain(|l| matches!(l.kind, LayerKind::Backdrop(_)));
            for l in &mut p.layers {
                if let LayerKind::Backdrop(b) = &mut l.kind {
                    b.kind = kind;
                    b.speed = 2;
                    b.ray.variant = variant;
                }
            }
            scenes.push((format!("{} {variant}", kind.label()), p));
        }
    }
    for kind in Precipitation::ALL {
        let mut p = presets::stormy_lake();
        for l in &mut p.layers {
            if let LayerKind::Weather(w) = &mut l.kind {
                w.kind = kind;
                w.lightning.per_loop = 2;
                w.lightning.chance = 1.0;
            }
        }
        scenes.push((format!("weather {}", kind.label()), p));
    }
    // Day cycle, rainbow, mist, caustics, heat haze, spotlights,
    // waterfalls, tornado smoke, wet ground and snow cover.
    // (The club's beat strobes jump on every beat by design.)
    let mut club = presets::club_spotlights();
    for l in &mut club.layers {
        match &mut l.kind {
            LayerKind::Lasers(z) => z.strobe = Param::new(0.0),
            LayerKind::Mesh(m) => m.material.emissive.amp = 0.0,
            _ => {}
        }
    }
    for p in [
        club,
        presets::rainbow_falls(),
        presets::sunken_temple(),
        presets::twister(),
        presets::lava_world(),
        presets::aurora_tundra(),
    ] {
        scenes.push((p.name.clone(), p));
    }
    let mut falls = presets::rainbow_falls();
    for l in &mut falls.layers {
        if let LayerKind::Falls(f) = &mut l.kind {
            f.kind = FallKind::Lava;
            f.flow = 2;
        }
    }
    scenes.push(("lava fall".into(), falls));
    let mut failures = Vec::new();
    for (name, p) in &mut scenes {
        p.post.grade.grain = Param::new(0.0);
        let p = &*p;
        let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);
        let mut step = |at_phase: f32| {
            let before = r.render_image(p, &at(at_phase - 2e-5), &target);
            let after = r.render_image(p, &at(at_phase), &target);
            mean_abs_diff(before.as_raw(), after.as_raw())
        };
        let baseline = step(0.3);
        let worst = step(1.0).max(step(0.5));
        eprintln!("{name:<40} jump {worst:.3} (elsewhere {baseline:.3})");
        if worst > baseline + 0.6 {
            failures.push(format!("{name}: jump {worst:.2}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Music-driven values and time warp keep the loop seamless in loop-window
/// mode, even with the window starting mid-bar, and actually react.
#[test]
fn music_reactive_scene_loops_and_reacts() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let rate = 22050.0f32;
    let samples: Vec<f32> = (0..(rate * 12.0) as usize)
        .map(|i| {
            let t = i as f32 / rate;
            let bt = (t * 124.0 / 60.0).fract() * 60.0 / 124.0;
            let kick = (std::f32::consts::TAU * 60.0 * bt).sin() * (-bt * 30.0).exp();
            let tone = (std::f32::consts::TAU * 330.0 * t).sin() * 0.1 * (1.0 + (t * 0.5).sin());
            kick * 0.8 + tone
        })
        .collect();
    let env = analysis::analyze(&samples, rate);
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(160, 90);
    let mut p = presets::music_reactor();
    p.music.offset = 1.37;
    let mut at = |p: &Project, phase: f32, audio: Option<&AudioEnvelope>| {
        r_render(&mut r, p, &p.ctx(phase, audio), &target)
    };
    fn r_render(
        r: &mut Renderer,
        p: &Project,
        ctx: &EvalCtx,
        t: &ez_render::RenderTarget,
    ) -> Vec<u8> {
        r.render_image(p, ctx, t).into_raw()
    }
    p.post.grade.grain = Param::new(0.0);
    let a = at(&p, 0.0, Some(&env));
    let b = at(&p, 1.0 - 2e-5, Some(&env));
    let mid = at(&p, 0.3, Some(&env));
    let mid_before = at(&p, 0.3 - 2e-5, Some(&env));
    let seam = mean_abs_diff(&a, &b);
    let baseline = mean_abs_diff(&mid, &mid_before);
    eprintln!("music reactor seam {seam:.3} (elsewhere {baseline:.3})");
    assert!(seam < baseline + 0.6, "music-driven loop jumps: {seam}");
    let silent = at(&p, 0.3, None);
    let react = mean_abs_diff(&mid, &silent);
    eprintln!("music reactor reaction {react:.2}");
    assert!(react > 1.0, "the music changes nothing: {react}");
}

/// The editor shows exactly the colours that get exported.
#[test]
fn display_image_matches_export() {
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(96, 54);
    let p = presets::synth_sunset();
    let out = r.render_image(&p, &EvalCtx::new(&p.timing, 0.2, None), &target);
    let shown = r.read_display_pixels(&target);
    let worst = out
        .as_raw()
        .iter()
        .zip(&shown)
        .map(|(a, b)| (*a as i32 - *b as i32).abs())
        .max()
        .unwrap();
    assert!(worst <= 1, "display differs from export by up to {worst}");
}

/// Sun shadows darken the ground behind a shape (away from the sun), and
/// switching them off brings the light back.
#[test]
fn sun_shadows_darken_the_floor() {
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
    let mut p = presets::empty();
    p.post.grade.grain = Param::new(0.0);
    p.environment.light_dir = [1.0, 0.8, 0.5];
    p.environment.light_intensity = Param::new(2.0);
    for l in &mut p.layers {
        if let LayerKind::Mirror(m) = &mut l.kind {
            m.base_color = [0.5, 0.5, 0.5];
            m.reflectivity = Param::new(0.1);
        }
    }
    let ctx = EvalCtx::new(&p.timing, 0.1, None);
    let lum = |img: &image::RgbaImage| -> f32 {
        img.as_raw().iter().map(|v| *v as f32).sum::<f32>() / img.as_raw().len() as f32
    };
    let off = r.render_image(&p, &ctx, &target);
    p.environment.shadows.enabled = true;
    let on = r.render_image(&p, &ctx, &target);
    eprintln!(
        "mean brightness without {:.2}, with shadows {:.2}",
        lum(&off),
        lum(&on)
    );
    assert!(lum(&on) < lum(&off) - 0.3, "no shadow visible");
}

/// Terrain level of detail: still loops, and looks like the full grid (with
/// twice the cells, the same triangle count, for solid terrains; grid lines
/// sit on every cell, so line styles keep their cells).
#[test]
fn terrain_lod_loops_and_matches_the_full_grid() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = r.create_target(320, 180);
    let mut tested = 0;
    for preset in presets::all() {
        let mut full = preset.project;
        for l in &mut full.layers {
            if let LayerKind::Terrain(t) = &mut l.kind {
                t.lod = false;
            }
        }
        if !full
            .layers
            .iter()
            .any(|l| matches!(l.kind, LayerKind::Terrain(_)))
        {
            continue;
        }
        let mut lod = full.clone();
        for l in &mut lod.layers {
            if let LayerKind::Terrain(t) = &mut l.kind {
                t.lod = true;
                if t.style == TerrainStyle::Solid {
                    t.cells *= 2;
                }
            }
        }
        let at = |phase: f32| EvalCtx::new(&full.timing, phase, None);
        let a = r.render_image(&lod, &at(0.0), &target);
        let b = r.render_image(&lod, &at(1.0), &target);
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let reference = r.render_image(&full, &at(0.0), &target);
        let change = mean_abs_diff(a.as_raw(), reference.as_raw());
        eprintln!(
            "{:<22} LOD seam {seam:.3}  vs full grid {change:.2}",
            preset.name
        );
        assert!(seam < 0.6, "{} does not loop with LOD: {seam}", preset.name);
        assert!(
            change < 4.0,
            "{} looks different with LOD: {change}",
            preset.name
        );
        tested += 1;
    }
    assert!(tested >= 3);
}

/// Deformations change the shape and keep the loop seamless.
#[test]
fn deformed_shapes_loop() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = r.create_target(320, 180);
    let plain = presets::orbiting_solid();
    let mut p = plain.clone();
    for l in &mut p.layers {
        if let LayerKind::Mesh(m) = &mut l.kind {
            if l.name == "Dodecahedron" {
                m.subdivide = 2;
                m.deform = Deform {
                    twist: Param::new(0.4).osc(Wave::Sine, 0.3, 1),
                    bend: Param::new(40.0),
                    taper: Param::new(-0.3),
                    noise: Param::new(0.15),
                    noise_speed: 2,
                    explode: Param::new(0.0).osc(Wave::ExpOut, 0.4, 8),
                    ..Default::default()
                };
            }
        }
    }
    let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);
    let a = r.render_image(&p, &at(0.0), &target);
    let b = r.render_image(&p, &at(1.0), &target);
    let before = r.render_image(&p, &at(1.0 - 1.0 / 240.0), &target);
    let reference = r.render_image(&plain, &at(0.0), &target);
    a.save(snapshot_dir().join("deformed.png")).unwrap();
    let seam = mean_abs_diff(a.as_raw(), b.as_raw());
    let step = mean_abs_diff(a.as_raw(), before.as_raw());
    let change = mean_abs_diff(a.as_raw(), reference.as_raw());
    eprintln!("deform seam {seam:.3} last step {step:.2} vs plain {change:.2}");
    assert!(seam < 0.6);
    assert!(change > 1.0, "deform barely visible");
}

/// Colours across copies: visible, and cycling keeps the loop seamless.
#[test]
fn color_ramp_across_copies_loops() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = r.create_target(320, 180);
    let plain = presets::orbiting_solid();
    let mut p = plain.clone();
    for l in &mut p.layers {
        if let LayerKind::Mesh(m) = &mut l.kind {
            if l.name == "Debris swarm" {
                m.material.emissive = Param::new(1.5);
                m.ramp = ColorRamp {
                    enabled: true,
                    colors: vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.2, 1.0]],
                    cycles: 2,
                    ..Default::default()
                };
            }
        }
    }
    let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);
    let a = r.render_image(&p, &at(0.0), &target);
    let b = r.render_image(&p, &at(1.0), &target);
    let reference = r.render_image(&plain, &at(0.0), &target);
    a.save(snapshot_dir().join("ramp.png")).unwrap();
    let seam = mean_abs_diff(a.as_raw(), b.as_raw());
    let change = mean_abs_diff(a.as_raw(), reference.as_raw());
    eprintln!("ramp seam {seam:.3} vs plain {change:.2}");
    assert!(seam < 0.6);
    assert!(change > 1.0, "ramp barely visible");
}

/// Copies scattered over a shape's surface render around it.
#[test]
fn copies_cover_a_surface() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = r.create_target(320, 180);
    let mut p = presets::orbiting_solid();
    p.layers
        .retain(|l| l.name == "Deep space" || l.name == "Dodecahedron");
    let without = p.clone();
    let spikes = Layer::new(
        "Spikes",
        LayerKind::Mesh(MeshLayer {
            source: MeshSource::Primitive(Primitive::Cone { segments: 8 }),
            instancer: Instancer::Surface {
                shape: MeshSource::Primitive(Primitive::Dodecahedron),
                size: 1.6,
                count: 120,
                seed: 4,
                align: true,
                lift: 0.1,
            },
            material: Material {
                emissive: Param::new(2.0),
                emissive_color: [1.0, 0.3, 0.1],
                ..Default::default()
            },
            ..Default::default()
        }),
    )
    .scaled(0.12)
    .spin([1, 1, 0]);
    p.layers.push(spikes);
    let ctx = EvalCtx::new(&p.timing, 0.2, None);
    let a = r.render_image(&p, &ctx, &target);
    let b = r.render_image(&without, &ctx, &target);
    a.save(snapshot_dir().join("surface.png")).unwrap();
    let change = mean_abs_diff(a.as_raw(), b.as_raw());
    eprintln!("surface copies change {change:.2}");
    assert!(change > 1.0);
}

/// Copies stand on the terrain the GPU draws: markers sunk just below the
/// CPU height are hidden, the same markers just above it show.
#[test]
fn terrain_copies_match_the_gpu_ground() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(320, 180);
    let mut p = presets::vector_valley();
    p.post = Default::default();
    p.environment.fog_density = Param::new(0.0);
    p.camera.height = Param::new(14.0);
    p.layers.retain(|l| matches!(l.kind, LayerKind::Terrain(_)));
    let tname = p.layers[0].name.clone();
    if let LayerKind::Terrain(t) = &mut p.layers[0].kind {
        t.style = TerrainStyle::Solid;
        t.fill_color = [0.2, 0.2, 0.2];
    }
    let with_markers = |lift: f32| {
        let mut q = p.clone();
        q.layers.push(
            Layer::new(
                "Markers",
                LayerKind::Mesh(MeshLayer {
                    source: MeshSource::Primitive(Primitive::Sphere { detail: 2 }),
                    instancer: Instancer::OnTerrain {
                        terrain: tname.clone(),
                        count: 400,
                        seed: 9,
                        align: false,
                        lift,
                        ground: None,
                    },
                    material: Material {
                        base_color: [0.0, 0.0, 0.0],
                        emissive: Param::new(4.0),
                        emissive_color: [0.0, 1.0, 0.0],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .scaled(0.25),
        );
        q
    };
    let green = |img: &image::RgbaImage| {
        img.pixels()
            .filter(|px| px[1] > 150 && px[0] < 110 && px[2] < 110)
            .count()
    };
    let ctx = EvalCtx::new(&p.timing, 0.3, None);
    let above = green(&r.render_image(&with_markers(0.2), &ctx, &target));
    let below = green(&r.render_image(&with_markers(-0.45), &ctx, &target));
    eprintln!("marker pixels above ground {above}, sunk {below}");
    assert!(above > 200, "markers not visible");
    assert!(
        (below as f32) < above as f32 * 0.25,
        "sunk markers still show: {below} vs {above}"
    );
}

/// Text layers draw, loop, and scrollers move.
#[test]
fn text_layers_loop() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = r.create_target(320, 180);
    let mut p = presets::empty();
    p.post.grade.grain = Param::new(0.0);
    let plain = p.clone();
    for (i, style) in TextStyle::ALL.into_iter().enumerate() {
        p.layers.push(
            Layer::new(
                "Text",
                LayerKind::Text(TextLayer {
                    text: "HELLO SCENE\nLOOP FOREVER".into(),
                    style,
                    font: TextFont::ALL[i % 3],
                    size: 0.5,
                    chrome: if i == 0 { 1.0 } else { 0.0 },
                    outline: 0.5,
                    shadow: 0.5,
                    face_camera: i % 2 == 0,
                    ..Default::default()
                }),
            )
            .at([0.0, 0.5 + i as f32 * 0.7, 0.0]),
        );
    }
    let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);
    let a = r.render_image(&p, &at(0.0), &target);
    let b = r.render_image(&p, &at(1.0), &target);
    let mid = r.render_image(&p, &at(0.4), &target);
    let reference = r.render_image(&plain, &at(0.4), &target);
    mid.save(snapshot_dir().join("text.png")).unwrap();
    let seam = mean_abs_diff(a.as_raw(), b.as_raw());
    let shown = mean_abs_diff(mid.as_raw(), reference.as_raw());
    eprintln!("text seam {seam:.3}, visible {shown:.2}");
    assert!(seam < 0.6);
    assert!(shown > 1.0, "text barely visible");
}

/// A two-scene sequence: each clip shows its scene, transitions mix them,
/// and the whole sequence loops.
#[test]
fn sequences_play_scenes_with_transitions() {
    use ez_core::sequence::*;
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
    let mut p = presets::orbiting_solid();
    p.post.grade.grain = Param::new(0.0);
    let alone = p.clone();
    p.start_sequence();
    // Scene B: another preset's look.
    let b = p.add_scene(false);
    let other = presets::synth_sunset();
    if let Some(s) = p.sequence.scenes.iter_mut().find(|s| s.id == b) {
        s.layers = other.layers.clone();
        s.camera = other.camera.clone();
        s.environment = other.environment.clone();
        s.post = other.post.clone();
        s.post.grade.grain = Param::new(0.0);
    }
    for kind in [
        TransitionKind::Crossfade,
        TransitionKind::Wipe,
        TransitionKind::Iris,
        TransitionKind::Flash,
        TransitionKind::Glitch,
    ] {
        p.sequence.clips = vec![
            Clip {
                scene: p.sequence.scene_id,
                beats: 8,
                transition: Transition {
                    kind,
                    beats: 4.0,
                    angle: 30.0,
                },
            },
            Clip {
                scene: b,
                beats: 8,
                transition: Transition {
                    kind,
                    beats: 4.0,
                    angle: 30.0,
                },
            },
        ];
        p.sync_sequence_length();
        let at = |beat: f32| EvalCtx::new(&p.timing, beat / 16.0, None);
        let start = r.render_image(&p, &at(0.0), &target);
        let end = r.render_image(&p, &at(16.0), &target);
        let seam = mean_abs_diff(start.as_raw(), end.as_raw());
        let clip_a = r.render_image(&p, &at(6.0), &target);
        let clip_b = r.render_image(&p, &at(14.0), &target);
        let mid = r.render_image(&p, &at(10.0), &target);
        mid.save(snapshot_dir().join(format!("transition_{kind:?}.png")))
            .unwrap();
        let (da, db) = (
            mean_abs_diff(mid.as_raw(), clip_a.as_raw()),
            mean_abs_diff(mid.as_raw(), clip_b.as_raw()),
        );
        eprintln!("{kind:?}: seam {seam:.3}, mid vs A {da:.1}, vs B {db:.1}");
        assert!(seam < 0.6, "{kind:?} sequence does not loop");
        assert!(
            mean_abs_diff(clip_a.as_raw(), clip_b.as_raw()) > 5.0,
            "scenes look alike"
        );
        assert!(
            da > 1.0 && db > 1.0,
            "{kind:?}: the transition shows only one scene"
        );
    }
    // Away from transitions, clip A is exactly scene A at its own moment.
    let clip_a = r.render_image(&p, &EvalCtx::new(&p.timing, 6.0 / 16.0, None), &target);
    let own = r.render_image(
        &alone,
        &EvalCtx::new(&alone.timing, 6.0 / alone.timing.loop_beats as f32, None),
        &target,
    );
    assert!(mean_abs_diff(clip_a.as_raw(), own.as_raw()) < 0.5);
}

/// Depth of field softens the picture away from the focus and loops.
#[test]
fn depth_of_field_blurs() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = r.create_target(320, 180);
    let mut p = presets::gold_room();
    p.post.grade.grain = Param::new(0.0);
    let sharp_project = p.clone();
    p.post.dof = DepthOfField {
        enabled: true,
        blur: Param::new(1.0),
        ..Default::default()
    };
    let detail = |img: &image::RgbaImage| {
        let (w, h) = img.dimensions();
        let mut sum = 0.0f64;
        for y in 0..h {
            for x in 1..w {
                let a = img.get_pixel(x, y)[1] as f64;
                let b = img.get_pixel(x - 1, y)[1] as f64;
                sum += (a - b).abs();
            }
        }
        sum / (w * h) as f64
    };
    let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);
    let sharp = r.render_image(&sharp_project, &at(0.2), &target);
    let soft = r.render_image(&p, &at(0.2), &target);
    let (ds, db) = (detail(&sharp), detail(&soft));
    eprintln!("detail sharp {ds:.2}, with depth of field {db:.2}");
    assert!(db < ds * 0.85, "no visible blur");
    let a = r.render_image(&p, &at(0.0), &target);
    let b = r.render_image(&p, &at(1.0), &target);
    assert!(mean_abs_diff(a.as_raw(), b.as_raw()) < 0.6);
}

/// Feedback trails reach a steady state that repeats every loop, so an
/// export that starts after one loop of warm-up closes seamlessly.
#[test]
fn feedback_trails_repeat_every_loop() {
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
    let mut p = presets::orbiting_solid();
    p.post.grade.grain = Param::new(0.0);
    p.timing.loop_beats = 4; // 2 s
    p.post.feedback = Feedback {
        enabled: true,
        length: Param::new(0.7),
        zoom: 1.3,
        turn: 20.0,
        hue: 0.2,
    };
    let frames = 24;
    let mut loops = Vec::new();
    for _ in 0..3 {
        let mut first = None;
        for i in 0..frames {
            let img = r.render_image(
                &p,
                &EvalCtx::new(&p.timing, i as f32 / frames as f32, None),
                &target,
            );
            if i == 5 {
                first = Some(img);
            }
        }
        loops.push(first.unwrap());
    }
    // Drawing the same moment again (a paused preview) doesn't feed the
    // picture back into itself.
    let again = r.render_image(
        &p,
        &EvalCtx::new(&p.timing, 5.0 / frames as f32, None),
        &target,
    );
    let again2 = r.render_image(
        &p,
        &EvalCtx::new(&p.timing, 5.0 / frames as f32, None),
        &target,
    );
    assert!(
        mean_abs_diff(again.as_raw(), again2.as_raw()) < 0.01,
        "a paused frame keeps changing"
    );
    let steady = mean_abs_diff(loops[1].as_raw(), loops[2].as_raw());
    let mut plain = p.clone();
    plain.post.feedback.enabled = false;
    let without = r.render_image(
        &plain,
        &EvalCtx::new(&p.timing, 5.0 / frames as f32, None),
        &target,
    );
    let trails = mean_abs_diff(loops[2].as_raw(), without.as_raw());
    loops[2].save(snapshot_dir().join("feedback.png")).unwrap();
    eprintln!("feedback: loop-to-loop {steady:.3}, trails {trails:.2}");
    assert!(steady < 0.6, "the trails don't settle into the loop");
    assert!(trails > 1.0, "no visible trails");
}

/// Raymarched objects: every shape shows, moves with the loop, closes the
/// loop, cuts into other shapes by depth and casts a sun shadow.
#[test]
fn raymarched_objects_loop_and_cast_shadows() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(240, 136);
    let mut p = presets::empty();
    p.post.grade.grain = Param::new(0.0);
    p.environment.light_dir = [0.6, 1.0, 0.3];
    p.environment.light_intensity = Param::new(2.0);
    p.environment.shadows.enabled = true;
    p.layers.retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    for l in &mut p.layers {
        if let LayerKind::Mirror(m) = &mut l.kind {
            m.base_color = [0.5, 0.5, 0.5];
            m.reflectivity = Param::new(0.1);
        }
    }
    let plain = p.clone();
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    for form in SdfShape::all_defaults() {
        let mut p = plain.clone();
        let mut layer = Layer::new(
            "Blob",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Sdf { form, cycles: 1 },
                ..Default::default()
            }),
        )
        .at([0.0, 1.2, 0.0]);
        layer.transform.scale = Param::new(1.2);
        // A bar straight through it: the depth test has to cut it.
        let mut bar = Layer::new(
            "Bar",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Cube),
                ..Default::default()
            }),
        )
        .at([0.0, 1.2, 0.0]);
        bar.transform.stretch = [4.0, 0.15, 0.15];
        p.layers.push(layer);
        p.layers.push(bar);
        let a = r.render_image(&p, &at(&p, 0.0), &target);
        let b = r.render_image(&p, &at(&p, 1.0), &target);
        let mid = r.render_image(&p, &at(&p, 0.37), &target);
        let reference = r.render_image(&plain, &at(&p, 0.37), &target);
        // The depth-of-field distance pass marches too.
        p.post.dof.enabled = true;
        r.render_image(&p, &at(&p, 0.37), &target);
        p.post.dof.enabled = false;
        p.environment.shadows.enabled = false;
        let unshadowed = r.render_image(&p, &at(&p, 0.37), &target);
        let name = form.label().to_lowercase().replace(' ', "_");
        mid.save(snapshot_dir().join(format!("sdf_{name}.png")))
            .unwrap();
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let moves = mean_abs_diff(a.as_raw(), mid.as_raw());
        let shown = mean_abs_diff(mid.as_raw(), reference.as_raw());
        let shadow = mean_abs_diff(mid.as_raw(), unshadowed.as_raw());
        eprintln!(
            "{}: seam {seam:.3}, moves {moves:.2}, visible {shown:.2}, shadow {shadow:.2}",
            form.label()
        );
        assert!(seam < 0.6, "{} doesn't loop", form.label());
        assert!(shown > 1.0, "{} barely visible", form.label());
        assert!(moves > 0.05, "{} doesn't move", form.label());
        assert!(shadow > 0.1, "{} casts no shadow", form.label());
    }
}

/// Sprites in every blend and facing: visible, the sheet plays, and the
/// loop closes.
#[test]
fn sprites_play_sheets_and_loop() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(240, 136);
    let mut plain = presets::empty();
    plain.post.grade.grain = Param::new(0.0);
    plain
        .layers
        .retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    let cases = [
        ("dots", None, SpriteFacing::Camera, SpriteBlend::Additive),
        (
            "flames",
            Some("sheet_flame"),
            SpriteFacing::Upright,
            SpriteBlend::Additive,
        ),
        (
            "explosion",
            Some("sheet_explosion"),
            SpriteFacing::Camera,
            SpriteBlend::Alpha,
        ),
        (
            "coins",
            Some("sheet_coin"),
            SpriteFacing::Fixed,
            SpriteBlend::Cutout,
        ),
    ];
    for (name, image, facing, blend) in cases {
        let mut p = plain.clone();
        let (columns, rows) = if image.is_some() { (4, 4) } else { (1, 1) };
        p.layers.push(
            Layer::new(
                name,
                LayerKind::Sprite(SpriteLayer {
                    image: image.map(String::from),
                    columns,
                    rows,
                    cycles: 2,
                    random_start: true,
                    facing,
                    blend,
                    size: Param::new(1.2),
                    glow: Param::new(1.5),
                    instancer: Instancer::Radial {
                        count: 6,
                        radius: 2.0,
                    },
                    // The dots move by growing and shrinking instead.
                    variation: Variation {
                        ripple: if image.is_none() { 0.5 } else { 0.0 },
                        ripple_cycles: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, 1.2, 0.0]),
        );
        let a = r.render_image(&p, &at(&p, 0.0), &target);
        let b = r.render_image(&p, &at(&p, 1.0), &target);
        let mid = r.render_image(&p, &at(&p, 0.3), &target);
        let reference = r.render_image(&plain, &at(&p, 0.3), &target);
        mid.save(snapshot_dir().join(format!("sprite_{name}.png")))
            .unwrap();
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let plays = mean_abs_diff(a.as_raw(), mid.as_raw());
        let shown = mean_abs_diff(mid.as_raw(), reference.as_raw());
        eprintln!("{name}: seam {seam:.3}, plays {plays:.2}, visible {shown:.2}");
        assert!(seam < 0.6, "{name} doesn't loop");
        assert!(shown > 0.5, "{name} barely visible");
        assert!(plays > 0.05, "{name} doesn't animate");
    }
}

/// Electric arcs between points, to the nearest copies and copy to copy:
/// visible, re-striking and seamless.
#[test]
fn electric_arcs_strike_and_loop() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(240, 136);
    let mut plain = presets::empty();
    plain.post.grade.grain = Param::new(0.0);
    plain
        .layers
        .retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    plain.layers.push(
        Layer::new(
            "Moons",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Sphere { detail: 1 }),
                instancer: Instancer::Orbit {
                    count: 6,
                    radius: 2.5,
                    spread: 0.5,
                    speed: 1,
                    seed: 3,
                },
                ..Default::default()
            }),
        )
        .scaled(0.2)
        .at([0.0, 1.5, 0.0]),
    );
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    let paths = [
        ArcPath::Points {
            from: [-2.0, 0.0, 0.0],
            to: [2.0, 0.5, 0.0],
        },
        ArcPath::Nearest {
            target: "Moons".into(),
            count: 3,
        },
        ArcPath::Chain {
            target: "Moons".into(),
        },
    ];
    for path in paths {
        let name = path.label();
        let mut p = plain.clone();
        p.layers.push(
            Layer::new(
                "Arcs",
                LayerKind::Arcs(ArcLayer {
                    path: path.clone(),
                    strikes: 8,
                    ..Default::default()
                }),
            )
            .at([0.0, 1.5, 0.0]),
        );
        let a = r.render_image(&p, &at(&p, 0.0), &target);
        let b = r.render_image(&p, &at(&p, 1.0), &target);
        // Just before and after a re-strike (8 per loop).
        let before = r.render_image(&p, &at(&p, 0.12), &target);
        let after = r.render_image(&p, &at(&p, 0.13), &target);
        let reference = r.render_image(&plain, &at(&p, 0.13), &target);
        after
            .save(snapshot_dir().join(format!("arcs_{}.png", name.replace(' ', "_"))))
            .unwrap();
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let strike = mean_abs_diff(before.as_raw(), after.as_raw());
        let shown = mean_abs_diff(after.as_raw(), reference.as_raw());
        eprintln!("{name}: seam {seam:.3}, re-strike {strike:.2}, visible {shown:.2}");
        assert!(seam < 0.6, "{name} doesn't loop");
        assert!(shown > 0.03, "{name} barely visible");
        assert!(strike > 0.05, "{name} doesn't re-strike");
    }
}

/// Every copy layout: the compute shader and the CPU fallback draw the
/// same picture, the copies loop, and 100k copies render.
#[test]
fn gpu_copies_match_the_cpu_and_loop() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    if !r.gpu_swarms() {
        eprintln!("no compute shaders here: CPU swarms only");
    }
    let mut cpu = Renderer::new(&gpu.device, &gpu.queue, 1);
    cpu.disable_gpu_swarms();
    let target = r.create_target(240, 136);
    let target_cpu = cpu.create_target(240, 136);
    let mut plain = presets::empty();
    plain.post.grade.grain = Param::new(0.0);
    plain
        .layers
        .retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    let mut layouts: Vec<(String, Instancer)> = SwarmForm::ALL
        .into_iter()
        .map(|form| {
            (
                form.label().to_string(),
                Instancer::Swarm {
                    form,
                    count: 3000,
                    radius: 3.0,
                    spread: 1.0,
                    speed: 1,
                    seed: 3,
                },
            )
        })
        .collect();
    layouts.extend(
        [
            Instancer::Single,
            Instancer::Grid {
                counts: [6, 3, 4],
                spacing: [0.8, 0.8, 0.8],
            },
            Instancer::Radial {
                count: 24,
                radius: 3.0,
            },
            Instancer::Scatter {
                count: 400,
                radius: 3.0,
                shell: false,
                seed: 5,
            },
            Instancer::Scatter {
                count: 400,
                radius: 3.0,
                shell: true,
                seed: 5,
            },
            Instancer::Orbit {
                count: 300,
                radius: 3.0,
                spread: 1.0,
                speed: 1,
                seed: 2,
            },
            Instancer::Wall {
                cols: 12,
                rows: 4,
                spacing: 0.6,
                curve: 120.0,
            },
            Instancer::Spiral {
                count: 120,
                radius: 2.5,
                height: 3.0,
                turns: 3.0,
            },
            Instancer::Curve {
                curve: RibbonCurve::Knot,
                freq: [2, 3, 1],
                size: 3.0,
                count: 80,
                laps: 1,
                align: true,
            },
            Instancer::Surface {
                shape: MeshSource::Primitive(Primitive::Torus {
                    thickness: 0.35,
                    segments: 32,
                }),
                size: 3.0,
                count: 300,
                seed: 1,
                align: true,
                lift: 0.0,
            },
        ]
        .into_iter()
        .map(|i| (i.label().to_string(), i)),
    );
    for (k, (name, instancer)) in layouts.into_iter().enumerate() {
        let mut p = plain.clone();
        let mut layer = Layer::new(
            "Swarm",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Cube),
                instancer,
                variation: Variation {
                    rotation: 40.0,
                    scale: 0.4,
                    spin: 2,
                    hue: 0.5,
                    ripple: 0.3,
                    ripple_cycles: 1,
                    ripple_spread: 1.0,
                    ..Default::default()
                },
                ..Default::default()
            }),
        )
        .at([0.0, 1.5, 0.0])
        .scaled(0.12);
        layer.symmetry = Symmetry::MirrorX;
        p.layers.push(layer);
        let g = r.render_image(&p, &at(&p, 0.3), &target);
        let c = cpu.render_image(&p, &at(&p, 0.3), &target_cpu);
        let a = r.render_image(&p, &at(&p, 0.0), &target);
        let b = r.render_image(&p, &at(&p, 1.0), &target);
        let reference = r.render_image(&plain, &at(&p, 0.3), &target);
        g.save(snapshot_dir().join(format!("copies_{k:02}.png")))
            .unwrap();
        let same = mean_abs_diff(g.as_raw(), c.as_raw());
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let shown = mean_abs_diff(g.as_raw(), reference.as_raw());
        eprintln!("{name}: gpu vs cpu {same:.3}, seam {seam:.3}, visible {shown:.2}");
        assert!(same < 0.3, "{name} differs on the GPU");
        assert!(seam < 0.6, "{name} doesn't loop");
        assert!(shown > 0.03, "{name} barely visible");
    }
    // A hundred thousand copies.
    let mut p = plain.clone();
    p.layers.push(
        Layer::new(
            "Galaxy",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Tetrahedron),
                instancer: Instancer::Swarm {
                    form: SwarmForm::Galaxy,
                    count: 100_000,
                    radius: 4.0,
                    spread: 1.0,
                    speed: 1,
                    seed: 1,
                },
                ..Default::default()
            }),
        )
        .at([0.0, 1.0, 0.0])
        .scaled(0.03),
    );
    for (name, rr, t) in [("gpu", &mut r, &target), ("cpu", &mut cpu, &target_cpu)] {
        rr.render_image(&p, &at(&p, 0.1), t);
        let start = std::time::Instant::now();
        for k in 0..3 {
            rr.render_image(&p, &at(&p, 0.2 + k as f32 * 0.01), t);
        }
        eprintln!(
            "100k copies ({name}): {:.1} ms/frame",
            start.elapsed().as_secs_f64() * 1000.0 / 3.0
        );
    }
    let img = r.render_image(&p, &at(&p, 0.2), &target);
    img.save(snapshot_dir().join("swarm_100k.png")).unwrap();
}

/// Logos flat on the screen: text and images, every look on, animated
/// placement. Visible where they are placed, moving and seamless.
#[test]
fn logos_show_where_placed_and_loop() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let (w, h) = (320u32, 180u32);
    let target = r.create_target(w, h);
    let mut plain = presets::empty();
    plain.post.grade.grain = Param::new(0.0);
    plain
        .layers
        .retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    let wobble = |base: f32, amp: f32, cycles: i32| Param {
        amp,
        cycles,
        ..Param::new(base)
    };
    let cases = [
        (
            "text",
            LogoLayer {
                text: "EZ2\nLOGO".into(),
                font: TextFont::Sans,
                x: Param::new(0.05),
                y: Param::new(0.95),
                anchor: LogoAnchor::TopLeft,
                size: Param::new(0.35),
                rotation: wobble(0.0, 8.0, 1),
                glow: Param::new(1.5),
                outline: Param::new(0.4),
                shadow: Param::new(0.6),
                chrome: Param::new(0.6),
                ..Default::default()
            },
        ),
        (
            "coin",
            LogoLayer {
                source: LogoSource::Image,
                image: Some("sheet_coin".into()),
                colors: LogoColors::Image,
                x: wobble(0.2, 0.1, 2),
                y: Param::new(0.8),
                size: Param::new(0.3),
                outline: Param::new(0.3),
                outline_color: [1.0, 1.0, 1.0],
                ..Default::default()
            },
        ),
        (
            "pixel",
            LogoLayer {
                text: "HI".into(),
                font: TextFont::Pixel,
                x: Param::new(0.1),
                y: Param::new(0.9),
                anchor: LogoAnchor::TopLeft,
                size: wobble(0.3, 0.05, 1),
                shadow: Param::new(1.0),
                ..Default::default()
            },
        ),
    ];
    // Mean difference in the top-left and bottom-right quarters.
    let quarters = |a: &image::RgbaImage, b: &image::RgbaImage| {
        let q = |x0: u32, y0: u32| {
            let mut sum = 0.0;
            for y in y0..y0 + h / 2 {
                for x in x0..x0 + w / 2 {
                    let (pa, pb) = (a.get_pixel(x, y), b.get_pixel(x, y));
                    for c in 0..3 {
                        sum += (pa[c] as i32 - pb[c] as i32).unsigned_abs() as f32;
                    }
                }
            }
            sum / (w * h / 4 * 3) as f32
        };
        (q(0, 0), q(w / 2, h / 2))
    };
    for (name, logo) in cases {
        let mut p = plain.clone();
        p.layers.push(Layer::new(name, LayerKind::Logo(logo)));
        let a = r.render_image(&p, &at(&p, 0.0), &target);
        let b = r.render_image(&p, &at(&p, 1.0), &target);
        let mid = r.render_image(&p, &at(&p, 0.3), &target);
        let reference = r.render_image(&plain, &at(&p, 0.3), &target);
        mid.save(snapshot_dir().join(format!("logo_{name}.png")))
            .unwrap();
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let moves = mean_abs_diff(a.as_raw(), mid.as_raw());
        let (top_left, bottom_right) = quarters(&mid, &reference);
        eprintln!(
            "{name}: seam {seam:.3}, moves {moves:.2}, top left {top_left:.2}, \
             bottom right {bottom_right:.3}"
        );
        assert!(seam < 0.6, "{name} doesn't loop");
        assert!(moves > 0.05, "{name} doesn't move");
        assert!(top_left > 4.0, "{name} barely visible");
        // Bloom may spill a little light; nothing is drawn there.
        assert!(bottom_right < 0.2, "{name} shows in the wrong place");
    }
    // Nothing to draw: no text, no image.
    let mut p = plain.clone();
    p.layers.push(Layer::new(
        "empty",
        LayerKind::Logo(LogoLayer {
            text: " ".into(),
            ..Default::default()
        }),
    ));
    p.layers.push(Layer::new(
        "no image",
        LayerKind::Logo(LogoLayer {
            source: LogoSource::Image,
            ..Default::default()
        }),
    ));
    let img = r.render_image(&p, &at(&p, 0.3), &target);
    let reference = r.render_image(&plain, &at(&p, 0.3), &target);
    assert!(mean_abs_diff(img.as_raw(), reference.as_raw()) < 0.05);
}

/// Logos go on after depth of field: a blurred scene leaves them sharp.
#[test]
fn logos_stay_sharp_under_depth_of_field() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(320, 180);
    let mut p = presets::empty();
    p.post.grade.grain = Param::new(0.0);
    p.post.bloom.enabled = false;
    p.layers.push(Layer::new(
        "Logo",
        LayerKind::Logo(LogoLayer {
            text: "SHARP".into(),
            size: Param::new(0.3),
            color_top: [1.0, 1.0, 1.0],
            color_bottom: [1.0, 1.0, 1.0],
            ..Default::default()
        }),
    ));
    let ctx = EvalCtx::new(&p.timing, 0.3, None);
    let sharp = r.render_image(&p, &ctx, &target);
    p.post.dof = DepthOfField {
        enabled: true,
        auto_focus: false,
        focus: Param::new(0.5),
        blur: Param::new(1.0),
    };
    let blurred = r.render_image(&p, &ctx, &target);
    blurred.save(snapshot_dir().join("logo_dof.png")).unwrap();
    // Inside the logo's middle row the picture is the logo either way.
    let row = 90;
    let mut diff = 0.0;
    let mut n = 0.0;
    for x in 100..220 {
        let (a, b) = (sharp.get_pixel(x, row), blurred.get_pixel(x, row));
        // Only where the logo covers (bright letters).
        if a[0] > 200 && a[1] > 200 {
            for c in 0..3 {
                diff += (a[c] as i32 - b[c] as i32).unsigned_abs() as f32;
            }
            n += 3.0;
        }
    }
    assert!(n > 30.0, "no logo pixels on the row");
    let diff = diff / n;
    eprintln!("logo under depth of field: diff {diff:.2}");
    assert!(diff < 3.0, "the logo is blurred");
}

/// Lit logos: every bevel shades the letters and follows the light, a
/// material sphere changes them, and a glint sweeps across and loops.
#[test]
fn lit_logos_follow_the_light_and_glint_loops() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(320, 180);
    let mut plain = presets::empty();
    plain.post.grade.grain = Param::new(0.0);
    plain.post.bloom.enabled = false;
    plain
        .layers
        .retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    let with = |g: LogoLayer| {
        let mut p = plain.clone();
        p.layers.push(Layer::new("Logo", LayerKind::Logo(g)));
        p
    };
    let base = LogoLayer {
        text: "LIT".into(),
        size: Param::new(0.5),
        color_top: [0.8, 0.8, 0.8],
        color_bottom: [0.8, 0.8, 0.8],
        ..Default::default()
    };
    let flat = r.render_image(&with(base.clone()), &at(&plain, 0.2), &target);
    for bevel in LogoBevel::ALL.into_iter().skip(1) {
        let lit = |angle: f32| LogoLayer {
            bevel,
            bevel_width: Param::new(0.8),
            light_angle: Param::new(angle),
            ..base.clone()
        };
        let a = r.render_image(&with(lit(135.0)), &at(&plain, 0.2), &target);
        let b = r.render_image(&with(lit(-45.0)), &at(&plain, 0.2), &target);
        a.save(snapshot_dir().join(format!("logo_bevel_{}.png", bevel.label())))
            .unwrap();
        let shaded = mean_abs_diff(a.as_raw(), flat.as_raw());
        let turns = mean_abs_diff(a.as_raw(), b.as_raw());
        eprintln!(
            "{}: shaded {shaded:.2}, follows the light {turns:.2}",
            bevel.label()
        );
        assert!(shaded > 0.5, "{} doesn't shade", bevel.label());
        assert!(turns > 0.5, "{} ignores the light", bevel.label());
    }
    // A material sphere.
    let gold = r.render_image(
        &with(LogoLayer {
            bevel: LogoBevel::Round,
            matcap: Some("matcap_gold".into()),
            ..base.clone()
        }),
        &at(&plain, 0.2),
        &target,
    );
    gold.save(snapshot_dir().join("logo_matcap_gold.png"))
        .unwrap();
    assert!(mean_abs_diff(gold.as_raw(), flat.as_raw()) > 1.0);
    // A glint sweeping twice per loop, with a light circling once.
    let p = with(LogoLayer {
        bevel: LogoBevel::Chiselled,
        light_angle: Param::new(0.0).osc(Wave::Saw, 180.0, 1),
        glint: Param::new(2.0),
        glint_cycles: 2,
        glint_width: 0.2,
        ..base.clone()
    });
    let a = r.render_image(&p, &at(&p, 0.0), &target);
    let b = r.render_image(&p, &at(&p, 1.0), &target);
    let mid = r.render_image(&p, &at(&p, 0.25), &target);
    mid.save(snapshot_dir().join("logo_glint.png")).unwrap();
    let seam = mean_abs_diff(a.as_raw(), b.as_raw());
    let sweep = mean_abs_diff(a.as_raw(), mid.as_raw());
    eprintln!("glint: seam {seam:.3}, sweeps {sweep:.2}");
    assert!(seam < 0.6, "the glint doesn't loop");
    assert!(sweep > 0.5, "the glint doesn't move");
}

/// Distance-field effects on logos: rings, stacked outlines, extrusion,
/// dissolve, reveals and morphing all show, animate and loop.
#[test]
fn logo_field_effects_show_and_loop() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(320, 180);
    let mut plain = presets::empty();
    plain.post.grade.grain = Param::new(0.0);
    plain.post.bloom.enabled = false;
    plain
        .layers
        .retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    let with = |g: LogoLayer| {
        let mut p = plain.clone();
        p.layers.push(Layer::new("Logo", LayerKind::Logo(g)));
        p
    };
    let base = LogoLayer {
        text: "FX".into(),
        size: Param::new(0.35),
        ..Default::default()
    };
    let wave = |base: f32, amp: f32| Param {
        amp,
        wave: Wave::Triangle,
        ..Param::new(base)
    };
    let mut render = |p: &Project, phase: f32| r.render_image(p, &at(p, phase), &target);
    let logo = render(&with(base.clone()), 0.3);
    let cases: Vec<(&str, LogoLayer)> = vec![
        (
            "contours",
            LogoLayer {
                contours: Param::new(2.0),
                contour_cycles: 2,
                ..base.clone()
            },
        ),
        (
            "stack",
            LogoLayer {
                stack: 4,
                stack_width: wave(0.04, 0.02),
                ..base.clone()
            },
        ),
        (
            "extrude",
            LogoLayer {
                extrude: wave(0.2, 0.1),
                ..base.clone()
            },
        ),
        (
            "dissolve",
            LogoLayer {
                dissolve: wave(0.5, 0.4),
                ..base.clone()
            },
        ),
        (
            "wipe",
            LogoLayer {
                reveal: LogoReveal::Wipe,
                reveal_amount: wave(0.5, 0.5),
                ..base.clone()
            },
        ),
        (
            "morph",
            LogoLayer {
                morph: wave(0.5, 0.5),
                morph_text: "OK".into(),
                ..base.clone()
            },
        ),
    ];
    for (name, g) in cases {
        let p = with(g);
        let a = render(&p, 0.0);
        let b = render(&p, 1.0);
        let mid = render(&p, 0.3);
        mid.save(snapshot_dir().join(format!("logo_fx_{name}.png")))
            .unwrap();
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let moves = mean_abs_diff(a.as_raw(), mid.as_raw());
        let changes = mean_abs_diff(mid.as_raw(), logo.as_raw());
        eprintln!("{name}: seam {seam:.3}, moves {moves:.2}, changes the logo {changes:.2}");
        assert!(seam < 0.6, "{name} doesn't loop");
        assert!(moves > 0.2, "{name} doesn't animate");
        assert!(changes > 0.3, "{name} doesn't show");
    }
    // Reveals: nothing at 0, the whole logo at 1.
    let empty = render(&plain, 0.3);
    for reveal in LogoReveal::ALL {
        let shown = |amount: f32| {
            with(LogoLayer {
                reveal,
                reveal_amount: Param::new(amount),
                ..base.clone()
            })
        };
        let none = render(&shown(0.0), 0.3);
        let full = render(&shown(1.0), 0.3);
        let hidden = mean_abs_diff(none.as_raw(), empty.as_raw());
        let whole = mean_abs_diff(full.as_raw(), logo.as_raw());
        eprintln!("{}: at 0 {hidden:.3}, at 1 {whole:.3}", reveal.label());
        assert!(hidden < 0.05, "{} shows something at 0", reveal.label());
        assert!(whole < 0.05, "{} hides something at 1", reveal.label());
    }
    // A full morph is the other logo.
    let morphed = render(
        &with(LogoLayer {
            morph: Param::new(1.0),
            morph_text: "OK".into(),
            ..base.clone()
        }),
        0.3,
    );
    let ok = render(
        &with(LogoLayer {
            text: "OK".into(),
            ..base.clone()
        }),
        0.3,
    );
    let diff = mean_abs_diff(morphed.as_raw(), ok.as_raw());
    eprintln!("morph to OK vs OK: {diff:.3}");
    assert!(diff < 0.3, "a full morph isn't the other logo");
}

/// Rasters and distortion on logos: copper bars, wobble, raster glitch
/// and colour split show, move and loop.
#[test]
fn logo_rasters_and_distortion_loop() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(320, 180);
    let mut plain = presets::empty();
    plain.post.grade.grain = Param::new(0.0);
    plain.post.bloom.enabled = false;
    plain
        .layers
        .retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    let with = |g: LogoLayer| {
        let mut p = plain.clone();
        p.layers.push(Layer::new("Logo", LayerKind::Logo(g)));
        p
    };
    let base = LogoLayer {
        text: "RASTER".into(),
        size: Param::new(0.3),
        ..Default::default()
    };
    let mut render = |p: &Project, phase: f32| r.render_image(p, &at(p, phase), &target);
    let logo = render(&with(base.clone()), 0.3);
    let cases: Vec<(&str, LogoLayer)> = vec![
        (
            "copper",
            LogoLayer {
                copper: Param::new(1.0),
                copper_cycles: 2,
                ..base.clone()
            },
        ),
        (
            "wobble",
            LogoLayer {
                wobble_x: Param::new(0.08),
                wobble_y: Param::new(0.05),
                wobble_cycles: 2,
                ..base.clone()
            },
        ),
        (
            "glitch",
            LogoLayer {
                glitch: Param::new(0.15),
                glitch_chance: 0.5,
                ..base.clone()
            },
        ),
        (
            "chroma",
            LogoLayer {
                chroma: Param::new(0.03).osc(Wave::Sine, 0.02, 1),
                ..base.clone()
            },
        ),
    ];
    for (name, g) in cases {
        let p = with(g);
        let a = render(&p, 0.0);
        let b = render(&p, 1.0);
        let mid = render(&p, 0.3);
        mid.save(snapshot_dir().join(format!("logo_raster_{name}.png")))
            .unwrap();
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let moves = mean_abs_diff(a.as_raw(), mid.as_raw());
        let changes = mean_abs_diff(mid.as_raw(), logo.as_raw());
        eprintln!("{name}: seam {seam:.3}, moves {moves:.2}, changes the logo {changes:.2}");
        assert!(seam < 0.6, "{name} doesn't loop");
        assert!(moves > 0.2, "{name} doesn't move");
        assert!(changes > 0.3, "{name} doesn't show");
    }
}

/// Logos attached to the screen and to each other: a subtitle snapped
/// under a title follows it; missing targets and loops fall back to the
/// screen.
#[test]
fn logos_attach_to_the_screen_and_each_other() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let (w, h) = (320.0f32, 180.0f32);
    let target = r.create_target(w as u32, h as u32);
    let mut p = presets::empty();
    p.post.grade.grain = Param::new(0.0);
    p.layers.retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    let logo = |g: LogoLayer, name: &str| Layer::new(name, LayerKind::Logo(g));
    let first = p.layers.len();
    // A title in the top right corner, 5% in from both edges.
    p.layers.push(logo(
        LogoLayer {
            text: "TITLE".into(),
            attach_point: LogoAnchor::TopRight,
            anchor: LogoAnchor::TopRight,
            x: Param::new(-0.05),
            y: Param::new(-0.05),
            size: Param::new(0.2),
            ..Default::default()
        },
        "Title",
    ));
    // Snapped under it, centred, with a small gap.
    p.layers.push(logo(
        LogoLayer {
            text: "subtitle".into(),
            attach_to: "Title".into(),
            attach_point: LogoAnchor::Bottom,
            anchor: LogoAnchor::Top,
            x: Param::new(0.0),
            y: Param::new(-0.02),
            size: Param::new(0.08),
            ..Default::default()
        },
        "Sub",
    ));
    // Attached to something that isn't there: measured from the screen.
    p.layers.push(logo(
        LogoLayer {
            text: "LOST".into(),
            attach_to: "Nobody".into(),
            attach_point: LogoAnchor::Centre,
            x: Param::new(0.0),
            y: Param::new(0.0),
            ..Default::default()
        },
        "Lost",
    ));
    // Two logos attached to each other.
    for (name, other) in [("A", "B"), ("B", "A")] {
        p.layers.push(logo(
            LogoLayer {
                text: name.into(),
                attach_to: other.into(),
                attach_point: LogoAnchor::Centre,
                x: Param::new(0.1),
                y: Param::new(0.0),
                ..Default::default()
            },
            name,
        ));
    }
    let ctx = EvalCtx::new(&p.timing, 0.3, None);
    let at = r.logo_anchors(&p, &ctx, [w, h]);
    let title = at[first].expect("title placed");
    let sub = at[first + 1].expect("subtitle placed");
    let lost = at[first + 2].expect("lost placed");
    assert!((title[0] - 0.95).abs() < 1e-4 && (title[1] - 0.95).abs() < 1e-4);
    // The title's bottom is its height below its top anchor; its middle is
    // half its width left of its right edge.
    let img = r.render_image(&p, &ctx, &target);
    img.save(snapshot_dir().join("logo_attach.png")).unwrap();
    assert!((sub[1] - (0.95 - 0.2 - 0.02)).abs() < 1e-4, "{sub:?}");
    assert!(sub[0] < 0.95 - 0.05 && sub[0] > 0.5, "{sub:?}");
    assert!((lost[0] - 0.5).abs() < 1e-4 && (lost[1] - 0.5).abs() < 1e-4);
    let (a, b) = (at[first + 3].expect("A"), at[first + 4].expect("B"));
    assert!(a[0].is_finite() && b[0].is_finite());
    // Moving the title carries the subtitle along.
    if let LayerKind::Logo(g) = &mut p.layers[first].kind {
        g.y = Param::new(-0.3);
    }
    let moved = r.logo_anchors(&p, &ctx, [w, h]);
    let sub2 = moved[first + 1].unwrap();
    assert!((sub2[1] - (sub[1] - 0.25)).abs() < 1e-4 && (sub2[0] - sub[0]).abs() < 1e-4);
    // Turned a quarter, the title's bottom is to its right... of the anchor.
    if let LayerKind::Logo(g) = &mut p.layers[first].kind {
        g.rotation = Param::new(90.0);
        g.anchor = LogoAnchor::Centre;
    }
    let turned = r.logo_anchors(&p, &ctx, [w, h]);
    let t = turned[first].unwrap();
    let s = turned[first + 1].unwrap();
    // Turned anticlockwise, the bottom points right: the subtitle's anchor
    // is half a title height right of the title's centre (in pixels).
    assert!(((s[0] - t[0]) * w - 0.1 * h).abs() < 0.5, "{t:?} {s:?}");
}

/// Retro looks on logos: pixel blocks, palette (cycling), halftone,
/// scanlines and moiré show, move and loop.
#[test]
fn logo_retro_looks_show_and_loop() {
    use ez_core::palette::PaletteId;
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(320, 180);
    let mut plain = presets::empty();
    plain.post.grade.grain = Param::new(0.0);
    plain.post.bloom.enabled = false;
    plain
        .layers
        .retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    let with = |g: LogoLayer| {
        let mut p = plain.clone();
        p.layers.push(Layer::new("Logo", LayerKind::Logo(g)));
        p
    };
    let base = LogoLayer {
        text: "RETRO".into(),
        size: Param::new(0.35),
        copper: Param::new(1.0),
        ..Default::default()
    };
    let mut render = |p: &Project, phase: f32| r.render_image(p, &at(p, phase), &target);
    let logo = render(&with(base.clone()), 0.3);
    let cases: Vec<(&str, LogoLayer)> = vec![
        (
            "pixelate",
            LogoLayer {
                pixelate: Param::new(0.04).osc(Wave::Triangle, 0.03, 1),
                ..base.clone()
            },
        ),
        (
            "palette",
            LogoLayer {
                palette: Some(PaletteId::C64),
                palette_by_brightness: true,
                palette_cycles: 1,
                ..base.clone()
            },
        ),
        (
            "vga",
            LogoLayer {
                palette: Some(PaletteId::Vga),
                pixelate: Param::new(0.02).osc(Wave::Sine, 0.01, 1),
                ..base.clone()
            },
        ),
        (
            "halftone",
            LogoLayer {
                halftone: Param::new(1.0),
                halftone_size: 0.05,
                glow: Param::new(1.0).osc(Wave::Sine, 0.5, 1),
                ..base.clone()
            },
        ),
        (
            "scanlines",
            LogoLayer {
                scanlines: Param::new(0.8),
                scanline_count: 20.0,
                crt_mask: 0.5,
                crt_glow: Param::new(0.5).osc(Wave::Sine, 0.5, 1),
                ..base.clone()
            },
        ),
        (
            "moire",
            LogoLayer {
                moire: Param::new(1.0),
                moire_lines: 25.0,
                ..base.clone()
            },
        ),
    ];
    for (name, g) in cases {
        let p = with(g);
        let a = render(&p, 0.0);
        let b = render(&p, 1.0);
        let mid = render(&p, 0.3);
        mid.save(snapshot_dir().join(format!("logo_retro_{name}.png")))
            .unwrap();
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let moves = mean_abs_diff(a.as_raw(), mid.as_raw());
        let changes = mean_abs_diff(mid.as_raw(), logo.as_raw());
        eprintln!("{name}: seam {seam:.3}, moves {moves:.2}, changes the logo {changes:.2}");
        assert!(seam < 0.6, "{name} doesn't loop");
        assert!(moves > 0.2, "{name} doesn't move");
        assert!(changes > 0.3, "{name} doesn't show");
    }
}

/// Logos meeting the scene: glass bends what is behind it, rays stream
/// out (the logo's light, or its shadow in the light behind), and echoes
/// trail behind a moving logo; all loop.
#[test]
fn logos_meet_the_scene() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(320, 180);
    let mut plain = presets::empty();
    plain.post.grade.grain = Param::new(0.0);
    plain.post.bloom.enabled = false;
    // Something with detail behind the logo: a spinning cube.
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    let with = |g: LogoLayer| {
        let mut p = plain.clone();
        p.layers.push(Layer::new("Logo", LayerKind::Logo(g)));
        p
    };
    let base = LogoLayer {
        text: "GLASS".into(),
        size: Param::new(0.35),
        ..Default::default()
    };
    let mut render = |p: &Project, phase: f32| r.render_image(p, &at(p, phase), &target);
    let logo = render(&with(base.clone()), 0.3);
    let empty = render(&plain, 0.3);
    // Glass: not the plain logo, not an empty picture, and the bend shows.
    let glass = |bend: f32| LogoLayer {
        glass: Param::new(1.0),
        refraction: bend,
        bevel: LogoBevel::Round,
        ..base.clone()
    };
    let g1 = render(&with(glass(0.15)), 0.3);
    let g0 = render(&with(glass(0.0)), 0.3);
    g1.save(snapshot_dir().join("logo_glass.png")).unwrap();
    let vs_logo = mean_abs_diff(g1.as_raw(), logo.as_raw());
    let bends = mean_abs_diff(g1.as_raw(), g0.as_raw());
    eprintln!("glass: vs the logo {vs_logo:.2}, bending {bends:.2}");
    assert!(vs_logo > 1.0, "glass looks like the plain logo");
    assert!(bends > 0.05, "glass doesn't bend");
    assert!(
        mean_abs_diff(g0.as_raw(), empty.as_raw()) < mean_abs_diff(logo.as_raw(), empty.as_raw())
    );
    // Rays: light beyond the logo's quad, both kinds, and they loop.
    let rays = |shadow: bool| LogoLayer {
        rays: Param::new(1.5).osc(Wave::Sine, 0.5, 1),
        rays_length: 0.6,
        rays_shadow: shadow,
        rays_threshold: if shadow { 0.05 } else { 0.0 },
        glow: Param::new(2.0),
        ..base.clone()
    };
    for (name, shadow) in [("rays", false), ("shadow_rays", true)] {
        let p = with(rays(shadow));
        let a = render(&p, 0.0);
        let b = render(&p, 1.0);
        let mid = render(&p, 0.3);
        mid.save(snapshot_dir().join(format!("logo_{name}.png")))
            .unwrap();
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let changes = mean_abs_diff(mid.as_raw(), logo.as_raw());
        eprintln!("{name}: seam {seam:.3}, changes the picture {changes:.2}");
        assert!(seam < 0.6, "{name} don't loop");
        assert!(changes > 0.3, "{name} don't show");
    }
    // Echoes trail a sliding logo.
    let slide = LogoLayer {
        x: Param::new(0.5).osc(Wave::Sine, 0.3, 1),
        ..base.clone()
    };
    let solo = render(&with(slide.clone()), 0.3);
    let p = with(LogoLayer {
        echoes: 4,
        echo_spacing: 0.03,
        ..slide
    });
    let a = render(&p, 0.0);
    let b = render(&p, 1.0);
    let mid = render(&p, 0.3);
    mid.save(snapshot_dir().join("logo_echoes.png")).unwrap();
    let seam = mean_abs_diff(a.as_raw(), b.as_raw());
    let trail = mean_abs_diff(mid.as_raw(), solo.as_raw());
    eprintln!("echoes: seam {seam:.3}, trail {trail:.2}");
    assert!(seam < 0.6, "echoes don't loop");
    assert!(trail > 0.3, "no trail");
}

/// The colour scheme: off it changes nothing, on it recolours the scene,
/// layers can keep their colours, and a turning key loops.
#[test]
fn color_scheme_recolours_and_loops() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(320, 180);
    let mut p = presets::neon_arena();
    p.post.grade.grain = Param::new(0.0);
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    let mut render = |p: &Project, phase: f32| r.render_image(p, &at(p, phase), &target);
    let before = render(&p, 0.3);
    // Off (the default): identical.
    let mut off = p.clone();
    off.color_scheme.key = color::hex(0x20ff80);
    assert_eq!(
        render(&off, 0.3).as_raw(),
        before.as_raw(),
        "an off scheme changed the picture"
    );
    // On: recoloured.
    let mut on = off.clone();
    on.color_scheme.enabled = true;
    on.color_scheme.harmony = Harmony::Mono;
    let green = render(&on, 0.3);
    green.save(snapshot_dir().join("scheme_green.png")).unwrap();
    let changed = mean_abs_diff(green.as_raw(), before.as_raw());
    eprintln!("scheme changes the picture by {changed:.2}");
    assert!(changed > 2.0, "the scheme barely shows");
    // Every layer keeping its colours, environment left out: as before.
    let mut kept = on.clone();
    kept.color_scheme.environment = false;
    for l in &mut kept.layers {
        l.keep_colors = true;
    }
    let same = mean_abs_diff(render(&kept, 0.3).as_raw(), before.as_raw());
    assert!(same < 0.05, "kept layers still changed ({same})");
    // A key turning once per loop loops, and moves.
    let mut turning = on.clone();
    turning.color_scheme.key_turn = Param::new(0.0).osc(Wave::Saw, 180.0, 1);
    let a = render(&turning, 0.0);
    let b = render(&turning, 1.0);
    let mid = render(&turning, 0.5);
    mid.save(snapshot_dir().join("scheme_turned.png")).unwrap();
    let seam = mean_abs_diff(a.as_raw(), b.as_raw());
    let moves = mean_abs_diff(a.as_raw(), mid.as_raw());
    eprintln!("turning scheme: seam {seam:.3}, moves {moves:.2}");
    assert!(seam < 0.6, "a turning scheme doesn't loop");
    assert!(moves > 1.0, "a turning scheme doesn't turn");
}
