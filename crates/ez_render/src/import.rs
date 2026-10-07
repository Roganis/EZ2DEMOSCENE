//! Loading user meshes (glTF / GLB, OBJ, STL, PLY, OFF, 3MF) into
//! [`MeshData`]. Imported meshes are centred and fitted into a unit sphere
//! so they behave like the built-in primitives; materials come from the
//! layer.
//!
//! STL and 3MF (3D printing) are Z up and are turned to stand Y up like the
//! other formats. Files without normals get smooth ones, kept sharp across
//! creases (a cube stays a cube).

use crate::mesh::{MeshData, Vertex};
use anyhow::{bail, Context, Result};
use glam::{Mat4, Vec3};
use std::collections::HashMap;
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

/// Mesh file extensions [`load_mesh_bytes`] reads.
pub const MESH_EXTENSIONS: &[&str] = &["gltf", "glb", "obj", "stl", "ply", "off", "3mf"];

/// Parse a mesh from bytes. `ext` is the file extension (see
/// [`MESH_EXTENSIONS`]).
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
        "stl" => stl_to_mesh(bytes)?,
        "ply" => ply_to_mesh(bytes)?,
        "off" => off_to_mesh(bytes)?,
        "3mf" => three_mf_to_mesh(bytes)?,
        _ => bail!("unsupported mesh format '{ext}' (use {})", extension_list()),
    };
    if m.vertices.is_empty() {
        bail!("the model contains no triangles");
    }
    fill_missing_normals(&mut m);
    m.normalize_size();
    Ok(m)
}

fn extension_list() -> String {
    MESH_EXTENSIONS
        .iter()
        .map(|e| format!(".{e}"))
        .collect::<Vec<_>>()
        .join(", ")
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
        "stl" | "ply" | "off" | "3mf" => {
            let bytes =
                std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            return load_mesh_bytes(&ext, &bytes).with_context(|| path.display().to_string());
        }
        _ => bail!("unsupported mesh format '{ext}' (use {})", extension_list()),
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

// --- STL, PLY, OFF, 3MF ---------------------------------------------------------

/// Triangles as positions and vertex indices, with normals and texture
/// coordinates per vertex if the file had them.
#[derive(Default)]
struct Soup {
    positions: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
    normals: Option<Vec<Vec3>>,
    uvs: Option<Vec<[f32; 2]>>,
}

impl Soup {
    /// Fan polygons into triangles (polygons with fewer than three corners
    /// are dropped); fails on an index past the vertices.
    fn add_polygon(&mut self, corners: &[u32]) -> Result<()> {
        if let Some(&bad) = corners
            .iter()
            .find(|&&i| i as usize >= self.positions.len())
        {
            bail!("a face uses vertex {bad}, which doesn't exist");
        }
        for k in 1..corners.len().saturating_sub(1) {
            self.triangles
                .push([corners[0], corners[k], corners[k + 1]]);
        }
        Ok(())
    }

    /// Turn a Z-up model to stand Y up.
    fn z_up(mut self) -> Soup {
        let turn = |v: &mut Vec3| *v = Vec3::new(v.x, v.z, -v.y);
        self.positions.iter_mut().for_each(turn);
        if let Some(n) = &mut self.normals {
            n.iter_mut().for_each(turn);
        }
        self
    }

    fn into_mesh(self) -> MeshData {
        let mut out = MeshData::default();
        if let Some(normals) = &self.normals {
            for (i, p) in self.positions.iter().enumerate() {
                out.vertices.push(Vertex {
                    pos: (*p).into(),
                    normal: normals[i].normalize_or(Vec3::ZERO).into(),
                    uv: self.uvs.as_ref().map(|u| u[i]).unwrap_or_default(),
                    edge: 1.0,
                });
            }
            out.indices.extend(self.triangles.iter().flatten());
        } else {
            crease_normals(&self, &mut out);
        }
        out
    }
}

/// Smooth normals that stay sharp across creases: each corner averages the
/// faces around its vertex that turn less than 45° from its own face.
/// Corners at the same place with the same normal share a vertex.
fn crease_normals(soup: &Soup, out: &mut MeshData) {
    let cos_crease = 45f32.to_radians().cos();
    // Corners at the same place are one vertex, whatever the file says.
    let mut at: HashMap<[u32; 3], u32> = HashMap::new();
    let mut place = Vec::with_capacity(soup.positions.len());
    for p in &soup.positions {
        let key = [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()];
        let next = at.len() as u32;
        place.push(*at.entry(key).or_insert(next));
    }
    let faces: Vec<(Vec3, Vec3)> = soup
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| soup.positions[i as usize]);
            let n = (b - a).cross(c - a);
            (n, n.normalize_or(Vec3::ZERO))
        })
        .collect();
    let mut around: Vec<Vec<u32>> = vec![Vec::new(); at.len()];
    for (f, t) in soup.triangles.iter().enumerate() {
        for &i in t {
            around[place[i as usize] as usize].push(f as u32);
        }
    }
    let mut made: HashMap<(u32, [i32; 3]), u32> = HashMap::new();
    for (f, t) in soup.triangles.iter().enumerate() {
        let own = faces[f].1;
        for &i in t {
            let v = place[i as usize];
            let mut n = Vec3::ZERO;
            for &g in &around[v as usize] {
                let (area_n, unit) = faces[g as usize];
                if g as usize == f || unit.dot(own) >= cos_crease {
                    n += area_n;
                }
            }
            let n = n.normalize_or(own);
            let key = (v, (n * 1000.0).round().as_ivec3().to_array());
            let index = *made.entry(key).or_insert_with(|| {
                out.vertices.push(Vertex {
                    pos: soup.positions[i as usize].into(),
                    normal: n.into(),
                    uv: soup.uvs.as_ref().map(|u| u[i as usize]).unwrap_or_default(),
                    edge: 1.0,
                });
                out.vertices.len() as u32 - 1
            });
            out.indices.push(index);
        }
    }
}

