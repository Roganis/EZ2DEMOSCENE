//! Where 3D things show on the picture, in the units logos use: `x`
//! across and `y` up, as fractions of the picture from its bottom-left
//! corner. Retro low resolutions and console screens keep the picture's
//! proportions, so these hold for every look.

use crate::clock::EvalCtx;
use crate::eval::{layer_frame, layer_scale, mesh_instances, CameraState, Instance};
use crate::scene::*;
use glam::{Mat4, Vec3, Vec4Swizzles};

/// A point on the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenSpot {
    /// Across, 0 at the left edge, 1 at the right.
    pub x: f32,
    /// Up, 0 at the bottom edge, 1 at the top.
    pub y: f32,
    /// Distance in front of the camera (world units).
    pub depth: f32,
}

/// The part of the picture a layer covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenBox {
    /// Where the layer's own position shows (the middle of its copies
    /// when that is behind the camera).
    pub centre: [f32; 2],
    /// Lower-left and upper-right corners (fractions, may reach past the
    /// edges).
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// Distance of the nearest part in front of the camera.
    pub depth: f32,
    /// Copies in front of the camera.
    pub copies: u32,
}

impl ScreenBox {
    /// True when some of it is inside the picture.
    pub fn on_screen(&self) -> bool {
        self.max[0] > 0.0 && self.min[0] < 1.0 && self.max[1] > 0.0 && self.min[1] < 1.0
    }

    /// A point of the box: (0, 0) its lower-left corner, (1, 1) its
    /// upper-right one.
    pub fn point(&self, at: [f32; 2]) -> [f32; 2] {
        [
            self.min[0] + (self.max[0] - self.min[0]) * at[0],
            self.min[1] + (self.max[1] - self.min[1]) * at[1],
        ]
    }

    /// The box covering both.
    pub fn union(&self, o: &ScreenBox) -> ScreenBox {
        ScreenBox {
            centre: self.centre,
            min: [self.min[0].min(o.min[0]), self.min[1].min(o.min[1])],
            max: [self.max[0].max(o.max[0]), self.max[1].max(o.max[1])],
            depth: self.depth.min(o.depth),
            copies: self.copies.max(o.copies),
        }
    }
}

/// Nearer than this, points count as behind the camera.
const NEAR: f32 = 0.05;

/// Where `p` shows on a picture `aspect` (width / height) wide; `None`
/// behind the camera.
pub fn project_point(cam: &CameraState, aspect: f32, p: Vec3) -> Option<ScreenSpot> {
    let clip = cam.proj(aspect) * cam.view() * p.extend(1.0);
    if clip.w <= NEAR {
        return None;
    }
    let ndc = clip.xy() / clip.w;
    Some(ScreenSpot {
        x: ndc.x * 0.5 + 0.5,
        y: ndc.y * 0.5 + 0.5,
        depth: clip.w,
    })
}

/// The picture area covered by `layer` (see [`ScreenBox`]): each copy of
/// a shape layer as the box its shape fits in (built-in shapes and models
/// fit within ±1 on every axis before scaling), turned and stretched with
/// it; other layers as such a box the size of their scale around their
/// position. Parts behind the camera are cut off. `None` when all of it
/// is behind the camera.
pub fn layer_box(
    layer: &Layer,
    ctx: &EvalCtx,
    cam: &CameraState,
    aspect: f32,
) -> Option<ScreenBox> {
    let frame = layer_frame(&layer.transform, ctx);
    let origin = frame.w_axis.xyz();
    let mut models: Vec<Mat4> = Vec::new();
    if let LayerKind::Mesh(m) = &layer.kind {
        let mut copies: Vec<Instance> = Vec::new();
        mesh_instances(layer, m, ctx, &mut copies);
        models.extend(copies.iter().map(|c| c.model));
    }
    if models.is_empty() {
        models.push(frame * Mat4::from_scale(layer_scale(&layer.transform, ctx)));
    }
    let view = cam.view();
    let proj = cam.proj(aspect);
    let mut out: Option<ScreenBox> = None;
    for model in models {
        let Some(b) = box_of(&(view * model), &proj) else {
            continue;
        };
        out = Some(match out {
            None => b,
            Some(o) => ScreenBox {
                copies: o.copies + 1,
                ..o.union(&b)
            },
        });
    }
    let mut b = out?;
    b.centre = match project_point(cam, aspect, origin) {
        Some(s) => [s.x, s.y],
        None => b.point([0.5, 0.5]),
    };
    Some(b)
}

