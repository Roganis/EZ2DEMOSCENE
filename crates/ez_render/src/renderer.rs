use crate::import::load_mesh_asset;
use crate::mesh::{primitive, MeshData, Vertex};
use crate::texgen;
use bytemuck::{Pod, Zeroable};
use ez_core::eval::{
    copies_with, instances_are_static, layer_frame, layer_matrix, layer_scale, mesh_instances_with,
    symmetry_matrices, Instance,
};
use ez_core::palette::PaletteId;
use ez_core::*;
use glam::{Mat4, Vec2, Vec3, Vec4};
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
/// Radius of an endless mirror floor: just inside the camera's far plane
/// (500), so it fades out before it is clipped.
const INFINITE_FLOOR_RADIUS: f32 = 450.0;
/// Cells per side of a morph's distance fields.
const MORPH_GRID: u32 = 64;

/// Tiles per row of a morph texture (see `sdf_bake::pack_pair`).
fn morph_columns() -> u32 {
    (MORPH_GRID as f32).sqrt().ceil() as u32
}
/// A logo's effect settings: three draw slots read as one.
const LOGO_FX_SIZE: u64 = 3 * DRAW_SLOT;
const POST_SLOT: u64 = 512;
/// Distance to the camera for depth of field.
const DOF_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;
/// Steps of a screen-space reflection ray (before homing in on a hit).
const SSR_STEPS: u32 = 32;
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
const SLOT_FEEDBACK: u32 = 17;

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
    /// Environment map: intensity (0 = off), turn (radians), roughest
    /// mip, sky mode (see `common.wgsl`).
    ibl: [f32; 4],
    /// Its diffuse light (9 spherical-harmonic coefficients, rgb).
    sh: [[f32; 4]; 9],
    /// Retro 3D: vertex snapping (0 = off), its grid (w, h), affine warp.
    retro: [f32; 4],
    /// Nintendo 64 fog: on, start, solid.
    retro_fog: [f32; 4],
    /// Colour levels (0 = full), dither, near-plane culling (camera views).
    retro_col: [f32; 4],
}

/// An environment map on the GPU and what the CPU worked out from it.
struct EnvMaps {
    /// What it was made from (see `Renderer::env_key`).
    key: String,
    view: wgpu::TextureView,
    sh: [[f32; 4]; 9],
    /// Its sun, taken out of the map (when asked for).
    sun: Option<crate::envmap::Sun>,
    /// The panorama as a texture, for the Environment map background.
    pano: Option<String>,
    /// A small, blurred copy on the CPU (for logos' environment sphere).
    tiny: Option<crate::envmap::Equirect>,
}

/// Side of a captured sky's cube faces.
const SKY_SIZE: u32 = 64;

/// "From the sky": the background captured into a cube each frame, then
/// blurred into the environment map's roughness mips.
struct SkyCapture {
    /// The capture (one mip) and a face view of each side to draw into.
    src_faces: Vec<wgpu::TextureView>,
    /// The environment map: a view per face and mip to draw into, and the
    /// whole cube to light with.
    dst_faces: Vec<wgpu::TextureView>,
    dst_cube: wgpu::TextureView,
    /// Globals of each face's view.
    globals: Vec<wgpu::Buffer>,
    globals_bg: Vec<wgpu::BindGroup>,
    filter_pipe: wgpu::RenderPipeline,
    /// One bind group per mip and face (its parameters, the capture).
    filter_bgs: Vec<wgpu::BindGroup>,
    /// What was captured last (a sky that doesn't move is captured once).
    captured: Option<u64>,
}

/// Group 3 of lit surfaces: the sun shadow map, the environment map and
/// the BRDF table.
struct Group3 {
    layout: wgpu::BindGroupLayout,
    shadow_view: wgpu::TextureView,
    shadow_sampler: wgpu::Sampler,
    env_sampler: wgpu::Sampler,
    lut_view: wgpu::TextureView,
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
    /// Environment map lighting (see `GlobalsRaw::ibl`, `sh`).
    ibl: [f32; 4],
    sh: [[f32; 4]; 9],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceRaw {
    model: [[f32; 4]; 4],
    inst: [f32; 4],
}

type Block = [[f32; 4]; 16];
/// A mesh's pictures: colour, relief, occlusion/roughness/metal, glow,
/// and how they are sampled (see [`sampler_kind`]).
type MeshTexKey = (String, String, String, String, u8);

/// Samplers by texture filter: 0 smooth, 1 nearest (also for the
/// three-point filter, worked out in the shader), 2 bilinear without
/// mipmaps.
fn sampler_kind(f: ez_core::TexFilter) -> u8 {
    match f {
        ez_core::TexFilter::Smooth => 0,
        ez_core::TexFilter::Nearest | ez_core::TexFilter::ThreePoint => 1,
        ez_core::TexFilter::Bilinear => 2,
    }
}

/// Ends the texture key of a picture tiled mirrored.
const MIRROR_KEY: &str = ":mirror";
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
    /// Uses two draw slots: `slot` and the next (the physical material).
    Mesh {
        slot: u32,
        mesh: String,
        texs: MeshTexKey,
        /// A liquid drawn as one surface (`liquid.wgsl`): not drawn as
        /// copies in the main picture (still in shadows and reflections).
        liquid: bool,
        first: u32,
        count: u32,
        /// A raymarched object in its box proxy (`sdf.wgsl`).
        sdf: bool,
        /// Instances made by the copies compute pass (in `SwarmGpu::out`).
        gpu: bool,
        /// Where GPU-placed copies can be (for culling; none = always drawn).
        bounds: Option<(Vec3, f32)>,
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
    /// Electric arcs: `count` arcs of `segments` quads each.
    Arcs {
        slot: u32,
        segments: u32,
        first: u32,
        count: u32,
    },
    Sprite {
        slot: u32,
        tex: String,
        pixelated: bool,
        blend: SpriteBlend,
        first: u32,
        count: u32,
    },
    /// A logo flat on the screen, drawn after the scene (see `logo.wgsl`):
    /// its blocks at `slot` and `slot + 1`, its texture, the logo it morphs
    /// into and its material sphere.
    Logo {
        slot: u32,
        tex: String,
        morph: String,
        matcap: String,
        /// Needs the picture behind it (glass).
        glass: bool,
    },
    /// Contact shadows under the copies `first..first + count` of a mesh.
    Contact {
        slot: u32,
        first: u32,
        count: u32,
        /// The copies are in the compute pass's buffer.
        gpu: bool,
    },
}

/// Place logo `i` (see `Renderer::place_logos`): measured from the screen
/// or from a point of the logo it is attached to, placed first. A missing
/// target or a loop of attachments falls back to the screen.
#[allow(clippy::too_many_arguments)]
fn place_logo(
    i: usize,
    layers: &[Layer],
    fits: &[Option<LogoFit>],
    ctx: &EvalCtx,
    w: f32,
    h: f32,
    out: &mut [Option<Vec2>],
    state: &mut [u8],
) {
    if state[i] != 0 {
        return;
    }
    state[i] = 1;
    let (LayerKind::Logo(g), Some(_)) = (&layers[i].kind, &fits[i]) else {
        state[i] = 2;
        return;
    };
    let [px, py] = g.attach_point.point();
    let screen = Vec2::new(px * w, py * h);
    let target = (!g.attach_to.is_empty())
        .then(|| {
            layers.iter().enumerate().position(|(j, l)| {
                j != i && l.enabled && l.name == g.attach_to && matches!(l.kind, LayerKind::Logo(_))
            })
        })
        .flatten();
    let origin = match target {
        // Not while it is being placed itself (a loop).
        Some(j) if state[j] != 1 => {
            place_logo(j, layers, fits, ctx, w, h, out, state);
            match (&layers[j].kind, out[j], &fits[j]) {
                (LayerKind::Logo(t), Some(at), Some(fit)) => {
                    let th = t.size.eval(ctx).max(0.0) * h;
                    let size = Vec2::new(th * fit.aspect, th);
                    let [ax, ay] = t.anchor.point();
                    let local = (Vec2::new(px, py) - Vec2::new(ax, ay)) * size;
                    let a = t.rotation.eval(ctx).to_radians();
                    at + Vec2::new(
                        local.x * a.cos() - local.y * a.sin(),
                        local.x * a.sin() + local.y * a.cos(),
                    )
                }
                _ => screen,
            }
        }
        _ => screen,
    };
    out[i] = Some(origin + Vec2::new(g.x.eval(ctx) * w, g.y.eval(ctx) * h));
    state[i] = 2;
}

/// Texture key of a logo's coarse far field.
fn far_key(logo: &str) -> String {
    format!("{logo}:far")
}

/// Texture keys of a logo's bind group: the logo, the logo it morphs
/// into, its material sphere.
type LogoKey = (String, String, String);

/// One logo draw: its four blocks, bind group key and fit.
type LogoDraw = ([Block; 4], LogoKey, LogoFit);

/// Rays streaming from a logo, drawn after the logos.
struct LogoRays {
    /// The logo's first block and textures.
    slot: u32,
    key: LogoKey,
    /// Middle of the logo (post-pass coordinates: 0..1, y down).
    centre: [f32; 2],
    strength: f32,
    length: f32,
    threshold: f32,
    tint: [f32; 3],
    /// From the light behind the logo, the logo cutting a shadow out.
    shadow: bool,
}

/// A target's logo rays source picture and its post bind group.
struct LogoRaysSource {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    bg: wgpu::BindGroup,
    size: (u32, u32),
    /// Frame it was last used.
    used: u64,
}

/// Post slots for logo rays (one logo each).
const SLOT_LOGO_RAYS: u32 = 18;
const LOGO_RAYS_MAX: usize = 4;

/// How a baked logo texture maps onto the screen.
#[derive(Clone, Copy, Debug)]
struct LogoFit {
    width: u32,
    height: u32,
    /// Width over height of the shape.
    aspect: f32,
    /// Padding around the shape (fraction of its width and height).
    pad: [f32; 2],
    /// Distance field spread in texels.
    spread: f32,
    /// The field at the logo's thickest point.
    max_field: f32,
    /// Far field: origin (texels), cell size, size (cells).
    far: [f32; 5],
}

impl LogoFit {
    /// Logo heights per unit of the field (0.5 per spread).
    fn heights_per_field(&self) -> f32 {
        let ch = self.height as f32 / (1.0 + 2.0 * self.pad[1]);
        2.0 * self.spread / ch.max(1.0)
    }

    /// Texels across the shape's height.
    fn content_height(&self) -> f32 {
        self.height as f32 / (1.0 + 2.0 * self.pad[1])
    }
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

/// A battle background's settings in draw block slots 8..15 (see
/// `bg_battle` in backdrop.wgsl). Everything that moves is a whole number
/// of turns per loop, wrapped, so the loop closes.
fn battle_block(blk: &mut Block, b: &ez_core::Battle, ctx: &EvalCtx, mirror: bool) {
    let turn = |n: i32| (n as f32 * ctx.phase).rem_euclid(1.0);
    // A mirrored picture repeats every two tiles: it scrolls by pairs.
    let tiles = if mirror { 2.0 } else { 1.0 };
    let slide = |n: i32| (n as f32 * ctx.phase * tiles).rem_euclid(tiles);
    for (i, l) in [&b.back, &b.front].into_iter().enumerate() {
        let o = 8 + i * 3;
        blk[o] = [
            if l.enabled { 1.0 } else { 0.0 },
            l.pattern.index() as f32,
            l.tiles.eval(ctx).max(0.05),
            l.warp.index() as f32,
        ];
        blk[o + 1] = [
            l.amount.eval(ctx),
            l.waves.eval(ctx),
            turn(l.wave_speed),
            l.bands.eval(ctx).max(0.0),
        ];
        blk[o + 2] = [
            slide(l.scroll[0]),
            slide(l.scroll[1]),
            turn(l.cycles),
            l.opacity.eval(ctx).clamp(0.0, 1.0),
        ];
    }
    blk[14] = [
        b.blend.index() as f32,
        b.lines.min(4096) as f32,
        b.steps.min(256) as f32,
        0.0,
    ];
}

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
        BackdropKind::Battle => 0.15,
        BackdropKind::Environment => 0.05,
    }
}

/// Key for a layer's cached instances. Instances carry no colour, so the
/// colours are left out: a turning colour scheme then keeps the cache.
fn layer_hash(layer: &Layer) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut layer = layer.clone();
    layer.kind.for_each_color_mut(|c| *c = [0.0; 3]);
    serde_json::to_string(&layer)
        .unwrap_or_default()
        .hash(&mut h);
    h.finish()
}

/// What the main scene pass draws (see `Renderer::draw_main`).
struct MainDraw<'a> {
    back: &'a [Cmd],
    solid: &'a [Cmd],
    clear: &'a [Cmd],
    /// Everything (for the floor's contact shadows).
    cmds: &'a [Cmd],
    /// The mirror floor's draw slot and bind group.
    floor: Option<(u32, &'a wgpu::BindGroup)>,
    /// The liquid drawn as a surface (its draw slot).
    liquid: Option<u32>,
}

/// The pipelines of one main scene pass (they depend on its samples) and
/// its globals.
struct PassPipes<'a> {
    scene: &'a ScenePipes,
    floor: &'a wgpu::RenderPipeline,
    contact: &'a wgpu::RenderPipeline,
    bg_up: &'a wgpu::RenderPipeline,
    liquid: &'a wgpu::RenderPipeline,
    globals: usize,
}

/// Single-sample versions of the main pass's own pipelines, for the retro
/// low-resolution pass (it draws the scene without antialiasing).
struct LowPipes {
    floor: wgpu::RenderPipeline,
    contact: wgpu::RenderPipeline,
    bg_up: wgpu::RenderPipeline,
    liquid: wgpu::RenderPipeline,
}

/// The retro low-resolution scene of a render target: colour and depth,
/// and the bind group reading them to blow them up.
struct RetroTarget {
    size: (u32, u32),
    color: wgpu::TextureView,
    depth: wgpu::TextureView,
    bg: wgpu::BindGroup,
}

/// Globals of the camera's view at the retro low resolution.
const GLOBALS_LOW: usize = 3;

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
    sdf: wgpu::RenderPipeline,
    /// Sprites: alpha, additive, cutout.
    sprite: [wgpu::RenderPipeline; 3],
    arcs: wgpu::RenderPipeline,
}

/// Copies of shapes placed by a compute shader (`copies.wgsl`) straight
/// into a vertex buffer; absent where there are no compute shaders (WebGL2).
struct SwarmGpu {
    pipe: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
    params: wgpu::Buffer,
    params_cap: u64,
    out: wgpu::Buffer,
    out_cap: u64,
    /// Copies placed on the CPU (surfaces, terrains), as matrices.
    locals: wgpu::Buffer,
    locals_cap: u64,
    bg: wgpu::BindGroup,
}

/// A layout as the compute shader reads it (see `copies.wgsl`).
struct GpuLayout {
    layout: u32,
    count: u32,
    seed: u32,
    /// radius, spread, speed (swarms and orbits)
    shape: [f32; 3],
    lay_u: [u32; 4],
    lay_f: [f32; 4],
    /// How far copies can be from the layer's origin (none = unknown).
    reach: Option<f32>,
}

/// Parameters for placing a layout's copies on the GPU. Copies on a surface
/// or a terrain are placed on the CPU and appended to `locals`.
fn gpu_layout(
    inst: &Instancer,
    ctx: &EvalCtx,
    surface: Option<&[ez_core::eval::SurfacePoint]>,
    locals: &mut Vec<[[f32; 4]; 4]>,
) -> GpuLayout {
    let count = ez_core::eval::layout_count(inst, surface);
    let mut g = GpuLayout {
        layout: 10,
        count,
        seed: 0,
        shape: [0.0; 3],
        lay_u: [0; 4],
        lay_f: [0.0; 4],
        reach: Some(0.0),
    };
    match *inst {
        Instancer::Single => {}
        Instancer::Grid { counts, spacing } => {
            let c = counts.map(|v| v.clamp(1, 64));
            g.layout = 4;
            g.lay_u = [c[0], c[1], c[2], 0];
            g.lay_f = [spacing[0], spacing[1], spacing[2], 0.0];
            let half = Vec3::new(
                (c[0] - 1) as f32 * spacing[0],
                (c[1] - 1) as f32 * spacing[1],
                (c[2] - 1) as f32 * spacing[2],
            ) * 0.5;
            g.reach = Some(half.length());
        }
        Instancer::Radial { radius, .. } => {
            g.layout = 5;
            g.lay_f[0] = radius;
            g.reach = Some(radius.abs());
        }
        Instancer::Wall {
            cols,
            rows,
            spacing,
            curve,
        } => {
            let (cols, rows) = (cols.clamp(1, 128), rows.clamp(1, 128));
            let width = (cols.max(2) - 1) as f32 * spacing;
            g.layout = 6;
            g.lay_u = [cols, rows, 0, 0];
            g.lay_f = [spacing, curve.to_radians(), width, 0.0];
            g.reach = Some(width * 0.5 + rows as f32 * spacing.abs() + width * 0.5);
        }
        Instancer::Spiral {
            radius,
            height,
            turns,
            ..
        } => {
            g.layout = 7;
            g.lay_f = [radius, height, turns, 0.0];
            g.reach = Some(radius.abs() + height.abs() * 0.5);
        }
        Instancer::Scatter {
            radius,
            shell,
            seed,
            ..
        } => {
            g.layout = 8;
            g.seed = seed;
            g.lay_u[0] = shell as u32;
            g.lay_f[0] = radius;
            g.reach = Some(radius.abs());
        }
        Instancer::Orbit {
            radius,
            spread,
            speed,
            seed,
            ..
        } => {
            g.layout = SwarmForm::Orbit.index();
            g.seed = seed;
            g.shape = [radius, spread, speed as f32];
            g.reach = Some(radius.abs() + spread.abs() * 1.5);
        }
        Instancer::Swarm {
            form,
            radius,
            spread,
            speed,
            seed,
            ..
        } => {
            g.layout = form.index();
            g.seed = seed;
            g.shape = [radius, spread, speed as f32];
            g.reach = Some(radius.abs() + spread.abs() * 1.5);
        }
        Instancer::Curve {
            curve,
            freq,
            size,
            laps,
            align,
            ..
        } => {
            g.layout = 9;
            let [a, b, c] = freq.map(|f| f.clamp(1, 16));
            let kind = match curve {
                RibbonCurve::Lissajous => 0,
                RibbonCurve::Knot => 1,
                RibbonCurve::Infinity => 2,
                RibbonCurve::Wave => 3,
                RibbonCurve::Rose => 4,
            };
            g.lay_u = [kind, a, b, c];
            g.lay_f = [size, laps as f32, if align { 1.0 } else { 0.0 }, 0.0];
            g.reach = Some(size.abs() * 1.6);
        }
        Instancer::Surface { size, lift, .. } => {
            g.layout = 11;
            g.lay_u[0] = locals.len() as u32;
            locals.extend(
                ez_core::eval::instancer_locals(inst, ctx, surface)
                    .iter()
                    .map(|m| m4(*m)),
            );
            g.reach = Some(size.abs() * 1.2 + lift.abs());
        }
        Instancer::OnTerrain { .. } => {
            g.layout = 11;
            g.lay_u[0] = locals.len() as u32;
            let placed = ez_core::eval::instancer_locals(inst, ctx, surface);
            g.count = placed.len() as u32;
            locals.extend(placed.iter().map(|m| m4(*m)));
            // Somewhere on the landscape: always drawn.
            g.reach = None;
        }
        Instancer::Fluid { ref fluid, .. } => {
            // Placed on the CPU from the bake.
            g.layout = 11;
            g.lay_u[0] = locals.len() as u32;
            locals.extend(
                ez_core::eval::instancer_locals(inst, ctx, surface)
                    .iter()
                    .map(|m| m4(*m)),
            );
            g.reach = Some(fluid.reach() * 1.2);
        }
        Instancer::Physics { ref physics, .. } => {
            // Placed on the CPU from the bake.
            g.layout = 11;
            g.lay_u[0] = locals.len() as u32;
            locals.extend(
                ez_core::eval::instancer_locals(inst, ctx, surface)
                    .iter()
                    .map(|m| m4(*m)),
            );
            g.reach = Some(physics.reach(Vec3::ONE) * 4.0);
        }
        Instancer::Flock { ref flock, .. } => {
            // Placed on the CPU from the bake.
            g.layout = 11;
            g.lay_u[0] = locals.len() as u32;
            locals.extend(
                ez_core::eval::instancer_locals(inst, ctx, surface)
                    .iter()
                    .map(|m| m4(*m)),
            );
            g.reach = Some(flock.reach());
        }
    }
    g
}

