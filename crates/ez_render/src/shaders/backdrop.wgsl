// Fullscreen procedural / raymarched backgrounds.
// D.v[0]: kind, speed (cycles/loop), intensity, detail
// D.v[1..3]: colours a, b, c;  D.v[4].x: has texture
// Raymarched kinds (tunnel, fractal, sponge, rings):
// D.v[5]: variant, pattern, size, twist
// D.v[6]: warp, bend, glow, fog
// D.v[7]: steps (0 = default), view roll (radians)

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
    return vec2<f32>(sin(a) * 0.8, cos(a * 2.0) * 0.5) * D.v[6].y;
}

fn steps_or(default_steps: i32) -> i32 {
    let s = i32(D.v[7].x + 0.5);
    if (s <= 0) {
        return default_steps;
    }
    return s;
}

// Distance from a point `q` of the cross-section to the tunnel wall
// (positive inside). `tw` is the twist angle at this depth.
fn tunnel_dist(q: vec2<f32>, tw: f32, radius: f32, wob: f32) -> f32 {
    let variant = i32(D.v[5].x + 0.5);
    let r = length(q);
    let ang = atan2(q.y, q.x) + tw;
    var n = 0.0;
    switch variant {
        case 1: { n = 4.0; }
        case 2: { n = 6.0; }
        case 3: { n = 3.0; }
        case 4: {
            // Flower: not an exact distance, so step more carefully.
            return (radius * (1.0 + 0.22 * cos(ang * 6.0)) + wob - r) * 0.6;
        }
        default: { return radius + wob - r; }
    }
    // Regular polygon: exact distance to the nearest side.
    let seg = TAU / n;
    let a = (ang - floor(ang / seg) * seg) - seg * 0.5;
    return radius * cos(PI / n) + wob - r * cos(a);
}

fn wall_pattern(u: f32, v: f32) -> f32 {
    let pattern = i32(D.v[5].y + 0.5);
    switch pattern {
        case 1: {
            return f32((u32(floor(u * 16.0)) + u32(floor(v * 4.0))) % 2u);
        }
        case 2: {
            return 0.5 + 0.5 * cos(v * TAU * 2.0);
        }
        case 3: {
            return 0.5 + 0.5 * cos(u * TAU * 8.0);
        }
        default: {
            let x = u32(fract(u * 2.0) * 64.0) ^ u32(fract(v) * 64.0);
            return f32(x & 63u) / 63.0;
        }
    }
}

fn bg_tunnel(rd_in: vec3<f32>, speed: f32, detail: f32, ca: vec3<f32>, cb: vec3<f32>, cc: vec3<f32>, use_tex: bool) -> vec3<f32> {
    let period = 16.0;
    // Wrapped, so the last frame of the loop is exactly the first.
    let z0 = -fract(G.time.x * speed) * period;
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
    let radius = 1.6 * D.v[5].z;
    // Twist: turns of the cross-section per tunnel period.
    let twist = D.v[5].w * TAU / period;
    let wobble = 0.08 * D.v[6].x;
    let steps = steps_or(64);
    var t = 0.0;
    var hit = false;
    for (var i = 0; i < steps; i = i + 1) {
        let p = ro + rd * t;
        let q = p.xy - tunnel_path(p.z, period);
        let ang = atan2(q.y, q.x);
        let d = tunnel_dist(q, (p.z - z0) * twist, radius, wobble * sin(ang * 6.0 + p.z * TAU / 4.0));
        if (d < 0.002) {
            hit = true;
            break;
        }
        t = t + d * 0.8;
        if (t > 60.0) {
            break;
        }
    }
    // Out of steps but not far away: a ray skimming a wall. Shade the wall
    // rather than leaving a dark seam.
    if (!hit && t < 60.0) {
        hit = true;
    }
    let p = ro + rd * t;
    let q = p.xy - tunnel_path(p.z, period);
    // Twist is measured from the camera, and the pattern repeats a whole
    // number of times per tunnel period: both keep the loop seamless.
    let u = (atan2(q.y, q.x) + (p.z - z0) * twist) / TAU + 0.5;
    let v = p.z / period * max(round(4.0 * detail), 1.0);
    var pattern: vec3<f32>;
    if (use_tex) {
        pattern = textureSampleLevel(t_tex, s_tex, vec2<f32>(u * 2.0, v), 0.0).rgb;
    } else {
        pattern = vec3<f32>(wall_pattern(u, v));
    }
    var col = mix(cb, cc, pattern) * (0.3 + 0.7 * pattern);
    // Light rings sliding past with the flight.
    let glow = D.v[6].z;
    if (glow > 0.0) {
        let ring = exp(-abs(fract(p.z / period * 2.0) - 0.5) * 30.0);
        col = col + cc * ring * glow * 3.0;
    }
    let fog = exp(-t * 0.06 * D.v[6].w);
    col = mix(ca, col, fog);
    if (!hit) {
        col = ca;
    }
    return col;
}

