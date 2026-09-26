//! Viewport gizmos: click to pick a layer, drag handles to move / rotate /
//! scale it, optional ground grid. Hold Ctrl to snap.

use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use ez_core::eval::{mesh_instances, CameraState};
use ez_core::{EvalCtx, Layer, LayerKind};
use glam::{Mat4, Vec3, Vec4};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GizmoMode {
    Move,
    Rotate,
    Scale,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Handle {
    Axis(usize),
    Center,
}

#[derive(Clone, Debug)]
struct Drag {
    handle: Handle,
    start_pos: [f32; 3],
    start_rot: [f32; 3],
    start_stretch: [f32; 3],
    start_scale: f32,
    /// Raw (unsnapped) accumulated amount along the handle.
    amount: f32,
}

pub struct Gizmo {
    pub mode: GizmoMode,
    pub grid: bool,
    /// Touch screen: bigger handles and hit areas.
    pub touch: bool,
    drag: Option<Drag>,
}

pub const MOVE_SNAP: f32 = 0.25;
pub const ROTATE_SNAP: f32 = 15.0;
pub const SCALE_SNAP: f32 = 0.1;

const AXIS_COLORS: [Color32; 3] = [
    Color32::from_rgb(240, 70, 70),
    Color32::from_rgb(90, 220, 90),
    Color32::from_rgb(80, 140, 255),
];

pub fn snap(v: f32, step: f32) -> f32 {
    (v / step).round() * step
}

/// World -> screen projection for the viewport rectangle.
pub struct Projector {
    vp: Mat4,
    rect: Rect,
    pub eye: Vec3,
}

impl Projector {
    pub fn new(cam: &CameraState, rect: Rect) -> Projector {
        let aspect = rect.width() / rect.height().max(1.0);
        Projector {
            vp: cam.proj(aspect) * cam.view(),
            rect,
            eye: cam.eye,
        }
    }

    /// `None` when the point is behind the camera.
    pub fn to_screen(&self, p: Vec3) -> Option<Pos2> {
        let c: Vec4 = self.vp * p.extend(1.0);
        if c.w <= 0.05 {
            return None;
        }
        let ndc = c.truncate() / c.w;
        Some(Pos2::new(
            self.rect.left() + (ndc.x * 0.5 + 0.5) * self.rect.width(),
            self.rect.top() + (0.5 - ndc.y * 0.5) * self.rect.height(),
        ))
    }
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t - p).length()
}

/// Screen positions representing a layer, used for click-picking.
fn pick_points(layer: &Layer, ctx: &EvalCtx, proj: &Projector) -> Vec<Pos2> {
    let mut pts = Vec::new();
    let pos = Vec3::from(layer.transform.position);
    match &layer.kind {
        LayerKind::Mesh(m) => {
            let mut inst = Vec::new();
            mesh_instances(layer, m, ctx, &mut inst);
            for i in inst.iter().take(512) {
                if let Some(p) = proj.to_screen(i.model.w_axis.truncate()) {
                    pts.push(p);
                }
            }
        }
        LayerKind::Particles(_)
        | LayerKind::Terrain(_)
        | LayerKind::Lasers(_)
        | LayerKind::Ribbon(_)
        | LayerKind::Falls(_) => pts.extend(proj.to_screen(pos)),
        LayerKind::Mirror(_) | LayerKind::Backdrop(_) | LayerKind::Weather(_) => {}
    }
    pts
}

/// Index of the layer closest to `click` (within 28 px), if any.
pub fn pick(layers: &[Layer], ctx: &EvalCtx, proj: &Projector, click: Pos2) -> Option<usize> {
    let mut best = (28.0f32, None);
    for (i, l) in layers.iter().enumerate().filter(|(_, l)| l.enabled) {
        for p in pick_points(l, ctx, proj) {
            let d = (p - click).length();
            if d < best.0 {
                best = (d, Some(i));
            }
        }
    }
    best.1
}

pub fn draw_grid(painter: &egui::Painter, proj: &Projector, height: f32) {
    let stroke = |major: bool| {
        Stroke::new(
            1.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, if major { 55 } else { 22 }),
        )
    };
    let n = 20;
    for i in -n..=n {
        let major = i % 5 == 0;
        for axis in 0..2 {
            // Split lines into short pieces so partly visible lines still draw.
            for k in -n..n {
                let (a, b) = if axis == 0 {
                    (
                        Vec3::new(i as f32, height, k as f32),
                        Vec3::new(i as f32, height, (k + 1) as f32),
                    )
                } else {
                    (
                        Vec3::new(k as f32, height, i as f32),
                        Vec3::new((k + 1) as f32, height, i as f32),
                    )
                };
                if let (Some(pa), Some(pb)) = (proj.to_screen(a), proj.to_screen(b)) {
                    painter.line_segment([pa, pb], stroke(major));
                }
            }
        }
    }
}

