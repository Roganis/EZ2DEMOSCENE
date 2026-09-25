//! Procedural geometry for the built-in primitives.
//!
//! Faceted shapes are built polygon by polygon: every polygon is fanned from
//! its centre with an `edge` attribute of 1 at the centre and 0 on the
//! border, which lets the shader draw crisp neon outlines ("Edges" mode).

use bytemuck::{Pod, Zeroable};
use ez_core::rng::Rng;
use ez_core::Primitive;
use glam::{Quat, Vec2, Vec3};
use std::f32::consts::{PI, TAU};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable, PartialEq)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// Distance-to-outline parameter: 0 on polygon borders, 1 inside.
    pub edge: f32,
}

#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    fn push(&mut self, pos: Vec3, normal: Vec3, uv: Vec2, edge: f32) -> u32 {
        self.vertices.push(Vertex {
            pos: pos.into(),
            normal: normal.into(),
            uv: uv.into(),
            edge,
        });
        (self.vertices.len() - 1) as u32
    }

    /// Adds a planar convex polygon, fanned from its centroid.
    pub fn polygon(&mut self, pts: &[Vec3]) {
        if pts.len() < 3 {
            return;
        }
        // Newell normal.
        let mut n = Vec3::ZERO;
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            n += Vec3::new(
                (a.y - b.y) * (a.z + b.z),
                (a.z - b.z) * (a.x + b.x),
                (a.x - b.x) * (a.y + b.y),
            );
        }
        let n = n.normalize_or(Vec3::Y);
        let c = pts.iter().copied().sum::<Vec3>() / pts.len() as f32;
        let t = (pts[0] - c).normalize_or(Vec3::X);
        let b = n.cross(t);
        let local: Vec<Vec2> = pts
            .iter()
            .map(|p| Vec2::new((*p - c).dot(t), (*p - c).dot(b)))
            .collect();
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for l in &local {
            lo = lo.min(*l);
            hi = hi.max(*l);
        }
        let size = (hi - lo).max(Vec2::splat(1e-5));
        let uv = |l: Vec2| (l - lo) / size;
        let ci = self.push(c, n, uv(Vec2::ZERO), 1.0);
        let first = self.vertices.len() as u32;
        for (p, l) in pts.iter().zip(&local) {
            self.push(*p, n, uv(*l), 0.0);
        }
        let m = pts.len() as u32;
        for i in 0..m {
            self.indices
                .extend_from_slice(&[ci, first + i, first + (i + 1) % m]);
        }
    }

    /// Smooth quad grid (u wraps if `wrap_u`), used for spheres/tori/bands.
    fn grid(&mut self, nu: u32, nv: u32, f: impl Fn(f32, f32) -> (Vec3, Vec3)) {
        let base = self.vertices.len() as u32;
        for j in 0..=nv {
            for i in 0..=nu {
                let (u, v) = (i as f32 / nu as f32, j as f32 / nv as f32);
                let (p, n) = f(u, v);
                self.push(p, n, Vec2::new(u, v), 1.0);
            }
        }
        let row = nu + 1;
        for j in 0..nv {
            for i in 0..nu {
                let a = base + j * row + i;
                let b = a + 1;
                let c = a + row;
                let d = c + 1;
                self.indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
    }

    fn append(&mut self, other: &MeshData) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.indices.extend(other.indices.iter().map(|i| i + base));
    }

    fn transform(&mut self, rot: Quat, scale: Vec3, offset: Vec3) {
        for v in &mut self.vertices {
            let p = rot * (Vec3::from(v.pos) * scale) + offset;
            let n = (rot * (Vec3::from(v.normal) / scale)).normalize_or(Vec3::Y);
            v.pos = p.into();
            v.normal = n.into();
        }
    }

    /// Axis-aligned bounds.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for v in &self.vertices {
            lo = lo.min(v.pos.into());
            hi = hi.max(v.pos.into());
        }
        (lo, hi)
    }

    /// Centre the mesh and scale it to fit a unit-radius sphere.
    pub fn normalize_size(&mut self) {
        if self.vertices.is_empty() {
            return;
        }
        let (lo, hi) = self.bounds();
        let c = (lo + hi) * 0.5;
        let r = self
            .vertices
            .iter()
            .map(|v| (Vec3::from(v.pos) - c).length())
            .fold(0.0f32, f32::max)
            .max(1e-6);
        for v in &mut self.vertices {
            v.pos = ((Vec3::from(v.pos) - c) / r).into();
        }
    }
}

