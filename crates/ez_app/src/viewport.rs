//! Renders the project into an offscreen texture shown by egui.

use egui_wgpu::RenderState;
use ez_core::{EvalCtx, Layer, LayerKind, MeshLayer, MeshSource, Param, Project};
use ez_render::{RenderTarget, Renderer};
use std::collections::HashMap;

/// Side of a shape thumbnail, in pixels.
const SHAPE_THUMB_PX: u32 = 128;
/// Thumbnails rendered per frame while the shape picker is open.
const SHAPE_THUMBS_PER_FRAME: usize = 6;
/// Thumbnails kept before the oldest are dropped.
const SHAPE_THUMBS_KEPT: usize = 600;

pub struct Viewport {
    pub renderer: Renderer,
    target: Option<RenderTarget>,
    texture_id: Option<egui::TextureId>,
    render_state: RenderState,
    shape_thumbs: ShapeThumbs,
}

/// Small pictures of shapes for the shape picker, rendered a few per frame
/// through one shared target and copied into their own textures.
#[derive(Default)]
struct ShapeThumbs {
    target: Option<RenderTarget>,
    done: HashMap<String, (egui::TextureId, wgpu::Texture, u64)>,
    /// Waiting thumbnails, in the order the picker showed them, with the
    /// frame they were last asked for.
    queue: Vec<(String, MeshSource, u64)>,
    frame: u64,
}

impl Viewport {
    pub fn new(render_state: &RenderState) -> Viewport {
        Viewport {
            renderer: Renderer::new(
                &render_state.device,
                &render_state.queue,
                ez_render::supported_msaa(&render_state.adapter),
            ),
            target: None,
            texture_id: None,
            render_state: render_state.clone(),
            shape_thumbs: ShapeThumbs::default(),
        }
    }

    /// Render at `size` pixels and return the egui texture to display.
    pub fn render(&mut self, project: &Project, ctx: &EvalCtx, size: [u32; 2]) -> egui::TextureId {
        // Never larger than the device's textures can be (4096 on some
        // phones): shrink, keeping the shape.
        let max = self
            .render_state
            .device
            .limits()
            .max_texture_dimension_2d
            .clamp(16, 7680);
        let k = (max as f32 / size[0].max(size[1]).max(1) as f32).min(1.0);
        let size = [
            ((size[0] as f32 * k) as u32).clamp(16, max),
            ((size[1] as f32 * k) as u32).clamp(16, max),
        ];
        let needs_new = self
            .target
            .as_ref()
            .is_none_or(|t| t.width != size[0] || t.height != size[1]);
        if needs_new {
            let target = self.renderer.create_target(size[0], size[1]);
            let mut egui_renderer = self.render_state.renderer.write();
            match self.texture_id {
                Some(id) => egui_renderer.update_egui_texture_from_wgpu_texture(
                    &self.render_state.device,
                    &target.display_view,
                    wgpu::FilterMode::Linear,
                    id,
                ),
                None => {
                    self.texture_id = Some(egui_renderer.register_native_texture(
                        &self.render_state.device,
                        &target.display_view,
                        wgpu::FilterMode::Linear,
                    ));
                }
            }
            self.target = Some(target);
        }
        let target = self.target.as_ref().expect("target");
        self.renderer.render(project, ctx, target);
        self.texture_id.expect("texture id")
    }

    /// The GPU adapter the app runs on (for the Graphics window).
    pub fn adapter_info(&self) -> wgpu::AdapterInfo {
        self.render_state.adapter.get_info()
    }

    /// The texture of the last rendered frame, if any.
    pub fn last_texture(&self) -> Option<egui::TextureId> {
        self.texture_id
    }

    /// A thumbnail of a shape, or `None` while it waits its turn (it is
    /// queued; call [`Self::render_shape_thumbs`] each frame).
    pub fn shape_thumb(&mut self, key: &str, source: &MeshSource) -> Option<egui::TextureId> {
        let t = &mut self.shape_thumbs;
        if let Some((id, _, used)) = t.done.get_mut(key) {
            *used = t.frame;
            return Some(*id);
        }
        match t.queue.iter_mut().find(|(k, _, _)| k == key) {
            Some(q) => q.2 = t.frame,
            None => t.queue.push((key.to_string(), source.clone(), t.frame)),
        }
        None
    }

