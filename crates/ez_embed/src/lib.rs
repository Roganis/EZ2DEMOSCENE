//! EZ2DEMOSCENE as a library: another program hands over its OpenGL
//! context and a texture, and gets a scene drawn into that texture.
//!
//! Built for EZ2PORT's scene renderer (its `SCENE-RENDERER.md`). The C side
//! is `scene/ez2embed.h` in the port; the one exported symbol is
//! [`ez2embed_api`], a versioned table, so a host can refuse a library it
//! does not understand instead of crashing on a missing symbol.
//!
//! wgpu runs its GL backend on the HOST'S context (`new_external`), so the
//! picture never leaves the GPU: the scene renders into its own target and
//! the `display` texture (gamma-encoded RGBA8, what a plain GL_RGBA8
//! texture holds) is copied into the host's texture. The host's context
//! must be current on every call, and wgpu leaves GL state behind it: the
//! host re-establishes its own after `render`. The reverse holds too:
//! wgpu-hal binds its one vertex array object when the device opens and
//! never again, so after the host has bound its own, wgpu's next draw
//! would rewrite the HOST'S attribute layout. [`Host::enter`] binds
//! wgpu's back before every call that can reach GL.
//!
//! Every entry point catches panics, so an error is a return value and
//! never an unwind into C.

use std::ffi::{c_char, c_void, CStr, CString};
use std::num::NonZeroU32;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use ez_core::clock::EvalCtx;
use ez_core::scene::Project;
use ez_render::{RenderTarget, Renderer, DISPLAY_FORMAT};
use wgpu::hal::api::Gles;

/// Bumped when an entry changes meaning or disappears; entries are only
/// ever appended within one ABI.
pub const EZS_ABI: u32 = 1;

pub type GetProc = unsafe extern "C" fn(name: *const c_char) -> *mut c_void;

/// The host's clock for one frame (`EzsClock` in ez2embed.h).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Clock {
    /// Audio clock in ms, what the player hears; negative = no song.
    pub song_ms: f64,
    /// Beats since the chart's origin, fractional (used when `song_ms >= 0`).
    pub beat: f64,
    /// The tempo now; <= 1 = the scene's own.
    pub bpm: f32,
    /// Milliseconds since the last frame, for a free-running scene.
    pub dt_ms: f32,
}

/// The table (`EzsApi` in ez2embed.h). Field order is the ABI.
#[repr(C)]
pub struct Api {
    pub abi: u32,
    pub size: u32,
    pub host_new: unsafe extern "C" fn(GetProc) -> *mut Host,
    pub host_free: unsafe extern "C" fn(*mut Host),
    pub scene_open: unsafe extern "C" fn(*mut Host, *const c_char) -> *mut Scene,
    pub scene_free: unsafe extern "C" fn(*mut Scene),
    pub set_clock: unsafe extern "C" fn(*mut Scene, *const Clock),
    pub set_signal: unsafe extern "C" fn(*mut Scene, u32, f32),
    pub hit: unsafe extern "C" fn(*mut Scene, u32, i32, f64),
    pub render: unsafe extern "C" fn(*mut Scene, u32, i32, i32) -> i32,
    pub last_error: unsafe extern "C" fn(*mut Host) -> *const c_char,
}

static API: Api = Api {
    abi: EZS_ABI,
    size: std::mem::size_of::<Api>() as u32,
    host_new,
    host_free,
    scene_open,
    scene_free,
    set_clock,
    set_signal,
    hit,
    render,
    last_error,
};

/// The one exported symbol.
#[no_mangle]
pub extern "C" fn ez2embed_api() -> *const Api {
    &API
}

/// One per GL context: the wgpu device over it and the shared renderer
/// (compiling the pipelines is the expensive part, so scenes share it).
pub struct Host {
    /// The first wgpu error since the last check (see make_host).
    gpu_error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    /// wgpu-hal's own vertex array, bound once at device open (see the top).
    wgpu_vao: u32,
    bind_vertex_array: unsafe extern "C" fn(u32),
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    error: CString,
    // Kept for the device's lifetime.
    _adapter: wgpu::Adapter,
    _instance: wgpu::Instance,
}

impl Host {
    /// Put back what wgpu-hal assumes is still bound since the device opened.
    fn enter(&self) {
        unsafe { (self.bind_vertex_array)(self.wgpu_vao) };
    }

    fn fail(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        log::error!("ez2embed: {msg}");
        self.error = CString::new(msg.replace('\0', " ")).unwrap_or_default();
    }
}

