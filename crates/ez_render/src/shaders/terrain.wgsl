// Scrolling landscape, generated entirely from the vertex index.
// The height field is periodic noise and the scroll offset wraps after one
// full period, so the terrain loops seamlessly.
// D.v[0]: size, cells, height, hills (period of the noise)
// D.v[1]: roughness, scroll offset (0..1, fraction of the terrain), valley, style
// D.v[2]: line colour * glow, seed
// D.v[3]: fill colour, has texture
// D.v[4]: texture tiles across the terrain, texture on lines
// D.v[5]: shape, biome, liquid kind (0 none), liquid level (height units)
// D.v[6]: liquid colour, liquid glow
// D.v[7]: waves, flow offset (0..1), time angle (whole turns per bar), _
// D.v[8..11]: layer model matrix
// D.v[12]: level of detail: focus u, focus w (0..1), grading kx, kz
// D.v[13]: grid index at the focus x, z; drawn cells (0 = uniform grid), _
@group(2) @binding(0) var t_tex: texture_2d<f32>;
@group(2) @binding(1) var s_tex: sampler;

struct TOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Grid coordinates that move with the landscape (lines sit on integers).
    @location(2) grid: vec2<f32>,
    // 0 at the terrain border, 1 well inside.
    @location(3) border: f32,
    // Ground height before the liquid fills it (local units).
    @location(4) hraw: f32,
    // Steepness of the ground: 0 flat, 1 vertical.
    @location(5) slope: f32,
};

fn t_hash(x: i32, y: i32, seed: u32) -> f32 {
    return hash1(hash_u(u32(x) * 0x8da6b343u) ^ hash_u(u32(y) * 0xd8163841u) ^ seed);
}

// Value noise that repeats every `period` cells in x and y.
fn t_noise(p: vec2<f32>, period: i32, seed: u32) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = p - floor(p);
    let s = f * f * (3.0 - 2.0 * f);
    let x0 = ((i.x % period) + period) % period;
    let y0 = ((i.y % period) + period) % period;
    let x1 = (x0 + 1) % period;
    let y1 = (y0 + 1) % period;
    let a = t_hash(x0, y0, seed);
    let b = t_hash(x1, y0, seed);
    let c = t_hash(x0, y1, seed);
    let d = t_hash(x1, y1, seed);
    return mix(mix(a, b, s.x), mix(c, d, s.x), s.y);
}

// Periodic fractal noise (0..1) with `octaves` octaves starting at `period`.
fn t_fbm(u: f32, w: f32, period_in: i32, rough: f32, seed: u32, octaves: i32) -> f32 {
    var sum = 0.0;
    var amp = 1.0;
    var norm = 0.0;
    var period = period_in;
    for (var o = 0; o < octaves; o = o + 1) {
        let p = vec2<f32>(u, w) * f32(period);
        sum = sum + t_noise(p, period, seed + u32(o) * 101u) * amp;
        norm = norm + amp;
        amp = amp * rough;
        period = period * 2;
    }
    return sum / max(norm, 1e-4);
}

// Ridged noise: sharp crests where the noise crosses its middle.
fn t_ridged(u: f32, w: f32, period_in: i32, rough: f32, seed: u32) -> f32 {
    var sum = 0.0;
    var amp = 1.0;
    var norm = 0.0;
    var period = period_in;
    var prev = 1.0;
    for (var o = 0; o < 4; o = o + 1) {
        let p = vec2<f32>(u, w) * f32(period);
        var r = 1.0 - abs(t_noise(p, period, seed + u32(o) * 101u) * 2.0 - 1.0);
        r = r * r;
        sum = sum + r * amp * prev;
        prev = mix(1.0, r, 0.6);
        norm = norm + amp;
        amp = amp * max(rough, 0.25);
        period = period * 2;
    }
    return sum / max(norm, 1e-4);
}

