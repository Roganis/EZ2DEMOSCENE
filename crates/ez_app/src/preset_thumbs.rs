//! Pictures of the built-in presets for the gallery, rendered ahead of time
//! with `--write-preset-thumbs` and bundled, so opening the gallery (at
//! start-up) doesn't draw 60 scenes and wait for their simulations.

use std::collections::HashMap;

/// Size of a thumbnail, in pixels.
#[cfg(not(target_arch = "wasm32"))]
const SIZE: [u32; 2] = [320, 180];
/// Where in the loop the thumbnail is taken.
#[cfg(not(target_arch = "wasm32"))]
const PHASE: f32 = 0.2;

static BUNDLED: &[u8] = include_bytes!("../../../assets/presets/thumbnails.zip");

/// A preset's file name in the zip.
pub fn file_name(preset: &str) -> String {
    format!("{}.jpg", crate::platform::slug(preset))
}

/// The bundled thumbnails, by file name.
pub fn bundled() -> HashMap<String, egui::ColorImage> {
    let mut out = HashMap::new();
    let Ok(mut zip) = zip::ZipArchive::new(std::io::Cursor::new(BUNDLED)) else {
        log::warn!("preset thumbnails: unreadable zip");
        return out;
    };
    for i in 0..zip.len() {
        let Ok(mut file) = zip.by_index(i) else {
            continue;
        };
        let name = file.name().to_string();
        let mut bytes = Vec::new();
        if std::io::Read::read_to_end(&mut file, &mut bytes).is_err() {
            continue;
        }
        match image::load_from_memory(&bytes) {
            Ok(img) => {
                let img = img.to_rgba8();
                let size = [img.width() as usize, img.height() as usize];
                out.insert(
                    name,
                    egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()),
                );
            }
            Err(e) => log::warn!("preset thumbnail {name}: {e}"),
        }
    }
    out
}

/// Render every built-in preset (simulations baked) into a zip of JPEGs.
#[cfg(not(target_arch = "wasm32"))]
pub fn write(path: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::io::Write;
    let gpu = ez_render::gpu::Gpu::headless()?;
    let mut renderer = ez_render::Renderer::new(&gpu.device, &gpu.queue, 4);
    renderer.set_wait_for_bakes(true);
    let target = renderer.create_target(SIZE[0], SIZE[1]);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    // JPEGs don't shrink any further.
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for p in ez_core::presets::all() {
        let ctx = ez_core::EvalCtx::new(&p.project.timing, PHASE, None);
        let img = renderer.render_image(&p.project, &ctx, &target);
        let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
            .encode_image(&rgb)
            .with_context(|| format!("encoding {}", p.name))?;
        zip.start_file(file_name(p.name), opts)?;
        zip.write_all(&jpeg)?;
        println!("rendered {}", p.name);
    }
    let bytes = zip.finish()?.into_inner();
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_preset_has_a_bundled_thumbnail() {
        let thumbs = super::bundled();
        let missing: Vec<&str> = ez_core::presets::all()
            .iter()
            .filter(|p| !thumbs.contains_key(&super::file_name(p.name)))
            .map(|p| p.name)
            .collect();
        assert!(
            missing.is_empty(),
            "no thumbnail for {missing:?}: run `ez2demoscene --write-preset-thumbs \
             assets/presets/thumbnails.zip`"
        );
    }
}