/// One loaded project with its own target and clock.
pub struct Scene {
    host: *mut Host,
    project: Project,
    target: Option<RenderTarget>,
    /// The host texture wrapped for wgpu, and what it was wrapped as.
    wrapped: Option<(u32, u32, u32, wgpu::Texture)>,
    clock: Clock,
    /// Beats run on the free clock, when there is no song.
    free_beat: f64,
    /// Whole beats added to the chart's beat since the song started.
    song_offset: Option<f64>,
    frames: u64,
}

fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

fn panic_text(p: Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".into())
}

unsafe extern "C" fn host_new(get_proc: GetProc) -> *mut Host {
    if std::env::var_os("EZ2_EMBED_LOG").is_some() {
        let _ = env_logger::Builder::from_env(
            env_logger::Env::default().filter_or("EZ2_EMBED_LOG", "info"),
        )
        .try_init();
    }
    guard(std::ptr::null_mut(), || match make_host(get_proc) {
        Ok(h) => Box::into_raw(Box::new(h)),
        Err(e) => {
            log::error!("ez2embed: {e}");
            LAST_OPEN_ERROR.with(|s| *s.borrow_mut() = CString::new(e).unwrap_or_default());
            std::ptr::null_mut()
        }
    })
}

thread_local! {
    /// Why host_new failed, for last_error(NULL).
    static LAST_OPEN_ERROR: std::cell::RefCell<CString> = std::cell::RefCell::new(CString::default());
}

const GL_VERTEX_ARRAY_BINDING: u32 = 0x85B5;

/// The host's current WGL context and DC, and a function that makes them
/// current again (the wgl* entry points come through the host's loader,
/// which falls back to opengl32's exports).
#[cfg(windows)]
fn wgl_current(
    proc: &dyn Fn(&str) -> *mut c_void,
) -> Result<impl FnOnce() -> Result<(), String>, String> {
    let (ctx, dc, make) = (
        proc("wglGetCurrentContext"),
        proc("wglGetCurrentDC"),
        proc("wglMakeCurrent"),
    );
    if ctx.is_null() || dc.is_null() || make.is_null() {
        return Err(
            "the host's loader has no wglGetCurrentContext / wglGetCurrentDC / wglMakeCurrent"
                .into(),
        );
    }
    // SAFETY: WGL's own signatures.
    let ctx: unsafe extern "system" fn() -> *mut c_void = unsafe { std::mem::transmute(ctx) };
    let dc: unsafe extern "system" fn() -> *mut c_void = unsafe { std::mem::transmute(dc) };
    let make: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32 =
        unsafe { std::mem::transmute(make) };
    let (c, d) = unsafe { (ctx(), dc()) };
    if c.is_null() {
        return Err("no GL context is current".into());
    }
    Ok(move || {
        if unsafe { make(d, c) } == 0 {
            Err("could not make the host's GL context current again".into())
        } else {
            Ok(())
        }
    })
}

fn make_host(get_proc: GetProc) -> Result<Host, String> {
    let proc = |name: &str| {
        let c = CString::new(name).unwrap_or_default();
        unsafe { get_proc(c.as_ptr()) }
    };
    let get_integerv = proc("glGetIntegerv");
    let bind_vertex_array = proc("glBindVertexArray");
    if get_integerv.is_null() || bind_vertex_array.is_null() {
        return Err("the context has no glGetIntegerv / glBindVertexArray".into());
    }
    // SAFETY: GL entry points of the current context, with GL's signatures.
    let get_integerv: unsafe extern "C" fn(u32, *mut i32) =
        unsafe { std::mem::transmute(get_integerv) };
    let bind_vertex_array: unsafe extern "C" fn(u32) =
        unsafe { std::mem::transmute(bind_vertex_array) };
    // On Windows, making the GL instance opens a hidden WGL context of its
    // own to ask the driver what it has, and leaves the thread with NO
    // current context: the host's must be made current again before wgpu
    // borrows it (EGL on Linux leaves it alone).
    #[cfg(windows)]
    let restore = wgl_current(&proc)?;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::GL,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    #[cfg(windows)]
    restore()?;
    // SAFETY: the host's context is current (the documented contract).
    let exposed = unsafe {
        wgpu::hal::gles::Adapter::new_external(
            |name| {
                let c = CString::new(name).unwrap_or_default();
                get_proc(c.as_ptr()) as *const c_void
            },
            wgpu::GlBackendOptions::default(),
        )
    }
    .ok_or("wgpu refused the host's GL context")?;
    // SAFETY: the adapter was made for this instance's GL backend.
    let adapter = unsafe { instance.create_adapter_from_hal::<Gles>(exposed) };
    let info = adapter.get_info();
    log::info!(
        "ez2embed: {} / {} ({:?})",
        info.name,
        info.driver_info,
        info.backend
    );
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("ez2embed"),
        // Never more than the context has: a GL 3.3 context has no compute,
        // which is how the renderer knows to take its CPU paths.
        required_limits: wgpu::Limits::default().or_worse_values_from(&adapter.limits()),
        ..Default::default()
    }))
    .map_err(|e| format!("creating the device: {e}"))?;
    // Opening the device bound wgpu-hal's vertex array: remember it.
    let mut wgpu_vao = 0i32;
    unsafe { get_integerv(GL_VERTEX_ARRAY_BINDING, &mut wgpu_vao) };
    // wgpu reports validation errors here, not as return values: keep the
    // first one so the render that caused it can fail and the host fall back.
    let gpu_error = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    {
        let slot = gpu_error.clone();
        device.on_uncaptured_error(std::sync::Arc::new(move |e| {
            log::error!("ez2embed: wgpu: {e}");
            let mut g = slot.lock().unwrap_or_else(|p| p.into_inner());
            if g.is_none() {
                *g = Some(e.to_string());
            }
        }));
    }
    let mut renderer = Renderer::new(&device, &queue, 1);
    // Live: never block a frame on a simulation bake; it appears when done.
    renderer.set_wait_for_bakes(false);
    // Errors from building the renderer (a pipeline this GL cannot compile)
    // matter only if a scene uses it; start each render with a clean slate.
    gpu_error.lock().unwrap_or_else(|p| p.into_inner()).take();
    Ok(Host {
        gpu_error,
        wgpu_vao: wgpu_vao as u32,
        bind_vertex_array,
        device,
        queue,
        renderer,
        error: CString::default(),
        _adapter: adapter,
        _instance: instance,
    })
}