fn polyhedron(verts: &[Vec3], faces: &[Vec<usize>]) -> MeshData {
    let mut m = MeshData::default();
    for f in faces {
        let pts: Vec<Vec3> = f.iter().map(|&i| verts[i]).collect();
        m.polygon(&pts);
    }
    m
}

fn icosahedron_data() -> (Vec<Vec3>, Vec<[usize; 3]>) {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let v: Vec<Vec3> = [
        [-1.0, t, 0.0],
        [1.0, t, 0.0],
        [-1.0, -t, 0.0],
        [1.0, -t, 0.0],
        [0.0, -1.0, t],
        [0.0, 1.0, t],
        [0.0, -1.0, -t],
        [0.0, 1.0, -t],
        [t, 0.0, -1.0],
        [t, 0.0, 1.0],
        [-t, 0.0, -1.0],
        [-t, 0.0, 1.0],
    ]
    .iter()
    .map(|p| Vec3::from(*p).normalize())
    .collect();
    let f = vec![
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ];
    (v, f)
}

/// Dual of a triangulated polyhedron (icosahedron -> dodecahedron).
fn dual(verts: &[Vec3], faces: &[[usize; 3]]) -> (Vec<Vec3>, Vec<Vec<usize>>) {
    let centers: Vec<Vec3> = faces
        .iter()
        .map(|f| ((verts[f[0]] + verts[f[1]] + verts[f[2]]) / 3.0).normalize())
        .collect();
    let mut out_faces = Vec::new();
    for (vi, v) in verts.iter().enumerate() {
        let mut around: Vec<usize> = (0..faces.len())
            .filter(|&fi| faces[fi].contains(&vi))
            .collect();
        // Sort around the vertex normal.
        let n = v.normalize();
        let t = (centers[around[0]] - *v).normalize();
        let b = n.cross(t);
        around.sort_by(|&a, &c| {
            let pa = centers[a] - *v;
            let pc = centers[c] - *v;
            let aa = pa.dot(b).atan2(pa.dot(t));
            let ac = pc.dot(b).atan2(pc.dot(t));
            aa.partial_cmp(&ac).unwrap()
        });
        out_faces.push(around);
    }
    (centers, out_faces)
}

fn fix_winding(m: &mut MeshData) {
    // Make every triangle face away from the origin (all shapes here are
    // star-shaped around it), so outward normals are consistent.
    for tri in m.indices.chunks_mut(3) {
        let a = Vec3::from(m.vertices[tri[0] as usize].pos);
        let b = Vec3::from(m.vertices[tri[1] as usize].pos);
        let c = Vec3::from(m.vertices[tri[2] as usize].pos);
        let n = (b - a).cross(c - a);
        if n.dot(a + b + c) < 0.0 {
            tri.swap(1, 2);
        }
    }
    for v in &mut m.vertices {
        let p = Vec3::from(v.pos);
        let n = Vec3::from(v.normal);
        if n.dot(p) < 0.0 && p.length() > 1e-4 {
            v.normal = (-n).into();
        }
    }
}

fn sphere(detail: u32) -> MeshData {
    let (mut verts, mut faces) = icosahedron_data();
    for _ in 0..detail.min(5) {
        let mut cache = std::collections::HashMap::new();
        let mut mid = |a: usize, b: usize, verts: &mut Vec<Vec3>| -> usize {
            let key = (a.min(b), a.max(b));
            *cache.entry(key).or_insert_with(|| {
                verts.push(((verts[a] + verts[b]) * 0.5).normalize());
                verts.len() - 1
            })
        };
        let mut nf = Vec::with_capacity(faces.len() * 4);
        for f in &faces {
            let ab = mid(f[0], f[1], &mut verts);
            let bc = mid(f[1], f[2], &mut verts);
            let ca = mid(f[2], f[0], &mut verts);
            nf.push([f[0], ab, ca]);
            nf.push([f[1], bc, ab]);
            nf.push([f[2], ca, bc]);
            nf.push([ab, bc, ca]);
        }
        faces = nf;
    }
    let mut m = MeshData::default();
    for v in &verts {
        let u = 0.5 + v.z.atan2(v.x) / TAU;
        let w = 0.5 - v.y.asin() / PI;
        m.push(*v, *v, Vec2::new(u, w), 1.0);
    }
    for f in faces {
        m.indices.extend(f.iter().map(|&i| i as u32));
    }
    fix_winding(&mut m);
    m
}

