//! Frame evaluation: camera, layer transforms, symmetry and instancing.
//! Pure CPU math shared by the renderer and the tests.

use crate::clock::EvalCtx;
use crate::rng::{hash2, hash_u32, Rng};
use crate::scene::*;
use glam::{EulerRot, Mat4, Quat, Vec3};
use std::f32::consts::TAU;

/// Evaluated camera for one frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraState {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    /// Vertical field of view in radians.
    pub fov_y: f32,
}

impl CameraState {
    pub fn view(&self) -> Mat4 {
        Mat4::look_at_rh(self.eye, self.target, self.up)
    }

    pub fn proj(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(self.fov_y, aspect.max(1e-3), 0.05, 500.0)
    }
}

/// Closed uniform Catmull-Rom spline through `p` at parameter `u`
/// (0..len, wrapping).
fn catmull<T>(p: &[T], u: f32) -> T
where
    T: Copy
        + std::ops::Add<Output = T>
        + std::ops::Sub<Output = T>
        + std::ops::Mul<f32, Output = T>,
{
    let n = p.len();
    let u = u.rem_euclid(n as f32);
    let i = (u.floor() as usize).min(n - 1);
    let t = u - i as f32;
    let at = |k: isize| p[(i as isize + k).rem_euclid(n as isize) as usize];
    let (p0, p1, p2, p3) = (at(-1), at(0), at(1), at(2));
    let (t2, t3) = (t * t, t * t * t);
    (p1 * 2.0
        + (p2 - p0) * t
        + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * t2
        + (p1 * 3.0 - p0 - p2 * 3.0 + p3) * t3)
        * 0.5
}

impl CameraPath {
    /// Spline parameter (0..points) for a position `s` (0..1) along the
    /// path, at an even speed.
    fn param_at(&self, s: f32) -> f32 {
        let eyes: Vec<Vec3> = self.points.iter().map(|p| Vec3::from(p.eye)).collect();
        let n = eyes.len();
        const STEPS: usize = 48;
        let mut lengths = Vec::with_capacity(n * STEPS + 1);
        let mut total = 0.0;
        let mut prev = catmull(&eyes, 0.0);
        lengths.push(0.0);
        for k in 1..=n * STEPS {
            let q = catmull(&eyes, k as f32 / STEPS as f32);
            total += (q - prev).length();
            lengths.push(total);
            prev = q;
        }
        if total <= 1e-6 {
            return s * n as f32;
        }
        let want = s.rem_euclid(1.0) * total;
        let k = lengths.partition_point(|l| *l < want).clamp(1, n * STEPS);
        let (a, b) = (lengths[k - 1], lengths[k]);
        let f = if b > a { (want - a) / (b - a) } else { 0.0 };
        (k as f32 - 1.0 + f) / STEPS as f32
    }

    /// `n` points along the whole flight line (for drawing it).
    pub fn line(&self, n: usize) -> Vec<Vec3> {
        let eyes: Vec<Vec3> = self.points.iter().map(|p| Vec3::from(p.eye)).collect();
        if eyes.is_empty() {
            return Vec::new();
        }
        (0..=n)
            .map(|k| catmull(&eyes, k as f32 / n as f32 * eyes.len() as f32))
            .collect()
    }

    /// Eye, target, roll (degrees) and FOV (degrees) at the moment.
    pub fn eval(&self, ctx: &EvalCtx) -> Option<(Vec3, Vec3, f32, f32)> {
        let n = self.points.len();
        if n == 0 {
            return None;
        }
        let u = match (self.cut_on, ctx.music.active) {
            (Some(kind), true) => {
                // Hard cuts: one point per hit in the loop, drifting towards
                // the next during the beat after the cut.
                let h = &ctx.music.hits[kind as usize];
                let beat = ctx.beat_seconds.max(1e-3);
                (h.count as usize % n) as f32 + (h.since / beat).clamp(0.0, 1.0) * self.drift
            }
            _ => {
                let s = (ctx.phase * self.laps.max(1) as f32).rem_euclid(1.0);
                let u = self.param_at(s);
                // Linger at the points.
                let (k, f) = (u.floor(), u - u.floor());
                let eased = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
                k + f + (eased - f) * self.ease.clamp(0.0, 1.0)
            }
        };
        let get = |f: fn(&PathPoint) -> Vec3| -> Vec3 {
            let v: Vec<Vec3> = self.points.iter().map(f).collect();
            catmull(&v, u)
        };
        let eye = get(|p| Vec3::from(p.eye));
        let target = get(|p| Vec3::from(p.target));
        let extra = get(|p| Vec3::new(p.roll, p.fov, 0.0));
        Some((eye, target, extra.x, extra.y))
    }
}

