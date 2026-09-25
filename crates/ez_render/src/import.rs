//! Loading user meshes (glTF / GLB / OBJ) into [`MeshData`].
//! Imported meshes are centred and fitted into a unit sphere so they behave
//! like the built-in primitives; materials come from the layer.

use crate::mesh::{MeshData, Vertex};
use anyhow::{bail, Context, Result};
use glam::{Mat4, Vec3};
use std::path::Path;

/// Load a mesh by asset path: in-memory (`mem://`) assets are parsed from
/// their bytes, files on disk with [`load_mesh`] (which also resolves
/// external glTF buffers).
pub fn load_mesh_asset(path: &str) -> Result<MeshData> {
    if ez_core::store::is_mem(path) || cfg!(target_arch = "wasm32") {
        let bytes = ez_core::store::read(path).with_context(|| format!("reading {path}"))?;
        load_mesh_bytes(&ez_core::store::extension(path), &bytes)
    } else {
        load_mesh(Path::new(path))
    }
}

/// Parse a mesh from bytes. `ext` is the file extension (gltf, glb, obj).
/// glTF files must embed their buffers (.glb or data URIs).
pub fn load_mesh_bytes(ext: &str, bytes: &[u8]) -> Result<MeshData> {
    let mut m = match ext {
        "gltf" | "glb" => {
            let (doc, buffers, _images) = gltf::import_slice(bytes).context("reading glTF")?;
            gltf_to_mesh(&doc, &buffers)?
        }
        "obj" => {
            let mut reader = std::io::BufReader::new(bytes);
            let (models, _) = tobj::load_obj_buf(&mut reader, &tobj::GPU_LOAD_OPTIONS, |_| {
                Ok((Vec::new(), Default::default()))
            })
            .context("reading OBJ")?;
            obj_to_mesh(models)
        }
        _ => bail!("unsupported mesh format '{ext}' (use .gltf, .glb or .obj)"),
    };
    if m.vertices.is_empty() {
        bail!("the model contains no triangles");
    }
    fill_missing_normals(&mut m);
    m.normalize_size();
    Ok(m)
}

pub fn load_mesh(path: &Path) -> Result<MeshData> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut m = match ext.as_str() {
        "gltf" | "glb" => load_gltf(path)?,
        "obj" => load_obj(path)?,
        _ => bail!("unsupported mesh format '{ext}' (use .gltf, .glb or .obj)"),
    };
    if m.vertices.is_empty() {
        bail!("{} contains no triangles", path.display());
    }
    fill_missing_normals(&mut m);
    m.normalize_size();
    Ok(m)
}

fn load_gltf(path: &Path) -> Result<MeshData> {
    let (doc, buffers, _images) =
        gltf::import(path).with_context(|| format!("reading {}", path.display()))?;
    gltf_to_mesh(&doc, &buffers)
}

fn gltf_to_mesh(doc: &gltf::Document, buffers: &[gltf::buffer::Data]) -> Result<MeshData> {
    let mut out = MeshData::default();
    let scene = doc.default_scene().or_else(|| doc.scenes().next());
    let Some(scene) = scene else {
        bail!("glTF has no scene");
    };
    for node in scene.nodes() {
        visit_node(&node, Mat4::IDENTITY, buffers, &mut out);
    }
    Ok(out)
}

