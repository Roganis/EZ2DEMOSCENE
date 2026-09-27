// Raymarched distance-field objects, drawn inside a box proxy (-1..1): the
// back faces of the box start a ray that marches through the object's own
// space, and the hit writes its real depth, so the object meets meshes,
// terrain and floors correctly and casts sun shadows.
// Material as in mesh.wgsl: D.v[0..3] and the colour ramp D.v[11..15].
// D.v[4]: shape (0 metaballs, 1 gyroid, 2 bulb, 3 melting box), settings a, b, c
// D.v[5]: motion angle (0..2π, whole cycles per loop), motion amount, _, _

@group(2) @binding(0) var t_tex: texture_2d<f32>;
@group(2) @binding(1) var s_tex: sampler;
@group(2) @binding(2) var t_relief: texture_2d<f32>;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) edge: f32,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    // hue, glow, rand, position along the copies
    @location(8) inst: vec4<f32>,
};

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Rows of the inverse model matrix: object = (dot(r.xyz, world) + r.w).
    @location(2) @interpolate(flat) r0: vec4<f32>,
    @location(3) @interpolate(flat) r1: vec4<f32>,
    @location(4) @interpolate(flat) r2: vec4<f32>,
    @location(5) @interpolate(flat) inst: vec4<f32>,
};

@vertex
fn vs_main(in: VIn) -> VOut {
    let model = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
    let world = model * vec4<f32>(in.pos, 1.0);
    // Inverse of the 3x3 part: rows are the cross products of its columns.
    let a = in.m0.xyz;
    let b = in.m1.xyz;
    let c = in.m2.xyz;
    let det = dot(a, cross(b, c));
    let inv = 1.0 / select(det, 1e-8, abs(det) < 1e-12);
    let r0 = cross(b, c) * inv;
    let r1 = cross(c, a) * inv;
    let r2 = cross(a, b) * inv;
    let t = in.m3.xyz;
    var out: VOut;
    out.pos = G.view_proj * world;
    out.world = world.xyz;
    out.normal = (model * vec4<f32>(in.normal, 0.0)).xyz;
    out.r0 = vec4<f32>(r0, -dot(r0, t));
    out.r1 = vec4<f32>(r1, -dot(r1, t));
    out.r2 = vec4<f32>(r2, -dot(r2, t));
    out.inst = in.inst;
    return out;
}

fn smin(a: f32, b: f32, k: f32) -> f32 {
    if (k <= 0.0) {
        return min(a, b);
    }
    let h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
    return mix(b, a, h) - k * h * (1.0 - h);
}

fn sd_metaballs(p: vec3<f32>, a: f32, seed: u32) -> f32 {
    let n = i32(D.v[4].y + 0.5);
    let k = D.v[4].z;
    var d = 1e9;
    for (var i = 0; i < 8; i = i + 1) {
        if (i >= n) {
            break;
        }
        let h = hash_u((u32(i) * 0x9e3779b9u) ^ seed);
        // Whole frequencies keep every orbit closed over the loop.
        let f = vec3<f32>(f32(1u + (h & 1u)), f32(1u + ((h >> 1u) & 1u)), f32(1u + ((h >> 2u) & 1u)));
        let ph = vec3<f32>(hash1(h ^ 0x1234567u), hash1(h ^ 0x7654321u), hash1(h ^ 0x0badf00du)) * TAU;
        let c = vec3<f32>(sin(a * f.x + ph.x), sin(a * f.y + ph.y) * 0.8, cos(a * f.z + ph.z)) * 0.45;
        let r = 0.24 + 0.1 * hash1(h ^ 0x5bd1e995u);
        d = smin(d, length(p - c) - r, k);
    }
    return d;
}

fn sd_gyroid(p: vec3<f32>, a: f32) -> f32 {
    let s = D.v[4].y;
    // The lattice flows through the ball by whole periods per loop.
    let q = p * s + vec3<f32>(0.0, a, 0.0);
    let g = dot(sin(q), cos(q.yzx));
    let shell = abs(g) / (s * 1.8) - D.v[4].z * 0.5;
    return max(length(p) - 0.95, shell);
}