/// One dispatch: a layer's copies seen through one symmetry copy.
struct SwarmJob {
    params: SwarmParams,
    count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SwarmParams {
    frame: [[f32; 4]; 4],
    size: [f32; 4],
    shape: [f32; 4],
    ints: [u32; 4],
    lay_u: [u32; 4],
    lay_f: [f32; 4],
    var_a: [f32; 4],
    var_b: [f32; 4],
    var_c: [u32; 4],
    var_d: [f32; 4],
    bands: [[f32; 4]; 4],
}

/// Uniform slot per swarm dispatch (dynamic offsets are 256-aligned).
const SWARM_SLOT: u64 = 512;

impl SwarmGpu {
    fn new(device: &wgpu::Device) -> Option<SwarmGpu> {
        if device.limits().max_compute_workgroups_per_dimension == 0 {
            return None;
        }
        let module = shader(device, "copies", include_str!("shaders/copies.wgsl"), false);
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("swarm"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<SwarmParams>() as u64
                        ),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("swarm"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("swarm"),
            layout: Some(&layout),
            module: &module,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let (params_cap, out_cap, locals_cap) = (4, 1024, 64);
        let params = Self::make_params(device, params_cap);
        let out = Self::make_out(device, out_cap);
        let locals = Self::make_locals(device, locals_cap);
        let bg = Self::make_bg(device, &bgl, &params, &out, &locals);
        Some(SwarmGpu {
            pipe,
            bgl,
            params,
            params_cap,
            out,
            out_cap,
            locals,
            locals_cap,
            bg,
        })
    }

    fn make_params(device: &wgpu::Device, cap: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("swarm params"),
            size: cap * SWARM_SLOT,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn make_out(device: &wgpu::Device, cap: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("swarm instances"),
            size: cap * std::mem::size_of::<InstanceRaw>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        })
    }

    fn make_locals(device: &wgpu::Device, cap: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("copies placed on the CPU"),
            size: cap * 64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn make_bg(
        device: &wgpu::Device,
        bgl: &wgpu::BindGroupLayout,
        params: &wgpu::Buffer,
        out: &wgpu::Buffer,
        locals: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("swarm"),
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: params,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<SwarmParams>() as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: out.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: locals.as_entire_binding(),
                },
            ],
        })
    }

    /// Room for `jobs` dispatches writing `instances` copies in total, and
    /// `locals` copies placed on the CPU.
    fn reserve(&mut self, device: &wgpu::Device, jobs: u64, instances: u64, locals: u64) {
        let mut changed = false;
        if locals > self.locals_cap {
            self.locals_cap = locals.next_power_of_two();
            self.locals = Self::make_locals(device, self.locals_cap);
            changed = true;
        }
        if jobs > self.params_cap {
            self.params_cap = jobs.next_power_of_two();
            self.params = Self::make_params(device, self.params_cap);
            changed = true;
        }
        if instances > self.out_cap {
            self.out_cap = instances.next_power_of_two();
            self.out = Self::make_out(device, self.out_cap);
            changed = true;
        }
        if changed {
            self.bg = Self::make_bg(device, &self.bgl, &self.params, &self.out, &self.locals);
        }
    }
}

/// Where a target's feedback history is and what the last step was.
#[derive(Clone, Copy)]
struct FeedbackState {
    /// History texture the last step read from, and the one it wrote.
    read: usize,
    write: usize,
    phase: f32,
    dt: f32,
    fresh: bool,
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

    globals_buf: [wgpu::Buffer; 4],
    globals_bg: [wgpu::BindGroup; 4],
    shadow_mesh_pipe: wgpu::RenderPipeline,
    shadow_sdf_pipe: wgpu::RenderPipeline,
    swarm: Option<SwarmGpu>,
    contact_pipe: wgpu::RenderPipeline,
    /// Background pipelines specialised per kind, built when first used:
    /// (kind, samples, low resolution).
    bg_pipes: HashMap<(i32, u32, bool), wgpu::RenderPipeline>,
    bg_module: wgpu::ShaderModule,
    scene_layout: wgpu::PipelineLayout,
    /// Upscale of the low-resolution background.
    bg_up_pipe: wgpu::RenderPipeline,
    shadow_terrain_pipe: wgpu::RenderPipeline,
    /// Depth of field: distance-to-camera passes for meshes and terrain.
    dof_mesh_pipe: wgpu::RenderPipeline,
    dof_sdf_pipe: wgpu::RenderPipeline,
    dof_terrain_pipe: wgpu::RenderPipeline,
    dof_floor_pipe: wgpu::RenderPipeline,
    shadow_bg: wgpu::BindGroup,
    /// The environment map in use (the black one while the colours light
    /// the scene).
    env: EnvMaps,
    /// What the lit surfaces' group 3 is made of (rebuilt when the
    /// environment map changes).
    group3: Group3,
    sky: Option<SkyCapture>,
    bgl_globals: wgpu::BindGroupLayout,
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
    /// Retro 3D: the main pass's own pipelines without antialiasing, the
    /// upscale, and the low-resolution scene per target (by target id).
    low_pipes: LowPipes,
    bgl_retro_up: wgpu::BindGroupLayout,
    retro_up_pipe: wgpu::RenderPipeline,
    retro_targets: HashMap<u64, RetroTarget>,
    blur_pipe: wgpu::RenderPipeline,
    warp_pipe: wgpu::RenderPipeline,
    bloom_down_pipe: wgpu::RenderPipeline,
    bloom_up_pipe: wgpu::RenderPipeline,
    final_pipe: wgpu::RenderPipeline,
    rays_pipe: wgpu::RenderPipeline,
    rays_add_pipe: wgpu::RenderPipeline,
    /// Screen-space reflections (`ssr.wgsl`) and its inputs' layout.
    ssr_pipe: wgpu::RenderPipeline,
    ssr_add_pipe: wgpu::RenderPipeline,
    /// Light shafts through the fog (`shafts.wgsl`).
    shafts_pipe: wgpu::RenderPipeline,
    liquid_pipes: LiquidPipes,
    bgl_ssr: wgpu::BindGroupLayout,

    sampler_repeat: wgpu::Sampler,
    sampler_nearest: wgpu::Sampler,
    /// Mirrored tiling, for pictures marked so (see `MIRROR_KEY`).
    sampler_mirror: wgpu::Sampler,
    sampler_mirror_nearest: wgpu::Sampler,
    /// Bilinear without mipmaps (retro filter), plain and mirrored.
    sampler_bilinear: wgpu::Sampler,
    sampler_mirror_bilinear: wgpu::Sampler,
    sampler_clamp: wgpu::Sampler,
    /// Nearest, clamped, no mipmaps (reading depth textures).
    sampler_point_clamp: wgpu::Sampler,

    meshes: HashMap<String, GpuMesh>,
    /// Mesh keys of models that came without texture coordinates (they got
    /// box-projected ones).
    box_uv_meshes: std::collections::HashSet<String>,
    /// Distance fields of shapes used in morphs, by mesh key.
    distance_grids: HashMap<String, std::sync::Arc<crate::sdf_bake::DistanceGrid>>,
    /// Displaced models drawn with triplanar textures (joined box-mapped
    /// models: their coordinates would streak).
    triplanar_meshes: std::collections::HashSet<String>,
    textures: HashMap<String, GpuTexture>,
    tex_bgs: HashMap<(String, bool), wgpu::BindGroup>,
    mesh_tex_bgs: HashMap<MeshTexKey, wgpu::BindGroup>,
    bgl_draw_mesh: wgpu::BindGroupLayout,
    draw_mesh_bg: wgpu::BindGroup,
    /// What the logos' environment sphere was last made for (map, camera
    /// axes, turn, strength).
    env_matcap: Option<(String, [f32; 11])>,
    /// Asset loading problems (shown in the UI), keyed by asset.
    pub errors: HashMap<String, String>,

    /// Instances of layers that don't animate, keyed by layer hash, with
    /// the frame number they were last used.
    instance_cache: HashMap<u64, (u64, Vec<InstanceRaw>)>,
    /// Scene transitions: the two pictures, and the mixing pass.
    seq_targets: Option<SeqTargets>,
    feedback_pipe: wgpu::RenderPipeline,
    /// Feedback history per target: (history to read next, last phase).
    feedback: HashMap<u64, FeedbackState>,
    compose_pipe: wgpu::RenderPipeline,
    compose_buf: wgpu::Buffer,
    /// Font atlases by texture key.
    fonts: HashMap<String, std::sync::Arc<crate::text::FontAtlas>>,
    logo_pipe: wgpu::RenderPipeline,
    bgl_logo: wgpu::BindGroupLayout,
    /// The picture behind the logos (for glass and shadow rays), at least
    /// as big as the target being drawn: texture, view, size.
    logo_backdrop: (wgpu::Texture, wgpu::TextureView, u32, u32),
    /// Logos drawn as a rays source cutting their shadow out.
    logo_shadow_pipe: wgpu::RenderPipeline,
    /// Logo rays' source pictures by target, made when first needed.
    logo_rays_src: HashMap<u64, LogoRaysSource>,
    /// Logo effect settings (two slots of the draw buffer).
    bgl_logo_fx: wgpu::BindGroupLayout,
    logo_fx_bg: wgpu::BindGroup,
    /// Logo texture bind groups by (logo, morph target, material sphere).
    logo_bgs: HashMap<LogoKey, wgpu::BindGroup>,
    /// Baked logos by texture key, with the frame they were last drawn
    /// (`None`: nothing to draw).
    logos: HashMap<String, (u64, Option<LogoFit>)>,
    /// Points on shape surfaces for Instancer::Surface: (mesh, count, seed).
    surface_cache: HashMap<(String, u32, u32), std::sync::Arc<Vec<ez_core::eval::SurfacePoint>>>,
    frame_no: u64,
    /// Instance data currently in `inst_buf` (skip identical uploads).
    uploaded: Vec<InstanceRaw>,
    floor_bg_cache: Option<((String, u64), wgpu::BindGroup)>,
    stats: FrameStats,
    /// Simulations baked into loops (flocks), and what they may depend on.
    bakes: ez_core::sim::BakeCache,
    sim_frame: ez_core::sim::Frame,
    audio: Option<std::sync::Arc<AudioEnvelope>>,
    /// Changes with every new music envelope (part of bake keys).
    audio_id: u64,
    /// Wait for bakes instead of drawing the last one (exports).
    wait_for_bakes: bool,
    /// Whether a simulation was drawn from an old bake, or not drawn, in
    /// frames since the last [`Renderer::take_inexact`].
    inexact: bool,
    /// Per simulated layer (by name), how its bake is doing.
    sim_status: HashMap<String, SimStatus>,
}

/// How a simulated layer's bake is doing (for the inspector).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SimStatus {
    /// The bake for the current settings is ready (else an older one, or
    /// nothing, is drawn).
    pub ready: bool,
    /// The gap closing the loop hides (see [`ez_core::sim::Seam`]).
    pub seam: ez_core::sim::Seam,
    /// Loops simulated before recording.
    pub warmup_loops: u32,
    /// Memory the bake takes.
    pub bytes: usize,
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
    /// The distance pass's G-buffer (half resolution): normal and
    /// roughness, reflectance, the reflected environment shown.
    gbuf: [wgpu::TextureView; 3],
    /// Screen-space reflections to add (half resolution), their inputs,
    /// and adding them onto the scene.
    ssr: wgpu::TextureView,
    bg_ssr: wgpu::BindGroup,
    bg_ssr_add: wgpu::BindGroup,
    /// Light shafts to add (half resolution), and adding them.
    shafts: wgpu::TextureView,
    bg_shafts_add: wgpu::BindGroup,
    /// Liquid surfaces (half resolution): splatted distances and
    /// thickness, the blur between and the smoothed surface; the inputs
    /// of each pass.
    liquid: [wgpu::TextureView; 4],
    bg_liquid: [wgpu::BindGroup; 4],
    rays: wgpu::TextureView,
    /// Depth of field: distance to the camera (half resolution) and its
    /// depth buffer.
    dof_dist: wgpu::TextureView,
    dof_z: wgpu::TextureView,
    /// Feedback trails: the post image as a texture (to copy into), the
    /// two history images and their bind groups (reading history `i`).
    hdr2_tex: wgpu::Texture,
    fb: [(wgpu::Texture, wgpu::TextureView); 2],
    bg_fb: [wgpu::BindGroup; 2],
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

