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
// Line wobble, as in retro RPG battles. P.v[4]: mode (1 wave, 2
// interlaced, 3 compression), amount (picture heights), waves, wave turn;
// P.v[5].x: lines for the interlacing (0 = pixel rows); P.v[1].z: aspect.
fn line_wobble(uv: vec2<f32>, pos: vec2<f32>) -> vec2<f32> {
    let mode = i32(P.v[4].x + 0.5);
    if (mode == 0) {
        return uv;
    }
    let w = P.v[4].y * sin(TAU * (P.v[4].z * uv.y + P.v[4].w));
    var out = uv;
    if (mode == 3) {
        out.y = out.y + w;
    } else {
        var dx = w / max(P.v[1].z, 0.01);
        if (mode == 2) {
            var line = u32(pos.y);
            if (P.v[5].x >= 1.0) {
                line = u32(uv.y * P.v[5].x);
            }
            if ((line & 1u) == 1u) {
                dx = -dx;
            }
        }
        out.x = out.x + dx;
    }
    return fold(out);
}

// Mirror at the edges rather than smearing them.
fn fold(uv: vec2<f32>) -> vec2<f32> {
    return 1.0 - abs(1.0 - (uv - 2.0 * floor(uv * 0.5)));
}

// Fisheye lens. P.v[5].y: amount (-1..1): above 0 the middle bulges and
// the corners stay put; below 0 the middle shrinks and the edges stretch.
fn lens(uv: vec2<f32>) -> vec2<f32> {
    let k = P.v[5].y;
    if (k == 0.0) {
        return uv;
    }
    let aspect = max(P.v[1].z, 0.01);
    let d = (uv - 0.5) * vec2<f32>(aspect, 1.0);
    let r = length(d);
    if (r < 1e-5) {
        return uv;
    }
    var nr = r;
    if (k > 0.0) {
        // tan: steeper towards the corners, which map onto themselves.
        let bind = length(vec2<f32>(aspect * 0.5, 0.5));
        let power = k * 0.49 * PI / bind;
        nr = tan(r * power) * bind / tan(bind * power);
        return fold(0.5 + d / r * nr / vec2<f32>(aspect, 1.0));
    }
    // atan: the middle shrinks and the edges stretch, the top and bottom
    // staying put; on a tall screen the sides can reach past the picture,
    // which is black there (see fs_warp).
    let power = -k * 4.0;
    nr = atan(r * power) * 0.5 / atan(0.5 * power);
    return 0.5 + d / r * nr / vec2<f32>(aspect, 1.0);
}

@fragment
fn fs_warp(in: VOut) -> @location(0) vec4<f32> {
    var uv = lens(line_wobble(in.uv, in.pos.xy));
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0))) {
        // Pinched past the picture's edge.
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
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
    let haze = P.v[2].x;
    if (haze > 0.0) {
        uv = uv + heat_haze(uv) * haze;
    }
    if (P.v[3].x > 0.5) {
        return vec4<f32>(dof(uv), 1.0);
    }
    return vec4<f32>(textureSampleLevel(t_a, s_lin, uv, 0.0).rgb, 1.0);
}

// Depth of field: t_b holds the distance to the camera (half resolution).
// P.v[3]: on, focus distance, largest blur radius (fraction of the height), _
fn dof_radius(uv: vec2<f32>) -> f32 {
    let d = textureSampleLevel(t_b, s_lin, uv, 0.0).r;
    let focus = max(P.v[3].y, 0.01);
    // Thin-lens-like: grows with |1 - focus / distance|.
    return clamp(abs(1.0 - focus / max(d, 0.01)), 0.0, 1.0) * P.v[3].z;
}