/// Binary or ASCII STL (Z up).
fn stl_to_mesh(bytes: &[u8]) -> Result<MeshData> {
    let mut soup = Soup::default();
    let binary_count = bytes
        .get(80..84)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    match binary_count {
        // A binary file is exactly its header, count and 50 bytes a triangle
        // (some start with "solid" too).
        Some(n) if bytes.len() == 84 + 50 * n => {
            for t in bytes[84..].chunks_exact(50) {
                let f = |k: usize| f32::from_le_bytes(t[k * 4..k * 4 + 4].try_into().unwrap());
                let base = soup.positions.len() as u32;
                for c in 0..3 {
                    soup.positions
                        .push(Vec3::new(f(3 + c * 3), f(4 + c * 3), f(5 + c * 3)));
                }
                soup.triangles.push([base, base + 1, base + 2]);
            }
        }
        _ => {
            let text = std::str::from_utf8(bytes).context("not an STL file")?;
            if !text.trim_start().starts_with("solid") {
                bail!("not an STL file");
            }
            let mut words = text.split_ascii_whitespace();
            while let Some(w) = words.next() {
                if w == "vertex" {
                    let mut xyz = [0.0f32; 3];
                    for v in &mut xyz {
                        *v = words
                            .next()
                            .and_then(|s| s.parse().ok())
                            .context("an STL vertex isn't three numbers")?;
                    }
                    soup.positions.push(Vec3::from(xyz));
                }
            }
            if soup.positions.len() % 3 != 0 {
                bail!("an STL facet doesn't have three vertices");
            }
            let n = soup.positions.len() as u32;
            soup.triangles = (0..n / 3).map(|t| [3 * t, 3 * t + 1, 3 * t + 2]).collect();
        }
    }
    Ok(soup.z_up().into_mesh())
}

/// A PLY mesh: vertices (with normals and texture coordinates if present)
/// and polygons. PLY files of points or splats have no faces.
fn ply_to_mesh(bytes: &[u8]) -> Result<MeshData> {
    let ply = crate::ply::Ply::parse(bytes)?;
    let vertex = ply
        .element("vertex")
        .context("the PLY file has no vertices")?;
    let faces = ply
        .element("face")
        .filter(|f| f.count > 0)
        .context("the PLY file has no faces (points or Gaussian splats, not a mesh)")?;
    let index = ["vertex_indices", "vertex_index"]
        .into_iter()
        .find(|n| faces.has(n))
        .context("the PLY faces have no vertex_indices")?;
    let uv_names = [("u", "v"), ("s", "t"), ("texture_u", "texture_v")]
        .into_iter()
        .find(|(u, v)| vertex.has(u) && vertex.has(v));
    let mut names = vec!["x", "y", "z", "nx", "ny", "nz"];
    if let Some((u, v)) = uv_names {
        names.extend([u, v]);
    }
    let data = ply.read(&[("vertex", &names), ("face", &[index])])?;
    let v = &data["vertex"].scalars;
    let col = |n: &str| {
        v.get(n)
            .with_context(|| format!("the PLY vertices have no {n}"))
    };
    let (x, y, z) = (col("x")?, col("y")?, col("z")?);
    let mut soup = Soup {
        positions: (0..x.len()).map(|i| Vec3::new(x[i], y[i], z[i])).collect(),
        ..Default::default()
    };
    if let (Some(nx), Some(ny), Some(nz)) = (v.get("nx"), v.get("ny"), v.get("nz")) {
        let normals: Vec<Vec3> = (0..x.len())
            .map(|i| Vec3::new(nx[i], ny[i], nz[i]))
            .collect();
        // Some writers put zeros there.
        if normals.iter().any(|n| n.length_squared() > 0.0) {
            soup.normals = Some(normals);
        }
    }
    if let Some((u, w)) = uv_names {
        let (u, w) = (&v[u], &v[w]);
        soup.uvs = Some((0..x.len()).map(|i| [u[i], 1.0 - w[i]]).collect());
    }
    let lists = &data["face"].lists[index];
    for f in 0..faces.count {
        let corners: Vec<u32> = lists.get(f).iter().map(|&i| i as u32).collect();
        soup.add_polygon(&corners)?;
    }
    Ok(soup.into_mesh())
}