impl Default for Gizmo {
    fn default() -> Self {
        Gizmo {
            mode: GizmoMode::Move,
            grid: false,
            touch: false,
            drag: None,
        }
    }
}

impl Gizmo {
    pub fn new(touch: bool) -> Gizmo {
        Gizmo {
            touch,
            ..Default::default()
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Draws the gizmo for `layer` and handles dragging. Returns true when
    /// the pointer is on (or dragging) a handle, so the caller should not
    /// orbit the camera.
    pub fn show(
        &mut self,
        painter: &egui::Painter,
        resp: &egui::Response,
        proj: &Projector,
        layer: &mut Layer,
        snapping: bool,
    ) -> bool {
        if matches!(layer.kind, LayerKind::Backdrop(_)) {
            self.drag = None;
            return false;
        }
        let only_y = matches!(layer.kind, LayerKind::Mirror(_));
        let t = &mut layer.transform;
        let origin = Vec3::from(t.position);
        let Some(o) = proj.to_screen(origin) else {
            self.drag = None;
            return false;
        };
        let len = (proj.eye - origin).length().max(0.5) * 0.15;
        let axes = [Vec3::X, Vec3::Y, Vec3::Z];
        let tips: Vec<Option<Pos2>> = axes
            .iter()
            .map(|a| proj.to_screen(origin + *a * len))
            .collect();

        // Hit testing.
        let hover_pos = resp.hover_pos();
        let mut hovered = None;
        if let Some(p) = hover_pos {
            let k = if self.touch { 2.2 } else { 1.0 };
            if (p - o).length() < 9.0 * k && !only_y {
                hovered = Some(Handle::Center);
            } else {
                let mut best = 8.0 * k;
                for (i, tip) in tips.iter().enumerate() {
                    if only_y && i != 1 {
                        continue;
                    }
                    let Some(tip) = tip else { continue };
                    let d = match self.mode {
                        GizmoMode::Rotate => ring_distance(proj, origin, i, len, p),
                        _ => dist_to_segment(p, o, *tip),
                    };
                    if d < best {
                        best = d;
                        hovered = Some(Handle::Axis(i));
                    }
                }
            }
        }

        if resp.drag_started() {
            if let Some(h) = hovered {
                self.drag = Some(Drag {
                    handle: h,
                    start_pos: t.position,
                    start_rot: t.rotation,
                    start_stretch: t.stretch,
                    start_scale: t.scale.base,
                    amount: 0.0,
                });
            }
        }
        if resp.drag_stopped() {
            self.drag = None;
        }

        if let (Some(d), true) = (&mut self.drag, resp.dragged()) {
            let delta = resp.drag_delta();
            match (self.mode, d.handle) {
                (GizmoMode::Move, Handle::Axis(i)) => {
                    if let Some(tip) = tips[i] {
                        let dir = tip - o;
                        let px = dir.length().max(1.0);
                        d.amount += delta.dot(dir / px) / px * len;
                        let mut v = d.start_pos[i] + d.amount;
                        if snapping {
                            v = snap(v, MOVE_SNAP);
                        }
                        t.position[i] = v;
                    }
                }
                (GizmoMode::Move, Handle::Center) => {
                    // Move on the ground plane: screen x -> camera right, y -> forward.
                    let right = tips[0].map(|p| p - o).unwrap_or(Vec2::X);
                    let fwd = tips[2].map(|p| p - o).unwrap_or(Vec2::Y);
                    let (rl, fl) = (right.length().max(1.0), fwd.length().max(1.0));
                    let dx = delta.dot(right / rl) / rl * len;
                    let dz = delta.dot(fwd / fl) / fl * len;
                    d.start_pos[0] += dx;
                    d.start_pos[2] += dz;
                    for i in [0, 2] {
                        t.position[i] = if snapping {
                            snap(d.start_pos[i], MOVE_SNAP)
                        } else {
                            d.start_pos[i]
                        };
                    }
                }
                (GizmoMode::Rotate, Handle::Axis(i)) => {
                    d.amount += (delta.x - delta.y) * 0.5;
                    let mut v = d.start_rot[i] + d.amount;
                    if snapping {
                        v = snap(v, ROTATE_SNAP);
                    }
                    t.rotation[i] = v;
                }
                (GizmoMode::Scale, Handle::Axis(i)) => {
                    d.amount += (delta.x - delta.y) * 0.01;
                    let mut v = (d.start_stretch[i] * (1.0 + d.amount)).max(0.01);
                    if snapping {
                        v = snap(v, SCALE_SNAP).max(SCALE_SNAP);
                    }
                    t.stretch[i] = v;
                }
                (GizmoMode::Scale, Handle::Center) => {
                    d.amount += (delta.x - delta.y) * 0.01;
                    let mut v = (d.start_scale * (1.0 + d.amount)).max(0.01);
                    if snapping {
                        v = snap(v, SCALE_SNAP).max(SCALE_SNAP);
                    }
                    t.scale.base = v;
                }
                (GizmoMode::Rotate, Handle::Center) => {}
            }
        }

        // Drawing.
        let active = self.drag.as_ref().map(|d| d.handle).or(hovered);
        for (i, tip) in tips.iter().enumerate() {
            if only_y && i != 1 {
                continue;
            }
            let Some(tip) = tip else { continue };
            let hot = active == Some(Handle::Axis(i));
            let color = if hot { Color32::WHITE } else { AXIS_COLORS[i] };
            match self.mode {
                GizmoMode::Rotate => draw_ring(
                    painter,
                    proj,
                    origin,
                    i,
                    len,
                    Stroke::new(if hot { 3.0 } else { 2.0 }, color),
                ),
                GizmoMode::Move | GizmoMode::Scale => {
                    painter
                        .line_segment([o, *tip], Stroke::new(if hot { 4.0 } else { 2.5 }, color));
                    if self.mode == GizmoMode::Move {
                        painter.circle_filled(*tip, if self.touch { 9.0 } else { 5.0 }, color);
                    } else {
                        painter.rect_filled(
                            Rect::from_center_size(
                                *tip,
                                Vec2::splat(if self.touch { 16.0 } else { 9.0 }),
                            ),
                            1.0,
                            color,
                        );
                    }
                }
            }
        }
        if !only_y && self.mode != GizmoMode::Rotate {
            let hot = active == Some(Handle::Center);
            painter.circle(
                o,
                7.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, if hot { 160 } else { 60 }),
                Stroke::new(1.5, Color32::WHITE),
            );
        }
        hovered.is_some() || self.drag.is_some()
    }
}

fn ring_point(origin: Vec3, axis: usize, r: f32, a: f32) -> Vec3 {
    let (s, c) = a.sin_cos();
    origin
        + match axis {
            0 => Vec3::new(0.0, c, s),
            1 => Vec3::new(c, 0.0, s),
            _ => Vec3::new(c, s, 0.0),
        } * r
}

fn draw_ring(
    painter: &egui::Painter,
    proj: &Projector,
    origin: Vec3,
    axis: usize,
    r: f32,
    stroke: Stroke,
) {
    let n = 48;
    let pts: Vec<Option<Pos2>> = (0..=n)
        .map(|k| {
            proj.to_screen(ring_point(
                origin,
                axis,
                r,
                k as f32 / n as f32 * std::f32::consts::TAU,
            ))
        })
        .collect();
    for w in pts.windows(2) {
        if let (Some(a), Some(b)) = (w[0], w[1]) {
            painter.line_segment([a, b], stroke);
        }
    }
}

fn ring_distance(proj: &Projector, origin: Vec3, axis: usize, r: f32, p: Pos2) -> f32 {
    let n = 48;
    let pts: Vec<Option<Pos2>> = (0..=n)
        .map(|k| {
            proj.to_screen(ring_point(
                origin,
                axis,
                r,
                k as f32 / n as f32 * std::f32::consts::TAU,
            ))
        })
        .collect();
    pts.windows(2)
        .filter_map(|w| Some(dist_to_segment(p, w[0]?, w[1]?)))
        .fold(f32::MAX, f32::min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ez_core::presets;

    fn projector() -> Projector {
        let cam = CameraState {
            eye: Vec3::new(0.0, 0.0, 10.0),
            target: Vec3::ZERO,
            up: Vec3::Y,
            fov_y: 1.0,
        };
        Projector::new(
            &cam,
            Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 450.0)),
        )
    }

