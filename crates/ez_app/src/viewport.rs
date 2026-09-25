//! Renders the project into an offscreen texture shown by egui.

use egui_wgpu::RenderState;
use ez_core::{EvalCtx, Project};
use ez_render::{RenderTarget, Renderer};

pub struct Viewport {
    pub renderer: Renderer,
    target: Option<RenderTarget>,
    texture_id: Option<egui::TextureId>,
    render_state: RenderState,
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
        }
    }

    /// Render at `size` pixels and return the egui texture to display.
    pub fn render(&mut self, project: &Project, ctx: &EvalCtx, size: [u32; 2]) -> egui::TextureId {
        let size = [size[0].clamp(16, 7680), size[1].clamp(16, 4320)];
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

    /// Render a project once into a new egui texture (preset thumbnails).
    pub fn thumbnail(
        &mut self,
        project: &Project,
        phase: f32,
        size: [u32; 2],
    ) -> (egui::TextureId, RenderTarget) {
        let target = self.renderer.create_target(size[0], size[1]);
        let ctx = EvalCtx::new(&project.timing, phase, None);
        self.renderer.render(project, &ctx, &target);
        let id = self.render_state.renderer.write().register_native_texture(
            &self.render_state.device,
            &target.display_view,
            wgpu::FilterMode::Linear,
        );
        (id, target)
    }
}
