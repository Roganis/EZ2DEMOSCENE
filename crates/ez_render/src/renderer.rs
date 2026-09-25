use crate::import::load_mesh;
use crate::mesh::{primitive, MeshData, Vertex};
use crate::texgen;
use bytemuck::{Pod, Zeroable};
use ez_core::eval::{
    instances_are_static, layer_matrix, mesh_instances, symmetry_matrices, Instance,
};
use ez_core::palette::PaletteId;
use ez_core::*;
use glam::{Mat4, Vec3, Vec4};
use image::RgbaImage;
use std::borrow::Cow;
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use wgpu::util::DeviceExt;

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Storage format of the final image (holds sRGB-encoded values).
pub const OUTPUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// View format to *display* the final image with (decodes sRGB on sampling).
pub const OUTPUT_VIEW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

const DRAW_SLOT: u64 = 256;
const POST_SLOT: u64 = 512;
const POST_SLOTS: u64 = 32;
const BLOOM_LEVELS: usize = 5;

// Post uniform slots.
const SLOT_BLUR_H: u32 = 0;
const SLOT_BLUR_V: u32 = 1;
const SLOT_WARP: u32 = 2;
const SLOT_BLOOM_DOWN: u32 = 3; // .. +BLOOM_LEVELS
const SLOT_BLOOM_UP: u32 = 8; // .. +BLOOM_LEVELS
const SLOT_FINAL: u32 = 14;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlobalsRaw {
    view_proj: [[f32; 4]; 4],
    inv_view_proj: [[f32; 4]; 4],
    view: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    cam_right: [f32; 4],
    cam_up: [f32; 4],
    time: [f32; 4],
    res: [f32; 4],
    fog: [f32; 4],
    sky: [f32; 4],
    ground: [f32; 4],
    light_dir: [f32; 4],
    light_color: [f32; 4],
    clip: [f32; 4],
    audio: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceRaw {
    model: [[f32; 4]; 4],
    inst: [f32; 4],
}

type Block = [[f32; 4]; 16];
type PostBlock = [[f32; 4]; 32];

struct GpuMesh {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    count: u32,
}

struct GpuTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

#[derive(Clone)]
enum Cmd {
    Backdrop {
        slot: u32,
        tex: String,
    },
    Mesh {
        slot: u32,
        mesh: String,
        tex: String,
        pixelated: bool,
        first: u32,
        count: u32,
    },
    Particles {
        slot: u32,
        count: u32,
    },
}

/// Rendering cost of one layer in the last frame.
#[derive(Clone, Debug, Default)]
pub struct LayerStats {
    /// Index in the rendered layer list.
    pub index: usize,
    pub name: String,
    pub triangles: u64,
    pub particles: u64,
    pub draws: u32,
    /// Rough relative GPU cost (1.0 ≈ a heavy layer on a mid-range GPU).
    pub load: f32,
}

/// What the last frame drew.
#[derive(Clone, Debug, Default)]
pub struct FrameStats {
    pub layers: Vec<LayerStats>,
    pub triangles: u64,
    pub particles: u64,
    pub draw_calls: u32,
    pub reflection: bool,
    /// Layers whose instances came from the cache.
    pub cached_layers: u32,
    pub load: f32,
}

static TARGET_IDS: AtomicU64 = AtomicU64::new(1);

fn backdrop_load(kind: BackdropKind) -> f32 {
    match kind {
        BackdropKind::Fractal => 1.5,
        BackdropKind::Tunnel => 0.6,
        BackdropKind::Nebula => 0.4,
        BackdropKind::Starfield => 0.3,
        BackdropKind::SynthGrid => 0.2,
        BackdropKind::Plasma => 0.15,
        BackdropKind::Gradient => 0.05,
    }
}

fn layer_hash(layer: &Layer) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(layer)
        .unwrap_or_default()
        .hash(&mut h);
    h.finish()
}

struct ScenePipes {
    backdrop: wgpu::RenderPipeline,
    mesh: wgpu::RenderPipeline,
    particles: wgpu::RenderPipeline,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    msaa: u32,

    bgl_tex: wgpu::BindGroupLayout,
    bgl_floor: wgpu::BindGroupLayout,
    bgl_post: wgpu::BindGroupLayout,

    globals_buf: [wgpu::Buffer; 2],
    globals_bg: [wgpu::BindGroup; 2],
    bgl_draw: wgpu::BindGroupLayout,
    draw_buf: wgpu::Buffer,
    draw_cap: u64,
    draw_bg: wgpu::BindGroup,
    post_buf: wgpu::Buffer,
    inst_buf: wgpu::Buffer,
    inst_cap: u64,

    main_pipes: ScenePipes,
    refl_pipes: ScenePipes,
    floor_pipe: wgpu::RenderPipeline,
    blur_pipe: wgpu::RenderPipeline,
    warp_pipe: wgpu::RenderPipeline,
    bloom_down_pipe: wgpu::RenderPipeline,
    bloom_up_pipe: wgpu::RenderPipeline,
    final_pipe: wgpu::RenderPipeline,

    sampler_repeat: wgpu::Sampler,
    sampler_nearest: wgpu::Sampler,
    sampler_clamp: wgpu::Sampler,

    meshes: HashMap<String, GpuMesh>,
    textures: HashMap<String, GpuTexture>,
    tex_bgs: HashMap<(String, bool), wgpu::BindGroup>,
    /// Asset loading problems (shown in the UI), keyed by asset.
    pub errors: HashMap<String, String>,

    /// Instances of layers that don't animate, keyed by layer hash, with
    /// the frame number they were last used.
    instance_cache: HashMap<u64, (u64, Vec<InstanceRaw>)>,
    frame_no: u64,
    /// Instance data currently in `inst_buf` (skip identical uploads).
    uploaded: Vec<InstanceRaw>,
    floor_bg_cache: Option<((String, u64), wgpu::BindGroup)>,
    stats: FrameStats,
}

