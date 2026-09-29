// Mode 7 floor: an endless flat picture at a height, found per pixel by
// meeting the view ray with the plane, up to a hard horizon. It writes its
// own depth, so shapes stand on it and go behind it.
// D.v[0]: height, tile size, turn (radians, whole turns per loop), brightness
// D.v[1]: centre x, z (the turn's middle), scroll x, z (tiles, 0..1)
// D.v[2]: tint rgb, fog (0/1)

@group(2) @binding(0) var t_tex: texture_2d<f32>;
@group(2) @binding(1) var s_tex: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FullscreenOut {
    return fullscreen(vi);
}

struct M7Out {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(in: FullscreenOut) -> M7Out {
    let rd = view_ray(in.ndc);
    let cam = G.cam_pos.xyz;
    let t = (D.v[0].x - cam.y) / rd.y;
    // Rays that never reach the plane (above the horizon) stay empty. The
    // test is after the texture lookup below would be, so compute uv
    // without branching on it first.
    let valid = t > 0.0 && abs(rd.y) > 1e-5;
    let tc = select(1.0, t, valid);
    let world = cam + rd * tc;
    let a = D.v[0].z;
    let c = cos(a);
    let s = sin(a);
    let p = world.xz - D.v[1].xy;
    let q = vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
    let uv = q / max(D.v[0].y, 1e-3) + D.v[1].zw;
    let texel = textureSample(t_tex, s_tex, uv).rgb;
    if (!valid || !clip_visible(world)) {
        discard;
    }
    var col = texel * D.v[2].rgb * D.v[0].w;
    if (D.v[2].w > 0.5) {
        col = apply_fog_at(col, world);
    }
    let clip = G.view_proj * vec4<f32>(world, 1.0);
    var out: M7Out;
    out.color = vec4<f32>(retro_color(col, in.pos.xy), 1.0);
    out.depth = clamp(clip.z / clip.w, 0.0, 1.0);
    return out;
}