    /// Render a few queued shape thumbnails. Returns whether more wait.
    pub fn render_shape_thumbs(&mut self) -> bool {
        let t = &mut self.shape_thumbs;
        let frame = t.frame;
        t.frame += 1;
        // Only tiles on screen this frame, top-left first; the rest were
        // scrolled away.
        t.queue.retain(|q| q.2 == frame);
        if t.queue.is_empty() {
            return false;
        }
        let n = t.queue.len().min(SHAPE_THUMBS_PER_FRAME);
        let batch: Vec<_> = t.queue.drain(..n).map(|(k, s, _)| (k, s)).collect();
        let px = SHAPE_THUMB_PX;
        if self.shape_thumbs.target.is_none() {
            self.shape_thumbs.target = Some(self.renderer.create_target(px, px));
        }
        let device = self.render_state.device.clone();
        let queue = self.render_state.queue.clone();
        for (key, source) in batch {
            let project = shape_thumb_project(source);
            let ctx = EvalCtx::new(&project.timing, 0.0, None);
            let target = self.shape_thumbs.target.as_ref().expect("thumb target");
            self.renderer.render(&project, &ctx, target);
            let size = wgpu::Extent3d {
                width: px,
                height: px,
                depth_or_array_layers: 1,
            };
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("shape thumb"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: ez_render::DISPLAY_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let mut enc = device.create_command_encoder(&Default::default());
            enc.copy_texture_to_texture(target.display.as_image_copy(), tex.as_image_copy(), size);
            queue.submit([enc.finish()]);
            let view = tex.create_view(&Default::default());
            let id = self.render_state.renderer.write().register_native_texture(
                &device,
                &view,
                wgpu::FilterMode::Linear,
            );
            let t = &mut self.shape_thumbs;
            t.done.insert(key, (id, tex, t.frame));
        }
        // Forget the least recently shown thumbnails.
        let t = &mut self.shape_thumbs;
        if t.done.len() > SHAPE_THUMBS_KEPT {
            let mut ages: Vec<(u64, String)> =
                t.done.iter().map(|(k, v)| (v.2, k.clone())).collect();
            ages.sort();
            let mut egui_renderer = self.render_state.renderer.write();
            for (_, k) in ages.iter().take(t.done.len() - SHAPE_THUMBS_KEPT) {
                if let Some((id, _, _)) = t.done.remove(k) {
                    egui_renderer.free_texture(&id);
                }
            }
        }
        !self.shape_thumbs.queue.is_empty()
    }

    /// Forget queued thumbnails the picker no longer shows.
    pub fn clear_shape_thumb_queue(&mut self) {
        self.shape_thumbs.queue.clear();
    }

    /// A new egui texture for preset thumbnails, drawn later with
    /// [`Self::draw_thumbnail`].
    pub fn thumbnail_target(&mut self, size: [u32; 2]) -> (egui::TextureId, RenderTarget) {
        let target = self.renderer.create_target(size[0], size[1]);
        let id = self.render_state.renderer.write().register_native_texture(
            &self.render_state.device,
            &target.display_view,
            wgpu::FilterMode::Linear,
        );
        (id, target)
    }

    /// Render a project once into a thumbnail's target. Simulations still
    /// baking are left out rather than waited for (a flock can take
    /// seconds); returns whether the picture is complete.
    pub fn draw_thumbnail(&mut self, project: &Project, phase: f32, target: &RenderTarget) -> bool {
        let ctx = EvalCtx::new(&project.timing, phase, None);
        self.renderer.take_inexact();
        self.renderer.render(project, &ctx, target);
        !self.renderer.take_inexact()
    }
}

/// A shape alone, lit and seen from slightly above, for its thumbnail.
fn shape_thumb_project(source: MeshSource) -> Project {
    let mut p = Project::default();
    p.camera.distance = Param::new(2.5);
    p.camera.height = Param::new(1.0);
    p.camera.angle = Param::new(35.0);
    p.layers.push(Layer {
        name: "shape".into(),
        kind: LayerKind::Mesh(MeshLayer {
            source,
            ..Default::default()
        }),
        ..Default::default()
    });
    p
}