// Craters on a grid of `period` cells (periodic), 3x3 neighbourhood.
fn t_craters(u: f32, w: f32, period: i32, seed: u32) -> f32 {
    let p = vec2<f32>(u, w) * f32(period);
    let c = floor(p);
    var h = 0.0;
    for (var j = -1; j <= 1; j = j + 1) {
        for (var i = -1; i <= 1; i = i + 1) {
            let cell = c + vec2<f32>(f32(i), f32(j));
            let cx = ((i32(cell.x) % period) + period) % period;
            let cy = ((i32(cell.y) % period) + period) % period;
            let r0 = t_hash(cx, cy, seed + 7u);
            if (r0 < 0.25) {
                continue;
            }
            let centre = cell + vec2<f32>(t_hash(cx, cy, seed + 11u), t_hash(cx, cy, seed + 13u)) * 0.6 + 0.2;
            let rad = 0.2 + 0.45 * t_hash(cx, cy, seed + 17u);
            let d = length(p - centre) / rad;
            let bowl = select(0.0, (d * d - 1.0) * 0.7, d < 1.0);
            let rim = exp(-(d - 1.0) * (d - 1.0) * 14.0) * 0.35;
            h = h + (bowl + rim) * rad;
        }
    }
    return h;
}

// Terrain height at normalised coordinates (u across, w along, both 0..1
// over the whole terrain before scrolling).
fn t_height(u: f32, w: f32) -> f32 {
    let height = D.v[0].z;
    let hills = max(i32(D.v[0].w + 0.5), 1);
    let rough = clamp(D.v[1].x, 0.0, 1.0);
    let valley = D.v[1].z;
    let seed = u32(D.v[2].w);
    let shape = i32(D.v[5].x + 0.5);
    var h = 0.0;
    switch shape {
        case 1: {
            // Ridged mountains.
            let r = t_ridged(u, w, hills, rough, seed);
            h = pow(r, 1.6) * 1.5;
        }
        case 2: {
            // Mesas: flat terraces with steep steps.
            let f = t_fbm(u, w, hills, rough * 0.6, seed, 3);
            let q = f * f * 1.6 * 5.0;
            h = (floor(q) + smoothstep(0.65, 1.0, fract(q))) / 5.0;
            h = h + t_fbm(u, w, hills * 8, 0.5, seed + 3u, 1) * 0.03 * rough;
        }
        case 3: {
            // Dunes: asymmetric waves (gentle windward, steep lee side),
            // bent by noise. Whole waves across the terrain keep it periodic.
            let warp = t_fbm(u, w, hills, 0.5, seed, 2);
            let bend = sin(u * TAU * f32(hills)) * 0.6 + sin((u + w) * TAU * f32(hills * 2)) * 0.25;
            let x = w * f32(hills * 3) + warp * 4.0 + bend;
            let f = fract(x);
            let crest = select((1.0 - f) / 0.25, f / 0.75, f < 0.75);
            let prof = smoothstep(0.0, 1.0, crest);
            h = prof * (0.2 + 0.6 * t_fbm(u, w, hills * 2, 0.5, seed + 5u, 2));
            h = h + t_fbm(u, w, hills * 4, rough, seed + 9u, 2) * 0.12 * rough;
        }
        case 4: {
            // Canyons carved into a plateau along noise contours.
            let f = t_fbm(u, w, hills, 0.45, seed, 3);
            let river = abs(f - 0.5);
            let cut = smoothstep(0.015, 0.09 + 0.05 * rough, river);
            let q = cut * 3.0;
            let steps = (floor(q) + smoothstep(0.6, 1.0, fract(q))) / 3.0;
            h = 0.05 + 0.85 * steps + t_fbm(u, w, hills * 4, 0.5, seed + 3u, 2) * 0.12 * rough;
        }
        case 5: {
            // Craters on gently rolling ground.
            let f = t_fbm(u, w, hills, rough, seed, 3);
            h = 0.35 + f * 0.35 + t_craters(u, w, hills * 2, seed);
            h = max(h, 0.0);
        }
        default: {
            let f = t_fbm(u, w, hills, rough, seed, 4);
            h = f * f * 1.6;
        }
    }
    // Flatten a valley down the middle.
    if (valley > 0.0) {
        let x = abs(u - 0.5) * 2.0;
        h = h * smoothstep(valley * 0.5, valley * 0.5 + 0.35, x);
    }
    return h * height;
}

