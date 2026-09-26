use crate::import::load_mesh_asset;
use crate::mesh::{primitive, MeshData, Vertex};
use crate::texgen;
use bytemuck::{Pod, Zeroable};
use ez_core::eval::{
    instances_are_static, layer_matrix, mesh_instances_with, symmetry_matrices, Instance,
};
use ez_core::palette::PaletteId;
use ez_core::*;
use glam::{Mat4, Vec3, Vec4};
use image::RgbaImage;
use std::borrow::Cow;
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub const HDR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Format of the final image: sRGB-encoded bytes (read back as-is for
/// export; decoded to linear when sampled for display).
pub const OUTPUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// The same image for on-screen display: gamma-encoded bytes in a plain
/// UNORM texture, which is what egui expects of user textures.
pub const DISPLAY_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

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
const SLOT_RAYS: u32 = 15;
const SLOT_RAYS_ADD: u32 = 16;

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
    hfog: [f32; 4],
    caus: [f32; 4],
    caus_col: [f32; 4],
    extra: [f32; 4],
    sun: [f32; 4],
    shadow_vp: [[f32; 4]; 4],
    shadow: [f32; 4],
}

/// Size of the sun shadow map.
const SHADOW_SIZE: u32 = 2048;

/// Per-frame lighting inputs gathered from the layers (weather) on top of
/// the environment.
#[derive(Clone, Copy, Default)]
struct FrameEnv {
    /// Lightning flash brightness and colour.
    lightning: (f32, [f32; 3]),
    /// Ground wetness from rain (0..1).
    wet: f32,
    /// Snow cover (0..1).
    snow: f32,
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
    /// Distance of the farthest vertex from the origin (for culling).
    radius: f32,
}

fn blocks_refl_on(blocks: &[Block], slot: u32) -> bool {
    blocks[slot as usize][0][3] > 0.5
}

/// The six planes of a view-projection matrix (xyz normal, w distance),
/// pointing inwards.
fn frustum_planes(vp: Mat4) -> [Vec4; 6] {
    let r = [vp.row(0), vp.row(1), vp.row(2), vp.row(3)];
    let n = |p: Vec4| p / p.truncate().length().max(1e-9);
    [
        n(r[3] + r[0]),
        n(r[3] - r[0]),
        n(r[3] + r[1]),
        n(r[3] - r[1]),
        n(r[2]),
        n(r[3] - r[2]),
    ]
}

fn sphere_visible(planes: &[Vec4; 6], c: Vec3, r: f32) -> bool {
    planes.iter().all(|p| p.truncate().dot(c) + p.w >= -r)
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
        /// Resolution divisor (1 full, 2 half, 4 quarter).
        res: u32,
        /// [`BackdropKind::index`], which picks the specialised pipeline.
        kind: i32,
    },
    /// Upscale the low-resolution background of the target (divisor).
    BackdropUp {
        res: u32,
    },
    Mesh {
        slot: u32,
        mesh: String,
        tex: String,
        relief: String,
        pixelated: bool,
        first: u32,
        count: u32,
    },
    Particles {
        slot: u32,
        count: u32,
    },
    Terrain {
        slot: u32,
        vertices: u32,
        tex: String,
        pixelated: bool,
    },
    Lasers {
        slot: u32,
        beams: u32,
    },
    Weather {
        slot: u32,
        instances: u32,
    },
    /// Night, stars, moon and rainbow over the backgrounds.
    SkyFx,
    Spots {
        slot: u32,
        beams: u32,
        pools: bool,
    },
    Falls {
        slot: u32,
        puffs: u32,
    },
    /// Letters `first..first + count` of the instance buffer, drawn with
    /// the font atlas `font`.
    Text {
        slot: u32,
        font: String,
        first: u32,
        count: u32,
    },
    /// Contact shadows under the copies `first..first + count` of a mesh.
    Contact {
        slot: u32,
        first: u32,
        count: u32,
    },
}

/// Vertices of one spotlight cone (see spots.wgsl).
const SPOT_VERTICES: u32 = 24 * 6;
/// Vertices of a waterfall curtain (see falls.wgsl).
const FALL_VERTICES: u32 = 12 * 40 * 6;
const FALL_PUFFS: u32 = 48;

/// Instances of one lightning bolt (main channel + branch), see weather.wgsl.
const BOLT_SEGMENTS: u32 = 28 + 14;

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
        BackdropKind::Sponge => 1.2,
        BackdropKind::Rings => 0.5,
        BackdropKind::Clouds => 1.2,
        BackdropKind::Aurora => 0.4,
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
    /// MSAA samples (picks the matching background pipeline).
    samples: u32,
    mesh: wgpu::RenderPipeline,
    particles: wgpu::RenderPipeline,
    terrain: wgpu::RenderPipeline,
    lasers: wgpu::RenderPipeline,
    weather: wgpu::RenderPipeline,
    sky_mul: wgpu::RenderPipeline,
    sky_add: wgpu::RenderPipeline,
    spots: wgpu::RenderPipeline,
    falls: wgpu::RenderPipeline,
    text: wgpu::RenderPipeline,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    msaa: u32,

    bgl_tex: wgpu::BindGroupLayout,
    /// Meshes: colour texture, sampler and relief texture, also read by the
    /// vertex shader (displacement).
    bgl_mesh_tex: wgpu::BindGroupLayout,
    bgl_floor: wgpu::BindGroupLayout,
    bgl_post: wgpu::BindGroupLayout,

    globals_buf: [wgpu::Buffer; 3],
    globals_bg: [wgpu::BindGroup; 3],
    shadow_mesh_pipe: wgpu::RenderPipeline,
    contact_pipe: wgpu::RenderPipeline,
    /// Background pipelines specialised per kind, built when first used:
    /// (kind, samples, low resolution).
    bg_pipes: HashMap<(i32, u32, bool), wgpu::RenderPipeline>,
    bg_module: wgpu::ShaderModule,
    scene_layout: wgpu::PipelineLayout,
    /// Upscale of the low-resolution background.
    bg_up_pipe: wgpu::RenderPipeline,
    shadow_terrain_pipe: wgpu::RenderPipeline,
    shadow_view: wgpu::TextureView,
    shadow_bg: wgpu::BindGroup,
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
    rays_pipe: wgpu::RenderPipeline,
    rays_add_pipe: wgpu::RenderPipeline,

    sampler_repeat: wgpu::Sampler,
    sampler_nearest: wgpu::Sampler,
    sampler_clamp: wgpu::Sampler,

    meshes: HashMap<String, GpuMesh>,
    textures: HashMap<String, GpuTexture>,
    tex_bgs: HashMap<(String, bool), wgpu::BindGroup>,
    mesh_tex_bgs: HashMap<(String, String, bool), wgpu::BindGroup>,
    /// Asset loading problems (shown in the UI), keyed by asset.
    pub errors: HashMap<String, String>,

    /// Instances of layers that don't animate, keyed by layer hash, with
    /// the frame number they were last used.
    instance_cache: HashMap<u64, (u64, Vec<InstanceRaw>)>,
    /// Scene transitions: the two pictures, and the mixing pass.
    seq_targets: Option<SeqTargets>,
    compose_pipe: wgpu::RenderPipeline,
    compose_buf: wgpu::Buffer,
    /// Font atlases by texture key.
    fonts: HashMap<String, std::sync::Arc<crate::text::FontAtlas>>,
    /// Points on shape surfaces for Instancer::Surface: (mesh, count, seed).
    surface_cache: HashMap<(String, u32, u32), std::sync::Arc<Vec<ez_core::eval::SurfacePoint>>>,
    frame_no: u64,
    /// Instance data currently in `inst_buf` (skip identical uploads).
    uploaded: Vec<InstanceRaw>,
    floor_bg_cache: Option<((String, u64), wgpu::BindGroup)>,
    stats: FrameStats,
}

/// All size-dependent GPU resources for one output image.
/// Two pictures for scene transitions and their compositing inputs.
struct SeqTargets {
    width: u32,
    height: u32,
    a: RenderTarget,
    b: RenderTarget,
    bind: wgpu::BindGroup,
}

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
    /// The same picture as gamma-encoded UNORM, for displaying it in egui
    /// (which treats texture values as gamma, not linear).
    pub display: wgpu::Texture,
    pub display_view: wgpu::TextureView,
    bg_blur_h: wgpu::BindGroup,
    bg_blur_v: wgpu::BindGroup,
    bg_warp: wgpu::BindGroup,
    bg_bloom_down: Vec<wgpu::BindGroup>,
    bg_bloom_up: Vec<wgpu::BindGroup>,
    bg_final: wgpu::BindGroup,
    /// Low-resolution backgrounds: (divisor, view, bind group for sampling).
    bg_low: Vec<(u32, wgpu::TextureView, wgpu::BindGroup)>,
    bg_rays: wgpu::BindGroup,
    bg_rays_add: wgpu::BindGroup,
    rays: wgpu::TextureView,
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

