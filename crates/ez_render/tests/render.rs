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
    r.set_wait_for_bakes(true);
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

/// A shape morph: visible, its amount changes the picture, the ends look
/// like the two shapes (a cube, a torus with its hole), and turning it off
/// draws the plain mesh again.
#[test]
fn morph_blends_two_shapes() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = r.create_target(160, 160);
    let mut p = presets::empty();
    p.post.grade.grain = Param::new(0.0);
    p.layers.retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    let plain = p.clone();
    p.camera.target = [0.0, 1.2, 0.0];
    p.camera.distance = Param::new(4.0);
    p.camera.height = Param::new(3.0);
    let with = |amount: f32, on: bool| {
        let mut q = p.clone();
        let mut layer = Layer::new(
            "Morph",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Cube),
                morph: ShapeMorph {
                    enabled: on,
                    target: MeshSource::Primitive(Primitive::Torus {
                        thickness: 0.3,
                        segments: 48,
                    }),
                    amount: Param::new(amount),
                },
                ..Default::default()
            }),
        )
        .at([0.0, 1.2, 0.0]);
        layer.transform.scale = Param::new(1.2);
        q.layers.push(layer);
        q
    };
    let ctx = EvalCtx::new(&p.timing, 0.0, None);
    let empty = r.render_image(&plain, &ctx, &target);
    let a = r.render_image(&with(0.0, true), &ctx, &target);
    let mid = r.render_image(&with(0.5, true), &ctx, &target);
    let b = r.render_image(&with(1.0, true), &ctx, &target);
    let mesh = r.render_image(&with(0.5, false), &ctx, &target);
    mid.save(snapshot_dir().join("morph_mid.png")).unwrap();
    b.save(snapshot_dir().join("morph_torus.png")).unwrap();
    let shown = mean_abs_diff(a.as_raw(), empty.as_raw());
    let changes = mean_abs_diff(a.as_raw(), b.as_raw());
    let between =
        mean_abs_diff(mid.as_raw(), a.as_raw()).min(mean_abs_diff(mid.as_raw(), b.as_raw()));
    // The morph at 0 is a (slightly rounded) cube: close to the mesh cube.
    let like_cube = mean_abs_diff(a.as_raw(), mesh.as_raw());
    eprintln!("morph: shown {shown:.2}, changes {changes:.2}, between {between:.2}, like cube {like_cube:.2}");
    assert!(shown > 1.0, "morph not visible");
    assert!(changes > 1.0, "amount changes nothing");
    assert!(between > 0.2, "halfway is one of the ends");
    assert!(
        like_cube < changes,
        "the start doesn't look like the layer's shape"
    );
    // The torus's hole: the centre pixel shows the background through it
    // when seen from above.
    let mut top = with(1.0, true);
    top.camera.height = Param::new(4.0);
    top.camera.distance = Param::new(0.3);
    let img = r.render_image(&top, &ctx, &target);
    let bg = r.render_image(
        &{
            let mut q = plain.clone();
            q.camera = top.camera.clone();
            q
        },
        &ctx,
        &target,
    );
    let c = |im: &image::RgbaImage| im.get_pixel(80, 80).0;
    let (hole, back) = (c(&img), c(&bg));
    let d: i32 = (0..3)
        .map(|i| (hole[i] as i32 - back[i] as i32).abs())
        .sum();
    assert!(d < 40, "no hole in the torus: {hole:?} vs {back:?}");
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