impl Camera {
    pub fn eval(&self, ctx: &EvalCtx) -> CameraState {
        let mut state = self.eval_motion(ctx);
        // Punch in on hits.
        if self.punch > 0.0 && ctx.music.active {
            let h = &ctx.music.hits[self.punch_on as usize];
            let k = (-h.since * 8.0).exp() * h.strength.clamp(0.0, 1.0) * self.punch.min(1.0);
            state.fov_y *= 1.0 - 0.35 * k;
        }
        state
    }

    /// The shot at the moment as a path point (before any punch-in).
    pub fn view_point(&self, ctx: &EvalCtx) -> PathPoint {
        let st = self.eval_motion(ctx);
        let roll = match (self.mode, self.path.eval(ctx)) {
            (CameraMode::Path, Some((_, _, roll, _))) => roll,
            _ => self.roll.eval(ctx),
        };
        PathPoint {
            eye: st.eye.into(),
            target: st.target.into(),
            roll,
            fov: st.fov_y.to_degrees(),
        }
    }

    /// Make this a Static camera framing `p` (to adjust it in the
    /// viewport).
    pub fn look_from(&mut self, p: &PathPoint) {
        let d = Vec3::from(p.eye) - Vec3::from(p.target);
        self.mode = CameraMode::Static;
        self.target = p.target;
        self.distance = crate::Param::new(d.x.hypot(d.z).max(0.1));
        self.height = crate::Param::new(d.y);
        self.angle = crate::Param::new(d.x.atan2(d.z).to_degrees());
        self.fov = crate::Param::new(p.fov);
        self.roll = crate::Param::new(p.roll);
    }

    fn eval_motion(&self, ctx: &EvalCtx) -> CameraState {
        if self.mode == CameraMode::Path {
            if let Some((eye, target, roll, fov)) = self.path.eval(ctx) {
                let fwd = (target - eye).normalize_or(Vec3::NEG_Z);
                let up = Quat::from_axis_angle(fwd, roll.to_radians()) * Vec3::Y;
                return CameraState {
                    eye,
                    target,
                    up,
                    fov_y: fov.clamp(5.0, 170.0).to_radians(),
                };
            }
        }
        let base = self.angle.eval(ctx).to_radians();
        let az = match self.mode {
            CameraMode::Orbit => base + ctx.turns(self.orbit_turns as f32),
            CameraMode::Pendulum => base + self.swing.eval(ctx).to_radians() * ctx.turns(1.0).sin(),
            CameraMode::Static | CameraMode::Path => base,
        };
        let d = self.distance.eval(ctx).max(0.1);
        let h = self.height.eval(ctx);
        let target = Vec3::from(self.target);
        let mut eye = target + Vec3::new(az.sin() * d, h, az.cos() * d);
        let shake = self.beat_shake.eval(ctx);
        if shake > 0.0 {
            let beat = (ctx.beat().floor() as u32) % ctx.loop_beats.max(1);
            let k = shake * ctx.beat_pulse(6.0) * 0.1;
            let dir = Vec3::new(
                hash2(beat, 1) - 0.5,
                hash2(beat, 2) - 0.5,
                hash2(beat, 3) - 0.5,
            );
            eye += dir * k * d;
        }
        let fwd = (target - eye).normalize_or(Vec3::NEG_Z);
        let roll = self.roll.eval(ctx).to_radians();
        let up = Quat::from_axis_angle(fwd, roll) * Vec3::Y;
        CameraState {
            eye,
            target,
            up,
            fov_y: self.fov.eval(ctx).clamp(5.0, 170.0).to_radians(),
        }
    }
}

/// Position + rotation of a layer (no scale).
pub fn layer_frame(t: &Transform, ctx: &EvalCtx) -> Mat4 {
    let r = t.rotation.map(f32::to_radians);
    let base = Quat::from_euler(EulerRot::YXZ, r[1], r[0], r[2]);
    let spin = Quat::from_euler(
        EulerRot::YXZ,
        ctx.turns(t.spin[1] as f32),
        ctx.turns(t.spin[0] as f32),
        ctx.turns(t.spin[2] as f32),
    );
    let mut pos = Vec3::from(t.position) + Vec3::Y * t.bob.eval(ctx);
    let mut rot = base * spin;
    if t.shake.is_active() {
        let (offset, turn) = shake_offset(&t.shake, ctx);
        pos += offset;
        rot = turn * rot;
    }
    Mat4::from_rotation_translation(rot, pos)
}