fn dof(uv: vec2<f32>) -> vec3<f32> {
    let aspect = max(P.v[1].z, 0.01);
    let r0 = dof_radius(uv);
    var sum = textureSampleLevel(t_a, s_lin, uv, 0.0).rgb;
    var total = 1.0;
    // 24 taps on a golden-angle spiral (a round bokeh disc).
    for (var i = 0; i < 24; i = i + 1) {
        let f = (f32(i) + 0.5) / 24.0;
        let a = f32(i) * 2.39996323;
        let off = vec2<f32>(cos(a) / aspect, sin(a)) * sqrt(f) * r0;
        let q = uv + off;
        // Sharp things in front don't smear into what is behind them:
        // a sample counts when its own blur reaches this far.
        let w = clamp(dof_radius(q) / max(length(off * vec2<f32>(aspect, 1.0)), 1e-4), 0.0, 1.0);
        sum = sum + textureSampleLevel(t_a, s_lin, q, 0.0).rgb * w;
        total = total + w;
    }
    return sum / total;
}

// Heat shimmer offset. P.v[2]: amount, scale, time angle (whole turns per
// loop), region (0 hot spots, 1 ground, 2 everywhere); P.v[1].z: aspect.
fn heat_haze(uv: vec2<f32>) -> vec2<f32> {
    let s = max(P.v[2].y, 0.05);
    let a = P.v[2].z;
    let q = vec2<f32>(uv.x * P.v[1].z, uv.y) / s;
    // Waves rise through the picture as the angle turns.
    let dx = sin(q.y * 70.0 + a + sin(q.x * 23.0 + a) * 1.7) * 0.6 + sin(q.y * 150.0 + 2.0 * a + q.x * 41.0) * 0.4;
    let dy = sin(q.y * 55.0 + 3.0 * a + q.x * 31.0) * 0.5;
    let region = i32(P.v[2].w + 0.5);
    var mask = 1.0;
    if (region == 0) {
        // Hot air rises: look for bright, warm things just below.
        var heat = 0.0;
        for (var i = 1; i <= 3; i = i + 1) {
            let c = textureSampleLevel(t_a, s_lin, uv + vec2<f32>(0.0, f32(i) * 0.03), 0.0).rgb;
            let lum = dot(c, vec3<f32>(0.3, 0.5, 0.2));
            heat = heat + smoothstep(0.7, 2.5, lum + max(c.r - c.b, 0.0) * 0.5);
        }
        mask = heat / 3.0;
    } else if (region == 1) {
        mask = smoothstep(0.35, 1.0, uv.y);
    }
    return vec2<f32>(dx, dy) * 0.0025 * mask;
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

// --- god rays / lens flare --------------------------------------------------
// P.v[0]: light position (uv), visibility, intensity
// P.v[1]: length (0..1), threshold, aspect, flare
// P.v[2]: tint
@fragment
fn fs_rays(in: VOut) -> @location(0) vec4<f32> {
    let sun = P.v[0].xy;
    let vis = P.v[0].z;
    let aspect = P.v[1].z;
    let thr = P.v[1].y;
    var sum = vec3<f32>(0.0);
    if (vis > 0.0) {
        let n = 48;
        let delta = (in.uv - sun) * clamp(P.v[1].x, 0.02, 1.0) / f32(n);
        // Fixed per-pixel offset breaks up banding.
        let jit = hash1(hash_u(u32(in.pos.x) * 7919u + u32(in.pos.y) * 104729u));
        var uv = in.uv - delta * jit;
        var decay = 1.0;
        for (var i = 0; i < n; i = i + 1) {
            let s = textureSampleLevel(t_a, s_lin, uv, 0.0).rgb;
            let d = (uv - sun) * vec2<f32>(aspect, 1.0);
            // P.v[2].w: how tightly rays gather at the source (0: everywhere).
            let near = exp(-dot(d, d) * P.v[2].w);
            sum = sum + min(max(s - vec3<f32>(thr), vec3<f32>(0.0)), vec3<f32>(6.0)) * decay * near;
            decay = decay * 0.965;
            uv = uv - delta;
        }
        sum = sum * P.v[0].w * vis / f32(n) * 2.0 * P.v[2].rgb;
    }
    // Lens flare: ghosts mirrored through the centre, and a halo.
    let flare = P.v[1].w;
    if (flare > 0.0 && vis > 0.0) {
        // How much of the light is actually visible (not blocked).
        let lsrc = textureSampleLevel(t_a, s_lin, clamp(sun, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0).rgb;
        let src = clamp(max(max(lsrc.r, lsrc.g), lsrc.b) - thr, 0.0, 4.0) * vis;
        let axis = vec2<f32>(0.5) - sun;
        var f = vec3<f32>(0.0);
        f = f + ghost(in.uv, sun + axis * 0.4, 0.02, aspect) * vec3<f32>(0.4, 0.8, 1.0);
        f = f + ghost(in.uv, sun + axis * 0.7, 0.045, aspect) * vec3<f32>(1.0, 0.6, 0.3) * 0.6;
        f = f + ghost(in.uv, sun + axis * 1.25, 0.03, aspect) * vec3<f32>(0.5, 1.0, 0.6);
        f = f + ghost(in.uv, sun + axis * 1.6, 0.07, aspect) * vec3<f32>(0.7, 0.4, 1.0) * 0.35;
        f = f + ghost(in.uv, sun + axis * 2.1, 0.015, aspect) * vec3<f32>(1.0, 0.9, 0.6);
        // Halo ring around the centre of the lens.
        let hd = (in.uv - vec2<f32>(0.5) - normalize(-axis + vec2<f32>(1e-5)) * 0.25) * vec2<f32>(aspect, 1.0);
        let ring = smoothstep(0.03, 0.0, abs(length((in.uv - 0.5) * vec2<f32>(aspect, 1.0)) - 0.45));
        f = f + vec3<f32>(0.6, 0.8, 1.0) * ring * exp(-dot(hd, hd) * 8.0) * 0.5;
        // Streak through the light.
        let sd = (in.uv - sun) * vec2<f32>(aspect, 1.0);
        f = f + P.v[2].rgb * exp(-abs(sd.y) * 300.0) * exp(-abs(sd.x) * 3.0) * 0.8;
        sum = sum + f * flare * src * 0.25;
    }
    return vec4<f32>(sum, 1.0);
}

fn ghost(uv: vec2<f32>, c: vec2<f32>, r: f32, aspect: f32) -> f32 {
    let d = length((uv - c) * vec2<f32>(aspect, 1.0));
    return smoothstep(r, r * 0.6, d) * (0.6 + 0.4 * smoothstep(r * 0.5, r, d));
}

// Adds the (half-size) rays image on top of the scene.
@fragment
fn fs_rays_add(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSampleLevel(t_a, s_lin, in.uv, 0.0).rgb, 1.0);
}

// Screen-space reflections onto the scene: t_a = the reflections, t_b =
// the distances (both half resolution). A 5×5 blur that stays on one
// surface (weights fall off with the difference in distance) smooths the
// march's jitter without bleeding onto what is behind.
@fragment
fn fs_ssr_add(in: VOut) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(t_a));
    let c = clamp(vec2<i32>(in.uv * vec2<f32>(dims)), vec2<i32>(0), dims - 1);
    let d0 = textureLoad(t_b, c, 0).r;
    let tol = 0.03 * d0 + 0.02;
    var sum = vec3<f32>(0.0);
    var wsum = 0.0;
    for (var y = -2; y <= 2; y = y + 1) {
        for (var x = -2; x <= 2; x = x + 1) {
            let p = clamp(c + vec2<i32>(x, y), vec2<i32>(0), dims - 1);
            let d = textureLoad(t_b, p, 0).r;
            let w = exp(-f32(x * x + y * y) * 0.25) * exp(-abs(d - d0) / tol);
            sum = sum + textureLoad(t_a, p, 0).rgb * w;
            wsum = wsum + w;
        }
    }
    return vec4<f32>(sum / max(wsum, 1e-5), 0.0);
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
    return vi_filter(uv) + textureSampleLevel(t_b, s_lin, uv, 0.0).rgb * bloom_k;
}

