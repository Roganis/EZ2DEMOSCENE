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

impl Camera {
    pub fn eval(&self, ctx: &EvalCtx) -> CameraState {
        let base = self.angle.to_radians();
        let az = match self.mode {
            CameraMode::Orbit => base + ctx.turns(self.orbit_turns as f32),
            CameraMode::Pendulum => base + self.swing.to_radians() * ctx.turns(1.0).sin(),
            CameraMode::Static => base,
        };
        let d = self.distance.eval(ctx).max(0.1);
        let h = self.height.eval(ctx);
        let target = Vec3::from(self.target);
        let mut eye = target + Vec3::new(az.sin() * d, h, az.cos() * d);
        if self.beat_shake > 0.0 {
            let beat = (ctx.beat().floor() as u32) % ctx.loop_beats.max(1);
            let k = self.beat_shake * ctx.beat_pulse(6.0) * 0.1;
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
    let pos = Vec3::from(t.position) + Vec3::Y * t.bob.eval(ctx);
    Mat4::from_rotation_translation(base * spin, pos)
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
}

/// Local placement of each instance inside the layer.
fn instancer_locals(inst: &Instancer, ctx: &EvalCtx) -> Vec<Mat4> {
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
    }
}

fn random_dir(rng: &mut Rng) -> Vec3 {
    let z = rng.signed();
    let a = rng.f32() * TAU;
    let r = (1.0 - z * z).max(0.0).sqrt();
    Vec3::new(r * a.cos(), z, r * a.sin())
}

/// All instances of a mesh layer in world space.
///
/// The layer scale sizes each copy; instancer distances (radius, spacing)
/// are in world units and are not affected by it.
pub fn mesh_instances(layer: &Layer, mesh: &MeshLayer, ctx: &EvalCtx, out: &mut Vec<Instance>) {
    let l = layer_frame(&layer.transform, ctx);
    let size = Mat4::from_scale(layer_scale(&layer.transform, ctx));
    let syms = symmetry_matrices(&layer.symmetry);
    let locals = instancer_locals(&mesh.instancer, ctx);
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
        let s = s.max(0.02);
        var *= Mat4::from_scale(Vec3::splat(s));
        let glow = if v.chase != 0.0 {
            let w = 0.5 + 0.5 * (wave_x * TAU).sin();
            1.0 + v.chase * (w.powi(6) * 3.0 - 0.5)
        } else {
            1.0
        };
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
            });
        }
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
    fn camera_loops() {
        let mut cam = Camera {
            orbit_turns: 2,
            beat_shake: 1.0,
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
    fn symmetry_counts() {
        assert_eq!(symmetry_matrices(&Symmetry::MirrorXZ).len(), 4);
        assert_eq!(symmetry_matrices(&Symmetry::Radial { count: 8 }).len(), 8);
    }
}
