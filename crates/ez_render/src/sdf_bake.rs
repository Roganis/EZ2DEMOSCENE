//! Signed distance fields of meshes, for morphing between shapes (see
//! `ShapeMorph`): a grid of distances to the surface, negative inside.
//!
//! 1. Cells near the surface get the exact distance to their nearest
//!    triangle, and remember that nearest point.
//! 2. Jump flooding passes those nearest points on to every other cell.
//! 3. Inside or outside: along each axis, a line through the cell crosses
//!    the surface an odd number of times before it if it is inside. The
//!    three axes vote, so a model with an open bottom or a small gap is
//!    still solid (only one axis sees through the hole). A model that
//!    encloses nothing at all (grass, a flag) becomes a thin shell.

use crate::mesh::MeshData;
use glam::{Vec2, Vec3};

/// The grid covers -EXTENT..EXTENT on each axis: a little more than the
/// unit sphere shapes are fitted into, so distances around them are right.
pub const EXTENT: f32 = 1.1;

/// Half thickness, in cells, of the shell around a model with no inside.
const SHELL: f32 = 0.75;

pub struct DistanceGrid {
    /// Cells per side.
    pub n: u32,
    /// Distances, `x + n * (y + n * z)`.
    pub d: Vec<f32>,
}

impl DistanceGrid {
    fn cell(&self, x: u32, y: u32, z: u32) -> f32 {
        let n = self.n;
        self.d[(x + n * (y + n * z)) as usize]
    }

    /// Trilinear distance at an object-space point (inside the grid).
    pub fn sample(&self, p: Vec3) -> f32 {
        let n = self.n as f32;
        let g = ((p / EXTENT) * 0.5 + 0.5) * n - 0.5;
        let g = g.clamp(Vec3::ZERO, Vec3::splat(n - 1.0));
        let i = g.floor();
        let f = g - i;
        let (x0, y0, z0) = (i.x as u32, i.y as u32, i.z as u32);
        let m = self.n - 1;
        let (x1, y1, z1) = ((x0 + 1).min(m), (y0 + 1).min(m), (z0 + 1).min(m));
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let c00 = lerp(self.cell(x0, y0, z0), self.cell(x1, y0, z0), f.x);
        let c10 = lerp(self.cell(x0, y1, z0), self.cell(x1, y1, z0), f.x);
        let c01 = lerp(self.cell(x0, y0, z1), self.cell(x1, y0, z1), f.x);
        let c11 = lerp(self.cell(x0, y1, z1), self.cell(x1, y1, z1), f.x);
        lerp(lerp(c00, c10, f.y), lerp(c01, c11, f.y), f.z)
    }
}