fn lod_position(i: f32, n: f32, focus: f32, k: f32, s0: f32) -> f32 {
    let x = i - s0;
    var d = abs(x) / n;
    if (k >= 1e-4) {
        d = (exp(abs(x) * k / n) - 1.0) / k;
    }
    return clamp(focus + sign(x) * d, 0.0, 1.0);
}

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> TOut {
    let size = D.v[0].x;
    let cells = max(u32(D.v[0].y + 0.5), 1u);
    let scroll = D.v[1].y;
    let cell = vid / 6u;
    let corner = vid % 6u;
    var o = vec2<u32>(0u, 0u);
    switch corner {
        case 1u, 3u: { o = vec2<u32>(1u, 0u); }
        case 2u, 5u: { o = vec2<u32>(0u, 1u); }
        case 4u: { o = vec2<u32>(1u, 1u); }
        default: {}
    }
    let n = f32(cells);
    var g = vec2<f32>(f32(cell % cells + o.x), f32(cell / cells + o.y));
    let drawn = u32(D.v[13].z + 0.5);
    if (drawn > 0u) {
        // Graded grid: full resolution at the focus, coarser away from it
        // (see ez_core::scene::lod_grading).
        let i = vec2<f32>(f32(cell % drawn + o.x), f32(cell / drawn + o.y));
        g = vec2<f32>(
            lod_position(i.x, n, D.v[12].x, D.v[12].z, D.v[13].x),
            lod_position(i.y, n, D.v[12].y, D.v[12].w, D.v[13].y)
        ) * n;
    }
    let u = g.x / n;
    // The landscape moves towards +z: sample further back as time passes.
    let w = g.y / n - scroll;
    let h = t_height(u, w);
    let e = 1.0 / n;
    let hx = t_height(u + e, w) - t_height(u - e, w);
    let hz = t_height(u, w + e) - t_height(u, w - e);
    let step = size / n;
    var local_n = normalize(vec3<f32>(-hx, 2.0 * step, -hz));
    let slope = 1.0 - local_n.y;
    // Liquids fill everything below their level with a flat surface.
    var top = h;
    if (D.v[5].z > 0.5 && h < D.v[5].w) {
        top = D.v[5].w;
        local_n = vec3<f32>(0.0, 1.0, 0.0);
    }
    // Sit a hair above the layer's base so a mirror floor at the same
    // height doesn't z-fight with flat valleys.
    let local = vec3<f32>((u - 0.5) * size, top + 0.03, (g.y / n - 0.5) * size);

    let model = mat4x4<f32>(D.v[8], D.v[9], D.v[10], D.v[11]);
    let world = model * vec4<f32>(local, 1.0);
    var out: TOut;
    out.pos = G.view_proj * world;
    out.world = world.xyz;
    out.normal = normalize((model * vec4<f32>(local_n, 0.0)).xyz);
    out.grid = vec2<f32>(g.x, g.y - scroll * n);
    let edge = min(min(g.x, n - g.x), min(g.y, n - g.y));
    out.border = clamp(edge / max(n * 0.12, 1.0), 0.0, 1.0);
    out.hraw = h;
    out.slope = slope;
    return out;
}

// Periodic 2D value noise in the liquid's coordinates (0..1 across the
// terrain, scrolling with it); `period` cells across.
fn l_noise(q: vec2<f32>, period: i32, seed: u32) -> f32 {
    return t_noise(q * f32(period), period, seed);
}

// Offsets that move the liquid's patterns, both loop-safe: a small circle
// (whole turns per bar), and the current, a whole number of drifts along
// the terrain per loop (only ever scaled by whole numbers).
fn l_circle(period: i32, radius_cells: f32) -> vec2<f32> {
    let a = D.v[7].z;
    return vec2<f32>(cos(a), sin(a)) * radius_cells / f32(period);
}

fn l_flow() -> vec2<f32> {
    return vec2<f32>(0.0, -D.v[7].y);
}