fn torus(thickness: f32, segments: u32) -> MeshData {
    let mut m = MeshData::default();
    let r = thickness.clamp(0.02, 0.9);
    let seg = segments.clamp(6, 256);
    m.grid(seg, (seg / 2).max(6), |u, v| {
        let a = u * TAU;
        let b = v * TAU;
        let ring = Vec3::new(a.cos(), 0.0, a.sin());
        let n = ring * b.cos() + Vec3::Y * b.sin();
        (ring * (1.0 - r) + n * r, n)
    });
    m
}

fn cylinder(segments: u32) -> MeshData {
    let n = segments.clamp(3, 128);
    let mut m = MeshData::default();
    let ring: Vec<Vec3> = (0..n)
        .map(|i| {
            let a = TAU * i as f32 / n as f32;
            Vec3::new(a.cos(), 0.0, a.sin())
        })
        .collect();
    let top: Vec<Vec3> = ring.iter().map(|p| *p + Vec3::Y * 0.5).collect();
    let bot: Vec<Vec3> = ring.iter().map(|p| *p - Vec3::Y * 0.5).collect();
    if n <= 16 {
        for i in 0..n as usize {
            let j = (i + 1) % n as usize;
            m.polygon(&[bot[i], bot[j], top[j], top[i]]);
        }
    } else {
        m.grid(n, 1, |u, v| {
            let a = u * TAU;
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            (d + Vec3::Y * (v - 0.5), d)
        });
    }
    m.polygon(&top.iter().rev().copied().collect::<Vec<_>>());
    m.polygon(&bot);
    fix_winding(&mut m);
    m
}

fn pyramid_like(base: &[Vec3], apex: Vec3) -> MeshData {
    let mut m = MeshData::default();
    m.polygon(base);
    for i in 0..base.len() {
        let j = (i + 1) % base.len();
        m.polygon(&[base[i], base[j], apex]);
    }
    m
}

fn fix_winding_about(m: &mut MeshData, center: Vec3) {
    for v in &mut m.vertices {
        v.pos = (Vec3::from(v.pos) - center).into();
    }
    fix_winding(m);
    for v in &mut m.vertices {
        v.pos = (Vec3::from(v.pos) + center).into();
    }
}

fn shard(rng: &mut Rng) -> MeshData {
    let sides = rng.range_u32(4, 6);
    let base: Vec<Vec3> = (0..sides)
        .map(|i| {
            let a = TAU * (i as f32 + rng.range(-0.25, 0.25)) / sides as f32;
            let r = rng.range(0.15, 0.3);
            Vec3::new(a.cos() * r, 0.0, a.sin() * r)
        })
        .collect();
    let apex = Vec3::new(rng.range(-0.1, 0.1), 1.0, rng.range(-0.1, 0.1));
    let mut m = pyramid_like(&base, apex);
    fix_winding_about(&mut m, Vec3::new(0.0, 0.3, 0.0));
    m
}

fn crystal(spikes: u32, seed: u32) -> MeshData {
    let mut rng = Rng::new(seed as u64 * 31 + 5);
    let mut m = MeshData::default();
    for i in 0..spikes.clamp(1, 32) {
        let mut s = shard(&mut rng);
        let h = if i == 0 { 1.0 } else { rng.range(0.35, 0.85) };
        let tilt = if i == 0 { 0.0 } else { rng.range(0.2, 0.7) };
        let dir = rng.f32() * TAU;
        let axis = Vec3::new(dir.cos(), 0.0, dir.sin());
        let rot = Quat::from_axis_angle(axis, tilt);
        let off = if i == 0 {
            Vec3::ZERO
        } else {
            Vec3::new(-dir.sin(), 0.0, dir.cos()) * rng.range(0.05, 0.25)
        };
        s.transform(rot, Vec3::new(1.0, h, 1.0), off);
        m.append(&s);
    }
    m
}