    #[test]
    fn projects_centre_and_rejects_behind() {
        let p = projector();
        let c = p.to_screen(Vec3::ZERO).unwrap();
        assert!((c.x - 400.0).abs() < 0.5 && (c.y - 225.0).abs() < 0.5);
        let up = p.to_screen(Vec3::Y).unwrap();
        assert!(up.y < c.y, "screen y grows downwards");
        assert!(p.to_screen(Vec3::new(0.0, 0.0, 20.0)).is_none());
    }

    #[test]
    fn snapping() {
        assert_eq!(snap(1.13, 0.25), 1.25);
        assert_eq!(snap(-7.0, 15.0), 0.0);
        assert_eq!(snap(22.6, 15.0), 30.0);
    }

    #[test]
    fn picks_the_nearest_layer() {
        let mut project = presets::empty();
        let cube = project
            .layers
            .iter()
            .position(|l| l.name == "Cube")
            .unwrap();
        project.layers[cube].transform.position = [0.0, 0.0, 0.0];
        let p = projector();
        let ctx = EvalCtx::at(0.0);
        assert_eq!(
            pick(&project.layers, &ctx, &p, Pos2::new(402.0, 226.0)),
            Some(cube)
        );
        assert_eq!(pick(&project.layers, &ctx, &p, Pos2::new(20.0, 20.0)), None);
    }
}