/// OFF (and its COFF, NOFF... variants; the extra values are skipped).
fn off_to_mesh(bytes: &[u8]) -> Result<MeshData> {
    let text = std::str::from_utf8(bytes).context("not an OFF file")?;
    let mut lines = text
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty());
    let first = lines.next().context("an empty OFF file")?;
    let mut words = first.split_whitespace();
    let keyword = words.next().unwrap_or("");
    if !keyword.ends_with("OFF") || keyword.contains('4') || keyword.contains('n') {
        bail!("not a 3D OFF file");
    }
    let rest: Vec<&str> = words.collect();
    let counts: Vec<usize> = if rest.is_empty() {
        lines
            .next()
            .context("the OFF file has no counts")?
            .split_whitespace()
            .map(|w| w.parse())
            .collect::<Result<_, _>>()?
    } else {
        rest.iter().map(|w| w.parse()).collect::<Result<_, _>>()?
    };
    let (nv, nf) = (
        *counts.first().context("no counts")?,
        *counts.get(1).context("no counts")?,
    );
    let mut soup = Soup::default();
    let number = |w: Option<&str>| -> Result<f32> {
        w.context("the OFF file ends early")?
            .parse()
            .context("an OFF value isn't a number")
    };
    for _ in 0..nv {
        let mut w = lines
            .next()
            .context("the OFF file ends early")?
            .split_whitespace();
        soup.positions.push(Vec3::new(
            number(w.next())?,
            number(w.next())?,
            number(w.next())?,
        ));
    }
    for _ in 0..nf {
        let mut w = lines
            .next()
            .context("the OFF file ends early")?
            .split_whitespace();
        let k = number(w.next())? as usize;
        let corners: Vec<u32> = (0..k)
            .map(|_| number(w.next()).map(|i| i as u32))
            .collect::<Result<_>>()?;
        soup.add_polygon(&corners)?;
    }
    Ok(soup.into_mesh())
}

