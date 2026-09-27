// Sky overlay drawn over the backgrounds: night falling (the sky darkens
// and takes the sunset tint), stars, the moon and a rainbow opposite the sun.
// fs_mul is blended multiplicatively, fs_add additively.

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FullscreenOut {
    var o = fullscreen(vi);
    o.pos.z = 1.0;
    return o;
}

@fragment
fn fs_mul(in: FullscreenOut) -> @location(0) vec4<f32> {
    let night = G.extra.y;
    let dusk = G.extra.z;
    let rd = view_ray(in.ndc);
    // Warm light near the horizon at sunrise and sunset.
    let warm = normalize(G.light_color.rgb + vec3<f32>(1e-3)) * 1.7;
    let glow_k = dusk * 0.7 * (0.5 + 0.5 * smoothstep(0.6, -0.1, rd.y));
    var m = mix(vec3<f32>(1.0), min(warm, vec3<f32>(1.3)), glow_k);
    m = m * mix(1.0, 0.12, night);
    return vec4<f32>(m, 1.0);
}

fn sky_stars(dir: vec3<f32>, scale: f32, density: f32) -> f32 {
    let p = dir * scale;
    let cell = floor(p);
    let h = hash3f(cell);
    if (h > density) {
        return 0.0;
    }
    let local = fract(p) - 0.5;
    let off = vec3<f32>(hash3f(cell + 7.1), hash3f(cell + 3.3), hash3f(cell + 5.9)) - 0.5;
    let d = length(local - off * 0.6);
    let tw = 0.7 + 0.3 * sin(TAU * (G.time.x * 3.0 + h * 13.0));
    return smoothstep(0.08, 0.0, d) * tw * (h / density);
}

// Spectrum from violet (0) to red (1).
fn spectrum(x: f32) -> vec3<f32> {
    let t = clamp(x, 0.0, 1.0);
    return clamp(vec3<f32>(
        smoothstep(0.45, 0.85, t) + smoothstep(0.1, 0.0, t) * 0.5,
        1.0 - abs(t - 0.5) * 2.2,
        smoothstep(0.55, 0.15, t),
    ), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_add(in: FullscreenOut) -> @location(0) vec4<f32> {
    let rd = view_ray(in.ndc);
    let night = G.extra.y;
    let sun = normalize(G.sun.xyz);
    var col = vec3<f32>(0.0);
    let above = smoothstep(-0.02, 0.08, rd.y);
    if (night > 0.0) {
        col = col + vec3<f32>(0.9, 0.95, 1.0) * sky_stars(rd, 140.0, 0.07) * night * above * 1.5;
        // The moon, opposite the sun.
        let moon = -sun;
        let md = dot(rd, moon);
        let disc = smoothstep(0.99955, 0.9997, md);
        let spots = 0.8 + 0.2 * vnoise3(rd * 400.0);
        col = col + vec3<f32>(0.9, 0.92, 1.0) * disc * spots * night * 3.0;
        col = col + vec3<f32>(0.5, 0.6, 0.9) * pow(max(md, 0.0), 300.0) * night * 0.4;
    }
    let bow_k = G.extra.w * (1.0 - night) * smoothstep(0.0, 0.08, sun.y);
    if (bow_k > 0.0) {
        let c = clamp(dot(rd, -sun), -1.0, 1.0);
        let ang = acos(c) * 57.29578;
        // Primary bow (violet inside, red outside) and a fainter,
        // reversed secondary bow, with a brighter sky inside the first.
        let b1 = (ang - 40.6) / 1.9;
        let m1 = smoothstep(-0.25, 0.1, b1) * smoothstep(1.25, 0.9, b1);
        let b2 = (53.5 - ang) / 3.0;
        let m2 = smoothstep(-0.2, 0.1, b2) * smoothstep(1.2, 0.9, b2);
        let inside = smoothstep(41.0, 30.0, ang) * 0.06;
        let fade = smoothstep(0.0, 0.12, rd.y);
        col = col + (spectrum(b1) * m1 + spectrum(b2) * m2 * 0.35 + vec3<f32>(inside)) * bow_k * fade * 0.35;
    }
    return vec4<f32>(col, 0.0);
}