// Ripples: gradient of two layers of periodic noise.
fn l_ripple(q: vec2<f32>, base: i32, seed: u32) -> vec2<f32> {
    let e = 0.25 / f32(base * 4);
    let c = l_circle(base, 1.0);
    let q1 = q + c + l_flow();
    let q2 = q + vec2<f32>(-c.y, c.x) * 0.7 + l_flow() * 2.0;
    let a = l_noise(q1, base, seed);
    let ax = l_noise(q1 + vec2<f32>(e, 0.0), base, seed);
    let az = l_noise(q1 + vec2<f32>(0.0, e), base, seed);
    let b = l_noise(q2, base * 4, seed + 3u);
    let bx = l_noise(q2 + vec2<f32>(e, 0.0), base * 4, seed + 3u);
    let bz = l_noise(q2 + vec2<f32>(0.0, e), base * 4, seed + 3u);
    return (vec2<f32>(ax - a, az - a) + vec2<f32>(bx - b, bz - b) * 0.5) / e * 0.004;
}

// Sky seen in a reflection: fog colour at the horizon, sky light above.
fn l_sky(r: vec3<f32>) -> vec3<f32> {
    let up = smoothstep(-0.05, 0.6, r.y);
    return mix(G.fog.rgb * 1.1 + G.sky.rgb * 0.1, G.sky.rgb * 0.9 + G.fog.rgb * 0.2, up);
}