/// A 3MF package (core specification): its build items, their objects and
/// components placed by their transforms (Z up).
fn three_mf_to_mesh(bytes: &[u8]) -> Result<MeshData> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("not a 3MF file")?;
    let mut read = |name: &str| -> Option<String> {
        let mut s = String::new();
        zip.by_name(name).ok()?.read_to_string(&mut s).ok()?;
        Some(s)
    };
    let model_path = read("_rels/.rels")
        .and_then(|rels| {
            xml_tags(&rels)
                .into_iter()
                .find(|(n, a)| {
                    n == "Relationship" && a.get("Type").is_some_and(|t| t.ends_with("/3dmodel"))
                })
                .and_then(|(_, a)| {
                    a.get("Target")
                        .map(|t| t.trim_start_matches('/').to_string())
                })
        })
        .unwrap_or_else(|| "3D/3dmodel.model".into());
    let xml = read(&model_path).context("the 3MF file has no model")?;

    // Objects: their own triangles and their components.
    #[derive(Default)]
    struct Object {
        positions: Vec<Vec3>,
        triangles: Vec<[u32; 3]>,
        components: Vec<(String, glam::Mat4)>,
    }
    let mut objects: HashMap<String, Object> = HashMap::new();
    let mut items: Vec<(String, glam::Mat4)> = Vec::new();
    let mut current: Option<String> = None;
    let num = |a: &HashMap<String, String>, k: &str| -> Result<f32> {
        a.get(k)
            .with_context(|| format!("a 3MF element has no {k}"))?
            .trim()
            .parse()
            .with_context(|| format!("a 3MF {k} isn't a number"))
    };
    for (name, a) in xml_tags(&xml) {
        match name.as_str() {
            "object" => {
                let id = a.get("id").cloned().unwrap_or_default();
                objects.entry(id.clone()).or_default();
                current = Some(id);
            }
            "vertex" | "triangle" | "component" => {
                let o = current
                    .as_ref()
                    .and_then(|c| objects.get_mut(c))
                    .context("a 3MF mesh outside an object")?;
                match name.as_str() {
                    "vertex" => {
                        o.positions
                            .push(Vec3::new(num(&a, "x")?, num(&a, "y")?, num(&a, "z")?))
                    }
                    "triangle" => o.triangles.push([
                        num(&a, "v1")? as u32,
                        num(&a, "v2")? as u32,
                        num(&a, "v3")? as u32,
                    ]),
                    _ => o.components.push((
                        a.get("objectid").cloned().unwrap_or_default(),
                        transform_3mf(a.get("transform"))?,
                    )),
                }
            }
            "item" => items.push((
                a.get("objectid").cloned().unwrap_or_default(),
                transform_3mf(a.get("transform"))?,
            )),
            _ => {}
        }
    }
    let mut soup = Soup::default();
    fn place(
        objects: &HashMap<String, Object>,
        id: &str,
        m: glam::Mat4,
        depth: u32,
        soup: &mut Soup,
    ) -> Result<()> {
        let o = objects
            .get(id)
            .with_context(|| format!("the 3MF file has no object {id}"))?;
        if depth > 16 {
            bail!("the 3MF objects contain each other");
        }
        let base = soup.positions.len() as u32;
        soup.positions
            .extend(o.positions.iter().map(|p| m.transform_point3(*p)));
        for t in &o.triangles {
            soup.add_polygon(&t.map(|i| base + i))?;
        }
        for (child, cm) in &o.components {
            place(objects, child, m * *cm, depth + 1, soup)?;
        }
        Ok(())
    }
    for (id, m) in &items {
        place(&objects, id, *m, 0, &mut soup)?;
    }
    Ok(soup.z_up().into_mesh())
}

/// A 3MF transform: 12 numbers, rows of a matrix applied to row vectors
/// (p' = p · M + t), as a column-vector matrix.
fn transform_3mf(text: Option<&String>) -> Result<glam::Mat4> {
    let Some(text) = text else {
        return Ok(glam::Mat4::IDENTITY);
    };
    let v: Vec<f32> = text
        .split_whitespace()
        .map(|w| w.parse())
        .collect::<Result<_, _>>()
        .context("a 3MF transform isn't numbers")?;
    if v.len() != 12 {
        bail!("a 3MF transform needs 12 numbers");
    }
    Ok(glam::Mat4::from_cols(
        glam::Vec4::new(v[0], v[1], v[2], 0.0),
        glam::Vec4::new(v[3], v[4], v[5], 0.0),
        glam::Vec4::new(v[6], v[7], v[8], 0.0),
        glam::Vec4::new(v[9], v[10], v[11], 1.0),
    ))
}

