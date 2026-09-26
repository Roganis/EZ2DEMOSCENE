// Laser beams: one camera-facing quad per beam, drawn additively.
// Beam directions are functions of (beam id, loop phase): they loop exactly.
// D.v[0]: count, pattern (0 fan, 1 cone, 2 scatter), spread (radians), length
// D.v[1]: colour a, width
// D.v[2]: colour b, brightness (intensity with the beat strobe applied)
// D.v[3]: sweep (radians), sweep phase (0..1), seed, rotation phase (0..1)
// D.v[8..11]: layer model matrix; beams leave the origin along local +Y.

struct LOut {
    @builtin(position) pos: vec4<f32>,
    // x: 0 at the source .. 1 at the tip, y: -1..1 across the beam
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32, @builtin(instance_index) beam: u32) -> LOut {
    let model = mat4x4<f32>(D.v[8], D.v[9], D.v[10], D.v[11]);
    let len = D.v[0].w;
    let width = D.v[1].w;
    let dir = beam_dir(beam);
    let s = (model * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
    let e = (model * vec4<f32>(dir * len, 1.0)).xyz;
    // Two triangles: (0,-1) (1,-1) (0,1) / (1,-1) (1,1) (0,1)
    var c = vec2<f32>(0.0, -1.0);
    switch vid {
        case 1u, 3u: { c = vec2<f32>(1.0, -1.0); }
        case 2u, 5u: { c = vec2<f32>(0.0, 1.0); }
        case 4u: { c = vec2<f32>(1.0, 1.0); }
        default: {}
    }
    let p = mix(s, e, c.x);
    let axis = normalize(e - s + vec3<f32>(0.0, 1e-5, 0.0));
    var side = cross(axis, G.cam_pos.xyz - p);
    side = side / max(length(side), 1e-4);
    // Beams widen slightly with distance, like real lasers in haze.
    let world = p + side * c.y * width * (1.0 + c.x * 2.0);
    var out: LOut;
    out.pos = G.view_proj * vec4<f32>(world, 1.0);
    out.uv = c;
    out.world = world;
    let count = max(D.v[0].x, 1.0);
    var t = 0.0;
    if (count > 1.5) {
        t = f32(beam) / (count - 1.0);
    }
    out.color = mix(D.v[1].rgb, D.v[2].rgb, t) * D.v[2].w;
    return out;
}

@fragment
fn fs_main(in: LOut) -> @location(0) vec4<f32> {
    if (!clip_visible(in.world)) {
        discard;
    }
    let x = in.uv.y;
    let core = exp(-x * x * 8.0);
    let halo = exp(-abs(x) * 3.0) * 0.35;
    // Bright at the source, fading towards the tip.
    let fade = pow(1.0 - in.uv.x, 1.5) * smoothstep(0.0, 0.01, in.uv.x + 0.002);
    let a = (core + halo) * fade * (1.0 - fog_amount_at(in.world));
    return vec4<f32>(in.color * a, 0.0);
}