unsafe extern "C" fn host_free(host: *mut Host) {
    if !host.is_null() {
        unsafe { &*host }.enter();
        guard((), || drop(unsafe { Box::from_raw(host) }));
    }
}

/// A project file (`.ez2.json`) or, if no such file, a built-in preset name.
unsafe extern "C" fn scene_open(host: *mut Host, spec: *const c_char) -> *mut Scene {
    if host.is_null() || spec.is_null() {
        return std::ptr::null_mut();
    }
    let h = unsafe { &mut *host };
    let spec = unsafe { CStr::from_ptr(spec) }
        .to_string_lossy()
        .into_owned();
    match catch_unwind(AssertUnwindSafe(|| open_project(&spec))) {
        Ok(Ok(project)) => Box::into_raw(Box::new(Scene {
            host,
            project,
            target: None,
            wrapped: None,
            clock: Clock {
                song_ms: -1.0,
                ..Default::default()
            },
            free_beat: 0.0,
            song_offset: None,
            frames: 0,
        })),
        Ok(Err(e)) => {
            h.fail(e);
            std::ptr::null_mut()
        }
        Err(p) => {
            h.fail(format!("opening {spec}: {}", panic_text(p)));
            std::ptr::null_mut()
        }
    }
}

fn open_project(spec: &str) -> Result<Project, String> {
    let path = Path::new(spec);
    if path.is_file() {
        return Project::load(path).map_err(|e| format!("{spec}: {e}"));
    }
    ez_core::presets::by_name(spec).ok_or_else(|| format!("{spec}: no such file or preset"))
}

unsafe extern "C" fn scene_free(scene: *mut Scene) {
    if !scene.is_null() {
        unsafe { &*(*scene).host }.enter();
        guard((), || drop(unsafe { Box::from_raw(scene) }));
    }
}

unsafe extern "C" fn set_clock(scene: *mut Scene, clock: *const Clock) {
    if let (Some(s), Some(c)) = (unsafe { scene.as_mut() }, unsafe { clock.as_ref() }) {
        s.clock = *c;
    }
}

/// Step 2 of the design; accepted and ignored in ABI 1's first build.
unsafe extern "C" fn set_signal(_scene: *mut Scene, _id: u32, _value: f32) {}
unsafe extern "C" fn hit(_scene: *mut Scene, _kind: u32, _lane: i32, _song_ms: f64) {}

/// Draw the scene at its clock into the host's GL texture `gl_texture`
/// (GL_RGBA8, `w` x `h`). 0 = drawn; otherwise last_error says why.
unsafe extern "C" fn render(scene: *mut Scene, gl_texture: u32, w: i32, h: i32) -> i32 {
    let Some(s) = (unsafe { scene.as_mut() }) else {
        return -1;
    };
    let host = unsafe { &mut *s.host };
    host.enter();
    match catch_unwind(AssertUnwindSafe(|| render_into(s, host, gl_texture, w, h))) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            host.fail(e);
            -1
        }
        Err(p) => {
            host.fail(format!("render: {}", panic_text(p)));
            -2
        }
    }
}