/// The start tags of an XML document, in order: local name (without its
/// namespace prefix) and attributes. Enough for machine-written files
/// like 3MF; comments and declarations are skipped.
fn xml_tags(xml: &str) -> Vec<(String, HashMap<String, String>)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        if rest.starts_with("!--") {
            rest = rest.find("-->").map_or("", |e| &rest[e + 3..]);
            continue;
        }
        let Some(close) = rest.find('>') else { break };
        let tag = &rest[..close];
        rest = &rest[close + 1..];
        if tag.starts_with(['/', '?', '!']) {
            continue;
        }
        let tag = tag.trim_end_matches('/');
        let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
        let name = tag[..name_end].rsplit(':').next().unwrap_or("").to_string();
        let mut attrs = HashMap::new();
        let mut a = &tag[name_end..];
        while let Some(eq) = a.find('=') {
            let key = a[..eq].trim().rsplit(':').next().unwrap_or("").to_string();
            let after = a[eq + 1..].trim_start();
            let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                break;
            };
            let Some(end) = after[1..].find(quote) else {
                break;
            };
            attrs.insert(key, after[1..1 + end].to_string());
            a = &after[end + 2..];
        }
        out.push((name, attrs));
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

    /// A unit cube as 12 triangles (counter-clockwise from outside).
    fn cube() -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
        let p = (0..8)
            .map(|i| [(i & 1) as f32, (i >> 1 & 1) as f32, (i >> 2 & 1) as f32])
            .collect();
        let quads = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        let t = quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect();
        (p, t)
    }

    fn binary_stl(p: &[[f32; 3]], t: &[[u32; 3]]) -> Vec<u8> {
        let mut b = vec![0u8; 80];
        b[..5].copy_from_slice(b"solid"); // some writers start binary files so
        b.extend((t.len() as u32).to_le_bytes());
        for tri in t {
            b.extend([0u8; 12]); // normal: left to the reader
            for &i in tri {
                for v in p[i as usize] {
                    b.extend(v.to_le_bytes());
                }
            }
            b.extend([0u8; 2]);
        }
        b
    }

    #[test]
    fn stl_cube_keeps_sharp_edges_and_stands_up() {
        let (p, t) = cube();
        let m = load_mesh_bytes("stl", &binary_stl(&p, &t)).unwrap();
        // Six flat sides: four corners each, normals along the axes.
        assert_eq!((m.vertices.len(), m.indices.len()), (24, 36));
        for v in &m.vertices {
            let n = Vec3::from(v.normal);
            assert!((n.abs().max_element() - 1.0).abs() < 1e-5, "{n}");
        }
        // Faces still face out after the Z-up turn and the fit.
        for t in m.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(m.vertices[t[k] as usize].pos));
            let n = (b - a).cross(c - a);
            assert!(n.dot(a + b + c) > 0.0);
        }
        let text = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\n\
            vertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
        let m = load_mesh_bytes("stl", text.as_bytes()).unwrap();
        assert_eq!(m.indices.len(), 3);
        // Z up became Y up.
        assert!(Vec3::from(m.vertices[0].normal).y > 0.99);
        assert!(load_mesh_bytes("stl", b"no mesh here").is_err());
    }

    #[test]
    fn ply_meshes() {
        let text = "ply\nformat ascii 1.0\nelement vertex 4\nproperty float x\nproperty float y\n\
            property float z\nproperty float s\nproperty float t\nelement face 1\n\
            property list uchar int vertex_indices\nend_header\n\
            0 0 0 0 0\n1 0 0 1 0\n1 1 0 1 1\n0 1 0 0 1\n4 0 1 2 3\n";
        let m = load_mesh_bytes("ply", text.as_bytes()).unwrap();
        assert_eq!(m.indices.len(), 6);
        // Texture coordinates kept (v flipped as for OBJ), normals made.
        assert!(m.vertices.iter().any(|v| v.uv == [1.0, 0.0]));
        assert!(Vec3::from(m.vertices[0].normal).z.abs() > 0.99);
        let points = "ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\n\
            property float y\nproperty float z\nend_header\n0 0 0\n";
        let err = load_mesh_bytes("ply", points.as_bytes()).unwrap_err();
        assert!(format!("{err:#}").contains("no faces"));
        let bad = text.replace("4 0 1 2 3", "3 0 1 9");
        assert!(load_mesh_bytes("ply", bad.as_bytes()).is_err());
    }

    #[test]
    fn off_meshes() {
        let text = "COFF\n# a square\n4 1 0\n0 0 0 255 0 0 255\n1 0 0 0 255 0 255\n\
            1 1 0 0 0 255 255\n0 1 0 9 9 9 255\n4 0 1 2 3\n";
        let m = load_mesh_bytes("off", text.as_bytes()).unwrap();
        assert_eq!(m.indices.len(), 6);
        assert!(load_mesh_bytes("off", b"OFF 3 1 0\n0 0 0\n").is_err());
        assert!(load_mesh_bytes("off", b"4OFF\n").is_err());
    }

    #[test]
    fn three_mf_items_and_components() {
        use std::io::Write;
        let model = r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
<!-- a triangle, and a component placing it 10 mm further along x -->
<resources>
<object id="1" type="model"><mesh><vertices>
<vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x='0' y='1' z='0'/>
</vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object>
<object id="2" type="model"><components>
<component objectid="1" transform="1 0 0 0 1 0 0 0 1 10 0 0"/>
</components></object>
</resources>
<build><item objectid="1"/><item objectid="2"/></build>
</model>"#;
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("_rels/.rels", opts).unwrap();
        zip.write_all(br#"<Relationships><Relationship Target="/3D/scan.model" Id="r" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/></Relationships>"#).unwrap();
        zip.start_file("3D/scan.model", opts).unwrap();
        zip.write_all(model.as_bytes()).unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        let m = load_mesh_bytes("3mf", &bytes).unwrap();
        assert_eq!(m.indices.len(), 6);
        // Two triangles 10 apart, fitted into the unit sphere.
        let (lo, hi) = m.bounds();
        assert!((hi.x - lo.x) > 1.8, "{lo} {hi}");
        assert!(load_mesh_bytes("3mf", b"not a zip").is_err());
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