/// Morphing a logo into an image uses the morph image's own "Shape from":
/// bright and dark parts give different logos, and unset it follows the
/// logo's own setting (as projects saved before worked).
#[test]
fn logo_morph_image_has_its_own_mask() {
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
    plain.post.bloom.enabled = false;
    plain
        .layers
        .retain(|l| !matches!(l.kind, LayerKind::Mesh(_)));
    let with = |mask: Option<LogoMask>, own: LogoMask| {
        let mut p = plain.clone();
        p.layers.push(Layer::new(
            "Logo",
            LayerKind::Logo(LogoLayer {
                text: "FX".into(),
                size: Param::new(0.35),
                mask: own,
                morph: Param::new(1.0),
                morph_source: LogoSource::Image,
                morph_image: Some("xor".into()),
                morph_mask: mask,
                ..Default::default()
            }),
        ));
        p
    };
    let ctx = EvalCtx::new(&plain.timing, 0.0, None);
    let mut render = |p: &Project| r.render_image(p, &ctx, &target);
    let empty = render(&plain);
    let bright = render(&with(Some(LogoMask::Bright), LogoMask::Alpha));
    let dark = render(&with(Some(LogoMask::Dark), LogoMask::Alpha));
    let inherited = render(&with(None, LogoMask::Bright));
    let shown = mean_abs_diff(bright.as_raw(), empty.as_raw());
    let differ = mean_abs_diff(bright.as_raw(), dark.as_raw());
    let same = mean_abs_diff(bright.as_raw(), inherited.as_raw());
    eprintln!("morph mask: shown {shown:.2}, bright vs dark {differ:.2}, inherited {same:.3}");
    assert!(shown > 0.5, "morph image not visible");
    assert!(differ > 0.5, "Shape from makes no difference");
    assert!(same < 0.01, "unset doesn't follow the logo's own setting");
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

/// Battle backgrounds loop and move with every pattern and line warp, and
/// the line wobble, VHS and ASCII effects loop and change the picture.
#[test]
fn battle_backgrounds_and_retro_effects_loop() {
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
    let dir = snapshot_dir();
    let base = presets::battle_screen();
    let mut failures: Vec<String> = Vec::new();
    let check = |r: &mut Renderer, p: &Project, name: &str, failures: &mut Vec<String>| {
        let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);
        let a = r.render_image(p, &at(0.0), &target);
        let b = r.render_image(p, &at(1.0), &target);
        let mid = r.render_image(p, &at(0.37), &target);
        a.save(dir.join(format!("battle_{name}.png"))).unwrap();
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let motion = mean_abs_diff(a.as_raw(), mid.as_raw());
        let lum = a.as_raw().iter().map(|v| *v as f32).sum::<f32>() / a.as_raw().len() as f32;
        eprintln!("{name:<28} seam {seam:.3}  motion {motion:.2}  lum {lum:.1}");
        if seam > 0.6 || motion < 1.0 || lum < 3.0 {
            failures.push(format!("{name}: seam {seam} motion {motion} lum {lum}"));
        }
        a
    };
    for (i, pattern) in BattlePattern::ALL.into_iter().enumerate() {
        let mut p = base.clone();
        if let LayerKind::Backdrop(b) = &mut p.layers[0].kind {
            b.battle.back.pattern = pattern;
            b.battle.back.warp = LineWarp::ALL[i % 4];
            b.battle.front.warp = LineWarp::ALL[(i + 1) % 4];
            b.battle.blend = BattleBlend::ALL[i % 4];
        }
        check(&mut r, &p, &format!("{pattern:?}"), &mut failures);
    }
    let plain = check(&mut r, &base, "plain", &mut failures);
    type Effect = (&'static str, fn(&mut PostStack));
    let effects: [Effect; 5] = [
        ("wobble_wave", |s| s.wobble.enabled = true),
        ("wobble_interlaced", |s| {
            s.wobble.enabled = true;
            s.wobble.mode = LineWarp::Interlaced;
        }),
        ("vhs", |s| {
            s.vhs.enabled = true;
            s.vhs.amount = Param::new(1.0);
        }),
        ("ascii", |s| s.ascii.enabled = true),
        ("ascii_green", |s| {
            s.ascii.enabled = true;
            s.ascii.color = AsciiColor::Green;
        }),
    ];
    for (name, f) in effects {
        let mut p = base.clone();
        f(&mut p.post);
        let img = check(&mut r, &p, name, &mut failures);
        let d = mean_abs_diff(img.as_raw(), plain.as_raw());
        if d < 2.0 {
            failures.push(format!("{name} barely changes the picture: {d}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The fisheye lens bends the picture both ways and keeps it seamless, and
/// a picture tiled mirrored flips at every edge (and still loops when it
/// scrolls).
#[test]
fn lens_and_mirrored_tiling() {
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
    let dir = snapshot_dir();
    let base = presets::battle_screen();
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    let plain = r.render_image(&base, &at(&base, 0.2), &target);
    for amount in [0.8, -0.8] {
        let mut p = base.clone();
        p.post.lens.enabled = true;
        p.post.lens.amount = Param::new(amount);
        let a = r.render_image(&p, &at(&p, 0.0), &target);
        let b = r.render_image(&p, &at(&p, 1.0), &target);
        let mid = r.render_image(&p, &at(&p, 0.2), &target);
        mid.save(dir.join(format!("lens_{amount}.png"))).unwrap();
        let seam = mean_abs_diff(a.as_raw(), b.as_raw());
        let bent = mean_abs_diff(mid.as_raw(), plain.as_raw());
        eprintln!("lens {amount}: seam {seam:.3} bent {bent:.2}");
        assert!(seam < 0.6, "lens {amount} breaks the loop: {seam}");
        assert!(
            bent > 3.0,
            "lens {amount} barely changes the picture: {bent}"
        );
    }

    // A picture that doesn't tile: a gradient from dark (left) to light.
    let path = dir.join("mirror_tile.png");
    image::RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba([(x * 4) as u8, (y * 4) as u8, 40, 255])
    })
    .save(&path)
    .unwrap();
    let mut p = base.clone();
    p.post = PostStack::default();
    p.layers.truncate(1);
    if let LayerKind::Backdrop(b) = &mut p.layers[0].kind {
        b.texture = Some("tile".into());
        b.color_a = [0.0; 3];
        b.color_b = [1.0; 3];
        b.color_c = [1.0; 3];
        b.battle = Battle {
            back: BattleLayer {
                pattern: BattlePattern::Picture,
                warp: LineWarp::None,
                tiles: Param::new(2.0),
                scroll: [1, 0],
                bands: Param::new(0.45),
                cycles: 0,
                ..Default::default()
            },
            lines: 0,
            steps: 0,
            ..Default::default()
        };
    }
    let seam_jump = |r: &mut Renderer, p: &Project, name: &str| {
        let img = r.render_image(p, &at(p, 0.0), &target);
        img.save(dir.join(format!("{name}.png"))).unwrap();
        // The largest brightness jump along a row: where tiles meet.
        let y = 20;
        let lum = |x: u32| {
            let p = img.get_pixel(x, y);
            (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0
        };
        let jump = (1..img.width())
            .map(|x| (lum(x) - lum(x - 1)).abs())
            .fold(0.0f32, f32::max);
        let b = r.render_image(p, &at(p, 1.0), &target);
        let loop_seam = mean_abs_diff(img.as_raw(), b.as_raw());
        eprintln!("{name}: largest jump {jump:.1}, loop seam {loop_seam:.3}");
        (jump, loop_seam)
    };
    p.textures = vec![UserTexture {
        name: "tile".into(),
        path: path.to_string_lossy().to_string(),
        retro: None,
        mirror: false,
    }];
    let (jump_repeat, _) = seam_jump(&mut r, &p, "mirror_off");
    p.textures[0].mirror = true;
    let (jump_mirror, seam) = seam_jump(&mut r, &p, "mirror_on");
    assert!(
        jump_repeat > 40.0,
        "plain repeat shows its seams: {jump_repeat}"
    );
    assert!(
        jump_mirror < 15.0,
        "mirrored tiling has no seams: {jump_mirror}"
    );
    assert!(seam < 0.6, "mirrored scrolling still loops: {seam}");
}

/// A flock: nothing until baked (and the frame says so), then the same
/// birds whether the renderer waited or the preview polled; it moves and
/// loops.
#[test]
fn flocks_bake_fly_and_loop() {
    use ez_core::sim::Flock;
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut p = presets::starling_dusk();
    for l in &mut p.layers {
        if let Some(Instancer::Flock { flock, .. }) = l.kind.instancer_mut() {
            **flock = Flock {
                count: 150,
                ..(**flock).clone()
            };
        }
    }
    let mut empty = p.clone();
    empty.layers.retain(|l| l.kind.instancer().is_none());
    let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);

    // The preview: nothing until the bake is ready, and it says so.
    let mut preview = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = preview.create_target(160, 90);
    let sky = preview.render_image(&empty, &at(0.2), &target);
    preview.take_inexact();
    let before = preview.render_image(&p, &at(0.2), &target);
    assert!(preview.take_inexact());
    assert!(mean_abs_diff(before.as_raw(), sky.as_raw()) < 0.01);
    let start = std::time::Instant::now();
    while preview.bake_progress().is_some() {
        assert!(start.elapsed().as_secs() < 120, "bake never finished");
        preview.poll_bakes(|| false);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let polled = preview.render_image(&p, &at(0.2), &target);
    assert!(!preview.take_inexact());

    // An export waits, and draws the same.
    let mut export = Renderer::new(&gpu.device, &gpu.queue, 1);
    export.set_wait_for_bakes(true);
    let waited = export.render_image(&p, &at(0.2), &target);
    assert!(!export.take_inexact());
    assert_eq!(waited.as_raw(), polled.as_raw());
    let birds = mean_abs_diff(waited.as_raw(), sky.as_raw());
    assert!(birds > 0.05, "no birds: {birds}");

    // They fly, and the loop closes.
    let a = export.render_image(&p, &at(0.0), &target);
    let b = export.render_image(&p, &at(1.0), &target);
    let later = export.render_image(&p, &at(0.3), &target);
    let (seam, motion) = (
        mean_abs_diff(a.as_raw(), b.as_raw()),
        mean_abs_diff(waited.as_raw(), later.as_raw()),
    );
    // The sky also moves: compare with it.
    let sky_later = export.render_image(&empty, &at(0.3), &target);
    let sky_motion = mean_abs_diff(sky.as_raw(), sky_later.as_raw());
    eprintln!("flock: birds {birds:.3}, seam {seam:.4}, motion {motion:.3} (sky {sky_motion:.3})");
    assert!(seam < 0.05, "seam {seam}");
    assert!(
        motion > sky_motion + 0.02,
        "birds don't move: {motion} vs {sky_motion}"
    );
    waited.save(snapshot_dir().join("flock_0.2.png")).unwrap();
}

/// Cloth: drawn at rest until baked (and the frame says so), then the same
/// sheet whether the renderer waited or the preview polled; it waves and
/// loops.
#[test]
fn cloth_waves_and_loops() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let mut p = presets::banners();
    for l in &mut p.layers {
        if let LayerKind::Mesh(MeshLayer {
            source: MeshSource::Cloth { cloth, .. },
            ..
        }) = &mut l.kind
        {
            cloth.detail = 12;
        }
    }
    let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);

    let mut preview = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = preview.create_target(160, 90);
    preview.take_inexact();
    let rest = preview.render_image(&p, &at(0.2), &target);
    assert!(preview.take_inexact());
    let start = std::time::Instant::now();
    while preview.bake_progress().is_some() {
        assert!(start.elapsed().as_secs() < 120, "bake never finished");
        preview.poll_bakes(|| false);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let polled = preview.render_image(&p, &at(0.2), &target);
    assert!(!preview.take_inexact());

    let mut export = Renderer::new(&gpu.device, &gpu.queue, 1);
    export.set_wait_for_bakes(true);
    let waited = export.render_image(&p, &at(0.2), &target);
    assert_eq!(waited.as_raw(), polled.as_raw());
    // Blown by the wind, the flags are not where they rest.
    let blown = mean_abs_diff(waited.as_raw(), rest.as_raw());

    let a = export.render_image(&p, &at(0.0), &target);
    let b = export.render_image(&p, &at(1.0), &target);
    let seam = mean_abs_diff(a.as_raw(), b.as_raw());
    // They wave: compare with the same scene without the flags (the camera
    // and the sky move too).
    let mut still = p.clone();
    still.layers.retain(|l| l.name != "Flags");
    let later = export.render_image(&p, &at(0.23), &target);
    let (s0, s1) = (
        export.render_image(&still, &at(0.2), &target),
        export.render_image(&still, &at(0.23), &target),
    );
    let (motion, scene_motion) = (
        mean_abs_diff(waited.as_raw(), later.as_raw()),
        mean_abs_diff(s0.as_raw(), s1.as_raw()),
    );
    eprintln!(
        "cloth: blown {blown:.3}, seam {seam:.4}, motion {motion:.3} (scene {scene_motion:.3})"
    );
    assert!(
        blown > 0.3,
        "the flags didn't move off their rest pose: {blown}"
    );
    assert!(seam < 0.05, "seam {seam}");
    assert!(
        motion > scene_motion + 0.05,
        "flags don't wave: {motion} vs {scene_motion}"
    );
    waited.save(snapshot_dir().join("cloth_0.2.png")).unwrap();
}

/// Rigid bodies: nothing until baked (and the frame says so), then the
/// same picture whether the renderer waited or the preview polled; the
/// wall stands, falls after the blast, and the loop closes.
#[test]
fn rigid_bodies_fall_and_loop() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let p = presets::beat_demolition();
    let mut bare = p.clone();
    bare.layers.retain(|l| l.kind.instancer().is_none());
    let at = |phase: f32| EvalCtx::new(&p.timing, phase, None);

    let mut preview = Renderer::new(&gpu.device, &gpu.queue, 1);
    let target = preview.create_target(160, 90);
    let empty = preview.render_image(&bare, &at(0.05), &target);
    preview.take_inexact();
    let before = preview.render_image(&p, &at(0.05), &target);
    assert!(preview.take_inexact());
    assert!(mean_abs_diff(before.as_raw(), empty.as_raw()) < 0.01);
    let start = std::time::Instant::now();
    while preview.bake_progress().is_some() {
        assert!(start.elapsed().as_secs() < 180, "bake never finished");
        preview.poll_bakes(|| false);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let polled = preview.render_image(&p, &at(0.05), &target);

    let mut export = Renderer::new(&gpu.device, &gpu.queue, 1);
    export.set_wait_for_bakes(true);
    let waited = export.render_image(&p, &at(0.05), &target);
    assert_eq!(waited.as_raw(), polled.as_raw());
    let shown = mean_abs_diff(waited.as_raw(), empty.as_raw());

    // The wall at the same moment of the camera's swing, standing and
    // fallen: compared with the scene without it.
    let fallen = export.render_image(&p, &at(0.4), &target);
    let bare_late = export.render_image(&bare, &at(0.4), &target);
    let (standing_vs_bare, fallen_vs_bare) =
        (shown, mean_abs_diff(fallen.as_raw(), bare_late.as_raw()));
    let a = export.render_image(&p, &at(0.0), &target);
    let b = export.render_image(&p, &at(1.0), &target);
    let seam = mean_abs_diff(a.as_raw(), b.as_raw());
    eprintln!("physics: shown {standing_vs_bare:.3}, fallen {fallen_vs_bare:.3}, seam {seam:.4}");
    assert!(shown > 0.5, "no wall: {shown}");
    // Fallen blocks lie low and scattered: the picture changes a lot.
    let change = mean_abs_diff(waited.as_raw(), fallen.as_raw());
    assert!(change > 1.0, "the wall didn't fall: {change}");
    assert!(seam < 0.05, "seam {seam}");
    waited
        .save(snapshot_dir().join("physics_0.05.png"))
        .unwrap();
    fallen.save(snapshot_dir().join("physics_0.4.png")).unwrap();
}

/// Image-based lighting: a white rough sphere vanishes into a uniform
/// environment (the "furnace" test: no light made or lost), a mirror ball
/// shows the map the right way round, and a whole turn changes nothing.
#[test]
fn environment_maps_light_the_scene() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let dir = snapshot_dir().join("ibl");
    std::fs::create_dir_all(&dir).unwrap();
    // Panoramas as .hdr files, read back like a user's.
    let write = |name: &str, f: &dyn Fn(glam::Vec3) -> [f32; 3]| {
        let (w, h) = (256u32, 128u32);
        let img = image::Rgb32FImage::from_fn(w, h, |x, y| {
            let d =
                ez_render::envmap::dir_of((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            image::Rgb(f(d))
        });
        let path = dir.join(name);
        image::DynamicImage::ImageRgb32F(img)
            .save_with_format(&path, image::ImageFormat::Hdr)
            .unwrap();
        path.to_string_lossy().to_string()
    };
    let uniform = write("uniform.hdr", &|_| [0.4, 0.4, 0.4]);
    // Colour by the main axis: +x red, -x green, +y white, -y black,
    // +z blue, -z yellow.
    let axes = write("axes.hdr", &|d| {
        let a = d.abs();
        if a.x >= a.y && a.x >= a.z {
            if d.x > 0.0 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            }
        } else if a.y >= a.z {
            if d.y > 0.0 {
                [1.0, 1.0, 1.0]
            } else {
                [0.0, 0.0, 0.0]
            }
        } else if d.z > 0.0 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 1.0, 0.0]
        }
    });
    let scene = |source: EnvSource, metallic: f32, rough: f32, turn: f32| {
        let mut p = Project {
            camera: Camera {
                mode: CameraMode::Static,
                target: [0.0; 3],
                distance: Param::new(3.0),
                height: Param::new(0.0),
                angle: Param::new(0.0),
                fov: Param::new(40.0),
                ..Default::default()
            },
            ..Default::default()
        };
        p.environment.fog_density = Param::new(0.0);
        p.environment.light_intensity = Param::new(0.0);
        // No darkened corners: the sky is compared across the picture.
        p.post.grade.vignette = Param::new(0.0);
        p.environment.env_light = EnvLight {
            source,
            rotation: Param::new(turn),
            ..Default::default()
        };
        p.layers.push(Layer::new(
            "Map",
            LayerKind::Backdrop(Backdrop {
                kind: BackdropKind::Environment,
                detail: Param::new(1.0),
                ..Default::default()
            }),
        ));
        p.layers.push(Layer::new(
            "Ball",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Sphere { detail: 4 }),
                material: Material {
                    base_color: [1.0; 3],
                    metallic: Param::new(metallic),
                    roughness: Param::new(rough),
                    rim: Param::new(0.0),
                    ..Default::default()
                },
                ..Default::default()
            }),
        ));
        p
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let (w, h) = (160u32, 160u32);
    let target = r.create_target(w, h);
    let ctx = EvalCtx::at(0.0);
    let px = |img: &image::RgbaImage, x: f32, y: f32| {
        let p = img.get_pixel((x * w as f32) as u32, (y * h as f32) as u32);
        [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    // Where a point of the ball lands on the picture.
    let cam = scene(EnvSource::Colours, 0.0, 1.0, 0.0).camera.eval(&ctx);
    let screen = |p: glam::Vec3| {
        let c = cam.proj(1.0) * cam.view() * p.extend(1.0);
        (c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5)
    };

    // Furnace: the rough white ball is as bright as the sky around it.
    let furnace = r.render_image(
        &scene(EnvSource::Hdri(uniform.clone()), 0.0, 1.0, 0.0),
        &ctx,
        &target,
    );
    furnace.save(dir.join("furnace.png")).unwrap();
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let (ball, sky) = (px(&furnace, 0.5, 0.5), px(&furnace, 0.5, 0.08));
    eprintln!("furnace: ball {ball:?}, sky {sky:?}");
    assert!(sky[0] > 30.0, "the sky is black: {sky:?}");
    assert!(
        (ball[0] / sky[0] - 1.0).abs() < 0.08,
        "ball {ball:?} vs sky {sky:?}"
    );

    // A mirror ball: +x on its right, -x on its left, +z (towards the
    // camera) in its middle.
    let mirror = r.render_image(
        &scene(EnvSource::Hdri(axes.clone()), 1.0, 0.0, 0.0),
        &ctx,
        &target,
    );
    mirror.save(dir.join("mirror.png")).unwrap();
    let s = 0.5f32.sqrt();
    let (rx, ry) = screen(glam::Vec3::new(s, 0.0, s));
    let (lx, ly) = screen(glam::Vec3::new(-s, 0.0, s));
    let (right, left, middle) = (
        px(&mirror, rx, ry),
        px(&mirror, lx, ly),
        px(&mirror, 0.5, 0.5),
    );
    eprintln!("mirror: right {right:?}, left {left:?}, middle {middle:?}");
    assert!(
        right[0] > 2.0 * right[1] && right[0] > 2.0 * right[2],
        "right should be red: {right:?}"
    );
    assert!(
        left[1] > 2.0 * left[0] && left[1] > 2.0 * left[2],
        "left should be green: {left:?}"
    );
    assert!(
        middle[2] > 2.0 * middle[0] && middle[2] > 2.0 * middle[1],
        "middle should be blue: {middle:?}"
    );
    // The map turns around +y: by -90°, its +x comes round to +z, which
    // the ball's middle reflects.
    let turned = r.render_image(
        &scene(EnvSource::Hdri(axes.clone()), 1.0, 0.0, -90.0),
        &ctx,
        &target,
    );
    let m = px(&turned, 0.5, 0.5);
    eprintln!("turned -90°: middle {m:?}");
    assert!(
        m[0] > 2.0 * m[1] && m[0] > 2.0 * m[2],
        "turned, the middle should be red: {m:?}"
    );

    // A whole turn is no turn.
    let studio = EnvSource::Studio(Studio::Sunset);
    let a = r.render_image(&scene(studio.clone(), 1.0, 0.2, 0.0), &ctx, &target);
    let b = r.render_image(&scene(studio.clone(), 1.0, 0.2, 360.0), &ctx, &target);
    let d = mean_abs_diff(a.as_raw(), b.as_raw());
    assert!(d < 0.05, "a whole turn changed the picture by {d}");
    // From the sky: a mirror ball in a gradient sky (red above, blue
    // below) shows red near its top and blue near its bottom.
    let mut sky = scene(EnvSource::Sky, 1.0, 0.0, 0.0);
    if let LayerKind::Backdrop(b) = &mut sky.layers[0].kind {
        b.kind = BackdropKind::Gradient;
        b.color_a = [1.0, 0.1, 0.05];
        b.color_b = [0.05, 0.1, 1.0];
        b.color_c = [0.0; 3];
    }
    let img = r.render_image(&sky, &ctx, &target);
    img.save(dir.join("sky.png")).unwrap();
    let (tx, ty) = screen(glam::Vec3::new(0.0, 0.8, 0.6));
    let (bx, by) = screen(glam::Vec3::new(0.0, -0.8, 0.6));
    let (top, bottom) = (px(&img, tx, ty), px(&img, bx, by));
    eprintln!("sky: top {top:?}, bottom {bottom:?}");
    assert!(
        top[0] > 2.0 * top[2],
        "the ball's top should reflect the red sky: {top:?}"
    );
    assert!(
        bottom[2] > 2.0 * bottom[0],
        "its bottom the blue ground: {bottom:?}"
    );
    // Captured once, it stays as it was.
    sky.environment.env_light.sky_static = true;
    let once = r.render_image(&sky, &ctx, &target);
    assert!(mean_abs_diff(once.as_raw(), img.as_raw()) < 0.05);

    // Colours: no map, the old look.
    let plain = r.render_image(&scene(EnvSource::Colours, 1.0, 0.2, 0.0), &ctx, &target);
    assert!(mean_abs_diff(a.as_raw(), plain.as_raw()) > 1.0);
}

/// Physical shading: a furnace test (rough white and rough metal under a
/// uniform map are as bright as it), and clearcoat, glass, sheen and the
/// occlusion/roughness/metal and glow maps each doing their job.
#[test]
fn physical_materials_keep_energy() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let dir = snapshot_dir().join("pbr");
    std::fs::create_dir_all(&dir).unwrap();
    let write_hdr = |name: &str, f: &dyn Fn(glam::Vec3) -> [f32; 3]| {
        let (w, h) = (256u32, 128u32);
        let img = image::Rgb32FImage::from_fn(w, h, |x, y| {
            let d =
                ez_render::envmap::dir_of((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            image::Rgb(f(d))
        });
        let path = dir.join(name);
        image::DynamicImage::ImageRgb32F(img)
            .save_with_format(&path, image::ImageFormat::Hdr)
            .unwrap();
        path.to_string_lossy().to_string()
    };
    let write_png = |name: &str, c: [u8; 3]| {
        let img = image::RgbaImage::from_pixel(8, 8, image::Rgba([c[0], c[1], c[2], 255]));
        let path = dir.join(name);
        img.save(&path).unwrap();
        path.to_string_lossy().to_string()
    };
    let uniform = write_hdr("uniform.hdr", &|_| [0.4, 0.4, 0.4]);
    // Blue in front of the ball (+z, towards the camera), yellow behind.
    let front_back = write_hdr("front_back.hdr", &|d| {
        if d.z > 0.0 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 1.0, 0.0]
        }
    });
    let scene = |source: EnvSource, mat: Material| {
        let mut p = Project {
            camera: Camera {
                mode: CameraMode::Static,
                target: [0.0; 3],
                distance: Param::new(3.0),
                height: Param::new(0.0),
                angle: Param::new(0.0),
                fov: Param::new(40.0),
                ..Default::default()
            },
            ..Default::default()
        };
        p.environment.fog_density = Param::new(0.0);
        p.environment.light_intensity = Param::new(0.0);
        p.post.grade.vignette = Param::new(0.0);
        p.environment.env_light = EnvLight {
            source,
            ..Default::default()
        };
        p.layers.push(Layer::new(
            "Map",
            LayerKind::Backdrop(Backdrop {
                kind: BackdropKind::Environment,
                detail: Param::new(1.0),
                ..Default::default()
            }),
        ));
        p.layers.push(Layer::new(
            "Ball",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Sphere { detail: 4 }),
                material: mat,
                ..Default::default()
            }),
        ));
        p
    };
    let physical = |color: [f32; 3], metallic: f32, rough: f32| Material {
        base_color: color,
        metallic: Param::new(metallic),
        roughness: Param::new(rough),
        rim: Param::new(0.0),
        pbr: Pbr {
            shading: Shading::Physical,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let (w, h) = (160u32, 160u32);
    let target = r.create_target(w, h);
    let ctx = EvalCtx::at(0.0);
    let px = |img: &image::RgbaImage, x: f32, y: f32| {
        let p = img.get_pixel((x * w as f32) as u32, (y * h as f32) as u32);
        [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    // Mean brightness of the ball's disc (inside 80% of its radius).
    let ball_mean = |img: &image::RgbaImage| {
        let (mut sum, mut n) = (0.0, 0.0);
        for (x, y, p) in img.enumerate_pixels() {
            let dx = (x as f32 + 0.5) / w as f32 - 0.5;
            let dy = (y as f32 + 0.5) / h as f32 - 0.5;
            if (dx * dx + dy * dy).sqrt() < 0.2 {
                sum += (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0;
                n += 1.0;
            }
        }
        sum / n
    };

    // Furnace: rough white plastic and rough and half-rough white metal
    // keep (nearly) all the light.
    for (metallic, rough) in [(0.0, 1.0), (0.0, 0.5), (1.0, 1.0), (1.0, 0.5)] {
        let img = r.render_image(
            &scene(
                EnvSource::Hdri(uniform.clone()),
                physical([1.0; 3], metallic, rough),
            ),
            &ctx,
            &target,
        );
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        let (ball, sky) = (ball_mean(&img), px(&img, 0.5, 0.08)[0]);
        eprintln!("furnace metallic {metallic} rough {rough}: ball {ball:.1}, sky {sky:.1}");
        img.save(dir.join(format!("furnace_{metallic}_{rough}.png")))
            .unwrap();
        assert!(
            ball / sky > 0.95 && ball / sky < 1.05,
            "metallic {metallic}, rough {rough}: ball {ball} vs sky {sky}"
        );
    }

    // A clearcoat adds a reflection on top of black paint.
    let black = physical([0.02; 3], 0.0, 0.8);
    let mut coated = black.clone();
    coated.pbr.clearcoat = Param::new(1.0);
    let map = EnvSource::Hdri(front_back.clone());
    let a = ball_mean(&r.render_image(&scene(map.clone(), black.clone()), &ctx, &target));
    let b = ball_mean(&r.render_image(&scene(map.clone(), coated), &ctx, &target));
    eprintln!("clearcoat: {a:.1} -> {b:.1}");
    assert!(b > a + 5.0, "clearcoat {a} -> {b}");

    // Glass shows what is behind it (yellow) in its middle, where an
    // opaque mirror shows what is in front (blue).
    let mut glass = physical([1.0; 3], 0.0, 0.02);
    MaterialPreset::Glass.apply(&mut glass);
    let img = r.render_image(&scene(map.clone(), glass), &ctx, &target);
    img.save(dir.join("glass.png")).unwrap();
    let mid = px(&img, 0.5, 0.5);
    eprintln!("glass middle {mid:?}");
    assert!(
        mid[0] > 100.0 && mid[1] > 100.0 && mid[2] < mid[0] * 0.6,
        "glass middle {mid:?}"
    );

    // Sheen brightens the edges of dark cloth.
    let cloth = physical([0.05; 3], 0.0, 0.9);
    let mut velvet = cloth.clone();
    velvet.pbr.sheen = Param::new(1.0);
    let uni = EnvSource::Hdri(uniform.clone());
    let edge = |img: &image::RgbaImage| px(img, 0.5 + 0.19, 0.5)[0];
    let a = edge(&r.render_image(&scene(uni.clone(), cloth), &ctx, &target));
    let b = edge(&r.render_image(&scene(uni.clone(), velvet), &ctx, &target));
    eprintln!("sheen edge: {a:.1} -> {b:.1}");
    assert!(b > a + 10.0, "sheen edge {a} -> {b}");

    // The ORM map multiplies the material's values: one with no
    // roughness turns a rough metal ball into a mirror (blue in the
    // middle).
    // (Blue only in a small spot straight ahead: a rough ball averages it
    // away.)
    let spot = EnvSource::Hdri(write_hdr("spot.hdr", &|d| {
        if d.z > 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 1.0, 0.0]
        }
    }));
    let mut rough = physical([1.0; 3], 1.0, 1.0);
    let before = px(
        &r.render_image(&scene(spot.clone(), rough.clone()), &ctx, &target),
        0.5,
        0.5,
    );
    rough.pbr.orm_map = Some(write_png("orm.png", [255, 0, 255]));
    let after = px(
        &r.render_image(&scene(spot.clone(), rough), &ctx, &target),
        0.5,
        0.5,
    );
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    eprintln!("orm: middle {before:?} -> {after:?}");
    assert!(before[0] > 150.0, "rough middle {before:?}");
    assert!(after[2] > 200.0 && after[0] < 120.0, "orm middle {after:?}");

    // A glow map: black keeps the glow off, white lets it through.
    let mut glow = physical([0.0; 3], 0.0, 1.0);
    glow.emissive = Param::new(1.0);
    glow.emissive_color = [1.0, 0.0, 0.0];
    let dark = EnvSource::Hdri(write_hdr("black.hdr", &|_| [0.0; 3]));
    glow.pbr.emissive_map = Some(write_png("glow_off.png", [0, 0, 0]));
    let off = px(
        &r.render_image(&scene(dark.clone(), glow.clone()), &ctx, &target),
        0.5,
        0.5,
    );
    glow.pbr.emissive_map = Some(write_png("glow_on.png", [255, 255, 255]));
    let on = px(
        &r.render_image(&scene(dark.clone(), glow), &ctx, &target),
        0.5,
        0.5,
    );
    eprintln!("glow map: {off:?} -> {on:?}");
    assert!(off[0] < 20.0 && on[0] > 150.0, "glow {off:?} -> {on:?}");

    // A logo's environment sphere reflects the map: flat letters face the
    // camera and mirror what is behind it (blue); turned half a turn,
    // yellow.
    let logo = |turn: f32| {
        let mut p = scene(map.clone(), physical([1.0; 3], 0.0, 1.0));
        p.layers.pop();
        p.environment.env_light.rotation = Param::new(turn);
        let with = p.clone();
        p.layers.push(Layer::new(
            "Logo",
            LayerKind::Logo(LogoLayer {
                text: "EZ".into(),
                size: Param::new(0.6),
                matcap: Some("matcap_environment".into()),
                ..Default::default()
            }),
        ));
        (with, p)
    };
    for (turn, blue) in [(0.0, true), (180.0, false)] {
        let (bare, lit) = logo(turn);
        let a = r.render_image(&bare, &ctx, &target);
        let b = r.render_image(&lit, &ctx, &target);
        b.save(dir.join(format!("logo_env_{turn}.png"))).unwrap();
        // The mean colour of the letters (where the logo changed the picture).
        let (mut sum, mut n) = ([0.0f32; 3], 0.0);
        for (pa, pb) in a.pixels().zip(b.pixels()) {
            let d: i32 = (0..3).map(|c| (pa[c] as i32 - pb[c] as i32).abs()).sum();
            if d > 200 {
                for c in 0..3 {
                    sum[c] += pb[c] as f32;
                }
                n += 1.0;
            }
        }
        assert!(n > 50.0, "no letters");
        let m = sum.map(|v| v / n);
        eprintln!("logo turned {turn}: letters {m:?}");
        if blue {
            assert!(m[2] > m[0] + 60.0, "letters {m:?}");
        } else {
            assert!(m[0] > m[2] + 60.0, "letters {m:?}");
        }
    }

    // Without a map (sky and ground colours) physical shading still
    // lights the ball.
    let img = r.render_image(
        &scene(EnvSource::Colours, physical([0.8; 3], 0.0, 0.5)),
        &ctx,
        &target,
    );
    assert!(ball_mean(&img) > 20.0);
}

/// Screen-space reflections: a chrome ball next to a red box shows red on
/// the side facing it, which goes when the box is hidden; turned off, the
/// picture is exactly as without them; the loop still closes.
#[test]
fn screen_space_reflections_show_neighbours() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let dir = snapshot_dir().join("ssr");
    std::fs::create_dir_all(&dir).unwrap();
    let scene = |ssr: bool, with_box: bool| {
        let mut p = Project {
            camera: Camera {
                mode: CameraMode::Static,
                target: [0.0; 3],
                distance: Param::new(4.0),
                height: Param::new(0.0),
                angle: Param::new(0.0),
                fov: Param::new(40.0),
                ..Default::default()
            },
            ..Default::default()
        };
        p.environment.fog_density = Param::new(0.0);
        p.post.grade.vignette = Param::new(0.0);
        p.post.bloom.enabled = false;
        // A plain grey sky, so red in the ball can only come from the box.
        p.environment.sky_color = [0.3, 0.3, 0.3];
        p.environment.ground_color = [0.3, 0.3, 0.3];
        p.environment.reflections.enabled = ssr;
        p.layers.push(Layer::new(
            "Ball",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Sphere { detail: 4 }),
                material: Material {
                    base_color: [0.95; 3],
                    metallic: Param::new(1.0),
                    roughness: Param::new(0.03),
                    rim: Param::new(0.0),
                    pbr: Pbr {
                        shading: Shading::Physical,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            }),
        ));
        let mut red = Layer::new(
            "Box",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Cube),
                material: Material {
                    base_color: [0.9, 0.05, 0.05],
                    emissive_color: [1.0, 0.0, 0.0],
                    emissive: Param::new(1.0),
                    metallic: Param::new(0.0),
                    roughness: Param::new(0.9),
                    rim: Param::new(0.0),
                    ..Default::default()
                },
                ..Default::default()
            }),
        )
        // To the right of the ball and a little towards the camera.
        .at([2.0, 0.0, 0.6])
        .scaled(0.8);
        red.enabled = with_box;
        p.layers.push(red);
        p
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let (w, h) = (320u32, 180u32);
    let target = r.create_target(w, h);
    let ctx = EvalCtx::at(0.0);
    // The right side of the ball, facing the box.
    let side = |img: &image::RgbaImage| {
        let (mut red, mut n) = (0.0, 0.0);
        for (x, y, p) in img.enumerate_pixels() {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            // The ball's right half (it reaches about 0.7 of the width).
            if fx > 0.5 && fx < 0.68 && (fy - 0.5).abs() < 0.15 {
                red += p[0] as f32 - (p[1] as f32 + p[2] as f32) * 0.5;
                n += 1.0;
            }
        }
        red / n
    };
    let off = r.render_image(&scene(false, true), &ctx, &target);
    let on = r.render_image(&scene(true, true), &ctx, &target);
    let gone = r.render_image(&scene(true, false), &ctx, &target);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    on.save(dir.join("on.png")).unwrap();
    off.save(dir.join("off.png")).unwrap();
    gone.save(dir.join("no_box.png")).unwrap();
    let (a, b, c) = (side(&off), side(&on), side(&gone));
    eprintln!("red on the ball's side: off {a:.1}, on {b:.1}, without the box {c:.1}");
    assert!(b > a + 8.0, "no reflection of the box: {a} -> {b}");
    assert!(c.abs() < 2.0, "red without the box: {c}");

    // Off: identical to a project that never had the setting (the
    // default), including with depth of field sharing the pass.
    let mut dof = scene(false, true);
    dof.post.dof.enabled = true;
    let mut dof_on = dof.clone();
    dof_on.environment.reflections = Reflections {
        enabled: false,
        strength: Param::new(0.7),
        ..Default::default()
    };
    let x = r.render_image(&dof, &ctx, &target);
    let y = r.render_image(&dof_on, &ctx, &target);
    assert_eq!(x.as_raw(), y.as_raw());

    // A moving camera: the last frame is the first.
    let mut moving = scene(true, true);
    moving.camera.mode = CameraMode::Orbit;
    let first = r.render_image(&moving, &EvalCtx::at(0.0), &target);
    let last = r.render_image(&moving, &EvalCtx::at(1.0), &target);
    assert!(mean_abs_diff(first.as_raw(), last.as_raw()) < 0.05);
}