fn bg_kaliset(rd: vec3<f32>, speed: f32, detail: f32, ca: vec3<f32>, cb: vec3<f32>, cc: vec3<f32>) -> vec3<f32> {
    let a = G.time.x * speed * TAU;
    let drift = D.v[6].y;
    let origin = vec3<f32>(1.0 + 0.3 * cos(a) * drift, 0.5 + 0.3 * sin(a) * drift, 0.5 + 0.2 * sin(a) * drift);
    // Formula variants: mirror offset and repeat period of the fold.
    let variant = i32(D.v[5].x + 0.5);
    var off = 0.85;
    var rep = 1.7;
    if (variant == 1) {
        off = 0.5;
        rep = 1.0;
    } else if (variant == 2) {
        off = 1.2;
        rep = 2.4;
    }
    let fold = 0.53 * detail * D.v[6].x;
    let zoom = D.v[5].z;
    let iters = steps_or(13);
    let keep = pow(0.73, D.v[6].w);
    var s = 0.1 * zoom;
    var fade = 1.0;
    var v = vec3<f32>(0.0);
    for (var r = 0; r < 12; r = r + 1) {
        var p = origin + s * rd * 0.5;
        p = abs(vec3<f32>(off) - (p - floor(p / rep) * rep));
        var pa = 0.0;
        var acc = 0.0;
        for (var i = 0; i < iters; i = i + 1) {
            p = abs(p) / dot(p, p) - fold;
            let lp = length(p);
            acc = acc + abs(lp - pa);
            pa = lp;
        }
        acc = acc * acc * acc * 0.0015;
        v = v + fade * mix(cb, cc, clamp(f32(r) / 12.0, 0.0, 1.0)) * acc * 0.12;
        fade = fade * keep;
        s = s + 0.1 * zoom;
    }
    return ca + v * 0.25 * (1.0 + D.v[6].z);
}

// Ray direction in "flight" space: looking down -Z, keeping the user's
// camera turn and field of view.
fn flight_dir(rd_in: vec3<f32>) -> vec3<f32> {
    let cam_fwd = cross(G.cam_up.xyz, G.cam_right.xyz);
    return normalize(vec3<f32>(dot(rd_in, G.cam_right.xyz), dot(rd_in, G.cam_up.xyz), -dot(rd_in, cam_fwd)));
}

fn rot2(v: vec2<f32>, a: f32) -> vec2<f32> {
    let c = cos(a);
    let s = sin(a);
    return vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y);
}