/// Multiplies what is already there by the shader's colour.
const MULTIPLY: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::Src,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

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
        let vf = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let bgl_mesh_tex = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mesh tex"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    visibility: vf,
                    ..tex_entry(0)
                },
                wgpu::BindGroupLayoutEntry {
                    visibility: vf,
                    ..sampler_entry(1)
                },
                wgpu::BindGroupLayoutEntry {
                    visibility: vf,
                    ..tex_entry(2)
                },
            ],
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

        let bgl_shadow = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let globals_buf = [0, 1, 2].map(|i| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(["globals main", "globals refl", "globals sun"][i]),
                size: globals_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let globals_bg = [0, 1, 2].map(|i| {
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
        let mesh_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[Some(&bgl_globals), Some(&bgl_draw), Some(&bgl_mesh_tex)],
            immediate_size: 0,
        });
        // Lit surfaces also read the sun shadow map (group 3).
        let mesh_lit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh lit"),
            bind_group_layouts: &[
                Some(&bgl_globals),
                Some(&bgl_draw),
                Some(&bgl_mesh_tex),
                Some(&bgl_shadow),
            ],
            immediate_size: 0,
        });
        let terrain_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[
                Some(&bgl_globals),
                Some(&bgl_draw),
                Some(&bgl_tex),
                Some(&bgl_shadow),
            ],
            immediate_size: 0,
        });
        let particle_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particles"),
            bind_group_layouts: &[Some(&bgl_globals), Some(&bgl_draw)],
            immediate_size: 0,
        });
        let floor_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("floor"),
            bind_group_layouts: &[
                Some(&bgl_globals),
                Some(&bgl_draw),
                Some(&bgl_floor),
                Some(&bgl_shadow),
            ],
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
        let sh_terrain = shader(
            device,
            "terrain",
            include_str!("shaders/terrain.wgsl"),
            true,
        );
        let beams = include_str!("shaders/beams.wgsl");
        let sh_lasers = shader(
            device,
            "lasers",
            &format!("{beams}\n{}", include_str!("shaders/lasers.wgsl")),
            true,
        );
        let sh_spots = shader(
            device,
            "spots",
            &format!("{beams}\n{}", include_str!("shaders/spots.wgsl")),
            true,
        );
        let sh_skyfx = shader(device, "sky fx", include_str!("shaders/skyfx.wgsl"), true);
        let sh_falls = shader(device, "falls", include_str!("shaders/falls.wgsl"), true);
        let sh_weather = shader(
            device,
            "weather",
            include_str!("shaders/weather.wgsl"),
            true,
        );
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
        let mesh_buffers = [Some(vertex_layout), Some(instance_layout.clone())];
        // Letters: one instance each, the quad comes from the vertex index.
        let glyph_buffers = [Some(instance_layout)];
        let sh_text = shader(device, "text", include_str!("shaders/text.wgsl"), true);

        let scene_pipes = |samples: u32| ScenePipes {
            samples,
            mesh: make_pipeline(
                device,
                PipeDesc {
                    label: "mesh",
                    layout: &mesh_lit_layout,
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
                    // Glowing particles output alpha 0 (pure addition);
                    // smoke outputs its coverage.
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                },
            ),
            terrain: make_pipeline(
                device,
                PipeDesc {
                    label: "terrain",
                    layout: &terrain_layout,
                    module: &sh_terrain,
                    fs: "fs_main",
                    buffers: &[],
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((true, wgpu::CompareFunction::Less)),
                    blend: None,
                },
            ),
            lasers: make_pipeline(
                device,
                PipeDesc {
                    label: "lasers",
                    layout: &particle_layout,
                    module: &sh_lasers,
                    fs: "fs_main",
                    buffers: &[],
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((false, wgpu::CompareFunction::Less)),
                    blend: Some(ADDITIVE),
                },
            ),
            weather: make_pipeline(
                device,
                PipeDesc {
                    label: "weather",
                    layout: &particle_layout,
                    module: &sh_weather,
                    fs: "fs_main",
                    buffers: &[],
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((false, wgpu::CompareFunction::Less)),
                    blend: Some(ADDITIVE),
                },
            ),
            sky_mul: make_pipeline(
                device,
                PipeDesc {
                    label: "sky fx mul",
                    layout: &particle_layout,
                    module: &sh_skyfx,
                    fs: "fs_mul",
                    buffers: &[],
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((false, wgpu::CompareFunction::Always)),
                    blend: Some(MULTIPLY),
                },
            ),
            sky_add: make_pipeline(
                device,
                PipeDesc {
                    label: "sky fx add",
                    layout: &particle_layout,
                    module: &sh_skyfx,
                    fs: "fs_add",
                    buffers: &[],
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((false, wgpu::CompareFunction::Always)),
                    blend: Some(ADDITIVE),
                },
            ),
            spots: make_pipeline(
                device,
                PipeDesc {
                    label: "spots",
                    layout: &particle_layout,
                    module: &sh_spots,
                    fs: "fs_main",
                    buffers: &[],
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((false, wgpu::CompareFunction::Less)),
                    blend: Some(ADDITIVE),
                },
            ),
            falls: make_pipeline(
                device,
                PipeDesc {
                    label: "falls",
                    layout: &particle_layout,
                    module: &sh_falls,
                    fs: "fs_main",
                    buffers: &[],
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((false, wgpu::CompareFunction::Less)),
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                },
            ),
            text: make_pipeline(
                device,
                PipeDesc {
                    label: "text",
                    layout: &scene_layout,
                    module: &sh_text,
                    fs: "fs_main",
                    buffers: &glyph_buffers,
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((false, wgpu::CompareFunction::Less)),
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                },
            ),
        };
        // Depth-only passes from the sun (vertex stage only).
        let depth_pipe = |label: &str,
                          layout: &wgpu::PipelineLayout,
                          module: &wgpu::ShaderModule,
                          buffers: &[Option<wgpu::VertexBufferLayout>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState {
                        constant: 2,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: Default::default(),
                fragment: None,
                multiview_mask: None,
                cache: None,
            })
        };
        let sh_contact = shader(
            device,
            "contact",
            include_str!("shaders/contact.wgsl"),
            true,
        );
        let contact_instances = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4],
        })];
        let contact_pipe = make_pipeline(
            device,
            PipeDesc {
                label: "contact shadows",
                layout: &particle_layout,
                module: &sh_contact,
                fs: "fs_main",
                buffers: &contact_instances,
                format: HDR_FORMAT,
                samples: msaa,
                depth: Some((false, wgpu::CompareFunction::Less)),
                blend: Some(MULTIPLY),
            },
        );
        let sh_bgup = shader(
            device,
            "bg upscale",
            include_str!("shaders/bgup.wgsl"),
            true,
        );
        let bg_up_pipe = make_pipeline(
            device,
            PipeDesc {
                label: "bg upscale",
                layout: &scene_layout,
                module: &sh_bgup,
                fs: "fs_main",
                buffers: &[],
                format: HDR_FORMAT,
                samples: msaa,
                depth: Some((false, wgpu::CompareFunction::Always)),
                blend: None,
            },
        );
        let shadow_mesh_pipe = depth_pipe("shadow mesh", &mesh_layout, &sh_mesh, &mesh_buffers);
        let shadow_terrain_pipe = depth_pipe("shadow terrain", &scene_layout, &sh_terrain, &[]);
        let shadow_view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("sun shadow map"),
                size: wgpu::Extent3d {
                    width: SHADOW_SIZE,
                    height: SHADOW_SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow"),
            layout: &bgl_shadow,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                },
            ],
        });
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
        // The final pass writes the export image and the display image.
        let final_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("final"),
            layout: Some(&post_layout),
            vertex: wgpu::VertexState {
                module: &sh_post,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &sh_post,
                entry_point: Some("fs_final"),
                compilation_options: Default::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: OUTPUT_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: DISPLAY_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
            }),
            multiview_mask: None,
            cache: None,
        });
        let compose_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("compose"),
            layout: Some(&post_layout),
            vertex: wgpu::VertexState {
                module: &sh_post,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &sh_post,
                entry_point: Some("fs_compose"),
                compilation_options: Default::default(),
                targets: &[
                    Some(wgpu::ColorTargetState {
                        format: OUTPUT_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                    Some(wgpu::ColorTargetState {
                        format: DISPLAY_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    }),
                ],
            }),
            multiview_mask: None,
            cache: None,
        });
        let compose_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compose params"),
            size: POST_SLOT,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let rays_pipe = post_pipe("god rays", "fs_rays", HDR_FORMAT, None);
        let rays_add_pipe = post_pipe("god rays add", "fs_rays_add", HDR_FORMAT, Some(ADDITIVE));

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
            bgl_mesh_tex,
            bgl_floor,
            bgl_post,
            globals_buf,
            globals_bg,
            shadow_mesh_pipe,
            contact_pipe,
            bg_pipes: HashMap::new(),
            bg_module: sh_backdrop,
            scene_layout,
            bg_up_pipe,
            shadow_terrain_pipe,
            shadow_view,
            shadow_bg,
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
            rays_pipe,
            rays_add_pipe,
            sampler_repeat,
            sampler_nearest,
            sampler_clamp,
            meshes: HashMap::new(),
            textures: HashMap::new(),
            tex_bgs: HashMap::new(),
            mesh_tex_bgs: HashMap::new(),
            errors: HashMap::new(),
            instance_cache: HashMap::new(),
            seq_targets: None,
            compose_pipe,
            compose_buf,
            fonts: HashMap::new(),
            surface_cache: HashMap::new(),
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
        self.upload_texture_as(key, img, wgpu::TextureFormat::Rgba8UnormSrgb)
    }

    /// Upload with mipmaps in `format` (Rgba8Unorm for data such as
    /// distance fields).
    fn upload_texture_as(&mut self, key: String, img: &RgbaImage, format: wgpu::TextureFormat) {
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
            format,
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
        self.mesh_tex_bgs
            .retain(|(k, r, _), _| *k != key && *r != key);
        self.textures.insert(
            key,
            GpuTexture {
                _texture: texture,
                view,
            },
        );
    }

    /// The atlas of a text layer's font, uploaded; `None` (with an error
    /// shown) when a font file can't be used.
    fn font_atlas(
        &mut self,
        t: &TextLayer,
    ) -> Option<(String, std::sync::Arc<crate::text::FontAtlas>)> {
        let key = match &t.font_file {
            Some(path) => format!("__font:file:{path}"),
            None => format!("__font:{:?}", t.font),
        };
        if let Some(a) = self.fonts.get(&key) {
            return Some((key, a.clone()));
        }
        let atlas = match &t.font_file {
            None => crate::text::builtin_atlas(t.font),
            Some(path) => {
                let built = ez_core::store::read(path)
                    .map_err(|e| anyhow::anyhow!("{e}"))
                    .and_then(|bytes| crate::text::build_atlas(&bytes, false));
                match built {
                    Ok(a) => {
                        self.errors.remove(&key);
                        std::sync::Arc::new(a)
                    }
                    Err(e) => {
                        self.errors.insert(
                            key.clone(),
                            format!("Font {}: {e:#}", ez_core::store::file_name(path)),
                        );
                        crate::text::builtin_atlas(t.font)
                    }
                }
            }
        };
        self.upload_texture_as(key.clone(), &atlas.image, wgpu::TextureFormat::Rgba8Unorm);
        self.tex_bind_group(&key, false);
        self.fonts.insert(key.clone(), atlas.clone());
        Some((key, atlas))
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
        let decoded = ez_core::store::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| image::load_from_memory(&b).map_err(|e| e.to_string()));
        match decoded {
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

    fn mesh_tex_bind_group(&mut self, tex: &str, relief: &str, nearest: bool) {
        let k = (tex.to_string(), relief.to_string(), nearest);
        if self.mesh_tex_bgs.contains_key(&k) {
            return;
        }
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mesh tex"),
            layout: &self.bgl_mesh_tex,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.textures[tex].view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(if nearest {
                        &self.sampler_nearest
                    } else {
                        &self.sampler_repeat
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.textures[relief].view),
                },
            ],
        });
        self.mesh_tex_bgs.insert(k, bg);
    }

    /// Geometry of a mesh source, subdivided `levels` times (cached).
    fn mesh_key_subdivided(&mut self, source: &MeshSource, levels: u32) -> String {
        let base = self.mesh_key(source);
        if levels == 0 {
            return base;
        }
        // Keep the result under ~2M triangles.
        let tris = self
            .meshes
            .get(&base)
            .map(|g| g.count as u64 / 3)
            .unwrap_or(1)
            .max(1);
        let mut levels = levels.min(4);
        while levels > 0 && tris * 4u64.pow(levels) > 2_000_000 {
            levels -= 1;
        }
        let key = format!("{base}#s{levels}");
        if levels == 0 || self.meshes.contains_key(&key) {
            return if levels == 0 { base } else { key };
        }
        let mut data = self.source_data(source, &base);
        data.subdivide(levels);
        self.upload_mesh(key.clone(), &data);
        key
    }

    /// `count` points evenly spread over `shape`'s surface.
    fn surface_points(
        &mut self,
        shape: &MeshSource,
        count: u32,
        seed: u32,
    ) -> std::sync::Arc<Vec<ez_core::eval::SurfacePoint>> {
        let key = (self.mesh_key(shape), count, seed);
        if let Some(p) = self.surface_cache.get(&key) {
            return p.clone();
        }
        let data = self.source_data(shape, &key.0);
        let positions: Vec<[f32; 3]> = data.vertices.iter().map(|v| v.pos).collect();
        let points = std::sync::Arc::new(ez_core::eval::sample_surface(
            &positions,
            &data.indices,
            count,
            seed,
        ));
        if self.surface_cache.len() > 64 {
            self.surface_cache.clear();
        }
        self.surface_cache.insert(key, points.clone());
        points
    }

    fn mesh_key(&mut self, source: &MeshSource) -> String {
        let key = match source {
            MeshSource::Primitive(p) => format!("p:{}", p.cache_key()),
            MeshSource::File { path } => format!("f:{path}"),
            MeshSource::Text { .. } => {
                format!("t:{}", serde_json::to_string(source).unwrap_or_default())
            }
        };
        if self.meshes.contains_key(&key) {
            return key;
        }
        let data = self.source_data(source, &key);
        self.upload_mesh(key.clone(), &data);
        key
    }

    /// Geometry of a shape source; problems are reported under `key` and
    /// give a cube.
    fn source_data(&mut self, source: &MeshSource, key: &str) -> MeshData {
        match source {
            MeshSource::Primitive(p) => primitive(p),
            MeshSource::File { path } => match load_mesh_asset(path) {
                Ok(m) => {
                    self.errors.remove(key);
                    m
                }
                Err(e) => {
                    self.errors
                        .insert(key.to_string(), format!("model {path}: {e:#}"));
                    primitive(&Primitive::Cube)
                }
            },
            MeshSource::Text {
                text,
                font,
                font_file,
                depth,
            } => {
                let bytes = match font_file {
                    Some(path) => match ez_core::store::read(path) {
                        Ok(b) => {
                            self.errors.remove(key);
                            Some(b)
                        }
                        Err(e) => {
                            self.errors
                                .insert(key.to_string(), format!("font {path}: {e:#}"));
                            None
                        }
                    },
                    None => None,
                };
                crate::text::text_mesh(text, *font, bytes.as_deref(), *depth)
            }
        }
    }

    /// Geometry of a neon ribbon, generated on first use.
    fn ribbon_key(&mut self, r: &Ribbon) -> String {
        let key = format!(
            "r:{}",
            serde_json::to_string(&(r.curve, r.freq, r.thickness)).unwrap_or_default()
        );
        if !self.meshes.contains_key(&key) {
            let data = crate::mesh::ribbon(r);
            self.upload_mesh(key.clone(), &data);
        }
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
                radius: data
                    .vertices
                    .iter()
                    .map(|v| Vec3::from(v.pos).length())
                    .fold(0.0, f32::max),
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

    #[allow(clippy::too_many_arguments)]
    fn globals(
        project: &Project,
        ctx: &EvalCtx,
        env: &EnvState,
        view: Mat4,
        proj: Mat4,
        eye: Vec3,
        res: (u32, u32),
        clip: Vec4,
        fx: &FrameEnv,
    ) -> GlobalsRaw {
        let e = &project.environment;
        let flash = fx.lightning;
        // Lightning brightens the ambient light and the fog.
        let fog_color = color::add(env.fog_color, color::scale(flash.1, flash.0 * 0.12));
        let vp = proj * view;
        let inv_view = view.inverse();
        let right = inv_view.x_axis.truncate().normalize_or(Vec3::X);
        let up = inv_view.y_axis.truncate().normalize_or(Vec3::Y);
        let hf = &e.height_fog;
        let ca = &e.caustics;
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
            fog: c4(fog_color, env.fog_density),
            sky: c4(env.sky_color, env.ambient + flash.0 * 0.8),
            ground: c4(env.ground_color, env.light_intensity),
            light_dir: v4(Vec3::from(env.light_dir), 0.0),
            light_color: c4(env.light_color, 1.0),
            clip: clip.into(),
            audio: [ctx.audio, ctx.bass, flash.0, 0.0],
            hfog: [
                hf.density.eval(ctx).max(0.0),
                hf.height,
                hf.falloff.max(0.05),
                0.0,
            ],
            caus: [
                ca.amount.eval(ctx).max(0.0),
                ca.scale.max(0.05),
                TAU * (ctx.phase * ca.speed as f32).rem_euclid(1.0),
                ca.below,
            ],
            caus_col: c4(ca.color, fx.wet.clamp(0.0, 1.0)),
            extra: [
                fx.snow.clamp(0.0, 1.0),
                env.night,
                env.dusk,
                e.rainbow.eval(ctx).max(0.0),
            ],
            sun: v4(Vec3::from(env.sun_dir), 0.0),
            shadow_vp: m4(Mat4::IDENTITY),
            shadow: [0.0; 4],
        }
    }

    /// Render one frame of `project` at `ctx` into `target`.
    /// Build the background pipeline for `kind` if it isn't cached. `low`
    /// is the low-resolution pass (no depth buffer).
    fn ensure_bg_pipe(&mut self, kind: i32, samples: u32, low: bool) {
        if self.bg_pipes.contains_key(&(kind, samples, low)) {
            return;
        }
        let constants = [("BG_KIND", kind as f64)];
        let options = wgpu::PipelineCompilationOptions {
            constants: &constants,
            ..Default::default()
        };
        let pipe = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("backdrop"),
                layout: Some(&self.scene_layout),
                vertex: wgpu::VertexState {
                    module: &self.bg_module,
                    entry_point: Some("vs_main"),
                    compilation_options: options.clone(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: (!low).then(|| wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: samples,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.bg_module,
                    entry_point: Some("fs_main"),
                    compilation_options: options,
                    targets: &[Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        self.bg_pipes.insert((kind, samples, low), pipe);
    }

    /// Render a frame of `project` at `ctx` into `target`: its scene, or
    /// with a sequence the scene playing now (two mixed during a
    /// transition).
    pub fn render(&mut self, project: &Project, ctx: &EvalCtx, target: &RenderTarget) {
        let Some(frame) = project.sequence.frame_at(ctx) else {
            return self.render_scene(project, ctx, target);
        };
        let Some(current) = project.scene_view(frame.scene) else {
            return self.render_scene(project, ctx, target);
        };
        let Some((from, from_ctx, t, tr)) = frame.from else {
            return self.render_scene(&current, &frame.ctx, target);
        };
        let Some(previous) = project.scene_view(from) else {
            return self.render_scene(&current, &frame.ctx, target);
        };
        let (w, h) = (target.width, target.height);
        let seq = match self.seq_targets.take() {
            Some(s) if s.width == w && s.height == h => s,
            _ => {
                let a = self.create_target(w, h);
                let b = self.create_target(w, h);
                let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("compose"),
                    layout: &self.bgl_post,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &self.compose_buf,
                                offset: 0,
                                size: wgpu::BufferSize::new(POST_SLOT),
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&a.output_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&b.output_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&self.sampler_clamp),
                        },
                    ],
                });
                SeqTargets {
                    width: w,
                    height: h,
                    a,
                    b,
                    bind,
                }
            }
        };
        self.render_scene(&previous, &from_ctx, &seq.a);
        self.render_scene(&current, &frame.ctx, &seq.b);
        let mut params = [[0.0f32; 4]; (POST_SLOT / 16) as usize];
        params[0] = [
            tr.kind.index() as f32,
            t,
            tr.angle.to_radians(),
            w as f32 / h.max(1) as f32,
        ];
        self.queue
            .write_buffer(&self.compose_buf, 0, bytemuck::cast_slice(&params));
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("compose"),
            });
        {
            let att = |view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("compose"),
                color_attachments: &[att(&target.output_view), att(&target.display_view)],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.compose_pipe);
            pass.set_bind_group(0, &seq.bind, &[0]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([enc.finish()]);
        self.seq_targets = Some(seq);
    }

    /// Render one scene (no sequence).
    fn render_scene(&mut self, project: &Project, ctx: &EvalCtx, target: &RenderTarget) {
        let layers = project.scene_layers(ctx);
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
        // Lightning flash lighting up the whole scene: brightness, colour.
        let mut fx = FrameEnv {
            lightning: (0.0, [1.0; 3]),
            ..Default::default()
        };
        let env = project.environment.eval(ctx);

        for (li, layer) in layers.iter().enumerate().filter(|(_, l)| l.enabled) {
            // Blinking layers can be hidden right now; flashing ones glow more.
            let Some(flash) = layer.blink.eval(ctx.beat_phase) else {
                continue;
            };
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
                        b.intensity.eval(ctx) * flash,
                        b.detail.eval(ctx),
                    ];
                    blk[1] = c4(b.color_a, 0.0);
                    blk[2] = c4(b.color_b, 0.0);
                    blk[3] = c4(b.color_c, 0.0);
                    blk[4] = [if b.texture.is_some() { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0];
                    let r = &b.ray;
                    blk[5] = [
                        r.variant as f32,
                        r.pattern as f32,
                        r.size.eval(ctx),
                        r.twist.eval(ctx),
                    ];
                    blk[6] = [
                        r.warp.eval(ctx),
                        r.bend.eval(ctx),
                        r.glow.eval(ctx),
                        r.fog.eval(ctx),
                    ];
                    blk[7] = [
                        r.steps.min(256) as f32,
                        // Wrapped so the last frame's roll is exactly the first's.
                        TAU * (r.spin as f32 * ctx.phase).rem_euclid(1.0),
                        ctx.phase.rem_euclid(1.0),
                        0.0,
                    ];
                    ls.draws = 1;
                    ls.load = backdrop_load(b.kind);
                    cmds.push(Cmd::Backdrop {
                        slot: blocks.len() as u32,
                        tex,
                        res: b.resolution.divisor(),
                        kind: b.kind.index() as i32,
                    });
                    blocks.push(blk);
                }
                LayerKind::Mesh(m) => {
                    let mesh = self.mesh_key_subdivided(&m.source, m.subdivide);
                    let mat = &m.material;
                    let tex = self.texture_key(project, mat.texture.as_deref());
                    let rel = &mat.relief;
                    let relief_on = rel.bump.is_animated()
                        || rel.bump.base != 0.0
                        || rel.displace.is_animated()
                        || rel.displace.base != 0.0;
                    let relief_name = rel.texture.as_deref().or(mat.texture.as_deref());
                    let relief = if relief_on {
                        self.texture_key(project, relief_name)
                    } else {
                        "__white".to_string()
                    };
                    self.mesh_tex_bind_group(&tex, &relief, mat.pixelated);
                    let surface = match &m.instancer {
                        Instancer::Surface {
                            shape, count, seed, ..
                        } => Some(self.surface_points(shape, *count, *seed)),
                        _ => None,
                    };
                    let surface = surface.as_deref().map(|v| v.as_slice());
                    let first = instances.len() as u32;
                    let to_raw = |i: &Instance| InstanceRaw {
                        model: m4(i.model),
                        inst: [i.hue, i.glow, i.rand, i.along],
                    };
                    let count = if instances_are_static(layer, m) {
                        let key = layer_hash(layer);
                        let frame = self.frame_no;
                        let entry = self.instance_cache.entry(key).or_insert_with(|| {
                            scratch.clear();
                            mesh_instances_with(layer, m, ctx, surface, &mut scratch);
                            (frame, scratch.iter().map(to_raw).collect())
                        });
                        entry.0 = frame;
                        instances.extend_from_slice(&entry.1);
                        stats.cached_layers += 1;
                        entry.1.len()
                    } else {
                        scratch.clear();
                        mesh_instances_with(layer, m, ctx, surface, &mut scratch);
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
                    blk[0] = c4(mat.base_color, mat.metallic.eval(ctx));
                    let e = mat.emissive.eval(ctx).max(0.0) * flash;
                    blk[1] = c4(color::scale(mat.emissive_color, e), mat.roughness.eval(ctx));
                    let mode = EmissiveMode::ALL
                        .iter()
                        .position(|x| *x == mat.emissive_mode)
                        .unwrap_or(0);
                    blk[2] = [
                        mode as f32,
                        if mat.texture.is_some() { 1.0 } else { 0.0 },
                        mat.texture_scale.eval(ctx),
                        if mat.flat_shading { 1.0 } else { 0.0 },
                    ];
                    blk[3] = [
                        ctx.phase * mat.scroll[0] as f32,
                        ctx.phase * mat.scroll[1] as f32,
                        mat.rim.eval(ctx),
                        mat.hue_shift.eval(ctx),
                    ];
                    let g = &mat.glitch;
                    blk[4] = [
                        g.amount.eval(ctx).max(0.0),
                        g.style.index() as f32,
                        g.rate.max(1) as f32,
                        g.chance.eval(ctx).clamp(0.0, 1.0),
                    ];
                    blk[5] = [g.seed as f32, 0.0, 0.0, 0.0];
                    blk[8] = [
                        rel.bump.eval(ctx),
                        rel.displace.eval(ctx),
                        match rel.mode {
                            ReliefMode::Bump => 0.0,
                            ReliefMode::NormalMap => 1.0,
                        },
                        if relief_on && relief_name.is_some() {
                            1.0
                        } else {
                            0.0
                        },
                    ];
                    let ramp = &m.ramp;
                    if ramp.enabled && !ramp.colors.is_empty() {
                        let n = ramp.colors.len().min(4);
                        for k in 0..4 {
                            let c = ramp.colors[k.min(n - 1)];
                            blk[11 + k] = c4(c, 0.0);
                        }
                        blk[11][3] = n as f32;
                        blk[12][3] = e;
                        blk[15] = [
                            1.0,
                            match ramp.mode {
                                RampMode::Gradient => 0.0,
                                RampMode::Steps => 1.0,
                            },
                            (ctx.phase * ramp.cycles as f32).rem_euclid(1.0),
                            if ramp.glow { 1.0 } else { 0.0 },
                        ];
                    }
                    let df = &m.deform;
                    if df.is_active() {
                        blk[9] = [
                            df.twist.eval(ctx),
                            df.bend.eval(ctx).to_radians(),
                            df.taper.eval(ctx),
                            df.noise.eval(ctx),
                        ];
                        blk[10] = [
                            df.noise_scale.max(0.01),
                            TAU * (ctx.phase * df.noise_speed as f32).rem_euclid(1.0),
                            df.explode.eval(ctx),
                            df.reach(ctx),
                        ];
                    }
                    cmds.push(Cmd::Mesh {
                        slot: blocks.len() as u32,
                        mesh,
                        tex,
                        relief,
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
                        blk[2] = c4(p.color_b, p.intensity.eval(ctx).max(0.0) * flash);
                        blk[3] = [
                            p.speed.eval(ctx),
                            p.radius.eval(ctx),
                            p.trail.min(16) as f32,
                            p.trail_spacing.eval(ctx),
                        ];
                        blk[4] = [
                            p.sprite.index() as f32,
                            if p.smoke { 1.0 } else { 0.0 },
                            0.0,
                            0.0,
                        ];
                        let model = m4(sym * lm);
                        blk[8..12].copy_from_slice(&model);
                        cmds.push(Cmd::Particles {
                            slot: blocks.len() as u32,
                            count,
                        });
                        blocks.push(blk);
                    }
                }
                LayerKind::Terrain(t) => {
                    let tex = self.texture_key(project, t.texture.as_deref());
                    self.tex_bind_group(&tex, t.pixelated);
                    let lm = layer_matrix(&layer.transform, ctx);
                    let cells = t.cells.clamp(4, t.max_cells());
                    let drawn = t.drawn_cells();
                    let syms = symmetry_matrices(&layer.symmetry);
                    ls.triangles = (drawn * drawn * 2) as u64 * syms.len() as u64;
                    ls.draws = syms.len() as u32;
                    ls.load = ls.triangles as f32 / 150_000.0 + 0.05;
                    for sym in syms {
                        let mut blk: Block = Zeroable::zeroed();
                        blk[0] = [
                            t.size.max(1.0),
                            cells as f32,
                            t.height.eval(ctx),
                            t.hills.clamp(1, 64) as f32,
                        ];
                        blk[1] = [
                            t.roughness.eval(ctx),
                            (ctx.phase * t.scroll as f32).rem_euclid(1.0),
                            t.valley.eval(ctx).clamp(0.0, 1.0),
                            t.style.index() as f32,
                        ];
                        blk[2] = c4(
                            color::scale(t.line_color, t.glow.eval(ctx).max(0.0) * flash),
                            (t.seed % 65536) as f32,
                        );
                        blk[3] = c4(t.fill_color, if t.texture.is_some() { 1.0 } else { 0.0 });
                        blk[4] = [
                            t.tiles.clamp(1, 256) as f32,
                            if t.texture_lines { 1.0 } else { 0.0 },
                            0.0,
                            0.0,
                        ];
                        let lq = &t.liquid;
                        blk[5] = [
                            t.shape.index() as f32,
                            t.biome.index() as f32,
                            lq.kind.index() as f32,
                            lq.level.eval(ctx) * t.height.eval(ctx),
                        ];
                        blk[6] = c4(lq.color, lq.glow.eval(ctx) * flash);
                        // Liquid motion: one small circle per bar, plus the
                        // current (whole drifts per loop).
                        let bars = (ctx.loop_beats / 4).max(1) as f32;
                        blk[7] = [
                            lq.waves.eval(ctx).max(0.0),
                            (ctx.phase * lq.flow as f32).rem_euclid(1.0),
                            TAU * (ctx.phase * bars).rem_euclid(1.0),
                            0.0,
                        ];
                        let model = sym * lm;
                        blk[8..12].copy_from_slice(&m4(model));
                        if t.lod {
                            // Detail follows the camera: its position over
                            // the terrain, in 0..1.
                            let size = t.size.max(1.0);
                            let eye = model.inverse().transform_point3(cam.eye);
                            let fu = (eye.x / size + 0.5).clamp(0.0, 1.0);
                            let fw = (eye.z / size + 0.5).clamp(0.0, 1.0);
                            let (kx, sx) = ez_core::scene::lod_grading(cells, drawn, fu);
                            let (kz, sz) = ez_core::scene::lod_grading(cells, drawn, fw);
                            blk[12] = [fu, fw, kx, kz];
                            blk[13] = [sx, sz, drawn as f32, 0.0];
                        }
                        cmds.push(Cmd::Terrain {
                            slot: blocks.len() as u32,
                            vertices: drawn * drawn * 6,
                            tex: tex.clone(),
                            pixelated: t.pixelated,
                        });
                        blocks.push(blk);
                    }
                }
                LayerKind::Lasers(z) => {
                    let lm = layer_matrix(&layer.transform, ctx);
                    let beams = z.count.clamp(1, 256);
                    let syms = symmetry_matrices(&layer.symmetry);
                    ls.draws = syms.len() as u32;
                    ls.triangles = beams as u64 * 2 * syms.len() as u64;
                    ls.load = 0.05 * syms.len() as f32;
                    if z.style == BeamStyle::Spotlight {
                        ls.triangles = beams as u64 * 48 * syms.len() as u64;
                        ls.load = 0.15 * syms.len() as f32 * (beams as f32 / 8.0).max(1.0);
                    }
                    // Beat strobe: full on each beat, decaying until the next.
                    let beat_flash = (1.0 - ctx.beat_frac()).powi(3);
                    let strobe = z.strobe.eval(ctx).clamp(0.0, 1.0);
                    let bright = z.intensity.eval(ctx).max(0.0)
                        * (1.0 - strobe + strobe * beat_flash)
                        * flash;
                    for sym in syms {
                        let mut blk: Block = Zeroable::zeroed();
                        blk[0] = [
                            beams as f32,
                            z.pattern.index() as f32,
                            z.spread.eval(ctx).to_radians(),
                            z.length.eval(ctx).max(0.0),
                        ];
                        blk[1] = c4(z.color_a, z.width.eval(ctx).max(0.001));
                        blk[2] = c4(z.color_b, bright);
                        let cycles = z.sweep_cycles as f32;
                        blk[3] = [
                            z.sweep.eval(ctx).to_radians(),
                            (ctx.phase * cycles).rem_euclid(1.0),
                            (z.seed % 65536) as f32,
                            (ctx.phase * cycles.max(1.0)).rem_euclid(1.0),
                        ];
                        blk[8..12].copy_from_slice(&m4(sym * lm));
                        let slot = blocks.len() as u32;
                        if z.style == BeamStyle::Spotlight {
                            let half = (z.cone.0.eval(ctx).clamp(1.0, 170.0) * 0.5).to_radians();
                            blk[4] = [half.tan(), if z.pools { 1.0 } else { 0.0 }, 0.0, 0.0];
                            cmds.push(Cmd::Spots {
                                slot,
                                beams,
                                pools: z.pools,
                            });
                        } else {
                            cmds.push(Cmd::Lasers { slot, beams });
                        }
                        blocks.push(blk);
                    }
                }
                LayerKind::Text(t) => {
                    let Some((font, atlas)) = self.font_atlas(t) else {
                        continue;
                    };
                    let glyphs = crate::text::layout(t, &atlas, ctx);
                    let mut lm = layer_matrix(&layer.transform, ctx);
                    if t.face_camera {
                        // Keep position and size, turn to face the camera.
                        let (scale, _, pos) = lm.to_scale_rotation_translation();
                        let fwd = (cam.eye - pos).normalize_or(Vec3::Z);
                        let side = cam.up.cross(fwd).normalize_or(Vec3::X);
                        let up = fwd.cross(side);
                        lm = Mat4::from_cols(
                            (side * scale.x).extend(0.0),
                            (up * scale.y).extend(0.0),
                            (fwd * scale.z).extend(0.0),
                            pos.extend(1.0),
                        );
                    }
                    let syms = symmetry_matrices(&layer.symmetry);
                    let size = t.size.max(0.01);
                    let cell = crate::text::FontAtlas::cell_em() * size;
                    let pen = crate::text::FontAtlas::pen_in_cell() * size;
                    let first = instances.len() as u32;
                    for sym in &syms {
                        let base = *sym * lm;
                        for g in &glyphs {
                            let origin = g.pos * size - pen;
                            let m = base
                                * Mat4::from_translation(origin.extend(0.0))
                                * Mat4::from_scale(Vec3::new(cell, cell, 1.0));
                            instances.push(InstanceRaw {
                                model: m4(m),
                                inst: [g.cell as f32, g.alpha * flash.min(1.0), g.along, 0.0],
                            });
                        }
                    }
                    let count = instances.len() as u32 - first;
                    ls.draws = syms.len() as u32;
                    ls.triangles = count as u64 * 2;
                    ls.load = 0.02 + count as f32 / 20_000.0;
                    let mut blk: Block = Zeroable::zeroed();
                    blk[0] = c4(t.color_top, t.glow.eval(ctx).max(0.0) * flash.max(1.0));
                    blk[1] = c4(t.color_bottom, t.outline);
                    blk[2] = c4(t.outline_color, t.shadow);
                    let base_h = 1.0
                        - (crate::text::CELL as f32
                            - crate::text::FontAtlas::pen_in_cell().y * crate::text::EM_PX)
                            / crate::text::CELL as f32;
                    blk[3] = [t.chrome, atlas.cols as f32, atlas.rows as f32, base_h];
                    if count > 0 {
                        cmds.push(Cmd::Text {
                            slot: blocks.len() as u32,
                            font,
                            first,
                            count,
                        });
                    }
                    blocks.push(blk);
                }
                LayerKind::Falls(fl) => {
                    let lm = layer_matrix(&layer.transform, ctx);
                    let syms = symmetry_matrices(&layer.symmetry);
                    ls.draws = syms.len() as u32 * 2;
                    ls.triangles = (FALL_VERTICES / 3 + FALL_PUFFS * 2) as u64 * syms.len() as u64;
                    ls.load = 0.1 * syms.len() as f32;
                    for sym in syms {
                        let mut blk: Block = Zeroable::zeroed();
                        blk[0] = [
                            fl.kind.index() as f32,
                            fl.width.max(0.05),
                            fl.height.max(0.05),
                            fl.push,
                        ];
                        blk[1] = c4(fl.color, fl.glow.eval(ctx).max(0.0) * flash);
                        blk[2] = [
                            (ctx.phase * fl.flow as f32).rem_euclid(1.0),
                            fl.foam.eval(ctx).max(0.0),
                            (fl.seed % 65536) as f32,
                            (fl.flow / 2).max(1) as f32,
                        ];
                        blk[8..12].copy_from_slice(&m4(sym * lm));
                        cmds.push(Cmd::Falls {
                            slot: blocks.len() as u32,
                            puffs: if fl.foam.is_animated() || fl.foam.base > 0.0 {
                                FALL_PUFFS
                            } else {
                                0
                            },
                        });
                        blocks.push(blk);
                    }
                }
                LayerKind::Ribbon(r) => {
                    let mesh = self.ribbon_key(r);
                    let tex = self.texture_key(project, None);
                    self.mesh_tex_bind_group(&tex, &tex, false);
                    let lm = layer_matrix(&layer.transform, ctx);
                    let first = instances.len() as u32;
                    let syms = symmetry_matrices(&layer.symmetry);
                    for sym in &syms {
                        instances.push(InstanceRaw {
                            model: m4(*sym * lm),
                            inst: [0.0, 1.0, 0.0, 0.0],
                        });
                    }
                    let tris = self
                        .meshes
                        .get(&mesh)
                        .map(|g| g.count as u64 / 3)
                        .unwrap_or(0);
                    ls.triangles = tris * syms.len() as u64;
                    ls.draws = 1;
                    ls.load = ls.triangles as f32 / 150_000.0;
                    let mut blk: Block = Zeroable::zeroed();
                    blk[0] = [0.02, 0.02, 0.03, 0.6];
                    blk[1] = c4(r.color, 0.25);
                    blk[2] = [4.0, 0.0, 1.0, 0.0];
                    blk[3] = [0.0, 0.0, 0.2, 0.0];
                    blk[6] = [
                        r.pulses as f32,
                        (ctx.phase * r.pulse_speed as f32).rem_euclid(1.0),
                        r.pulse_length.eval(ctx).clamp(0.001, 1.0),
                        r.pulse_glow.eval(ctx).max(0.0) * flash,
                    ];
                    blk[7] = [r.glow.eval(ctx).max(0.0) * flash, 0.0, 0.0, 0.0];
                    cmds.push(Cmd::Mesh {
                        slot: blocks.len() as u32,
                        mesh,
                        tex: tex.clone(),
                        relief: tex,
                        pixelated: false,
                        first,
                        count: syms.len() as u32,
                    });
                    blocks.push(blk);
                }
                LayerKind::Weather(wx) => {
                    let count = if wx.kind == Precipitation::None {
                        0
                    } else {
                        wx.count.min(100_000)
                    };
                    let mut blk: Block = Zeroable::zeroed();
                    blk[0] = [
                        wx.kind.index() as f32,
                        count as f32,
                        wx.falls.clamp(1, 256) as f32,
                        (wx.seed % 65536) as f32,
                    ];
                    blk[1] = c4(wx.color, wx.size.eval(ctx).max(0.001));
                    let dir_a = wx.wind_dir.to_radians();
                    let dir = [dir_a.cos(), dir_a.sin()];
                    let push = wx.wind.eval(ctx).clamp(-80.0, 80.0).to_radians().tan();
                    blk[2] = [
                        dir[0] * push,
                        dir[1] * push,
                        wx.intensity.eval(ctx).max(0.0) * flash,
                        wx.streak.max(0.0),
                    ];
                    let ground = layer.transform.position[1];
                    blk[3] = [
                        wx.area.max(0.5),
                        wx.height.max(0.1),
                        ground,
                        wx.splashes.eval(ctx).clamp(0.0, 1.0),
                    ];
                    blk[7] = [dir[0], dir[1], 0.0, 0.0];
                    let mut instances = count;
                    let lt = &wx.lightning;
                    if let Some((b, slot, _)) = lt.strike(ctx.beat_phase) {
                        let flash_k = lt.flash.eval(ctx).max(0.0);
                        fx.lightning.0 += b * flash_k;
                        fx.lightning.1 = lt.color;
                        // The bolt: in front of the camera, at a random
                        // bearing and distance for each strike.
                        let hs = |k: u32| {
                            (ez_core::rng::hash_u32(
                                slot.wrapping_mul(0x2c1b_3c6d)
                                    ^ lt.seed.wrapping_mul(0x297a_2d39)
                                    ^ k.wrapping_mul(0x68e3_1da4),
                            ) >> 8) as f32
                                / (1u32 << 24) as f32
                        };
                        let fwd = (cam.target - cam.eye) * Vec3::new(1.0, 0.0, 1.0);
                        let yaw = fwd.z.atan2(fwd.x) + (hs(1) - 0.5) * 1.6;
                        let dist = lt.distance.max(2.0) * (0.7 + 0.6 * hs(2));
                        let foot = Vec3::new(
                            cam.eye.x + yaw.cos() * dist,
                            ground,
                            cam.eye.z + yaw.sin() * dist,
                        );
                        let top = foot
                            + Vec3::new(
                                (hs(3) - 0.5) * dist * 0.3,
                                wx.height.max(4.0) * 2.5,
                                (hs(4) - 0.5) * dist * 0.3,
                            );
                        blk[4] = v4(top, b * flash_k.max(0.3));
                        blk[5] = v4(foot, (slot * 7919 + lt.seed) as f32 % 65536.0);
                        blk[6] = c4(lt.color, 0.0);
                        instances += BOLT_SEGMENTS;
                    }
                    let ground_k = wx.ground.eval(ctx).clamp(0.0, 1.0);
                    match wx.kind {
                        Precipitation::Rain => fx.wet = fx.wet.max(ground_k),
                        Precipitation::Snow => fx.snow = fx.snow.max(ground_k),
                        _ => {}
                    }
                    ls.particles = count as u64;
                    ls.draws = 1;
                    ls.load = count as f32 / 60_000.0 + 0.02;
                    if instances > 0 {
                        cmds.push(Cmd::Weather {
                            slot: blocks.len() as u32,
                            instances,
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
                    blk[0] = [
                        f.size.max(0.1),
                        height,
                        f.reflectivity.eval(ctx).clamp(0.0, 1.0),
                        1.0,
                    ];
                    blk[1] = c4(f.base_color, if f.texture.is_some() { 1.0 } else { 0.0 });
                    blk[2] = c4(f.tint, f.texture_scale);
                    blk[3] = c4(
                        color::scale(f.grid_color, f.grid.eval(ctx).max(0.0) * flash),
                        f.grid_scale.eval(ctx),
                    );
                    blk[4] = [-ctx.phase * f.grid_scroll as f32, 0.0, 0.0, 0.0];
                    floor = Some((blocks.len() as u32, tex, height, f.blur.eval(ctx)));
                    ls.draws = 1;
                    ls.triangles = 2;
                    ls.load = 0.1;
                    blocks.push(blk);
                }
            }
            stats.layers.push(ls);
        }

        // Backgrounds are opaque and fill the screen: only the last one
        // shows, so the others are not drawn at all.
        if let Some(last) = cmds.iter().rposition(|c| matches!(c, Cmd::Backdrop { .. })) {
            let mut i = 0;
            cmds.retain(|c| {
                let keep = !matches!(c, Cmd::Backdrop { .. }) || i == last;
                i += 1;
                keep
            });
        }
        for c in &cmds {
            if let Cmd::Backdrop { kind, res, .. } = c {
                self.ensure_bg_pipe(*kind, self.msaa, false);
                self.ensure_bg_pipe(*kind, 1, false);
                if *res > 1 {
                    self.ensure_bg_pipe(*kind, 1, true);
                }
            }
        }

        // Contact shadows under shapes on the mirror floor.
        let contact = project.environment.shadows.contact;
        if contact > 0.0 {
            if let Some((_, _, fh, _)) = &floor {
                let meshes: Vec<(u32, u32)> = cmds
                    .iter()
                    .filter_map(|c| match c {
                        Cmd::Mesh { first, count, .. } if *count > 0 => Some((*first, *count)),
                        _ => None,
                    })
                    .collect();
                if !meshes.is_empty() {
                    let mut blk: Block = Zeroable::zeroed();
                    blk[0] = [*fh, contact.clamp(0.0, 1.0), 0.0, 0.0];
                    let slot = blocks.len() as u32;
                    blocks.push(blk);
                    for (first, count) in meshes {
                        cmds.push(Cmd::Contact { slot, first, count });
                    }
                }
            }
        }
        let e = &project.environment;
        if e.day_cycle.enabled || e.rainbow.base != 0.0 || e.rainbow.is_animated() {
            cmds.push(Cmd::SkyFx);
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

        // A mirror floor that is off screen, or seen from below, shows no
        // reflection: skip rendering the scene a second time.
        if let Some((slot, _, fh, _)) = &floor {
            let size = blocks[*slot as usize][0][0];
            let vp = proj * view;
            let planes = frustum_planes(vp);
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .map(|(x, z)| Vec3::new(x * size, *fh, z * size));
            // Off screen when all four corners lie outside the same plane.
            let off = planes
                .iter()
                .any(|p| corners.iter().all(|c| p.truncate().dot(*c) + p.w < 0.0));
            if off || cam.eye.y < *fh {
                blocks[*slot as usize][0][3] = 0.0;
                self.stats.reflection = false;
            }
        }

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
        // Sun shadow map: an orthographic view from the sun around the
        // camera's target, snapped to whole texels so edges don't shimmer.
        let sh = &project.environment.shadows;
        let casters = cmds
            .iter()
            .any(|c| matches!(c, Cmd::Mesh { .. } | Cmd::Terrain { .. }));
        let shadow = (sh.enabled && casters).then(|| {
            let r = sh.distance.max(1.0);
            let l = Vec3::from(env.light_dir).normalize_or(Vec3::Y);
            let centre = cam.target;
            let up = if l.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
            let eye = centre + l * r * 2.0;
            let lview = Mat4::look_at_rh(eye, centre, up);
            let lproj = Mat4::orthographic_rh(-r, r, -r, r, 0.05, r * 4.0);
            let c = (lproj * lview).project_point3(centre);
            let texel = 2.0 / SHADOW_SIZE as f32;
            let snap = Vec3::new(
                (c.x / texel).round() * texel - c.x,
                (c.y / texel).round() * texel - c.y,
                0.0,
            );
            let lproj = Mat4::from_translation(snap) * lproj;
            (lview, lproj, eye, r)
        });
        if let Some((lview, lproj, eye, _)) = shadow {
            let g = Self::globals(
                project,
                ctx,
                &env,
                lview,
                lproj,
                eye,
                (SHADOW_SIZE, SHADOW_SIZE),
                Vec4::ZERO,
                &fx,
            );
            self.queue
                .write_buffer(&self.globals_buf[2], 0, bytemuck::bytes_of(&g));
        }
        let with_shadow = |mut g: GlobalsRaw| {
            if let Some((lview, lproj, _, r)) = shadow {
                g.shadow_vp = m4(lproj * lview);
                g.shadow = [
                    sh.strength.clamp(0.0, 1.0),
                    sh.softness.max(0.0),
                    1.0 / SHADOW_SIZE as f32,
                    2.0 * r / SHADOW_SIZE as f32 * 1.5,
                ];
            }
            g
        };
        let main_globals = Self::globals(
            project,
            ctx,
            &env,
            view,
            proj,
            cam.eye,
            (w, h),
            Vec4::ZERO,
            &fx,
        );
        let main_globals = with_shadow(main_globals);
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
                &env,
                rview,
                proj,
                reye,
                target.refl_size,
                Vec4::new(0.0, 1.0, 0.0, -fh + 0.001),
                &fx,
            );
            let g = with_shadow(g);
            self.queue
                .write_buffer(&self.globals_buf[1], 0, bytemuck::bytes_of(&g));
        }
        self.write_post_params(
            project,
            ctx,
            target,
            floor.as_ref().map(|f| f.3).unwrap_or(0.0),
            proj * view,
            cam.eye,
            &env,
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

        // --- sun shadow map -------------------------------------------------------
        if shadow.is_some() {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.draw_shadow_casters(&mut pass, &cmds);
        }

        // --- reflection -------------------------------------------------------
        let reflect = floor
            .as_ref()
            .is_some_and(|(slot, ..)| blocks_refl_on(&blocks, *slot));
        if reflect {
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
                let (solid, clear): (Vec<Cmd>, Vec<Cmd>) = cmds.iter().cloned().partition(|c| {
                    matches!(
                        c,
                        Cmd::Backdrop { .. } | Cmd::SkyFx | Cmd::Mesh { .. } | Cmd::Terrain { .. }
                    )
                });
                self.draw_scene(&mut pass, &self.refl_pipes, 1, &solid);
                self.draw_scene(&mut pass, &self.refl_pipes, 1, &clear);
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

        // --- low-resolution background ------------------------------------------
        let low_bg = cmds.iter().find_map(|c| match c {
            Cmd::Backdrop {
                slot,
                tex,
                res,
                kind,
            } if *res > 1 => Some((*slot, tex.clone(), *res, *kind)),
            _ => None,
        });
        if let Some((slot, tex, res, kind)) = &low_bg {
            if let Some((_, view, _)) = target.bg_low.iter().find(|(d, _, _)| d == res) {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("background low"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.bg_pipes[&(*kind, 1, true)]);
                pass.set_bind_group(0, &self.globals_bg[0], &[]);
                pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                pass.set_bind_group(2, &self.tex_bgs[&(tex.clone(), false)], &[]);
                pass.draw(0..3, 0..1);
            }
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
                            r: main_globals.fog[0] as f64,
                            g: main_globals.fog[1] as f64,
                            b: main_globals.fog[2] as f64,
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
                .partition(|c| matches!(c, Cmd::Backdrop { .. } | Cmd::SkyFx));
            let back: Vec<Cmd> = back
                .into_iter()
                .map(|c| match c {
                    Cmd::Backdrop { res, .. } if res > 1 => Cmd::BackdropUp { res },
                    other => other,
                })
                .collect();
            for c in &back {
                if let Cmd::BackdropUp { res } = c {
                    if let Some((_, _, bg)) = target.bg_low.iter().find(|(d, _, _)| d == res) {
                        pass.set_pipeline(&self.bg_up_pipe);
                        pass.set_bind_group(0, &self.globals_bg[0], &[]);
                        pass.set_bind_group(1, &self.draw_bg, &[0]);
                        pass.set_bind_group(2, bg, &[]);
                        pass.draw(0..3, 0..1);
                    }
                }
            }
            self.draw_scene(&mut pass, &self.main_pipes, 0, &back);
            if let (Some((slot, _, _, _)), Some(bg)) = (&floor, &floor_bg) {
                pass.set_pipeline(&self.floor_pipe);
                pass.set_bind_group(0, &self.globals_bg[0], &[]);
                pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                pass.set_bind_group(2, *bg, &[]);
                pass.set_bind_group(3, &self.shadow_bg, &[]);
                pass.draw(0..6, 0..1);
                for c in &cmds {
                    if let Cmd::Contact { slot, first, count } = c {
                        pass.set_pipeline(&self.contact_pipe);
                        pass.set_bind_group(0, &self.globals_bg[0], &[]);
                        pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                        pass.set_vertex_buffer(0, self.inst_buf.slice(..));
                        pass.draw(0..6, *first..*first + *count);
                    }
                }
            }
            // Solid geometry before anything see-through, which doesn't
            // write depth and would otherwise be painted over.
            // Skip what is entirely outside the view (the reflection and the
            // shadow map still get everything).
            let planes = frustum_planes(proj * view);
            let rest: Vec<Cmd> = rest
                .into_iter()
                .filter(|c| match self.cmd_bounds(c, &blocks) {
                    Some((centre, r)) => sphere_visible(&planes, centre, r),
                    None => true,
                })
                .collect();
            let (solid, clear): (Vec<Cmd>, Vec<Cmd>) = rest
                .into_iter()
                .partition(|c| matches!(c, Cmd::Mesh { .. } | Cmd::Terrain { .. }));
            self.draw_scene(&mut pass, &self.main_pipes, 0, &solid);
            self.draw_scene(&mut pass, &self.main_pipes, 0, &clear);
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
        if project.post.rays.enabled {
            self.post_pass(
                &mut enc,
                "god rays",
                &self.rays_pipe,
                &target.rays,
                &target.bg_rays,
                SLOT_RAYS,
                false,
            );
            self.post_pass(
                &mut enc,
                "god rays add",
                &self.rays_add_pipe,
                &target.hdr2,
                &target.bg_rays_add,
                SLOT_RAYS_ADD,
                true,
            );
        }
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
        {
            let att = |view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("final"),
                color_attachments: &[att(&target.output_view), att(&target.display_view)],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.final_pipe);
            pass.set_bind_group(0, &target.bg_final, &[SLOT_FINAL * POST_SLOT as u32]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit([enc.finish()]);
    }

    /// A bounding sphere of what a command draws, when it is cheap to know.
    fn cmd_bounds(&self, cmd: &Cmd, blocks: &[Block]) -> Option<(Vec3, f32)> {
        let model = |slot: u32| {
            let b = &blocks[slot as usize];
            Mat4::from_cols_array_2d(&[b[8], b[9], b[10], b[11]])
        };
        let scale = |m: &Mat4| {
            m.x_axis
                .truncate()
                .length()
                .max(m.y_axis.truncate().length())
                .max(m.z_axis.truncate().length())
        };
        match cmd {
            Cmd::Mesh {
                mesh,
                first,
                count,
                slot,
                ..
            } => {
                // Deformed shapes reach further (blk[10].w, 0 = none).
                let reach = blocks[*slot as usize][10][3].max(1.0);
                let mr = self.meshes.get(mesh)?.radius * reach;
                let inst = self
                    .uploaded
                    .get(*first as usize..(*first + *count) as usize)?;
                if inst.is_empty() {
                    return None;
                }
                let mut lo = Vec3::splat(f32::MAX);
                let mut hi = Vec3::splat(f32::MIN);
                let mut rmax = 0.0f32;
                for i in inst {
                    let m = Mat4::from_cols_array_2d(&i.model);
                    let c = m.w_axis.truncate();
                    lo = lo.min(c);
                    hi = hi.max(c);
                    rmax = rmax.max(scale(&m) * mr);
                }
                let centre = (lo + hi) * 0.5;
                // Glitch and displacement can push vertices out a little.
                Some((centre, (hi - lo).length() * 0.5 + rmax * 1.5 + 0.5))
            }
            Cmd::Particles { slot, .. } => {
                let m = model(*slot);
                let b = &blocks[*slot as usize];
                let r = (b[3][1] * 3.5 * b[3][0].abs().max(1.0) + b[1][3]) * scale(&m);
                Some((m.w_axis.truncate(), r + 1.0))
            }
            Cmd::Lasers { slot, .. } | Cmd::Spots { slot, .. } => {
                let m = model(*slot);
                let len = blocks[*slot as usize][0][3];
                Some((m.w_axis.truncate(), len * scale(&m) * 1.3 + 1.0))
            }
            _ => None,
        }
    }

    /// Meshes and terrain seen from the sun, depth only.
    fn draw_shadow_casters(&self, pass: &mut wgpu::RenderPass<'_>, cmds: &[Cmd]) {
        for cmd in cmds {
            match cmd {
                Cmd::Mesh {
                    slot,
                    mesh,
                    tex,
                    relief,
                    pixelated,
                    first,
                    count,
                } if *count > 0 => {
                    let m = &self.meshes[mesh];
                    pass.set_pipeline(&self.shadow_mesh_pipe);
                    pass.set_bind_group(0, &self.globals_bg[2], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_bind_group(
                        2,
                        &self.mesh_tex_bgs[&(tex.clone(), relief.clone(), *pixelated)],
                        &[],
                    );
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_vertex_buffer(1, self.inst_buf.slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.count, 0, *first..*first + *count);
                }
                Cmd::Terrain {
                    slot,
                    vertices,
                    tex,
                    pixelated,
                } => {
                    pass.set_pipeline(&self.shadow_terrain_pipe);
                    pass.set_bind_group(0, &self.globals_bg[2], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_bind_group(2, &self.tex_bgs[&(tex.clone(), *pixelated)], &[]);
                    pass.draw(0..*vertices, 0..1);
                }
                _ => {}
            }
        }
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
                Cmd::Text {
                    slot,
                    font,
                    first,
                    count,
                } => {
                    pass.set_pipeline(&pipes.text);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_bind_group(2, &self.tex_bgs[&(font.clone(), false)], &[]);
                    pass.set_vertex_buffer(0, self.inst_buf.slice(..));
                    pass.draw(0..6, *first..*first + *count);
                }
                Cmd::BackdropUp { .. } => {}
                Cmd::Backdrop {
                    slot, tex, kind, ..
                } => {
                    pass.set_pipeline(&self.bg_pipes[&(*kind, pipes.samples, false)]);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_bind_group(2, &self.tex_bgs[&(tex.clone(), false)], &[]);
                    pass.draw(0..3, 0..1);
                }
                Cmd::Mesh {
                    slot,
                    mesh,
                    tex,
                    relief,
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
                    pass.set_bind_group(
                        2,
                        &self.mesh_tex_bgs[&(tex.clone(), relief.clone(), *pixelated)],
                        &[],
                    );
                    pass.set_bind_group(3, &self.shadow_bg, &[]);
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
                Cmd::Terrain {
                    slot,
                    vertices,
                    tex,
                    pixelated,
                } => {
                    pass.set_pipeline(&pipes.terrain);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_bind_group(2, &self.tex_bgs[&(tex.clone(), *pixelated)], &[]);
                    pass.set_bind_group(3, &self.shadow_bg, &[]);
                    pass.draw(0..*vertices, 0..1);
                }
                Cmd::Lasers { slot, beams } => {
                    pass.set_pipeline(&pipes.lasers);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.draw(0..6, 0..*beams);
                }
                Cmd::Contact { .. } => {}
                Cmd::SkyFx => {
                    for pipe in [&pipes.sky_mul, &pipes.sky_add] {
                        pass.set_pipeline(pipe);
                        pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                        pass.set_bind_group(1, &self.draw_bg, &[0]);
                        pass.draw(0..3, 0..1);
                    }
                }
                Cmd::Spots { slot, beams, pools } => {
                    pass.set_pipeline(&pipes.spots);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.draw(0..SPOT_VERTICES, 0..*beams);
                    if *pools {
                        pass.draw(0..6, *beams..*beams * 2);
                    }
                }
                Cmd::Falls { slot, puffs } => {
                    pass.set_pipeline(&pipes.falls);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.draw(0..FALL_VERTICES, 0..1);
                    if *puffs > 0 {
                        pass.draw(0..6, 1..1 + *puffs);
                    }
                }
                Cmd::Weather { slot, instances } => {
                    pass.set_pipeline(&pipes.weather);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.draw(0..6, 0..*instances);
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

    #[allow(clippy::too_many_arguments)]
    fn write_post_params(
        &self,
        project: &Project,
        ctx: &EvalCtx,
        target: &RenderTarget,
        blur: f32,
        view_proj: Mat4,
        eye: Vec3,
        env: &EnvState,
    ) {
        let post = &project.post;
        let mut slots: Vec<PostBlock> = vec![Zeroable::zeroed(); SLOT_RAYS_ADD as usize + 1];
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
            k.angle.eval(ctx).to_radians() + TAU * k.turns as f32 * ctx.phase,
            k.zoom.eval(ctx),
        ];
        slots[SLOT_WARP as usize][1] = [
            k.center[0],
            k.center[1],
            target.width as f32 / target.height as f32,
            mirror_mode as f32,
        ];
        let hz = &post.haze;
        if hz.enabled {
            slots[SLOT_WARP as usize][2] = [
                hz.amount.eval(ctx).max(0.0),
                hz.scale.max(0.05),
                TAU * (ctx.phase * hz.speed as f32).rem_euclid(1.0),
                match hz.region {
                    HazeRegion::HotSpots => 0.0,
                    HazeRegion::Ground => 1.0,
                    HazeRegion::Everywhere => 2.0,
                },
            ];
        }

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
                b.threshold.eval(ctx).max(0.0),
                if i == 0 { 1.0 } else { 0.0 },
            ];
        }
        let up_w = 0.35 + 0.65 * b.radius.eval(ctx).clamp(0.0, 1.0);
        for i in 0..BLOOM_LEVELS - 1 {
            let (_, sw, sh) = target.bloom[i + 1];
            slots[(SLOT_BLOOM_UP as usize) + i][0] = [1.0 / sw as f32, 1.0 / sh as f32, 0.0, 0.0];
            slots[(SLOT_BLOOM_UP as usize) + i][1] = [up_w, 0.0, 0.0, 0.0];
        }

        let gr = &post.rays;
        if gr.enabled {
            let (light_uv, vis) = match gr.source {
                RaySource::Centre => ([0.5, 0.5], 1.0),
                RaySource::Sun => {
                    let ld = Vec3::from(env.light_dir).normalize_or(Vec3::Y);
                    let clip = view_proj * (eye + ld * 1000.0).extend(1.0);
                    if clip.w <= 1e-4 || env.night > 0.97 {
                        ([0.5, 0.5], 0.0)
                    } else {
                        let (x, y) = (clip.x / clip.w, clip.y / clip.w);
                        // Fade out as the light leaves the picture.
                        let off = (x.abs().max(y.abs()) - 1.0).max(0.0);
                        (
                            [x * 0.5 + 0.5, 0.5 - y * 0.5],
                            (1.0 - off * 0.6).clamp(0.0, 1.0),
                        )
                    }
                }
            };
            slots[SLOT_RAYS as usize][0] = [
                light_uv[0],
                light_uv[1],
                vis,
                gr.intensity.eval(ctx).max(0.0),
            ];
            slots[SLOT_RAYS as usize][1] = [
                gr.length.eval(ctx).clamp(0.0, 1.0),
                gr.threshold.eval(ctx).max(0.0),
                target.width as f32 / target.height as f32,
                gr.flare.eval(ctx).max(0.0),
            ];
            slots[SLOT_RAYS as usize][2] = c4(gr.tint, 0.0);
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
            g.contrast.eval(ctx),
            g.saturation.eval(ctx),
            g.vignette.eval(ctx),
        ];
        let bloom_k = if b.enabled {
            b.intensity.eval(ctx).max(0.0) * 0.5
        } else {
            0.0
        };
        f[2] = [
            g.grain.eval(ctx),
            g.beat_flash.eval(ctx),
            if post.chroma.enabled {
                post.chroma.amount.eval(ctx).max(0.0)
            } else {
                0.0
            },
            bloom_k,
        ];
        // Scale fat pixels relative to 1080p so previews match exports.
        let pix = if post.pixelate.enabled {
            (post.pixelate.size.eval(ctx) * target.height as f32 / 1080.0 * 2.0).max(1.0)
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
        f[3] = [pix, count, post.palette.dither.eval(ctx), ctx.phase];
        f[4] = [
            if post.crt.enabled { 1.0 } else { 0.0 },
            post.crt.scanlines.eval(ctx),
            post.crt.curvature.eval(ctx),
            post.crt.noise.eval(ctx),
        ];
        let frames = ctx.loop_beats as f32 * 6.0;
        let frame_id = (ctx.beat_phase * frames).floor().rem_euclid(frames);
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
            view_formats: &[],
        });
        let output_view = output.create_view(&Default::default());
        let display = dev.create_texture(&wgpu::TextureDescriptor {
            label: Some("display"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DISPLAY_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let display_view = display.create_view(&Default::default());
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
        let bg_low = [2u32, 4]
            .into_iter()
            .map(|d| {
                let view = tex(
                    "background low",
                    (w / d).max(1),
                    (h / d).max(1),
                    HDR_FORMAT,
                    1,
                    sampled,
                );
                let bg = dev.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("background low"),
                    layout: &self.bgl_tex,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler_clamp),
                        },
                    ],
                });
                (d, view, bg)
            })
            .collect();
        let rays = tex("god rays", rw, rh, HDR_FORMAT, 1, sampled);
        let bg_rays = post_bg(&hdr2, &hdr2);
        let bg_rays_add = post_bg(&rays, &rays);
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
            display,
            display_view,
            bg_blur_h,
            bg_blur_v,
            bg_warp,
            bg_bloom_down,
            bg_bloom_up,
            bg_final,
            bg_low,
            bg_rays,
            bg_rays_add,
            rays,
        }
    }

    /// Start copying the final image of `target` back to the CPU without
    /// waiting (required on the web, where blocking is impossible). Poll the
    /// returned [`Readback`] until it is ready.
    pub fn start_readback(&self, target: &RenderTarget) -> Readback {
        self.readback_texture(&target.output, target.width, target.height)
    }

    /// Copy the display image (what the editor shows) back to the CPU
    /// (blocking; desktop only). Used to check it matches the export image.
    pub fn read_display_pixels(&self, target: &RenderTarget) -> Vec<u8> {
        let rb = self.readback_texture(&target.display, target.width, target.height);
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        rb.take().expect("readback finished")
    }

    fn readback_texture(&self, texture: &wgpu::Texture, w: u32, h: u32) -> Readback {
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
                texture,
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
        let index = self.queue.submit([enc.finish()]);
        let ready = Arc::new(AtomicBool::new(false));
        let flag = ready.clone();
        buf.slice(..).map_async(wgpu::MapMode::Read, move |r| {
            if r.is_ok() {
                flag.store(true, Ordering::Release);
            }
        });
        Readback {
            buf,
            index,
            ready,
            width: w,
            height: h,
            padded,
        }
    }

    /// Copy the final image back to the CPU as tightly packed sRGB RGBA8
    /// (blocking; desktop only).
    pub fn read_pixels(&self, target: &RenderTarget) -> Vec<u8> {
        let rb = self.start_readback(target);
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        rb.take().expect("readback finished")
    }

    /// Blocks until `rb` has finished (not later frames). Native only.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn wait_for(&self, rb: &Readback) {
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(rb.index.clone()),
            timeout: None,
        });
    }

    /// Lets pending GPU work (e.g. readbacks) make progress without
    /// blocking. On the web the browser does this by itself.
    pub fn poll(&self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
    }

    /// Blocks until all submitted GPU work, including readbacks, is done.
    /// Browsers cannot block, so this is native only.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn wait(&self) {
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
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

/// A pending GPU -> CPU copy of a rendered frame.
pub struct Readback {
    buf: wgpu::Buffer,
    /// Submission of the copy (to wait for exactly this frame on native).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    index: wgpu::SubmissionIndex,
    ready: Arc<AtomicBool>,
    pub width: u32,
    pub height: u32,
    padded: u32,
}

impl Readback {
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Tightly packed RGBA8 pixels, or `None` if not finished yet.
    pub fn take(self) -> Option<Vec<u8>> {
        if !self.is_ready() {
            return None;
        }
        let row = (4 * self.width) as usize;
        let data = self.buf.slice(..).get_mapped_range().ok()?;
        let mut out = Vec::with_capacity(row * self.height as usize);
        for y in 0..self.height as usize {
            let s = y * self.padded as usize;
            out.extend_from_slice(&data[s..s + row]);
        }
        drop(data);
        self.buf.unmap();
        Some(out)
    }
}

/// Highest MSAA sample count (4 or 1) the adapter supports for the HDR
/// scene targets. WebGL2 devices often can't multisample float targets.
pub fn supported_msaa(adapter: &wgpu::Adapter) -> u32 {
    let f = adapter.get_texture_format_features(HDR_FORMAT);
    let d = adapter.get_texture_format_features(DEPTH_FORMAT);
    if f.flags.sample_count_supported(4)
        && d.flags.sample_count_supported(4)
        && f.allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
    {
        4
    } else {
        1
    }
}

#[cfg(test)]
mod cull_tests {
    use super::*;

    #[test]
    fn frustum_culling() {
        let view = Mat4::look_at_rh(Vec3::new(0.0, 0.0, 10.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective_rh(1.0, 16.0 / 9.0, 0.1, 100.0);
        let planes = frustum_planes(proj * view);
        assert!(sphere_visible(&planes, Vec3::ZERO, 1.0));
        // Behind the camera, far to the side, beyond the far plane.
        assert!(!sphere_visible(&planes, Vec3::new(0.0, 0.0, 20.0), 1.0));
        assert!(!sphere_visible(&planes, Vec3::new(60.0, 0.0, 0.0), 1.0));
        assert!(!sphere_visible(&planes, Vec3::new(0.0, 0.0, -200.0), 1.0));
        // Partly inside counts as visible.
        assert!(sphere_visible(&planes, Vec3::new(0.0, 0.0, 20.0), 12.0));
    }
}