/// All size-dependent GPU resources for one output image.
pub struct RenderTarget {
    id: u64,
    pub width: u32,
    pub height: u32,
    msaa_color: Option<wgpu::TextureView>,
    depth: wgpu::TextureView,
    hdr: wgpu::TextureView,
    hdr2: wgpu::TextureView,
    bloom: Vec<(wgpu::TextureView, u32, u32)>,
    refl: wgpu::TextureView,
    refl_depth: wgpu::TextureView,
    refl_tmp: wgpu::TextureView,
    refl_blur: wgpu::TextureView,
    refl_size: (u32, u32),
    pub output: wgpu::Texture,
    pub output_view: wgpu::TextureView,
    /// sRGB view of the output, for displaying it (e.g. in egui).
    pub display_view: wgpu::TextureView,
    bg_blur_h: wgpu::BindGroup,
    bg_blur_v: wgpu::BindGroup,
    bg_warp: wgpu::BindGroup,
    bg_bloom_down: Vec<wgpu::BindGroup>,
    bg_bloom_up: Vec<wgpu::BindGroup>,
    bg_final: wgpu::BindGroup,
}

fn m4(m: Mat4) -> [[f32; 4]; 4] {
    m.to_cols_array_2d()
}

fn v4(v: Vec3, w: f32) -> [f32; 4] {
    [v.x, v.y, v.z, w]
}

fn c4(c: [f32; 3], w: f32) -> [f32; 4] {
    [c[0], c[1], c[2], w]
}

fn uniform_entry(binding: u32, dynamic: bool, size: u64) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: dynamic,
            min_binding_size: wgpu::BufferSize::new(size),
        },
        count: None,
    }
}

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

struct PipeDesc<'a> {
    label: &'a str,
    layout: &'a wgpu::PipelineLayout,
    module: &'a wgpu::ShaderModule,
    fs: &'a str,
    buffers: &'a [Option<wgpu::VertexBufferLayout<'a>>],
    format: wgpu::TextureFormat,
    samples: u32,
    depth: Option<(bool, wgpu::CompareFunction)>,
    blend: Option<wgpu::BlendState>,
}