fn panel(bevel: f32) -> MeshData {
    let b = bevel.clamp(0.0, 0.45);
    let (w, h, d) = (0.5, 0.5, 0.5);
    let bot = [
        Vec3::new(-w, -h, -d),
        Vec3::new(w, -h, -d),
        Vec3::new(w, -h, d),
        Vec3::new(-w, -h, d),
    ];
    let top = [
        Vec3::new(-w + b, h, -d + b),
        Vec3::new(w - b, h, -d + b),
        Vec3::new(w - b, h, d - b),
        Vec3::new(-w + b, h, d - b),
    ];
    let mut m = MeshData::default();
    m.polygon(&bot);
    m.polygon(&[top[3], top[2], top[1], top[0]]);
    for i in 0..4 {
        let j = (i + 1) % 4;
        m.polygon(&[bot[i], bot[j], top[j], top[i]]);
    }
    fix_winding(&mut m);
    m
}

fn ring(arc: f32, width: f32, height: f32, segments: u32) -> MeshData {
    let arc = arc.clamp(1.0, 360.0).to_radians();
    let w = width.clamp(0.001, 1.0);
    let h = height.max(0.001) * 0.5;
    let seg = segments.clamp(2, 512);
    let full = arc >= TAU - 1e-3;
    let a0 = -arc * 0.5;
    let at = |u: f32| a0 + u * arc;
    let dir = |a: f32| Vec3::new(a.sin(), 0.0, a.cos());
    let (ro, ri) = (1.0, 1.0 - w);
    let mut m = MeshData::default();
    // outer, inner, top, bottom strips
    m.grid(seg, 1, |u, v| {
        let d = dir(at(u));
        (d * ro + Vec3::Y * (v * 2.0 - 1.0) * h, d)
    });
    m.grid(seg, 1, |u, v| {
        let d = dir(at(u));
        (d * ri + Vec3::Y * (h - v * 2.0 * h), -d)
    });
    m.grid(seg, 1, |u, v| {
        let d = dir(at(u));
        (d * (ro - v * w) + Vec3::Y * h, Vec3::Y)
    });
    m.grid(seg, 1, |u, v| {
        let d = dir(at(u));
        (d * (ri + v * w) - Vec3::Y * h, -Vec3::Y)
    });
    if !full {
        for &a in &[at(0.0), at(1.0)] {
            let d = dir(a);
            m.polygon(&[
                d * ri - Vec3::Y * h,
                d * ro - Vec3::Y * h,
                d * ro + Vec3::Y * h,
                d * ri + Vec3::Y * h,
            ]);
        }
    }
    // Strips were built with arbitrary winding: orient by stored normals.
    for tri in m.indices.chunks_mut(3) {
        let a = Vec3::from(m.vertices[tri[0] as usize].pos);
        let b = Vec3::from(m.vertices[tri[1] as usize].pos);
        let c = Vec3::from(m.vertices[tri[2] as usize].pos);
        let n = Vec3::from(m.vertices[tri[0] as usize].normal);
        if (b - a).cross(c - a).dot(n) < 0.0 {
            tri.swap(1, 2);
        }
    }
    m
}