/// Light shafts: the sun lights the fog only where it reaches it (a roof
/// over the left half keeps the fog there dark); no fog, no shafts; off
/// changes nothing; the loop closes.
#[test]
fn light_shafts_follow_the_sun_shadows() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let dir = snapshot_dir().join("shafts");
    std::fs::create_dir_all(&dir).unwrap();
    let scene = |shafts: bool, fog: f32| {
        let mut p = Project {
            camera: Camera {
                mode: CameraMode::Static,
                target: [0.0, 2.0, -10.0],
                distance: Param::new(10.0),
                height: Param::new(0.0),
                angle: Param::new(0.0),
                fov: Param::new(60.0),
                ..Default::default()
            },
            ..Default::default()
        };
        let e = &mut p.environment;
        e.fog_density = Param::new(fog);
        e.fog_color = [0.05, 0.05, 0.06];
        e.light_dir = [0.05, 1.0, 0.02];
        e.light_color = [1.0, 0.95, 0.85];
        e.shadows = Shadows {
            enabled: true,
            distance: 30.0,
            ..Default::default()
        };
        e.shafts = LightShafts {
            enabled: shafts,
            strength: Param::new(1.0),
            scattering: 0.0,
            // Within the roof and the shadow map.
            reach: 20.0,
            ..Default::default()
        };
        p.post.grade.vignette = Param::new(0.0);
        p.post.bloom.enabled = false;
        // A roof over the left half of the view.
        p.layers.push(
            Layer::new(
                "Roof",
                LayerKind::Mesh(MeshLayer {
                    source: MeshSource::Primitive(Primitive::Cube),
                    material: Material {
                        base_color: [0.1; 3],
                        rim: Param::new(0.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([-10.0, 6.0, -15.0])
            .stretched([20.0, 0.3, 40.0]),
        );
        p
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let (w, h) = (320u32, 180u32);
    let target = r.create_target(w, h);
    let ctx = EvalCtx::at(0.0);
    // Mean brightness of a band below the roof's edge on each side.
    let halves = |img: &image::RgbaImage| {
        let (mut l, mut rr, mut nl, mut nr) = (0.0, 0.0, 0.0, 0.0);
        for (x, y, p) in img.enumerate_pixels() {
            let fy = y as f32 / h as f32;
            if !(0.55..0.8).contains(&fy) {
                continue;
            }
            let v = (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0;
            let fx = x as f32 / w as f32;
            if fx < 0.35 {
                l += v;
                nl += 1.0;
            } else if fx > 0.65 {
                rr += v;
                nr += 1.0;
            }
        }
        (l / nl, rr / nr)
    };
    let off = r.render_image(&scene(false, 0.05), &ctx, &target);
    let on = r.render_image(&scene(true, 0.05), &ctx, &target);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    off.save(dir.join("off.png")).unwrap();
    on.save(dir.join("on.png")).unwrap();
    let ((l0, r0), (l1, r1)) = (halves(&off), halves(&on));
    eprintln!("shafts: left {l0:.1} -> {l1:.1}, right {r0:.1} -> {r1:.1}");
    assert!(r1 - r0 > 15.0, "no light in the lit fog: {r0} -> {r1}");
    assert!(
        (l1 - l0) < (r1 - r0) * 0.4,
        "the roof's shadow doesn't cut the shafts: left +{}, right +{}",
        l1 - l0,
        r1 - r0
    );

    // No fog: nothing to light, exactly as without shafts.
    let a = r.render_image(&scene(false, 0.0), &ctx, &target);
    let b = r.render_image(&scene(true, 0.0), &ctx, &target);
    assert_eq!(a.as_raw(), b.as_raw());

    // A moving camera: the last frame is the first.
    let mut moving = scene(true, 0.05);
    moving.camera.mode = CameraMode::Orbit;
    let first = r.render_image(&moving, &EvalCtx::at(0.0), &target);
    let last = r.render_image(&moving, &EvalCtx::at(1.0), &target);
    assert!(mean_abs_diff(first.as_raw(), last.as_raw()) < 0.05);
}

/// A simulated liquid: drawn once baked, as droplets and as a surface in
/// its material, hidden behind what is in front of it, and the loop closes
/// (cross-fade halves).
#[test]
fn liquid_draws_and_loops() {
    use ez_core::sim::{Container, Fluid};
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let dir = snapshot_dir().join("liquid");
    std::fs::create_dir_all(&dir).unwrap();
    let scene = |surface: bool, wall: bool| {
        let mut p = Project {
            camera: Camera {
                mode: CameraMode::Static,
                target: [0.0, -0.3, 0.0],
                distance: Param::new(4.0),
                height: Param::new(2.0),
                angle: Param::new(0.0),
                fov: Param::new(40.0),
                ..Default::default()
            },
            timing: Timing {
                bpm: 120.0,
                loop_beats: 8,
            },
            ..Default::default()
        };
        p.environment.fog_density = Param::new(0.0);
        p.post.grade.vignette = Param::new(0.0);
        p.post.bloom.enabled = false;
        let mut liquid = Layer::new(
            "Liquid",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Sphere { detail: 1 }),
                material: Material {
                    base_color: [1.0, 0.1, 0.05],
                    emissive_color: [1.0, 0.1, 0.05],
                    emissive: Param::new(0.6),
                    rim: Param::new(0.0),
                    ..Default::default()
                },
                instancer: Instancer::Fluid {
                    fluid: Box::new(Fluid {
                        count: 600,
                        container: Container::Box,
                        size: 1.0,
                        spacing: 0.12,
                        surface,
                        ..Default::default()
                    }),
                    placed: None,
                },
                ..Default::default()
            }),
        )
        .scaled(0.08);
        liquid.transform.tilt = Param::new(0.0).osc(Wave::Sine, 15.0, 1);
        p.layers.push(liquid);
        if wall {
            // A dark wall in front of everything.
            p.layers.push(
                Layer::new(
                    "Wall",
                    LayerKind::Mesh(MeshLayer {
                        source: MeshSource::Primitive(Primitive::Cube),
                        material: Material {
                            base_color: [0.0; 3],
                            metallic: Param::new(0.0),
                            roughness: Param::new(1.0),
                            rim: Param::new(0.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                )
                .at([0.0, 0.5, 2.2])
                .stretched([6.0, 6.0, 0.1]),
            );
        }
        p
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    r.set_wait_for_bakes(true);
    let (w, h) = (240u32, 160u32);
    let target = r.create_target(w, h);
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    // How red the middle of the picture is.
    let red = |img: &image::RgbaImage| {
        let (mut sum, mut n) = (0.0, 0.0);
        for (x, y, p) in img.enumerate_pixels() {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            if (fx - 0.5).abs() < 0.25 && (fy - 0.55).abs() < 0.25 {
                sum += p[0] as f32 - (p[1] as f32 + p[2] as f32) * 0.5;
                n += 1.0;
            }
        }
        sum / n
    };
    for surface in [false, true] {
        let p = scene(surface, false);
        let img = r.render_image(&p, &at(&p, 0.2), &target);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert!(!r.take_inexact(), "drawn from an old bake");
        img.save(dir.join(format!("liquid_{surface}.png"))).unwrap();
        let shown = red(&img);
        eprintln!("liquid (surface {surface}): red {shown:.1}");
        assert!(shown > 15.0, "no liquid (surface {surface}): {shown}");
        // Behind a wall, nothing shows.
        let hidden = r.render_image(&scene(surface, true), &at(&p, 0.2), &target);
        let behind = red(&hidden);
        eprintln!("behind the wall: red {behind:.1}");
        assert!(
            behind < 2.0,
            "seen through the wall (surface {surface}): {behind}"
        );
        // The loop closes.
        let a = r.render_image(&p, &at(&p, 0.0), &target);
        let b = r.render_image(&p, &at(&p, 1.0), &target);
        assert!(mean_abs_diff(a.as_raw(), b.as_raw()) < 0.05);
    }
}

/// Retro 3D: the scene drawn at a low resolution aliases natively (square
/// pixels), snapping and texture warp change the picture, subdividing
/// shrinks the warp, every texture filter looks different, and it all
/// loops.
#[test]
fn retro_3d_is_chunky_wobbly_and_loops() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let dir = snapshot_dir().join("retro3d");
    std::fs::create_dir_all(&dir).unwrap();
    let scene = |retro: Retro3d, subdivide: u32, filter: TexFilter| {
        let mut p = presets::empty();
        p.retro = retro;
        p.camera = Camera {
            target: [0.0, 0.0, 0.0],
            distance: Param::new(3.2),
            height: Param::new(1.2),
            orbit_turns: 1,
            swing: Param::new(0.0),
            fov: Param::new(50.0),
            ..Default::default()
        };
        p.environment.fog_density = Param::new(0.0);
        p.post.bloom.enabled = false;
        p.post.grade.vignette = Param::new(0.0);
        p.post.grade.grain = Param::new(0.0);
        // Only the sky of the empty scene.
        p.layers.truncate(1);
        let mut floor = Layer::new(
            "Floor",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Plane),
                subdivide,
                material: Material {
                    base_color: [1.0; 3],
                    texture: Some("checker".into()),
                    texture_scale: Param::new(3.0),
                    filter,
                    rim: Param::new(0.0),
                    ..Default::default()
                },
                ..Default::default()
            }),
        )
        .at([0.0, -0.8, 0.0])
        .scaled(6.0);
        floor.transform.spin = [0, 1, 0];
        p.layers.push(floor);
        p.layers.push(
            Layer::new(
                "Box",
                LayerKind::Mesh(MeshLayer {
                    source: MeshSource::Primitive(Primitive::Cube),
                    subdivide,
                    material: Material {
                        base_color: [1.0, 0.8, 0.6],
                        texture: Some("brick".into()),
                        filter,
                        rim: Param::new(0.0),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .spin([1, 2, 0]),
        );
        p
    };
    let ps1 = || {
        let mut r = Retro3d::default();
        r.apply_style(RetroStyle::Ps1);
        // Dithering breaks up flat areas on purpose (tested on its own).
        r.color_15bit = false;
        r
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let (w, h) = (640u32, 360u32);
    let target = r.create_target(w, h);
    let at = |p: &Project, phase: f32| EvalCtx::new(&p.timing, phase, None);
    // Share of neighbouring pixels (across) that are exactly the same.
    let same = |img: &image::RgbaImage| {
        let mut n = 0u32;
        for y in 0..h {
            for x in 0..w - 1 {
                n += (img.get_pixel(x, y) == img.get_pixel(x + 1, y)) as u32;
            }
        }
        n as f32 / (h * (w - 1)) as f32
    };

    let off = scene(Retro3d::default(), 0, TexFilter::Smooth);
    let on = scene(ps1(), 0, TexFilter::Nearest);
    let a = r.render_image(&off, &at(&off, 0.3), &target);
    let b = r.render_image(&on, &at(&on, 0.3), &target);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    a.save(dir.join("off.png")).unwrap();
    b.save(dir.join("ps1.png")).unwrap();
    let (sa, sb) = (same(&a), same(&b));
    eprintln!("equal neighbours: off {sa:.3}, PS1 {sb:.3}");
    // 320 × 240 on a 16:9 output is 427 × 240: every low pixel covers
    // about 1.5 × 1.5 output pixels.
    assert!(sb > sa + 0.06, "not chunky: {sa} -> {sb}");

    // Loops: the first and last frames match.
    for p in [&on, &scene(ps1(), 2, TexFilter::ThreePoint)] {
        let first = r.render_image(p, &at(p, 0.0), &target);
        let last = r.render_image(p, &at(p, 1.0), &target);
        let mid = r.render_image(p, &at(p, 0.37), &target);
        let seam = mean_abs_diff(first.as_raw(), last.as_raw());
        let motion = mean_abs_diff(first.as_raw(), mid.as_raw());
        eprintln!("retro seam {seam:.3}, motion {motion:.2}");
        assert!(seam < 0.6, "retro 3D does not loop: {seam}");
        assert!(motion > 2.0, "retro 3D scene does not move: {motion}");
    }

    // Snapping and warp each change the full-resolution picture.
    let full = |snap: bool, affine: f32| Retro3d {
        enabled: true,
        snap,
        affine: Param::new(affine),
        ..Default::default()
    };
    let plain = r.render_image(
        &scene(full(false, 0.0), 0, TexFilter::Smooth),
        &at(&off, 0.3),
        &target,
    );
    let d_off = mean_abs_diff(plain.as_raw(), a.as_raw());
    assert!(
        d_off < 0.01,
        "Retro 3D on with nothing set changed the picture: {d_off}"
    );
    let snapped = r.render_image(
        &scene(full(true, 0.0), 0, TexFilter::Smooth),
        &at(&off, 0.3),
        &target,
    );
    let d_snap = mean_abs_diff(plain.as_raw(), snapped.as_raw());
    let warped = r.render_image(
        &scene(full(false, 1.0), 0, TexFilter::Smooth),
        &at(&off, 0.3),
        &target,
    );
    let d_warp = mean_abs_diff(plain.as_raw(), warped.as_raw());
    let plain_sub = r.render_image(
        &scene(full(false, 0.0), 3, TexFilter::Smooth),
        &at(&off, 0.3),
        &target,
    );
    let warped_sub = r.render_image(
        &scene(full(false, 1.0), 3, TexFilter::Smooth),
        &at(&off, 0.3),
        &target,
    );
    let d_warp_sub = mean_abs_diff(plain_sub.as_raw(), warped_sub.as_raw());
    snapped.save(dir.join("snapped.png")).unwrap();
    warped.save(dir.join("warped.png")).unwrap();
    warped_sub.save(dir.join("warped_subdivided.png")).unwrap();
    eprintln!("snap {d_snap:.2}, warp {d_warp:.2}, warp after subdividing {d_warp_sub:.2}");
    assert!(d_snap > 0.3, "snapping shows no change: {d_snap}");
    assert!(d_warp > 1.0, "texture warp shows no change: {d_warp}");
    assert!(
        d_warp_sub < d_warp * 0.6,
        "subdividing doesn't reduce the warp: {d_warp} -> {d_warp_sub}"
    );

    // Every texture filter is its own look.
    let shots: Vec<(TexFilter, image::RgbaImage)> = TexFilter::ALL
        .iter()
        .map(|&f| {
            let mut p = scene(Retro3d::default(), 0, f);
            // A pattern that changes every texel, magnified near the
            // camera and shrunk in the distance.
            if let LayerKind::Mesh(m) = &mut p.layers[1].kind {
                m.material.texture = Some("dither".into());
                m.material.texture_scale = Param::new(0.4);
            }
            (f, r.render_image(&p, &at(&p, 0.3), &target))
        })
        .collect();
    for (i, (fa, ia)) in shots.iter().enumerate() {
        ia.save(dir.join(format!("filter_{}.png", fa.index())))
            .unwrap();
        for (fb, ib) in &shots[i + 1..] {
            let d = mean_abs_diff(ia.as_raw(), ib.as_raw());
            eprintln!("{:?} vs {:?}: {d:.3}", fa, fb);
            assert!(d > 0.05, "{fa:?} and {fb:?} look the same: {d}");
        }
    }

    // Text drawn sharp on top of the chunky scene, or as chunky.
    let mut text = on.clone();
    text.layers
        .push(Layer::new("Title", LayerKind::Text(TextLayer::default())));
    let sharp = r.render_image(&text, &at(&text, 0.3), &target);
    text.retro.sharp_overlays = false;
    let chunky = r.render_image(&text, &at(&text, 0.3), &target);
    sharp.save(dir.join("text_sharp.png")).unwrap();
    chunky.save(dir.join("text_chunky.png")).unwrap();
    let d_text = mean_abs_diff(sharp.as_raw(), chunky.as_raw());
    eprintln!("sharp vs chunky text: {d_text:.3}");
    assert!(d_text > 0.05, "sharp text setting does nothing: {d_text}");
}

/// Retro 3D console quirks: N64 fog, 15-bit colour and dither, the N64
/// video blur, Saturn mesh transparency (shapes and sprites) and
/// near-plane culling each do what they say, and the N64 look loops.
#[test]
fn retro_console_quirks_show_and_loop() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let dir = snapshot_dir().join("retro_quirks");
    std::fs::create_dir_all(&dir).unwrap();
    // A box `dist` away from a still camera, over the fog colour (no
    // background).
    let scene = |dist: f32, retro: Retro3d, material: Material| {
        let mut p = presets::empty();
        p.layers.clear();
        p.retro = retro;
        p.camera = Camera {
            mode: CameraMode::Static,
            target: [0.0; 3],
            distance: Param::new(dist),
            height: Param::new(0.0),
            angle: Param::new(30.0),
            fov: Param::new(40.0),
            ..Default::default()
        };
        p.environment.fog_color = [0.02, 0.02, 0.02];
        p.environment.fog_density = Param::new(0.0);
        p.post.bloom.enabled = false;
        p.post.grade.vignette = Param::new(0.0);
        p.post.grade.grain = Param::new(0.0);
        p.layers.push(
            Layer::new(
                "Box",
                LayerKind::Mesh(MeshLayer {
                    source: MeshSource::Primitive(Primitive::Sphere { detail: 4 }),
                    material,
                    ..Default::default()
                }),
            )
            .scaled(1.2),
        );
        p
    };
    let lit = || Material {
        base_color: [0.9, 0.7, 0.5],
        metallic: Param::new(0.0),
        roughness: Param::new(0.6),
        rim: Param::new(0.0),
        ..Default::default()
    };
    let on = |f: &dyn Fn(&mut Retro3d)| {
        let mut r = Retro3d {
            enabled: true,
            ..Default::default()
        };
        f(&mut r);
        r
    };
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let (w, h) = (320u32, 180u32);
    let target = r.create_target(w, h);
    let ctx = EvalCtx::at(0.0);
    let empty = {
        let mut p = scene(6.0, Retro3d::default(), lit());
        p.layers.clear();
        r.render_image(&p, &ctx, &target)
    };
    // Share of pixels that differ from the empty background.
    let cover = |img: &image::RgbaImage| {
        img.pixels()
            .zip(empty.pixels())
            .filter(|(p, e)| (0..3).any(|c| (p[c] as i32 - e[c] as i32).abs() > 12))
            .count() as f32
            / (w * h) as f32
    };

    // N64 fog: solid at 6, so a ball 12 away is gone; 3 away it shows.
    let fog = |near: f32, far: f32| {
        on(&|r: &mut Retro3d| {
            r.fog.enabled = true;
            r.fog.near = Param::new(near);
            r.fog.far = Param::new(far);
        })
    };
    let plain = r.render_image(&scene(12.0, Retro3d::default(), lit()), &ctx, &target);
    let fogged = r.render_image(&scene(12.0, fog(1.0, 6.0), lit()), &ctx, &target);
    let clear = r.render_image(&scene(12.0, fog(20.0, 40.0), lit()), &ctx, &target);
    plain.save(dir.join("fog_off.png")).unwrap();
    fogged.save(dir.join("fog_n64.png")).unwrap();
    let (c_plain, c_fog, c_clear) = (cover(&plain), cover(&fogged), cover(&clear));
    eprintln!("ball coverage: no fog {c_plain:.3}, N64 fog {c_fog:.3}, fog starting behind it {c_clear:.3}");
    assert!(c_plain > 0.01, "ball not drawn: {c_plain}");
    assert!(c_fog < 0.002, "N64 fog doesn't hide the far ball: {c_fog}");
    assert!(
        (c_clear - c_plain).abs() < 0.002,
        "fog starting behind the ball touches it"
    );

    // 15-bit colour: fewer colours; dither changes the picture; the video
    // blur smooths the dither away.
    let unique = |img: &image::RgbaImage| {
        img.pixels()
            .map(|p| (p[0], p[1], p[2]))
            .collect::<std::collections::HashSet<_>>()
            .len()
    };
    let rough = |img: &image::RgbaImage| {
        let mut s = 0.0;
        for y in 0..h {
            for x in 0..w - 1 {
                let (a, b) = (img.get_pixel(x, y), img.get_pixel(x + 1, y));
                s += (0..3)
                    .map(|c| (a[c] as f32 - b[c] as f32).abs())
                    .sum::<f32>();
            }
        }
        s / (w * h) as f32
    };
    let full = r.render_image(&scene(3.0, Retro3d::default(), lit()), &ctx, &target);
    let banded = r.render_image(
        &scene(
            3.0,
            on(&|r: &mut Retro3d| {
                r.color_15bit = true;
                r.dither = Param::new(0.0);
            }),
            lit(),
        ),
        &ctx,
        &target,
    );
    let dithered = r.render_image(
        &scene(3.0, on(&|r: &mut Retro3d| r.color_15bit = true), lit()),
        &ctx,
        &target,
    );
    let vi = r.render_image(
        &scene(
            3.0,
            on(&|r: &mut Retro3d| {
                r.color_15bit = true;
                r.vi_blur = Param::new(1.0);
            }),
            lit(),
        ),
        &ctx,
        &target,
    );
    banded.save(dir.join("15bit.png")).unwrap();
    dithered.save(dir.join("15bit_dither.png")).unwrap();
    vi.save(dir.join("15bit_dither_vi.png")).unwrap();
    let (u_full, u_band) = (unique(&full), unique(&banded));
    let (r_dith, r_vi) = (rough(&dithered), rough(&vi));
    let d_dith = mean_abs_diff(banded.as_raw(), dithered.as_raw());
    eprintln!(
        "colours: full {u_full}, 15-bit {u_band}; dither changes {d_dith:.3}; \
         roughness dithered {r_dith:.2}, with video blur {r_vi:.2}"
    );
    assert!(
        (u_band as f32) < u_full as f32 * 0.6,
        "15-bit colour doesn't band"
    );
    assert!(d_dith > 0.2, "dither shows no change: {d_dith}");
    assert!(r_vi < r_dith * 0.8, "video blur doesn't smooth the dither");

    // Saturn mesh: half of the ball's pixels left out.
    let solid = cover(&full);
    let meshed_img = r.render_image(
        &scene(
            3.0,
            Retro3d::default(),
            Material {
                mesh: Param::new(0.5),
                ..lit()
            },
        ),
        &ctx,
        &target,
    );
    meshed_img.save(dir.join("mesh.png")).unwrap();
    let meshed = cover(&meshed_img);
    eprintln!("ball coverage: solid {solid:.3}, mesh {meshed:.3}");
    assert!(
        (meshed / solid - 0.5).abs() < 0.08,
        "mesh doesn't leave half out"
    );
    let mut sprite = |blend: SpriteBlend| {
        let mut p = scene(3.0, Retro3d::default(), lit());
        p.layers = vec![Layer::new(
            "Dot",
            LayerKind::Sprite(SpriteLayer {
                blend,
                size: Param::new(1.5),
                ..Default::default()
            }),
        )];
        cover(&r.render_image(&p, &ctx, &target))
    };
    let (s_cut, s_mesh) = (sprite(SpriteBlend::Cutout), sprite(SpriteBlend::Mesh));
    eprintln!("sprite coverage: cutout {s_cut:.3}, mesh {s_mesh:.3}");
    assert!(
        s_cut > 0.01 && (s_mesh / s_cut - 0.5).abs() < 0.1,
        "sprite mesh doesn't leave half out"
    );

    // Near-plane culling: the ball 3 away vanishes at a distance of 2.5
    // (its near side is 1.8 away), not at 1.
    // Shapes are two-sided, so the inside shows through the hole.
    let mut culled = |d: f32| {
        r.render_image(
            &scene(
                3.0,
                on(&|r: &mut Retro3d| r.near_cull = Param::new(d)),
                lit(),
            ),
            &ctx,
            &target,
        )
    };
    let (keep, gone) = (culled(1.0), culled(2.5));
    gone.save(dir.join("near_cull.png")).unwrap();
    let (d_keep, d_gone) = (
        mean_abs_diff(keep.as_raw(), full.as_raw()),
        mean_abs_diff(gone.as_raw(), full.as_raw()),
    );
    eprintln!("near culling: change at 1 {d_keep:.3}, at 2.5 {d_gone:.3}");
    assert!(d_keep < 0.01, "culling at 1 changed the ball");
    assert!(d_gone > 3.0, "near triangles weren't culled");

    // The N64 look loops (an orbiting camera, a spinning textured ball).
    let mut p = scene(4.0, Retro3d::default(), lit());
    p.retro.apply_style(RetroStyle::N64);
    p.camera.mode = CameraMode::Orbit;
    p.camera.swing = Param::new(0.0);
    p.layers[0].transform.spin = [0, 1, 1];
    if let LayerKind::Mesh(m) = &mut p.layers[0].kind {
        m.material.texture = Some("brick".into());
    }
    let at = |ph: f32| EvalCtx::new(&p.timing, ph, None);
    let a = r.render_image(&p, &at(0.0), &target);
    let b = r.render_image(&p, &at(1.0), &target);
    let m = r.render_image(&p, &at(0.4), &target);
    a.save(dir.join("n64.png")).unwrap();
    let seam = mean_abs_diff(a.as_raw(), b.as_raw());
    let motion = mean_abs_diff(a.as_raw(), m.as_raw());
    eprintln!("N64 look: seam {seam:.3}, motion {motion:.2}");
    assert!(seam < 0.6, "N64 look doesn't loop: {seam}");
    assert!(motion > 1.0, "N64 scene doesn't move");
}

/// Quake features: light styles (sun, glow, sprites, particles),
/// turbulent warp (materials, terrain liquids, waterfalls), the two-layer
/// sky, palette-space lighting and solid square particles all show and
/// loop.
#[test]
fn quake_features_show_and_loop() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let dir = snapshot_dir().join("quake");
    std::fs::create_dir_all(&dir).unwrap();
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let (w, h) = (240u32, 136u32);
    let target = r.create_target(w, h);
    let base = || {
        let mut p = presets::empty();
        p.layers.clear();
        p.camera = Camera {
            mode: CameraMode::Static,
            target: [0.0; 3],
            distance: Param::new(4.0),
            height: Param::new(1.0),
            angle: Param::new(20.0),
            fov: Param::new(50.0),
            ..Default::default()
        };
        p.environment.fog_density = Param::new(0.0);
        p.post.bloom.enabled = false;
        p.post.grade.vignette = Param::new(0.0);
        p.post.grade.grain = Param::new(0.0);
        p
    };
    let ball = |mat: Material| {
        Layer::new(
            "Ball",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Sphere { detail: 4 }),
                material: mat,
                ..Default::default()
            }),
        )
        .scaled(1.3)
    };
    let lum = |img: &image::RgbaImage| {
        img.as_raw().iter().map(|v| *v as f32).sum::<f32>() / img.as_raw().len() as f32
    };
    // "az": dark for the first half of the loop, twice as bright after.
    let az = LightStyle {
        pattern: "az".into(),
        plays: 1,
    };
    let mut failures = Vec::new();
    let mut check = |name: &str, p: &Project, r: &mut Renderer, want_change: bool| {
        let at = |ph: f32| EvalCtx::new(&p.timing, ph, None);
        let a = r.render_image(p, &at(0.1), &target);
        let b = r.render_image(p, &at(0.6), &target);
        let first = r.render_image(p, &at(0.0), &target);
        let last = r.render_image(p, &at(1.0), &target);
        // Not a whole fraction of the loop away from 0.1 either.
        let c = r.render_image(p, &at(0.23), &target);
        b.save(dir.join(format!("{name}.png"))).unwrap();
        let (la, lb) = (lum(&a), lum(&b));
        let change =
            mean_abs_diff(a.as_raw(), b.as_raw()).max(mean_abs_diff(a.as_raw(), c.as_raw()));
        let seam = mean_abs_diff(first.as_raw(), last.as_raw());
        eprintln!("{name:<22} brightness {la:.1} -> {lb:.1}, change {change:.2}, seam {seam:.3}");
        if seam > 0.6 {
            failures.push(format!("{name}: seam {seam}"));
        }
        if want_change && change < 0.3 {
            failures.push(format!("{name}: no change over the loop ({change})"));
        }
        (a, b)
    };

    // Light styles.
    let mut p = base();
    p.layers.push(ball(Material::default()));
    p.environment.light_style = az.clone();
    let (a, b) = check("sun style", &p, &mut r, true);
    assert!(
        lum(&b) > lum(&a) + 3.0,
        "the sun's light style doesn't brighten"
    );
    let mut p = base();
    p.layers.push(ball(Material {
        emissive: Param::new(1.0),
        glow_style: az.clone(),
        ..Default::default()
    }));
    let (a, b) = check("glow style", &p, &mut r, true);
    assert!(
        lum(&b) > lum(&a) + 3.0,
        "the glow's light style doesn't brighten"
    );
    let mut p = base();
    p.layers.push(Layer::new(
        "Dot",
        LayerKind::Sprite(SpriteLayer {
            size: Param::new(2.0),
            glow_style: az.clone(),
            ..Default::default()
        }),
    ));
    let (a, b) = check("sprite style", &p, &mut r, true);
    assert!(
        lum(&b) > lum(&a) + 1.0,
        "the sprite's light style doesn't brighten"
    );
    let particles = |sprite: Sprite, style: LightStyle| {
        let mut p = base();
        p.layers.push(Layer::new(
            "Sparks",
            LayerKind::Particles(ParticleLayer {
                emitter: Emitter::Fountain,
                count: 600,
                size: Param::new(0.08),
                radius: Param::new(1.5),
                sprite,
                glow_style: style,
                ..Default::default()
            }),
        ));
        p
    };
    let (a, b) = check(
        "particle style",
        &particles(Sprite::Glow, az.clone()),
        &mut r,
        true,
    );
    assert!(
        lum(&b) > lum(&a) + 0.2,
        "the particles' light style doesn't brighten"
    );
    check(
        "square particles",
        &particles(Sprite::SolidSquare, LightStyle::default()),
        &mut r,
        true,
    );

    // Turbulent warp on a textured ball (still: only the warp moves).
    let turb = |amount: f32| {
        let mut p = base();
        p.layers.push(ball(Material {
            texture: Some("brick".into()),
            texture_scale: Param::new(2.0),
            turbulence: Turbulence {
                amount: Param::new(amount),
                waves: 1.0,
                cycles: 3,
            },
            ..Default::default()
        }));
        p
    };
    check("turbulent material", &turb(0.15), &mut r, true);
    check("still material", &turb(0.0), &mut r, false);
    // Lava on terrain and a waterfall.
    let mut p = base();
    p.camera.distance = Param::new(9.0);
    p.camera.height = Param::new(4.0);
    p.layers.push(Layer::new(
        "Lava",
        LayerKind::Terrain(Terrain {
            size: 20.0,
            cells: 32,
            scroll: 0,
            style: TerrainStyle::Solid,
            liquid: Liquid {
                kind: LiquidKind::Lava,
                level: Param::new(0.6),
                turbulence: Param::new(0.3),
                turb_cycles: 2,
                flow: 0,
                ..Default::default()
            },
            ..Default::default()
        }),
    ));
    check("turbulent lava", &p, &mut r, true);
    // The warp itself, at one moment: with and without.
    let warp_diff = |p: &Project, off: &Project, r: &mut Renderer| {
        let a = r.render_image(p, &EvalCtx::at(0.3), &target);
        let b = r.render_image(off, &EvalCtx::at(0.3), &target);
        mean_abs_diff(a.as_raw(), b.as_raw())
    };
    let mut calm = p.clone();
    if let LayerKind::Terrain(t) = &mut calm.layers[0].kind {
        t.liquid.turbulence = Param::new(0.0);
    }
    let d_lava = warp_diff(&p, &calm, &mut r);
    let mut p = base();
    p.camera.distance = Param::new(12.0);
    p.layers.push(
        Layer::new(
            "Falls",
            LayerKind::Falls(Falls {
                turbulence: Param::new(0.2),
                foam: Param::new(0.0),
                ..Default::default()
            }),
        )
        .at([0.0, 4.0, 0.0]),
    );
    check("turbulent waterfall", &p, &mut r, true);
    let mut calm = p.clone();
    if let LayerKind::Falls(f) = &mut calm.layers[0].kind {
        f.turbulence = Param::new(0.0);
    }
    let d_falls = warp_diff(&p, &calm, &mut r);
    eprintln!("turbulence changes lava by {d_lava:.2}, the waterfall by {d_falls:.2}");
    assert!(
        d_lava > 1.0 && d_falls > 0.3,
        "liquid turbulence shows no change"
    );

    // The two-layer sky scrolls, and the near layer has holes.
    let sky = |cutout: [f32; 3]| {
        let mut p = base();
        p.camera.height = Param::new(3.0);
        p.camera.target = [0.0, 3.0, 0.0];
        p.layers.push(Layer::new(
            "Sky",
            LayerKind::Backdrop(Backdrop {
                kind: BackdropKind::LayeredSky,
                sky: LayeredSky {
                    cutout,
                    ..Default::default()
                },
                ..Default::default()
            }),
        ));
        p
    };
    let (a, _) = check("layered sky", &sky([0.0; 3]), &mut r, true);
    // A see-through colour the near layer doesn't have: no holes.
    let (solid, _) = check("sky without holes", &sky([1.0, 0.0, 1.0]), &mut r, true);
    let holes = mean_abs_diff(a.as_raw(), solid.as_raw());
    eprintln!("near layer holes change the sky by {holes:.2}");
    assert!(lum(&a) > 5.0, "the sky is black");
    assert!(
        holes > 1.0,
        "the far layer doesn't show through the near one"
    );

    // Palette-space lighting: far fewer colours, and it loops.
    let mut p = base();
    p.layers.push(ball(Material {
        texture: Some("marble".into()),
        ..Default::default()
    }));
    let plain = r.render_image(&p, &EvalCtx::at(0.3), &target);
    p.retro.enabled = true;
    p.retro.colormap.enabled = true;
    let (cm, _) = check("colormap", &p, &mut r, false);
    let unique = |img: &image::RgbaImage| {
        img.pixels()
            .map(|p| (p[0], p[1], p[2]))
            .collect::<std::collections::HashSet<_>>()
            .len()
    };
    let (u0, u1) = (unique(&plain), unique(&cm));
    eprintln!("colours: plain {u0}, colormap {u1}");
    assert!(
        u1 < u0 / 3 && u1 < 400,
        "colormap doesn't step through the palette: {u0} -> {u1}"
    );
    p.retro.colormap.palette = ColormapPalette::Retro(palette::PaletteId::C64);
    let c64 = r.render_image(&p, &EvalCtx::at(0.3), &target);
    c64.save(dir.join("colormap_c64.png")).unwrap();
    assert!(
        unique(&c64) < 60,
        "the C64 colormap has {} colours",
        unique(&c64)
    );

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Animating on steps holds a layer's motion between whole steps (the
/// camera stays smooth), and the Mode 7 floor draws to a hard horizon,
/// turns and scrolls, hides what is under it, and loops.
#[test]
fn stepped_motion_and_mode7_floor() {
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let dir = snapshot_dir().join("group4");
    std::fs::create_dir_all(&dir).unwrap();
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let (w, h) = (240u32, 136u32);
    let target = r.create_target(w, h);
    let base = |orbit: bool| {
        let mut p = presets::empty();
        p.layers.clear();
        p.camera = Camera {
            mode: if orbit {
                CameraMode::Orbit
            } else {
                CameraMode::Static
            },
            target: [0.0; 3],
            distance: Param::new(5.0),
            height: Param::new(2.0),
            swing: Param::new(0.0),
            fov: Param::new(50.0),
            ..Default::default()
        };
        p.environment.fog_density = Param::new(0.0);
        p.post.bloom.enabled = false;
        p.post.grade.vignette = Param::new(0.0);
        p.post.grade.grain = Param::new(0.0);
        p
    };
    let at = |p: &Project, ph: f32| EvalCtx::new(&p.timing, ph, None);

    // A spinning, bobbing cube on steps: 16 beats at 120 bpm is 8 s, so
    // 4 fps is 32 steps per loop. Two moments inside one step match.
    assert_eq!(step_count(4.0, 8.0), Some(32));
    assert_eq!(step_count(0.0, 8.0), None);
    let cube = |fps: f32| {
        let mut l = Layer::new("Cube", LayerKind::Mesh(MeshLayer::default())).spin([1, 2, 0]);
        l.transform.bob = Param::new(0.0).osc(Wave::Sine, 0.5, 2);
        l.step_fps = fps;
        l
    };
    let mut still = base(false);
    still.layers.push(cube(4.0));
    let (a, b) = (1.0 / 32.0 + 0.002, 2.0 / 32.0 - 0.002);
    let s1 = r.render_image(&still, &at(&still, a), &target);
    let s2 = r.render_image(&still, &at(&still, b), &target);
    let held = mean_abs_diff(s1.as_raw(), s2.as_raw());
    let mut smooth = base(false);
    smooth.layers.push(cube(0.0));
    let m1 = r.render_image(&smooth, &at(&smooth, a), &target);
    let m2 = r.render_image(&smooth, &at(&smooth, b), &target);
    let moved = mean_abs_diff(m1.as_raw(), m2.as_raw());
    let next = r.render_image(&still, &at(&still, b + 0.004), &target);
    let stepped = mean_abs_diff(s2.as_raw(), next.as_raw());
    eprintln!("within a step: stepped {held:.3}, smooth {moved:.3}; across a step {stepped:.3}");
    assert!(held < 0.01, "a stepped layer moves within a step");
    assert!(moved > 0.3 && stepped > 0.3, "no motion to step");
    // The camera keeps moving smoothly.
    let mut orbit = base(true);
    orbit.layers.push(cube(4.0));
    let o1 = r.render_image(&orbit, &at(&orbit, a), &target);
    let o2 = r.render_image(&orbit, &at(&orbit, b), &target);
    assert!(
        mean_abs_diff(o1.as_raw(), o2.as_raw()) > 0.3,
        "the camera stepped with the layer"
    );
    for p in [&still, &orbit] {
        let first = r.render_image(p, &at(p, 0.0), &target);
        let last = r.render_image(p, &at(p, 1.0), &target);
        let seam = mean_abs_diff(first.as_raw(), last.as_raw());
        assert!(seam < 0.6, "stepped motion doesn't loop: {seam}");
    }

    // Mode 7: ground below the horizon, the background above it.
    let floor = |height: f32| {
        let mut l = Layer::new(
            "Mode 7",
            LayerKind::Mode7(Mode7Floor {
                texture: Some("checker".into()),
                tile_size: 2.0,
                turns: 1,
                scroll: [0, 3],
                ..Default::default()
            }),
        );
        l.transform.position[1] = height;
        l
    };
    let mut p = base(false);
    // A level camera: the horizon across the middle.
    p.camera.height = Param::new(0.0);
    p.camera.target = [0.0, 1.5, 0.0];
    let empty = r.render_image(&p, &at(&p, 0.3), &target);
    p.layers.push(floor(0.0));
    let img = r.render_image(&p, &at(&p, 0.3), &target);
    img.save(dir.join("mode7.png")).unwrap();
    let rows_changed = |y0: u32, y1: u32| {
        let mut n = 0;
        let mut t = 0;
        for y in y0..y1 {
            for x in 0..w {
                t += 1;
                let (a, b) = (img.get_pixel(x, y), empty.get_pixel(x, y));
                n += (0..3).any(|c| (a[c] as i32 - b[c] as i32).abs() > 8) as u32;
            }
        }
        n as f32 / t as f32
    };
    let (top, bottom) = (rows_changed(0, h / 4), rows_changed(h * 3 / 4, h));
    eprintln!("Mode 7 covers {top:.3} of the top rows, {bottom:.3} of the bottom rows");
    assert!(top < 0.01, "Mode 7 above the horizon");
    // (Black squares can match the dark background.)
    assert!(bottom > 0.6, "Mode 7 floor missing below the horizon");
    let first = r.render_image(&p, &at(&p, 0.0), &target);
    let last = r.render_image(&p, &at(&p, 1.0), &target);
    let later = r.render_image(&p, &at(&p, 0.37), &target);
    let seam = mean_abs_diff(first.as_raw(), last.as_raw());
    let motion = mean_abs_diff(first.as_raw(), later.as_raw());
    eprintln!("Mode 7 seam {seam:.3}, motion {motion:.2}");
    assert!(
        seam < 0.6 && motion > 2.0,
        "Mode 7 doesn't turn, scroll and loop"
    );
    // A cube under the floor is hidden; above it, it shows.
    let with_cube = |y: f32, p: &Project, r: &mut Renderer| {
        let mut q = p.clone();
        q.layers.push(
            Layer::new("Cube", LayerKind::Mesh(MeshLayer::default()))
                .at([0.0, y, 0.0])
                .scaled(0.6),
        );
        r.render_image(&q, &at(&q, 0.3), &target)
    };
    let under = with_cube(-1.0, &p, &mut r);
    let over = with_cube(1.0, &p, &mut r);
    let (d_under, d_over) = (
        mean_abs_diff(under.as_raw(), img.as_raw()),
        mean_abs_diff(over.as_raw(), img.as_raw()),
    );
    eprintln!("cube under the floor changes {d_under:.3}, above it {d_over:.3}");
    assert!(d_under < 0.05, "the floor doesn't hide what is under it");
    assert!(d_over > 0.1, "a cube above the floor does not show");
}

/// The whole screen at an old machine's resolution: the border is the
/// border colour, inside only the palette's colours in machine-sized
/// (wide) pixels, colour cubes keep their levels, and it loops.
#[test]
fn console_screens_and_palettes() {
    use ez_core::palette::PaletteId;
    use ez_core::*;
    let gpu = match Gpu::headless() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("skipping GPU test: {e:#}");
            return;
        }
    };
    let dir = snapshot_dir().join("screens");
    std::fs::create_dir_all(&dir).unwrap();
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 1);
    let (w, h) = (640u32, 360u32);
    let target = r.create_target(w, h);
    let scene = || {
        let mut p = presets::empty();
        p.post.grade.vignette = Param::new(0.0);
        p.post.grade.grain = Param::new(0.0);
        p.post.bloom.enabled = false;
        if let LayerKind::Mesh(m) = &mut p.layers[2].kind {
            m.material.texture = Some("plasma".into());
        }
        p
    };
    let rgb = |c: u32| [(c >> 16) as u8, (c >> 8) as u8, c as u8];
    let near =
        |p: &image::Rgba<u8>, c: [u8; 3]| (0..3).all(|k| (p[k] as i32 - c[k] as i32).abs() <= 2);

    // C64 multicolour on a 4:3 TV inside a 16:9 output.
    let mut p = scene();
    ScreenPreset::C64Multicolour.apply(&mut p.retro, &mut p.post.palette);
    let img = r.render_image(&p, &EvalCtx::new(&p.timing, 0.3, None), &target);
    img.save(dir.join("c64.png")).unwrap();
    let border = rgb(0x6c5eb5);
    let pal: Vec<[u8; 3]> = PaletteId::C64.colors().iter().map(|c| rgb(*c)).collect();
    let (x0, x1) = (w / 8, w - w / 8);
    let mut bad_border = 0;
    let mut off_palette = 0;
    let mut changes = 0u32;
    for y in 0..h {
        for x in 0..w {
            let px = img.get_pixel(x, y);
            if x < x0 - 1 || x > x1 {
                bad_border += !near(px, border) as u32;
            } else if x > x0 && x < x1 - 1 {
                off_palette += !pal.iter().any(|c| near(px, *c)) as u32;
            }
        }
        // Colour changes along a row: at most one per machine pixel (160
        // across), in the busiest row.
        let mut n = 0;
        for x in x0 + 1..x1 - 1 {
            n += (img.get_pixel(x, y) != img.get_pixel(x + 1, y)) as u32;
        }
        changes = changes.max(n);
    }
    eprintln!("C64: {bad_border} border pixels off, {off_palette} pixels off the palette, {changes} changes in the busiest row");
    assert_eq!(bad_border, 0, "the border isn't the border colour");
    assert_eq!(off_palette, 0, "colours outside the C64 palette");
    assert!(changes <= 160, "finer than 160 pixels across: {changes}");
    assert!(changes > 10, "the picture is flat");

    // Loops.
    let a = r.render_image(&p, &EvalCtx::new(&p.timing, 0.0, None), &target);
    let b = r.render_image(&p, &EvalCtx::new(&p.timing, 1.0, None), &target);
    assert!(
        mean_abs_diff(a.as_raw(), b.as_raw()) < 0.6,
        "the C64 screen doesn't loop"
    );

    // Game Boy: square pixels, 10:9, four greens.
    let mut p = scene();
    ScreenPreset::GameBoy.apply(&mut p.retro, &mut p.post.palette);
    let img = r.render_image(&p, &EvalCtx::new(&p.timing, 0.3, None), &target);
    img.save(dir.join("gameboy.png")).unwrap();
    let greens: Vec<[u8; 3]> = PaletteId::GameBoy
        .colors()
        .iter()
        .map(|c| rgb(*c))
        .collect();
    assert!(
        img.pixels().all(|px| greens.iter().any(|c| near(px, *c))),
        "Game Boy colours"
    );

    // Palettes over the full picture: the NES list and the Amiga cube.
    let mut p = scene();
    p.post.palette.enabled = true;
    p.post.palette.palette = PaletteId::Nes;
    let img = r.render_image(&p, &EvalCtx::new(&p.timing, 0.3, None), &target);
    img.save(dir.join("nes.png")).unwrap();
    let nes: Vec<[u8; 3]> = PaletteId::Nes.colors().iter().map(|c| rgb(*c)).collect();
    let used = img
        .pixels()
        .map(|px| [px[0], px[1], px[2]])
        .collect::<std::collections::HashSet<_>>();
    assert!(
        used.iter().all(|u| nes
            .iter()
            .any(|c| (0..3).all(|k| (u[k] as i32 - c[k] as i32).abs() <= 2))),
        "NES colours"
    );
    assert!(used.len() > 8, "only {} NES colours used", used.len());
    for (pal, levels) in [(PaletteId::Amiga, 16u32), (PaletteId::AmstradCpc, 3)] {
        p.post.palette.palette = pal;
        let img = r.render_image(&p, &EvalCtx::new(&p.timing, 0.3, None), &target);
        let step = 255.0 / (levels - 1) as f32;
        let ok = img.as_raw().chunks(4).all(|px| {
            (0..3).all(|k| {
                let v = px[k] as f32 / step;
                (v - v.round()).abs() * step <= 2.0
            })
        });
        assert!(ok, "{pal:?} has channels off its {levels} levels");
    }
}