fn shade_liquid(in: TOut, depth: f32, q: vec2<f32>) -> vec3<f32> {
    let kind = i32(D.v[5].z + 0.5);
    let lcol = D.v[6].rgb;
    let glow = max(D.v[6].w, 0.0);
    let waves = max(D.v[7].x, 0.0);
    let hills = max(i32(D.v[0].w + 0.5), 1);
    let seed = u32(D.v[2].w) + 991u;
    let v = normalize(G.cam_pos.xyz - in.world);
    let l = normalize(G.light_dir.xyz);
    let ambient = mix(G.ground.rgb, G.sky.rgb, 0.8) * G.sky.w;
    let sun = G.light_color.rgb * G.ground.w * sun_shadow(in.world, vec3<f32>(0.0, 1.0, 0.0));
    let base_p = hills * 6;
    switch kind {
        case 2: {
            // Lava: glowing rivers under a cracked, drifting crust.
            let c = l_circle(hills * 2, 0.35);
            let f1 = l_noise(q + c + l_flow(), hills * 3, seed);
            let f2 = l_noise(q + c * 1.7 + l_flow() * 2.0, hills * 12, seed + 5u);
            let f3 = l_noise(q - c * 0.5 + l_flow(), hills * 24, seed + 9u);
            let crust_n = f1 * 0.55 + f2 * 0.3 + f3 * 0.15;
            let crust = smoothstep(0.42, 0.52, crust_n) * clamp(waves, 0.0, 1.0);
            let hot = 1.0 - crust;
            let pulse = 0.85 + 0.15 * sin(D.v[7].z * 2.0 + f1 * 12.0);
            let core = mix(lcol, vec3<f32>(1.0, 0.75, 0.3), smoothstep(0.32, 0.15, crust_n) * 0.5);
            let emit = core * glow * (0.3 + 1.6 * hot * hot) * pulse;
            let crust_col = vec3<f32>(0.05, 0.03, 0.03) * (ambient + sun * max(l.y, 0.0));
            // Dark crust edges still glow a little.
            let edge = smoothstep(0.58, 0.46, crust_n) * crust;
            return mix(emit, crust_col + lcol * glow * edge * 0.8, crust);
        }
        case 3: {
            // Toxic goo: glowing, slowly swirling, with popping bubbles.
            let c = l_circle(hills * 2, 0.6);
            let f = l_noise(q + c + l_flow(), hills * 5, seed) * 0.6 + l_noise(q - c + l_flow(), hills * 17, seed + 2u) * 0.4;
            var col = lcol * glow * (0.35 + 0.9 * smoothstep(0.35, 0.8, f));
            // Bubbles: cells with a ring that grows and pops, loop-safe.
            let bp = f32(hills * 20);
            let cell = floor(q * bp);
            let bn = hills * 20;
            let cx = ((i32(cell.x) % bn) + bn) % bn;
            let cy = ((i32(cell.y) % bn) + bn) % bn;
            let r0 = t_hash(cx, cy, seed + 31u);
            if (r0 < 0.35 * clamp(waves, 0.0, 2.0)) {
                let life = fract(D.v[7].z / TAU + r0 * 7.0);
                let centre = cell + 0.5;
                let d = length(q * bp - centre);
                let rr = life * 0.45;
                let ring = smoothstep(0.08, 0.0, abs(d - rr)) * (1.0 - life);
                col = col + mix(lcol, vec3<f32>(1.0), 0.5) * ring * glow * 2.0;
            }
            // A bit of gloss on top.
            let r = reflect(-v, vec3<f32>(0.0, 1.0, 0.0));
            let fres = 0.03 + 0.5 * pow(1.0 - max(v.y, 0.0), 5.0);
            return col + l_sky(r) * fres + sun * pow(max(dot(r, l), 0.0), 120.0) * 0.8;
        }
        case 4: {
            // Ice: pale, cracked and shiny, frozen still.
            let c1 = abs(l_noise(q, hills * 6, seed) - 0.5);
            let c2 = abs(l_noise(q, hills * 18, seed + 4u) - 0.5);
            let crack = max(smoothstep(0.025, 0.0, c1), smoothstep(0.02, 0.0, c2) * 0.6) * min(waves, 2.0) * 0.5;
            let n = vec3<f32>(0.0, 1.0, 0.0);
            let deep = mix(lcol * 0.35, lcol, smoothstep(0.0, 1.5, 1.5 - depth) * 0.5 + 0.5);
            var col = deep * (ambient + sun * max(dot(n, l), 0.0) * 0.6);
            col = mix(col, vec3<f32>(0.95, 0.98, 1.0) * (ambient + sun * 0.5), crack);
            let r = reflect(-v, n);
            let fres = 0.05 + 0.6 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
            col = col + l_sky(r) * fres * glow + sun * pow(max(dot(r, l), 0.0), 200.0) * glow * 0.5;
            return col;
        }
        default: {
            // Water: fresnel reflection of the sky, sun glint, shallow tint
            // and foam at the shore.
            let rip = l_ripple(q, base_p, seed) * waves;
            let n = normalize(vec3<f32>(rip.x, 1.0, rip.y));
            let r = reflect(-v, n);
            let fres = 0.02 + 0.98 * pow(1.0 - max(dot(n, v), 0.0), 5.0);
            let shallow = 1.0 - smoothstep(0.0, 1.2, depth);
            let body = mix(lcol, lcol * 2.2 + vec3<f32>(0.02, 0.06, 0.05), shallow * 0.7)
                * (ambient + sun * max(l.y, 0.0) * 0.35);
            var col = mix(body, l_sky(r), clamp(fres * glow, 0.0, 1.0));
            col = col + sun * pow(max(dot(r, l), 0.0), 400.0) * 8.0 * glow;
            col = col + sun * pow(max(dot(r, l), 0.0), 40.0) * 0.15 * glow;
            let fm = l_noise(q + l_circle(hills * 4, 0.5) + l_flow(), hills * 40, seed + 17u);
            let foam = smoothstep(0.18, 0.0, depth) * smoothstep(0.3, 0.7, fm + (0.18 - depth) * 3.0) * min(waves, 1.0);
            col = mix(col, vec3<f32>(0.9) * (ambient + sun * max(l.y, 0.0)), foam * 0.8);
            return col;
        }
    }
}

