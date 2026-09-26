// Lit, instanced meshes.
// D.v[0]: base rgb, metallic
// D.v[1]: emissive rgb (colour * strength), roughness
// D.v[2]: emissive mode, has texture, texture scale, flat shading
// D.v[3]: scroll u, scroll v, rim, hue shift
// D.v[4]: glitch amount, style (0 jitter, 1 slices, 2 shatter), steps per loop, chance
// D.v[5]: glitch seed
// D.v[6]: pulse mode (4): pulses, head position (0..1), pulse length, pulse glow
// D.v[7]: pulse mode (4): base glow
// D.v[8]: relief strength, displacement, relief mode (0 bump, 1 normal map), has relief

@group(2) @binding(0) var t_tex: texture_2d<f32>;
@group(2) @binding(1) var s_tex: sampler;
@group(2) @binding(2) var t_relief: texture_2d<f32>;

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) edge: f32,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    // hue, glow, rand, unused
    @location(8) inst: vec4<f32>,
};

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) edge: f32,
    @location(4) inst: vec4<f32>,
};

fn hash_v3(p: vec3<f32>, salt: u32) -> vec3<f32> {
    let q = vec3<i32>(floor(p * 64.0));
    let h = hash_u(u32(q.x) * 0x8da6b343u) ^ hash_u(u32(q.y) * 0xd8163841u) ^ hash_u(u32(q.z) * 0xcb1ab31fu) ^ salt;
    return vec3<f32>(hash1(h), hash1(h ^ 0x68e31da4u), hash1(h ^ 0xb5297a4du)) - 0.5;
}

// Loop-safe corruption of the object-space position.
fn glitch(pos: vec3<f32>, normal: vec3<f32>, inst_rand: f32) -> vec3<f32> {
    let amount = D.v[4].x;
    if (amount <= 0.0) {
        return pos;
    }
    let steps = max(D.v[4].z, 1.0);
    let step = u32(floor(fract(G.time.x) * steps));
    let seed = u32(D.v[5].x) * 0x9e3779b9u + u32(inst_rand * 65536.0);
    let salt = hash_u(step ^ seed);
    // Only some steps glitch.
    if (hash1(salt ^ 0x51ed270bu) >= D.v[4].w) {
        return pos;
    }
    let style = i32(D.v[4].y + 0.5);
    var p = pos;
    switch style {
        case 1: {
            // Horizontal slices shift sideways.
            let band = u32(i32(floor(pos.y * 8.0)) + 1024);
            let r = hash1(hash_u(band) ^ salt);
            if (r < 0.4) {
                let dir = vec3<f32>(hash1(band ^ salt ^ 0x1234567u) - 0.5, 0.0, hash1(band ^ salt ^ 0x7654321u) - 0.5);
                p = p + normalize(dir + vec3<f32>(1e-4, 0.0, 0.0)) * (r * 2.5 + 0.2) * amount * 0.5;
            }
        }
        case 2: {
            // Faces fly apart: every face moves along its own normal.
            let r = hash_v3(normal, salt);
            p = p + normal * (r.x + 0.5) * amount * 0.6 + r * amount * 0.15;
        }
        default: {
            // Vertices shake (shared positions move together: no holes).
            p = p + hash_v3(pos, salt) * amount * 0.4;
        }
    }
    return p;
}

