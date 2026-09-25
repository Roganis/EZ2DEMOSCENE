// Fullscreen procedural / raymarched backgrounds.
// D.v[0]: kind, speed (cycles/loop), intensity, detail
// D.v[1..3]: colours a, b, c;  D.v[4].x: has texture

@group(2) @binding(0) var t_tex: texture_2d<f32>;
@group(2) @binding(1) var s_tex: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FullscreenOut {
    var o = fullscreen(vi);
    o.pos.z = 1.0;
    return o;
}

fn stars(dir: vec3<f32>, scale: f32, density: f32, twinkle_cycles: f32) -> f32 {
    let p = dir * scale;
    let cell = floor(p);
    let h = hash3f(cell);
    if (h > density) {
        return 0.0;
    }
    let local = fract(p) - 0.5;
    let off = vec3<f32>(hash3f(cell + 7.1), hash3f(cell + 3.3), hash3f(cell + 5.9)) - 0.5;
    let d = length(local - off * 0.6);
    let tw = 0.6 + 0.4 * sin(TAU * (G.time.x * twinkle_cycles + h * 13.0));
    return smoothstep(0.08, 0.0, d) * tw * (h / density);
}

fn tunnel_path(z: f32, period: f32) -> vec2<f32> {
    let a = z * TAU / period;
    return vec2<f32>(sin(a) * 0.8, cos(a * 2.0) * 0.5);
}

fn bg_tunnel(rd_in: vec3<f32>, speed: f32, detail: f32, ca: vec3<f32>, cb: vec3<f32>, cc: vec3<f32>, use_tex: bool) -> vec3<f32> {
    let period = 16.0;
    let z0 = -G.time.x * speed * period;
    let ro = vec3<f32>(tunnel_path(z0, period), z0);
    // Look down the tunnel (-Z) with a slight turn following the path.
    let ahead = vec3<f32>(tunnel_path(z0 - 2.0, period), z0 - 2.0);
    let fwd = normalize(ahead - ro);
    let right = normalize(cross(fwd, vec3<f32>(0.0, 1.0, 0.0)));
    let upv = cross(right, fwd);
    // Keep the user's camera roll/fov by using the view ray in camera space.
    let cam_fwd = cross(G.cam_up.xyz, G.cam_right.xyz);
    let cam_rd = vec3<f32>(dot(rd_in, G.cam_right.xyz), dot(rd_in, G.cam_up.xyz), dot(rd_in, cam_fwd));
    let rd = normalize(right * cam_rd.x + upv * cam_rd.y + fwd * cam_rd.z);
    let radius = 1.6;
    var t = 0.0;
    var hit = false;
    for (var i = 0; i < 64; i = i + 1) {
        let p = ro + rd * t;
        let q = p.xy - tunnel_path(p.z, period);
        let ang = atan2(q.y, q.x);
        let wall = radius + 0.08 * sin(ang * 6.0 + p.z * TAU / 4.0);
        let d = wall - length(q);
        if (d < 0.002) {
            hit = true;
            break;
        }
        t = t + d * 0.8;
        if (t > 60.0) {
            break;
        }
    }
    let p = ro + rd * t;
    let q = p.xy - tunnel_path(p.z, period);
    let u = atan2(q.y, q.x) / TAU + 0.5;
    let v = p.z / period * 4.0 * detail;
    var pattern: vec3<f32>;
    if (use_tex) {
        pattern = textureSampleLevel(t_tex, s_tex, vec2<f32>(u * 2.0, v), 0.0).rgb;
    } else {
        let x = u32(fract(u * 2.0) * 64.0) ^ u32(fract(v) * 64.0);
        let k = f32(x & 63u) / 63.0;
        pattern = vec3<f32>(k);
    }
    var col = mix(cb, cc, pattern) * (0.3 + 0.7 * pattern);
    let fog = exp(-t * 0.06);
    col = mix(ca, col, fog);
    if (!hit) {
        col = ca;
    }
    return col;
}

fn bg_kaliset(rd: vec3<f32>, speed: f32, detail: f32, ca: vec3<f32>, cb: vec3<f32>, cc: vec3<f32>) -> vec3<f32> {
    let a = G.time.x * speed * TAU;
    let origin = vec3<f32>(1.0 + 0.3 * cos(a), 0.5 + 0.3 * sin(a), 0.5 + 0.2 * sin(a));
    var s = 0.1;
    var fade = 1.0;
    var v = vec3<f32>(0.0);
    for (var r = 0; r < 12; r = r + 1) {
        var p = origin + s * rd * 0.5;
        p = abs(vec3<f32>(0.85) - (p - floor(p / 1.7) * 1.7));
        var pa = 0.0;
        var acc = 0.0;
        for (var i = 0; i < 13; i = i + 1) {
            p = abs(p) / dot(p, p) - 0.53 * detail;
            let lp = length(p);
            acc = acc + abs(lp - pa);
            pa = lp;
        }
        acc = acc * acc * acc * 0.0015;
        v = v + fade * mix(cb, cc, clamp(f32(r) / 12.0, 0.0, 1.0)) * acc * 0.12;
        fade = fade * 0.73;
        s = s + 0.1;
    }
    return ca + v * 0.25;
}

