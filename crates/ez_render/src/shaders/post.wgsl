// Post-processing passes (fullscreen). Each pass reads its parameters from
// a dynamic-offset uniform block.

struct Post {
    v: array<vec4<f32>, 32>,
};

@group(0) @binding(0) var<uniform> P: Post;
@group(0) @binding(1) var t_a: texture_2d<f32>;
@group(0) @binding(2) var t_b: texture_2d<f32>;
@group(0) @binding(3) var s_lin: sampler;

const TAU: f32 = 6.28318530718;
const PI: f32 = 3.14159265359;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    var out: VOut;
    out.pos = vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, 1.0 - y);
    return out;
}

fn hash_u(x_in: u32) -> u32 {
    var x = x_in;
    x = x ^ (x >> 16u);
    x = x * 0x7feb352du;
    x = x ^ (x >> 15u);
    x = x * 0x846ca68bu;
    x = x ^ (x >> 16u);
    return x;
}

fn hash1(x: u32) -> f32 {
    return f32(hash_u(x) >> 8u) / 16777216.0;
}

// --- separable gaussian blur ---------------------------------------------
// P.v[0]: texel step (xy, already scaled by radius)
@fragment
fn fs_blur(in: VOut) -> @location(0) vec4<f32> {
    let stp = P.v[0].xy;
    // Unrolled on purpose: D3D's FXC rejects dynamically indexed local arrays.
    var c = textureSampleLevel(t_a, s_lin, in.uv, 0.0).rgb * 0.227027;
    c = c + blur_tap(in.uv, stp, 1.0, 0.1945946);
    c = c + blur_tap(in.uv, stp, 2.0, 0.1216216);
    c = c + blur_tap(in.uv, stp, 3.0, 0.054054);
    c = c + blur_tap(in.uv, stp, 4.0, 0.016216);
    return vec4<f32>(c, 1.0);
}

fn blur_tap(uv: vec2<f32>, stp: vec2<f32>, k: f32, w: f32) -> vec3<f32> {
    let o = stp * k;
    return (textureSampleLevel(t_a, s_lin, uv + o, 0.0).rgb + textureSampleLevel(t_a, s_lin, uv - o, 0.0).rgb) * w;
}

// --- kaleidoscope / mirror split -----------------------------------------
// P.v[0]: kaleido on, segments, angle (rad), zoom
// P.v[1]: centre xy, aspect, mirror mode (0 off, 1 LR, 2 TB, 3 quad)
@fragment
fn fs_warp(in: VOut) -> @location(0) vec4<f32> {
    var uv = in.uv;
    let mode = i32(P.v[1].w + 0.5);
    if (mode == 1 || mode == 3) {
        uv.x = 0.5 - abs(uv.x - 0.5);
    }
    if (mode == 2 || mode == 3) {
        uv.y = 0.5 - abs(uv.y - 0.5);
    }
    if (P.v[0].x > 0.5) {
        let aspect = P.v[1].z;
        let c = P.v[1].xy;
        let seg = TAU / max(P.v[0].y, 1.0);
        var p = (uv - c) * vec2<f32>(aspect, 1.0);
        let r = length(p) / max(P.v[0].w, 0.01);
        var a = atan2(p.y, p.x) + P.v[0].z;
        a = a - floor(a / seg) * seg;
        a = abs(a - seg * 0.5);
        p = vec2<f32>(cos(a), sin(a)) * r;
        uv = c + p / vec2<f32>(aspect, 1.0);
        // mirrored repeat
        uv = 1.0 - abs(1.0 - (uv - 2.0 * floor(uv * 0.5)));
    }
    return vec4<f32>(textureSampleLevel(t_a, s_lin, uv, 0.0).rgb, 1.0);
}

// --- bloom (dual filter) ---------------------------------------------------
// P.v[0]: texel size of the SOURCE (xy), threshold, prefilter flag
// P.v[1].x: upsample weight
@fragment
fn fs_bloom_down(in: VOut) -> @location(0) vec4<f32> {
    let t = P.v[0].xy;
    var c = textureSampleLevel(t_a, s_lin, in.uv, 0.0).rgb * 4.0;
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(-t.x, -t.y), 0.0).rgb;
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(t.x, -t.y), 0.0).rgb;
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(-t.x, t.y), 0.0).rgb;
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(t.x, t.y), 0.0).rgb;
    c = c / 8.0;
    if (P.v[0].w > 0.5) {
        let thr = P.v[0].z;
        let lum = max(max(c.r, c.g), c.b);
        let soft = clamp(lum - thr * 0.5, 0.0, thr) ;
        let contrib = max(soft * soft / (4.0 * thr + 1e-4), lum - thr) / max(lum, 1e-4);
        c = c * max(contrib, 0.0);
        c = min(c, vec3<f32>(64.0));
    }
    return vec4<f32>(c, 1.0);
}

@fragment
fn fs_bloom_up(in: VOut) -> @location(0) vec4<f32> {
    let t = P.v[0].xy;
    var c = vec3<f32>(0.0);
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(-t.x * 2.0, 0.0), 0.0).rgb;
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(t.x * 2.0, 0.0), 0.0).rgb;
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(0.0, -t.y * 2.0), 0.0).rgb;
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(0.0, t.y * 2.0), 0.0).rgb;
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(-t.x, -t.y), 0.0).rgb * 2.0;
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(t.x, -t.y), 0.0).rgb * 2.0;
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(-t.x, t.y), 0.0).rgb * 2.0;
    c = c + textureSampleLevel(t_a, s_lin, in.uv + vec2<f32>(t.x, t.y), 0.0).rgb * 2.0;
    return vec4<f32>(c / 12.0 * P.v[1].x, 1.0);
}

