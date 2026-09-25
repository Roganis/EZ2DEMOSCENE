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
        // The user can force WebGL2 or WebGPU from the Graphics window
        // (useful when a phone's driver misbehaves with one of them).
        let pref = platform::GpuBackendPref::load();
        let mut options = eframe::WebOptions::default();
        if let egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
            match pref {
                platform::GpuBackendPref::Auto => {}
                platform::GpuBackendPref::WebGl => {
                    setup.instance_descriptor.backends = wgpu::Backends::GL
                }
                platform::GpuBackendPref::WebGpu => {
                    setup.instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU
                }
            }
        }
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(|cc| Ok(Box::new(app::EzApp::new(cc, None)))),
            )
            .await;
        // Hide the loading message, or show what went wrong.
        if let Some(el) = document.get_element_by_id("ez2_loading") {
            match result {
                Ok(()) => el.remove(),
                Err(e) if pref != platform::GpuBackendPref::Auto => {
                    // A forced backend that isn't available: go back to
                    // automatic so the next start works.
                    platform::GpuBackendPref::Auto.save();
                    el.set_inner_html(&format!(
                        "<p>EZ2DEMOSCENE could not start with {}: {e:?}</p><p>The graphics setting is back on Automatic. <a href=\"javascript:location.reload()\">Reload</a> to start again.</p>",
                        pref.label()
                    ))
                }
                Err(e) => el.set_inner_html(&format!(
                    "<p>EZ2DEMOSCENE could not start: {e:?}</p><p>It needs a browser with WebGPU or WebGL2 (recent Chrome, Edge, Firefox or Safari).</p>"
                )),
            }
        }
    });
}