/// Closest point to `p` on triangle `abc` (Ericson, Real-Time Collision
/// Detection 5.1.5).
fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Bake `mesh` (fitted in the unit sphere) into an `n`³ grid.
pub fn bake(mesh: &MeshData, n: u32) -> DistanceGrid {
    let n = n.max(4);
    let nu = n as usize;
    let h = 2.0 * EXTENT / n as f32;
    let center = |i: usize| -EXTENT + (i as f32 + 0.5) * h;
    let idx = |x: usize, y: usize, z: usize| x + nu * (y + nu * z);
    let tris: Vec<[Vec3; 3]> = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| t.map(|i| Vec3::from(mesh.vertices[i as usize].pos)))
        .filter(|[a, b, c]| (*b - *a).cross(*c - *a).length_squared() > 1e-14)
        .collect();

    // 1. Exact nearest points in a band around each triangle.
    let mut nearest: Vec<Option<Vec3>> = vec![None; nu * nu * nu];
    let mut best = vec![f32::INFINITY; nu * nu * nu];
    let to_cell = |v: f32| ((v + EXTENT) / h - 0.5).floor() as i64;
    let band = 1;
    for [a, b, c] in &tris {
        let lo = a.min(*b).min(*c);
        let hi = a.max(*b).max(*c);
        let range = |l: f32, u: f32| {
            let s = (to_cell(l) - band).clamp(0, nu as i64 - 1) as usize;
            let e = (to_cell(u) + 1 + band).clamp(0, nu as i64 - 1) as usize;
            s..=e
        };
        for z in range(lo.z, hi.z) {
            for y in range(lo.y, hi.y) {
                for x in range(lo.x, hi.x) {
                    let p = Vec3::new(center(x), center(y), center(z));
                    let q = closest_on_triangle(p, *a, *b, *c);
                    let d = p.distance_squared(q);
                    let i = idx(x, y, z);
                    if d < best[i] {
                        best[i] = d;
                        nearest[i] = Some(q);
                    }
                }
            }
        }
    }

    // 2. Jump flooding: every cell looks at its neighbours' nearest points
    // at shrinking distances.
    let mut step = (nu / 2).max(1);
    loop {
        let s = step as i64;
        for z in 0..nu {
            for y in 0..nu {
                for x in 0..nu {
                    let p = Vec3::new(center(x), center(y), center(z));
                    let i = idx(x, y, z);
                    for dz in [-s, 0, s] {
                        for dy in [-s, 0, s] {
                            for dx in [-s, 0, s] {
                                let (qx, qy, qz) = (x as i64 + dx, y as i64 + dy, z as i64 + dz);
                                if qx < 0
                                    || qy < 0
                                    || qz < 0
                                    || qx >= nu as i64
                                    || qy >= nu as i64
                                    || qz >= nu as i64
                                {
                                    continue;
                                }
                                if let Some(q) = nearest[idx(qx as usize, qy as usize, qz as usize)]
                                {
                                    let d = p.distance_squared(q);
                                    if d < best[i] {
                                        best[i] = d;
                                        nearest[i] = Some(q);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if step == 1 {
            break;
        }
        step /= 2;
    }

    // 3. Inside or outside: crossings along each axis, then a vote.
    let mut votes = vec![0u8; nu * nu * nu];
    for axis in 0..3 {
        // Along `axis`; the other two span the columns.
        let (u_ax, v_ax) = match axis {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let mut hits: Vec<Vec<f32>> = vec![Vec::new(); nu * nu];
        // Columns sit a hair off the cell centres so rays don't run exactly
        // along the edges of axis-aligned faces.
        let jitter = Vec2::new(0.013, 0.007) * h;
        for [a, b, c] in &tris {
            let p2 = |v: &Vec3| Vec2::new(v[u_ax], v[v_ax]);
            let (a2, b2, c2) = (p2(a), p2(b), p2(c));
            let area = (b2 - a2).perp_dot(c2 - a2);
            if area.abs() < 1e-12 {
                continue;
            }
            let lo = a2.min(b2).min(c2);
            let hi = a2.max(b2).max(c2);
            let span = |l: f32, u: f32| {
                let s = to_cell(l).clamp(0, nu as i64 - 1) as usize;
                let e = (to_cell(u) + 1).clamp(0, nu as i64 - 1) as usize;
                s..=e
            };
            for j in span(lo.y, hi.y) {
                for i in span(lo.x, hi.x) {
                    let q = Vec2::new(center(i), center(j)) + jitter;
                    // Barycentric coordinates in the projection.
                    let w0 = (b2 - q).perp_dot(c2 - q) / area;
                    let w1 = (c2 - q).perp_dot(a2 - q) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    hits[i + nu * j].push(a[axis] * w0 + b[axis] * w1 + c[axis] * w2);
                }
            }
        }
        for j in 0..nu {
            for i in 0..nu {
                let col = &mut hits[i + nu * j];
                col.sort_by(f32::total_cmp);
                let mut k = 0;
                for t in 0..nu {
                    let pos = center(t);
                    while k < col.len() && col[k] < pos {
                        k += 1;
                    }
                    // Odd crossings before, and at least one after.
                    if k % 2 == 1 && k < col.len() {
                        let cell = match axis {
                            0 => idx(t, i, j),
                            1 => idx(i, t, j),
                            _ => idx(i, j, t),
                        };
                        votes[cell] += 1;
                    }
                }
            }
        }
    }

    // Open or flat models (grass, flags, rooms…) enclose nothing: they
    // become a thin solid shell around their surface instead.
    let inside = votes.iter().filter(|v| **v >= 2).count();
    let shell = inside * 500 < votes.len();
    let d = best
        .iter()
        .zip(&votes)
        .map(|(b, v)| {
            let u = b.sqrt().min(2.0 * EXTENT);
            if shell {
                u - SHELL * h
            } else if *v >= 2 {
                -u
            } else {
                u
            }
        })
        .collect();
    DistanceGrid { n, d }
}

/// Two grids of the same size as one RGBA float image: slice `z` is the
/// tile at column `z % cols`, row `z / cols`; red holds `a`, green `b`.
/// Returns (width, height, columns, pixels).
pub fn pack_pair(a: &DistanceGrid, b: &DistanceGrid) -> (u32, u32, u32, Vec<[f32; 4]>) {
    assert_eq!(a.n, b.n);
    let n = a.n;
    let cols = (n as f32).sqrt().ceil() as u32;
    let rows = n.div_ceil(cols);
    let (w, h) = (cols * n, rows * n);
    let mut px = vec![[0.0, 0.0, 0.0, 1.0]; (w * h) as usize];
    for z in 0..n {
        let (tx, ty) = (z % cols, z / cols);
        for y in 0..n {
            for x in 0..n {
                let src = (x + n * (y + n * z)) as usize;
                let dst = ((ty * n + y) * w + tx * n + x) as usize;
                px[dst] = [a.d[src], b.d[src], 0.0, 1.0];
            }
        }
    }
    (w, h, cols, px)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::primitive;
    use ez_core::Primitive;

    #[test]
    fn sphere_distances() {
        let mut m = primitive(&Primitive::Sphere { detail: 3 });
        m.normalize_size();
        let g = bake(&m, 40);
        let h = 2.0 * EXTENT / 40.0;
        assert!(
            (g.sample(Vec3::ZERO) + 1.0).abs() < 2.0 * h,
            "{}",
            g.sample(Vec3::ZERO)
        );
        assert!(g.sample(Vec3::new(0.0, 0.5, 0.0)) < -0.4);
        assert!((g.sample(Vec3::new(1.05, 0.0, 0.0)) - 0.05).abs() < 2.0 * h);
        assert!(g.sample(Vec3::new(0.7, 0.7, 0.0)).abs() < 2.0 * h);
    }

    #[test]
    fn open_box_is_still_solid() {
        // A cube with its bottom face removed: only the vertical axis sees
        // through, the other two vote inside.
        let mut m = primitive(&Primitive::Cube);
        m.normalize_size();
        let tris: Vec<[u32; 3]> = m.indices.as_chunks::<3>().0.to_vec();
        m.indices = tris
            .into_iter()
            .filter(|t| !t.iter().all(|i| m.vertices[*i as usize].pos[1] < -0.3))
            .flatten()
            .collect();
        let g = bake(&m, 32);
        assert!(g.sample(Vec3::ZERO) < 0.0);
        assert!(g.sample(Vec3::new(0.0, 0.0, 1.05)) > 0.0);
    }

    #[test]
    fn pack_keeps_slices() {
        let mut m = primitive(&Primitive::Sphere { detail: 2 });
        m.normalize_size();
        let a = bake(&m, 9);
        let (w, h, cols, px) = pack_pair(&a, &a);
        assert_eq!((w, h, cols), (27, 27, 3));
        // Centre cell: slice 4 is tile (1, 1).
        let v = px[((9 + 4) * w + 9 + 4) as usize];
        assert_eq!(v[0], a.d[(4 + 9 * (4 + 9 * 4)) as usize]);
    }
}