/// Liquid surfaces (`liquid.wgsl`): splatting droplets, blurring, and
/// shading the surface onto the picture.
struct LiquidPipes {
    splat_dist: wgpu::RenderPipeline,
    splat_thick: wgpu::RenderPipeline,
    blur_h: wgpu::RenderPipeline,
    blur_v: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

/// Dynamic offsets of a mesh draw: its slot and the next.
fn mesh_offsets(slot: u32) -> [u32; 2] {
    [slot * DRAW_SLOT as u32, (slot + 1) * DRAW_SLOT as u32]
}

/// The physical-material slot of a mesh draw (see mesh.wgsl's D2).
fn pbr_block(mat: &Material, ctx: &EvalCtx) -> Block {
    let p = &mat.pbr;
    let mut b: Block = Zeroable::zeroed();
    let on = |x: bool| if x { 1.0 } else { 0.0 };
    b[0] = [
        on(p.shading == Shading::Physical),
        on(p.orm_map.is_some()),
        on(p.emissive_map.is_some()),
        0.0,
    ];
    b[1] = [
        p.clearcoat.eval(ctx).clamp(0.0, 1.0),
        p.clearcoat_roughness.eval(ctx).clamp(0.0, 1.0),
        p.sheen.eval(ctx).max(0.0),
        p.transmission.eval(ctx).clamp(0.0, 1.0),
    ];
    b[2] = c4(p.sheen_color, p.ior.max(1.0));
    b
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
        // Meshes and raymarched shapes read two draw slots: theirs and the
        // next (the physical material).
        let bgl_draw_mesh = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw mesh"),
            entries: &[
                uniform_entry(0, true, DRAW_SLOT),
                uniform_entry(1, true, DRAW_SLOT),
            ],
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
                wgpu::BindGroupLayoutEntry {
                    visibility: vf,
                    ..sampler_entry(3)
                },
                tex_entry(4),
                tex_entry(5),
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
                // The environment map, its sampler and the BRDF table.
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let globals_buf = [0, 1, 2, 3].map(|i| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(["globals main", "globals refl", "globals sun", "globals low"][i]),
                size: globals_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        let globals_bg = [0, 1, 2, 3].map(|i| {
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
        let draw_mesh_bg = Self::make_draw_mesh_bg(device, &bgl_draw_mesh, &draw_buf);
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
            bind_group_layouts: &[
                Some(&bgl_globals),
                Some(&bgl_draw_mesh),
                Some(&bgl_mesh_tex),
            ],
            immediate_size: 0,
        });
        // Lit surfaces also read the sun shadow map (group 3).
        let mesh_lit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh lit"),
            bind_group_layouts: &[
                Some(&bgl_globals),
                Some(&bgl_draw_mesh),
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
        let sh_sdf = shader(device, "sdf", include_str!("shaders/sdf.wgsl"), true);
        let sh_sprite = shader(device, "sprite", include_str!("shaders/sprite.wgsl"), true);
        let sh_arcs = shader(device, "arcs", include_str!("shaders/arcs.wgsl"), true);
        let sh_logo = shader(device, "logo", include_str!("shaders/logo.wgsl"), true);
        // Logos go on the picture after depth of field (no depth, no MSAA:
        // the distance field antialiases).
        // The logo, the logo it morphs into and the material sphere, then
        // a second block of settings.
        let bgl_logo = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("logo tex"),
            entries: &[
                tex_entry(0),
                tex_entry(1),
                tex_entry(2),
                sampler_entry(3),
                tex_entry(4),
                tex_entry(5),
                tex_entry(6),
            ],
        });
        let bgl_logo_fx = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("logo effects"),
            entries: &[uniform_entry(0, true, LOGO_FX_SIZE)],
        });
        let logo_fx_bg = Self::make_draw_bg_sized(device, &bgl_logo_fx, &draw_buf, LOGO_FX_SIZE);
        let logo_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("logo"),
            bind_group_layouts: &[
                Some(&bgl_globals),
                Some(&bgl_draw),
                Some(&bgl_logo),
                Some(&bgl_logo_fx),
            ],
            immediate_size: 0,
        });
        // A logo cutting its shadow out of the light behind it:
        // picture × (1 - coverage).
        let logo_shadow_pipe = make_pipeline(
            device,
            PipeDesc {
                label: "logo shadow",
                layout: &logo_layout,
                module: &sh_logo,
                fs: "fs_main",
                buffers: &[],
                format: HDR_FORMAT,
                samples: 1,
                depth: None,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::Zero,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent::OVER,
                }),
            },
        );
        let logo_pipe = make_pipeline(
            device,
            PipeDesc {
                label: "logo",
                layout: &logo_layout,
                module: &sh_logo,
                fs: "fs_main",
                buffers: &[],
                format: HDR_FORMAT,
                samples: 1,
                depth: None,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            },
        );

        let scene_pipes = |samples: u32| ScenePipes {
            samples,
            arcs: make_pipeline(
                device,
                PipeDesc {
                    label: "arcs",
                    layout: &particle_layout,
                    module: &sh_arcs,
                    fs: "fs_main",
                    buffers: &glyph_buffers,
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((false, wgpu::CompareFunction::Less)),
                    blend: Some(ADDITIVE),
                },
            ),
            sprite: [
                (Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), false),
                (Some(ADDITIVE), false),
                (None, true),
            ]
            .map(|(blend, write)| {
                make_pipeline(
                    device,
                    PipeDesc {
                        label: "sprite",
                        layout: &scene_layout,
                        module: &sh_sprite,
                        fs: "fs_main",
                        buffers: &glyph_buffers,
                        format: HDR_FORMAT,
                        samples,
                        depth: Some((write, wgpu::CompareFunction::Less)),
                        blend,
                    },
                )
            }),
            sdf: make_pipeline(
                device,
                PipeDesc {
                    label: "sdf",
                    layout: &mesh_lit_layout,
                    module: &sh_sdf,
                    fs: "fs_main",
                    buffers: &mesh_buffers,
                    format: HDR_FORMAT,
                    samples,
                    depth: Some((true, wgpu::CompareFunction::Less)),
                    blend: None,
                },
            ),
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
        // Raymarched objects write their own depth from the fragment stage.
        let shadow_sdf_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow sdf"),
            layout: Some(&mesh_layout),
            vertex: wgpu::VertexState {
                module: &sh_sdf,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &mesh_buffers,
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
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &sh_sdf,
                entry_point: Some("fs_shadow"),
                compilation_options: Default::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });
        let dof_pipe = |label: &str,
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
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some("fs_depth"),
                    compilation_options: Default::default(),
                    // Distance, then the G-buffer for screen-space
                    // reflections (see `DistOut` in common.wgsl).
                    targets: &[
                        Some(wgpu::ColorTargetState {
                            format: DOF_FORMAT,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        }),
                        Some(HDR_FORMAT.into()),
                        Some(HDR_FORMAT.into()),
                        Some(HDR_FORMAT.into()),
                    ],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // What surfaces reflect needs the split-sum table (group 3).
        let dof_mesh_pipe = dof_pipe("dof mesh", &mesh_lit_layout, &sh_mesh, &mesh_buffers);
        let dof_sdf_pipe = dof_pipe("dof sdf", &mesh_lit_layout, &sh_sdf, &mesh_buffers);
        let dof_terrain_pipe = dof_pipe("dof terrain", &scene_layout, &sh_terrain, &[]);
        let dof_floor_pipe = dof_pipe("dof floor", &particle_layout, &sh_floor, &[]);
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
        let env_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("environment"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        // No environment map yet: a black one (unused while the colours
        // light the scene).
        let env_none = Self::make_env_cube(device, queue, 1, &[vec![vec![[0.0f32; 4]]; 6]]);
        let lut_view = {
            let lut = crate::envmap::brdf_lut();
            let n = crate::envmap::LUT_SIZE;
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("brdf table"),
                size: wgpu::Extent3d {
                    width: n,
                    height: n,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba16Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let bytes: Vec<u8> = lut
                .iter()
                .flat_map(|[a, b]| [*a, *b, 0.0, 1.0])
                .flat_map(|c| crate::logo::f16_bits(c).to_le_bytes())
                .collect();
            queue.write_texture(
                texture.as_image_copy(),
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(8 * n),
                    rows_per_image: Some(n),
                },
                wgpu::Extent3d {
                    width: n,
                    height: n,
                    depth_or_array_layers: 1,
                },
            );
            texture.create_view(&Default::default())
        };
        let shadow_bg = Self::make_group3(
            device,
            &bgl_shadow,
            &shadow_view,
            &shadow_sampler,
            &env_none,
            &env_sampler,
            &lut_view,
        );
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
                // Drawn right after the backgrounds: an endless floor fades
                // into them at the horizon.
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            },
        );
        // Retro 3D: the low-resolution scene blown up with its depth.
        let bgl_retro_up = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("retro upscale"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let retro_up_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("retro upscale"),
            bind_group_layouts: &[Some(&bgl_retro_up)],
            immediate_size: 0,
        });
        let sh_retro_up = shader(
            device,
            "retro upscale",
            include_str!("shaders/retro_up.wgsl"),
            false,
        );
        let retro_up_pipe = make_pipeline(
            device,
            PipeDesc {
                label: "retro upscale",
                layout: &retro_up_layout,
                module: &sh_retro_up,
                fs: "fs_main",
                buffers: &[],
                format: HDR_FORMAT,
                samples: msaa,
                depth: Some((true, wgpu::CompareFunction::Always)),
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
        let ssr_add_pipe = post_pipe("ssr add", "fs_ssr_add", HDR_FORMAT, Some(ADDITIVE));
        // Screen-space reflections: the G-buffer, the distances and the
        // scene in (group 2), the environment for what they replace
        // (group 3).
        let bgl_ssr = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ssr"),
            entries: &[
                tex_entry(0),
                tex_entry(1),
                tex_entry(2),
                tex_entry(3),
                tex_entry(4),
                sampler_entry(5),
            ],
        });
        let ssr_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ssr"),
            bind_group_layouts: &[
                Some(&bgl_globals),
                Some(&bgl_draw),
                Some(&bgl_ssr),
                Some(&bgl_shadow),
            ],
            immediate_size: 0,
        });
        let sh_ssr = shader(device, "ssr", include_str!("shaders/ssr.wgsl"), true);
        // Light shafts read the distances (group 2 as the reflections'),
        // the fog in the globals and the shadow map (group 3).
        let sh_shafts = shader(device, "shafts", include_str!("shaders/shafts.wgsl"), true);
        let shafts_pipe = make_pipeline(
            device,
            PipeDesc {
                label: "light shafts",
                layout: &ssr_layout,
                module: &sh_shafts,
                fs: "fs_main",
                buffers: &[],
                format: HDR_FORMAT,
                samples: 1,
                depth: None,
                blend: None,
            },
        );
        // Liquid surfaces: droplet splats, blurs and the composite, all
        // with the droplet layer's material (group 1) and the environment
        // (group 3).
        let bgl_liquid = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("liquid"),
            entries: &[tex_entry(0), tex_entry(1)],
        });
        let liquid_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("liquid"),
            bind_group_layouts: &[
                Some(&bgl_globals),
                Some(&bgl_draw_mesh),
                Some(&bgl_liquid),
                Some(&bgl_shadow),
            ],
            immediate_size: 0,
        });
        let sh_liquid = shader(device, "liquid", include_str!("shaders/liquid.wgsl"), true);
        let splat_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4, 8 => Float32x4],
        })];
        let liquid_pipe = |label: &str,
                           vs: &str,
                           fs: &str,
                           buffers: &[Option<wgpu::VertexBufferLayout>],
                           blend: Option<wgpu::BlendState>,
                           scene_samples: u32| {
            // Drawn in the scene's pass (with its samples) or on its own.
            let scene = scene_samples > 0;
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&liquid_layout),
                vertex: wgpu::VertexState {
                    module: &sh_liquid,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                // The composite is drawn in the scene's pass, against its
                // depth.
                depth_stencil: scene.then(|| wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: scene_samples.max(1),
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &sh_liquid,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: HDR_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // The nearest droplet wins.
        let nearest = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Min,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Min,
            },
        };
        let low_liquid = liquid_pipe(
            "liquid composite (retro)",
            "vs_full",
            "fs_composite",
            &[],
            Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            1,
        );
        let liquid_pipes = LiquidPipes {
            splat_dist: liquid_pipe(
                "liquid splat distance",
                "vs_splat",
                "fs_splat_dist",
                &splat_buffers,
                Some(nearest),
                0,
            ),
            splat_thick: liquid_pipe(
                "liquid splat thickness",
                "vs_splat",
                "fs_splat_thick",
                &splat_buffers,
                Some(ADDITIVE),
                0,
            ),
            blur_h: liquid_pipe("liquid blur h", "vs_full", "fs_blur_h", &[], None, 0),
            blur_v: liquid_pipe("liquid blur v", "vs_full", "fs_blur_v", &[], None, 0),
            composite: liquid_pipe(
                "liquid composite",
                "vs_full",
                "fs_composite",
                &[],
                Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                msaa,
            ),
            layout: bgl_liquid,
        };
        let low_pipes = LowPipes {
            floor: make_pipeline(
                device,
                PipeDesc {
                    label: "floor (retro)",
                    layout: &floor_layout,
                    module: &sh_floor,
                    fs: "fs_main",
                    buffers: &[],
                    format: HDR_FORMAT,
                    samples: 1,
                    depth: Some((true, wgpu::CompareFunction::Less)),
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                },
            ),
            contact: make_pipeline(
                device,
                PipeDesc {
                    label: "contact shadows (retro)",
                    layout: &particle_layout,
                    module: &sh_contact,
                    fs: "fs_main",
                    buffers: &contact_instances,
                    format: HDR_FORMAT,
                    samples: 1,
                    depth: Some((false, wgpu::CompareFunction::Less)),
                    blend: Some(MULTIPLY),
                },
            ),
            bg_up: make_pipeline(
                device,
                PipeDesc {
                    label: "bg upscale (retro)",
                    layout: &scene_layout,
                    module: &sh_bgup,
                    fs: "fs_main",
                    buffers: &[],
                    format: HDR_FORMAT,
                    samples: 1,
                    depth: Some((false, wgpu::CompareFunction::Always)),
                    blend: None,
                },
            ),
            liquid: low_liquid,
        };
        let ssr_pipe = make_pipeline(
            device,
            PipeDesc {
                label: "ssr",
                layout: &ssr_layout,
                module: &sh_ssr,
                fs: "fs_main",
                buffers: &[],
                format: HDR_FORMAT,
                samples: 1,
                depth: None,
                blend: None,
            },
        );
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
        let feedback_pipe = post_pipe("feedback", "fs_feedback", HDR_FORMAT, None);
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
        let sampler_mirror = sampler(wgpu::AddressMode::MirrorRepeat, wgpu::FilterMode::Linear);
        let sampler_mirror_nearest = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mirror nearest"),
            address_mode_u: wgpu::AddressMode::MirrorRepeat,
            address_mode_v: wgpu::AddressMode::MirrorRepeat,
            address_mode_w: wgpu::AddressMode::MirrorRepeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let bilinear = |addr| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("bilinear"),
                address_mode_u: addr,
                address_mode_v: addr,
                address_mode_w: addr,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                lod_max_clamp: 0.0,
                ..Default::default()
            })
        };
        let sampler_point_clamp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("point clamp"),
            ..Default::default()
        });
        let sampler_bilinear = bilinear(wgpu::AddressMode::Repeat);
        let sampler_mirror_bilinear = bilinear(wgpu::AddressMode::MirrorRepeat);

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
            shadow_sdf_pipe,
            swarm: SwarmGpu::new(device),
            contact_pipe,
            bg_pipes: HashMap::new(),
            bg_module: sh_backdrop,
            scene_layout,
            bg_up_pipe,
            shadow_terrain_pipe,
            dof_mesh_pipe,
            dof_sdf_pipe,
            dof_terrain_pipe,
            dof_floor_pipe,
            shadow_bg,
            env: EnvMaps {
                key: String::new(),
                view: env_none,
                sh: [[0.0; 4]; 9],
                sun: None,
                pano: None,
                tiny: None,
            },
            sky: None,
            bgl_globals: bgl_globals.clone(),
            group3: Group3 {
                layout: bgl_shadow,
                shadow_view,
                shadow_sampler,
                env_sampler,
                lut_view,
            },
            bgl_draw,
            draw_buf,
            draw_cap,
            draw_bg,
            post_buf,
            inst_buf,
            inst_cap,
            main_pipes,
            refl_pipes,
            low_pipes,
            bgl_retro_up,
            retro_up_pipe,
            retro_targets: HashMap::new(),
            floor_pipe,
            blur_pipe,
            warp_pipe,
            bloom_down_pipe,
            bloom_up_pipe,
            final_pipe,
            rays_pipe,
            rays_add_pipe,
            ssr_pipe,
            ssr_add_pipe,
            shafts_pipe,
            liquid_pipes,
            bgl_ssr,
            sampler_repeat,
            sampler_nearest,
            sampler_mirror,
            sampler_mirror_nearest,
            sampler_bilinear,
            sampler_mirror_bilinear,
            sampler_point_clamp,
            sampler_clamp,
            meshes: HashMap::new(),
            box_uv_meshes: Default::default(),
            distance_grids: HashMap::new(),
            triplanar_meshes: Default::default(),
            textures: HashMap::new(),
            tex_bgs: HashMap::new(),
            mesh_tex_bgs: HashMap::new(),
            bgl_draw_mesh,
            draw_mesh_bg,
            env_matcap: None,
            errors: HashMap::new(),
            instance_cache: HashMap::new(),
            seq_targets: None,
            feedback_pipe,
            feedback: HashMap::new(),
            compose_pipe,
            compose_buf,
            fonts: HashMap::new(),
            logo_pipe,
            logo_fx_bg,
            bgl_logo_fx,
            logo_backdrop: Self::make_logo_backdrop(device, 1, 1),
            logo_shadow_pipe,
            logo_rays_src: HashMap::new(),
            bgl_logo,
            logo_bgs: HashMap::new(),
            logos: HashMap::new(),
            surface_cache: HashMap::new(),
            frame_no: 0,
            uploaded: Vec::new(),
            floor_bg_cache: None,
            stats: FrameStats::default(),
            bakes: ez_core::sim::BakeCache::new(),
            sim_frame: Default::default(),
            audio: None,
            audio_id: 0,
            wait_for_bakes: false,
            inexact: false,
            sim_status: HashMap::new(),
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
        Self::make_draw_bg_sized(device, layout, buf, DRAW_SLOT)
    }

    /// The draw buffer seen twice, one slot at a time: a draw's own slot
    /// and the next.
    fn make_draw_mesh_bg(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buf: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let entry = |binding| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: buf,
                offset: 0,
                size: wgpu::BufferSize::new(DRAW_SLOT),
            }),
        };
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("draw mesh"),
            layout,
            entries: &[entry(0), entry(1)],
        })
    }

    /// The draw buffer seen `size` bytes at a time (several slots).
    fn make_draw_bg_sized(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buf: &wgpu::Buffer,
        size: u64,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("draw"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(size),
                }),
            }],
        })
    }

    /// Whether big swarms are placed by a compute shader here.
    pub fn gpu_swarms(&self) -> bool {
        self.swarm.is_some()
    }

    /// Place big swarms on the CPU instead (as on WebGL2), e.g. to compare.
    pub fn disable_gpu_swarms(&mut self) {
        self.swarm = None;
    }

    /// Buffer holding a mesh command's instances.
    fn mesh_instances(&self, gpu: bool) -> &wgpu::Buffer {
        match (&self.swarm, gpu) {
            (Some(sw), true) => &sw.out,
            _ => &self.inst_buf,
        }
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
        self.logo_bgs
            .retain(|(a, b, c), _| *a != key && *b != key && *c != key);
        self.mesh_tex_bgs
            .retain(|(k, r, o, e, _), _| ![k, r, o, e].contains(&&key));
        self.textures.insert(
            key,
            GpuTexture {
                _texture: texture,
                view,
            },
        );
    }

    /// Upload float RGBA pixels (as half floats) with mipmaps.
    fn upload_float_texture(&mut self, key: String, w: u32, h: u32, px: &[[f32; 4]]) {
        let levels = crate::logo::mips(w, h, px);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&key),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (mip, (lw, lh, level)) in levels.iter().enumerate() {
            let bytes: Vec<u8> = level
                .iter()
                .flatten()
                .flat_map(|c| crate::logo::f16_bits(*c).to_le_bytes())
                .collect();
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(8 * lw),
                    rows_per_image: Some(*lh),
                },
                wgpu::Extent3d {
                    width: *lw,
                    height: *lh,
                    depth_or_array_layers: 1,
                },
            );
        }
        let view = texture.create_view(&Default::default());
        self.tex_bgs.retain(|(k, _), _| *k != key);
        self.logo_bgs
            .retain(|(a, b, c), _| *a != key && *b != key && *c != key);
        self.textures.insert(
            key,
            GpuTexture {
                _texture: texture,
                view,
            },
        );
    }

    /// A logo layer's baked texture, uploaded, and how it fits; `None` when
    /// there is nothing to draw (no text, no image, an empty mask).
    fn logo_texture(&mut self, project: &Project, g: &LogoLayer) -> Option<(String, LogoFit)> {
        let key = match g.source {
            LogoSource::Text => format!("__logo:text:{:?}:{:?}:{}", g.font, g.font_file, g.text),
            LogoSource::Image => {
                format!("__logo:image:{}:{:?}", self.image_id(project, g)?, g.mask)
            }
        };
        let frame = self.frame_no;
        if let Some((used, fit)) = self.logos.get_mut(&key) {
            *used = frame;
            return fit.map(|f| (key, f));
        }
        // Problems are shown per font file and per image, not per bake
        // (which changes with every letter typed).
        let bake = match g.source {
            LogoSource::Text => {
                let bytes = g.font_file.as_ref().and_then(|path| {
                    let err_key = format!("__font:file:{path}");
                    let name = ez_core::store::file_name(path);
                    match ez_core::store::read(path) {
                        Ok(b) if skrifa::FontRef::new(&b).is_ok() => {
                            self.errors.remove(&err_key);
                            Some(b)
                        }
                        Ok(_) => {
                            self.errors
                                .insert(err_key, format!("Font {name}: not a font"));
                            None
                        }
                        Err(e) => {
                            self.errors.insert(err_key, format!("Font {name}: {e}"));
                            None
                        }
                    }
                });
                crate::logo::bake_text(&g.text, g.font, bytes.as_deref())
            }
            LogoSource::Image => {
                let name = g.image.as_deref().unwrap_or_default();
                let err_key = format!("__logo:image:{}", self.image_id(project, g)?);
                match self.load_image(project, name) {
                    Ok(img) => {
                        self.errors.remove(&err_key);
                        crate::logo::bake_image(&img, g.mask)
                    }
                    Err(e) => {
                        self.errors
                            .insert(err_key, format!("Logo image '{name}': {e}"));
                        None
                    }
                }
            }
        };
        let fit = bake.map(|b| {
            self.upload_float_texture(key.clone(), b.width, b.height, &b.pixels);
            let f = &b.far;
            let far: Vec<[f32; 4]> = f.field.iter().map(|v| [*v; 4]).collect();
            self.upload_float_texture(far_key(&key), f.width, f.height, &far);
            LogoFit {
                far: [
                    f.origin[0],
                    f.origin[1],
                    f.cell,
                    f.width as f32,
                    f.height as f32,
                ],
                width: b.width,
                height: b.height,
                aspect: b.aspect,
                pad: b.pad,
                spread: b.spread,
                max_field: b.max_field,
            }
        });
        self.logos.insert(key.clone(), (frame, fit));
        // Editing text leaves old bakes behind: forget those not drawn for
        // a while.
        let stale: Vec<String> = self
            .logos
            .iter()
            .filter(|(_, (used, _))| frame - *used > 240)
            .map(|(k, _)| k.clone())
            .collect();
        for k in stale {
            self.logos.remove(&k);
            self.textures.remove(&k);
            self.textures.remove(&far_key(&k));
            self.tex_bgs.retain(|(t, _), _| *t != k);
            self.logo_bgs
                .retain(|(a, b, c), _| *a != k && *b != k && *c != k);
        }
        fit.map(|f| (key, f))
    }

    /// What a logo image is, for caching: a built-in name, or the file and
    /// its retro look. `None` without an image.
    fn image_id(&self, project: &Project, g: &LogoLayer) -> Option<String> {
        let name = g.image.as_deref().filter(|n| !n.is_empty())?;
        Some(if texgen::is_builtin(name) {
            format!("b:{name}")
        } else {
            match project.find_texture(name) {
                Some(t) => format!("u:{}:{:?}", t.path, t.retro),
                None => format!("u:{name}"),
            }
        })
    }

    /// The textures of a logo in one bind group (clamped: beyond the edge
    /// the shader extends the field itself).
    fn logo_bind_group(&mut self, key: &LogoKey) {
        if self.logo_bgs.contains_key(key) {
            return;
        }
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("logo tex"),
            layout: &self.bgl_logo,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.textures[&key.0].view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.textures[&key.1].view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.textures[&key.2].view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_clamp),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        &self.textures[&far_key(&key.0)].view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(
                        &self.textures[&far_key(&key.1)].view,
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&self.logo_backdrop.1),
                },
            ],
        });
        self.logo_bgs.insert(key.clone(), bg);
    }

    /// The four blocks of a logo layer drawn at `ctx` with its anchor at
    /// `at` (pixels from the bottom left of a `w` × `h` picture) and its
    /// opacity times `fade`, the key of its textures' bind group and its
    /// fit. `None` when there is nothing to draw.
    #[allow(clippy::too_many_arguments)]
    fn logo_blocks(
        &mut self,
        project: &Project,
        g: &LogoLayer,
        ctx: &EvalCtx,
        at: Vec2,
        w: u32,
        h: u32,
        flash: f32,
        fade: f32,
    ) -> Option<LogoDraw> {
        let (tex, fit) = self.logo_texture(project, g)?;
        let mut blk: Block = Zeroable::zeroed();
        blk[0] = c4(g.color_top, g.glow.eval(ctx).max(0.0) * flash);
        blk[1] = c4(g.color_bottom, g.outline.eval(ctx));
        blk[2] = c4(g.outline_color, g.shadow.eval(ctx));
        blk[3] = c4(g.tint, g.chrome.eval(ctx));
        blk[4] = [
            at.x / w as f32,
            at.y / h as f32,
            g.size.eval(ctx).max(0.0),
            g.rotation.eval(ctx).to_radians(),
        ];
        let [ax, ay] = g.anchor.point();
        blk[5] = [
            ax,
            ay,
            fit.aspect,
            g.opacity.eval(ctx).clamp(0.0, 1.0) * fade,
        ];
        blk[6] = [
            fit.pad[0],
            fit.pad[1],
            fit.spread,
            if g.colors == LogoColors::Gradient {
                1.0
            } else {
                0.0
            },
        ];
        blk[7] = [fit.width as f32, fit.height as f32, 0.0, 0.0];
        // Lighting: bevel, light, material sphere, glint.
        blk[8] = [
            g.bevel.index() as f32,
            g.bevel_width.eval(ctx).max(0.0) * 0.5,
            g.bevel_depth.eval(ctx).max(0.0),
            g.steps.max(1) as f32,
        ];
        let (la, lh) = (
            g.light_angle.eval(ctx).to_radians(),
            g.light_height.eval(ctx).clamp(0.0, 90.0).to_radians(),
        );
        blk[9] = [
            la.cos() * lh.cos(),
            la.sin() * lh.cos(),
            lh.sin(),
            g.lighting.eval(ctx).max(0.0),
        ];
        blk[10] = c4(g.light_color, g.shine.eval(ctx).max(0.0));
        let has_matcap = g.matcap.as_deref().is_some_and(|m| !m.is_empty());
        blk[11] = [
            4.0 + g.gloss.clamp(0.0, 1.0).powi(2) * 124.0,
            if has_matcap {
                g.matcap_amount.eval(ctx).clamp(0.0, 1.0)
            } else {
                0.0
            },
            fit.max_field,
            g.glint.eval(ctx).max(0.0),
        ];
        blk[12] = [
            (ctx.phase * g.glint_cycles as f32).rem_euclid(1.0),
            g.glint_width.max(0.005),
            g.glint_angle.to_radians(),
            0.0,
        ];
        blk[13] = c4(g.glint_color, 0.0);
        blk[14] = [fit.far[0], fit.far[1], fit.far[2], 0.0];
        blk[15] = [fit.far[3], fit.far[4], 0.0, 0.0];
        let matcap = match g.matcap.as_deref() {
            Some(crate::texgen::ENV_MATCAP) => self
                .env_matcap(project, ctx)
                .unwrap_or_else(|| self.texture_key(project, g.matcap.as_deref())),
            name => self.texture_key(project, name),
        };
        // The logo it morphs into: the same layer made of the
        // morph source.
        let morph_t = g.morph.eval(ctx).clamp(0.0, 1.0);
        let target = if g.morph.is_animated() || g.morph.base > 0.0 {
            let other = LogoLayer {
                source: g.morph_source,
                text: g.morph_text.clone(),
                image: g.morph_image.clone(),
                mask: g.morph_mask.unwrap_or(g.mask),
                ..g.clone()
            };
            self.logo_texture(project, &other)
        } else {
            None
        };
        let (morph, other) = match target {
            Some((k, f)) => (k, f),
            None => (tex.clone(), fit),
        };
        let morph_t = if morph == tex { 0.0 } else { morph_t };
        let mut e: Block = Zeroable::zeroed();
        e[0] = [
            g.contours.eval(ctx).max(0.0),
            g.contour_spacing.max(0.005),
            (ctx.phase * g.contour_cycles as f32).rem_euclid(1.0),
            g.contour_reach.max(0.01),
        ];
        e[1] = c4(g.contour_color, g.contour_width.clamp(0.01, 1.0));
        let stack_w = g.stack_width.eval(ctx).max(0.0);
        e[2] = [
            g.stack.min(16) as f32,
            stack_w,
            g.stack_gap.max(0.0),
            if g.contour_inside { 1.0 } else { 0.0 },
        ];
        e[3] = c4(g.stack_color_a, 0.0);
        e[4] = c4(g.stack_color_b, 0.0);
        let extrude = g.extrude.eval(ctx).clamp(0.0, 2.0);
        e[5] = [extrude, g.extrude_angle.to_radians(), 0.0, 0.0];
        e[6] = c4(g.extrude_color, 0.0);
        e[7] = [
            g.dissolve.eval(ctx).clamp(0.0, 1.0),
            g.dissolve_scale.max(0.1),
            g.dissolve_edges.clamp(0.0, 1.0),
            g.burn_width.max(0.0),
        ];
        e[8] = c4(g.burn_color, (g.dissolve_seed % 997) as f32 * 3.7);
        e[9] = [
            g.reveal_amount.eval(ctx).clamp(0.0, 1.0),
            g.reveal.index() as f32,
            g.reveal_angle.to_radians(),
            g.reveal_soft.max(0.0),
        ];
        e[10] = [morph_t, other.aspect, other.pad[0], other.pad[1]];
        e[11] = [
            other.spread,
            other.width as f32,
            other.height as f32,
            other.max_field,
        ];
        // Room for what reaches past the padding (logo heights).
        let mut margin: f32 = 0.0;
        if g.contours.is_animated() || g.contours.base > 0.0 {
            margin = margin.max(g.contour_reach.max(0.01) * 3.0);
        }
        if g.stack > 0 {
            let w = g.stack_width.base.abs() + g.stack_width.amp.abs();
            margin = margin.max(g.stack.min(16) as f32 * (w + g.stack_gap.max(0.0)));
        }
        margin = margin.max(g.extrude.base.abs() + g.extrude.amp.abs());
        // Distortion moves the picture around.
        let reach = |p: &Param| p.base.abs() + p.amp.abs();
        margin += reach(&g.wobble_x).max(reach(&g.wobble_y))
            + reach(&g.glitch)
            + reach(&g.chroma)
            + g.glitch_split.abs();
        let wider = if morph_t > 0.0 {
            (other.aspect - fit.aspect).max(0.0)
        } else {
            0.0
        };
        e[12] = [
            fit.heights_per_field(),
            other.heights_per_field(),
            margin.min(2.0),
            wider,
        ];
        e[13] = [
            other.content_height() / fit.content_height().max(1.0),
            0.0,
            0.0,
            0.0,
        ];
        e[14] = [other.far[0], other.far[1], other.far[2], 0.0];
        e[15] = [other.far[3], other.far[4], 0.0, 0.0];
        // Rasters and distortion.
        let mut e2: Block = Zeroable::zeroed();
        e2[0] = [
            g.copper.eval(ctx).clamp(0.0, 1.0),
            g.copper_bars.max(0.1),
            (ctx.phase * g.copper_cycles as f32 * 2.0).rem_euclid(2.0),
            0.0,
        ];
        e2[1] = c4(g.copper_a, 0.0);
        e2[2] = c4(g.copper_b, 0.0);
        e2[3] = [
            g.wobble_x.eval(ctx),
            g.wobble_y.eval(ctx),
            g.wobble_waves,
            (ctx.phase * g.wobble_cycles as f32).rem_euclid(1.0),
        ];
        let per_loop = g.glitch_per_loop.max(1);
        e2[4] = [
            g.glitch.eval(ctx).max(0.0),
            g.glitch_slices.max(0.1),
            g.glitch_chance.clamp(0.0, 1.0),
            (((ctx.phase.rem_euclid(1.0) * per_loop as f32) as u32) % per_loop) as f32,
        ];
        e2[5] = [
            g.chroma.eval(ctx).max(0.0),
            g.chroma_angle.to_radians(),
            g.glitch_split,
            0.0,
        ];
        // Retro looks; the palette in a third block.
        let mut e3: Block = Zeroable::zeroed();
        let mut colors: Vec<[f32; 3]> = match g.palette {
            Some(pal) => pal.colors_f32(),
            None => Vec::new(),
        };
        colors.truncate(16);
        let luma = |c: &[f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        if g.palette_by_brightness {
            colors.sort_by(|a, b| luma(a).total_cmp(&luma(b)));
        }
        for (i, c) in colors.iter().enumerate() {
            e3[i] = c4(*c, 0.0);
        }
        let count = match g.palette {
            Some(PaletteId::Vga) => 216.0,
            Some(_) => colors.len() as f32,
            None => 0.0,
        };
        let shift = if colors.is_empty() {
            0.0
        } else {
            let n = colors.len() as f32;
            ((ctx.phase * g.palette_cycles as f32).rem_euclid(1.0) * n).floor() % n
        };
        // Retro 3D without sharp logos: at least as chunky as the scene
        // (a low-resolution pixel, in logo heights).
        let mut block = g.pixelate.eval(ctx).max(0.0);
        if let (Some((_, lh)), false) = (
            project.retro.internal_size((w, h)),
            project.retro.sharp_overlays,
        ) {
            let size = g.size.eval(ctx).max(1e-3);
            block = block.max(1.0 / (lh as f32 * size));
        }
        e2[6] = [block, count, g.dither.clamp(0.0, 1.0), shift];
        e2[7] = [
            if g.palette_by_brightness { 1.0 } else { 0.0 },
            g.halftone.eval(ctx).clamp(0.0, 1.0),
            g.halftone_size.max(0.002),
            g.halftone_angle.to_radians(),
        ];
        e2[8] = [
            g.scanlines.eval(ctx).clamp(0.0, 1.0),
            g.scanline_count.max(1.0),
            g.crt_mask.clamp(0.0, 1.0),
            g.crt_glow.eval(ctx).max(0.0),
        ];
        e2[9] = [
            g.moire.eval(ctx).clamp(0.0, 1.0),
            g.moire_lines.max(1.0),
            (ctx.phase * g.moire_cycles as f32).rem_euclid(1.0) * TAU,
            0.0,
        ];
        // Glass.
        e2[10] = [
            g.glass.eval(ctx).clamp(0.0, 1.0),
            g.refraction,
            g.dispersion.clamp(0.0, 1.0),
            0.0,
        ];
        e2[11] = c4(g.glass_tint, 0.0);
        let key = (tex, morph, matcap);
        self.logo_bind_group(&key);
        Some(([blk, e, e2, e3], key, fit))
    }

    /// Where every logo layer's anchor sits on a `w` × `h` picture (pixels
    /// from the bottom left), following attachments to other logos. `None`
    /// for other layers and logos with nothing to draw.
    fn place_logos(
        &mut self,
        project: &Project,
        layers: &[Layer],
        ctx: &EvalCtx,
        w: f32,
        h: f32,
    ) -> Vec<Option<Vec2>> {
        let fits: Vec<Option<LogoFit>> = layers
            .iter()
            .map(|l| match &l.kind {
                LayerKind::Logo(g) if l.enabled => self.logo_texture(project, g).map(|(_, f)| f),
                _ => None,
            })
            .collect();
        let mut out = vec![None; layers.len()];
        let mut state = vec![0u8; layers.len()];
        for i in 0..layers.len() {
            place_logo(i, layers, &fits, ctx, w, h, &mut out, &mut state);
        }
        out
    }

    /// Where the logo layers' anchors are on a picture of `size`, as
    /// fractions from the bottom left (for the viewport's handles).
    pub fn logo_anchors(
        &mut self,
        project: &Project,
        ctx: &EvalCtx,
        size: [f32; 2],
    ) -> Vec<Option<[f32; 2]>> {
        let layers = project.scene_layers(ctx);
        self.place_logos(project, &layers, ctx, size[0], size[1])
            .into_iter()
            .map(|p| p.map(|p| [p.x / size[0].max(1.0), p.y / size[1].max(1.0)]))
            .collect()
    }

    fn make_logo_backdrop(
        device: &wgpu::Device,
        w: u32,
        h: u32,
    ) -> (wgpu::Texture, wgpu::TextureView, u32, u32) {
        let t = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("logo backdrop"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let v = t.create_view(&Default::default());
        (t, v, w, h)
    }

    /// Grow the logo backdrop to hold a `w` × `h` picture.
    fn ensure_logo_backdrop(&mut self, w: u32, h: u32) {
        let (_, _, bw, bh) = self.logo_backdrop;
        if bw >= w && bh >= h {
            return;
        }
        self.logo_backdrop = Self::make_logo_backdrop(&self.device, w.max(bw), h.max(bh));
        // The logos' bind groups hold the old one.
        self.logo_bgs.clear();
    }

    /// A built-in or project image, decoded (with its retro look).
    fn load_image(&self, project: &Project, name: &str) -> Result<RgbaImage, String> {
        if texgen::is_builtin(name) {
            return Ok(texgen::generate(name));
        }
        let (path, retro) = match project.find_texture(name) {
            Some(t) => (t.path.clone(), t.retro.clone()),
            None => (name.to_string(), None),
        };
        let bytes = ez_core::store::read(&path).map_err(|e| e.to_string())?;
        let img = image::load_from_memory(&bytes)
            .map_err(|e| e.to_string())?
            .to_rgba8();
        Ok(match &retro {
            Some(r) => texgen::retroize(&img, r),
            None => img,
        })
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
        self.texture_key_as(project, name, false)
    }

    /// Like [`Self::texture_key`]; `linear` pictures hold data (such as
    /// roughness), not colours, and are read without sRGB decoding.
    fn texture_key_as(&mut self, project: &Project, name: Option<&str>, linear: bool) -> String {
        let Some(name) = name.filter(|n| !n.is_empty()) else {
            return "__white".into();
        };
        let format = if linear {
            wgpu::TextureFormat::Rgba8Unorm
        } else {
            wgpu::TextureFormat::Rgba8UnormSrgb
        };
        let lin = if linear { ":lin" } else { "" };
        if texgen::is_builtin(name) {
            let key = format!("b:{name}{lin}");
            if !self.textures.contains_key(&key) {
                let img = texgen::generate(name);
                self.upload_texture_as(key.clone(), &img, format);
            }
            return key;
        }
        let (path, retro, mirror) = match project.find_texture(name) {
            Some(t) => (t.path.clone(), t.retro.clone(), t.mirror),
            None => (name.to_string(), None, false),
        };
        // Mirror tiling is a different sampler, so a different key.
        let key = format!(
            "u:{path}:{retro:?}{lin}{}",
            if mirror { MIRROR_KEY } else { "" }
        );
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
                self.upload_texture_as(key.clone(), &img, format);
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

    /// The sampler for texture `key`: mirrored tiling when its key says so.
    fn sampler_for(&self, key: &str, nearest: bool) -> &wgpu::Sampler {
        self.sampler_of(key, nearest as u8)
    }

    /// The sampler of a texture by kind (see [`sampler_kind`]).
    fn sampler_of(&self, key: &str, kind: u8) -> &wgpu::Sampler {
        match (key.ends_with(MIRROR_KEY), kind) {
            (false, 1) => &self.sampler_nearest,
            (false, 2) => &self.sampler_bilinear,
            (false, _) => &self.sampler_repeat,
            (true, 1) => &self.sampler_mirror_nearest,
            (true, 2) => &self.sampler_mirror_bilinear,
            (true, _) => &self.sampler_mirror,
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
                    resource: wgpu::BindingResource::Sampler(self.sampler_for(key, nearest)),
                },
            ],
        });
        self.tex_bgs.insert(k, bg);
    }

    fn mesh_tex_bind_group(&mut self, k: &MeshTexKey) {
        let (tex, relief, orm, emit, kind) = (&k.0[..], &k.1[..], &k.2[..], &k.3[..], k.4);
        if self.mesh_tex_bgs.contains_key(k) {
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
                    resource: wgpu::BindingResource::Sampler(self.sampler_of(tex, kind)),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.textures[relief].view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(self.sampler_of(relief, kind)),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&self.textures[orm].view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&self.textures[emit].view),
                },
            ],
        });
        self.mesh_tex_bgs.insert(k.clone(), bg);
    }

    /// Geometry of a mesh source, subdivided `levels` times (cached).
    /// `displaced` models (files and library models, usually faceted and
    /// low-poly) get their corners joined so displacement doesn't tear
    /// them apart, and enough detail for it even at Subdivide 0.
    fn mesh_key_subdivided(&mut self, source: &MeshSource, levels: u32, displaced: bool) -> String {
        let base = self.mesh_key(source);
        // A cloth's sheet changes every frame: it is as fine as it is.
        if matches!(source, MeshSource::Cloth { .. }) {
            return base;
        }
        let model = matches!(source, MeshSource::File { .. } | MeshSource::Library { .. });
        if displaced && model {
            return self.displaced_model_key(source, &base, levels);
        }
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

    fn displaced_model_key(&mut self, source: &MeshSource, base: &str, levels: u32) -> String {
        let tris = self
            .meshes
            .get(base)
            .map(|g| g.count as u64 / 3)
            .unwrap_or(1)
            .max(1);
        // At least ~30k triangles to carry the relief, at most ~2M.
        let mut levels = levels.min(4);
        while levels < 4 && tris * 4u64.pow(levels) < 30_000 {
            levels += 1;
        }
        while levels > 0 && tris * 4u64.pow(levels) > 2_000_000 {
            levels -= 1;
        }
        let key = format!("{base}#d{levels}");
        if self.meshes.contains_key(&key) {
            return key;
        }
        let mut data = self.source_data(source, base);
        let box_uv = self.box_uv_meshes.contains(base);
        data.weld_smooth(!box_uv);
        data.subdivide(levels);
        if box_uv {
            self.triplanar_meshes.insert(key.clone());
        }
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
            // Keyed by the library's generation: a model wanted before the
            // web app fetched the library loads again once it arrives.
            MeshSource::Library { id } => format!("l:{id}:{}", ez_core::models::generation()),
            MeshSource::Text { .. } => {
                format!("t:{}", serde_json::to_string(source).unwrap_or_default())
            }
            // Every raymarched shape is drawn in the same box.
            MeshSource::Sdf { .. } => return self.sdf_box_key(),
            // Cloth: this frame's sheet from its bake, or lying at rest
            // until it is baked.
            MeshSource::Cloth { cloth, mesh } => match mesh {
                Some(k) if self.meshes.contains_key(k) => return k.clone(),
                _ => format!(
                    "cloth-rest:{}",
                    serde_json::to_string(&**cloth).unwrap_or_default()
                ),
            },
        };
        if self.meshes.contains_key(&key) {
            return key;
        }
        let data = self.source_data(source, &key);
        self.upload_mesh(key.clone(), &data);
        key
    }

    /// The box every raymarched shape is drawn in.
    fn sdf_box_key(&mut self) -> String {
        let key = "sdf-box".to_string();
        if !self.meshes.contains_key(&key) {
            let mut data = primitive(&Primitive::Cube);
            for v in &mut data.vertices {
                v.pos = (Vec3::from(v.pos) * 2.04).into();
            }
            self.upload_mesh(key.clone(), &data);
        }
        key
    }

    /// Distance field of a shape (baked once, then cached) and its key.
    fn distance_grid(
        &mut self,
        source: &MeshSource,
    ) -> (String, std::sync::Arc<crate::sdf_bake::DistanceGrid>) {
        let key = match source {
            // Raymarched shapes morph as the ball they stand on.
            MeshSource::Sdf { .. } => "sdf-ball".to_string(),
            _ => self.mesh_key(source),
        };
        if let Some(g) = self.distance_grids.get(&key) {
            return (key, g.clone());
        }
        let data = self.source_data(source, &key);
        let grid = std::sync::Arc::new(crate::sdf_bake::bake(&data, MORPH_GRID));
        if self.distance_grids.len() > 32 {
            self.distance_grids.clear();
        }
        self.distance_grids.insert(key.clone(), grid.clone());
        (key, grid)
    }

    /// The texture holding the distance fields of a morph's two shapes
    /// (red: from, green: into), uploaded once per pair.
    fn morph_texture(&mut self, from: &MeshSource, into: &MeshSource) -> String {
        let (ka, a) = self.distance_grid(from);
        let (kb, b) = self.distance_grid(into);
        let key = format!("morph:{ka}|{kb}");
        if !self.textures.contains_key(&key) {
            let (w, h, _, px) = crate::sdf_bake::pack_pair(&a, &b);
            self.upload_float_texture(key.clone(), w, h, &px);
        }
        key
    }

    /// Geometry of a shape source; problems are reported under `key` and
    /// give a cube.
    fn source_data(&mut self, source: &MeshSource, key: &str) -> MeshData {
        match source {
            MeshSource::Primitive(p) => primitive(p),
            MeshSource::Cloth { cloth, .. } => {
                use ez_core::sim::Sim;
                let pos: Vec<Vec3> = cloth.sim().bodies().iter().map(|b| b.pos).collect();
                let (cols, rows) = cloth.grid();
                crate::mesh::cloth_sheet(&pos, cols as usize, rows as usize)
            }
            // Copies on a raymarched shape stand on a ball of about its size.
            MeshSource::Sdf { .. } => {
                let mut data = primitive(&Primitive::Sphere { detail: 3 });
                for v in &mut data.vertices {
                    v.pos = (Vec3::from(v.pos) * 0.8).into();
                }
                data
            }
            MeshSource::File { path } => match load_mesh_asset(path) {
                Ok(mut m) => {
                    self.errors.remove(key);
                    if m.lacks_uvs() {
                        m.box_uvs();
                        self.box_uv_meshes.insert(key.to_string());
                    }
                    m
                }
                Err(e) => {
                    self.errors
                        .insert(key.to_string(), format!("model {path}: {e:#}"));
                    primitive(&Primitive::Cube)
                }
            },
            MeshSource::Library { id } => {
                // Problems are reported per model, whatever the generation.
                let err_key = format!("l:{id}");
                let result = match ez_core::models::library() {
                    Some(lib) => lib
                        .glb(id)
                        .map_err(anyhow::Error::msg)
                        .and_then(|glb| crate::import::load_mesh_bytes("glb", &glb)),
                    None => Err(anyhow::anyhow!("the model library is loading")),
                };
                match result {
                    Ok(mut m) => {
                        self.errors.remove(&err_key);
                        if m.lacks_uvs() {
                            m.box_uvs();
                            self.box_uv_meshes.insert(key.to_string());
                        }
                        m
                    }
                    Err(e) => {
                        self.errors.insert(err_key, format!("model {id}: {e:#}"));
                        primitive(&Primitive::Cube)
                    }
                }
            }
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
        self.logos.clear();
        self.errors.clear();
        self.floor_bg_cache = None;
    }

    /// Statistics of the last rendered frame.
    pub fn stats(&self) -> &FrameStats {
        &self.stats
    }

    // ------------------------------------------------------------------
    // Simulations

    // ------------------------------------------------------------------
    // Environment light

    /// A cube texture from per-mip, per-face texels (`size` the largest
    /// face's side).
    fn make_env_cube(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: u32,
        mips: &[Vec<Vec<[f32; 4]>>],
    ) -> wgpu::TextureView {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("environment map"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
            mip_level_count: mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (mip, faces) in mips.iter().enumerate() {
            let n = (size >> mip).max(1);
            for (face, texels) in faces.iter().enumerate() {
                let bytes: Vec<u8> = texels
                    .iter()
                    .flatten()
                    .flat_map(|c| crate::logo::f16_bits(*c).to_le_bytes())
                    .collect();
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: mip as u32,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: face as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &bytes,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(8 * n),
                        rows_per_image: Some(n),
                    },
                    wgpu::Extent3d {
                        width: n,
                        height: n,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        })
    }

    fn make_group3(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        shadow_view: &wgpu::TextureView,
        shadow_sampler: &wgpu::Sampler,
        env_view: &wgpu::TextureView,
        env_sampler: &wgpu::Sampler,
        lut_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow and environment"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(shadow_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(env_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(env_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(lut_view),
                },
            ],
        })
    }

    /// What an environment map is made from (its cache key).
    fn env_key(light: &EnvLight) -> Option<String> {
        let sun = if light.sun_from_map { ":sun" } else { "" };
        match &light.source {
            EnvSource::Colours | EnvSource::Sky => None,
            EnvSource::Studio(k) => Some(format!("studio:{k:?}{sun}")),
            EnvSource::Hdri(path) => Some(format!("hdri:{path}{sun}")),
        }
    }

    /// Makes the environment map for `key` (from `light`) the current one.
    fn load_env(&mut self, key: String, light: &EnvLight) {
        let eq = match &light.source {
            EnvSource::Studio(k) => Ok(crate::envmap::studio(*k)),
            EnvSource::Hdri(path) => ez_core::store::read(path)
                .map_err(|e| e.to_string())
                .and_then(|b| crate::envmap::load_hdr(&b)),
            _ => return,
        };
        let mut eq = match eq {
            Ok(eq) => {
                self.errors.remove(&key);
                eq
            }
            Err(e) => {
                self.errors
                    .insert(key.clone(), format!("environment map: {e}"));
                crate::envmap::Equirect::new(4, 2, |_| [0.0; 3])
            }
        };
        let sun = if light.sun_from_map {
            crate::envmap::extract_sun(&mut eq)
        } else {
            None
        };
        let cube = crate::envmap::prefilter(&eq);
        let sh = crate::envmap::irradiance_sh(&eq).map(|c| [c[0], c[1], c[2], 0.0]);
        let view = Self::make_env_cube(&self.device, &self.queue, crate::envmap::CUBE_SIZE, &cube);
        // The panorama for the background (a smaller copy: it is blurred
        // or far away).
        let small = eq.at_most(2048);
        let px: Vec<[f32; 4]> = small.px.iter().map(|c| [c[0], c[1], c[2], 1.0]).collect();
        let pano = format!("env-pano:{key}");
        self.upload_float_texture(pano.clone(), small.w as u32, small.h as u32, &px);
        self.set_env(EnvMaps {
            key,
            view,
            sh,
            sun,
            pano: Some(pano),
            tiny: Some(small.clone().at_most(64)),
        });
    }

    /// The logos' environment sphere: a mirror ball reflecting the
    /// environment light's map as the camera sees it now. `None` without a
    /// map (Colours, or the sky, which has no copy on the CPU).
    fn env_matcap(&mut self, project: &Project, ctx: &EvalCtx) -> Option<String> {
        let light = &project.environment.env_light;
        let key = Self::env_key(light)?;
        if self.env.key != key {
            self.load_env(key, &light.clone());
        }
        let eq = self.env.tiny.as_ref()?;
        let inv = project.camera.eval(ctx).view().inverse();
        let (right, up, back) = (
            inv.x_axis.truncate(),
            inv.y_axis.truncate(),
            inv.z_axis.truncate(),
        );
        let turn = light.rotation.eval(ctx).to_radians();
        let strength = light.intensity.eval(ctx).max(0.0);
        let state = [
            right.x, right.y, right.z, up.x, up.y, up.z, back.x, back.y, back.z, turn, strength,
        ];
        let tex = "env-matcap".to_string();
        let made = (self.env.key.clone(), state);
        if self.env_matcap.as_ref() != Some(&made) || !self.textures.contains_key(&tex) {
            let px = crate::envmap::matcap(eq, right, up, back, turn, strength, 64);
            self.upload_float_texture(tex.clone(), 64, 64, &px);
            self.env_matcap = Some(made);
        }
        Some(tex)
    }

    /// Makes `env` the environment map lit surfaces see.
    fn set_env(&mut self, env: EnvMaps) {
        if let Some(old) = self.env.pano.take() {
            if env.pano.as_ref() != Some(&old) {
                self.textures.remove(&old);
                self.tex_bgs.retain(|(k, _), _| *k != old);
            }
        }
        self.env = env;
        self.shadow_bg = Self::make_group3(
            &self.device,
            &self.group3.layout,
            &self.group3.shadow_view,
            &self.group3.shadow_sampler,
            &self.env.view,
            &self.group3.env_sampler,
            &self.group3.lut_view,
        );
    }

    /// Makes the sky capture's textures and pipeline (once).
    fn ensure_sky(&mut self) {
        if self.sky.is_some() {
            return;
        }
        let device = &self.device;
        let cube = |label: &str, mips: u32| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: SKY_SIZE,
                    height: SKY_SIZE,
                    depth_or_array_layers: 6,
                },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let face_view = |t: &wgpu::Texture, face: u32, mip: u32| {
            t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: face,
                array_layer_count: Some(1),
                base_mip_level: mip,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let cube_view = |t: &wgpu::Texture| {
            t.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::Cube),
                ..Default::default()
            })
        };
        let mips = crate::envmap::CUBE_MIPS;
        let src = cube("sky capture", 1);
        let dst = cube("sky environment map", mips);
        let src_faces = (0..6).map(|f| face_view(&src, f, 0)).collect();
        let dst_faces = (0..mips)
            .flat_map(|m| (0..6).map(move |f| (m, f)))
            .map(|(m, f)| face_view(&dst, f, m))
            .collect();
        let (src_cube, dst_cube) = (cube_view(&src), cube_view(&dst));
        let globals: Vec<wgpu::Buffer> = (0..6)
            .map(|_| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("globals sky face"),
                    size: std::mem::size_of::<GlobalsRaw>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        let globals_bg = globals
            .iter()
            .map(|b| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("globals sky face"),
                    layout: &self.bgl_globals,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: b.as_entire_binding(),
                    }],
                })
            })
            .collect();
        // The blur: its parameters per mip and face, fixed.
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky filter"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        const SLOT: u64 = 256;
        let mut params = vec![0u8; (SLOT * 6 * mips as u64) as usize];
        for m in 0..mips {
            for f in 0..6u32 {
                let at = ((m * 6 + f) as u64 * SLOT) as usize;
                let v = [
                    f as f32,
                    (SKY_SIZE >> m).max(1) as f32,
                    m as f32 / (mips - 1) as f32,
                    0.0,
                ];
                params[at..at + 16].copy_from_slice(bytemuck::cast_slice(&v));
            }
        }
        let param_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sky filter params"),
            contents: &params,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let filter_bgs = (0..6 * mips as u64)
            .map(|i| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sky filter"),
                    layout: &bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &param_buf,
                                offset: i * SLOT,
                                size: wgpu::BufferSize::new(16),
                            }),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&src_cube),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.group3.env_sampler),
                        },
                    ],
                })
            })
            .collect();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky filter"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/env_filter.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky filter"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let filter_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky filter"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(HDR_FORMAT.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        self.sky = Some(SkyCapture {
            src_faces,
            dst_faces,
            dst_cube,
            globals,
            globals_bg,
            filter_pipe,
            filter_bgs,
            captured: None,
        });
    }

    /// "From the sky": draws the background all around (`main` is the
    /// frame's globals, `bg` its background: draw slot, texture, kind) and
    /// blurs it into the environment map.
    fn capture_sky(
        &mut self,
        enc: &mut wgpu::CommandEncoder,
        main: &GlobalsRaw,
        bg: (u32, &str, i32),
        once: bool,
    ) {
        let (slot, tex, kind) = bg;
        // A sky that doesn't move is captured once per look.
        let key = ez_core::sim::KeyHasher::new()
            .u64(slot as u64)
            .bytes(tex.as_bytes())
            .u64(kind as u64)
            .bytes(bytemuck::bytes_of(&main.sky))
            .bytes(bytemuck::bytes_of(&main.fog))
            .finish();
        self.ensure_bg_pipe(kind, 1, true);
        let Some(sky) = self.sky.as_mut() else {
            return;
        };
        if once && sky.captured == Some(key) {
            return;
        }
        sky.captured = Some(key);
        // Each face's view: rays through its pixels, as the cube is read.
        let eye = Vec3::new(main.cam_pos[0], main.cam_pos[1], main.cam_pos[2]);
        let frames = [
            (Vec3::X, Vec3::NEG_Z, Vec3::Y),
            (Vec3::NEG_X, Vec3::Z, Vec3::Y),
            (Vec3::Y, Vec3::X, Vec3::NEG_Z),
            (Vec3::NEG_Y, Vec3::X, Vec3::Z),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        ];
        for (f, (fwd, right, up)) in frames.iter().enumerate() {
            // Clip (x, y, z) → eye + (right x + up y + fwd) / (a - b z):
            // near at 1/a, far at 1/(a - b).
            let (a, b) = (10.0, 9.999);
            let inv = Mat4::from_cols(
                right.extend(0.0),
                up.extend(0.0),
                (-eye * b).extend(-b),
                (*fwd + eye * a).extend(a),
            );
            let mut g = *main;
            g.inv_view_proj = m4(inv);
            g.view_proj = m4(inv.inverse());
            g.cam_right = v4(*right, 0.0);
            g.cam_up = v4(*up, 0.0);
            g.res = [
                SKY_SIZE as f32,
                SKY_SIZE as f32,
                1.0 / SKY_SIZE as f32,
                1.0 / SKY_SIZE as f32,
            ];
            g.clip = [0.0; 4];
            self.queue
                .write_buffer(&sky.globals[f], 0, bytemuck::bytes_of(&g));
        }
        let pipe = &self.bg_pipes[&(kind, 1, true)];
        let Some(tex_bg) = self.tex_bgs.get(&(tex.to_string(), false)) else {
            return;
        };
        fn clear(view: &wgpu::TextureView) -> wgpu::RenderPassColorAttachment<'_> {
            wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            }
        }
        for f in 0..6 {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky capture"),
                color_attachments: &[Some(clear(&sky.src_faces[f]))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, &sky.globals_bg[f], &[]);
            pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
            pass.set_bind_group(2, tex_bg, &[]);
            pass.draw(0..3, 0..1);
        }
        for (i, view) in sky.dst_faces.iter().enumerate() {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sky filter"),
                color_attachments: &[Some(clear(view))],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&sky.filter_pipe);
            pass.set_bind_group(0, &sky.filter_bgs[i], &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Environment light for this frame: the map (loaded when it changed),
    /// its turn and strength into `fx`, and its sun into `env`.
    fn apply_env_light(
        &mut self,
        project: &Project,
        ctx: &EvalCtx,
        env: &mut EnvState,
        fx: &mut FrameEnv,
    ) {
        let light = &project.environment.env_light;
        if light.source == EnvSource::Sky {
            self.ensure_sky();
            if self.env.key != "sky" {
                let view = self.sky.as_ref().expect("made above").dst_cube.clone();
                self.set_env(EnvMaps {
                    key: "sky".into(),
                    view,
                    sh: [[0.0; 4]; 9],
                    sun: None,
                    pano: None,
                    tiny: None,
                });
            }
            fx.ibl = [
                light.intensity.eval(ctx).max(0.0).max(1e-6),
                light.rotation.eval(ctx).to_radians(),
                (crate::envmap::CUBE_MIPS - 1) as f32,
                1.0,
            ];
            return;
        }
        let Some(key) = Self::env_key(light) else {
            fx.ibl = [0.0; 4];
            return;
        };
        if self.env.key != key {
            let light = light.clone();
            self.load_env(key, &light);
        }
        let light = &project.environment.env_light;
        let turn = light.rotation.eval(ctx).to_radians();
        let strength = light.intensity.eval(ctx).max(0.0);
        fx.ibl = [
            strength.max(1e-6),
            turn,
            (crate::envmap::CUBE_MIPS - 1) as f32,
            0.0,
        ];
        fx.sh = self.env.sh;
        if let Some(sun) = self.env.sun {
            // The map turns by `turn` around +y; so does its sun.
            let (sa, ca) = turn.sin_cos();
            let d = sun.dir;
            let dir =
                Vec3::new(d.x * ca + d.z * sa, d.y, -d.x * sa + d.z * ca).normalize_or(Vec3::Y);
            let l = sun.light * strength;
            let lum = (0.2126 * l.x + 0.7152 * l.y + 0.0722 * l.z).max(1e-6);
            env.light_dir = dir.into();
            env.sun_dir = dir.into();
            env.light_color = (l / lum).into();
            // Our sun light has no 1/π: match the map's diffuse light.
            env.light_intensity = lum / std::f32::consts::PI;
        }
    }

    /// The music simulations may follow (the loop window of the song).
    pub fn set_audio(&mut self, audio: Option<std::sync::Arc<AudioEnvelope>>) {
        let same = match (&self.audio, &audio) {
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.audio = audio;
            self.audio_id += 1;
        }
    }

    /// Wait for simulations to bake before drawing them (exports, stills),
    /// instead of drawing their last bake meanwhile.
    pub fn set_wait_for_bakes(&mut self, wait: bool) {
        self.wait_for_bakes = wait;
    }

    /// Call once per frame: collects finished bakes and runs bakes that
    /// can't have a thread (the browser) while `more()` allows.
    pub fn poll_bakes(&mut self, more: impl FnMut() -> bool) {
        self.bakes.poll(more);
        self.bakes.end_frame();
    }

    /// Progress of the running bakes (0..1), `None` when none runs.
    pub fn bake_progress(&self) -> Option<f32> {
        self.bakes.progress()
    }

    /// Whether a simulation was drawn from an old bake (or not at all)
    /// since the last call; clears the flag.
    pub fn take_inexact(&mut self) -> bool {
        std::mem::take(&mut self.inexact)
    }

    /// How the bake of the simulated layer `name` is doing.
    pub fn sim_status(&self, name: &str) -> Option<SimStatus> {
        self.sim_status.get(name).copied()
    }

    /// Fills in the copies of flock layers from their bakes, starting the
    /// bakes they need.
    fn link_sims(&mut self, project: &Project, ctx: &EvalCtx, layers: &mut Cow<'_, [Layer]>) {
        use ez_core::sim::{BakeJob, KeyHasher, SimClock};
        let wants = |l: &Layer| {
            l.enabled
                && (matches!(
                    l.kind.instancer(),
                    Some(
                        Instancer::Flock { .. }
                            | Instancer::Physics { .. }
                            | Instancer::Fluid { .. }
                    )
                ) || matches!(&l.kind, LayerKind::Mesh(m) if matches!(m.source, MeshSource::Cloth { .. })))
        };
        if !layers.iter().any(wants) {
            return;
        }
        for (i, layer) in layers.to_mut().iter_mut().enumerate() {
            if !wants(layer) {
                continue;
            }
            let name = layer.name.clone();
            let slot = KeyHasher::new()
                .bytes(name.as_bytes())
                .u64(i as u64)
                .finish();
            // Everything a bake depends on, and the bake itself.
            let mut bake_of =
                |settings: String, music: bool, make: &dyn Fn(&SimClock) -> BakeJob| {
                    let key = KeyHasher::new()
                        .bytes(settings.as_bytes())
                        .json(&project.timing)
                        .u64(if music { self.audio_id } else { 0 })
                        .json(&if music { Some(&project.music) } else { None })
                        .finish();
                    let clock = SimClock::with_music(
                        project.timing,
                        project.music.clone(),
                        if music { self.audio.clone() } else { None },
                    );
                    let job = || make(&clock);
                    let mut bake = self.bakes.get(slot, key, job);
                    if self.wait_for_bakes && !self.bakes.is_ready(key) {
                        self.bakes.finish_all();
                        bake = self.bakes.get(slot, key, job);
                    }
                    let ready = self.bakes.is_ready(key);
                    self.inexact |= !ready;
                    self.sim_status.insert(
                        name.clone(),
                        bake.as_ref().map_or(SimStatus::default(), |b| SimStatus {
                            ready,
                            seam: b.seam(),
                            warmup_loops: b.warmup_loops(),
                            bytes: b.bytes(),
                        }),
                    );
                    bake
                };
            if let LayerKind::Mesh(MeshLayer {
                source: MeshSource::Cloth { cloth, mesh },
                ..
            }) = &mut layer.kind
            {
                let make = |clock: &SimClock| {
                    BakeJob::new(Box::new(cloth.sim()), clock.clone(), cloth.looping.clone())
                };
                let json = serde_json::to_string(&**cloth).unwrap_or_default();
                let Some(bake) = bake_of(json, cloth.uses_music(), &make) else {
                    continue;
                };
                bake.sample(ctx.beat_phase, &mut self.sim_frame);
                // A cross-faded cloth is the two halves mixed by weight.
                let mut pos = vec![Vec3::ZERO; bake.len()];
                for l in &self.sim_frame.layers {
                    for (p, b) in pos.iter_mut().zip(&l.bodies) {
                        *p += b.pos * l.weight;
                    }
                }
                let (cols, rows) = cloth.grid();
                let data = crate::mesh::cloth_sheet(&pos, cols as usize, rows as usize);
                let key = format!("cloth:{slot:016x}");
                self.upload_dynamic_mesh(&key, &data);
                *mesh = Some(key);
                continue;
            }
            // Rigid bodies collide as the copies are drawn: at the
            // layer's (resting) size.
            let scale = Vec3::from(layer.transform.stretch) * layer.transform.scale.base;
            // A liquid's container turns with the layer.
            let transform = layer.transform.clone();
            let (bake, placed) = match layer.kind.instancer_mut() {
                Some(Instancer::Flock { flock, placed }) => {
                    let make = |clock: &SimClock| {
                        let sim = flock.sim(&clock.ctx(0.0));
                        BakeJob::new(Box::new(sim), clock.clone(), flock.looping.clone())
                    };
                    let json = serde_json::to_string(&**flock).unwrap_or_default();
                    (bake_of(json, flock.uses_music(), &make), placed)
                }
                Some(Instancer::Physics { physics, placed }) => {
                    let make = |clock: &SimClock| {
                        let sim = physics.sim(scale);
                        BakeJob::new(Box::new(sim), clock.clone(), physics.looping.clone())
                    };
                    let json =
                        serde_json::to_string(&(&**physics, scale.to_array())).unwrap_or_default();
                    (bake_of(json, false, &make), placed)
                }
                Some(Instancer::Fluid { fluid, placed }) => {
                    let make = |clock: &SimClock| {
                        let sim = fluid.sim(ez_core::sim::layer_tilt(&transform));
                        BakeJob::new(Box::new(sim), clock.clone(), fluid.looping.clone())
                    };
                    // The turning parts of the transform move the liquid.
                    let turning = (
                        &**fluid,
                        transform.rotation,
                        transform.spin,
                        &transform.tilt,
                    );
                    let json = serde_json::to_string(&turning).unwrap_or_default();
                    let music = fluid.uses_music() || transform.tilt.uses_music();
                    (bake_of(json, music, &make), placed)
                }
                _ => continue,
            };
            let Some(bake) = bake else {
                continue;
            };
            // The simulation ran in real time: read it at the real phase.
            bake.sample(ctx.beat_phase, &mut self.sim_frame);
            let mut mats = Vec::with_capacity(bake.len() * self.sim_frame.layers.len());
            for l in &self.sim_frame.layers {
                for b in &l.bodies {
                    // Cross-faded halves shrink away as they fade.
                    let size = b.size * l.weight;
                    if size > 1e-3 {
                        let rot = glam::Quat::from_array(b.rot).normalize();
                        mats.push(Mat4::from_scale_rotation_translation(
                            Vec3::splat(size),
                            rot,
                            b.pos,
                        ));
                    }
                }
            }
            *placed = Some(std::sync::Arc::new(mats));
        }
    }

    /// Uploads a mesh that changes every frame (same size: rewritten in
    /// place).
    fn upload_dynamic_mesh(&mut self, key: &str, data: &MeshData) {
        let bytes: &[u8] = bytemuck::cast_slice(&data.vertices);
        if let Some(m) = self.meshes.get_mut(key) {
            if m.vbuf.size() == bytes.len() as u64 && m.count == data.indices.len() as u32 {
                self.queue.write_buffer(&m.vbuf, 0, bytes);
                m.radius = data
                    .vertices
                    .iter()
                    .map(|v| Vec3::from(v.pos).length())
                    .fold(0.0, f32::max);
                return;
            }
        }
        let vbuf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dynamic mesh vertices"),
                contents: bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        let ibuf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dynamic mesh indices"),
                contents: bytemuck::cast_slice(&data.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        self.meshes.insert(
            key.to_string(),
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
            caus_col: c4(project.scene_color(ca.color, ctx), fx.wet.clamp(0.0, 1.0)),
            extra: [
                fx.snow.clamp(0.0, 1.0),
                env.night,
                env.dusk,
                e.rainbow.eval(ctx).max(0.0),
            ],
            sun: v4(Vec3::from(env.sun_dir), 0.0),
            shadow_vp: m4(Mat4::IDENTITY),
            shadow: [0.0; 4],
            ibl: fx.ibl,
            sh: fx.sh,
            // Only the camera's views snap (see `render_scene`).
            retro: [
                0.0,
                1.0,
                1.0,
                if project.retro.enabled {
                    project.retro.affine.eval(ctx).clamp(0.0, 1.0)
                } else {
                    0.0
                },
            ],
            retro_fog: {
                let f = &project.retro.fog;
                if project.retro.enabled && f.enabled {
                    let near = f.near.eval(ctx).max(0.0);
                    [1.0, near, f.far.eval(ctx).max(near + 0.01), 0.0]
                } else {
                    [0.0; 4]
                }
            },
            retro_col: {
                let r = &project.retro;
                if r.enabled && r.color_15bit {
                    [31.0, r.dither.eval(ctx).clamp(0.0, 1.0), 0.0, 0.0]
                } else {
                    [0.0; 4]
                }
            },
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
        let mut layers = project.scene_layers(ctx);
        self.link_sims(project, ctx, &mut layers);
        let (w, h) = (target.width, target.height);
        let cam = project.camera.eval(ctx);
        let view = cam.view();
        let proj = cam.proj(w as f32 / h as f32);

        let mut blocks: Vec<Block> = Vec::new();
        let mut instances: Vec<InstanceRaw> = Vec::new();
        let mut cmds: Vec<Cmd> = Vec::new();
        let mut floor: Option<(u32, String, f32, f32)> = None; // slot, tex, height, blur
        let mut scratch: Vec<Instance> = Vec::new();
        let mut swarm_jobs: Vec<SwarmJob> = Vec::new();
        let mut gpu_locals: Vec<[[f32; 4]; 4]> = Vec::new();
        let mut swarm_total: u32 = 0;
        self.frame_no += 1;
        let mut stats = FrameStats::default();
        // Lightning flash lighting up the whole scene: brightness, colour.
        let mut fx = FrameEnv {
            lightning: (0.0, [1.0; 3]),
            ..Default::default()
        };
        let mut env = project.scene_environment(ctx).eval(ctx);
        self.apply_env_light(project, ctx, &mut env, &mut fx);

        // Logos placed against the screen or each other.
        let logo_places = self.place_logos(project, &layers, ctx, w as f32, h as f32);
        let mut logo_rays: Vec<LogoRays> = Vec::new();
        // Glass and shadow rays need the picture behind the logos: make
        // room for it before the logos' bind groups are made.
        let wants_backdrop = layers.iter().any(|l| {
            matches!(&l.kind, LayerKind::Logo(g) if l.enabled
                && (g.glass.is_animated() || g.glass.base > 0.0
                    || (g.rays_shadow && (g.rays.is_animated() || g.rays.base > 0.0))))
        });
        if wants_backdrop {
            self.ensure_logo_backdrop(w, h);
        }
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
                    // The environment map background shows the map's
                    // panorama (none while the colours light the scene).
                    let pano = if b.kind == BackdropKind::Environment {
                        self.env
                            .pano
                            .clone()
                            .filter(|k| fx.ibl[0] > 0.0 && self.textures.contains_key(k))
                    } else {
                        None
                    };
                    let tex = match &pano {
                        Some(k) => k.clone(),
                        None => self.texture_key(project, b.texture.as_deref()),
                    };
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
                    let mirror = tex.ends_with(MIRROR_KEY);
                    blk[4] = [
                        if b.texture.is_some() || pano.is_some() {
                            1.0
                        } else {
                            0.0
                        },
                        if mirror { 1.0 } else { 0.0 },
                        0.0,
                        0.0,
                    ];
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
                    if b.kind == BackdropKind::Battle {
                        battle_block(&mut blk, &b.battle, ctx, mirror);
                    }
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
                    // A morph is raymarched too: a blend of two distance
                    // fields, drawn in the same box as the other SDF shapes.
                    let morph = m.morph.enabled;
                    let sdf = morph || matches!(m.source, MeshSource::Sdf { .. });
                    let mesh = if morph {
                        self.sdf_box_key()
                    } else if sdf {
                        self.mesh_key(&m.source)
                    } else {
                        let displaced = m.material.relief.displace.is_animated()
                            || m.material.relief.displace.base != 0.0;
                        self.mesh_key_subdivided(&m.source, m.subdivide, displaced)
                    };
                    let mat = &m.material;
                    let tex = self.texture_key(project, mat.texture.as_deref());
                    let rel = &mat.relief;
                    let relief_on = rel.bump.is_animated()
                        || rel.bump.base != 0.0
                        || rel.displace.is_animated()
                        || rel.displace.base != 0.0;
                    let relief_name = rel.texture.as_deref().or(mat.texture.as_deref());
                    let relief = if morph {
                        // Its relief slot holds the two distance fields.
                        self.morph_texture(&m.source, &m.morph.target)
                    } else if relief_on {
                        self.texture_key(project, relief_name)
                    } else {
                        "__white".to_string()
                    };
                    // The fields need smooth (linear) sampling.
                    let filter = if morph {
                        ez_core::TexFilter::Smooth
                    } else {
                        project.retro.filter_for(mat.tex_filter())
                    };
                    let pixelated = sampler_kind(filter);
                    let pbr = &mat.pbr;
                    let orm = self.texture_key_as(project, pbr.orm_map.as_deref(), true);
                    let emit = self.texture_key(project, pbr.emissive_map.as_deref());
                    let texs = (tex.clone(), relief, orm, emit, pixelated);
                    self.mesh_tex_bind_group(&texs);
                    let surface = match &m.instancer {
                        Instancer::Surface {
                            shape, count, seed, ..
                        } => Some(self.surface_points(shape, *count, *seed)),
                        _ => None,
                    };
                    let surface = surface.as_deref().map(|v| v.as_slice());
                    // With compute shaders every layout is placed on the GPU.
                    let gpu = self.swarm.is_some();
                    let mut first = instances.len() as u32;
                    let mut bounds = None;
                    let to_raw = |i: &Instance| InstanceRaw {
                        model: m4(i.model),
                        inst: [i.hue, i.glow, i.rand, i.along],
                    };
                    let count = if gpu {
                        // One dispatch per symmetry copy, into the copies
                        // buffer.
                        let lay = gpu_layout(&m.instancer, ctx, surface, &mut gpu_locals);
                        let n = lay.count;
                        // Copies on a terrain are placed in the world already.
                        let l = if matches!(m.instancer, Instancer::OnTerrain { .. }) {
                            Mat4::IDENTITY
                        } else {
                            layer_frame(&layer.transform, ctx)
                        };
                        let size = layer_scale(&layer.transform, ctx);
                        let v = &m.variation;
                        let mut bands = [[0.0f32; 4]; 4];
                        let spectrum_on = v.spectrum != 0.0 && ctx.music.active;
                        if spectrum_on {
                            for (k, b) in ctx.music.spectrum.iter().enumerate().take(16) {
                                bands[k / 4][k % 4] = *b;
                            }
                        }
                        first = swarm_total;
                        let syms = symmetry_matrices(&layer.symmetry);
                        // Where the copies can be: the layout's reach around
                        // each symmetry copy, plus the size of one copy.
                        if let (Some(reach), Some(g)) = (lay.reach, self.meshes.get(&mesh)) {
                            let grow = (1.0 + v.scale)
                                * (1.0 + v.ripple.abs())
                                * (1.0 + v.spectrum.abs())
                                * m.deform.reach(ctx).max(1.0);
                            let one = g.radius * size.abs().max_element() * grow;
                            let centres: Vec<Vec3> = syms
                                .iter()
                                .map(|s| (*s * l).transform_point3(Vec3::ZERO))
                                .collect();
                            let c = centres.iter().copied().sum::<Vec3>() / centres.len() as f32;
                            let spread = centres
                                .iter()
                                .map(|p| (*p - c).length())
                                .fold(0.0, f32::max);
                            bounds = Some((c, spread + reach + one * 1.5 + 0.5));
                        }
                        if n > 0 {
                            for sym in syms {
                                swarm_jobs.push(SwarmJob {
                                    params: SwarmParams {
                                        frame: m4(sym * l),
                                        size: [size.x, size.y, size.z, 0.0],
                                        shape: [
                                            lay.shape[0],
                                            lay.shape[1],
                                            lay.shape[2],
                                            ctx.phase,
                                        ],
                                        ints: [lay.layout, n, lay.seed, swarm_total],
                                        lay_u: lay.lay_u,
                                        lay_f: lay.lay_f,
                                        var_a: [
                                            v.rotation.to_radians(),
                                            v.spin as f32,
                                            v.scale,
                                            v.ripple,
                                        ],
                                        var_b: [
                                            v.ripple_cycles as f32,
                                            v.ripple_spread,
                                            v.chase,
                                            v.hue,
                                        ],
                                        var_c: [v.seed, spectrum_on as u32, 0, 0],
                                        var_d: [v.spectrum, 0.0, 0.0, 0.0],
                                        bands,
                                    },
                                    count: n,
                                });
                                swarm_total += n;
                            }
                        }
                        (swarm_total - first) as usize
                    } else if instances_are_static(layer, m) {
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
                    // A mirrored picture repeats every two tiles: scroll
                    // by pairs so the loop still closes.
                    let tiles = if tex.ends_with(MIRROR_KEY) { 2.0 } else { 1.0 };
                    blk[3] = [
                        ctx.phase * mat.scroll[0] as f32 * tiles,
                        ctx.phase * mat.scroll[1] as f32 * tiles,
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
                    let tri = self.triplanar_meshes.contains(&mesh);
                    blk[5] = [g.seed as f32, if tri { 1.0 } else { 0.0 }, 0.0, 0.0];
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
                    if morph {
                        blk[4] = [4.0, MORPH_GRID as f32, morph_columns() as f32, 0.0];
                        blk[5] = [m.morph.amount.eval(ctx).clamp(0.0, 1.0), 0.0, 0.0, 0.0];
                        blk[8] = [0.0; 4];
                        blk[9] = [0.0; 4];
                        blk[10] = [0.0; 4];
                        ls.triangles = 0;
                        ls.load = count as f32 / 30.0;
                    } else if let MeshSource::Sdf { form, cycles } = &m.source {
                        // No glitch, relief or deform on raymarched shapes:
                        // their slots hold the shape instead.
                        let [a, b, c] = form.params();
                        blk[4] = [form.index() as f32, a, b, c];
                        blk[5] = [
                            TAU * (ctx.phase * *cycles as f32).rem_euclid(1.0),
                            if *cycles != 0 { 1.5 } else { 0.0 },
                            0.0,
                            0.0,
                        ];
                        blk[8] = [0.0; 4];
                        blk[9] = [0.0; 4];
                        blk[10] = [0.0; 4];
                        // Marching costs per pixel, not per triangle.
                        ls.triangles = 0;
                        ls.load = count as f32 / 40.0;
                    }
                    // A liquid surface: splat size, blur tolerance and
                    // absorption (see liquid.wgsl).
                    let liquid = match &m.instancer {
                        Instancer::Fluid { fluid, .. } if fluid.surface && !sdf => {
                            let size = layer_scale(&layer.transform, ctx).max_element().max(1e-4);
                            let d = fluid.spacing.max(0.01);
                            blk[7][1] = d * 1.1 / size;
                            blk[7][2] = d * 2.0;
                            blk[7][3] = 1.5 / d.max(0.05);
                            true
                        }
                        _ => false,
                    };
                    cmds.push(Cmd::Mesh {
                        slot: blocks.len() as u32,
                        mesh,
                        texs,
                        liquid,
                        first,
                        count: count as u32,
                        sdf,
                        gpu,
                        bounds,
                    });
                    blocks.push(blk);
                    let mut pb = pbr_block(mat, ctx);
                    pb[3][0] = filter.index() as f32;
                    pb[3][1] = mat.mesh.eval(ctx).clamp(0.0, 1.0);
                    blocks.push(pb);
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
                    // Retro 3D may set one filter for everything (terrain
                    // has square or smooth pixels).
                    let own = if t.pixelated {
                        ez_core::TexFilter::Nearest
                    } else {
                        ez_core::TexFilter::Smooth
                    };
                    let t_nearest = sampler_kind(project.retro.filter_for(own)) == 1;
                    self.tex_bind_group(&tex, t_nearest);
                    let lm = layer_matrix(&layer.transform, ctx);
                    let cells = t.cells.clamp(4, t.max_cells());
                    let drawn = t.drawn_cells();
                    let syms = symmetry_matrices(&layer.symmetry);
                    ls.triangles = (drawn * drawn * 2) as u64 * syms.len() as u64;
                    ls.draws = syms.len() as u32;
                    ls.load = ls.triangles as f32 / 150_000.0 + 0.05;
                    // A mirrored picture repeats every two tiles: with an odd
                    // tile count the terrain only repeats every two lengths,
                    // so scroll by pairs of them and the loop still closes.
                    let odd = t.tiles.clamp(1, 256) % 2 == 1;
                    let scroll_tiles = if tex.ends_with(MIRROR_KEY) && odd {
                        2.0
                    } else {
                        1.0
                    };
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
                            (ctx.phase * t.scroll as f32 * scroll_tiles).rem_euclid(scroll_tiles),
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
                            pixelated: t_nearest,
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
                LayerKind::Logo(g) => {
                    let Some(at) = logo_places[li] else {
                        continue;
                    };
                    // Echoes: the logo a moment ago, oldest first, fading;
                    // then the logo now.
                    let mut draws = Vec::new();
                    for k in (1..=g.echoes.min(16)).rev() {
                        let mut past = *ctx;
                        let back = k as f32 * g.echo_spacing.max(0.0);
                        past.phase = (ctx.phase - back).rem_euclid(1.0);
                        past.beat_phase = (ctx.beat_phase - back).rem_euclid(1.0);
                        let places = self.place_logos(project, &layers, &past, w as f32, h as f32);
                        if let Some(then) = places[li] {
                            let fade = g.echo_fade.clamp(0.0, 1.0).powi(k as i32);
                            let d = self.logo_blocks(project, g, &past, then, w, h, flash, fade);
                            draws.push((d, false));
                        }
                    }
                    let now = self.logo_blocks(project, g, ctx, at, w, h, flash, 1.0);
                    draws.push((now, true));
                    ls.draws = draws.len() as u32;
                    ls.triangles = 2 * draws.len() as u64;
                    ls.load = 0.01 * draws.len() as f32;
                    for (d, current) in draws {
                        let Some((blks, (tex, morph, matcap), fit)) = d else {
                            continue;
                        };
                        let slot = blocks.len() as u32;
                        let glass = blks[2][10][0] > 0.0;
                        let strength = g.rays.eval(ctx).max(0.0);
                        if current && strength > 0.0 {
                            // Rays stream from the middle of the logo.
                            let lh = g.size.eval(ctx).max(0.0) * h as f32;
                            let [ax, ay] = g.anchor.point();
                            let local = (Vec2::splat(0.5) - Vec2::new(ax, ay))
                                * Vec2::new(lh * fit.aspect, lh);
                            let a = g.rotation.eval(ctx).to_radians();
                            let c = at
                                + Vec2::new(
                                    local.x * a.cos() - local.y * a.sin(),
                                    local.x * a.sin() + local.y * a.cos(),
                                );
                            logo_rays.push(LogoRays {
                                slot,
                                key: (tex.clone(), morph.clone(), matcap.clone()),
                                centre: [c.x / w as f32, 1.0 - c.y / h as f32],
                                strength,
                                length: g.rays_length.clamp(0.0, 1.0),
                                threshold: g.rays_threshold.max(0.0),
                                tint: g.rays_tint,
                                shadow: g.rays_shadow,
                            });
                        }
                        cmds.push(Cmd::Logo {
                            slot,
                            tex,
                            morph,
                            matcap,
                            glass,
                        });
                        blocks.extend(blks);
                    }
                }
                LayerKind::Arcs(arc) => {
                    let lm = layer_matrix(&layer.transform, ctx);
                    let syms = symmetry_matrices(&layer.symmetry);
                    // Copies of the layer the arcs connect to.
                    let copies = |name: &str, out: &mut Vec<Instance>| {
                        let Some(t) = layers.iter().find(|l| l.enabled && l.name == name) else {
                            return;
                        };
                        match &t.kind {
                            LayerKind::Mesh(m) => mesh_instances_with(t, m, ctx, None, out),
                            LayerKind::Sprite(sp) => {
                                copies_with(t, &sp.instancer, &sp.variation, ctx, None, out)
                            }
                            _ => out.push(Instance {
                                model: layer_matrix(&t.transform, ctx),
                                hue: 0.0,
                                glow: 1.0,
                                rand: 0.0,
                                along: 0.0,
                            }),
                        }
                    };
                    let mut ends: Vec<(Vec3, Vec3)> = Vec::new();
                    scratch.clear();
                    match &arc.path {
                        ArcPath::Points { from, to } => {
                            for sym in &syms {
                                let m = *sym * lm;
                                ends.push((
                                    m.transform_point3(Vec3::from(*from)),
                                    m.transform_point3(Vec3::from(*to)),
                                ));
                            }
                        }
                        ArcPath::Nearest { target, count } => {
                            copies(target, &mut scratch);
                            for sym in &syms {
                                let src = (*sym * lm).transform_point3(Vec3::ZERO);
                                let mut pts: Vec<Vec3> =
                                    scratch.iter().map(|i| i.model.w_axis.truncate()).collect();
                                pts.sort_by(|a, b| {
                                    (*a - src)
                                        .length_squared()
                                        .total_cmp(&(*b - src).length_squared())
                                });
                                ends.extend(
                                    pts.into_iter()
                                        .take((*count).max(1) as usize)
                                        .map(|p| (src, p)),
                                );
                            }
                        }
                        ArcPath::Chain { target } => {
                            copies(target, &mut scratch);
                            let pts: Vec<Vec3> =
                                scratch.iter().map(|i| i.model.w_axis.truncate()).collect();
                            let n = pts.len();
                            // Round the ring when there are enough copies.
                            let links = if n > 2 { n } else { n.saturating_sub(1) };
                            ends.extend((0..links).map(|i| (pts[i], pts[(i + 1) % n])));
                        }
                    }
                    ends.truncate(512);
                    let first = instances.len() as u32;
                    for (i, (a, b)) in ends.iter().enumerate() {
                        let seed = (arc.seed % 10_000) * 1000 + i as u32;
                        instances.push(InstanceRaw {
                            model: [
                                [a.x, a.y, a.z, seed as f32],
                                [b.x, b.y, b.z, 1.0],
                                [0.0; 4],
                                [0.0; 4],
                            ],
                            inst: [0.0; 4],
                        });
                    }
                    let count = ends.len() as u32;
                    let segments = if arc.branches { 32 + 12 } else { 32 };
                    ls.triangles = count as u64 * segments as u64 * 2;
                    ls.load = 0.01 + ls.triangles as f32 / 150_000.0;
                    let mut blk: Block = Zeroable::zeroed();
                    blk[0] = c4(arc.color, arc.glow.eval(ctx).max(0.0) * flash);
                    blk[1] = [
                        arc.strikes.max(1) as f32,
                        arc.crawl,
                        arc.fade.clamp(0.0, 1.0),
                        if arc.branches { 1.0 } else { 0.0 },
                    ];
                    blk[2] = [
                        ctx.phase,
                        arc.width.eval(ctx).max(0.0),
                        arc.jag.eval(ctx).max(0.0),
                        0.0,
                    ];
                    if count > 0 {
                        cmds.push(Cmd::Arcs {
                            slot: blocks.len() as u32,
                            segments,
                            first,
                            count,
                        });
                    }
                    blocks.push(blk);
                }
                LayerKind::Sprite(sp) => {
                    let tex = self.texture_key(project, sp.image.as_deref());
                    self.tex_bind_group(&tex, sp.pixelated);
                    scratch.clear();
                    copies_with(layer, &sp.instancer, &sp.variation, ctx, None, &mut scratch);
                    if sp.blend == SpriteBlend::Alpha {
                        // Soft edges need the far copies drawn first.
                        let d =
                            |i: &Instance| (i.model.w_axis.truncate() - cam.eye).length_squared();
                        scratch.sort_by(|a, b| d(b).total_cmp(&d(a)));
                    }
                    let first = instances.len() as u32;
                    instances.extend(scratch.iter().map(|i| InstanceRaw {
                        model: m4(i.model),
                        inst: [i.hue, i.glow, i.rand, i.along],
                    }));
                    let count = scratch.len() as u32;
                    ls.triangles = count as u64 * 2;
                    ls.load = 0.01 + count as f32 / 20_000.0;
                    let has_image = sp.image.as_deref().is_some_and(|n| !n.is_empty());
                    let (cols, rows) = (sp.columns.max(1), sp.rows.max(1));
                    let aspect = if has_image {
                        let t = &self.textures[&tex]._texture;
                        (t.width() as f32 / cols as f32)
                            / (t.height() as f32 / rows as f32).max(1.0)
                    } else {
                        1.0
                    };
                    let mut blk: Block = Zeroable::zeroed();
                    blk[0] = c4(sp.tint, sp.glow.eval(ctx).max(0.0) * flash);
                    blk[1] = [
                        cols as f32,
                        rows as f32,
                        sp.frame_count() as f32,
                        (ctx.phase * sp.cycles as f32).rem_euclid(1.0),
                    ];
                    blk[2] = [
                        match sp.facing {
                            SpriteFacing::Camera => 0.0,
                            SpriteFacing::Upright => 1.0,
                            SpriteFacing::Fixed => 2.0,
                        },
                        if sp.random_start { 1.0 } else { 0.0 },
                        aspect,
                        if has_image { 1.0 } else { 0.0 },
                    ];
                    blk[3] = [
                        sp.size.eval(ctx).max(0.0),
                        sp.opacity.eval(ctx).clamp(0.0, 1.0),
                        match sp.blend {
                            SpriteBlend::Alpha => 0.0,
                            SpriteBlend::Additive => 1.0,
                            SpriteBlend::Cutout => 2.0,
                            SpriteBlend::Mesh => 3.0,
                        },
                        0.0,
                    ];
                    if count > 0 {
                        cmds.push(Cmd::Sprite {
                            slot: blocks.len() as u32,
                            tex,
                            pixelated: sp.pixelated,
                            // The Saturn mesh is a solid cutout with holes.
                            blend: match sp.blend {
                                SpriteBlend::Mesh => SpriteBlend::Cutout,
                                b => b,
                            },
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
                    let texs = (tex.clone(), tex.clone(), tex.clone(), tex, 0);
                    self.mesh_tex_bind_group(&texs);
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
                        texs,
                        liquid: false,
                        first,
                        count: syms.len() as u32,
                        sdf: false,
                        gpu: false,
                        bounds: None,
                    });
                    blocks.push(blk);
                    blocks.push(Zeroable::zeroed());
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
                        if f.infinite {
                            INFINITE_FLOOR_RADIUS
                        } else {
                            f.size.max(0.1)
                        },
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
                    blk[4] = [
                        -ctx.phase * f.grid_scroll as f32,
                        0.0,
                        0.0,
                        if f.infinite { 1.0 } else { 0.0 },
                    ];
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
                let meshes: Vec<(u32, u32, bool)> = cmds
                    .iter()
                    .filter_map(|c| match c {
                        Cmd::Mesh {
                            first, count, gpu, ..
                        } if *count > 0 => Some((*first, *count, *gpu)),
                        _ => None,
                    })
                    .collect();
                if !meshes.is_empty() {
                    let mut blk: Block = Zeroable::zeroed();
                    blk[0] = [*fh, contact.clamp(0.0, 1.0), 0.0, 0.0];
                    let slot = blocks.len() as u32;
                    blocks.push(blk);
                    for (first, count, gpu) in meshes {
                        cmds.push(Cmd::Contact {
                            slot,
                            first,
                            count,
                            gpu,
                        });
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

        // Screen-space reflections: their settings, when something could
        // reflect (shapes, terrain).
        let refl = &project.environment.reflections;
        let ssr_slot = (refl.enabled
            && refl.strength.eval(ctx) > 0.0
            && cmds
                .iter()
                .any(|c| matches!(c, Cmd::Mesh { .. } | Cmd::Terrain { .. })))
        .then(|| {
            let mut blk: Block = Zeroable::zeroed();
            blk[0] = [
                refl.strength.eval(ctx).clamp(0.0, 1.0),
                refl.max_distance.max(0.1),
                refl.roughness_cutoff.clamp(0.01, 1.0),
                SSR_STEPS as f32,
            ];
            blk[1] = [0.3, 0.25, 0.0, 0.0];
            blocks.push(blk);
            blocks.len() as u32 - 1
        });

        // Light shafts: their settings, when there is fog to light.
        let sh = &project.environment.shafts;
        let fog_on = project.environment.fog_density.eval(ctx) > 0.0
            || project.environment.height_fog.density.eval(ctx) > 0.0;
        let shafts_slot = (sh.enabled && fog_on && sh.strength.eval(ctx) > 0.0).then(|| {
            let mut blk: Block = Zeroable::zeroed();
            blk[0] = [
                sh.strength.eval(ctx).max(0.0),
                sh.scattering.clamp(-0.95, 0.95),
                sh.steps.clamp(4, 96) as f32,
                sh.reach.max(1.0),
            ];
            blocks.push(blk);
            blocks.len() as u32 - 1
        });

        // --- uploads -------------------------------------------------------
        if blocks.is_empty() {
            blocks.push(Zeroable::zeroed());
        }
        if blocks.len() as u64 > self.draw_cap {
            self.draw_cap = (blocks.len() as u64).next_power_of_two();
            self.draw_buf = Self::make_draw_buf(&self.device, self.draw_cap);
            self.draw_bg = Self::make_draw_bg(&self.device, &self.bgl_draw, &self.draw_buf);
            self.draw_mesh_bg =
                Self::make_draw_mesh_bg(&self.device, &self.bgl_draw_mesh, &self.draw_buf);
            self.logo_fx_bg = Self::make_draw_bg_sized(
                &self.device,
                &self.bgl_logo_fx,
                &self.draw_buf,
                LOGO_FX_SIZE,
            );
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
        let mut main_globals = with_shadow(main_globals);
        // Retro 3D: the camera's views snap their vertices to a grid.
        if let Some((gw, gh)) = project.retro.snap_grid((w, h)) {
            main_globals.retro[0] = project.retro.snap_amount.eval(ctx).clamp(0.0, 1.0);
            main_globals.retro[1] = gw as f32;
            main_globals.retro[2] = gh as f32;
        }
        if project.retro.enabled {
            main_globals.retro_col[2] = project.retro.near_cull.eval(ctx).max(0.0);
        }
        self.queue
            .write_buffer(&self.globals_buf[0], 0, bytemuck::bytes_of(&main_globals));
        // ...and may be drawn at a low resolution.
        let retro_size = project.retro.internal_size((w, h));
        if let Some((lw, lh)) = retro_size {
            self.ensure_retro_target(target.id, (lw, lh));
            let mut g = main_globals;
            g.res = [lw as f32, lh as f32, 1.0 / lw as f32, 1.0 / lh as f32];
            self.queue
                .write_buffer(&self.globals_buf[GLOBALS_LOW], 0, bytemuck::bytes_of(&g));
        }
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
            let mut g = with_shadow(g);
            g.retro = main_globals.retro;
            g.retro_col = main_globals.retro_col;
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
                            resource: wgpu::BindingResource::Sampler(self.sampler_for(tex, false)),
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
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("ez2 frame"),
            });

        // --- the sky as the environment map ------------------------------------
        let light = &project.environment.env_light;
        if light.source == EnvSource::Sky {
            let bg = cmds.iter().find_map(|c| match c {
                Cmd::Backdrop {
                    slot, tex, kind, ..
                } => Some((*slot, tex.clone(), *kind)),
                _ => None,
            });
            if let Some((slot, tex, kind)) = bg {
                let once = light.sky_static;
                self.capture_sky(&mut enc, &main_globals, (slot, &tex, kind), once);
            }
        }

        let floor_bg = floor
            .as_ref()
            .and(self.floor_bg_cache.as_ref().map(|(_, bg)| bg));

        // --- big swarms: copies placed by the compute shader ------------------
        if let (Some(sw), false) = (self.swarm.as_mut(), swarm_jobs.is_empty()) {
            sw.reserve(
                &self.device,
                swarm_jobs.len() as u64,
                swarm_total as u64,
                gpu_locals.len() as u64,
            );
            if !gpu_locals.is_empty() {
                self.queue
                    .write_buffer(&sw.locals, 0, bytemuck::cast_slice(&gpu_locals));
            }
            let mut bytes = vec![0u8; swarm_jobs.len() * SWARM_SLOT as usize];
            for (i, job) in swarm_jobs.iter().enumerate() {
                let at = i * SWARM_SLOT as usize;
                bytes[at..at + std::mem::size_of::<SwarmParams>()]
                    .copy_from_slice(bytemuck::bytes_of(&job.params));
            }
            self.queue.write_buffer(&sw.params, 0, &bytes);
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("copies"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&sw.pipe);
            for (i, job) in swarm_jobs.iter().enumerate() {
                pass.set_bind_group(0, &sw.bg, &[(i as u64 * SWARM_SLOT) as u32]);
                pass.dispatch_workgroups(job.count.div_ceil(64), 1, 1);
            }
        }

        // --- sun shadow map -------------------------------------------------------
        if shadow.is_some() {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.group3.shadow_view,
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
                        Cmd::Backdrop { .. }
                            | Cmd::SkyFx
                            | Cmd::Mesh { .. }
                            | Cmd::Terrain { .. }
                            | Cmd::Sprite {
                                blend: SpriteBlend::Cutout,
                                ..
                            }
                    )
                });
                self.draw_scene(&mut pass, &self.refl_pipes, 1, &solid, false);
                self.draw_scene(&mut pass, &self.refl_pipes, 1, &clear, false);
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

        // One liquid is drawn as a surface (the last); any others show
        // their droplets.
        let mut last_liquid = true;
        for c in cmds.iter_mut().rev() {
            if let Cmd::Mesh { liquid, .. } = c {
                if *liquid && !std::mem::take(&mut last_liquid) {
                    *liquid = false;
                }
            }
        }
        // The liquid drawn as a surface: (slot, first copy, copies, on the
        // GPU).
        let liquids: Vec<(u32, u32, u32, bool)> = cmds
            .iter()
            .filter_map(|c| match c {
                Cmd::Mesh {
                    slot,
                    first,
                    count,
                    gpu,
                    liquid: true,
                    ..
                } if *count > 0 => Some((*slot, *first, *count, *gpu)),
                _ => None,
            })
            .collect();
        // --- liquid surfaces: droplets splatted and smoothed ----------------------
        for &(slot, first, count, gpu) in &liquids {
            let lp = &self.liquid_pipes;
            let far = wgpu::Color {
                r: 60_000.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            };
            let clear = wgpu::Color::TRANSPARENT;
            let steps = [
                (&target.liquid[0], far, &lp.splat_dist, 0, true),
                (&target.liquid[1], clear, &lp.splat_thick, 0, true),
                (&target.liquid[2], clear, &lp.blur_h, 1, false),
                (&target.liquid[3], clear, &lp.blur_v, 2, false),
            ];
            for (view, clear, pipe, bg, splat) in steps {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("liquid"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(pipe);
                pass.set_bind_group(0, &self.globals_bg[0], &[]);
                pass.set_bind_group(1, &self.draw_mesh_bg, &mesh_offsets(slot));
                pass.set_bind_group(2, &target.bg_liquid[bg], &[]);
                pass.set_bind_group(3, &self.shadow_bg, &[]);
                if splat {
                    pass.set_vertex_buffer(0, self.mesh_instances(gpu).slice(..));
                    pass.draw(0..6, first..first + count);
                } else {
                    pass.draw(0..3, 0..1);
                }
            }
        }

        // --- main scene -------------------------------------------------------
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
        let (solid, clear): (Vec<Cmd>, Vec<Cmd>) = rest.into_iter().partition(|c| {
            matches!(
                c,
                Cmd::Mesh { .. }
                    | Cmd::Terrain { .. }
                    | Cmd::Sprite {
                        blend: SpriteBlend::Cutout,
                        ..
                    }
            )
        });
        // Retro 3D at a low resolution: the scene is drawn small, then
        // blown up; text can stay sharp, drawn after at full size.
        let low = retro_size.and_then(|_| self.retro_targets.get(&target.id));
        let (clear, overlay): (Vec<Cmd>, Vec<Cmd>) =
            if low.is_some() && project.retro.sharp_overlays {
                clear
                    .into_iter()
                    .partition(|c| !matches!(c, Cmd::Text { .. }))
            } else {
                (clear, Vec::new())
            };
        let main = MainDraw {
            back: &back,
            solid: &solid,
            clear: &clear,
            cmds: &cmds,
            floor: floor
                .as_ref()
                .zip(floor_bg)
                .map(|((slot, ..), bg)| (*slot, bg)),
            liquid: liquids.last().map(|l| l.0),
        };
        let fog_clear = wgpu::Color {
            r: main_globals.fog[0] as f64,
            g: main_globals.fog[1] as f64,
            b: main_globals.fog[2] as f64,
            a: 1.0,
        };
        if let Some(rt) = low {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene (retro low resolution)"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &rt.color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(fog_clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &rt.depth,
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
            let set = PassPipes {
                scene: &self.refl_pipes,
                floor: &self.low_pipes.floor,
                contact: &self.low_pipes.contact,
                bg_up: &self.low_pipes.bg_up,
                liquid: &self.low_pipes.liquid,
                globals: GLOBALS_LOW,
            };
            self.draw_main(&mut pass, &set, target, &main);
        }
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
                        load: wgpu::LoadOp::Clear(fog_clear),
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
            if let Some(rt) = low {
                // The low-resolution scene with square pixels, then what
                // stays sharp.
                pass.set_pipeline(&self.retro_up_pipe);
                pass.set_bind_group(0, &rt.bg, &[]);
                pass.draw(0..3, 0..1);
                self.draw_scene(&mut pass, &self.main_pipes, 0, &overlay, true);
            } else {
                let set = PassPipes {
                    scene: &self.main_pipes,
                    floor: &self.floor_pipe,
                    contact: &self.contact_pipe,
                    bg_up: &self.bg_up_pipe,
                    liquid: &self.liquid_pipes.composite,
                    globals: 0,
                };
                self.draw_main(&mut pass, &set, target, &main);
            }
        }

        // --- distance to the camera (depth of field) and what surfaces
        // reflect (screen-space reflections) ----------------------------------
        if project.post.dof.enabled || ssr_slot.is_some() || shafts_slot.is_some() {
            let gbuf = |view| {
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })
            };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dof distance"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &target.dof_dist,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // Nothing drawn = very far (the sky).
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 60_000.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    gbuf(&target.gbuf[0]),
                    gbuf(&target.gbuf[1]),
                    gbuf(&target.gbuf[2]),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.dof_z,
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
            self.draw_distance(&mut pass, &cmds, floor.as_ref().map(|f| f.0));
        }
        // --- screen-space reflections, added onto the scene -----------------------
        if let Some(slot) = ssr_slot {
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ssr"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.ssr,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.ssr_pipe);
                pass.set_bind_group(0, &self.globals_bg[0], &[]);
                pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                pass.set_bind_group(2, &target.bg_ssr, &[]);
                pass.set_bind_group(3, &self.shadow_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            self.post_pass(
                &mut enc,
                "ssr add",
                &self.ssr_add_pipe,
                &target.hdr,
                &target.bg_ssr_add,
                SLOT_WARP,
                true,
            );
        }
        // --- sunlight scattered by the fog, added onto the scene ------------------
        if let Some(slot) = shafts_slot {
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("light shafts"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.shafts,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.shafts_pipe);
                pass.set_bind_group(0, &self.globals_bg[0], &[]);
                pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                pass.set_bind_group(2, &target.bg_ssr, &[]);
                pass.set_bind_group(3, &self.shadow_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            self.post_pass(
                &mut enc,
                "light shafts add",
                &self.ssr_add_pipe,
                &target.hdr,
                &target.bg_shafts_add,
                SLOT_WARP,
                true,
            );
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
        // --- logos, on the picture before the glows and trails -------------------
        let backdrop_needed = cmds
            .iter()
            .any(|c| matches!(c, Cmd::Logo { glass: true, .. }))
            || logo_rays.iter().any(|r| r.shadow);
        if backdrop_needed && self.logo_backdrop.2 >= w && self.logo_backdrop.3 >= h {
            // The picture behind the logos, for glass and shadow rays.
            enc.copy_texture_to_texture(
                target.hdr2_tex.as_image_copy(),
                self.logo_backdrop.0.as_image_copy(),
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
        if cmds.iter().any(|c| matches!(c, Cmd::Logo { .. })) {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("logos"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.hdr2,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.logo_pipe);
            pass.set_bind_group(0, &self.globals_bg[0], &[]);
            for c in &cmds {
                if let Cmd::Logo {
                    slot,
                    tex,
                    morph,
                    matcap,
                    ..
                } = c
                {
                    let key = (tex.clone(), morph.clone(), matcap.clone());
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_bind_group(2, &self.logo_bgs[&key], &[]);
                    pass.set_bind_group(3, &self.logo_fx_bg, &[(slot + 1) * DRAW_SLOT as u32]);
                    pass.draw(0..6, 0..1);
                }
            }
        }
        // --- rays streaming from logos -----------------------------------------
        if !logo_rays.is_empty() {
            self.ensure_logo_rays_source(target);
        }
        for (k, ray) in logo_rays.iter().take(LOGO_RAYS_MAX).enumerate() {
            let src = &self.logo_rays_src[&target.id];
            let slot = SLOT_LOGO_RAYS + k as u32;
            let mut p: PostBlock = Zeroable::zeroed();
            p[0] = [ray.centre[0], ray.centre[1], 1.0, ray.strength];
            p[1] = [ray.length, ray.threshold, w as f32 / h.max(1) as f32, 0.0];
            // No falloff: the whole logo streams.
            p[2] = c4(ray.tint, 0.0);
            self.queue.write_buffer(
                &self.post_buf,
                slot as u64 * POST_SLOT,
                bytemuck::bytes_of(&p),
            );
            // Source: the logo's light on black, or the picture behind it
            // with the logo's shadow cut out.
            if ray.shadow {
                enc.copy_texture_to_texture(
                    self.logo_backdrop.0.as_image_copy(),
                    src.tex.as_image_copy(),
                    wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                );
            }
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("logo rays source"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &src.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: if ray.shadow {
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
                pass.set_pipeline(if ray.shadow {
                    &self.logo_shadow_pipe
                } else {
                    &self.logo_pipe
                });
                pass.set_bind_group(0, &self.globals_bg[0], &[]);
                pass.set_bind_group(1, &self.draw_bg, &[ray.slot * DRAW_SLOT as u32]);
                pass.set_bind_group(2, &self.logo_bgs[&ray.key], &[]);
                pass.set_bind_group(3, &self.logo_fx_bg, &[(ray.slot + 1) * DRAW_SLOT as u32]);
                pass.draw(0..6, 0..1);
            }
            self.post_pass(
                &mut enc,
                "logo rays",
                &self.rays_pipe,
                &target.rays,
                &src.bg,
                slot,
                false,
            );
            self.post_pass(
                &mut enc,
                "logo rays add",
                &self.rays_add_pipe,
                &target.hdr2,
                &target.bg_rays_add,
                SLOT_RAYS_ADD,
                true,
            );
        }
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
        let fb = &project.post.feedback;
        if fb.enabled {
            let loop_s = project.timing.loop_seconds().max(0.01);
            let state = self.feedback.get(&target.id).copied();
            let next = match state {
                // The same moment drawn again (a paused preview while
                // editing): redo the last step from the same history
                // instead of feeding the picture back into itself.
                Some(st) if st.phase == ctx.phase => st,
                Some(st) => {
                    // Seconds since the last frame of this target: a jump
                    // (a scrub, the first frame) starts over without history.
                    let dt = (ctx.phase - st.phase).rem_euclid(1.0) * loop_s;
                    FeedbackState {
                        read: st.write,
                        write: st.read,
                        phase: ctx.phase,
                        dt,
                        fresh: dt > 0.5,
                    }
                }
                None => FeedbackState {
                    read: 0,
                    write: 1,
                    phase: ctx.phase,
                    dt: 0.0,
                    fresh: true,
                },
            };
            let (dt, fresh, read, write) = (next.dt, next.fresh, next.read, next.write);
            let keep = 0.5 + 0.49 * fb.length.eval(ctx).clamp(0.0, 1.0);
            let mut params = [[0.0f32; 4]; (POST_SLOT / 16) as usize];
            params[0] = [
                keep.powf(dt * 30.0),
                fb.zoom.max(0.01).powf(dt),
                (fb.turn * dt).to_radians(),
                fb.hue * dt,
            ];
            params[1] = [
                target.width as f32 / target.height.max(1) as f32,
                if fresh { 1.0 } else { 0.0 },
                0.0,
                0.0,
            ];
            self.queue.write_buffer(
                &self.post_buf,
                SLOT_FEEDBACK as u64 * POST_SLOT,
                bytemuck::cast_slice(&params),
            );
            self.post_pass(
                &mut enc,
                "feedback",
                &self.feedback_pipe,
                &target.fb[write].1,
                &target.bg_fb[read],
                SLOT_FEEDBACK,
                false,
            );
            enc.copy_texture_to_texture(
                target.fb[write].0.as_image_copy(),
                target.hdr2_tex.as_image_copy(),
                wgpu::Extent3d {
                    width: target.width,
                    height: target.height,
                    depth_or_array_layers: 1,
                },
            );
            self.feedback.insert(target.id, next);
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

    /// Make the retro low-resolution scene of target `id` (`size` pixels)
    /// if it isn't there at that size.
    fn ensure_retro_target(&mut self, id: u64, size: (u32, u32)) {
        if self.retro_targets.get(&id).is_some_and(|t| t.size == size) {
            return;
        }
        let tex = |label, format| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let color = tex("retro scene", HDR_FORMAT).create_view(&Default::default());
        let depth = tex("retro depth", DEPTH_FORMAT).create_view(&Default::default());
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("retro upscale"),
            layout: &self.bgl_retro_up,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&depth),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_point_clamp),
                },
            ],
        });
        self.retro_targets.insert(
            id,
            RetroTarget {
                size,
                color,
                depth,
                bg,
            },
        );
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
            // GPU copies aren't read back: their layout's reach.
            Cmd::Mesh {
                gpu: true, bounds, ..
            } => *bounds,
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
                    texs,
                    first,
                    count,
                    sdf,
                    gpu,
                    ..
                } if *count > 0 => {
                    let m = &self.meshes[mesh];
                    pass.set_pipeline(if *sdf {
                        &self.shadow_sdf_pipe
                    } else {
                        &self.shadow_mesh_pipe
                    });
                    pass.set_bind_group(0, &self.globals_bg[2], &[]);
                    pass.set_bind_group(1, &self.draw_mesh_bg, &mesh_offsets(*slot));
                    pass.set_bind_group(2, &self.mesh_tex_bgs[texs], &[]);
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_vertex_buffer(1, self.mesh_instances(*gpu).slice(..));
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

    /// Solid meshes and terrain with the distance-to-camera pipelines.
    fn draw_distance(&self, pass: &mut wgpu::RenderPass<'_>, cmds: &[Cmd], floor: Option<u32>) {
        if let Some(slot) = floor {
            pass.set_pipeline(&self.dof_floor_pipe);
            pass.set_bind_group(0, &self.globals_bg[0], &[]);
            pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
            pass.draw(0..6, 0..1);
        }
        for cmd in cmds {
            match cmd {
                Cmd::Mesh {
                    slot,
                    mesh,
                    texs,
                    first,
                    count,
                    sdf,
                    gpu,
                    liquid: false,
                    ..
                } if *count > 0 => {
                    let m = &self.meshes[mesh];
                    pass.set_pipeline(if *sdf {
                        &self.dof_sdf_pipe
                    } else {
                        &self.dof_mesh_pipe
                    });
                    pass.set_bind_group(0, &self.globals_bg[0], &[]);
                    pass.set_bind_group(1, &self.draw_mesh_bg, &mesh_offsets(*slot));
                    pass.set_bind_group(3, &self.shadow_bg, &[]);
                    pass.set_bind_group(2, &self.mesh_tex_bgs[texs], &[]);
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_vertex_buffer(1, self.mesh_instances(*gpu).slice(..));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.count, 0, *first..*first + *count);
                }
                Cmd::Terrain {
                    slot,
                    vertices,
                    tex,
                    pixelated,
                } => {
                    pass.set_pipeline(&self.dof_terrain_pipe);
                    pass.set_bind_group(0, &self.globals_bg[0], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_bind_group(2, &self.tex_bgs[&(tex.clone(), *pixelated)], &[]);
                    pass.draw(0..*vertices, 0..1);
                }
                _ => {}
            }
        }
    }

    /// The main scene: backgrounds, the floor, solid then see-through
    /// geometry, with the pipelines of the pass drawn into (full size or
    /// the retro low resolution).
    fn draw_main(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        set: &PassPipes<'_>,
        target: &RenderTarget,
        m: &MainDraw<'_>,
    ) {
        let g = &self.globals_bg[set.globals];
        for c in m.back {
            if let Cmd::BackdropUp { res } = c {
                if let Some((_, _, bg)) = target.bg_low.iter().find(|(d, _, _)| d == res) {
                    pass.set_pipeline(set.bg_up);
                    pass.set_bind_group(0, g, &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[0]);
                    pass.set_bind_group(2, bg, &[]);
                    pass.draw(0..3, 0..1);
                }
            }
        }
        self.draw_scene(pass, set.scene, set.globals, m.back, true);
        if let Some((slot, bg)) = m.floor {
            pass.set_pipeline(set.floor);
            pass.set_bind_group(0, g, &[]);
            pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
            pass.set_bind_group(2, bg, &[]);
            pass.set_bind_group(3, &self.shadow_bg, &[]);
            pass.draw(0..6, 0..1);
            for c in m.cmds {
                if let Cmd::Contact {
                    slot,
                    first,
                    count,
                    gpu,
                } = c
                {
                    pass.set_pipeline(set.contact);
                    pass.set_bind_group(0, g, &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_vertex_buffer(0, self.mesh_instances(*gpu).slice(..));
                    pass.draw(0..6, *first..*first + *count);
                }
            }
        }
        self.draw_scene(pass, set.scene, set.globals, m.solid, true);
        // The liquid surface, with its own depth.
        if let Some(slot) = m.liquid {
            pass.set_pipeline(set.liquid);
            pass.set_bind_group(0, g, &[]);
            pass.set_bind_group(1, &self.draw_mesh_bg, &mesh_offsets(slot));
            pass.set_bind_group(2, &target.bg_liquid[3], &[]);
            pass.set_bind_group(3, &self.shadow_bg, &[]);
            pass.draw(0..3, 0..1);
        }
        self.draw_scene(pass, set.scene, set.globals, m.clear, true);
    }

    /// Draw `cmds` with `pipes`. With `surface_liquids`, a liquid drawn as
    /// a surface (by its own passes) leaves out its droplets.
    fn draw_scene(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipes: &ScenePipes,
        globals: usize,
        cmds: &[Cmd],
        surface_liquids: bool,
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
                Cmd::Arcs {
                    slot,
                    segments,
                    first,
                    count,
                } => {
                    pass.set_pipeline(&pipes.arcs);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_vertex_buffer(0, self.inst_buf.slice(..));
                    pass.draw(0..6 * segments, *first..*first + *count);
                }
                Cmd::Sprite {
                    slot,
                    tex,
                    pixelated,
                    blend,
                    first,
                    count,
                } => {
                    let i = match blend {
                        SpriteBlend::Alpha => 0,
                        SpriteBlend::Additive => 1,
                        SpriteBlend::Cutout | SpriteBlend::Mesh => 2,
                    };
                    pass.set_pipeline(&pipes.sprite[i]);
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_bg, &[slot * DRAW_SLOT as u32]);
                    pass.set_bind_group(2, &self.tex_bgs[&(tex.clone(), *pixelated)], &[]);
                    pass.set_vertex_buffer(0, self.inst_buf.slice(..));
                    pass.draw(0..6, *first..*first + *count);
                }
                // Logos go on after depth of field, in their own pass.
                Cmd::BackdropUp { .. } | Cmd::Logo { .. } => {}
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
                    texs,
                    first,
                    count,
                    sdf,
                    gpu,
                    liquid,
                    ..
                } => {
                    // A liquid surface is drawn by its own passes (its
                    // droplets still show in the floor's reflection).
                    if *count == 0 || (*liquid && surface_liquids) {
                        continue;
                    }
                    let m = &self.meshes[mesh];
                    pass.set_pipeline(if *sdf { &pipes.sdf } else { &pipes.mesh });
                    pass.set_bind_group(0, &self.globals_bg[globals], &[]);
                    pass.set_bind_group(1, &self.draw_mesh_bg, &mesh_offsets(*slot));
                    pass.set_bind_group(2, &self.mesh_tex_bgs[texs], &[]);
                    pass.set_bind_group(3, &self.shadow_bg, &[]);
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_vertex_buffer(1, self.mesh_instances(*gpu).slice(..));
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

        let wb = &post.wobble;
        if wb.enabled {
            slots[SLOT_WARP as usize][4] = [
                (wb.mode.index() as f32).max(1.0),
                wb.amount.eval(ctx),
                wb.waves.eval(ctx),
                (wb.speed as f32 * ctx.phase).rem_euclid(1.0),
            ];
            slots[SLOT_WARP as usize][5] = [wb.lines.min(4096) as f32, 0.0, 0.0, 0.0];
        }

        if post.lens.enabled {
            slots[SLOT_WARP as usize][5][1] = post.lens.amount.eval(ctx).clamp(-1.0, 1.0);
        }

        let dof = &post.dof;
        if dof.enabled {
            let cam = project.camera.eval(ctx);
            let focus = if dof.auto_focus {
                (cam.target - cam.eye).length()
            } else {
                dof.focus.eval(ctx)
            };
            slots[SLOT_WARP as usize][3] = [
                1.0,
                focus.max(0.01),
                dof.blur.eval(ctx).clamp(0.0, 2.0) * 0.03,
                0.0,
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
            // Rays gather near the sun (falloff).
            slots[SLOT_RAYS as usize][2] = c4(project.scene_color(gr.tint, ctx), 2.5);
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
        let vhs = &post.vhs;
        if vhs.enabled {
            f[6] = [
                vhs.amount.eval(ctx).max(0.0),
                vhs.bleed.eval(ctx).max(0.0),
                vhs.bands.eval(ctx).max(0.0),
                0.0,
            ];
        }
        let ascii = &post.ascii;
        if ascii.enabled {
            let rows = ascii.rows.eval(ctx).clamp(4.0, 400.0);
            f[7] = [
                target.height as f32 / rows,
                ascii.backdrop.eval(ctx).clamp(0.0, 1.0),
                0.0,
                0.0,
            ];
            if let Some(c) = ascii.color.rgb() {
                f[24] = [c[0], c[1], c[2], 1.0];
            }
        }
        // Retro 3D: the N64 video filter, one console pixel wide (the
        // low resolution's, or 320 × 240's at full resolution).
        let rt = &project.retro;
        if rt.enabled {
            let vi = rt.vi_blur.eval(ctx).clamp(0.0, 1.0);
            if vi > 0.0 {
                let out = (target.width, target.height);
                let (cw, ch) = rt
                    .internal_size(out)
                    .unwrap_or_else(|| ez_core::retro::fit_to_output([320, 240], out));
                slots[SLOT_FINAL as usize][25] = [vi, 1.0 / cw as f32, 1.0 / ch as f32, 0.0];
            }
        }
        for (i, s) in slots.iter().enumerate() {
            self.queue
                .write_buffer(&self.post_buf, i as u64 * POST_SLOT, bytemuck::bytes_of(s));
        }
    }

    // ------------------------------------------------------------------
    // Targets

    /// A post pass's inputs: the parameters, two pictures and a sampler.
    fn post_bind_group(&self, a: &wgpu::TextureView, b: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
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
    }

    /// The logo rays' source picture for `target`, made the first time a
    /// logo with rays is drawn there. Ones not used for a while (targets
    /// gone or rays turned off) are dropped.
    fn ensure_logo_rays_source(&mut self, target: &RenderTarget) {
        let frame = self.frame_no;
        self.logo_rays_src
            .retain(|id, src| *id == target.id || frame - src.used < 240);
        let size = (target.width, target.height);
        if let Some(src) = self.logo_rays_src.get_mut(&target.id) {
            if src.size == size {
                src.used = frame;
                return;
            }
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("logo rays source"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let bg = self.post_bind_group(&view, &view);
        self.logo_rays_src.insert(
            target.id,
            LogoRaysSource {
                tex,
                view,
                bg,
                size,
                used: frame,
            },
        );
    }

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
        let full_tex = |label: &str, extra: wgpu::TextureUsages| {
            let t = dev.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HDR_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | sampled | extra,
                view_formats: &[],
            });
            let v = t.create_view(&Default::default());
            (t, v)
        };
        let (hdr2_tex, hdr2) = full_tex(
            "hdr2",
            wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
        );

        let fb = [
            full_tex("feedback a", wgpu::TextureUsages::COPY_SRC),
            full_tex("feedback b", wgpu::TextureUsages::COPY_SRC),
        ];
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
        let post_bg = |a: &wgpu::TextureView, b: &wgpu::TextureView| self.post_bind_group(a, b);
        let bg_blur_h = post_bg(&refl, &refl);
        let bg_blur_v = post_bg(&refl_tmp, &refl_tmp);
        let bg_fb = [post_bg(&hdr2, &fb[0].1), post_bg(&hdr2, &fb[1].1)];
        let (dw, dh) = ((w / 2).max(1), (h / 2).max(1));
        let dof_dist = tex("dof distance", dw, dh, DOF_FORMAT, 1, sampled);
        let dof_z = tex("dof depth", dw, dh, DEPTH_FORMAT, 1, none);
        let gbuf = [
            "g-buffer surface",
            "g-buffer reflectance",
            "g-buffer environment",
        ]
        .map(|l| tex(l, dw, dh, HDR_FORMAT, 1, sampled));
        let ssr = tex("ssr", dw, dh, HDR_FORMAT, 1, sampled);
        let bg_ssr = dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ssr"),
            layout: &self.bgl_ssr,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&gbuf[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&gbuf[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&gbuf[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&dof_dist),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&hdr),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.sampler_clamp),
                },
            ],
        });
        let bg_ssr_add = post_bg(&ssr, &dof_dist);
        let shafts = tex("light shafts", dw, dh, HDR_FORMAT, 1, sampled);
        let bg_shafts_add = post_bg(&shafts, &dof_dist);
        let liquid = [
            "liquid distance",
            "liquid thickness",
            "liquid blur",
            "liquid surface",
        ]
        .map(|l| tex(l, dw, dh, HDR_FORMAT, 1, sampled));
        let liquid_bg = |a: &wgpu::TextureView, b: &wgpu::TextureView| {
            dev.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("liquid"),
                layout: &self.liquid_pipes.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(a),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(b),
                    },
                ],
            })
        };
        // Splat (reads nothing), blur across (the splats), blur down (the
        // first blur), composite (the surface).
        let bg_liquid = [
            liquid_bg(&liquid[3], &liquid[3]),
            liquid_bg(&liquid[0], &liquid[1]),
            liquid_bg(&liquid[2], &liquid[2]),
            liquid_bg(&liquid[3], &liquid[3]),
        ];
        let bg_warp = post_bg(&hdr, &dof_dist);
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
            gbuf,
            ssr,
            bg_ssr,
            bg_ssr_add,
            shafts,
            bg_shafts_add,
            liquid,
            bg_liquid,
            rays,
            dof_dist,
            dof_z,
            hdr2_tex,
            fb,
            bg_fb,
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

    #[test]
    fn instance_cache_key_ignores_colours() {
        let a = Layer::default();
        let mut b = a.clone();
        b.kind.for_each_color_mut(|c| *c = [0.1, 0.9, 0.3]);
        assert_eq!(layer_hash(&a), layer_hash(&b));
        b.name.push('x');
        assert_ne!(layer_hash(&a), layer_hash(&b));
    }
}
