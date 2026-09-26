// Big swarms on the GPU: one thread per copy writes its instance (model
// matrix + hue, glow, rand, along) straight into a vertex buffer. A line
// for line port of `swarm_local` and the variation in ez_core's eval.rs,
// so the CPU fallback (WebGL2) shows the same swarm.

const TAU: f32 = 6.28318530718;

struct Params {
    // Symmetry copy * layer frame.
    frame: mat4x4<f32>,
    // xyz: layer scale
    size: vec4<f32>,
    // radius, spread, speed (turns per loop), loop phase
    shape: vec4<f32>,
    // form, count, seed, first output index
    ints: vec4<u32>,
    // variation: rotation (radians), spin (turns), scale, ripple
    var_a: vec4<f32>,
    // variation: ripple cycles, ripple spread, chase, hue
    var_b: vec4<f32>,
    // variation: seed, spectrum on (0/1), _, _
    var_c: vec4<u32>,
    // variation: spectrum amount, _, _, _
    var_d: vec4<f32>,
    // 16 spectrum bands
    bands: array<vec4<f32>, 4>,
};

struct Inst {
    m0: vec4<f32>,
    m1: vec4<f32>,
    m2: vec4<f32>,
    m3: vec4<f32>,
    inst: vec4<f32>,
};

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read_write> out_inst: array<Inst>;

fn hu(x_in: u32) -> u32 {
    var x = x_in;
    x = x ^ (x >> 16u);
    x = x * 0x7feb352du;
    x = x ^ (x >> 15u);
    x = x * 0x846ca68bu;
    x = x ^ (x >> 16u);
    return x;
}

fn hf(x: u32) -> f32 {
    return f32(hu(x) >> 8u) / 16777216.0;
}

fn swarm_hash(seed: u32, i: u32, k: u32) -> f32 {
    let key = hu((i * 0x9e3779b9u) ^ hu(seed + k * 0x85ebca6bu));
    return f32(key >> 8u) / 16777216.0;
}

fn hash_dir(u: f32, v: f32) -> vec3<f32> {
    let z = u * 2.0 - 1.0;
    let a = v * TAU;
    let r = sqrt(max(1.0 - z * z, 0.0));
    return vec3<f32>(r * cos(a), z, r * sin(a));
}

// Rotation of `angle` around a unit `axis`.
fn axis_angle(axis: vec3<f32>, angle: f32) -> mat3x3<f32> {
    let c = cos(angle);
    let s = sin(angle);
    let t = 1.0 - c;
    let x = axis.x;
    let y = axis.y;
    let z = axis.z;
    return mat3x3<f32>(
        vec3<f32>(t * x * x + c, t * x * y + s * z, t * x * z - s * y),
        vec3<f32>(t * x * y - s * z, t * y * y + c, t * y * z + s * x),
        vec3<f32>(t * x * z + s * y, t * y * z - s * x, t * z * z + c),
    );
}

fn rot_y(a: f32) -> mat3x3<f32> {
    return axis_angle(vec3<f32>(0.0, 1.0, 0.0), a);
}

fn to4(r: mat3x3<f32>, t: vec3<f32>) -> mat4x4<f32> {
    return mat4x4<f32>(vec4<f32>(r[0], 0.0), vec4<f32>(r[1], 0.0), vec4<f32>(r[2], 0.0), vec4<f32>(t, 1.0));
}

fn swarm_local(i: u32) -> mat4x4<f32> {
    let seed = P.ints.z;
    let form = P.ints.x;
    let radius = P.shape.x;
    let spread = P.shape.y;
    let speed = P.shape.z;
    let phase = P.shape.w;
    let tumble_axis = hash_dir(swarm_hash(seed, i, 7u), swarm_hash(seed, i, 8u));
    let tumble_sign = select(1.0, -1.0, swarm_hash(seed, i, 9u) < 0.5);
    let tumble = axis_angle(tumble_axis, TAU * phase * tumble_sign);
    var pos = vec3<f32>(0.0);
    switch form {
        case 0u: {
            let tilt = axis_angle(hash_dir(swarm_hash(seed, i, 0u), swarm_hash(seed, i, 1u)), swarm_hash(seed, i, 2u) * 0.6);
            let r = max(radius + spread * (swarm_hash(seed, i, 3u) * 2.0 - 1.0), 0.0);
            let y = spread * 0.5 * (swarm_hash(seed, i, 4u) * 2.0 - 1.0);
            let fast = select(1.0, 2.0, swarm_hash(seed, i, 5u) < 0.3);
            let a = TAU * (swarm_hash(seed, i, 6u) + phase * speed * fast);
            pos = tilt * vec3<f32>(cos(a) * r, y, sin(a) * r);
        }
        case 1u, 2u: {
            var r = radius + spread * 0.2 * (swarm_hash(seed, i, 2u) * 2.0 - 1.0);
            if (form == 1u) {
                r = radius * pow(swarm_hash(seed, i, 2u), 1.0 / 3.0);
            }
            let p = hash_dir(swarm_hash(seed, i, 0u), swarm_hash(seed, i, 1u)) * r;
            let a = TAU * phase * speed;
            let bob_k = 1.0 + floor(swarm_hash(seed, i, 5u) * 3.0);
            let bob = spread * 0.3 * sin(TAU * (phase * bob_k + swarm_hash(seed, i, 4u)));
            pos = rot_y(a) * p + vec3<f32>(0.0, bob, 0.0);
        }
        default: {
            let arm = floor(swarm_hash(seed, i, 0u) * 3.0);
            let u = swarm_hash(seed, i, 1u);
            let r = radius * max(sqrt(u), 0.05);
            let band = 1.0 + min(floor((1.0 - u) * 3.0), 2.0);
            let a = arm * TAU / 3.0 + (r / max(radius, 1e-3)) * 4.0 + (swarm_hash(seed, i, 2u) - 0.5) * 1.1
                + TAU * phase * speed * band;
            let y = spread * 0.25 * (swarm_hash(seed, i, 3u) * 2.0 - 1.0) * (1.0 - u * 0.7);
            pos = vec3<f32>(cos(a) * r, y, sin(a) * r);
        }
    }
    return to4(tumble, pos);
}