fn bg_plasma(rd: vec3<f32>, speed: f32, detail: f32, ca: vec3<f32>, cb: vec3<f32>, cc: vec3<f32>) -> vec3<f32> {
    let t = G.time.x * TAU * speed;
    let uv = vec2<f32>(atan2(rd.x, -rd.z) / PI, rd.y) * 3.0 * detail;
    var v = sin(uv.x * 3.0 + t);
    v = v + sin(3.0 * (uv.x * sin(t * 0.5 * 2.0) + uv.y * cos(t)) + t);
    let c = uv + vec2<f32>(sin(t) * 1.5, cos(t) * 1.5);
    v = v + sin(sqrt(dot(c, c) * 4.0 + 1.0) * 2.0 - t);
    v = v + sin(uv.y * 4.0 - 2.0 * t);
    let k = v * 0.25;
    let w = 0.5 + 0.5 * vec3<f32>(sin(k * PI), sin(k * PI + 2.094), sin(k * PI + 4.188));
    return ca + cb * w.x + cc * w.y * w.z;
}

fn bg_synth(rd: vec3<f32>, speed: f32, detail: f32, ca: vec3<f32>, cb: vec3<f32>, cc: vec3<f32>) -> vec3<f32> {
    // Sky
    let h = rd.y;
    var col = mix(cb * 0.6, ca, smoothstep(-0.05, 0.6, h));
    // Sun towards -Z
    let sun_dir = normalize(vec3<f32>(0.0, 0.12, -1.0));
    let sd = acos(clamp(dot(rd, sun_dir), -1.0, 1.0));
    let sun_r = 0.28;
    if (sd < sun_r && h > -0.02) {
        let sy = (rd.y - sun_dir.y) / sun_r;  // -1..1 within the disc
        let stripe = fract(sy * 5.0 * detail + G.time.x * speed);
        let cut = sy < 0.1 && stripe < mix(0.05, 0.45, clamp(-sy, 0.0, 1.0));
        if (!cut) {
            col = mix(cb, cc, smoothstep(-0.8, 0.9, sy)) * 1.6;
        }
    }
    col = col + cb * exp(-sd * 3.0) * 0.4;
    // Stars
    col = col + vec3<f32>(stars(rd, 90.0, 0.08, 2.0)) * smoothstep(0.1, 0.5, h);
    // Ground grid (only visible if there is no floor layer in front)
    if (h < 0.0) {
        let t = -1.0 / h;
        let p = rd.xz * t;
        let z = p.y + G.time.x * speed * 8.0;
        let gx = abs(fract(p.x * 0.5) - 0.5);
        let gz = abs(fract(z * 0.5) - 0.5);
        let line = max(smoothstep(0.03 * t * 0.1 + 0.01, 0.0, gx), smoothstep(0.03 * t * 0.1 + 0.01, 0.0, gz));
        col = mix(ca * 0.3, cb * 2.0, line * exp(-t * 0.03));
    }
    return col;
}

@fragment
fn fs_main(in: FullscreenOut) -> @location(0) vec4<f32> {
    let rd = view_ray(in.ndc);
    let kind = i32(D.v[0].x + 0.5);
    let speed = D.v[0].y;
    let intensity = D.v[0].z;
    let detail = D.v[0].w;
    let ca = D.v[1].rgb;
    let cb = D.v[2].rgb;
    let cc = D.v[3].rgb;
    let use_tex = D.v[4].x > 0.5;
    var col = ca;
    switch kind {
        case 0: {
            // Gradient sky with a soft sun glow.
            let h = rd.y;
            col = mix(cb, ca, smoothstep(-0.2, 0.8, h));
            col = col + cc * pow(max(1.0 - abs(h), 0.0), 8.0) * 0.6;
            col = col + G.light_color.rgb * pow(max(dot(rd, normalize(G.light_dir.xyz)), 0.0), 64.0) * 0.5;
        }
        case 1: {
            // Nebula: fbm clouds drifting on a closed loop + stars.
            let a = G.time.x * TAU * speed;
            let drift = vec3<f32>(cos(a), sin(a), sin(a) * 0.5) * 0.6;
            let p = rd * 2.2 * detail + drift;
            let n = fbm3(p, 5);
            let n2 = fbm3(p * 2.0 + vec3<f32>(n * 2.0), 4);
            let cloud = smoothstep(0.3, 0.9, n * 0.6 + n2 * 0.6);
            col = mix(ca, cb, cloud);
            col = col + cc * pow(cloud, 4.0) * 1.5;
            col = col + vec3<f32>(stars(rd, 120.0, 0.05, 3.0)) * (1.0 - cloud);
        }
        case 2: {
            // Starfield with faint coloured dust.
            let a = G.time.x * TAU * speed;
            let dust = fbm3(rd * 3.0 * detail + vec3<f32>(cos(a), sin(a), 0.0) * 0.3, 4);
            col = mix(ca, cb, smoothstep(0.35, 0.8, dust));
            col = col + cc * stars(rd, 80.0, 0.06, 2.0);
            col = col + vec3<f32>(1.0) * stars(rd, 200.0, 0.04, 4.0) * 0.8;
        }
        case 3: {
            col = bg_tunnel(rd, speed, detail, ca, cb, cc, use_tex);
        }
        case 4: {
            col = bg_kaliset(rd, speed, detail, ca, cb, cc);
        }
        case 5: {
            col = bg_plasma(rd, speed, detail, ca, cb, cc);
        }
        case 6: {
            col = bg_synth(rd, speed, detail, ca, cb, cc);
        }
        default: {}
    }
    return vec4<f32>(max(col * intensity, vec3<f32>(0.0)), 1.0);
}
