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
// D.v[9]: deform: twist (turns bottom to top), bend (radians), taper, wobble
// D.v[10]: deform: wobble scale, wobble angle (loop-safe), explode, reach (0 = no deform)
// D.v[11..14]: colour ramp colours (rgb); D.v[11].w colour count, D.v[12].w glow strength
// D.v[15]: ramp on, mode (0 gradient, 1 steps), shift along the copies (0..1), colour the glow

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

fn rot_xz(v: vec3<f32>, a: f32) -> vec3<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec3<f32>(c * v.x - s * v.z, v.y, s * v.x + c * v.z);
}

struct Deformed {
    pos: vec3<f32>,
    normal: vec3<f32>,
};

// Twist, bend, taper, wobble and explode in object space (the shape fits a
// unit sphere; y runs from its bottom to its top).
fn deform(pos_in: vec3<f32>, n_in: vec3<f32>) -> Deformed {
    var out: Deformed;
    out.pos = pos_in;
    out.normal = n_in;
    if (D.v[10].w <= 0.0) {
        return out;
    }
    var p = pos_in;
    var n = n_in;
    // Taper: scale across by height.
    let taper = D.v[9].z;
    if (taper != 0.0) {
        let k = max(1.0 + taper * p.y, 0.02);
        p = vec3<f32>(p.x * k, p.y, p.z * k);
        n = normalize(vec3<f32>(n.x, n.y * k - taper * dot(n.xz, p.xz) / k, n.z));
    }
    // Twist: turn around y, more further up.
    let twist = D.v[9].x;
    if (twist != 0.0) {
        let a = twist * PI * p.y;
        p = rot_xz(p, a);
        n = rot_xz(n, a);
    }
    // Bend: curve the y axis into an arc in the x-y plane.
    let bend = D.v[9].y;
    if (abs(bend) > 1e-4) {
        let k = bend * 0.5;
        let r = 1.0 / k;
        let phi = p.y * k;
        let c = cos(phi);
        let s = sin(phi);
        let x = r - (r - p.x) * c;
        let y = (r - p.x) * s;
        p = vec3<f32>(x, y, p.z);
        n = vec3<f32>(c * n.x - s * n.y, s * n.x + c * n.y, n.z);
    }
    // Wobble: bumps along the normal that flow around once per cycle.
    let wobble = D.v[9].w;
    if (wobble != 0.0) {
        let a = D.v[10].y;
        let q = pos_in * D.v[10].x + vec3<f32>(cos(a), sin(a), 0.0) * 1.5;
        p = p + normalize(n) * (vnoise3(q) * 2.0 - 1.0) * wobble;
    }
    // Explode: faces (flat shading) or vertices fly out along their normal.
    let explode = D.v[10].z;
    if (explode != 0.0) {
        let r = hash_v3(n_in, 0x2545f491u);
        p = p + normalize(n) * (r.x + 0.75) * explode;
    }
    out.pos = p;
    out.normal = n;
    return out;
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
    let d = deform(pos, in.normal);
    let world = model * vec4<f32>(glitch(d.pos, d.normal, in.inst.z), 1.0);
    var out: VOut;
    out.world = world.xyz;
    out.normal = normalize((model * vec4<f32>(d.normal, 0.0)).xyz);
    out.pos = G.view_proj * world;
    out.uv = in.uv;
    out.edge = in.edge;
    out.inst = in.inst;
    return out;
}

// Colour of the ramp at t (0..1, wrapping back to the first colour).
fn ramp_color(t_in: f32) -> vec3<f32> {
    let n = max(i32(D.v[11].w + 0.5), 1);
    let t = fract(t_in);
    if (D.v[15].y > 0.5) {
        let k = min(i32(floor(t * f32(n))), n - 1);
        return D.v[11 + k].rgb;
    }
    let x = t * f32(n);
    let k = min(i32(floor(x)), n - 1);
    let a = D.v[11 + k].rgb;
    let b = D.v[11 + (k + 1) % n].rgb;
    let f = fract(x);
    return mix(a, b, f * f * (3.0 - 2.0 * f));
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    var base_in = D.v[0].rgb;
    var metallic = D.v[0].w;
    var emissive_in = D.v[1].rgb;
    if (D.v[15].x > 0.5) {
        let rc = ramp_color(in.inst.w + D.v[15].z);
        base_in = rc;
        if (D.v[15].w > 0.5) {
            emissive_in = rc * D.v[12].w;
        }
    }
    var rough = clamp(D.v[1].w, 0.02, 1.0);
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
    var base = hue_rotate(base_in * tex, hue);
    var col = lit_surface(base, metallic, rough, n, in.world, v, rim_k, 1.0);

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

    return vec4<f32>(apply_fog_at(col, in.world), 1.0);
}

// Distance to the camera, for depth of field (a small extra pass).
@fragment
fn fs_depth(in: VOut) -> @location(0) vec4<f32> {
    if (!clip_visible(in.world)) {
        discard;
    }
    return vec4<f32>(length(in.world - G.cam_pos.xyz), 0.0, 0.0, 1.0);
}