fn sd_bulb(p_in: vec3<f32>, a: f32) -> f32 {
    let power = D.v[4].y + sin(a) * D.v[5].y;
    let q = p_in * 1.25;
    var z = q;
    var dr = 1.0;
    var r = length(z);
    for (var i = 0; i < 6; i = i + 1) {
        r = max(length(z), 1e-6);
        if (r > 2.0) {
            break;
        }
        let theta = acos(clamp(z.z / r, -1.0, 1.0)) * power;
        let phi = atan2(z.y, z.x) * power;
        dr = pow(r, power - 1.0) * power * dr + 1.0;
        let zr = pow(r, power);
        z = zr * vec3<f32>(sin(theta) * cos(phi), sin(phi) * sin(theta), cos(theta)) + q;
    }
    r = max(length(z), 1e-6);
    return 0.5 * log(r) * r / dr / 1.25;
}

fn sd_round_box(p: vec3<f32>, b: vec3<f32>, r: f32) -> f32 {
    let q = abs(p) - b + r;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0) - r;
}

fn sd_soft_box(p: vec3<f32>, a: f32) -> f32 {
    let bx = sd_round_box(p, vec3<f32>(0.46), D.v[4].z);
    // Two balls on opposite sides orbit through the box's faces.
    let c = vec3<f32>(cos(a), sin(a * 2.0) * 0.3, sin(a)) * 0.72;
    let balls = min(length(p - c), length(p + c)) - 0.28;
    return smin(bx, balls, D.v[4].y);
}

fn map(p: vec3<f32>, seed: u32) -> f32 {
    let a = D.v[5].x;
    switch i32(D.v[4].x + 0.5) {
        case 1: {
            return sd_gyroid(p, a);
        }
        case 2: {
            return sd_bulb(p, a);
        }
        case 3: {
            return sd_soft_box(p, a);
        }
        default: {
            return sd_metaballs(p, a, seed);
        }
    }
}

fn to_object(r0: vec4<f32>, r1: vec4<f32>, r2: vec4<f32>, w: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(r0.xyz, w) + r0.w, dot(r1.xyz, w) + r1.w, dot(r2.xyz, w) + r2.w);
}

struct Hit {
    world: vec3<f32>,
    // Object-space point and normal.
    p: vec3<f32>,
    n: vec3<f32>,
    // Step count relative to the budget (for a little extra darkening).
    cost: f32,
};

// March the ray through this fragment of the box's back faces. Returns
// false (and the fragment is discarded) when it misses.
fn march(in: VOut, steps: i32, hit: ptr<function, Hit>) -> bool {
    // The ray from the camera (or the sun) through this pixel, in the
    // world, from the near plane: works for perspective, orthographic and
    // mirrored views alike.
    let clip = G.view_proj * vec4<f32>(in.world, 1.0);
    let ndc = clip.xy / clip.w;
    let n4 = G.inv_view_proj * vec4<f32>(ndc, 0.0, 1.0);
    let f4 = G.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    let near = n4.xyz / n4.w;
    let dir = normalize(f4.xyz / f4.w - near);
    // Only the far side of the box does the work (either winding).
    if (dot(dir, in.normal) < 0.0) {
        return false;
    }
    let ro = to_object(in.r0, in.r1, in.r2, near);
    let rd_raw = vec3<f32>(dot(in.r0.xyz, dir), dot(in.r1.xyz, dir), dot(in.r2.xyz, dir));
    let scale = max(length(rd_raw), 1e-6);
    let rd = rd_raw / scale;
    // Where the ray is inside the box.
    let inv_rd = 1.0 / select(rd, vec3<f32>(1e-6), abs(rd) < vec3<f32>(1e-6));
    let ta = (vec3<f32>(-1.02) - ro) * inv_rd;
    let tb = (vec3<f32>(1.02) - ro) * inv_rd;
    let t_in = max(max(min(ta.x, tb.x), min(ta.y, tb.y)), min(ta.z, tb.z));
    let t_out = min(min(max(ta.x, tb.x), max(ta.y, tb.y)), max(ta.z, tb.z));
    var t = max(t_in, 0.0);
    let seed = u32(in.inst.z * 65536.0);
    var i = 0;
    var found = false;
    loop {
        if (i >= steps || t > t_out) {
            break;
        }
        let p = ro + rd * t;
        let d = map(p, seed);
        if (d < 0.0008 + 0.0015 * t) {
            found = true;
            break;
        }
        t = t + d * 0.9;
        i = i + 1;
    }
    if (!found) {
        return false;
    }
    let p = ro + rd * t;
    // Normal from four samples (tetrahedron).
    let e = 0.0015;
    let k0 = vec3<f32>(1.0, -1.0, -1.0);
    let k1 = vec3<f32>(-1.0, -1.0, 1.0);
    let k2 = vec3<f32>(-1.0, 1.0, -1.0);
    let k3 = vec3<f32>(1.0, 1.0, 1.0);
    let n = k0 * map(p + k0 * e, seed) + k1 * map(p + k1 * e, seed) + k2 * map(p + k2 * e, seed) + k3 * map(p + k3 * e, seed);
    (*hit).world = near + dir * (t / scale);
    (*hit).p = p;
    (*hit).n = normalize(n + vec3<f32>(0.0, 1e-7, 0.0));
    (*hit).cost = f32(i) / f32(steps);
    return true;
}

