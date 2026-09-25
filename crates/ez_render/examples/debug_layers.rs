//! Renders a preset with each layer toggled off in turn (debug aid).
//! `cargo run -p ez_render --example debug_layers -- "Neon Arena" out.png`

use ez_core::{presets, EvalCtx};
use ez_render::gpu::Gpu;
use ez_render::Renderer;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let name = args.get(1).cloned().unwrap_or_else(|| "Neon Arena".into());
    let out = args.get(2).cloned().unwrap_or_else(|| "layers.png".into());
    let project = presets::by_name(&name).expect("unknown preset");
    let (w, h) = (384, 216);
    let gpu = Gpu::headless()?;
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = r.create_target(w, h);
    let n = project.layers.len() + 2;
    let cols = 3;
    let rows = n.div_ceil(cols) as u32;
    let mut sheet = image::RgbaImage::new(w * cols as u32, h * rows);
    let ctx = EvalCtx::new(&project.timing, 0.1, None);
    for i in 0..n {
        let mut p = project.clone();
        if i > 0 && i <= p.layers.len() {
            p.layers[i - 1].enabled = false;
            println!("{i}: without {}", p.layers[i - 1].name);
        } else if i > p.layers.len() {
            p.post.bloom.enabled = false;
            println!("{i}: without bloom");
        }
        let img = r.render_image(&p, &ctx, &target);
        image::imageops::overlay(
            &mut sheet,
            &img,
            ((i % cols) as u32 * w) as i64,
            ((i / cols) as u32 * h) as i64,
        );
    }
    sheet.save(&out)?;
    Ok(())
}