// Ground colour for a biome from height (0..1 of the mountain height),
// steepness and a little noise. `emit` receives glowing parts.
fn biome_color(biome: i32, hn: f32, slope: f32, nv: f32, shore: f32, emit: ptr<function, vec3<f32>>) -> vec3<f32> {
    let steep = smoothstep(0.25, 0.5, slope + (nv - 0.5) * 0.15);
    switch biome {
        case 1: {
            // Alpine
            let grass = mix(vec3<f32>(0.10, 0.22, 0.05), vec3<f32>(0.20, 0.30, 0.08), nv);
            let rock = mix(vec3<f32>(0.22, 0.20, 0.18), vec3<f32>(0.32, 0.30, 0.27), nv);
            let snow = vec3<f32>(0.92, 0.95, 1.0);
            var c = mix(grass, rock, max(steep, smoothstep(0.45, 0.6, hn + nv * 0.1)));
            let snow_k = smoothstep(0.62, 0.72, hn + nv * 0.12) * (1.0 - smoothstep(0.45, 0.7, slope));
            c = mix(c, snow, snow_k);
            return mix(c, vec3<f32>(0.62, 0.55, 0.38), shore);
        }
        case 2: {
            // Desert: sand flats, banded red rock cliffs.
            let sand = mix(vec3<f32>(0.72, 0.52, 0.30), vec3<f32>(0.82, 0.64, 0.40), nv);
            let band = 0.5 + 0.5 * sin(hn * 40.0 + nv * 2.0);
            let rock = mix(vec3<f32>(0.45, 0.18, 0.08), vec3<f32>(0.62, 0.32, 0.16), band);
            return mix(sand, rock, steep);
        }
        case 3: {
            // Volcanic: black basalt, grey ash on top, glowing cracks low down.
            let basalt = mix(vec3<f32>(0.03, 0.028, 0.03), vec3<f32>(0.08, 0.07, 0.07), nv);
            let ash = vec3<f32>(0.22, 0.21, 0.2);
            let c = mix(basalt, ash, smoothstep(0.6, 0.8, hn) * (1.0 - steep));
            let crack = smoothstep(0.03, 0.0, abs(nv - 0.5)) * (1.0 - smoothstep(0.15, 0.5, hn));
            *emit = *emit + vec3<f32>(1.0, 0.25, 0.03) * crack * 2.0;
            return c;
        }
        case 4: {
            // Arctic: snow on flats, blue ice on slopes, dark rock on cliffs.
            let snow = mix(vec3<f32>(0.85, 0.9, 0.96), vec3<f32>(0.95, 0.97, 1.0), nv);
            let ice = vec3<f32>(0.35, 0.58, 0.75);
            let rock = vec3<f32>(0.12, 0.13, 0.15);
            var c = mix(snow, ice, steep);
            c = mix(c, rock, smoothstep(0.55, 0.75, slope));
            return c;
        }
        case 5: {
            // Alien: purple moss, teal crystal slopes, glowing tips.
            let moss = mix(vec3<f32>(0.18, 0.03, 0.22), vec3<f32>(0.3, 0.06, 0.35), nv);
            let crystal = vec3<f32>(0.03, 0.4, 0.38);
            let c = mix(moss, crystal, steep);
            let tip = smoothstep(0.7, 0.9, hn + nv * 0.15);
            *emit = *emit + vec3<f32>(1.0, 0.2, 0.7) * tip * 1.5;
            return c;
        }
        default: {
            return D.v[3].rgb;
        }
    }
}