/// Screen box of the cube ±1 through `model_view`: its corners in front of
/// the camera, and where its edges cross into view.
fn box_of(model_view: &Mat4, proj: &Mat4) -> Option<ScreenBox> {
    let corner = |i: usize| {
        let c = Vec3::new(
            if i & 1 == 0 { -1.0 } else { 1.0 },
            if i & 2 == 0 { -1.0 } else { 1.0 },
            if i & 4 == 0 { -1.0 } else { 1.0 },
        );
        model_view.transform_point3(c)
    };
    let corners: Vec<Vec3> = (0..8).map(corner).collect();
    // In front: the view looks down -z.
    let front = |v: Vec3| -v.z > NEAR;
    let mut pts: Vec<Vec3> = corners.iter().copied().filter(|v| front(*v)).collect();
    if pts.len() < 8 {
        for a in 0..8usize {
            for bit in [1usize, 2, 4] {
                let b = a | bit;
                if b == a {
                    continue;
                }
                let (va, vb) = (corners[a], corners[b]);
                if front(va) != front(vb) {
                    let t = (-NEAR - va.z) / (vb.z - va.z);
                    pts.push(va + (vb - va) * t);
                }
            }
        }
    }
    if pts.is_empty() {
        return None;
    }
    let mut min = [f32::MAX; 2];
    let mut max = [f32::MIN; 2];
    let mut depth = f32::MAX;
    for v in &pts {
        let c = *proj * v.extend(1.0);
        let (x, y) = (c.x / c.w * 0.5 + 0.5, c.y / c.w * 0.5 + 0.5);
        min = [min[0].min(x), min[1].min(y)];
        max = [max[0].max(x), max[1].max(y)];
        depth = depth.min(-v.z);
    }
    Some(ScreenBox {
        centre: [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5],
        min,
        max,
        depth,
        copies: 1,
    })
}

impl Project {
    /// The scene on screen at `ctx` (a timeline's current clip, with its
    /// own moment), ready to measure.
    pub fn shown_at(&self, ctx: &EvalCtx) -> (Project, EvalCtx) {
        if let Some(f) = self.sequence.frame_at(ctx) {
            if let Some(p) = self.scene_view(f.scene) {
                return (p, f.ctx);
            }
        }
        (self.clone(), *ctx)
    }

    /// Where the enabled layer called `name` shows at `ctx` on a picture
    /// `aspect` wide. `None` when there is no such layer or it is all
    /// behind the camera.
    pub fn locate_layer(&self, name: &str, ctx: &EvalCtx, aspect: f32) -> Option<ScreenBox> {
        let (p, ctx) = self.shown_at(ctx);
        let layers = p.scene_layers(&ctx);
        let layer = layers.iter().find(|l| l.enabled && l.name == name)?;
        layer_box(layer, &ctx, &p.camera.eval(&ctx), aspect)
    }

