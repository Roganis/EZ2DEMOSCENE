//! Desktop graphics backend: the user's choice, or an automatic one that
//! steps past a backend whose start-up crashed or hung last time.
//!
//! Some drivers kill the process outright (e.g. old Intel Vulkan drivers) or
//! hang while compiling shaders, so there is no error to catch. Instead, the
//! backend being tried is written to disk before the GPU is touched and
//! marked good once frames have been drawn and every pipeline compiled (in
//! the background, see `Renderer::warm_up_in_background`); a leftover
//! "trying" means the last start failed, and the next backend in line is
//! used.

use std::path::PathBuf;

/// Graphics backend choice for the desktop app.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// Try each backend in turn until one starts.
    Auto,
    Dx12,
    Vulkan,
    Metal,
    Gl,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Backend::Auto => "Automatic",
            Backend::Dx12 => "DirectX 12",
            Backend::Vulkan => "Vulkan",
            Backend::Metal => "Metal",
            Backend::Gl => "OpenGL",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Backend::Auto => "auto",
            Backend::Dx12 => "dx12",
            Backend::Vulkan => "vulkan",
            Backend::Metal => "metal",
            Backend::Gl => "gl",
        }
    }

    fn from_key(s: &str) -> Option<Backend> {
        [
            Backend::Auto,
            Backend::Dx12,
            Backend::Vulkan,
            Backend::Metal,
            Backend::Gl,
        ]
        .into_iter()
        .find(|b| b.key() == s.trim())
    }

    pub fn wgpu(self) -> wgpu::Backends {
        match self {
            Backend::Auto => wgpu::Backends::PRIMARY | wgpu::Backends::GL,
            Backend::Dx12 => wgpu::Backends::DX12,
            Backend::Vulkan => wgpu::Backends::VULKAN,
            Backend::Metal => wgpu::Backends::METAL,
            Backend::Gl => wgpu::Backends::GL,
        }
    }

    /// The backends this platform offers, most reliable first. Vulkan comes
    /// last on Windows: it crashes with some older Intel drivers.
    pub fn available() -> &'static [Backend] {
        if cfg!(windows) {
            &[Backend::Dx12, Backend::Gl, Backend::Vulkan]
        } else if cfg!(target_os = "macos") {
            &[Backend::Metal]
        } else {
            &[Backend::Vulkan, Backend::Gl]
        }
    }
}

fn pref_file() -> PathBuf {
    crate::library::data_dir().join("gpu_backend.txt")
}

fn state_file() -> PathBuf {
    crate::library::data_dir().join("gpu_start.txt")
}

/// The user's saved choice.
pub fn load_pref() -> Backend {
    std::fs::read_to_string(pref_file())
        .ok()
        .and_then(|s| Backend::from_key(&s))
        .filter(|b| *b == Backend::Auto || Backend::available().contains(b))
        .unwrap_or(Backend::Auto)
}

/// Remember the choice (used from the next start). Choosing Automatic
/// again starts over from the most reliable backend.
pub fn save_pref(b: Backend) {
    let _ = std::fs::create_dir_all(crate::library::data_dir());
    let _ = std::fs::write(pref_file(), b.key());
    if b == Backend::Auto {
        let _ = std::fs::remove_file(state_file());
    }
}

/// The automatic pick being tried in this run, and the one that failed
/// before it.
static CURRENT: std::sync::Mutex<(Option<Backend>, Option<Backend>)> =
    std::sync::Mutex::new((None, None));

/// Decide the backends for this start. `WGPU_BACKEND` still wins.
pub fn choose() -> wgpu::Backends {
    if let Some(b) = wgpu::Backends::from_env() {
        return b;
    }
    let pref = load_pref();
    if pref != Backend::Auto {
        return pref.wgpu();
    }
    let list = Backend::available();
    let last = std::fs::read_to_string(state_file()).unwrap_or_default();
    let mut words = last.split_whitespace();
    let (tried, status) = (words.next().and_then(Backend::from_key), words.next());
    let (pick, failed) = match (tried, status) {
        (Some(b), Some("ok")) if list.contains(&b) => (b, None),
        (Some(b), Some("trying")) => {
            // Move on to the next one; after the last, start over.
            let i = list
                .iter()
                .position(|x| *x == b)
                .map_or(0, |i| (i + 1) % list.len());
            (list[i], Some(b))
        }
        _ => (list[0], None),
    };
    let _ = std::fs::create_dir_all(crate::library::data_dir());
    let _ = std::fs::write(state_file(), format!("{} trying", pick.key()));
    log::info!("graphics backend: {} (automatic)", pick.label());
    *CURRENT.lock().unwrap() = (Some(pick), failed);
    pick.wgpu()
}

/// Whether Automatic is still trying backends this run.
pub fn trying() -> bool {
    CURRENT.lock().unwrap().0.is_some()
}

/// Call once the app has drawn a few frames and compiled its pipelines:
/// the backend works. Safe from any thread.
pub fn started_ok() {
    if let Some(b) = CURRENT.lock().unwrap().0.take() {
        let _ = std::fs::write(state_file(), format!("{} ok", b.key()));
    }
}

/// The backend that failed to start last time, if Automatic moved on.
pub fn failed_last_time() -> Option<Backend> {
    CURRENT.lock().unwrap().1
}

/// How long Automatic waits for the first frames and the pipelines before
/// it gives up on a backend that hangs (e.g. compiling shaders).
const START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// If the automatic backend has not drawn its first frames in time, start
/// the app again (which moves on to the next backend) and quit this one.
pub fn watchdog(args: Vec<String>) {
    if !trying() {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(START_TIMEOUT);
        if !trying() {
            return;
        }
        let retries: usize = std::env::var("EZ2_GPU_RETRY")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if retries + 1 >= Backend::available().len() {
            return;
        }
        eprintln!("Graphics start-up is taking too long; trying another backend…");
        if let Ok(exe) = std::env::current_exe() {
            let spawned = std::process::Command::new(exe)
                .args(&args)
                .env("EZ2_GPU_RETRY", (retries + 1).to_string())
                .spawn();
            if spawned.is_ok() {
                std::process::exit(1);
            }
        }
    });
}