fn depth_of(world: vec3<f32>) -> f32 {
    let c = G.view_proj * vec4<f32>(world, 1.0);
    return clamp(c.z / c.w, 0.0, 1.0);
}

// Colour of the ramp at t (as in mesh.wgsl).
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

struct FOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_main(in: VOut) -> FOut {
    var h: Hit;
    if (!march(in, 96, &h)) {
        discard;
    }
    if (!clip_visible(h.world)) {
        discard;
    }
    var base = D.v[0].rgb;
    var emissive = D.v[1].rgb;
    if (D.v[15].x > 0.5) {
        let rc = ramp_color(in.inst.w + D.v[15].z);
        base = rc;
        if (D.v[15].w > 0.5) {
            emissive = rc * D.v[12].w;
        }
    }
    let hue = D.v[3].w + in.inst.x;
    base = hue_rotate(base, hue);
    // World normal: the inverse-transpose of the model matrix.
    var n = normalize(in.r0.xyz * h.n.x + in.r1.xyz * h.n.y + in.r2.xyz * h.n.z);
    let v = normalize(G.cam_pos.xyz - h.world);
    // Ambient occlusion from a few steps out along the normal.
    let seed = u32(in.inst.z * 65536.0);
    var occ = 0.0;
    for (var k = 1; k <= 4; k = k + 1) {
        let s = f32(k) * 0.06;
        occ = occ + (s - map(h.p + h.n * s, seed)) / f32(k);
    }
    let ao = clamp(1.0 - occ * 2.5, 0.25, 1.0) * (1.0 - 0.3 * h.cost);
    var col = lit_surface(base, D.v[0].w, clamp(D.v[1].w, 0.02, 1.0), n, h.world, v, D.v[3].z, ao);
    // Glow: solid, or on the creases for the other emissive modes.
    var mask = 1.0;
    if (i32(D.v[2].x + 0.5) != 0) {
        mask = 1.0 - ao;
    }
    col = col + hue_rotate(emissive, hue) * mask * in.inst.y;
    var out: FOut;
    out.color = vec4<f32>(apply_fog_at(col, h.world), 1.0);
    out.depth = depth_of(h.world);
    return out;
}

// Distance to the camera, for depth of field.
@fragment
fn fs_depth(in: VOut) -> FOut {
    var h: Hit;
    if (!march(in, 64, &h)) {
        discard;
    }
    if (!clip_visible(h.world)) {
        discard;
    }
    var out: FOut;
    out.color = vec4<f32>(length(h.world - G.cam_pos.xyz), 0.0, 0.0, 1.0);
    out.depth = depth_of(h.world);
    return out;
}

// Seen from the sun: depth only.
@fragment
fn fs_shadow(in: VOut) -> @builtin(frag_depth) f32 {
    var h: Hit;
    if (!march(in, 64, &h)) {
        discard;
    }
    // frag_depth skips the pipeline's depth bias: add a little here.
    return min(depth_of(h.world) + 0.0015, 1.0);
}
