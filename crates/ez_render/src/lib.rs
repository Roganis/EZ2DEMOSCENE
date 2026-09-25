//! wgpu renderer for EZ2DEMOSCENE.
//!
//! [`Renderer::render`] draws a [`ez_core::Project`] at a given loop phase
//! into a [`RenderTarget`]; the editor viewport, thumbnails and the offline
//! exporter all go through this same path.

pub mod gpu;
pub mod import;
pub mod mesh;
mod renderer;
pub mod texgen;

pub use renderer::{
    supported_msaa, FrameStats, LayerStats, Readback, RenderTarget, Renderer, HDR_FORMAT,
    OUTPUT_FORMAT, OUTPUT_VIEW_FORMAT,
};