@fragment
fn fs_main(in: TOut) -> @location(0) vec4<f32> {
    // Derivatives first (uniform control flow).
    let fw = max(fwidth(in.grid), vec2<f32>(1e-4));
    let d = abs(fract(in.grid - 0.5) - 0.5) / fw;
    var line = 1.0 - clamp(min(d.x, d.y) - 0.5, 0.0, 1.0);
    let style = i32(D.v[1].w + 0.5);
    // The texture moves with the landscape; whole tiles keep the loop seamless.
    let uv = in.grid / max(D.v[0].y, 1.0) * D.v[4].x;
    let texel = textureSample(t_tex, s_tex, uv).rgb;
    let has_tex = D.v[3].w > 0.5;
    let liquid_kind = i32(D.v[5].z + 0.5);
    let depth = D.v[5].w - in.hraw;
    let in_liquid = liquid_kind > 0 && depth > 0.0;

    if (!clip_visible(in.world)) {
        discard;
    }
    // Wireframe: only the lines are drawn (and liquids).
    if (style == 0 && line < 0.05 && !in_liquid) {
        discard;
    }
    let dist = length(G.cam_pos.xyz - in.world);
    // Liquid coordinates: 0..1 across the terrain, scrolling with it.
    let q = in.grid / max(D.v[0].y, 1.0);
    if (in_liquid) {
        var col = shade_liquid(in, depth, q);
        // Grid lines shine faintly through the surface.
        col = col + D.v[2].rgb * line * in.border * select(0.15, 0.0, style == 1);
        return vec4<f32>(apply_fog_at(col, in.world), 1.0);
    }
    var glow = D.v[2].rgb * line * in.border;
    if (has_tex && D.v[4].y > 0.5) {
        glow = glow * texel * 1.5;
    }
    // Heat from lava and goo lights up the nearby shore.
    var emit = vec3<f32>(0.0);
    if (liquid_kind == 2 || liquid_kind == 3) {
        let above = max(-depth, 0.0);
        let heat = exp(-above * 2.5) * max(D.v[6].w, 0.0);
        emit = D.v[6].rgb * heat * select(0.6, 0.3, liquid_kind == 3);
    }
    var col = vec3<f32>(0.0);
    if (style == 0) {
        col = glow + emit * 0.5;
    } else {
        var n = normalize(in.normal);
        let v = normalize(G.cam_pos.xyz - in.world);
        if (dot(n, v) < 0.0) {
            n = -n;
        }
        let l = normalize(G.light_dir.xyz);
        let diffuse = G.light_color.rgb * G.ground.w * max(dot(n, l), 0.0) * sun_shadow(in.world, n);
        let ambient = mix(G.ground.rgb, G.sky.rgb, n.y * 0.5 + 0.5) * G.sky.w;
        let biome = i32(D.v[5].y + 0.5);
        var ground = D.v[3].rgb;
        if (biome > 0) {
            let hn = in.hraw / max(D.v[0].z, 1e-3);
            let hills = max(i32(D.v[0].w + 0.5), 1);
            let nv = l_noise(q, hills * 16, u32(D.v[2].w) + 77u);
            var shore = 0.0;
            if (liquid_kind == 1) {
                shore = smoothstep(0.35, 0.05, -depth) * (1.0 - smoothstep(0.3, 0.6, in.slope));
            }
            ground = biome_color(biome, hn, in.slope, nv, shore, &emit);
        }
        if (has_tex) {
            ground = ground * texel;
        }
        // Wet, darker sand just above the water line.
        if (liquid_kind == 1) {
            ground = ground * mix(1.0, 0.55, smoothstep(0.25, 0.0, -depth));
        }
        // Rain darkens the ground; snow settles on the flatter parts.
        ground = ground * (1.0 - 0.4 * G.caus_col.w);
        let snow = snow_cover(in.world, n);
        ground = mix(ground, vec3<f32>(0.88, 0.91, 0.96), snow);
        col = ground * (diffuse + ambient + caustic_light(in.world, n)) + emit;
        // Puddles mirror the sky.
        let pud = puddle(in.world, n) * (1.0 - snow);
        if (pud > 0.0) {
            let r = reflect(-v, vec3<f32>(0.0, 1.0, 0.0));
            let fres = 0.1 + 0.9 * pow(1.0 - max(v.y, 0.0), 4.0);
            let sky_r = mix(G.sky.rgb, G.fog.rgb, 0.4 - 0.4 * smoothstep(0.0, 0.5, r.y)) * 1.8;
            let sheen = sky_r * fres + G.light_color.rgb * G.ground.w * pow(max(dot(r, l), 0.0), 300.0) * 1.5;
            col = mix(col, col * 0.4 + sheen * (1.0 + rain_rings(in.world)), pud);
        }
        if (style == 2) {
            col = col + glow;
        }
    }
    return vec4<f32>(apply_fog_at(col, in.world), 1.0);
}
