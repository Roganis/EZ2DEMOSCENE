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