/// Offset and extra rotation of a shake at this moment.
pub fn shake_offset(s: &Shake, ctx: &EvalCtx) -> (Vec3, Quat) {
    let n = s.per_loop.max(1);
    // Jolts keep to the real beat, even with time warp.
    let step = ((ctx.beat_phase.rem_euclid(1.0) * n as f32).floor() as u32).min(n - 1);
    let key = step.wrapping_mul(0x9e37_79b9) ^ s.seed.wrapping_mul(0x85eb_ca6b);
    let r = |k: u32| hash2(key, k) * 2.0 - 1.0;
    let dir = Vec3::new(r(11), r(12), r(13));
    let offset = dir * s.amount.eval(ctx);
    let deg = s.turn.eval(ctx);
    let turn = if deg != 0.0 {
        let axis = Vec3::new(r(21), r(22), r(23)).normalize_or(Vec3::Y);
        Quat::from_axis_angle(axis, (deg * r(24)).to_radians())
    } else {
        Quat::IDENTITY
    };
    (offset, turn)
}

/// Per-axis scale of a layer.
pub fn layer_scale(t: &Transform, ctx: &EvalCtx) -> Vec3 {
    Vec3::from(t.stretch) * t.scale.eval(ctx)
}

/// Full model matrix of a layer (frame * scale), used for particles.
pub fn layer_matrix(t: &Transform, ctx: &EvalCtx) -> Mat4 {
    layer_frame(t, ctx) * Mat4::from_scale(layer_scale(t, ctx))
}

/// World-space copies for a symmetry mode.
pub fn symmetry_matrices(sym: &Symmetry) -> Vec<Mat4> {
    let mx = Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0));
    let mz = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
    match *sym {
        Symmetry::None => vec![Mat4::IDENTITY],
        Symmetry::MirrorX => vec![Mat4::IDENTITY, mx],
        Symmetry::MirrorZ => vec![Mat4::IDENTITY, mz],
        Symmetry::MirrorXZ => vec![Mat4::IDENTITY, mx, mz, mx * mz],
        Symmetry::Radial { count } => {
            let n = count.clamp(1, 64);
            (0..n)
                .map(|k| Mat4::from_rotation_y(TAU * k as f32 / n as f32))
                .collect()
        }
        Symmetry::Kaleido { count } => {
            let n = count.clamp(1, 64);
            let w = TAU / n as f32;
            (0..n)
                .map(|k| {
                    if k % 2 == 0 {
                        Mat4::from_rotation_y(w * k as f32)
                    } else {
                        Mat4::from_rotation_y(w * (k + 1) as f32) * mx
                    }
                })
                .collect()
        }
    }
}

/// One evaluated mesh instance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instance {
    pub model: Mat4,
    /// Hue rotation in turns.
    pub hue: f32,
    /// Emissive multiplier.
    pub glow: f32,
    /// Stable random value (0..1) for shader variation.
    pub rand: f32,
    /// Position among the copies (0 for the first, towards 1 for the last).
    pub along: f32,
}

/// A point on a shape's surface with the surface's normal there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfacePoint {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
}

/// `count` points spread over the triangles (`indices` into `positions`)
/// in proportion to their area, the same for the same `seed`.
pub fn sample_surface(
    positions: &[[f32; 3]],
    indices: &[u32],
    count: u32,
    seed: u32,
) -> Vec<SurfacePoint> {
    let tris: Vec<[Vec3; 3]> = indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|t| {
            let p = |i: u32| positions.get(i as usize).map(|v| Vec3::from(*v));
            Some([p(t[0])?, p(t[1])?, p(t[2])?])
        })
        .collect();
    let mut cumulative = Vec::with_capacity(tris.len());
    let mut total = 0.0f32;
    for [a, b, c] in &tris {
        total += (*b - *a).cross(*c - *a).length() * 0.5;
        cumulative.push(total);
    }
    if tris.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    let mut rng = Rng::new(seed as u64 * 48_271 + 11);
    (0..count.min(20_000))
        .map(|_| {
            let x = rng.f32() * total;
            let k = cumulative.partition_point(|c| *c < x).min(tris.len() - 1);
            let [a, b, c] = tris[k];
            // Uniform point in the triangle.
            let (mut u, mut v) = (rng.f32(), rng.f32());
            if u + v > 1.0 {
                u = 1.0 - u;
                v = 1.0 - v;
            }
            let pos = a + (b - a) * u + (c - a) * v;
            let normal = (b - a).cross(c - a).normalize_or(Vec3::Y);
            SurfacePoint {
                pos: pos.into(),
                normal: normal.into(),
            }
        })
        .collect()
}

