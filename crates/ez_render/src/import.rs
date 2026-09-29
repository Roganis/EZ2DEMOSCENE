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
/// glTF files must embed their buffers (.glb or data URIs). Images are not
/// read (materials come from the layer), so files that point at texture
/// files next to them still load.
pub fn load_mesh_bytes(ext: &str, bytes: &[u8]) -> Result<MeshData> {
    let mut m = match ext {
        "gltf" | "glb" => {
            let gltf = gltf::Gltf::from_slice(bytes).context("reading glTF")?;
            let buffers = gltf::import_buffers(&gltf.document, None, gltf.blob.clone())
                .context("reading glTF buffers")?;
            gltf_to_mesh(&gltf.document, &buffers)?
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
    // Buffers only: pictures (materials) are read separately, so a missing
    // texture file doesn't stop the shape from loading.
    let gltf = gltf::Gltf::open(path).with_context(|| format!("reading {}", path.display()))?;
    let buffers = gltf::import_buffers(&gltf.document, path.parent(), gltf.blob.clone())
        .context("reading glTF buffers")?;
    gltf_to_mesh(&gltf.document, &buffers)
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

/// The physical material of a glTF model (its first textured or coloured
/// one), with its pictures decoded. Maps follow glTF: the ORM picture
/// holds occlusion (red), roughness (green) and metalness (blue).
#[derive(Clone, Debug)]
pub struct ModelMaterial {
    pub base_color: [f32; 3],
    pub metallic: f32,
    pub roughness: f32,
    /// Glow colour times its strength.
    pub emissive: [f32; 3],
    pub transmission: f32,
    pub ior: f32,
    pub color_map: Option<image::RgbaImage>,
    pub orm_map: Option<image::RgbaImage>,
    pub emissive_map: Option<image::RgbaImage>,
}

/// Read the material of a glTF / GLB model by asset path. `None` for
/// other formats and for models without materials.
pub fn load_model_material(path: &str) -> Result<Option<ModelMaterial>> {
    let ext = ez_core::store::extension(path);
    if ext != "gltf" && ext != "glb" {
        return Ok(None);
    }
    let bytes = ez_core::store::read(path).with_context(|| format!("reading {path}"))?;
    let gltf = gltf::Gltf::from_slice(&bytes).context("reading glTF")?;
    // Files next to a model on disk can be read; in memory only embedded
    // data can.
    let dir = (!ez_core::store::is_mem(path) && !cfg!(target_arch = "wasm32"))
        .then(|| std::path::Path::new(path).parent().map(|d| d.to_path_buf()))
        .flatten();
    let buffers = gltf::import_buffers(&gltf.document, dir.as_deref(), gltf.blob.clone())
        .context("reading glTF buffers")?;
    // A picture that can't be read is skipped, not fatal.
    let images: Vec<Option<gltf::image::Data>> = gltf
        .document
        .images()
        .map(|i| gltf::image::Data::from_source(i.source(), dir.as_deref(), &buffers).ok())
        .collect();
    Ok(model_material(&gltf.document, &images))
}

fn model_material(
    doc: &gltf::Document,
    images: &[Option<gltf::image::Data>],
) -> Option<ModelMaterial> {
    // The material of the first primitive that has one.
    let mat = doc
        .meshes()
        .flat_map(|m| m.primitives().collect::<Vec<_>>())
        .map(|p| p.material())
        .find(|m| m.index().is_some())?;
    let pbr = mat.pbr_metallic_roughness();
    let image = |t: Option<gltf::texture::Texture>| {
        t.and_then(|t| images.get(t.source().index())?.as_ref())
            .and_then(to_rgba)
    };
    let color_map = image(pbr.base_color_texture().map(|t| t.texture()));
    let mr = image(pbr.metallic_roughness_texture().map(|t| t.texture()));
    let occ = image(mat.occlusion_texture().map(|t| t.texture()));
    let orm_map = orm_picture(mr, occ);
    let emissive_map = image(mat.emissive_texture().map(|t| t.texture()));
    let [r, g, b, _] = pbr.base_color_factor();
    let strength = mat.emissive_strength().unwrap_or(1.0);
    let e = mat.emissive_factor();
    Some(ModelMaterial {
        base_color: [r, g, b],
        metallic: pbr.metallic_factor(),
        roughness: pbr.roughness_factor(),
        emissive: [e[0] * strength, e[1] * strength, e[2] * strength],
        transmission: mat
            .transmission()
            .map(|t| t.transmission_factor())
            .unwrap_or(0.0),
        ior: mat.ior().unwrap_or(1.5),
        color_map,
        orm_map,
        emissive_map,
    })
}

/// One ORM picture from glTF's metal/roughness picture (green, blue) and
/// occlusion picture (red), which may be separate or missing.
fn orm_picture(
    mr: Option<image::RgbaImage>,
    occ: Option<image::RgbaImage>,
) -> Option<image::RgbaImage> {
    match (mr, occ) {
        (None, None) => None,
        (Some(mut mr), occ) => {
            let occ = occ.map(|o| {
                image::imageops::resize(
                    &o,
                    mr.width(),
                    mr.height(),
                    image::imageops::FilterType::Triangle,
                )
            });
            for (x, y, p) in mr.enumerate_pixels_mut() {
                p[0] = occ.as_ref().map(|o| o.get_pixel(x, y)[0]).unwrap_or(255);
                p[3] = 255;
            }
            Some(mr)
        }
        (None, Some(mut occ)) => {
            for p in occ.pixels_mut() {
                *p = image::Rgba([p[0], 255, 255, 255]);
            }
            Some(occ)
        }
    }
}

fn to_rgba(d: &gltf::image::Data) -> Option<image::RgbaImage> {
    use gltf::image::Format;
    let n = (d.width * d.height) as usize;
    let px: Vec<u8> = match d.format {
        Format::R8G8B8A8 => d.pixels.clone(),
        Format::R8G8B8 => d
            .pixels
            .chunks(3)
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        Format::R8G8 => d
            .pixels
            .chunks(2)
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        Format::R8 => d.pixels.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        // 16-bit: keep the high byte (little-endian pairs).
        Format::R16G16B16A16 => d.pixels.chunks(2).map(|c| c[1]).collect(),
        Format::R16G16B16 => d
            .pixels
            .chunks(6)
            .flat_map(|c| [c[1], c[3], c[5], 255])
            .collect(),
        _ => return None,
    };
    if px.len() != n * 4 {
        return None;
    }
    image::RgbaImage::from_raw(d.width, d.height, px)
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

    #[test]
    fn reads_gltf_materials() {
        let dir = std::env::temp_dir().join("ez2_gltf_material_test");
        std::fs::create_dir_all(&dir).unwrap();
        let tri: Vec<u8> = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        std::fs::write(dir.join("tri.bin"), tri).unwrap();
        // Metal/roughness in green and blue, occlusion (red) separate.
        image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 100, 200, 255]))
            .save(dir.join("mr.png"))
            .unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([50, 0, 0, 255]))
            .save(dir.join("occ.png"))
            .unwrap();
        let json = r#"{
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "material": 0}]}],
            "buffers": [{"uri": "tri.bin", "byteLength": 36}],
            "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3",
                           "min": [0, 0, 0], "max": [1, 1, 0]}],
            "images": [{"uri": "mr.png"}, {"uri": "occ.png"}, {"uri": "missing.png"}],
            "textures": [{"source": 0}, {"source": 1}, {"source": 2}],
            "materials": [{
                "pbrMetallicRoughness": {
                    "baseColorFactor": [1, 0.5, 0.25, 1],
                    "metallicFactor": 0.8,
                    "roughnessFactor": 0.6,
                    "metallicRoughnessTexture": {"index": 0}
                },
                "occlusionTexture": {"index": 1},
                "emissiveTexture": {"index": 2},
                "emissiveFactor": [1, 0, 0]
            }]
        }"#;
        let path = dir.join("model.gltf");
        std::fs::write(&path, json).unwrap();
        let m = load_model_material(path.to_str().unwrap())
            .unwrap()
            .expect("a material");
        assert_eq!(m.base_color, [1.0, 0.5, 0.25]);
        assert_eq!((m.metallic, m.roughness), (0.8, 0.6));
        assert_eq!(m.emissive, [1.0, 0.0, 0.0]);
        let orm = m.orm_map.expect("an ORM picture");
        assert_eq!(orm.dimensions(), (4, 4));
        assert_eq!(orm.get_pixel(1, 1).0, [50, 100, 200, 255]);
        // The missing glow picture is skipped.
        assert!(m.emissive_map.is_none() && m.color_map.is_none());
        // The mesh still loads, and OBJ files have no material here.
        assert!(load_mesh(&path).is_ok());
        assert!(load_model_material("x.obj").unwrap().is_none());
    }
}