fn make_pipeline(device: &wgpu::Device, d: PipeDesc) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(d.label),
        layout: Some(d.layout),
        vertex: wgpu::VertexState {
            module: d.module,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: d.buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: d.depth.map(|(write, cmp)| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(cmp),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: d.samples,
            ..Default::default()
        },
        fragment: Some(wgpu::FragmentState {
            module: d.module,
            entry_point: Some(d.fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: d.format,
                blend: d.blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

const ADDITIVE: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

fn shader(device: &wgpu::Device, label: &str, src: &str, with_common: bool) -> wgpu::ShaderModule {
    let code = if with_common {
        format!("{}\n{}", include_str!("shaders/common.wgsl"), src)
    } else {
        src.to_string()
    };
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(code)),
    })
}

impl Renderer {
    /// `msaa` must be 1 or 4.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, msaa: u32) -> Renderer {
        let msaa = if msaa >= 4 { 4 } else { 1 };
        let globals_size = std::mem::size_of::<GlobalsRaw>() as u64;
        let bgl_globals = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[uniform_entry(0, false, globals_size)],
        });
        let bgl_draw = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw"),
            entries: &[uniform_entry(0, true, DRAW_SLOT)],
        });
        let bgl_tex = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tex"),
            entries: &[tex_entry(0), sampler_entry(1)],
        });
        let bgl_floor = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("floor"),
            entries: &[
                tex_entry(0),
                sampler_entry(1),
                tex_entry(2),
                sampler_entry(3),
            ],
        });
        let bgl_post = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[
                uniform_entry(0, true, POST_SLOT),
                tex_entry(1),
                tex_entry(2),
                sampler_entry(3),
            ],
        });

        let globals_buf = [0, 1].map(|i| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(if i == 0 {
                    "globals main"
                } else {
                    "globals refl"
                }),
                size: globals_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let globals_bg = [0, 1].map(|i| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("globals"),
                layout: &bgl_globals,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf[i].as_entire_binding(),
                }],
            })
        });
        let draw_cap = 64;
        let draw_buf = Self::make_draw_buf(device, draw_cap);
        let draw_bg = Self::make_draw_bg(device, &bgl_draw, &draw_buf);
        let post_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("post params"),
            size: POST_SLOT * POST_SLOTS,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let inst_cap = 1024;
        let inst_buf = Self::make_inst_buf(device, inst_cap);

        let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene"),
            bind_group_layouts: &[Some(&bgl_globals), Some(&bgl_draw), Some(&bgl_tex)],
            immediate_size: 0,
        });
        let particle_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particles"),
            bind_group_layouts: &[Some(&bgl_globals), Some(&bgl_draw)],
            immediate_size: 0,
        });
        let floor_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("floor"),
            bind_group_layouts: &[Some(&bgl_globals), Some(&bgl_draw), Some(&bgl_floor)],
            immediate_size: 0,
        });
        let post_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("post"),
            bind_group_layouts: &[Some(&bgl_post)],
            immediate_size: 0,
        });

        let sh_backdrop = shader(
            device,
            "backdrop",
            include_str!("shaders/backdrop.wgsl"),
            true,
        );
        let sh_mesh = shader(device, "mesh", include_str!("shaders/mesh.wgsl"), true);
        let sh_particles = shader(
            device,
            "particles",
            include_str!("shaders/particles.wgsl"),
            true,
        );
        let sh_floor = shader(device, "floor", include_str!("shaders/floor.wgsl"), true);
        let sh_post = shader(device, "post", include_str!("shaders/post.wgsl"), false);

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32],
        };
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4],
        };
        let mesh_buffers = [Some(vertex_layout), Some(instance_layout)];

        let scene_pipes = |samples: u32| ScenePipes {
            backdrop: make_pipeline(
                device,
                PipeDesc {
                    label: "backdrop",
                    layout: &scene_layout,
                    module: &sh_backdrop,
                    fs: "fs_main",
                    buffers: &[],
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((false, wgpu::CompareFunction::Always)),
                    blend: None,
                },
            ),
            mesh: make_pipeline(
                device,
                PipeDesc {
                    label: "mesh",
                    layout: &scene_layout,
                    module: &sh_mesh,
                    fs: "fs_main",
                    buffers: &mesh_buffers,
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((true, wgpu::CompareFunction::Less)),
                    blend: None,
                },
            ),
            particles: make_pipeline(
                device,
                PipeDesc {
                    label: "particles",
                    layout: &particle_layout,
                    module: &sh_particles,
                    fs: "fs_main",
                    buffers: &[],
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((false, wgpu::CompareFunction::Less)),
                    blend: Some(ADDITIVE),
                },
            ),
        };
        let main_pipes = scene_pipes(msaa);
        let refl_pipes = scene_pipes(1);
        let floor_pipe = make_pipeline(
            device,
            PipeDesc {
                label: "floor",
                layout: &floor_layout,
                module: &sh_floor,
                fs: "fs_main",
                buffers: &[],
                format: HDR_FORMAT,
                samples: msaa,
                depth: Some((true, wgpu::CompareFunction::Less)),
                blend: None,
            },
        );
        let post_pipe = |label: &str, fs: &str, format, blend| {
            make_pipeline(
                device,
                PipeDesc {
                    label,
                    layout: &post_layout,
                    module: &sh_post,
                    fs,
                    buffers: &[],
                    format,
                    samples: 1,
                    depth: None,
                    blend,
                },
            )
        };
        let blur_pipe = post_pipe("blur", "fs_blur", HDR_FORMAT, None);
        let warp_pipe = post_pipe("warp", "fs_warp", HDR_FORMAT, None);
        let bloom_down_pipe = post_pipe("bloom down", "fs_bloom_down", HDR_FORMAT, None);
        let bloom_up_pipe = post_pipe("bloom up", "fs_bloom_up", HDR_FORMAT, Some(ADDITIVE));
        let final_pipe = post_pipe("final", "fs_final", OUTPUT_FORMAT, None);

        let sampler = |addr, filter| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: None,
                address_mode_u: addr,
                address_mode_v: addr,
                address_mode_w: addr,
                mag_filter: filter,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                anisotropy_clamp: 1,
                ..Default::default()
            })
        };
        let sampler_repeat = sampler(wgpu::AddressMode::Repeat, wgpu::FilterMode::Linear);
        let sampler_nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let sampler_clamp = sampler(wgpu::AddressMode::ClampToEdge, wgpu::FilterMode::Linear);

        let mut r = Renderer {
            device: device.clone(),
            queue: queue.clone(),
            msaa,
            bgl_tex,
            bgl_floor,
            bgl_post,
            globals_buf,
            globals_bg,
            bgl_draw,
            draw_buf,
            draw_cap,
            draw_bg,
            post_buf,
            inst_buf,
            inst_cap,
            main_pipes,
            refl_pipes,
            floor_pipe,
            blur_pipe,
            warp_pipe,
            bloom_down_pipe,
            bloom_up_pipe,
            final_pipe,
            sampler_repeat,
            sampler_nearest,
            sampler_clamp,
            meshes: HashMap::new(),
            textures: HashMap::new(),
            tex_bgs: HashMap::new(),
            errors: HashMap::new(),
            instance_cache: HashMap::new(),
            frame_no: 0,
            uploaded: Vec::new(),
            floor_bg_cache: None,
            stats: FrameStats::default(),
        };
        let white = RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
        r.upload_texture("__white".into(), &white);
        r
    }

    pub fn msaa(&self) -> u32 {
        self.msaa
    }

    fn make_draw_buf(device: &wgpu::Device, slots: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("draw params"),
            size: DRAW_SLOT * slots,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn make_draw_bg(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buf: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("draw"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(DRAW_SLOT),
                }),
            }],
        })
    }

    fn make_inst_buf(device: &wgpu::Device, cap: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: cap * std::mem::size_of::<InstanceRaw>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    // ------------------------------------------------------------------
    // Assets

    fn upload_texture(&mut self, key: String, img: &RgbaImage) {
        let (w, h) = img.dimensions();
        let mips = (32 - w.max(h).max(1).leading_zeros()).max(1);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&key),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: mips,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let mut level = img.clone();
        for mip in 0..mips {
            let (lw, lh) = level.dimensions();
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &level,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * lw),
                    rows_per_image: Some(lh),
                },
                wgpu::Extent3d {
                    width: lw,
                    height: lh,
                    depth_or_array_layers: 1,
                },
            );
            if mip + 1 < mips {
                level = image::imageops::resize(
                    &level,
                    (lw / 2).max(1),
                    (lh / 2).max(1),
                    image::imageops::FilterType::Triangle,
                );
            }
        }
        let view = texture.create_view(&Default::default());
        self.tex_bgs.retain(|(k, _), _| *k != key);
        self.textures.insert(
            key,
            GpuTexture {
                _texture: texture,
                view,
            },
        );
    }

    /// Resolve a material texture name to a loaded GPU texture key.
    fn texture_key(&mut self, project: &Project, name: Option<&str>) -> String {
        let Some(name) = name.filter(|n| !n.is_empty()) else {
            return "__white".into();
        };
        if texgen::is_builtin(name) {
            let key = format!("b:{name}");
            if !self.textures.contains_key(&key) {
                let img = texgen::generate(name);
                self.upload_texture(key.clone(), &img);
            }
            return key;
        }
        let (path, retro) = match project.find_texture(name) {
            Some(t) => (t.path.clone(), t.retro.clone()),
            None => (name.to_string(), None),
        };
        let key = format!("u:{path}:{retro:?}");
        if self.textures.contains_key(&key) {
            return key;
        }
        match image::open(&path) {
            Ok(img) => {
                let mut img = img.to_rgba8();
                if let Some(r) = &retro {
                    img = texgen::retroize(&img, r);
                }
                self.errors.remove(&key);
                self.upload_texture(key.clone(), &img);
                key
            }
            Err(e) => {
                self.errors
                    .insert(key.clone(), format!("texture '{name}' ({path}): {e}"));
                let missing = "b:__missing".to_string();
                if !self.textures.contains_key(&missing) {
                    let img = texgen::generate("__missing");
                    self.upload_texture(missing.clone(), &img);
                }
                missing
            }
        }
    }

    fn tex_bind_group(&mut self, key: &str, nearest: bool) {
        let k = (key.to_string(), nearest);
        if self.tex_bgs.contains_key(&k) {
            return;
        }
        let tex = &self.textures[key];
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tex"),
            layout: &self.bgl_tex,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&tex.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(if nearest {
                        &self.sampler_nearest
                    } else {
                        &self.sampler_repeat
                    }),
                },
            ],
        });
        self.tex_bgs.insert(k, bg);
    }

    fn mesh_key(&mut self, source: &MeshSource) -> String {
        let key = match source {
            MeshSource::Primitive(p) => format!("p:{}", p.cache_key()),
            MeshSource::File { path } => format!("f:{path}"),
        };
        if self.meshes.contains_key(&key) {
            return key;
        }
        let data = match source {
            MeshSource::Primitive(p) => primitive(p),
            MeshSource::File { path } => match load_mesh(Path::new(path)) {
                Ok(m) => {
                    self.errors.remove(&key);
                    m
                }
                Err(e) => {
                    self.errors
                        .insert(key.clone(), format!("model {path}: {e:#}"));
                    primitive(&Primitive::Cube)
                }
            },
        };
        self.upload_mesh(key.clone(), &data);
        key
    }

    fn upload_mesh(&mut self, key: String, data: &MeshData) {
        let vbuf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh vertices"),
                contents: bytemuck::cast_slice(&data.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let ibuf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh indices"),
                contents: bytemuck::cast_slice(&data.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        self.meshes.insert(
            key,
            GpuMesh {
                vbuf,
                ibuf,
                count: data.indices.len() as u32,
            },
        );
    }

    /// Forget a file-based mesh/texture so it is reloaded next frame.
    pub fn reload_assets(&mut self) {
        self.meshes.retain(|k, _| k.starts_with("p:"));
        self.textures.retain(|k, _| !k.starts_with("u:"));
        self.tex_bgs.retain(|(k, _), _| !k.starts_with("u:"));
        self.errors.clear();
        self.floor_bg_cache = None;
    }

    /// Statistics of the last rendered frame.
    pub fn stats(&self) -> &FrameStats {
        &self.stats
    }

    // ------------------------------------------------------------------
    // Frame

    fn globals(
        project: &Project,
        ctx: &EvalCtx,
        view: Mat4,
        proj: Mat4,
        eye: Vec3,
        res: (u32, u32),
        clip: Vec4,
    ) -> GlobalsRaw {
        let env = &project.environment;
        let vp = proj * view;
        let inv_view = view.inverse();
        let right = inv_view.x_axis.truncate().normalize_or(Vec3::X);
        let up = inv_view.y_axis.truncate().normalize_or(Vec3::Y);
        let ld = Vec3::from(env.light_dir).normalize_or(Vec3::Y);
        GlobalsRaw {
            view_proj: m4(vp),
            inv_view_proj: m4(vp.inverse()),
            view: m4(view),
            cam_pos: v4(eye, 1.0),
            cam_right: v4(right, 0.0),
            cam_up: v4(up, 0.0),
            time: [
                ctx.phase,
                ctx.beat(),
                ctx.beat_frac(),
                ctx.loop_beats as f32,
            ],
            res: [
                res.0 as f32,
                res.1 as f32,
                1.0 / res.0 as f32,
                1.0 / res.1 as f32,
            ],
            fog: c4(env.fog_color, env.fog_density.eval(ctx).max(0.0)),
            sky: c4(env.sky_color, env.ambient),
            ground: c4(env.ground_color, env.light_intensity),
            light_dir: v4(ld, 0.0),
            light_color: c4(env.light_color, 1.0),
            clip: clip.into(),
            audio: [ctx.audio, ctx.bass, 0.0, 0.0],
        }
    }

    /// Render one frame of `project` at `ctx` into `target`.
    pub fn render(&mut self, project: &Project, ctx: &EvalCtx, target: &RenderTarget) {
        let layers = project.scene_layers();
        let (w, h) = (target.width, target.height);
        let cam = project.camera.eval(ctx);
        let view = cam.view();
        let proj = cam.proj(w as f32 / h as f32);

        let mut blocks: Vec<Block> = Vec::new();
        let mut instances: Vec<InstanceRaw> = Vec::new();
        let mut cmds: Vec<Cmd> = Vec::new();
        let mut floor: Option<(u32, String, f32, f32)> = None; // slot, tex, height, blur
        let mut scratch: Vec<Instance> = Vec::new();
        self.frame_no += 1;
        let mut stats = FrameStats::default();

        for (li, layer) in layers.iter().enumerate().filter(|(_, l)| l.enabled) {
            let mut ls = LayerStats {
                index: li,
                name: layer.name.clone(),
                ..Default::default()
            };
            match &layer.kind {
                LayerKind::Backdrop(b) => {
                    let tex = self.texture_key(project, b.texture.as_deref());
                    self.tex_bind_group(&tex, false);
                    let mut blk: Block = Zeroable::zeroed();
                    blk[0] = [
                        b.kind.index() as f32,
                        b.speed as f32,
                        b.intensity.eval(ctx),
                        b.detail,
                    ];
                    blk[1] = c4(b.color_a, 0.0);
                    blk[2] = c4(b.color_b, 0.0);
                    blk[3] = c4(b.color_c, 0.0);
                    blk[4] = [if b.texture.is_some() { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0];
                    ls.draws = 1;
                    ls.load = backdrop_load(b.kind);
                    cmds.push(Cmd::Backdrop {
                        slot: blocks.len() as u32,
                        tex,
                    });
                    blocks.push(blk);
                }
                LayerKind::Mesh(m) => {
                    let mesh = self.mesh_key(&m.source);
                    let mat = &m.material;
                    let tex = self.texture_key(project, mat.texture.as_deref());
                    self.tex_bind_group(&tex, mat.pixelated);
                    let first = instances.len() as u32;
                    let to_raw = |i: &Instance| InstanceRaw {
                        model: m4(i.model),
                        inst: [i.hue, i.glow, i.rand, 0.0],
                    };
                    let count = if instances_are_static(layer, m) {
                        let key = layer_hash(layer);
                        let frame = self.frame_no;
                        let entry = self.instance_cache.entry(key).or_insert_with(|| {
                            scratch.clear();
                            mesh_instances(layer, m, ctx, &mut scratch);
                            (frame, scratch.iter().map(to_raw).collect())
                        });
                        entry.0 = frame;
                        instances.extend_from_slice(&entry.1);
                        stats.cached_layers += 1;
                        entry.1.len()
                    } else {
                        scratch.clear();
                        mesh_instances(layer, m, ctx, &mut scratch);
                        instances.extend(scratch.iter().map(to_raw));
                        scratch.len()
                    };
                    let tris = self
                        .meshes
                        .get(&mesh)
                        .map(|g| g.count as u64 / 3)
                        .unwrap_or(0);
                    ls.triangles = tris * count as u64;
                    ls.draws = 1;
                    ls.load = ls.triangles as f32 / 150_000.0 + count as f32 / 20_000.0;
                    let mut blk: Block = Zeroable::zeroed();
                    blk[0] = c4(mat.base_color, mat.metallic);
                    let e = mat.emissive.eval(ctx).max(0.0);
                    blk[1] = c4(color::scale(mat.emissive_color, e), mat.roughness);
                    let mode = EmissiveMode::ALL
                        .iter()
                        .position(|x| *x == mat.emissive_mode)
                        .unwrap_or(0);
                    blk[2] = [
                        mode as f32,
                        if mat.texture.is_some() { 1.0 } else { 0.0 },
                        mat.texture_scale,
                        if mat.flat_shading { 1.0 } else { 0.0 },
                    ];
                    blk[3] = [
                        ctx.phase * mat.scroll[0] as f32,
                        ctx.phase * mat.scroll[1] as f32,
                        mat.rim,
                        mat.hue_shift.eval(ctx),
                    ];
                    cmds.push(Cmd::Mesh {
                        slot: blocks.len() as u32,
                        mesh,
                        tex,
                        pixelated: mat.pixelated,
                        first,
                        count: count as u32,
                    });
                    blocks.push(blk);
                }
                LayerKind::Particles(p) => {
                    let lm = layer_matrix(&layer.transform, ctx);
                    let count = p.count.min(200_000) * (p.trail.min(16) + 1);
                    let syms = symmetry_matrices(&layer.symmetry);
                    ls.particles = count as u64 * syms.len() as u64;
                    ls.draws = syms.len() as u32;
                    ls.load = ls.particles as f32 / 40_000.0;
                    for sym in syms {
                        let mut blk: Block = Zeroable::zeroed();
                        blk[0] = [
                            p.emitter.index() as f32,
                            p.count as f32,
                            p.lifetimes.max(1) as f32,
                            p.seed as f32,
                        ];
                        blk[1] = c4(p.color_a, p.size.eval(ctx).max(0.0));
                        blk[2] = c4(p.color_b, p.intensity.eval(ctx).max(0.0));
                        blk[3] = [p.speed, p.radius, p.trail.min(16) as f32, p.trail_spacing];
                        blk[4] = [p.sprite.index() as f32, 0.0, 0.0, 0.0];
                        let model = m4(sym * lm);
                        blk[8..12].copy_from_slice(&model);
                        cmds.push(Cmd::Particles {
                            slot: blocks.len() as u32,
                            count,
                        });
                        blocks.push(blk);
                    }
                }
                LayerKind::Mirror(f) => {
                    if floor.is_some() {
                        continue; // one mirror floor per scene
                    }
                    let tex = self.texture_key(project, f.texture.as_deref());
                    let mut blk: Block = Zeroable::zeroed();
                    let height = layer.transform.position[1];
                    blk[0] = [f.size.max(0.1), height, f.reflectivity.clamp(0.0, 1.0), 1.0];
                    blk[1] = c4(f.base_color, if f.texture.is_some() { 1.0 } else { 0.0 });
                    blk[2] = c4(f.tint, f.texture_scale);
                    blk[3] = c4(
                        color::scale(f.grid_color, f.grid.eval(ctx).max(0.0)),
                        f.grid_scale,
                    );
                    blk[4] = [-ctx.phase * f.grid_scroll as f32, 0.0, 0.0, 0.0];
                    floor = Some((blocks.len() as u32, tex, height, f.blur));
                    ls.draws = 1;
                    ls.triangles = 2;
                    ls.load = 0.1;
                    blocks.push(blk);
                }
            }
            stats.layers.push(ls);
        }

        // Frame statistics. A mirror floor renders the scene a second time
        // at half resolution.
        stats.reflection = floor.is_some();
        let refl_k = if stats.reflection { 1.5 } else { 1.0 };
        for l in &mut stats.layers {
            l.load *= refl_k;
            stats.triangles += l.triangles;
            stats.particles += l.particles;
            stats.draw_calls += l.draws;
            stats.load += l.load;
        }
        self.stats = stats;
        // Forget cached instances not used for a while.
        let frame = self.frame_no;
        self.instance_cache
            .retain(|_, (used, _)| frame - *used < 120);

        // --- uploads -------------------------------------------------------
        if blocks.is_empty() {
            blocks.push(Zeroable::zeroed());
        }
        if blocks.len() as u64 > self.draw_cap {
            self.draw_cap = (blocks.len() as u64).next_power_of_two();
            self.draw_buf = Self::make_draw_buf(&self.device, self.draw_cap);
            self.draw_bg = Self::make_draw_bg(&self.device, &self.bgl_draw, &self.draw_buf);
        }
        self.queue
            .write_buffer(&self.draw_buf, 0, bytemuck::cast_slice(&blocks));
        let same_as_uploaded = bytemuck::cast_slice::<_, u8>(&instances)
            == bytemuck::cast_slice::<_, u8>(&self.uploaded);
        if !instances.is_empty() && !same_as_uploaded {
            if instances.len() as u64 > self.inst_cap {
                self.inst_cap = (instances.len() as u64).next_power_of_two();
                self.inst_buf = Self::make_inst_buf(&self.device, self.inst_cap);
            }
            self.queue
                .write_buffer(&self.inst_buf, 0, bytemuck::cast_slice(&instances));
            self.uploaded = instances;
        }
        let main_globals = Self::globals(project, ctx, view, proj, cam.eye, (w, h), Vec4::ZERO);
        self.queue
            .write_buffer(&self.globals_buf[0], 0, bytemuck::bytes_of(&main_globals));
        if let Some((_, _, fh, _)) = &floor {
            let mirror = Mat4::from_translation(Vec3::Y * 2.0 * fh)
                * Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0));
            let rview = view * mirror;
            let reye = mirror.transform_point3(cam.eye);
            let g = Self::globals(
                project,
                ctx,
                rview,
                proj,
                reye,
                target.refl_size,
                Vec4::new(0.0, 1.0, 0.0, -fh + 0.001),
            );
            self.queue
                .write_buffer(&self.globals_buf[1], 0, bytemuck::bytes_of(&g));
        }
        self.write_post_params(
            project,
            ctx,
            target,
            floor.as_ref().map(|f| f.3).unwrap_or(0.0),
        );

        // Floor bind group (references the target's reflection texture).
        if let Some((_, tex, _, _)) = &floor {
            let key = (tex.clone(), target.id);
            if self.floor_bg_cache.as_ref().map(|(k, _)| k) != Some(&key) {
                let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("floor"),
                    layout: &self.bgl_floor,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&self.textures[tex].view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler_repeat),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&target.refl_blur),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&self.sampler_clamp),
                        },
                    ],
                });
                self.floor_bg_cache = Some((key, bg));
            }
        }
        let floor_bg = floor
            .as_ref()
            .and(self.floor_bg_cache.as_ref().map(|(_, bg)| bg));

        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ez2 frame"),
            });

        // --- reflection -------------------------------------------------------
        if floor.is_some() {
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("reflection"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.refl,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &target.refl_depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                self.draw_scene(&mut pass, &self.refl_pipes, 1, &cmds);
            }
            self.post_pass(
                &mut enc,
                "blur h",
                &self.blur_pipe,
                &target.refl_tmp,
                &target.bg_blur_h,
                SLOT_BLUR_H,
                false,
            );
            self.post_pass(
                &mut enc,
                "blur v",
                &self.blur_pipe,
                &target.refl_blur,
                &target.bg_blur_v,
                SLOT_BLUR_V,
                false,
            );
        }

        // --- main scene -------------------------------------------------------
        {
            let (view_tex, resolve) = match &target.msaa_color {
                Some(ms) => (ms, Some(&target.hdr)),
                None => (&target.hdr, None),
            };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: view_tex,
                    depth_slice: None,
                    resolve_target: resolve,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: project.environment.fog_color[0] as f64,
                            g: project.environment.fog_color[1] as f64,
                            b: project.environment.fog_color[2] as f64,
                            a: 1.0,
                        }),
                        store: if resolve.is_some() {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // Backdrops first, then the floor, then everything else.
            let (back, rest): (Vec<Cmd>, Vec<Cmd>) = cmds
                .iter()
                .cloned()
                .partition(|c| matches!(c, Cmd::Backdrop { .. }));
            self.draw_scene(&mut pass, &self.main_pipes, 0, &back);
            if let (Some((slot, _, _, _)), Some(bg)) = (&floor, &floor_bg) {
                pass.set_pipeline(&self.floor_pipe);
                pass.set_bind_group(0, &self.globals_bg[0], &[]);
                pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                pass.set_bind_group(2, *bg, &[]);
                pass.draw(0..6, 0..1);
            }
            self.draw_scene(&mut pass, &self.main_pipes, 0, &rest);
        }

        // --- post ---------------------------------------------------------------
        self.post_pass(
            &mut enc,
            "warp",
            &self.warp_pipe,
            &target.hdr2,
            &target.bg_warp,
            SLOT_WARP,
            false,
        );
        if project.post.bloom.enabled {
            for i in 0..BLOOM_LEVELS {
                self.post_pass(
                    &mut enc,
                    "bloom down",
                    &self.bloom_down_pipe,
                    &target.bloom[i].0,
                    &target.bg_bloom_down[i],
                    SLOT_BLOOM_DOWN + i as u32,
                    false,
                );
            }
            for i in (0..BLOOM_LEVELS - 1).rev() {
                self.post_pass(
                    &mut enc,
                    "bloom up",
                    &self.bloom_up_pipe,
                    &target.bloom[i].0,
                    &target.bg_bloom_up[i],
                    SLOT_BLOOM_UP + i as u32,
                    true,
                );
            }
        }
        self.post_pass(
            &mut enc,
            "final",
            &self.final_pipe,
            &target.output_view,
            &target.bg_final,
            SLOT_FINAL,
            false,
        );
        self.queue.submit([enc.finish()]);
    }

    fn draw_scene(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipes: &ScenePipes,
        globals: usize,
        cmds: &[Cmd],
    ) {
        for cmd in cmds {
            match cmd {
                Cmd::Backdrop { slot, tex } => {
                    pass.set_pipeline(&pipes.backdrop);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_bind_group(2, &self.tex_bgs[&(tex.clone(), false)], &[]);
                    pass.draw(0..3, 0..1);
                }
                Cmd::Mesh {
                    slot,
                    mesh,
                    tex,
                    pixelated,
                    first,
                    count,
                } => {
                    if *count == 0 {
                        continue;
                    }
                    let m = &self.meshes[mesh];
                    pass.set_pipeline(&pipes.mesh);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_bind_group(2, &self.tex_bgs[&(tex.clone(), *pixelated)], &[]);
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_vertex_buffer(1, self.inst_buf.slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.count, 0, *first..*first + *count);
                }
                Cmd::Particles { slot, count } => {
                    pass.set_pipeline(&pipes.particles);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.draw(0..6, 0..*count);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn post_pass(
        &self,
        enc: &mut wgpu::CommandEncoder,
        label: &str,
        pipe: &wgpu::RenderPipeline,
        out: &wgpu::TextureView,
        bg: &wgpu::BindGroup,
        slot: u32,
        load: bool,
    ) {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: out,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if load {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipe);
        pass.set_bind_group(0, bg, &[slot * POST_SLOT as u32]);
        pass.draw(0..3, 0..1);
    }

    fn write_post_params(
        &self,
        project: &Project,
        ctx: &EvalCtx,
        target: &RenderTarget,
        blur: f32,
    ) {
        let post = &project.post;
        let mut slots: Vec<PostBlock> = vec![Zeroable::zeroed(); SLOT_FINAL as usize + 1];
        let (rw, rh) = target.refl_size;
        let br = blur.clamp(0.0, 1.0) * 3.0;
        slots[SLOT_BLUR_H as usize][0] = [br / rw as f32, 0.0, 0.0, 0.0];
        slots[SLOT_BLUR_V as usize][0] = [0.0, br / rh as f32, 0.0, 0.0];

        let k = &post.kaleido;
        let mirror_mode = if post.mirror.enabled {
            1 + MirrorSplitMode::ALL
                .iter()
                .position(|m| *m == post.mirror.mode)
                .unwrap_or(0)
        } else {
            0
        };
        slots[SLOT_WARP as usize][0] = [
            if k.enabled { 1.0 } else { 0.0 },
            k.segments.max(1) as f32,
            k.angle.to_radians() + TAU * k.turns as f32 * ctx.phase,
            k.zoom.eval(ctx),
        ];
        slots[SLOT_WARP as usize][1] = [
            k.center[0],
            k.center[1],
            target.width as f32 / target.height as f32,
            mirror_mode as f32,
        ];

        let b = &post.bloom;
        for i in 0..BLOOM_LEVELS {
            let (sw, sh) = if i == 0 {
                (target.width, target.height)
            } else {
                (target.bloom[i - 1].1, target.bloom[i - 1].2)
            };
            slots[(SLOT_BLOOM_DOWN as usize) + i][0] = [
                1.0 / sw as f32,
                1.0 / sh as f32,
                b.threshold.max(0.0),
                if i == 0 { 1.0 } else { 0.0 },
            ];
        }
        let up_w = 0.35 + 0.65 * b.radius.clamp(0.0, 1.0);
        for i in 0..BLOOM_LEVELS - 1 {
            let (_, sw, sh) = target.bloom[i + 1];
            slots[(SLOT_BLOOM_UP as usize) + i][0] = [1.0 / sw as f32, 1.0 / sh as f32, 0.0, 0.0];
            slots[(SLOT_BLOOM_UP as usize) + i][1] = [up_w, 0.0, 0.0, 0.0];
        }

        let g = &post.grade;
        let f = &mut slots[SLOT_FINAL as usize];
        f[0] = [
            target.width as f32,
            target.height as f32,
            1.0 / target.width as f32,
            1.0 / target.height as f32,
        ];
        f[1] = [
            g.exposure.eval(ctx).max(0.0),
            g.contrast,
            g.saturation,
            g.vignette,
        ];
        let bloom_k = if b.enabled {
            b.intensity.eval(ctx).max(0.0) * 0.5
        } else {
            0.0
        };
        f[2] = [
            g.grain,
            g.beat_flash,
            if post.chroma.enabled {
                post.chroma.amount.eval(ctx).max(0.0)
            } else {
                0.0
            },
            bloom_k,
        ];
        // Scale fat pixels relative to 1080p so previews match exports.
        let pix = if post.pixelate.enabled {
            (post.pixelate.size * target.height as f32 / 1080.0 * 2.0).max(1.0)
        } else {
            0.0
        };
        let (count, cols) = if post.palette.enabled {
            if post.palette.palette == PaletteId::Vga {
                (-1.0, vec![])
            } else {
                let c = post.palette.palette.colors_f32();
                (c.len().min(16) as f32, c)
            }
        } else {
            (0.0, vec![])
        };
        f[3] = [pix, count, post.palette.dither, ctx.phase];
        f[4] = [
            if post.crt.enabled { 1.0 } else { 0.0 },
            post.crt.scanlines,
            post.crt.curvature,
            post.crt.noise,
        ];
        let frames = ctx.loop_beats as f32 * 6.0;
        let frame_id = (ctx.phase * frames).floor().rem_euclid(frames);
        f[5] = [ctx.beat_frac(), frame_id, ctx.loop_beats as f32, 0.0];
        for (i, c) in cols.iter().take(16).enumerate() {
            f[8 + i] = c4(*c, 1.0);
        }
        for (i, s) in slots.iter().enumerate() {
            self.queue
                .write_buffer(&self.post_buf, i as u64 * POST_SLOT, bytemuck::bytes_of(s));
        }
    }

    // ------------------------------------------------------------------
    // Targets

    pub fn create_target(&self, width: u32, height: u32) -> RenderTarget {
        let (w, h) = (width.max(8), height.max(8));
        let dev = &self.device;
        let tex =
            |label: &str, w: u32, h: u32, format, samples: u32, extra: wgpu::TextureUsages| {
                dev.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | extra,
                    view_formats: &[],
                })
                .create_view(&Default::default())
            };
        let sampled = wgpu::TextureUsages::TEXTURE_BINDING;
        let none = wgpu::TextureUsages::empty();
        let msaa_color =
            (self.msaa > 1).then(|| tex("msaa color", w, h, HDR_FORMAT, self.msaa, none));
        let depth = tex("depth", w, h, DEPTH_FORMAT, self.msaa, none);
        let hdr = tex("hdr", w, h, HDR_FORMAT, 1, sampled);
        let hdr2 = tex("hdr2", w, h, HDR_FORMAT, 1, sampled);
        let mut bloom = Vec::new();
        let (mut bw, mut bh) = (w, h);
        for _ in 0..BLOOM_LEVELS {
            bw = (bw / 2).max(1);
            bh = (bh / 2).max(1);
            bloom.push((tex("bloom", bw, bh, HDR_FORMAT, 1, sampled), bw, bh));
        }
        let (rw, rh) = ((w / 2).max(1), (h / 2).max(1));
        let refl = tex("refl", rw, rh, HDR_FORMAT, 1, sampled);
        let refl_depth = tex("refl depth", rw, rh, DEPTH_FORMAT, 1, none);
        let refl_tmp = tex("refl tmp", rw, rh, HDR_FORMAT, 1, sampled);
        let refl_blur = tex("refl blur", rw, rh, HDR_FORMAT, 1, sampled);
        let output = dev.create_texture(&wgpu::TextureDescriptor {
            label: Some("output"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OUTPUT_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[OUTPUT_VIEW_FORMAT],
        });
        let output_view = output.create_view(&Default::default());
        let display_view = output.create_view(&wgpu::TextureViewDescriptor {
            format: Some(OUTPUT_VIEW_FORMAT),
            ..Default::default()
        });
        let post_bg = |a: &wgpu::TextureView, b: &wgpu::TextureView| {
            dev.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post"),
                layout: &self.bgl_post,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.post_buf,
                            offset: 0,
                            size: wgpu::BufferSize::new(POST_SLOT),
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(a),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(b),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.sampler_clamp),
                    },
                ],
            })
        };
        let bg_blur_h = post_bg(&refl, &refl);
        let bg_blur_v = post_bg(&refl_tmp, &refl_tmp);
        let bg_warp = post_bg(&hdr, &hdr);
        let bg_bloom_down = (0..BLOOM_LEVELS)
            .map(|i| {
                let src = if i == 0 { &hdr2 } else { &bloom[i - 1].0 };
                post_bg(src, src)
            })
            .collect();
        let bg_bloom_up = (0..BLOOM_LEVELS - 1)
            .map(|i| post_bg(&bloom[i + 1].0, &bloom[i + 1].0))
            .collect();
        let bg_final = post_bg(&hdr2, &bloom[0].0);
        RenderTarget {
            id: TARGET_IDS.fetch_add(1, Ordering::Relaxed),
            width: w,
            height: h,
            msaa_color,
            depth,
            hdr,
            hdr2,
            bloom,
            refl,
            refl_depth,
            refl_tmp,
            refl_blur,
            refl_size: (rw, rh),
            output,
            output_view,
            display_view,
            bg_blur_h,
            bg_blur_v,
            bg_warp,
            bg_bloom_down,
            bg_bloom_up,
            bg_final,
        }
    }

    /// Copy the final image back to the CPU as tightly packed sRGB RGBA8.
    pub fn read_pixels(&self, target: &RenderTarget) -> Vec<u8> {
        let (w, h) = (target.width, target.height);
        let row = 4 * w;
        let padded =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.output,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([enc.finish()]);
        let slice = buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let data = slice.get_mapped_range().expect("mapping readback buffer");
        let mut out = Vec::with_capacity((row * h) as usize);
        for y in 0..h {
            let s = (y * padded) as usize;
            out.extend_from_slice(&data[s..s + row as usize]);
        }
        drop(data);
        buf.unmap();
        out
    }

    /// Convenience: render and read back as an image.
    pub fn render_image(
        &mut self,
        project: &Project,
        ctx: &EvalCtx,
        target: &RenderTarget,
    ) -> RgbaImage {
        self.render(project, ctx, target);
        RgbaImage::from_raw(target.width, target.height, self.read_pixels(target))
            .expect("pixel buffer size")
    }
}