/// Local placement of each instance inside the layer. `surface` are the
/// points for [`Instancer::Surface`] (no copies without them).
fn instancer_locals(
    inst: &Instancer,
    ctx: &EvalCtx,
    surface: Option<&[SurfacePoint]>,
) -> Vec<Mat4> {
    match *inst {
        Instancer::Single => vec![Mat4::IDENTITY],
        Instancer::Grid { counts, spacing } => {
            let c = counts.map(|v| v.clamp(1, 64));
            let mut out = Vec::with_capacity((c[0] * c[1] * c[2]) as usize);
            for iy in 0..c[1] {
                for iz in 0..c[2] {
                    for ix in 0..c[0] {
                        let p = Vec3::new(
                            (ix as f32 - (c[0] - 1) as f32 * 0.5) * spacing[0],
                            (iy as f32 - (c[1] - 1) as f32 * 0.5) * spacing[1],
                            (iz as f32 - (c[2] - 1) as f32 * 0.5) * spacing[2],
                        );
                        out.push(Mat4::from_translation(p));
                    }
                }
            }
            out
        }
        Instancer::Radial { count, radius } => {
            let n = count.clamp(1, 1024);
            (0..n)
                .map(|k| {
                    Mat4::from_rotation_y(TAU * k as f32 / n as f32)
                        * Mat4::from_translation(Vec3::new(0.0, 0.0, radius))
                })
                .collect()
        }
        Instancer::Scatter {
            count,
            radius,
            shell,
            seed,
        } => {
            let mut rng = Rng::new(seed as u64 * 7919 + 17);
            (0..count.min(20_000))
                .map(|_| {
                    let dir = random_dir(&mut rng);
                    let r = if shell {
                        radius
                    } else {
                        radius * rng.f32().cbrt()
                    };
                    Mat4::from_translation(dir * r)
                })
                .collect()
        }
        Instancer::Orbit {
            count,
            radius,
            spread,
            speed,
            seed,
        } => {
            let mut rng = Rng::new(seed as u64 * 104_729 + 3);
            (0..count.min(20_000))
                .map(|_| {
                    let tilt = Quat::from_axis_angle(random_dir(&mut rng), rng.range(0.0, 0.6));
                    let r = (radius + spread * rng.signed()).max(0.0);
                    let y = spread * 0.5 * rng.signed();
                    let fast = if rng.chance(0.3) { 2 } else { 1 };
                    let start = rng.f32();
                    let a = TAU * (start + ctx.phase * (speed * fast) as f32);
                    let axis = random_dir(&mut rng);
                    let tumble_turns = if rng.chance(0.5) { 1.0 } else { -1.0 };
                    let tumble = Quat::from_axis_angle(axis, ctx.turns(tumble_turns));
                    let pos = tilt * Vec3::new(a.cos() * r, y, a.sin() * r);
                    Mat4::from_rotation_translation(tumble, pos)
                })
                .collect()
        }
        Instancer::Wall {
            cols,
            rows,
            spacing,
            curve,
        } => {
            let (cols, rows) = (cols.clamp(1, 128), rows.clamp(1, 128));
            let arc = curve.to_radians();
            let width = (cols.max(2) - 1) as f32 * spacing;
            let mut out = Vec::with_capacity((cols * rows) as usize);
            for r in 0..rows {
                for c in 0..cols {
                    let u = if cols > 1 {
                        c as f32 / (cols - 1) as f32 - 0.5
                    } else {
                        0.0
                    };
                    let y = r as f32 * spacing;
                    let m = if arc.abs() < 1e-3 {
                        Mat4::from_translation(Vec3::new(u * width, y, 0.0))
                    } else {
                        let rad = width / arc;
                        let a = u * arc;
                        Mat4::from_translation(Vec3::new(rad * a.sin(), y, rad * (1.0 - a.cos())))
                            * Mat4::from_rotation_y(-a)
                    };
                    out.push(m);
                }
            }
            out
        }
        Instancer::Spiral {
            count,
            radius,
            height,
            turns,
        } => {
            let n = count.clamp(1, 4096);
            (0..n)
                .map(|k| {
                    let t = k as f32 / n as f32;
                    let a = t * turns * TAU;
                    Mat4::from_translation(Vec3::new(
                        a.cos() * radius,
                        t * height - height * 0.5,
                        a.sin() * radius,
                    )) * Mat4::from_rotation_y(-a)
                })
                .collect()
        }
        Instancer::OnTerrain {
            count,
            seed,
            align,
            lift,
            ref ground,
            ..
        } => {
            let Some(ground) = ground else {
                return Vec::new();
            };
            let (terrain, placement) = &**ground;
            let g = crate::terrain::Ground::at(terrain, ctx);
            let full = layer_matrix(placement, ctx);
            let frame = layer_frame(placement, ctx);
            let (_, base_rot, _) = frame.to_scale_rotation_translation();
            let up = base_rot * Vec3::Y;
            let mut rng = Rng::new(seed as u64 * 15_485_863 + 5);
            (0..count.min(20_000))
                .map(|_| {
                    let (u, w) = (rng.f32(), rng.f32());
                    let (p, n, fade) = g.point(u, w);
                    let n = full.transform_vector3(n).normalize_or(up);
                    let rot = if align {
                        Quat::from_rotation_arc(up, n) * base_rot
                    } else {
                        base_rot
                    };
                    // Copies shrink away at the edges, where the landscape
                    // wraps around.
                    Mat4::from_scale_rotation_translation(
                        Vec3::splat(fade.max(1e-3)),
                        rot,
                        full.transform_point3(p) + n * lift,
                    )
                })
                .collect()
        }
        Instancer::Surface {
            size, align, lift, ..
        } => surface
            .unwrap_or(&[])
            .iter()
            .map(|sp| {
                let n = Vec3::from(sp.normal);
                let p = Vec3::from(sp.pos) * size + n * lift;
                if align {
                    // Copies stand up along the surface normal.
                    Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::Y, n), p)
                } else {
                    Mat4::from_translation(p)
                }
            })
            .collect(),
        Instancer::Curve {
            curve,
            freq,
            size,
            count,
            laps,
            align,
        } => {
            let n = count.clamp(1, 4096);
            let at = |t: f32| Vec3::from(curve.point(freq, t.rem_euclid(1.0))) * size;
            (0..n)
                .map(|k| {
                    let t = k as f32 / n as f32 + (ctx.phase * laps as f32).rem_euclid(1.0);
                    let p = at(t);
                    if !align {
                        return Mat4::from_translation(p);
                    }
                    // Face along the curve (+z forward), up as close to +y
                    // as the curve allows.
                    let fwd = (at(t + 1e-3) - at(t - 1e-3)).normalize_or(Vec3::Z);
                    let side = Vec3::Y.cross(fwd).normalize_or(Vec3::X);
                    let up = fwd.cross(side);
                    Mat4::from_cols(
                        side.extend(0.0),
                        up.extend(0.0),
                        fwd.extend(0.0),
                        p.extend(1.0),
                    )
                })
                .collect()
        }
    }
}

