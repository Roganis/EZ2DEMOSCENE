// Weather: rain, snow, embers, dust or fireflies in a box that follows the
// camera, plus lightning bolts. Like the particles, every drop is an
// analytic function of (id, loop phase), and each falls a whole number of
// times per loop, so the weather loops and scrubs exactly.
// D.v[0]: kind, drop count, falls per loop, seed
// D.v[1]: colour, size
// D.v[2]: wind offset per unit of fall (xz), intensity, streak length
// D.v[3]: half size of the box, height, ground y, splash fraction
// D.v[4]: bolt top (xyz), bolt brightness (0 = no bolt)
// D.v[5]: bolt bottom (xyz), bolt seed
// D.v[6]: bolt colour, _
// D.v[7]: wind direction (xz unit), _, _

struct WOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) quad: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world: vec3<f32>,
    // 0 round sprite, 1 streak, 2 splash ring, 3 bolt
    @location(3) @interpolate(flat) shape: u32,
};

const BOLT_MAIN: u32 = 28u;
const BOLT_BRANCH: u32 = 14u;

// Wrap into [-a, a): drops tile space around the camera.
fn wrapc(x: f32, a: f32) -> f32 {
    return x - 2.0 * a * floor((x + a) / (2.0 * a));
}

// Smooth 1D value noise for the bolt's zigzag.
fn noise1(x: f32, seed: u32) -> f32 {
    let i = floor(x);
    let f = x - i;
    let a = hash2u(u32(i32(i) + 1024), seed);
    let b = hash2u(u32(i32(i) + 1025), seed);
    return mix(a, b, f * f * (3.0 - 2.0 * f)) - 0.5;
}

// Point `t` (0 top .. 1 bottom) along a jagged bolt.
fn bolt_point(top: vec3<f32>, bottom: vec3<f32>, t: f32, seed: u32, jag: f32) -> vec3<f32> {
    let axis = bottom - top;
    let len = length(axis);
    let side = normalize(cross(axis, vec3<f32>(0.3, 0.1, 1.0)));
    let side2 = normalize(cross(axis, side));
    let d1 = noise1(t * 6.0, seed) * 0.5 + noise1(t * 17.0, seed + 1u) * 0.3 + noise1(t * 43.0, seed + 2u) * 0.15;
    let d2 = noise1(t * 5.0, seed + 3u) * 0.5 + noise1(t * 19.0, seed + 4u) * 0.3;
    let env = sqrt(clamp(t * 4.0, 0.0, 1.0));
    return top + axis * t + (side * d1 + side2 * d2) * len * jag * env;
}

struct Seg {
    a: vec3<f32>,
    b: vec3<f32>,
    width: f32,
    bright: f32,
};

fn bolt_segment(k: u32) -> Seg {
    let top = D.v[4].xyz;
    let bottom = D.v[5].xyz;
    let seed = u32(D.v[5].w);
    var s: Seg;
    if (k < BOLT_MAIN) {
        let t0 = f32(k) / f32(BOLT_MAIN);
        let t1 = f32(k + 1u) / f32(BOLT_MAIN);
        s.a = bolt_point(top, bottom, t0, seed, 0.18);
        s.b = bolt_point(top, bottom, t1, seed, 0.18);
        s.width = 0.14;
        s.bright = 1.0;
    } else {
        // One branch splitting off the main bolt and fading out.
        let j = k - BOLT_MAIN;
        let start_t = 0.25 + hash2u(seed, 5u) * 0.3;
        let root = bolt_point(top, bottom, start_t, seed, 0.18);
        let axis = bottom - top;
        let out = normalize(cross(axis, vec3<f32>(0.0, 0.0, 1.0)) * select(-1.0, 1.0, hash2u(seed, 6u) > 0.5));
        let end = root + axis * 0.4 + out * length(axis) * 0.25;
        let t0 = f32(j) / f32(BOLT_BRANCH);
        let t1 = f32(j + 1u) / f32(BOLT_BRANCH);
        s.a = bolt_point(root, end, t0, seed + 9u, 0.25);
        s.b = bolt_point(root, end, t1, seed + 9u, 0.25);
        s.width = 0.09 * (1.0 - t0 * 0.6);
        s.bright = 0.6 * (1.0 - t0);
    }
    return s;
}

