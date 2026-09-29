// Screen-space reflections (half resolution). For each surface pixel of
// the distance pass's G-buffer, march the mirror ray through the distance
// texture; at a hit, swap the reflected environment the surface already
// shows for the picture there: k × hit − e (see `Mirror` in common.wgsl).
// The result is added onto the scene. No history: a fixed Bayer jitter
// spreads the steps, so every frame stands alone and loops.
// D.v[0]: strength, maximum distance, roughness cut-off, steps
// D.v[1]: thickness at the start, thickness per unit of distance, _, _

@group(2) @binding(0) var t_surf: texture_2d<f32>;
@group(2) @binding(1) var t_k: texture_2d<f32>;
@group(2) @binding(2) var t_e: texture_2d<f32>;
@group(2) @binding(3) var t_dist: texture_2d<f32>;
@group(2) @binding(4) var t_scene: texture_2d<f32>;
@group(2) @binding(5) var s_lin: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FullscreenOut {
    return fullscreen(vi);
}

// 4×4 ordered dither, 0..1 (bit arithmetic: no local lookup table).
fn bayer2(x: i32, y: i32) -> i32 {
    return (((x ^ y) & 1) << 1) | (y & 1);
}

fn bayer4(p: vec2<i32>) -> f32 {
    let v = bayer2(p.x & 1, p.y & 1) * 4 + bayer2((p.x >> 1) & 1, (p.y >> 1) & 1);
    return (f32(v) + 0.5) / 16.0;
}

fn screen_uv(q: vec3<f32>) -> vec3<f32> {
    let c = G.view_proj * vec4<f32>(q, 1.0);
    if (c.w <= 1e-4) {
        return vec3<f32>(-1.0, -1.0, 0.0);
    }
    return vec3<f32>(c.x / c.w * 0.5 + 0.5, 0.5 - c.y / c.w * 0.5, 1.0);
}

fn scene_dist(uv: vec2<f32>) -> f32 {
    let dims = vec2<i32>(textureDimensions(t_dist));
    let p = clamp(vec2<i32>(uv * vec2<f32>(dims)), vec2<i32>(0), dims - 1);
    return textureLoad(t_dist, p, 0).r;
}

@fragment
fn fs_main(in: FullscreenOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.pos.xy);
    let surf = textureLoad(t_surf, px, 0);
    let k = textureLoad(t_k, px, 0).rgb;
    let rough = surf.z;
    let cut = D.v[0].z;
    if (surf.w < 0.5 || max(k.x, max(k.y, k.z)) < 1e-3 || rough >= cut) {
        return vec4<f32>(0.0);
    }
    let cam = G.cam_pos.xyz;
    let rd = view_ray(in.ndc);
    let dist = textureLoad(t_dist, px, 0).r;
    let p = cam + rd * dist;
    let n = oct_decode(surf.xy);
    let r = reflect(rd, n);
    let max_d = max(D.v[0].y, 0.1);
    let steps = clamp(i32(D.v[0].w), 4, 64);
    // Start a little off the surface: half-resolution distances are coarse.
    let start = 0.03 + dist * 0.01;
    let jitter = bayer4(px);
    var lo = start;
    var hi = start;
    var hit = false;
    for (var i = 0; i < steps; i = i + 1) {
        let f = (f32(i) + jitter) / f32(steps);
        let t = start + max_d * f * f;
        let s = screen_uv(p + r * t);
        if (s.z < 0.5 || any(s.xy < vec2<f32>(0.0)) || any(s.xy > vec2<f32>(1.0))) {
            break;
        }
        let behind = length(p + r * t - cam) - scene_dist(s.xy);
        if (behind > 0.0 && behind < D.v[1].x + t * D.v[1].y) {
            hit = true;
            hi = t;
            break;
        }
        // Far behind something: the ray may pass behind it and come out
        // the other side, so keep going.
        lo = t;
    }
    if (!hit) {
        return vec4<f32>(0.0);
    }
    // Home in on the surface.
    for (var j = 0; j < 5; j = j + 1) {
        let mid = (lo + hi) * 0.5;
        let s = screen_uv(p + r * mid);
        if (length(p + r * mid - cam) > scene_dist(s.xy)) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let uv = screen_uv(p + r * hi).xy;
    var w = D.v[0].x;
    // Fade near the picture's edges, far along the ray and towards the
    // roughness cut-off; not the back of something.
    let edge = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    w = w * smoothstep(0.0, 0.06, edge);
    w = w * (1.0 - smoothstep(0.7, 1.0, (hi - start) / max_d));
    w = w * (1.0 - smoothstep(cut * 0.6, cut, rough));
    let hs = textureLoad(t_surf, clamp(vec2<i32>(uv * vec2<f32>(textureDimensions(t_surf))), vec2<i32>(0), vec2<i32>(textureDimensions(t_surf)) - 1), 0);
    if (hs.w > 0.5) {
        w = w * (1.0 - smoothstep(0.0, 0.5, dot(oct_decode(hs.xy), r)));
    }
    if (w <= 0.0) {
        return vec4<f32>(0.0);
    }
    // The picture there, blurred by roughness (a small disc of taps).
    let radius = rough * rough * 0.25 * min(hi, 6.0) / max(dist + hi, 0.5);
    var col = textureSampleLevel(t_scene, s_lin, uv, 0.0).rgb;
    if (radius > 0.001) {
        for (var j = 0; j < 6; j = j + 1) {
            let a = f32(j) * 1.0472 + jitter * 6.2832;
            let o = vec2<f32>(cos(a), sin(a) * G.res.x * G.res.w) * radius;
            col = col + textureSampleLevel(t_scene, s_lin, uv + o, 0.0).rgb;
        }
        col = col / 7.0;
    }
    let e = textureLoad(t_e, px, 0).rgb;
    return vec4<f32>((k * col - e) * w, 1.0);
}