fn random_dir(rng: &mut Rng) -> Vec3 {
    let z = rng.signed();
    let a = rng.f32() * TAU;
    let r = (1.0 - z * z).max(0.0).sqrt();
    Vec3::new(r * a.cos(), z, r * a.sin())
}

/// True when a mesh layer's instances do not change over the loop, so the
/// renderer can compute them once and reuse them every frame.
pub fn instances_are_static(layer: &Layer, mesh: &MeshLayer) -> bool {
    let t = &layer.transform;
    let v = &mesh.variation;
    t.spin == [0; 3]
        && !t.shake.is_active()
        && !t.scale.is_animated()
        && !t.bob.is_animated()
        && !matches!(
            mesh.instancer,
            Instancer::Orbit { .. } | Instancer::OnTerrain { .. }
        )
        && !matches!(mesh.instancer, Instancer::Curve { laps, .. } if laps != 0)
        && v.spin == 0
        && v.ripple == 0.0
        && v.chase == 0.0
        && v.spectrum == 0.0
}

/// All instances of a mesh layer in world space.
///
/// The layer scale sizes each copy; instancer distances (radius, spacing)
/// are in world units and are not affected by it.
pub fn mesh_instances(layer: &Layer, mesh: &MeshLayer, ctx: &EvalCtx, out: &mut Vec<Instance>) {
    mesh_instances_with(layer, mesh, ctx, None, out)
}