fn visit_node(node: &gltf::Node, parent: Mat4, buffers: &[gltf::buffer::Data], out: &mut MeshData) {
    let local = Mat4::from_cols_array_2d(&node.transform().matrix());
    let world = parent * local;
    if let Some(mesh) = node.mesh() {
        let nmat = world.inverse().transpose();
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| &d.0[..]));
            let Some(pos) = reader.read_positions() else {
                continue;
            };
            let pos: Vec<[f32; 3]> = pos.collect();
            let normals: Option<Vec<[f32; 3]>> = reader.read_normals().map(|n| n.collect());
            let uvs: Option<Vec<[f32; 2]>> =
                reader.read_tex_coords(0).map(|t| t.into_f32().collect());
            let base = out.vertices.len() as u32;
            for (i, p) in pos.iter().enumerate() {
                let wp = world.transform_point3(Vec3::from(*p));
                let n = normals
                    .as_ref()
                    .map(|n| {
                        nmat.transform_vector3(Vec3::from(n[i]))
                            .normalize_or(Vec3::Y)
                    })
                    .unwrap_or(Vec3::ZERO);
                out.vertices.push(Vertex {
                    pos: wp.into(),
                    normal: n.into(),
                    uv: uvs.as_ref().map(|u| u[i]).unwrap_or([0.0, 0.0]),
                    edge: 1.0,
                });
            }
            match reader.read_indices() {
                Some(idx) => out.indices.extend(idx.into_u32().map(|i| i + base)),
                None => out.indices.extend(base..base + pos.len() as u32),
            }
        }
    }
    for child in node.children() {
        visit_node(&child, world, buffers, out);
    }
}

fn load_obj(path: &Path) -> Result<MeshData> {
    let (models, _mats) = tobj::load_obj(path, &tobj::GPU_LOAD_OPTIONS)
        .with_context(|| format!("reading {}", path.display()))?;
    Ok(obj_to_mesh(models))
}

fn obj_to_mesh(models: Vec<tobj::Model>) -> MeshData {
    let mut out = MeshData::default();
    for model in models {
        let m = &model.mesh;
        let base = out.vertices.len() as u32;
        let n = m.positions.len() / 3;
        for i in 0..n {
            let normal = if m.normals.len() >= (i + 1) * 3 {
                [m.normals[i * 3], m.normals[i * 3 + 1], m.normals[i * 3 + 2]]
            } else {
                [0.0; 3]
            };
            let uv = if m.texcoords.len() >= (i + 1) * 2 {
                [m.texcoords[i * 2], 1.0 - m.texcoords[i * 2 + 1]]
            } else {
                [0.0; 2]
            };
            out.vertices.push(Vertex {
                pos: [
                    m.positions[i * 3],
                    m.positions[i * 3 + 1],
                    m.positions[i * 3 + 2],
                ],
                normal,
                uv,
                edge: 1.0,
            });
        }
        out.indices.extend(m.indices.iter().map(|i| i + base));
    }
    out
}

/// Compute smooth normals for vertices that have none.
fn fill_missing_normals(m: &mut MeshData) {
    if m.vertices
        .iter()
        .all(|v| Vec3::from(v.normal).length_squared() > 0.0)
    {
        return;
    }
    let mut acc = vec![Vec3::ZERO; m.vertices.len()];
    for t in m.indices.chunks(3) {
        if t.len() < 3 {
            continue;
        }
        let a = Vec3::from(m.vertices[t[0] as usize].pos);
        let b = Vec3::from(m.vertices[t[1] as usize].pos);
        let c = Vec3::from(m.vertices[t[2] as usize].pos);
        let n = (b - a).cross(c - a);
        for &i in t {
            acc[i as usize] += n;
        }
    }
    for (v, n) in m.vertices.iter_mut().zip(acc) {
        if Vec3::from(v.normal).length_squared() == 0.0 {
            v.normal = n.normalize_or(Vec3::Y).into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_obj_from_bytes() {
        let m = load_mesh_bytes("obj", b"v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n").unwrap();
        assert_eq!(m.indices.len(), 3);
        assert!(load_mesh_bytes("stl", b"").is_err());
    }

    #[test]
    fn loads_obj() {
        let dir = std::env::temp_dir().join("ez2_obj_test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("tri.obj");
        std::fs::write(&p, "v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n").unwrap();
        let m = load_mesh(&p).unwrap();
        assert_eq!(m.vertices.len(), 3);
        assert_eq!(m.indices.len(), 3);
        // fitted into a unit sphere
        assert!(m
            .vertices
            .iter()
            .all(|v| Vec3::from(v.pos).length() <= 1.0001));
        assert!(Vec3::from(m.vertices[0].normal).z.abs() > 0.99);
    }
}