fn sd_box(p: vec3<f32>, b: vec3<f32>) -> f32 {
    let q = abs(p) - b;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

// Infinite sponge-like structures repeating every 2 units.
fn sponge_sdf(p_in: vec3<f32>) -> f32 {
    let variant = i32(D.v[5].x + 0.5);
    let p = p_in - 2.0 * floor((p_in + 1.0) / 2.0);
    switch variant {
        case 1: {
            // Beam lattice: three infinite beams through every cell corner.
            let q = abs(p) - vec3<f32>(1.0);
            let b = 0.18;
            let dx = length(max(abs(vec2<f32>(q.y, q.z)) - vec2<f32>(b), vec2<f32>(0.0)));
            let dy = length(max(abs(vec2<f32>(q.x, q.z)) - vec2<f32>(b), vec2<f32>(0.0)));
            let dz = length(max(abs(vec2<f32>(q.x, q.y)) - vec2<f32>(b), vec2<f32>(0.0)));
            return min(dx, min(dy, dz));
        }
        case 2: {
            // Field of small cubes, one per cell corner.
            let q = abs(p) - vec3<f32>(1.0);
            return sd_box(q, vec3<f32>(0.28)) - 0.03;
        }
        default: {
            // Menger sponge.
            var d = sd_box(p, vec3<f32>(1.0));
            var sc = 1.0;
            for (var m = 0; m < 4; m = m + 1) {
                let a = (p * sc) - 2.0 * floor((p * sc) / 2.0) - 1.0;
                sc = sc * 3.0;
                let r = abs(vec3<f32>(1.0) - 3.0 * abs(a));
                let da = max(r.x, r.y);
                let db = max(r.y, r.z);
                let dc = max(r.z, r.x);
                let c = (min(da, min(db, dc)) - 1.0) / sc;
                d = max(d, c);
            }
            return d;
        }
    }
}

// Camera depth of the current flight (twist is measured from it).
var<private> flight_z: f32;

fn sponge_map(p: vec3<f32>) -> f32 {
    let size = max(D.v[5].z, 0.05);
    var q = p / size;
    let tw = D.v[5].w * (p.z - flight_z) / size * TAU / 8.0;
    let xy = rot2(q.xy, tw);
    q = vec3<f32>(xy, q.z);
    return sponge_sdf(q) * size;
}

fn bg_sponge(rd_in: vec3<f32>, speed: f32, ca: vec3<f32>, cb: vec3<f32>, cc: vec3<f32>) -> vec3<f32> {
    let size = max(D.v[5].z, 0.05);
    // One loop flies 8 cells per speed unit, so the view repeats exactly.
    let period = 16.0 * size;
    // Wrapped, so the last frame of the loop is exactly the first.
    let z0 = -fract(G.time.x * speed) * period;
    let bend = D.v[6].y;
    let a = fract(G.time.x * speed) * TAU;
    let ro = vec3<f32>(sin(a) * 0.15 * bend * size, cos(a * 2.0) * 0.1 * bend * size, z0);
    flight_z = z0;
    let rd = flight_dir(rd_in);
    let steps = steps_or(80);
    var t = 0.02;
    var glow = 0.0;
    var hit = false;
    var n_steps = 0;
    for (var i = 0; i < steps; i = i + 1) {
        let d = sponge_map(ro + rd * t);
        glow = glow + exp(-d * 40.0 / size) * 0.02;
        if (d < 0.001 * t) {
            hit = true;
            break;
        }
        t = t + d;
        n_steps = i;
        if (t > 40.0 * size) {
            break;
        }
    }
    let p = ro + rd * t;
    let e = vec2<f32>(0.002 * size, 0.0);
    let n = normalize(vec3<f32>(
        sponge_map(p + e.xyy) - sponge_map(p - e.xyy),
        sponge_map(p + e.yxy) - sponge_map(p - e.yxy),
        sponge_map(p + e.yyx) - sponge_map(p - e.yyx),
    ));
    let ao = 1.0 - f32(n_steps) / f32(steps);
    let light = 0.35 + 0.65 * max(dot(n, normalize(vec3<f32>(0.4, 0.8, 0.3))), 0.0);
    var col = mix(cb, cc, 0.5 + 0.5 * n.y) * light * ao;
    col = col + cc * glow * D.v[6].z;
    let fog = exp(-t / size * 0.08 * D.v[6].w);
    col = mix(ca, col, fog);
    if (!hit) {
        col = ca + cc * glow * D.v[6].z;
    }
    return col;
}

// Glowing rings (or squares / triangles) every 2 units along -Z.
fn ring_sdf(p: vec3<f32>) -> f32 {
    let size = max(D.v[5].z, 0.05);
    let thick = 0.06 * D.v[6].x;
    let cell = 2.0 * size;
    var q = p;
    q.z = q.z - cell * floor(q.z / cell + 0.5);
    let tw = D.v[5].w * (cell * floor(p.z / cell + 0.5) - flight_z) / cell * 0.35;
    let xy = rot2(q.xy, tw);
    let variant = i32(D.v[5].x + 0.5);
    let radius = 1.5 * size;
    var d2 = 0.0;
    switch variant {
        case 1: {
            let a = abs(xy) - vec2<f32>(radius);
            d2 = abs(max(a.x, a.y));
        }
        case 2: {
            let ang = atan2(xy.y, xy.x) + PI * 0.5;
            let seg = TAU / 3.0;
            let loc = (ang - floor(ang / seg) * seg) - seg * 0.5;
            d2 = abs(length(xy) * cos(loc) - radius * 0.6);
        }
        default: {
            d2 = abs(length(xy) - radius);
        }
    }
    return length(vec2<f32>(d2, q.z)) - thick;
}

fn bg_rings(rd_in: vec3<f32>, speed: f32, ca: vec3<f32>, cb: vec3<f32>, cc: vec3<f32>) -> vec3<f32> {
    let size = max(D.v[5].z, 0.05);
    let period = 16.0 * size;
    // Wrapped, so the last frame of the loop is exactly the first.
    let z0 = -fract(G.time.x * speed) * period;
    let a = fract(G.time.x * speed) * TAU;
    let bend = D.v[6].y;
    let ro = vec3<f32>(sin(a) * 0.4 * bend * size, cos(a) * 0.3 * bend * size, z0);
    flight_z = z0;
    let rd = flight_dir(rd_in);
    let steps = steps_or(64);
    var t = 0.0;
    var glow = 0.0;
    var hit = false;
    for (var i = 0; i < steps; i = i + 1) {
        let p = ro + rd * t;
        let d = ring_sdf(p);
        // Colour of the glow alternates between the two colours per ring.
        glow = glow + exp(-d * 12.0 / size) * 0.03;
        if (d < 0.001) {
            hit = true;
            break;
        }
        t = t + d * 0.9;
        if (t > 50.0 * size) {
            break;
        }
    }
    let p = ro + rd * t;
    let k = 0.5 + 0.5 * cos(PI * floor(p.z / (2.0 * size) + 0.5));
    let ring_col = mix(cb, cc, k);
    let fog = exp(-t / size * 0.05 * D.v[6].w);
    var col = ca + ring_col * glow * (1.0 + D.v[6].z);
    if (hit) {
        col = mix(ca, ring_col * 2.0, fog);
    }
    return col;
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
    var rd = view_ray(in.ndc);
    let kind = i32(D.v[0].x + 0.5);
    // Raymarched kinds can roll the view a whole number of turns per loop.
    let roll = D.v[7].y;
    if (roll != 0.0 && (kind == 3 || kind == 4 || kind >= 7)) {
        let cam_fwd = cross(G.cam_up.xyz, G.cam_right.xyz);
        let x = dot(rd, G.cam_right.xyz);
        let y = dot(rd, G.cam_up.xyz);
        let r = rot2(vec2<f32>(x, y), roll);
        rd = normalize(G.cam_right.xyz * r.x + G.cam_up.xyz * r.y + cam_fwd * dot(rd, cam_fwd));
    }
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
        case 7: {
            col = bg_sponge(rd, speed, ca, cb, cc);
        }
        case 8: {
            col = bg_rings(rd, speed, ca, cb, cc);
        }
        default: {}
    }
    return vec4<f32>(max(col * intensity, vec3<f32>(0.0)), 1.0);
}