/// [`mesh_instances`] with the surface points an [`Instancer::Surface`]
/// needs (see [`sample_surface`]).
pub fn mesh_instances_with(
    layer: &Layer,
    mesh: &MeshLayer,
    ctx: &EvalCtx,
    surface: Option<&[SurfacePoint]>,
    out: &mut Vec<Instance>,
) {
    // Copies on a terrain are placed in the world already.
    let l = if matches!(mesh.instancer, Instancer::OnTerrain { .. }) {
        Mat4::IDENTITY
    } else {
        layer_frame(&layer.transform, ctx)
    };
    let size = Mat4::from_scale(layer_scale(&layer.transform, ctx));
    let syms = symmetry_matrices(&layer.symmetry);
    let locals = instancer_locals(&mesh.instancer, ctx, surface);
    let v = &mesh.variation;
    let n = locals.len().max(1) as f32;
    for (i, local) in locals.iter().enumerate() {
        let i32_ = i as u32;
        let seed = hash_u32(v.seed.wrapping_mul(0x9e37_79b9) ^ i32_);
        let r = |k: u32| hash2(seed, k);
        let frac = i as f32 / n;
        let mut var = Mat4::IDENTITY;
        if v.rotation != 0.0 {
            let a = v.rotation.to_radians();
            var *= Mat4::from_euler(
                EulerRot::YXZ,
                (r(1) * 2.0 - 1.0) * a,
                (r(2) * 2.0 - 1.0) * a,
                (r(3) * 2.0 - 1.0) * a,
            );
        }
        if v.spin > 0 {
            let turns = 1 + (r(4) * v.spin as f32) as i32 % v.spin as i32;
            let sign = if r(5) < 0.5 { -1.0 } else { 1.0 };
            let axis = Vec3::new(r(6) - 0.5, r(7) - 0.5, r(8) - 0.5).normalize_or(Vec3::Y);
            var *= Mat4::from_axis_angle(axis, ctx.turns(sign * turns as f32));
        }
        let mut s = 1.0 + v.scale * (r(9) * 2.0 - 1.0);
        let wave_x = ctx.phase * v.ripple_cycles as f32 - frac * v.ripple_spread;
        if v.ripple != 0.0 {
            s *= 1.0 + v.ripple * (wave_x * TAU).sin();
        }
        let band = if v.spectrum != 0.0 {
            ctx.music.band_for(i, locals.len())
        } else {
            0.0
        };

        let s = s.max(0.02);
        var *= Mat4::from_scale(Vec3::splat(s));
        if band != 0.0 {
            // Equalizer bars grow upwards from their base.
            let k = (1.0 + v.spectrum * band).max(0.02);
            var *= Mat4::from_translation(Vec3::Y * (k - 1.0) * 0.5)
                * Mat4::from_scale(Vec3::new(1.0, k, 1.0));
        }
        let glow = if v.chase != 0.0 {
            let w = 0.5 + 0.5 * (wave_x * TAU).sin();
            1.0 + v.chase * (w.powi(6) * 3.0 - 0.5)
        } else {
            1.0
        } + v.spectrum.abs() * band * 2.0;
        let hue = if v.hue != 0.0 {
            v.hue * (r(10) - 0.5)
        } else {
            0.0
        };
        for sym in &syms {
            out.push(Instance {
                model: *sym * l * *local * var * size,
                hue,
                glow: glow.max(0.0),
                rand: r(11),
                along: frac,
            });
        }
    }
}

#[cfg(test)]
mod surface_tests {
    use super::*;

    #[test]
    fn copies_ride_the_terrain_and_loop() {
        let mut p = crate::presets::lava_world();
        let tname = p
            .layers
            .iter()
            .find(|l| matches!(l.kind, LayerKind::Terrain(_)))
            .unwrap()
            .name
            .clone();
        p.layers.push(Layer::new(
            "Posts",
            LayerKind::Mesh(MeshLayer {
                instancer: Instancer::OnTerrain {
                    terrain: tname,
                    count: 50,
                    seed: 2,
                    align: true,
                    lift: 0.0,
                    ground: None,
                },
                ..Default::default()
            }),
        ));
        let at = |phase: f32| {
            let ctx = EvalCtx::new(&p.timing, phase, None);
            let layers = p.scene_layers(&ctx).into_owned();
            let l = layers.iter().find(|l| l.name == "Posts").unwrap().clone();
            let LayerKind::Mesh(m) = &l.kind else {
                unreachable!()
            };
            let mut out = Vec::new();
            mesh_instances(&l, m, &ctx, &mut out);
            out
        };
        let (a, b, mid) = (at(0.0), at(1.0), at(0.1));
        assert_eq!(a.len(), 50);
        for (x, y) in a.iter().zip(&b) {
            assert!(x.model.abs_diff_eq(y.model, 1e-3));
        }
        // They move with the scroll.
        let moved = a
            .iter()
            .zip(&mid)
            .filter(|(x, y)| !x.model.abs_diff_eq(y.model, 1e-3))
            .count();
        assert!(moved > 40, "{moved}");
    }