fn render_into(
    s: &mut Scene,
    host: &mut Host,
    gl_texture: u32,
    w: i32,
    h: i32,
) -> Result<(), String> {
    let name = NonZeroU32::new(gl_texture).ok_or("texture 0")?;
    if w <= 0 || h <= 0 {
        return Err(format!("size {w}x{h}"));
    }
    let (w, h) = (w as u32, h as u32);

    // The clock. In a song the chart's beat and tempo drive the loop, so a
    // four-bar scene turns once every four bars of THIS song, on the beat.
    let c = s.clock;
    if c.bpm > 1.0 && (c.bpm - s.project.timing.bpm).abs() > 0.01 {
        s.project.timing.bpm = c.bpm;
    }
    // Off the song (menus, the ready count) the scene runs free; when the
    // song starts it joins the chart's beat a whole number of beats on
    // from where it was, so it neither jumps back nor loses the beat.
    let beat = if c.song_ms >= 0.0 {
        if s.song_offset.is_none() {
            s.song_offset = Some((s.free_beat - c.beat).round());
        }
        c.beat + s.song_offset.unwrap_or(0.0)
    } else {
        s.song_offset = None;
        s.free_beat += c.dt_ms.max(0.0) as f64 / 1000.0 * s.project.timing.bpm as f64 / 60.0;
        s.free_beat
    };
    let loop_beats = s.project.timing.loop_beats.max(1) as f64;
    let phase = (beat / loop_beats).rem_euclid(1.0) as f32;
    let ctx = EvalCtx::new(&s.project.timing, phase, None);

    if s.target
        .as_ref()
        .is_none_or(|t| t.width != w || t.height != h)
    {
        s.target = Some(host.renderer.create_target(w, h));
    }
    let target = s.target.as_ref().unwrap();
    // EZ2_EMBED_DUMP=file.png saves the 60th frame as wgpu drew it, before
    // the copy into the host's texture: the way to tell a wrong picture from
    // a right picture drawn wrong by the host.
    if let Some(p) = std::env::var_os("EZ2_EMBED_DUMP") {
        s.frames += 1;
        if s.frames == 60 {
            let img = host.renderer.render_image(&s.project, &ctx, target);
            let _ = img.save(p);
        }
    }
    host.renderer.render(&s.project, &ctx, target);

    if s.wrapped
        .as_ref()
        .is_none_or(|(n, ww, hh, _)| *n != gl_texture || *ww != w || *hh != h)
    {
        s.wrapped = Some((
            gl_texture,
            w,
            h,
            wrap_host_texture(&host.device, name, w, h)?,
        ));
    }
    let dst = &s.wrapped.as_ref().unwrap().3;
    let mut enc = host
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("ez2embed copy"),
        });
    enc.copy_texture_to_texture(
        target.display.as_image_copy(),
        dst.as_image_copy(),
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    host.queue.submit([enc.finish()]);
    if let Some(e) = host
        .gpu_error
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
    {
        return Err(format!("wgpu: {}", e.lines().next().unwrap_or("error")));
    }
    Ok(())
}

/// The host's texture as a wgpu texture. The host keeps owning it: the
/// drop callback does nothing, so wgpu never deletes the GL name.
fn wrap_host_texture(
    device: &wgpu::Device,
    name: NonZeroU32,
    w: u32,
    h: u32,
) -> Result<wgpu::Texture, String> {
    let size = wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    };
    let hal_desc = wgpu::hal::TextureDescriptor {
        label: Some("ez2embed host texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DISPLAY_FORMAT,
        usage: wgpu::TextureUses::COPY_DST,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: vec![],
    };
    // SAFETY: the host created `name` as a GL_RGBA8 2D texture of w x h
    // with one level, and keeps it alive until the scene is freed or the
    // texture is re-wrapped.
    let hal_dev = unsafe { device.as_hal::<Gles>() }.ok_or("not a GL device")?;
    let hal_tex = unsafe { hal_dev.texture_from_raw(name, &hal_desc, Some(Box::new(|| {}))) };
    drop(hal_dev);
    let desc = wgpu::TextureDescriptor {
        label: Some("ez2embed host texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DISPLAY_FORMAT,
        usage: wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    };
    Ok(unsafe {
        device.create_texture_from_hal::<Gles>(hal_tex, &desc, wgpu::TextureUses::UNINITIALIZED)
    })
}

unsafe extern "C" fn last_error(host: *mut Host) -> *const c_char {
    match unsafe { host.as_ref() } {
        Some(h) => h.error.as_ptr(),
        None => LAST_OPEN_ERROR.with(|s| s.borrow().as_ptr()),
    }
}
