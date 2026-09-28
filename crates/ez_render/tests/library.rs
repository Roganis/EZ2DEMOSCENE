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
                    m.vertices.iter().all(|v| glam::Vec3::from(v.normal).length() > 0.5),
                    "{}: missing normals",
                    e.id
                );
            }
            Err(err) => failures.push(format!("{}: {err:#}", e.id)),
        }
    }
    assert!(failures.is_empty(), "{} failed:\n{}", failures.len(), failures.join("\n"));
}
