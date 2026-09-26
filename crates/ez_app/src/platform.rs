//! Platform differences behind one small API:
//! - picking files: native file dialogs vs. the browser's file picker
//!   (async, so results arrive a frame later through [`take_picked`]);
//! - saving files: native "save as" dialog vs. a browser download.
//!
//! Picked files are identified by an asset path: a real file path on
//! desktop, or a `mem://` path in the in-memory asset store on the web.

use std::cell::RefCell;

/// Which layer an import should be applied to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerRef {
    /// Index in the project's layer list.
    Layer(usize),
    /// Source node id in the node graph.
    Node(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TexSlot {
    Material,
    Backdrop,
    Mirror,
    Terrain,
    Relief,
}

/// What a picked file is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    OpenProject,
    AddModelLayer,
    SetModel(LayerRef),
    AddImages,
    SetTexture(LayerRef, TexSlot),
    LoadMusic,
    LoadMidi,
    SetFont(LayerRef),
    /// Dropped on the window: what it is depends on the file extension.
    Dropped,
}

impl Purpose {
    fn filter(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Purpose::OpenProject => ("EZ2 project or pack", &["json", "ez2pack"]),
            Purpose::AddModelLayer | Purpose::SetModel(_) => {
                ("3D models", crate::inspector::MODEL_EXTENSIONS)
            }
            Purpose::AddImages | Purpose::SetTexture(..) => {
                ("Images", crate::inspector::IMAGE_EXTENSIONS)
            }
            Purpose::LoadMusic => ("Audio", crate::app::AUDIO_EXTENSIONS),
            Purpose::LoadMidi => ("MIDI", crate::app::MIDI_EXTENSIONS),
            Purpose::SetFont(_) => ("Fonts", crate::inspector::FONT_EXTENSIONS),
            Purpose::Dropped => ("Any file", &[]),
        }
    }

    fn multiple(self) -> bool {
        matches!(self, Purpose::AddImages)
    }
}

#[derive(Clone, Debug)]
pub struct Picked {
    pub purpose: Purpose,
    /// Asset path (disk path or `mem://…`).
    pub path: String,
    /// File name as the user sees it.
    pub name: String,
}

thread_local! {
    static PICKED: RefCell<Vec<Picked>> = const { RefCell::new(Vec::new()) };
}

fn push(p: Picked) {
    PICKED.with(|q| q.borrow_mut().push(p));
}

/// Files picked since the last call.
pub fn take_picked() -> Vec<Picked> {
    PICKED.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Ask the user for file(s); results arrive through [`take_picked`].
pub fn pick(purpose: Purpose) {
    let (label, exts) = purpose.filter();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let dialog = rfd::FileDialog::new().add_filter(label, exts);
        let files = if purpose.multiple() {
            dialog.pick_files().unwrap_or_default()
        } else {
            dialog.pick_file().into_iter().collect()
        };
        for f in files {
            push(Picked {
                purpose,
                name: f
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                path: f.to_string_lossy().to_string(),
            });
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        let dialog = rfd::AsyncFileDialog::new().add_filter(label, exts);
        wasm_bindgen_futures::spawn_local(async move {
            let files = if purpose.multiple() {
                dialog.pick_files().await.unwrap_or_default()
            } else {
                dialog.pick_file().await.into_iter().collect()
            };
            for f in files {
                let name = f.file_name();
                let bytes = f.read().await;
                let path = ez_core::store::insert_new(&name, bytes);
                crate::library::persist_asset(&path);
                push(Picked {
                    purpose,
                    path,
                    name,
                });
            }
        });
    }
}

/// Queue a file dropped on the window (arrives as `Purpose::Dropped`).
pub fn handle_drop(file: egui::DroppedFileHandle) {
    let name = file
        .path()
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| file.path().to_string_lossy().to_string());
    #[cfg(not(target_arch = "wasm32"))]
    {
        push(Picked {
            purpose: Purpose::Dropped,
            path: file.path().to_string_lossy().to_string(),
            name,
        });
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm_bindgen_futures::spawn_local(async move {
            match file.bytes_async().await {
                Ok(bytes) => {
                    let path = ez_core::store::insert_new(&name, bytes);
                    crate::library::persist_asset(&path);
                    push(Picked {
                        purpose: Purpose::Dropped,
                        path,
                        name,
                    });
                }
                Err(e) => log::warn!("could not read dropped file {name}: {e}"),
            }
        });
    }
}

