// Retro 3D: the scene drawn at a low resolution, blown up to the screen
// with square pixels (no smoothing), with its depth, so text drawn after it
// is still hidden behind the shapes.

@group(0) @binding(0) var t_color: texture_2d<f32>;
@group(0) @binding(1) var t_depth: texture_depth_2d;
// Nearest, non-filtering: WebGL2 can't load texels of depth textures, but
// can sample them.
@group(0) @binding(2) var s_depth: sampler;

struct UpVOut {
    @builtin(position) pos: vec4<f32>,
    // 0..1 from the top left.
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> UpVOut {
    var out: UpVOut;
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    let p = vec2<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0);
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    return out;
}

struct UpOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(in: UpVOut) -> UpOut {
    let size = vec2<i32>(textureDimensions(t_color));
    let p = clamp(vec2<i32>(floor(in.uv * vec2<f32>(size))), vec2<i32>(0), size - 1);
    var out: UpOut;
    out.color = vec4<f32>(textureLoad(t_color, p, 0).rgb, 1.0);
    out.depth = textureSampleLevel(t_depth, s_depth, (vec2<f32>(p) + 0.5) / vec2<f32>(size), 0);
    return out;
}
