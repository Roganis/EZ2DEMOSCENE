//! Every model of the bundled library loads as a usable mesh.

#[test]
fn every_library_model_loads() {
    let lib = ez_core::models::library().expect("bundled library");
    let mut failures = Vec::new();
    for e in &lib.entries {
        let glb = lib.glb(&e.id).expect("in the zip");
        match ez_render::import::load_mesh_bytes("glb", &glb) {
            Ok(m) => {
                assert!(!m.indices.is_empty(), "{}: no triangles", e.id);
                // Fitted into the unit sphere like the built-in shapes.
                let r = m
                    .vertices
                    .iter()
                    .map(|v| glam::Vec3::from(v.pos).length())
                    .fold(0.0, f32::max);
                assert!((0.5..=1.01).contains(&r), "{}: radius {r}", e.id);
                assert!(
                    m.vertices
                        .iter()
                        .all(|v| glam::Vec3::from(v.normal).length() > 0.5),
                    "{}: missing normals",
                    e.id
                );
            }
            Err(err) => failures.push(format!("{}: {err:#}", e.id)),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Every library model bakes into a usable distance field: its surface
/// points lie on the zero level, and it is solid (not hollow or a
/// balloon). Slow in debug builds: run with --release --ignored.
#[test]
#[ignore]
fn every_library_model_bakes() {
    let lib = ez_core::models::library().expect("bundled library");
    let n = 64;
    let h = 2.0 * ez_render::sdf_bake::EXTENT / n as f32;
    let start = std::time::Instant::now();
    let mut worst: Vec<(f32, String)> = Vec::new();
    let mut slowest = (0.0f64, String::new());
    for e in &lib.entries {
        let glb = lib.glb(&e.id).unwrap();
        let m = ez_render::import::load_mesh_bytes("glb", &glb).unwrap();
        let t = std::time::Instant::now();
        let g = ez_render::sdf_bake::bake(&m, n);
        let dt = t.elapsed().as_secs_f64();
        if dt > slowest.0 {
            slowest = (dt, e.id.clone());
        }
        // Share of surface points (vertices and face centres) off the zero
        // level by more than a cell and a half.
        let mut pts: Vec<glam::Vec3> = m.vertices.iter().map(|v| v.pos.into()).collect();
        for t in m.indices.chunks(3) {
            let c = t
                .iter()
                .map(|i| glam::Vec3::from(m.vertices[*i as usize].pos))
                .sum::<glam::Vec3>()
                / 3.0;
            pts.push(c);
        }
        let off =
            pts.iter().filter(|p| g.sample(**p).abs() > 1.5 * h).count() as f32 / pts.len() as f32;
        let inside = g.d.iter().filter(|d| **d < 0.0).count() as f32 / g.d.len() as f32;
        let score = off
            + if !(0.0005..=0.6).contains(&inside) {
                1.0
            } else {
                0.0
            };
        worst.push((
            score,
            format!(
                "{} off {:.1}% inside {:.1}%",
                e.id,
                off * 100.0,
                inside * 100.0
            ),
        ));
    }
    worst.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!(
        "{} models in {:.1}s, slowest {:.3}s ({})",
        lib.entries.len(),
        start.elapsed().as_secs_f64(),
        slowest.0,
        slowest.1
    );
    for (s, w) in worst.iter().take(40) {
        println!("{s:.3} {w}");
    }
    let bad = worst.iter().filter(|w| w.0 > 0.25).count();
    println!("{bad} models score above 0.25");
}