/// Generate geometry for a primitive. Shapes fit roughly in a unit sphere.
pub fn primitive(p: &Primitive) -> MeshData {
    match p {
        Primitive::Cube => {
            let v: Vec<Vec3> = (0..8)
                .map(|i| {
                    Vec3::new(
                        if i & 1 == 0 { -0.5 } else { 0.5 },
                        if i & 2 == 0 { -0.5 } else { 0.5 },
                        if i & 4 == 0 { -0.5 } else { 0.5 },
                    )
                })
                .collect();
            let faces = vec![
                vec![0, 2, 3, 1],
                vec![4, 5, 7, 6],
                vec![0, 1, 5, 4],
                vec![2, 6, 7, 3],
                vec![0, 4, 6, 2],
                vec![1, 3, 7, 5],
            ];
            let mut m = polyhedron(&v, &faces);
            fix_winding(&mut m);
            m
        }
        Primitive::Tetrahedron => {
            let v = [
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(1.0, -1.0, -1.0),
                Vec3::new(-1.0, 1.0, -1.0),
                Vec3::new(-1.0, -1.0, 1.0),
            ]
            .map(|p| p.normalize());
            let mut m = polyhedron(
                &v,
                &[vec![0, 1, 2], vec![0, 3, 1], vec![0, 2, 3], vec![1, 3, 2]],
            );
            fix_winding(&mut m);
            m
        }
        Primitive::Octahedron => {
            let v = [
                Vec3::X,
                Vec3::NEG_X,
                Vec3::Y,
                Vec3::NEG_Y,
                Vec3::Z,
                Vec3::NEG_Z,
            ];
            let faces = vec![
                vec![0, 2, 4],
                vec![4, 2, 1],
                vec![1, 2, 5],
                vec![5, 2, 0],
                vec![0, 4, 3],
                vec![4, 1, 3],
                vec![1, 5, 3],
                vec![5, 0, 3],
            ];
            let mut m = polyhedron(&v, &faces);
            fix_winding(&mut m);
            m
        }
        Primitive::Icosahedron => {
            let (v, f) = icosahedron_data();
            let faces: Vec<Vec<usize>> = f.iter().map(|t| t.to_vec()).collect();
            let mut m = polyhedron(&v, &faces);
            fix_winding(&mut m);
            m
        }
        Primitive::Dodecahedron => {
            let (v, f) = icosahedron_data();
            let (dv, df) = dual(&v, &f);
            let mut m = polyhedron(&dv, &df);
            fix_winding(&mut m);
            m
        }
        Primitive::Sphere { detail } => sphere(*detail),
        Primitive::Torus {
            thickness,
            segments,
        } => torus(*thickness, *segments),
        Primitive::Cylinder { segments } => cylinder(*segments),
        Primitive::Shard { seed } => {
            let mut rng = Rng::new(*seed as u64 * 131 + 7);
            shard(&mut rng)
        }
        Primitive::Crystal { spikes, seed } => crystal(*spikes, *seed),
        Primitive::Panel { bevel } => panel(*bevel),
        Primitive::Ring {
            arc,
            width,
            height,
            segments,
        } => ring(*arc, *width, *height, *segments),
        Primitive::Plane => {
            let mut m = MeshData::default();
            m.polygon(&[
                Vec3::new(-0.5, 0.0, 0.5),
                Vec3::new(0.5, 0.0, 0.5),
                Vec3::new(0.5, 0.0, -0.5),
                Vec3::new(-0.5, 0.0, -0.5),
            ]);
            m
        }
        Primitive::Pyramid => {
            let base = [
                Vec3::new(-0.5, -0.5, -0.5),
                Vec3::new(0.5, -0.5, -0.5),
                Vec3::new(0.5, -0.5, 0.5),
                Vec3::new(-0.5, -0.5, 0.5),
            ];
            let mut m = pyramid_like(&base, Vec3::new(0.0, 0.5, 0.0));
            fix_winding_about(&mut m, Vec3::new(0.0, -0.2, 0.0));
            m
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_primitives_valid() {
        for p in Primitive::all_defaults() {
            let m = primitive(&p);
            assert!(!m.vertices.is_empty(), "{p:?}");
            assert_eq!(m.indices.len() % 3, 0);
            assert!(m.indices.iter().all(|&i| (i as usize) < m.vertices.len()));
            assert!(m
                .vertices
                .iter()
                .all(|v| v.pos.iter().all(|c| c.is_finite())));
        }
    }

    #[test]
    fn dodecahedron_has_12_pentagons() {
        let m = primitive(&Primitive::Dodecahedron);
        // 12 faces * (1 centre + 5 border) vertices
        assert_eq!(m.vertices.len(), 72);
        assert_eq!(m.indices.len(), 12 * 5 * 3);
    }
}
