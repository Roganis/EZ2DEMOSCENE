//! EZ2DEMOSCENE: build loopable demoscene-style 3D scenes.

mod app;
#[cfg_attr(target_arch = "wasm32", path = "audio_web.rs")]
mod audio;
#[cfg(not(target_arch = "wasm32"))]
mod cli;
#[cfg_attr(target_arch = "wasm32", path = "export_web.rs")]
mod export_ui;
mod gizmo;
mod inspector;
#[cfg_attr(target_arch = "wasm32", path = "library_web.rs")]
mod library;
mod nodes;
mod platform;
mod viewport;
mod widgets;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    use std::path::PathBuf;
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

/// Browser entry point: runs the editor on the page's `<canvas id="ez2_canvas">`.
#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;
    console_error_panic_hook::set_once();
    let _ = eframe::WebLogger::init(log::LevelFilter::Info);
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .expect("no document");
        let canvas = document
            .get_element_by_id("ez2_canvas")
            .expect("missing <canvas id=\"ez2_canvas\">")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("ez2_canvas is not a canvas");
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(app::EzApp::new(cc, None)))),
            )
            .await;
        // Hide the loading message, or show what went wrong.
        if let Some(el) = document.get_element_by_id("ez2_loading") {
            match result {
                Ok(()) => el.remove(),
                Err(e) => el.set_inner_html(&format!(
                    "<p>EZ2DEMOSCENE could not start: {e:?}</p><p>It needs a browser with WebGPU or WebGL2 (recent Chrome, Edge, Firefox or Safari).</p>"
                )),
            }
        }
    });
}