// Retro 3D: the Nintendo 64's video output filter. P.v[25]: amount (0 =
// off), one console pixel (uv across, down). Neighbours within a dither
// step of the middle are averaged in (de-dither), then the picture is
// softened sideways.
fn vi_filter(uv: vec2<f32>) -> vec3<f32> {
    let c = textureSampleLevel(t_a, s_lin, uv, 0.0).rgb;
    let k = P.v[25].x;
    if (k <= 0.0) {
        return c;
    }
    let dx = vec2<f32>(P.v[25].y, 0.0);
    let dy = vec2<f32>(0.0, P.v[25].z);
    let l = textureSampleLevel(t_a, s_lin, uv - dx, 0.0).rgb;
    let r = textureSampleLevel(t_a, s_lin, uv + dx, 0.0).rgb;
    let u = textureSampleLevel(t_a, s_lin, uv - dy, 0.0).rgb;
    let d = textureSampleLevel(t_a, s_lin, uv + dy, 0.0).rgb;
    // A dither step is 1/32 of the gamma range; compare in gamma.
    let gc = sqrt(max(c, vec3<f32>(0.0)));
    var sum = c;
    var n = 1.0;
    for (var i = 0; i < 4; i = i + 1) {
        var nb = l;
        if (i == 1) {
            nb = r;
        } else if (i == 2) {
            nb = u;
        } else if (i == 3) {
            nb = d;
        }
        let diff = abs(sqrt(max(nb, vec3<f32>(0.0))) - gc);
        let close = select(0.0, 1.0, max(diff.r, max(diff.g, diff.b)) < 0.07);
        sum = sum + nb * close;
        n = n + close;
    }
    let even = sum / n;
    let soft = (l + even * 2.0 + r) * 0.25;
    return mix(c, mix(even, soft, 0.5), clamp(k, 0.0, 1.0));
}