    #[test]
    fn surface_samples_follow_area() {
        // Two triangles, areas 0.5 and 3.
        let pos = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [10.0, 0.0, 0.0],
            [13.0, 0.0, 0.0],
            [10.0, 0.0, 2.0],
        ];
        let pts = sample_surface(&pos, &[0, 1, 2, 3, 4, 5], 4000, 3);
        assert_eq!(pts.len(), 4000);
        let big = pts.iter().filter(|p| p.pos[0] >= 10.0).count() as f32 / 4000.0;
        assert!((big - 3.0 / 3.5).abs() < 0.03, "{big}");
        for p in &pts {
            assert!(p.pos[1].abs() < 1e-6);
            assert!((p.normal[1].abs() - 1.0).abs() < 1e-5);
        }
        assert_eq!(pts, sample_surface(&pos, &[0, 1, 2, 3, 4, 5], 4000, 3));
        assert!(sample_surface(&pos, &[], 10, 1).is_empty());
    }

    #[test]
    fn surface_copies_stand_on_the_points() {
        let layer = Layer::new(
            "Moss",
            LayerKind::Mesh(MeshLayer {
                instancer: Instancer::Surface {
                    shape: MeshSource::Primitive(Primitive::Cube),
                    size: 2.0,
                    count: 3,
                    seed: 1,
                    align: true,
                    lift: 0.5,
                },
                ..Default::default()
            }),
        );
        let LayerKind::Mesh(m) = &layer.kind else {
            unreachable!()
        };
        let pts = [SurfacePoint {
            pos: [0.0, 0.0, 0.5],
            normal: [0.0, 0.0, 1.0],
        }];
        let mut out = Vec::new();
        mesh_instances_with(&layer, m, &EvalCtx::at(0.0), Some(&pts), &mut out);
        assert_eq!(out.len(), 1);
        let p = out[0].model.w_axis.truncate();
        assert!(p.abs_diff_eq(Vec3::new(0.0, 0.0, 1.5), 1e-5), "{p}");
        // Up along the normal.
        let up = out[0].model.y_axis.truncate().normalize();
        assert!(up.abs_diff_eq(Vec3::Z, 1e-5));
        // Without points, no copies.
        out.clear();
        mesh_instances(&layer, m, &EvalCtx::at(0.0), &mut out);
        assert!(out.is_empty());
    }
}

#[cfg(test)]
mod shake_tests {
    use super::*;
    use crate::Param;

    #[test]
    fn shake_loops_and_moves() {
        let mut t = Transform::default();
        t.shake.amount = Param::new(0.0).osc(crate::Wave::ExpOut, 1.0, 16);
        t.shake.turn = Param::new(10.0);
        let a = layer_frame(&t, &EvalCtx::at(0.0));
        let b = layer_frame(&t, &EvalCtx::at(1.0));
        assert!(a.abs_diff_eq(b, 1e-4), "shake must loop");
        let still = layer_frame(&Transform::default(), &EvalCtx::at(0.0));
        assert!(!a.abs_diff_eq(still, 1e-3), "shake should move the layer");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: Mat4, b: Mat4) -> bool {
        a.to_cols_array()
            .iter()
            .zip(b.to_cols_array())
            .all(|(x, y)| (x - y).abs() < 1e-3)
    }

    #[test]
    fn camera_path_passes_points_at_even_speed() {
        let cam = Camera {
            mode: CameraMode::Path,
            ..Default::default()
        };
        // The default path is four points; it starts at the first.
        let start = cam.eval(&EvalCtx::at(0.0));
        assert!(start.eye.abs_diff_eq(Vec3::new(0.0, 2.0, 10.0), 1e-3));
        // Even speed: equal steps in time cover (nearly) equal distances.
        let steps: Vec<f32> = (0..64)
            .map(|i| {
                let a = cam.eval(&EvalCtx::at(i as f32 / 64.0)).eye;
                let b = cam.eval(&EvalCtx::at((i + 1) as f32 / 64.0)).eye;
                (a - b).length()
            })
            .collect();
        let (lo, hi) = steps
            .iter()
            .fold((f32::MAX, 0.0f32), |(l, h), d| (l.min(*d), h.max(*d)));
        assert!(hi / lo < 1.1, "{lo} .. {hi}");
        // Easing lingers: the camera moves much less right at a point.
        let mut eased = cam.clone();
        eased.path.ease = 1.0;
        let near =
            |c: &Camera| (c.eval(&EvalCtx::at(0.0)).eye - c.eval(&EvalCtx::at(0.005)).eye).length();
        assert!(near(&eased) < near(&cam) * 0.2);
        let end = eased.eval(&EvalCtx::at(1.0));
        assert!(end.eye.abs_diff_eq(start.eye, 1e-3));
    }

