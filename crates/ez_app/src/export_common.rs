//! Export dialog pieces shared by the desktop and web versions.

use egui::{RichText, Ui};
use ez_export::{GifOptions, GIF_FRAME_RATES};

/// Small sizes for the web: quick to load, light to share.
pub const WEB_RESOLUTIONS: &[(&str, u32, u32)] = &[
    ("Web 360p (640×360)", 640, 360),
    ("Web small (320×180)", 320, 180),
    ("Web square (360×360)", 360, 360),
    ("Web square small (240×240)", 240, 240),
    ("Web vertical (360×640)", 360, 640),
];

/// The "Presets" menu next to the size: common sizes, small web sizes,
/// and halving the current size (same shape).
pub fn size_presets(ui: &mut Ui, presets: &[(&str, u32, u32)], width: &mut u32, height: &mut u32) {
    ui.menu_button("Presets", |ui| {
        for (name, w, h) in presets {
            if ui.button(*name).clicked() {
                (*width, *height) = (*w, *h);
                ui.close();
            }
        }
        ui.separator();
        ui.label(RichText::new("For the web").weak().small());
        for (name, w, h) in WEB_RESOLUTIONS {
            if ui.button(*name).clicked() {
                (*width, *height) = (*w, *h);
                ui.close();
            }
        }
        ui.separator();
        for (name, div) in [("Half size", 2), ("Quarter size", 4)] {
            let (w, h) = (even(*width / div), even(*height / div));
            if ui
                .button(format!("{name} ({w}×{h})"))
                .on_hover_text("Smaller, same shape")
                .clicked()
            {
                (*width, *height) = (w, h);
                ui.close();
            }
        }
    });
}

/// Video encoders want even sizes.
fn even(v: u32) -> u32 {
    v.max(16) & !1
}

/// Frame rates offered for `gif` exports or others.
pub fn frame_rates(gif: bool, video: &[f32]) -> Vec<f32> {
    if gif {
        GIF_FRAME_RATES.to_vec()
    } else {
        video.to_vec()
    }
}

/// The GIF-only rows of an export dialog's grid: colours and dithering.
pub fn gif_rows(ui: &mut Ui, gif: &mut GifOptions) {
    ui.label("Colours");
    ui.horizontal(|ui| {
        for n in GifOptions::COLORS {
            ui.selectable_value(&mut gif.colors, n, n.to_string())
                .on_hover_text(
                    "Colours per frame: fewer make a smaller file and a more retro look",
                );
        }
    });
    ui.end_row();
    ui.label("Dither");
    ui.checkbox(&mut gif.dither, "smooth gradients with a fine pattern")
        .on_hover_text("Off: flat bands of colour and a smaller file");
    ui.end_row();
}

/// Switching to GIF: the size and frame rate of a typical GIF, unless
/// already small.
pub fn gif_defaults(width: &mut u32, height: &mut u32, fps: &mut f32) {
    if *width > 800 {
        (*width, *height) = (480, 270);
    }
    if !GIF_FRAME_RATES.contains(fps) {
        *fps = 25.0;
    }
}