// Camera-facing quad along the segment a → b, `width` wide.
fn along(a: vec3<f32>, b: vec3<f32>, width: f32, c: vec2<f32>) -> vec3<f32> {
    let mid = (a + b) * 0.5;
    let ax = b - a;
    let to_cam = normalize(G.cam_pos.xyz - mid);
    var side = cross(ax, to_cam);
    if (dot(side, side) < 1e-10) {
        side = G.cam_right.xyz;
    }
    side = normalize(side);
    return mid + ax * 0.5 * c.y * 1.15 + side * width * c.x;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> WOut {
    let c = quad_corner(vi);
    var out: WOut;
    out.quad = c;
    let count = u32(D.v[0].y + 0.5);
    if (ii >= count) {
        // Lightning bolt.
        let s = bolt_segment(ii - count);
        let wp = along(s.a, s.b, s.width, c);
        out.pos = G.view_proj * vec4<f32>(wp, 1.0);
        out.world = wp;
        out.color = D.v[6].rgb * D.v[4].w * s.bright * 6.0;
        out.shape = 3u;
        return out;
    }

    let kind = i32(D.v[0].x + 0.5);
    let falls = max(D.v[0].z, 1.0);
    let seed = u32(D.v[0].w);
    let size = D.v[1].w;
    let wind = D.v[2].xy;
    let area = max(D.v[3].x, 0.5);
    let height = max(D.v[3].y, 0.1);
    let ground = D.v[3].z;
    let id = ii;
    let h0 = hash2u(id, seed);
    // A third of the drops fall one extra time per loop (still whole).
    let f = falls + select(0.0, 1.0, hash2u(id, seed + 9u) > 0.66 && kind == 0);
    let x = G.time.x * f + h0;
    let life = fract(x);
    let cycle = u32(floor(x) - floor(x / f) * f);
    let h1 = hash2u(id * 7u + cycle * 131u, seed + 1u);
    let h2 = hash2u(id * 13u + cycle * 71u, seed + 2u);
    let h3 = hash2u(id * 17u + cycle * 37u, seed + 3u);
    var base = vec3<f32>((h1 * 2.0 - 1.0) * area, 0.0, (h2 * 2.0 - 1.0) * area);
    var alpha = 1.0;
    var wp = vec3<f32>(0.0);
    var shape = 0u;
    let cam = G.cam_pos.xyz;

    switch kind {
        case 0: {
            // Rain: falls during 88% of its life, then splashes.
            let fall_t = life / 0.88;
            if (fall_t < 1.0) {
                let drop = height * (1.0 - fall_t);
                let p = base + vec3<f32>(wind.x, 0.0, wind.y) * (height - drop);
                let rx = wrapc(p.x - cam.x, area);
                let rz = wrapc(p.z - cam.z, area);
                let world = vec3<f32>(cam.x + rx, ground + drop, cam.z + rz);
                let axis = normalize(vec3<f32>(wind.x, -1.0, wind.y));
                let len = size * 20.0 * D.v[2].w;
                let tail = world - axis * len;
                wp = along(world, tail, size * 0.35, c);
                alpha = 1.0 - smoothstep(0.75 * area, area, max(abs(rx), abs(rz)));
                shape = 1u;
            } else {
                // Splash ring on the ground where the drop landed.
                let st = (life - 0.88) / 0.12;
                let p = base + vec3<f32>(wind.x, 0.0, wind.y) * height;
                let rx = wrapc(p.x - cam.x, area);
                let rz = wrapc(p.z - cam.z, area);
                let r = size * (1.5 + 5.0 * st);
                wp = vec3<f32>(cam.x + rx + c.x * r, ground + 0.04, cam.z + rz + c.y * r);
                let on = select(0.0, 1.0, h3 < D.v[3].w);
                alpha = on * (1.0 - st) * 0.8 * (1.0 - smoothstep(0.75 * area, area, max(abs(rx), abs(rz))));
                shape = 2u;
            }
        }
        case 1: {
            // Snow: slow fall with a lazy sway.
            let drop = height * (1.0 - life);
            let sway = TAU * (life * 2.0 + h3);
            let p = base + vec3<f32>(wind.x, 0.0, wind.y) * (height - drop)
                + vec3<f32>(sin(sway), 0.0, cos(sway * 0.7)) * 0.4;
            let rx = wrapc(p.x - cam.x, area);
            let rz = wrapc(p.z - cam.z, area);
            let world = vec3<f32>(cam.x + rx, ground + drop, cam.z + rz);
            let sz = size * (0.6 + 0.8 * h3);
            wp = world + (G.cam_right.xyz * c.x + G.cam_up.xyz * c.y) * sz;
            alpha = smoothstep(0.0, 0.05, life) * smoothstep(1.0, 0.95, life)
                * (1.0 - smoothstep(0.75 * area, area, max(abs(rx), abs(rz))));
        }
        case 2: {
            // Embers rising and flickering.
            let rise = height * life;
            let wob = TAU * (life * 3.0 + h3);
            let p = base + vec3<f32>(wind.x, 0.0, wind.y) * rise
                + vec3<f32>(sin(wob), 0.0, cos(wob * 1.3)) * 0.3;
            let rx = wrapc(p.x - cam.x, area);
            let rz = wrapc(p.z - cam.z, area);
            let world = vec3<f32>(cam.x + rx, ground + rise, cam.z + rz);
            let sz = size * (0.5 + h3) * (1.0 - life * 0.6);
            wp = world + (G.cam_right.xyz * c.x + G.cam_up.xyz * c.y) * sz;
            let flicker = 0.6 + 0.4 * sin(TAU * (life * 9.0 + h1 * 5.0));
            alpha = sin(PI * life) * flicker * (1.0 - smoothstep(0.75 * area, area, max(abs(rx), abs(rz))));
        }
        case 3: {
            // Sandstorm: soft puffs blown along the wind, low to the ground.
            let dir = D.v[7].xy;
            let y = height * 0.6 * h3 * h3;
            let p = base + vec3<f32>(dir.x, 0.0, dir.y) * life * 2.0 * area;
            let rx = wrapc(p.x - cam.x, area);
            let rz = wrapc(p.z - cam.z, area);
            let world = vec3<f32>(cam.x + rx, ground + y, cam.z + rz);
            let sz = size * (0.5 + h1);
            let ax = vec3<f32>(dir.x, 0.0, dir.y);
            wp = along(world - ax * sz * 1.5, world + ax * sz * 1.5, sz, c);
            alpha = sin(PI * life) * 0.35 * (1.0 - smoothstep(0.7 * area, area, max(abs(rx), abs(rz))));
        }
        case 4: {
            // Fireflies wandering on small loops and blinking.
            let a0 = vec3<f32>((hash2u(id, seed + 4u) * 2.0 - 1.0) * area, 0.0, (hash2u(id, seed + 5u) * 2.0 - 1.0) * area);
            let k1 = 1.0 + floor(hash2u(id, seed + 6u) * 3.0);
            let k2 = 1.0 + floor(hash2u(id, seed + 7u) * 3.0);
            let t = TAU * G.time.x;
            let wander = vec3<f32>(sin(t * k1 + h0 * 9.0), 0.0, cos(t * k2 + h0 * 5.0)) * 1.2;
            let p = a0 + wander;
            let rx = wrapc(p.x - cam.x, area);
            let rz = wrapc(p.z - cam.z, area);
            let y = 0.3 + height * 0.25 * hash2u(id, seed + 8u) + sin(t * k2 * 2.0 + h0 * 3.0) * 0.3;
            let world = vec3<f32>(cam.x + rx, ground + y, cam.z + rz);
            wp = world + (G.cam_right.xyz * c.x + G.cam_up.xyz * c.y) * size;
            let blink = pow(max(sin(TAU * (G.time.x * falls * 4.0 + h0)), 0.0), 6.0);
            alpha = (0.1 + blink) * (1.0 - smoothstep(0.75 * area, area, max(abs(rx), abs(rz))));
        }
        default: {
            alpha = 0.0;
            wp = cam;
        }
    }
    out.pos = G.view_proj * vec4<f32>(wp, 1.0);
    out.world = wp;
    out.shape = shape;
    var col = D.v[1].rgb;
    if (kind == 2) {
        col = mix(col, col * vec3<f32>(1.0, 0.35, 0.2), life);
    }
    let dist = length(cam - wp);
    let fog = 1.0 - fog_amount(dist);
    // Drops brushing past the lens would smear across the picture.
    let near = smoothstep(0.8, 2.5, dist);
    out.color = col * D.v[2].z * max(alpha, 0.0) * fog * near;
    return out;
}

@fragment
fn fs_main(in: WOut) -> @location(0) vec4<f32> {
    if (!clip_visible(in.world)) {
        discard;
    }
    let q = in.quad;
    var a = 0.0;
    switch in.shape {
        case 1u: {
            // Thin streak, brighter at the head.
            a = exp(-q.x * q.x * 6.0) * smoothstep(1.0, 0.6, abs(q.y)) * (0.5 - 0.5 * q.y);
        }
        case 2u: {
            let r = length(q);
            a = smoothstep(0.2, 0.0, abs(r - 0.75)) * step(r, 1.0);
        }
        case 3u: {
            a = exp(-q.x * q.x * 40.0) + exp(-q.x * q.x * 4.0) * 0.25;
        }
        default: {
            let r2 = dot(q, q);
            a = exp(-r2 * 4.0) * (1.0 - smoothstep(0.8, 1.0, r2));
        }
    }
    return vec4<f32>(in.color * a, 0.0);
}