fn band_for(i: u32, n: u32) -> f32 {
    let x = (f32(i) + 0.5) / f32(n) * 16.0 - 0.5;
    let a = u32(clamp(floor(x), 0.0, 15.0));
    let b = min(a + 1u, 15u);
    let t = clamp(x - f32(a), 0.0, 1.0);
    return P.bands[a / 4u][a % 4u] * (1.0 - t) + P.bands[b / 4u][b % 4u] * t;
}

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    let n = P.ints.y;
    if (i >= n) {
        return;
    }
    let local = swarm_local(i);
    let phase = P.shape.w;
    // Variation (eval.rs copies_with).
    let vs = hu((P.var_c.x * 0x9e3779b9u) ^ i);
    let frac = f32(i) / f32(max(n, 1u));
    var v = mat4x4<f32>(vec4<f32>(1.0, 0.0, 0.0, 0.0), vec4<f32>(0.0, 1.0, 0.0, 0.0), vec4<f32>(0.0, 0.0, 1.0, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0));
    let rot = P.var_a.x;
    if (rot != 0.0) {
        let ay = (hf((vs * 0x9e3779b9u) ^ hu(1u)) * 2.0 - 1.0) * rot;
        let ax = (hf((vs * 0x9e3779b9u) ^ hu(2u)) * 2.0 - 1.0) * rot;
        let az = (hf((vs * 0x9e3779b9u) ^ hu(3u)) * 2.0 - 1.0) * rot;
        let e = axis_angle(vec3<f32>(0.0, 1.0, 0.0), ay) * axis_angle(vec3<f32>(1.0, 0.0, 0.0), ax) * axis_angle(vec3<f32>(0.0, 0.0, 1.0), az);
        v = v * to4(e, vec3<f32>(0.0));
    }
    let spin = i32(P.var_a.y);
    if (spin > 0) {
        let turns = 1 + (i32(hf((vs * 0x9e3779b9u) ^ hu(4u)) * f32(spin)) % spin);
        let sign = select(1.0, -1.0, hf((vs * 0x9e3779b9u) ^ hu(5u)) < 0.5);
        var axis = vec3<f32>(
            hf((vs * 0x9e3779b9u) ^ hu(6u)) - 0.5,
            hf((vs * 0x9e3779b9u) ^ hu(7u)) - 0.5,
            hf((vs * 0x9e3779b9u) ^ hu(8u)) - 0.5,
        );
        axis = select(vec3<f32>(0.0, 1.0, 0.0), normalize(axis), dot(axis, axis) > 1e-12);
        v = v * to4(axis_angle(axis, phase * sign * f32(turns) * TAU), vec3<f32>(0.0));
    }
    var s = 1.0 + P.var_a.z * (hf((vs * 0x9e3779b9u) ^ hu(9u)) * 2.0 - 1.0);
    let wave_x = phase * P.var_b.x - frac * P.var_b.y;
    if (P.var_a.w != 0.0) {
        s = s * (1.0 + P.var_a.w * sin(wave_x * TAU));
    }
    var band = 0.0;
    if (P.var_c.y != 0u) {
        band = band_for(i, n);
    }
    s = max(s, 0.02);
    v = v * mat4x4<f32>(vec4<f32>(s, 0.0, 0.0, 0.0), vec4<f32>(0.0, s, 0.0, 0.0), vec4<f32>(0.0, 0.0, s, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0));
    if (band != 0.0) {
        let k = max(1.0 + P.var_d.x * band, 0.02);
        v = v * mat4x4<f32>(vec4<f32>(1.0, 0.0, 0.0, 0.0), vec4<f32>(0.0, k, 0.0, 0.0), vec4<f32>(0.0, 0.0, 1.0, 0.0), vec4<f32>(0.0, (k - 1.0) * 0.5, 0.0, 1.0));
    }
    var glow = 1.0;
    if (P.var_b.z != 0.0) {
        let w = 0.5 + 0.5 * sin(wave_x * TAU);
        glow = 1.0 + P.var_b.z * (pow(w, 6.0) * 3.0 - 0.5);
    }
    glow = glow + abs(P.var_d.x) * band * 2.0;
    var hue = 0.0;
    if (P.var_b.w != 0.0) {
        hue = P.var_b.w * (hf((vs * 0x9e3779b9u) ^ hu(10u)) - 0.5);
    }
    let size = mat4x4<f32>(vec4<f32>(P.size.x, 0.0, 0.0, 0.0), vec4<f32>(0.0, P.size.y, 0.0, 0.0), vec4<f32>(0.0, 0.0, P.size.z, 0.0), vec4<f32>(0.0, 0.0, 0.0, 1.0));
    let m = P.frame * local * v * size;
    var o: Inst;
    o.m0 = m[0];
    o.m1 = m[1];
    o.m2 = m[2];
    o.m3 = m[3];
    o.inst = vec4<f32>(hue, max(glow, 0.0), hf((vs * 0x9e3779b9u) ^ hu(11u)), frac);
    out_inst[P.ints.w + i] = o;
}