    /// Where the world point `p` shows at `ctx`; `None` behind the camera.
    pub fn locate_point(&self, p: Vec3, ctx: &EvalCtx, aspect: f32) -> Option<ScreenSpot> {
        let (project, ctx) = self.shown_at(ctx);
        project_point(&project.camera.eval(&ctx), aspect, p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::param::Param;

    fn still_camera() -> Camera {
        Camera {
            mode: CameraMode::Static,
            target: [0.0, 0.0, 0.0],
            distance: Param::new(10.0),
            height: Param::new(0.0),
            fov: Param::new(60.0),
            ..Default::default()
        }
    }

    #[test]
    fn the_target_is_in_the_middle_and_behind_is_none() {
        let mut p = crate::presets::empty();
        p.camera = still_camera();
        let ctx = EvalCtx::at(0.0);
        let mid = p.locate_point(Vec3::ZERO, &ctx, 16.0 / 9.0).unwrap();
        assert!(
            (mid.x - 0.5).abs() < 1e-4 && (mid.y - 0.5).abs() < 1e-4,
            "{mid:?}"
        );
        assert!((mid.depth - 10.0).abs() < 1e-3);
        // Up is up, and the top edge is where the field of view ends.
        let cam = p.camera.eval(&ctx);
        let top = cam.target + cam.up * 10.0 * (30f32).to_radians().tan();
        let s = p.locate_point(top, &ctx, 1.0).unwrap();
        assert!((s.y - 1.0).abs() < 1e-3, "{s:?}");
        // Behind the camera.
        let behind = cam.eye + (cam.eye - cam.target);
        assert!(p.locate_point(behind, &ctx, 1.0).is_none());
    }

    #[test]
    fn a_layer_box_holds_its_shape_and_its_copies() {
        let mut p = crate::presets::empty();
        p.camera = still_camera();
        let ctx = EvalCtx::at(0.0);
        let cube = p.layers.iter().position(|l| l.name == "Cube").unwrap();
        p.layers[cube].transform.position = [0.0, 0.0, 0.0];
        p.layers[cube].transform.scale = Param::new(1.0);
        p.layers[cube].transform.spin = [0, 0, 0];
        let one = p.locate_layer("Cube", &ctx, 1.0).unwrap();
        assert_eq!(one.copies, 1);
        assert!((one.centre[0] - 0.5).abs() < 1e-4);
        // A cube ±1 whose near face is 9 away, with a 60° view: its top
        // edge 1 up at 9 away.
        let ry = 0.5 / (30f32).to_radians().tan() / 9.0;
        assert!(
            ((one.max[1] - one.min[1]) * 0.5 - ry).abs() < 0.005,
            "{one:?}"
        );
        assert!((one.depth - 9.0).abs() < 1e-3);
        assert!(one.on_screen());
        // A row of copies widens the box.
        if let LayerKind::Mesh(m) = &mut p.layers[cube].kind {
            m.instancer = Instancer::Grid {
                counts: [5, 1, 1],
                spacing: [2.0, 2.0, 2.0],
            };
        }
        let row = p.locate_layer("Cube", &ctx, 1.0).unwrap();
        assert_eq!(row.copies, 5);
        assert!(row.max[0] - row.min[0] > 3.0 * (one.max[0] - one.min[0]));
        assert!(p.locate_layer("No such layer", &ctx, 1.0).is_none());
    }

    /// A flat ring close by stays a flat box, and a shape reaching behind
    /// the camera is cut at the camera instead of blowing up.
    #[test]
    fn flat_and_nearby_shapes_get_tight_boxes() {
        let mut p = crate::presets::empty();
        p.camera = still_camera();
        let ctx = EvalCtx::at(0.0);
        let cube = p.layers.iter().position(|l| l.name == "Cube").unwrap();
        let t = &mut p.layers[cube].transform;
        t.position = [0.0, -2.0, 0.0];
        t.scale = Param::new(3.0);
        t.stretch = [1.0, 0.02, 1.0];
        t.spin = [0, 0, 0];
        let flat = p.locate_layer("Cube", &ctx, 1.0).unwrap();
        assert!(flat.max[1] - flat.min[1] < 0.2, "{flat:?}");
        // Huge and reaching past the camera: still a finite box.
        let t = &mut p.layers[cube].transform;
        t.scale = Param::new(30.0);
        let big = p.locate_layer("Cube", &ctx, 1.0).unwrap();
        assert!(
            big.min.iter().chain(&big.max).all(|v| v.is_finite()),
            "{big:?}"
        );
        assert!(big.depth <= NEAR * 1.01, "{big:?}");
    }

    #[test]
    fn a_timeline_measures_the_scene_on_screen() {
        let mut p = crate::presets::empty();
        p.camera = still_camera();
        p.start_sequence();
        let b = p.add_scene(false);
        // The new scene has a cube of its own: name it apart.
        for l in &mut p.sequence.scenes[0].layers {
            l.name = format!("{} B", l.name);
        }
        p.sequence.clips.push(crate::sequence::Clip {
            scene: b,
            beats: 16,
            ..Default::default()
        });
        p.sync_sequence_length();
        // The first clip shows the cube; the second, an empty scene.
        let first = EvalCtx::new(&p.timing, 0.25, None);
        let second = EvalCtx::new(&p.timing, 0.75, None);
        assert!(p.locate_layer("Cube", &first, 1.0).is_some());
        assert!(p.locate_layer("Cube", &second, 1.0).is_none());
        assert!(p.locate_layer("Cube B", &second, 1.0).is_some());
    }
}
