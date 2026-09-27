// Sprites: one quad per copy, facing the camera, standing upright or fixed
// in the scene, showing an image or one frame of a sprite sheet.
// D.v[0]: tint rgb, glow
// D.v[1]: sheet columns, rows, frames used, frame position (0..1 of the frames)
// D.v[2]: facing (0 camera, 1 upright, 2 fixed), random start, frame aspect (w/h), has image
// D.v[3]: height, opacity, blend (0 alpha, 1 additive, 2 cutout), _

@group(2) @binding(0) var t_tex: texture_2d<f32>;
@group(2) @binding(1) var s_tex: sampler;

struct SIn {
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    // hue, glow, rand, position along the copies
    @location(8) inst: vec4<f32>,
};

struct SOut {
    @builtin(position) pos: vec4<f32>,
    // Frame of the sheet (column, row).
    @location(0) @interpolate(flat) frame: vec2<f32>,
    // Position on the quad (0..1, y up).
    @location(1) local: vec2<f32>,
    @location(2) world: vec3<f32>,
    @location(3) @interpolate(flat) inst: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, in: SIn) -> SOut {
    let corner = quad_corner(vi);
    let centre = in.m3.xyz;
    let sx = length(in.m0.xyz);
    let sy = max(length(in.m1.xyz), 1e-6);
    let half_h = D.v[3].x * sy * 0.5;
    let half_w = D.v[3].x * sx * 0.5 * D.v[2].z;
    var right = G.cam_right.xyz;
    var up = G.cam_up.xyz;
    let facing = i32(D.v[2].x + 0.5);
    if (facing == 1) {
        up = vec3<f32>(0.0, 1.0, 0.0);
        let to_cam = G.cam_pos.xyz - centre;
        right = normalize(cross(up, vec3<f32>(to_cam.x, 0.0, to_cam.z) + vec3<f32>(0.0, 0.0, 1e-5)));
    } else if (facing == 2) {
        right = in.m0.xyz / max(sx, 1e-6);
        up = in.m1.xyz / sy;
    }
    let world = centre + right * corner.x * half_w + up * corner.y * half_h;
    // Frame of the sheet: whole passes per loop, optionally offset per copy.
    let cols = max(D.v[1].x, 1.0);
    let rows = max(D.v[1].y, 1.0);
    let frames = max(D.v[1].z, 1.0);
    let start = select(0.0, in.inst.z, D.v[2].y > 0.5);
    let f = min(floor(fract(D.v[1].w + start) * frames), frames - 1.0);
    let col = f - floor(f / cols) * cols;
    let row = floor(f / cols);
    let c = corner * 0.5 + 0.5;
    var out: SOut;
    out.pos = G.view_proj * vec4<f32>(world, 1.0);
    out.frame = vec2<f32>(col, row);
    out.local = c;
    out.world = world;
    out.inst = in.inst;
    return out;
}

@fragment
fn fs_main(in: SOut) -> @location(0) vec4<f32> {
    // Stay inside this frame of the sheet: cap the mip level at a few
    // texels per frame and keep samples half a texel from its border.
    let cols = max(D.v[1].x, 1.0);
    let rows = max(D.v[1].y, 1.0);
    let frame_px = vec2<f32>(textureDimensions(t_tex)) / vec2<f32>(cols, rows);
    let p = vec2<f32>(in.local.x, 1.0 - in.local.y) * frame_px;
    let lod_want = log2(max(max(length(dpdx(p)), length(dpdy(p))), 1e-6));
    let lod = clamp(lod_want, 0.0, max(log2(min(frame_px.x, frame_px.y)) - 2.0, 0.0));
    let e = 0.5 * exp2(lod) / frame_px;
    let q = clamp(vec2<f32>(in.local.x, 1.0 - in.local.y), e, vec2<f32>(1.0) - e);
    let uv = (in.frame + q) / vec2<f32>(cols, rows);
    let texel = textureSampleLevel(t_tex, s_tex, uv, lod);
    if (!clip_visible(in.world)) {
        discard;
    }
    var rgb = texel.rgb;
    var a = texel.a;
    if (D.v[2].w < 0.5) {
        // No image: a soft glowing dot.
        let d = length(in.local * 2.0 - 1.0);
        a = exp(-d * d * 3.5) * smoothstep(1.0, 0.75, d);
        rgb = vec3<f32>(1.0);
    }
    rgb = hue_rotate(rgb * D.v[0].rgb, in.inst.x) * max(D.v[0].w, 0.0) * in.inst.y;
    a = clamp(a * D.v[3].y, 0.0, 1.0);
    let fog = fog_amount_at(in.world);
    let mode = i32(D.v[3].z + 0.5);
    if (mode == 2) {
        if (a < 0.5) {
            discard;
        }
        return vec4<f32>(mix(rgb, G.fog.rgb, fog), 1.0);
    }
    if (mode == 1) {
        // Additive light fades into the fog.
        return vec4<f32>(rgb * a * (1.0 - fog), 0.0);
    }
    return vec4<f32>(mix(rgb, G.fog.rgb, fog) * a, a);
}