    #[test]
    fn look_from_round_trips() {
        let p = PathPoint {
            eye: [3.0, 5.0, -7.0],
            target: [1.0, 1.0, 2.0],
            roll: 12.0,
            fov: 40.0,
        };
        let mut cam = Camera::default();
        cam.look_from(&p);
        let back = cam.view_point(&EvalCtx::at(0.4));
        assert!(
            Vec3::from(back.eye).abs_diff_eq(Vec3::from(p.eye), 1e-3),
            "{back:?}"
        );
        assert_eq!((back.target, back.roll), (p.target, p.roll));
        assert!((back.fov - p.fov).abs() < 1e-3);
    }

    #[test]
    fn camera_punches_in_on_hits() {
        let cam = Camera {
            punch: 1.0,
            ..Default::default()
        };
        let mut ctx = EvalCtx::at(0.3);
        let calm = cam.eval(&ctx).fov_y;
        ctx.music.active = true;
        let kick = &mut ctx.music.hits[crate::audio::HitKind::Kick as usize];
        kick.since = 0.0;
        kick.strength = 1.0;
        assert!(cam.eval(&ctx).fov_y < calm * 0.7);
        ctx.music.hits[0].since = 2.0;
        assert!((cam.eval(&ctx).fov_y - calm).abs() < 1e-4);
    }

    #[test]
    fn camera_cuts_on_hits() {
        use crate::audio::HitKind;
        let mut cam = Camera {
            mode: CameraMode::Path,
            ..Default::default()
        };
        cam.path.cut_on = Some(HitKind::Kick);
        cam.path.drift = 0.0;
        let with_hits = |count: u32| {
            let mut ctx = EvalCtx::at(0.3);
            ctx.music.active = true;
            ctx.music.hits[HitKind::Kick as usize].count = count;
            ctx.music.hits[HitKind::Kick as usize].since = 0.0;
            ctx
        };
        for k in 0..6u32 {
            let want = Vec3::from(cam.path.points[k as usize % 4].eye);
            assert!(
                cam.eval(&with_hits(k)).eye.abs_diff_eq(want, 1e-3),
                "hit {k}"
            );
        }
        // Without music it flies as usual.
        let flying = cam.eval(&EvalCtx::at(0.3)).eye;
        assert!(!flying.abs_diff_eq(Vec3::from(cam.path.points[0].eye), 0.1));
    }

    #[test]
    fn camera_loops() {
        let mut cam = Camera {
            orbit_turns: 2,
            beat_shake: crate::Param::new(1.0),
            ..Default::default()
        };
        for mode in CameraMode::ALL {
            cam.mode = mode;
            let a = cam.eval(&EvalCtx::at(0.0));
            let b = cam.eval(&EvalCtx::at(1.0));
            assert!((a.eye - b.eye).length() < 1e-3, "{mode:?}");
        }
    }

    #[test]
    fn instances_loop() {
        for inst in Instancer::defaults() {
            let mut layer = Layer::default().spin([1, 2, 0]);
            layer.symmetry = Symmetry::Kaleido { count: 6 };
            let mesh = MeshLayer {
                instancer: inst.clone(),
                variation: Variation {
                    spin: 2,
                    ripple: 0.3,
                    chase: 1.0,
                    rotation: 30.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let (mut a, mut b) = (Vec::new(), Vec::new());
            mesh_instances(&layer, &mesh, &EvalCtx::at(0.0), &mut a);
            mesh_instances(&layer, &mesh, &EvalCtx::at(1.0), &mut b);
            assert_eq!(a.len(), b.len());
            for (x, y) in a.iter().zip(&b) {
                assert!(approx(x.model, y.model), "{inst:?}");
                assert!((x.glow - y.glow).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn static_layers_really_are_static() {
        for p in crate::presets::all() {
            for l in &p.project.layers {
                if let LayerKind::Mesh(m) = &l.kind {
                    if instances_are_static(l, m) {
                        let (mut a, mut b) = (Vec::new(), Vec::new());
                        mesh_instances(l, m, &EvalCtx::at(0.0), &mut a);
                        mesh_instances(l, m, &EvalCtx::at(0.37), &mut b);
                        assert_eq!(a, b, "{} / {}", p.name, l.name);
                    }
                }
            }
        }
    }

    #[test]
    fn symmetry_counts() {
        assert_eq!(symmetry_matrices(&Symmetry::MirrorXZ).len(), 4);
        assert_eq!(symmetry_matrices(&Symmetry::Radial { count: 8 }).len(), 8);
    }
}
