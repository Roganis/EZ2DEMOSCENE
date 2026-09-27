// Electric arcs: one instance per arc, its segments from the vertex index
// (six vertices each). The zigzag is the weather bolt's noise with both
// ends pinned; it crawls during a strike and changes shape at each new
// strike, a whole number of times per loop.
// D.v[0]: colour, glow
// D.v[1]: strikes per loop, crawl, fade, branches (0/1)
// D.v[2]: loop phase, width, jag, _
// Instance: m0 = start (xyz), seed; m1 = end (xyz), brightness

const ARC_MAIN: u32 = 32u;
const ARC_BRANCH: u32 = 12u;

struct AIn {
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(8) inst: vec4<f32>,
};

struct AOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) quad: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world: vec3<f32>,
};

// Point `t` along an arc pinned at both ends; `crawl` slides the noise.
fn arc_point(a: vec3<f32>, b: vec3<f32>, t: f32, seed: u32, jag: f32, crawl: f32) -> vec3<f32> {
    let axis = b - a;
    let len = length(axis);
    var side = cross(axis, vec3<f32>(0.3, 0.1, 1.0));
    if (dot(side, side) < 1e-8) {
        side = cross(axis, vec3<f32>(1.0, 0.0, 0.0));
    }
    side = normalize(side);
    let side2 = normalize(cross(axis, side));
    let x = t + crawl;
    let d1 = bolt_noise(x * 5.0, seed) * 0.55 + bolt_noise(x * 15.0, seed + 1u) * 0.3 + bolt_noise(x * 41.0, seed + 2u) * 0.15;
    let d2 = bolt_noise(x * 4.0, seed + 3u) * 0.55 + bolt_noise(x * 17.0, seed + 4u) * 0.3;
    let env = sqrt(clamp(t * 6.0, 0.0, 1.0) * clamp((1.0 - t) * 6.0, 0.0, 1.0));
    return a + axis * t + (side * d1 + side2 * d2) * len * jag * env;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, in: AIn) -> AOut {
    let c = quad_corner(vi % 6u);
    let k = vi / 6u;
    let p_from = in.m0.xyz;
    let p_to = in.m1.xyz;
    let strikes = max(D.v[1].x, 1.0);
    // Which strike, and how far into it.
    let s = D.v[2].x * strikes;
    let strike = u32(floor(s) - floor(s / strikes) * strikes);
    let into = fract(s);
    let seed = hash_u(u32(in.m0.w) * 0x9e3779b9u + strike * 0x85ebca6bu);
    let crawl = into * D.v[1].y;
    let jag = D.v[2].z;
    var a: vec3<f32>;
    var b: vec3<f32>;
    var width = D.v[2].y;
    var bright = 1.0;
    if (k < ARC_MAIN) {
        let t0 = f32(k) / f32(ARC_MAIN);
        let t1 = f32(k + 1u) / f32(ARC_MAIN);
        a = arc_point(p_from, p_to, t0, seed, jag, crawl);
        b = arc_point(p_from, p_to, t1, seed, jag, crawl);
    } else {
        // A branch forks off and fizzles out.
        let j = k - ARC_MAIN;
        let start_t = 0.2 + hash2u(seed, 5u) * 0.5;
        let root = arc_point(p_from, p_to, start_t, seed, jag, crawl);
        let axis = p_to - p_from;
        var out = cross(axis, vec3<f32>(0.0, 1.0, 0.0));
        if (dot(out, out) < 1e-8) {
            out = cross(axis, vec3<f32>(1.0, 0.0, 0.0));
        }
        out = normalize(out) * select(-1.0, 1.0, hash2u(seed, 6u) > 0.5);
        let end = root + axis * 0.25 + out * length(axis) * (0.15 + 0.2 * hash2u(seed, 7u));
        let t0 = f32(j) / f32(ARC_BRANCH);
        let t1 = f32(j + 1u) / f32(ARC_BRANCH);
        a = bolt_point(root, end, t0, seed + 9u, jag * 1.5);
        b = bolt_point(root, end, t1, seed + 9u, jag * 1.5);
        width = width * 0.6 * (1.0 - t0 * 0.7);
        bright = 0.6 * (1.0 - t0) * D.v[1].w;
    }
    let wp = beam_quad(a, b, width, c);
    // Each strike flashes and fades towards the next.
    let flash = 1.0 - D.v[1].z * into;
    var out: AOut;
    out.pos = G.view_proj * vec4<f32>(wp, 1.0);
    out.quad = c;
    out.world = wp;
    out.color = D.v[0].rgb * max(D.v[0].w, 0.0) * bright * flash * in.m1.w * (1.0 - fog_amount_at(wp));
    return out;
}

@fragment
fn fs_main(in: AOut) -> @location(0) vec4<f32> {
    if (!clip_visible(in.world)) {
        discard;
    }
    let q = in.quad;
    // A white-hot core inside a coloured glow.
    let core = exp(-q.x * q.x * 30.0);
    let halo = exp(-q.x * q.x * 3.0) * 0.35;
    let rgb = in.color * (halo + core) + vec3<f32>(core * 0.6) * length(in.color) * 0.3;
    return vec4<f32>(rgb, 0.0);
}
