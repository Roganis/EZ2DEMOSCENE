// Waterfall: a curtain pouring from the layer origin down its -Y axis and
// arcing out along +Z, with streaks that run down a whole number of times
// per loop, plus foam / smoke puffs at the foot. Premultiplied alpha.
// D.v[0]: kind (0 water, 1 lava, 2 toxic), width, height, push
// D.v[1]: colour, glow
// D.v[2]: streak offset (0..1), foam, seed, puff lives per loop
// D.v[8..11]: layer model matrix

const COLS: u32 = 12u;
const ROWS: u32 = 40u;
const STREAK_P: i32 = 6;

struct FOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    // sheet: u across (0..1), t down (0..1); puffs: quad -1..1
    @location(1) uv: vec2<f32>,
    @location(2) normal: vec3<f32>,
    @location(3) @interpolate(flat) puff: u32,
    @location(4) fade: f32,
};

fn f_local(u: f32, t: f32) -> vec3<f32> {
    let width = D.v[0].y;
    let height = D.v[0].z;
    let push = D.v[0].w;
    return vec3<f32>((u - 0.5) * width * (1.0 + t * 0.2), -height * t, push * sqrt(t));
}

// Value noise that repeats every `py` cells along y.
fn f_noise(p: vec2<f32>, py: i32, seed: u32) -> f32 {
    let i = floor(p);
    let f = p - i;
    let s = f * f * (3.0 - 2.0 * f);
    let x0 = i32(i.x);
    let y0 = ((i32(i.y) % py) + py) % py;
    let y1 = (y0 + 1) % py;
    let a = hash1(hash_u(u32(x0 + 4096) * 0x8da6b343u) ^ hash_u(u32(y0) * 0xd8163841u) ^ seed);
    let b = hash1(hash_u(u32(x0 + 4097) * 0x8da6b343u) ^ hash_u(u32(y0) * 0xd8163841u) ^ seed);
    let c = hash1(hash_u(u32(x0 + 4096) * 0x8da6b343u) ^ hash_u(u32(y1) * 0xd8163841u) ^ seed);
    let d = hash1(hash_u(u32(x0 + 4097) * 0x8da6b343u) ^ hash_u(u32(y1) * 0xd8163841u) ^ seed);
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> FOut {
    let model = mat4x4<f32>(D.v[8], D.v[9], D.v[10], D.v[11]);
    var out: FOut;
    if (ii > 0u) {
        // Foam / smoke puff at the foot, growing and rising as it fades.
        let k = ii - 1u;
        let seed = u32(D.v[2].z);
        let h0 = hash2u(k, seed);
        let lives = max(D.v[2].w, 1.0);
        let x = G.time.x * lives + h0;
        let life = fract(x);
        let cyc = u32(floor(x) - floor(x / lives) * lives);
        let h1 = hash2u(k * 7u + cyc * 131u, seed + 1u);
        let h2 = hash2u(k * 13u + cyc * 71u, seed + 2u);
        let width = D.v[0].y;
        let foot = f_local(h1, 1.0);
        let local = foot + vec3<f32>(0.0, life * width * 0.35, (h2 - 0.3) * width * 0.3);
        let centre = (model * vec4<f32>(local, 1.0)).xyz;
        let size = width * (0.12 + 0.3 * life) * (0.7 + 0.6 * h2);
        let c = quad_corner(vi);
        let world = centre + (G.cam_right.xyz * c.x + G.cam_up.xyz * c.y) * size;
        out.pos = G.view_proj * vec4<f32>(world, 1.0);
        out.world = world;
        out.uv = c;
        out.puff = 1u;
        out.fade = sin(PI * life) * D.v[2].y;
        out.normal = vec3<f32>(0.0, 1.0, 0.0);
        return out;
    }
    let cell = vi / 6u;
    let corner = vi % 6u;
    var o = vec2<f32>(0.0, 0.0);
    switch corner {
        case 1u, 3u: { o = vec2<f32>(1.0, 0.0); }
        case 2u, 5u: { o = vec2<f32>(0.0, 1.0); }
        case 4u: { o = vec2<f32>(1.0, 1.0); }
        default: {}
    }
    let u = (f32(cell % COLS) + o.x) / f32(COLS);
    let t = (f32(cell / COLS) + o.y) / f32(ROWS);
    let p = f_local(u, t);
    let p2 = f_local(u, min(t + 0.01, 1.0));
    let tangent = normalize(p2 - p + vec3<f32>(0.0, -1e-4, 0.0));
    let n_local = normalize(cross(vec3<f32>(1.0, 0.0, 0.0), tangent));
    let world = (model * vec4<f32>(p, 1.0)).xyz;
    out.pos = G.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.uv = vec2<f32>(u, t);
    out.normal = normalize((model * vec4<f32>(n_local, 0.0)).xyz);
    out.puff = 0u;
    out.fade = 1.0;
    return out;
}

@fragment
fn fs_main(in: FOut) -> @location(0) vec4<f32> {
    if (!clip_visible(in.world)) {
        discard;
    }
    let kind = i32(D.v[0].x + 0.5);
    let col_in = D.v[1].rgb;
    let glow = max(D.v[1].w, 0.0);
    let l = normalize(G.light_dir.xyz);
    let ambient = mix(G.ground.rgb, G.sky.rgb, 0.7) * G.sky.w;
    let sun = G.light_color.rgb * G.ground.w;
    let fog = fog_amount_at(in.world);
    if (in.puff == 1u) {
        let r2 = dot(in.uv, in.uv);
        var a = exp(-r2 * 3.0) * smoothstep(1.0, 0.7, r2) * in.fade;
        var c = vec3<f32>(0.0);
        switch kind {
            case 1: {
                // Smoke lit from below by the lava.
                c = mix(vec3<f32>(0.08, 0.06, 0.06), col_in * glow, 0.35);
                a = a * 0.5;
            }
            case 2: {
                c = col_in * glow * 0.6;
                a = a * 0.4;
            }
            default: {
                c = vec3<f32>(0.92, 0.95, 1.0) * (ambient + sun * 0.6);
                a = a * 0.5;
            }
        }
        c = mix(c, G.fog.rgb, fog);
        return vec4<f32>(c * a, a);
    }
    let u = in.uv.x;
    let t = in.uv.y;
    let seed = u32(D.v[2].z);
    // Streaks: long along the fall, scrolling down (periodic along t).
    let y = (t - D.v[2].x) * f32(STREAK_P);
    let s1 = f_noise(vec2<f32>(u * 22.0, y), STREAK_P, seed);
    let s2 = f_noise(vec2<f32>(u * 61.0, y * 3.0), STREAK_P * 3, seed + 5u);
    let streak = s1 * 0.65 + s2 * 0.35;
    let edges = smoothstep(0.0, 0.07, u) * smoothstep(1.0, 0.93, u) * smoothstep(0.0, 0.02, t) * smoothstep(1.0, 0.9, t);
    let n = normalize(in.normal);
    let v = normalize(G.cam_pos.xyz - in.world);
    var c = vec3<f32>(0.0);
    var a = 0.0;
    switch kind {
        case 1: {
            // Lava: glowing streams with dark cooling crust.
            let crust = smoothstep(0.55, 0.7, streak);
            let hot = col_in * glow * (0.6 + 2.2 * (1.0 - crust) * streak);
            c = mix(hot, vec3<f32>(0.05, 0.03, 0.03) * (ambient + sun * 0.5), crust * 0.8);
            a = 0.97 * edges;
        }
        case 2: {
            c = col_in * glow * (0.35 + 1.4 * streak);
            a = (0.6 + 0.35 * streak) * edges;
        }
        default: {
            // Water: bright, foamy streaks over a translucent sheet with a
            // bit of sky reflected.
            let lit = ambient + sun * (0.4 + 0.6 * max(dot(n, l), 0.0));
            let foam = smoothstep(0.45, 0.8, streak) + smoothstep(0.7, 1.0, t) * 0.6;
            let fres = 0.05 + 0.4 * pow(1.0 - abs(dot(n, v)), 3.0);
            c = mix(col_in * 0.6, vec3<f32>(0.95, 0.97, 1.0), clamp(foam, 0.0, 1.0)) * lit * glow;
            c = c + G.sky.rgb * fres * 0.5;
            a = clamp(0.35 + 0.55 * streak + foam * 0.3, 0.0, 0.95) * edges;
        }
    }
    c = mix(c, G.fog.rgb, fog);
    return vec4<f32>(c * a, a);
}