@vertex
fn vs_main(in: VIn) -> VOut {
    let model = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
    var pos = in.pos;
    // Displacement: push the surface out by the relief brightness.
    if (D.v[8].y != 0.0 && D.v[8].w > 0.5) {
        let duv = in.uv * D.v[2].z + D.v[3].xy;
        let h = lum(textureSampleLevel(t_relief, s_tex, duv, 0.0).rgb);
        pos = pos + normalize(in.normal) * h * D.v[8].y;
    }
    let world = model * vec4<f32>(glitch(pos, in.normal, in.inst.z), 1.0);
    var out: VOut;
    out.world = world.xyz;
    out.normal = normalize((model * vec4<f32>(in.normal, 0.0)).xyz);
    out.pos = G.view_proj * world;
    out.uv = in.uv;
    out.edge = in.edge;
    out.inst = in.inst;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let base_in = D.v[0].rgb;
    let metallic = D.v[0].w;
    let emissive_in = D.v[1].rgb;
    let rough = clamp(D.v[1].w, 0.02, 1.0);
    let mode = i32(D.v[2].x + 0.5);
    let has_tex = D.v[2].y > 0.5;
    let tex_scale = D.v[2].z;
    let flat_n = D.v[2].w > 0.5;
    let scroll = D.v[3].xy;
    let rim_k = D.v[3].z;
    let hue = D.v[3].w + in.inst.x;

    // Derivative-based values first (uniform control flow).
    let face_n = normalize(cross(dpdx(in.world), dpdy(in.world)));
    let edge_w = fwidth(in.edge) * 1.5 + 0.035;
    let uv = in.uv * tex_scale + scroll;
    let texel = textureSample(t_tex, s_tex, uv).rgb;
    let relief = textureSample(t_relief, s_tex, uv).rgb;
    let height = lum(relief);
    let dhx = dpdx(height);
    let dhy = dpdy(height);
    let dpx = dpdx(in.world);
    let dpy = dpdy(in.world);
    let duvx = dpdx(uv);
    let duvy = dpdy(uv);

    if (!clip_visible(in.world)) {
        discard;
    }

    let v = normalize(G.cam_pos.xyz - in.world);
    var n = normalize(in.normal);
    if (flat_n) {
        n = face_n;
    }
    if (dot(n, v) < 0.0) {
        n = -n;
    }
    let bump = D.v[8].x;
    if (bump != 0.0 && D.v[8].w > 0.5) {
        if (D.v[8].z > 0.5) {
            // Normal map, in a tangent frame built from screen derivatives
            // (no precomputed tangents needed). Image rows run downwards,
            // so green points along -v.
            let dp2perp = cross(dpy, n);
            let dp1perp = cross(n, dpx);
            let t = dp2perp * duvx.x + dp1perp * duvy.x;
            let b = dp2perp * duvx.y + dp1perp * duvy.y;
            let inv = inverseSqrt(max(max(dot(t, t), dot(b, b)), 1e-20));
            let tn = relief * 2.0 - 1.0;
            n = normalize((t * tn.x - b * tn.y) * inv * bump + n * max(tn.z, 0.05));
        } else {
            // Bump: brightness is height (surface gradient method).
            let r1 = cross(dpy, n);
            let r2 = cross(n, dpx);
            let det = dot(dpx, r1);
            let grad = sign(det) * (dhx * r1 + dhy * r2);
            n = normalize(abs(det) * n - grad * bump * 0.1);
        }
    }

    var tex = vec3<f32>(1.0);
    if (has_tex) {
        tex = texel;
    }
    let base = hue_rotate(base_in * tex, hue);
    let l = normalize(G.light_dir.xyz);
    let ndl = max(dot(n, l), 0.0);
    let diffuse = G.light_color.rgb * G.ground.w * ndl;
    let ambient = mix(G.ground.rgb, G.sky.rgb, n.y * 0.5 + 0.5) * G.sky.w;
    let h = normalize(l + v);
    let shin = mix(512.0, 8.0, rough);
    let spec = pow(max(dot(n, h), 0.0), shin) * (1.0 - rough) * G.ground.w;
    let ndv = max(dot(n, v), 0.0);
    let fres = pow(1.0 - ndv, 5.0);
    let f0 = mix(vec3<f32>(0.04), base, metallic);
    let fr = f0 + (vec3<f32>(1.0) - f0) * fres;
    let env = env_color(reflect(-v, n), rough);

    var col = base * (1.0 - metallic) * (diffuse + ambient);
    col = col + (spec * G.light_color.rgb + env * (1.0 - rough * 0.6)) * fr;
    col = col + G.sky.rgb * rim_k * pow(1.0 - ndv, 3.0) * 0.6;

    var mask = 1.0;
    switch mode {
        case 1: {
            mask = 1.0 - smoothstep(0.0, edge_w, in.edge);
        }
        case 2: {
            let s = fract(uv.y * 6.0);
            mask = smoothstep(0.35, 0.45, s) * (1.0 - smoothstep(0.55, 0.65, s));
        }
        case 3: {
            let lum = dot(texel, vec3<f32>(0.299, 0.587, 0.114));
            mask = lum * lum;
        }
        case 4: {
            // Light pulses running along U (neon ribbons), each with a tail.
            let pulses = max(D.v[6].x, 0.0);
            let tail = max(D.v[6].z * pulses, 1e-3);
            let d = fract(D.v[6].y - in.uv.x * pulses);
            let p = pow(max(1.0 - d / tail, 0.0), 2.0) * select(0.0, 1.0, pulses > 0.0);
            mask = D.v[7].x + p * D.v[6].w;
        }
        default: {}
    }
    let emissive = hue_rotate(emissive_in, hue) * mask * in.inst.y;
    col = col + emissive;

    let dist = length(G.cam_pos.xyz - in.world);
    return vec4<f32>(apply_fog(col, dist), 1.0);
}
