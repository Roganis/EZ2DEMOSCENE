// Spotlight cones: hazy volumes of light along the beam directions (see
// beams.wgsl), drawn additively, plus pools of light where they hit y = 0.
// D.v[0]: count, pattern, spread (radians), length
// D.v[1]: colour a, _
// D.v[2]: colour b, brightness
// D.v[3]: sweep, sweep phase, seed, rotation phase
// D.v[4]: tan(half cone angle), pools on, floor height, _
// D.v[8..11]: layer model matrix

const SEGS: u32 = 24u;

struct SOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // x: 0 at the lamp .. 1 at the end; pools: -1..1 across the disc
    @location(2) uv: vec2<f32>,
    @location(3) color: vec3<f32>,
    @location(4) @interpolate(flat) pool: u32,
};

fn beam_color(i: u32) -> vec3<f32> {
    let count = max(D.v[0].x, 1.0);
    var t = 0.0;
    if (count > 1.5) {
        t = f32(i) / (count - 1.0);
    }
    return mix(D.v[1].rgb, D.v[2].rgb, t) * D.v[2].w;
}

fn frame(axis: vec3<f32>) -> mat2x3<f32> {
    var helper = vec3<f32>(0.0, 1.0, 0.0);
    if (abs(axis.y) > 0.9) {
        helper = vec3<f32>(1.0, 0.0, 0.0);
    }
    let u = normalize(cross(axis, helper));
    let v = cross(axis, u);
    return mat2x3<f32>(u, v);
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> SOut {
    let model = mat4x4<f32>(D.v[8], D.v[9], D.v[10], D.v[11]);
    let count = u32(max(D.v[0].x, 1.0) + 0.5);
    let len = D.v[0].w;
    let tan_a = D.v[4].x;
    let beam = ii % count;
    let s = (model * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
    let e = (model * vec4<f32>(beam_dir(beam) * len, 1.0)).xyz;
    let axis = normalize(e - s + vec3<f32>(0.0, 1e-5, 0.0));
    let wlen = length(e - s);
    let f = frame(axis);
    var out: SOut;
    out.color = beam_color(beam);
    out.pool = 0u;
    if (ii >= count) {
        // Pool of light on the ground where the cone lands.
        out.pool = 1u;
        let floor_y = D.v[4].z;
        var hit = -1.0;
        if (axis.y < -0.02) {
            hit = (floor_y - s.y) / axis.y;
        }
        let c = quad_corner(vi);
        if (hit <= 0.0 || hit > wlen * 1.3 || D.v[4].y < 0.5) {
            out.pos = vec4<f32>(2.0, 2.0, 2.0, 1.0);
            return out;
        }
        let centre = s + axis * hit;
        let r = hit * tan_a;
        var along = vec3<f32>(axis.x, 0.0, axis.z);
        if (dot(along, along) < 1e-6) {
            along = vec3<f32>(1.0, 0.0, 0.0);
        }
        along = normalize(along);
        let side = vec3<f32>(-along.z, 0.0, along.x);
        let stretch = 1.0 / max(abs(axis.y), 0.25);
        let world = vec3<f32>(centre.x, floor_y + 0.02, centre.z) + along * c.x * r * stretch * 1.4 + side * c.y * r * 1.4;
        out.pos = G.view_proj * vec4<f32>(world, 1.0);
        out.world = world;
        out.normal = vec3<f32>(0.0, 1.0, 0.0);
        out.uv = c * 1.4;
        out.color = out.color * (1.0 - smoothstep(wlen, wlen * 1.3, hit));
        return out;
    }
    // Cone side: SEGS quads between a small ring at the lamp and the end.
    let seg = vi / 6u;
    let corner = vi % 6u;
    var o = vec2<f32>(0.0, 0.0);
    switch corner {
        case 1u, 3u: { o = vec2<f32>(1.0, 0.0); }
        case 2u, 5u: { o = vec2<f32>(0.0, 1.0); }
        case 4u: { o = vec2<f32>(1.0, 1.0); }
        default: {}
    }
    let ang = TAU * (f32(seg) + o.x) / f32(SEGS);
    let radial = f[0] * cos(ang) + f[1] * sin(ang);
    let t = o.y;
    let r = mix(0.08, wlen * tan_a, t);
    let world = s + axis * wlen * t + radial * r;
    out.pos = G.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = radial;
    out.uv = vec2<f32>(t, 0.0);
    return out;
}

@fragment
fn fs_main(in: SOut) -> @location(0) vec4<f32> {
    if (!clip_visible(in.world)) {
        discard;
    }
    let fog = 1.0 - fog_amount_at(in.world);
    if (in.pool == 1u) {
        let r2 = dot(in.uv, in.uv);
        let a = exp(-r2 * 2.2) * smoothstep(1.9, 1.2, r2);
        return vec4<f32>(in.color * a * 0.6 * fog, 0.0);
    }
    let v = normalize(G.cam_pos.xyz - in.world);
    // Light is brightest where we look through the most haze: the middle of
    // the cone, not its silhouette.
    let through = pow(abs(dot(normalize(in.normal), v)), 1.5);
    let t = in.uv.x;
    let fade = pow(1.0 - t, 1.3) * smoothstep(0.0, 0.04, t);
    let dust = 0.75 + 0.25 * vnoise3(in.world * 1.3);
    let a = through * fade * dust * 0.18;
    return vec4<f32>(in.color * a * fog, 0.0);
}
