//! Renders every preset at a few loop phases into one PNG for quick review.
//! `cargo run -p ez_render --example contact_sheet -- out.png [width]`

use ez_core::{presets, EvalCtx};
use ez_render::gpu::Gpu;
use ez_render::Renderer;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let out = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "contact_sheet.png".into());
    let w: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(480);
    let only = args.get(3).cloned();
    let h = w * 9 / 16;
    let gpu = Gpu::headless()?;
    let mut r = Renderer::new(&gpu.device, &gpu.queue, 4);
    let target = r.create_target(w, h);
    let phases = [0.0, 0.3, 0.65];
    let list: Vec<_> = presets::all()
        .into_iter()
        .filter(|p| {
            only.as_ref()
                .is_none_or(|o| p.name.to_lowercase().contains(&o.to_lowercase()))
        })
        .collect();
    let mut sheet = image::RgbaImage::new(w * phases.len() as u32, h * list.len() as u32);
    for (row, p) in list.iter().enumerate() {
        for (col, ph) in phases.iter().enumerate() {
            let img = r.render_image(
                &p.project,
                &EvalCtx::new(&p.project.timing, *ph, None),
                &target,
            );
            image::imageops::overlay(
                &mut sheet,
                &img,
                (col as u32 * w) as i64,
                (row as u32 * h) as i64,
            );
        }
    }
    sheet.save(&out)?;
    println!("wrote {out}");
    Ok(())
}