/// Save bytes: a "save as" dialog on desktop, a download in the browser.
/// Returns a description of where it went, or `None` if cancelled.
pub fn save_file(
    suggested_name: &str,
    filter: (&str, &[&str]),
    bytes: &[u8],
) -> Result<Option<String>, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(filter.0, filter.1)
            .set_file_name(suggested_name)
            .save_file()
        else {
            return Ok(None);
        };
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = filter;
        download(suggested_name, bytes)?;
        Ok(Some(format!("downloaded {suggested_name}")))
    }
}

/// Save `bytes` from the browser: a download, or the Android share sheet
/// inside the app (see web/ez2_save.js).
#[cfg(target_arch = "wasm32")]
pub fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    let global = js_sys::global();
    let f = js_sys::Reflect::get(&global, &"ez2SaveFile".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .ok_or("ez2_save.js is missing")?;
    let array = js_sys::Uint8Array::from(bytes);
    let promise = f
        .call2(&wasm_bindgen::JsValue::NULL, &name.into(), &array)
        .map_err(|e| format!("{e:?}"))?;
    if let Ok(p) = promise.dyn_into::<js_sys::Promise>() {
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = wasm_bindgen_futures::JsFuture::from(p).await {
                log::warn!("saving failed: {e:?}");
            }
        });
    }
    Ok(())
}

/// A `?name=value` parameter of the page URL (browser only).
pub fn query_param(name: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let search = web_sys::window()?.location().search().ok()?;
        let params = web_sys::UrlSearchParams::new_with_str(&search).ok()?;
        params.get(name)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = name;
        None
    }
}

/// Graphics backend choice for the web build, kept in `localStorage`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GpuBackendPref {
    /// WebGPU where the browser has it, otherwise WebGL2.
    Auto,
    WebGl,
    WebGpu,
}

#[allow(dead_code)]
const BACKEND_KEY: &str = "ez2_gpu_backend";

impl GpuBackendPref {
    pub fn label(self) -> &'static str {
        match self {
            GpuBackendPref::Auto => "Automatic",
            GpuBackendPref::WebGl => "WebGL2",
            GpuBackendPref::WebGpu => "WebGPU",
        }
    }

    /// What `Auto` means here: WebGL2 on Android, where WebGPU drivers
    /// (e.g. Mali-G615) corrupt the picture; WebGPU with a WebGL2
    /// fallback everywhere else.
    pub fn auto_uses_webgl() -> bool {
        is_android()
    }

    /// The saved choice (always `Auto` outside the browser).
    pub fn load() -> GpuBackendPref {
        #[cfg(target_arch = "wasm32")]
        {
            let v = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item(BACKEND_KEY).ok().flatten());
            match v.as_deref() {
                Some("webgl") => GpuBackendPref::WebGl,
                Some("webgpu") => GpuBackendPref::WebGpu,
                _ => GpuBackendPref::Auto,
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        GpuBackendPref::Auto
    }

    /// Remember the choice; it takes effect the next time the app starts.
    pub fn save(self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(s) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let v = match self {
                GpuBackendPref::Auto => "auto",
                GpuBackendPref::WebGl => "webgl",
                GpuBackendPref::WebGpu => "webgpu",
            };
            let _ = s.set_item(BACKEND_KEY, v);
        }
    }
}

/// True when running in Android's browser or the Android app.
pub fn is_android() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.navigator().user_agent().ok())
            .is_some_and(|ua| ua.contains("Android"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// Restart the web app (reload the page). Does nothing on desktop.
pub fn reload() {
    #[cfg(target_arch = "wasm32")]
    if let Some(w) = web_sys::window() {
        let _ = w.location().reload();
    }
}

/// Window title (desktop) or browser tab title (web).
pub fn set_title(ctx: &egui::Context, title: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_string()));
    #[cfg(target_arch = "wasm32")]
    {
        let _ = ctx;
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.set_title(title);
        }
    }
}

/// True on the web build.
pub const IS_WEB: bool = cfg!(target_arch = "wasm32");

/// Rough "this is a phone/tablet" check (touch screen) used for defaults.
#[allow(dead_code)]
pub fn is_touch_device() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.match_media("(pointer: coarse)").ok().flatten())
            .map(|m| m.matches())
            .unwrap_or(false)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

/// File-name friendly version of a project name.
pub fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "loop".into()
    } else {
        s
    }
}