// --- final composite ---------------------------------------------------------
// P.v[0]: res w, h, 1/w, 1/h
// P.v[1]: exposure, contrast, saturation, vignette
// P.v[2]: grain, beat flash, chroma amount, bloom intensity
// P.v[3]: pixel size (0 = off), palette count (0 off, -1 VGA cube), dither, phase
// P.v[4]: crt on, scanlines, curvature, noise
// P.v[5]: beat fraction, frame id, beats per loop, _
// P.v[8..24]: palette colours (sRGB)

fn aces(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// 4x4 ordered-dither threshold (the classic Bayer matrix
// 0 8 2 10 / 12 4 14 6 / 3 11 1 9 / 15 7 13 5), computed by bit interleaving
// because D3D's FXC rejects dynamically indexed local arrays.
fn bayer4(p: vec2<u32>) -> f32 {
    let x = p.x & 3u;
    let y = p.y & 3u;
    let e = x ^ y;
    let v = ((e & 1u) << 3u) | ((y & 1u) << 2u) | (((e >> 1u) & 1u) << 1u) | ((y >> 1u) & 1u);
    return (f32(v) + 0.5) / 16.0 - 0.5;
}

fn sample_scene(uv: vec2<f32>, bloom_k: f32) -> vec3<f32> {
    return textureSampleLevel(t_a, s_lin, uv, 0.0).rgb + textureSampleLevel(t_b, s_lin, uv, 0.0).rgb * bloom_k;
}

@fragment
fn fs_final(in: VOut) -> @location(0) vec4<f32> {
    let res = P.v[0].xy;
    var uv = in.uv;
    let crt = P.v[4].x > 0.5;
    var outside = false;
    if (crt && P.v[4].z > 0.0) {
        let cc = uv * 2.0 - 1.0;
        let k = P.v[4].z * 0.25;
        let warped = cc * (1.0 + k * dot(cc, cc) * vec2<f32>(0.6, 0.9));
        uv = warped * 0.5 + 0.5;
        outside = any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0));
    }
    let frame_id = u32(P.v[5].y);
    if (crt && P.v[4].w > 0.0) {
        let line = u32(uv.y * res.y / 2.0);
        let jitter = hash1((line * 747796405u) ^ (frame_id * 2891336453u)) - 0.5;
        let roll = sin(TAU * (uv.y * 3.0 + P.v[3].w * 2.0)) * 0.5 + 0.5;
        uv.x = uv.x + jitter * P.v[4].w * 0.006 * (0.5 + roll);
    }
    let pix = P.v[3].x;
    var pcoord = vec2<u32>(in.pos.xy);
    if (pix > 1.0) {
        let block = floor(uv * res / pix);
        uv = (block + 0.5) * pix / res;
        pcoord = vec2<u32>(block);
    }
    let bloom_k = P.v[2].w;
    var col: vec3<f32>;
    let ca = P.v[2].z;
    if (ca > 0.0) {
        let dir = (uv - 0.5) * ca;
        col = vec3<f32>(
            sample_scene(uv + dir, bloom_k).r,
            sample_scene(uv, bloom_k).g,
            sample_scene(uv - dir, bloom_k).b,
        );
    } else {
        col = sample_scene(uv, bloom_k);
    }
    col = col * P.v[1].x;
    col = col + vec3<f32>(P.v[2].y * exp(-P.v[5].x * 8.0));
    col = aces(col);
    col = to_srgb(col);
    // contrast & saturation in display space
    col = (col - 0.5) * P.v[1].y + 0.5;
    let l = dot(col, vec3<f32>(0.299, 0.587, 0.114));
    col = mix(vec3<f32>(l), col, P.v[1].z);
    // vignette
    let d = in.uv - 0.5;
    col = col * (1.0 - P.v[1].w * dot(d, d) * 2.0);
    col = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));

    // palette reduction with ordered dither
    let count = i32(P.v[3].y);
    let dither = P.v[3].z * bayer4(pcoord);
    if (count == -1) {
        col = clamp(round((col + dither / 5.0) * 5.0) / 5.0, vec3<f32>(0.0), vec3<f32>(1.0));
    } else if (count > 0) {
        let spread = 1.0 / sqrt(f32(count));
        let p = clamp(col + vec3<f32>(dither * spread), vec3<f32>(0.0), vec3<f32>(1.0));
        var best = P.v[8].rgb;
        var best_d = 1e9;
        for (var i = 0; i < count; i = i + 1) {
            let pc = P.v[8 + i].rgb;
            let e = p - pc;
            let dist = dot(e * e, vec3<f32>(0.3, 0.59, 0.11));
            if (dist < best_d) {
                best_d = dist;
                best = pc;
            }
        }
        col = best;
    }

    if (crt) {
        let s = 0.5 + 0.5 * cos(uv.y * res.y * PI / 1.5);
        col = col * (1.0 - P.v[4].y * 0.6 * s);
        // subtle RGB mask (a select, not `mask[m] = ...`: FXC can't store
        // through a runtime vector index)
        let m = u32(in.pos.x) % 3u;
        let boost = 1.0 + P.v[4].y * 0.25;
        let mask = select(vec3<f32>(1.0), vec3<f32>(boost), vec3<u32>(0u, 1u, 2u) == vec3<u32>(m));
        col = col * mask;
    }
    // grain (loop-safe: frame id wraps with the loop)
    let g = hash1(hash_u(u32(in.pos.x) + u32(in.pos.y) * 4099u) ^ (frame_id * 83492791u)) - 0.5;
    col = col + vec3<f32>(g * P.v[2].x);
    if (outside) {
        col = vec3<f32>(0.0);
    }
    // All grading above happens in display (sRGB) space; the output texture
    // is sRGB, so hand the hardware linear values and it encodes them back.
    return vec4<f32>(to_linear(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0))), 1.0);
}
