//! EZ2DEMOSCENE: build loopable demoscene-style 3D scenes.

mod app;
mod audio;
mod cli;
mod export_ui;
mod gizmo;
mod inspector;
mod library;
mod nodes;
mod viewport;
mod widgets;

use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::run(&args)? {
        std::process::exit(code);
    }
    let initial = args.first().map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("EZ2DEMOSCENE")
            .with_inner_size([1500.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "EZ2DEMOSCENE",
        options,
        Box::new(move |cc| Ok(Box::new(app::EzApp::new(cc, initial)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