struct FinalOut {
    // sRGB export image: hand the hardware linear values.
    @location(0) output: vec4<f32>,
    // Display image (UNORM): the gamma-encoded values as they are.
    @location(1) display: vec4<f32>,
};

@fragment
fn fs_final(in: VOut) -> FinalOut {
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
    // VHS: lines jitter, a noisy band rolls down (P.v[6]: amount, bleed,
    // bands). Loop-safe: the frame id and the phase wrap with the loop.
    let vhs = P.v[6].x;
    var band = 0.0;
    if (vhs > 0.0) {
        let line = u32(uv.y * 240.0);
        let jit = hash1((line * 2654435761u) ^ (frame_id * 97531u)) - 0.5;
        let centre = fract(P.v[3].w * 3.0) * 1.3 - 0.15;
        band = smoothstep(0.07, 0.0, abs(uv.y - centre)) * P.v[6].z;
        let tear = hash1(line ^ (frame_id * 7919u)) - 0.5;
        uv.x = uv.x + (jit * 0.003 + tear * band * 0.04) * vhs;
    }
    // ASCII: one character per cell (P.v[7]: cell height in pixels).
    let cell_h = P.v[7].x;
    var cell_sub = vec2<f32>(0.0);
    let pix = P.v[3].x;
    var pcoord = vec2<u32>(in.pos.xy);
    if (cell_h > 0.0) {
        let cell = vec2<f32>(cell_h * 0.7, cell_h);
        let p = uv * res;
        let block = floor(p / cell);
        cell_sub = fract(p / cell);
        uv = (block + 0.5) * cell / res;
        pcoord = vec2<u32>(block);
    } else if (pix > 1.0) {
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
    if (vhs > 0.0 && P.v[6].y > 0.0) {
        // Colour smears to the right: red and blue lag behind.
        let o = vec2<f32>(P.v[6].y * vhs * 0.008, 0.0);
        let r = (sample_scene(uv - o, bloom_k).r + sample_scene(uv - o * 2.0, bloom_k).r) * 0.5;
        let b = (sample_scene(uv + o, bloom_k).b + sample_scene(uv - o * 3.0, bloom_k).b) * 0.5;
        col = vec3<f32>(mix(col.r, r, 0.8), col.g, mix(col.b, b, 0.8));
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

    if (cell_h > 0.0) {
        col = ascii_cell(col, cell_sub);
    }
    if (vhs > 0.0) {
        // Snow, thicker in the band, and a little washed-out colour.
        let n = hash1(hash_u(u32(in.pos.x) + u32(in.pos.y) * 7919u) ^ (frame_id * 1597334677u));
        col = col + vec3<f32>((n - 0.5) * vhs * (0.08 + band * 0.5));
        let l = dot(col, vec3<f32>(0.299, 0.587, 0.114));
        col = mix(col, vec3<f32>(l), vhs * 0.15);
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
    let c = clamp(col, vec3<f32>(0.0), vec3<f32>(1.0));
    var out: FinalOut;
    out.output = vec4<f32>(to_linear(c), 1.0);
    out.display = vec4<f32>(c, 1.0);
    return out;
}

// A character (5×5 bits, row by row from the top left) as dense as the
// brightness `l`: blank . : - + o 8 # @
fn ascii_glyph(l: f32) -> u32 {
    if (l < 0.08) {
        return 0u;
    }
    if (l < 0.16) {
        return 4194304u;
    }
    if (l < 0.25) {
        return 131200u;
    }
    if (l < 0.35) {
        return 14336u;
    }
    if (l < 0.45) {
        return 145536u;
    }
    if (l < 0.55) {
        return 15254976u;
    }
    if (l < 0.67) {
        return 15252014u;
    }
    if (l < 0.8) {
        return 11512810u;
    }
    return 15652782u;
}

// The cell's colour `col` drawn as a character; `sub` is where in the cell
// (0..1). P.v[7].y: how much picture shows behind; P.v[24]: fixed ink
// colour (w = 1) instead of the picture's.
fn ascii_cell(col: vec3<f32>, sub: vec2<f32>) -> vec3<f32> {
    // Lifted, so the dark parts of a scene still get characters.
    let l = sqrt(clamp(dot(col, vec3<f32>(0.299, 0.587, 0.114)), 0.0, 1.0));
    let n = ascii_glyph(l);
    // 5 × 5 dots with a one-dot margin.
    let x = i32(floor(sub.x * 7.0)) - 1;
    let y = i32(floor(sub.y * 7.0)) - 1;
    var on = 0.0;
    if (x >= 0 && x < 5 && y >= 0 && y < 5) {
        on = f32((n >> u32(x + y * 5)) & 1u);
    }
    var ink = col / max(max(col.r, max(col.g, col.b)), 0.2);
    if (P.v[24].w > 0.5) {
        ink = P.v[24].rgb * (0.4 + 0.6 * l);
    }
    return ink * on + col * P.v[7].y * (1.0 - on);
}

// Scene transitions: mixes two finished pictures (t_a = the scene going
// out, t_b = the scene coming in; both linear from sRGB textures).
// P.v[0]: kind (see TransitionKind), progress 0..1, angle (radians), aspect
@fragment
fn fs_compose(in: VOut) -> FinalOut {
    let uv = in.uv;
    let kind = i32(P.v[0].x + 0.5);
    let t = clamp(P.v[0].y, 0.0, 1.0);
    let aspect = max(P.v[0].w, 0.01);
    let a = textureSampleLevel(t_a, s_lin, uv, 0.0).rgb;
    let b = textureSampleLevel(t_b, s_lin, uv, 0.0).rgb;
    var c = select(a, b, t >= 1.0);
    let p = (uv - 0.5) * vec2<f32>(aspect, 1.0);
    switch kind {
        case 1: {
            c = mix(a, b, smoothstep(0.0, 1.0, t));
        }
        case 2: {
            // Wipe: a soft edge sweeps across, the new scene behind it.
            let dir = vec2<f32>(cos(P.v[0].z), sin(P.v[0].z));
            let extent = 0.5 * (abs(dir.x) * aspect + abs(dir.y)) + 0.05;
            let edge = mix(-extent, extent, t);
            c = mix(b, a, smoothstep(edge - 0.03, edge + 0.03, dot(p, dir)));
        }
        case 3: {
            // Iris: a circle opening from the middle.
            let r = t * (0.5 * length(vec2<f32>(aspect, 1.0)) + 0.05);
            c = mix(b, a, smoothstep(r - 0.02, r + 0.02, length(p)));
        }
        case 4: {
            // Flash to white and back.
            let w = 1.0 - abs(t * 2.0 - 1.0);
            c = mix(select(b, a, t < 0.5), vec3<f32>(1.0), pow(w, 0.6));
        }
        case 5: {
            // Glitch: blocks of the new scene pop in, rows jitter.
            let cell = floor(uv * vec2<f32>(16.0, 9.0));
            let r = hash1(u32(cell.x) + u32(cell.y) * 97u + 13u);
            let step = u32(t * 12.0);
            let band = hash1(u32(uv.y * 24.0) * 31u + step * 7919u);
            let shift = vec2<f32>((band - 0.5) * 0.08 * sin(PI * t), 0.0);
            let a2 = textureSampleLevel(t_a, s_lin, uv + shift, 0.0).rgb;
            let b2 = textureSampleLevel(t_b, s_lin, uv - shift, 0.0).rgb;
            c = select(a2, b2, r < t);
            let split = sin(PI * t) * 0.006;
            let rr = select(textureSampleLevel(t_a, s_lin, uv + vec2<f32>(split, 0.0), 0.0).r,
                textureSampleLevel(t_b, s_lin, uv + vec2<f32>(split, 0.0), 0.0).r, r < t);
            c = vec3<f32>(rr, c.g, c.b);
        }
        default: {}
    }
    var out: FinalOut;
    out.output = vec4<f32>(c, 1.0);
    out.display = vec4<f32>(to_srgb(c), 1.0);
    return out;
}

// Feedback trails: the previous result (t_b), zoomed and turned around the
// centre and faded, under the new picture (t_a).
// P.v[0]: fade (0..1, this frame), zoom (this frame), turn (radians), hue (turns)
// P.v[1]: aspect, start over (1 = no history)
@fragment
fn fs_feedback(in: VOut) -> @location(0) vec4<f32> {
    let cur = textureSampleLevel(t_a, s_lin, in.uv, 0.0).rgb;
    if (P.v[1].y > 0.5) {
        return vec4<f32>(cur, 1.0);
    }
    let aspect = max(P.v[1].x, 0.01);
    var p = (in.uv - 0.5) * vec2<f32>(aspect, 1.0) / max(P.v[0].y, 0.01);
    let c = cos(-P.v[0].z);
    let s = sin(-P.v[0].z);
    p = vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
    let q = p / vec2<f32>(aspect, 1.0) + 0.5;
    var prev = textureSampleLevel(t_b, s_lin, q, 0.0).rgb * P.v[0].x;
    // Outside the old picture there is nothing to trail.
    let inside = step(0.0, q.x) * step(q.x, 1.0) * step(0.0, q.y) * step(q.y, 1.0);
    prev = prev * inside;
    if (P.v[0].w != 0.0) {
        prev = hue_turn(prev, P.v[0].w);
    }
    return vec4<f32>(max(cur, prev), 1.0);
}

fn hue_turn(c: vec3<f32>, turns: f32) -> vec3<f32> {
    let a = turns * TAU;
    let k = vec3<f32>(0.57735);
    let cosa = cos(a);
    return c * cosa + cross(k, c) * sin(a) + k * dot(k, c) * (1.0 - cosa);
}
