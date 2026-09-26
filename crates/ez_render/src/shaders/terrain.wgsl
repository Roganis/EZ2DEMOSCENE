// Scrolling landscape, generated entirely from the vertex index.
// The height field is periodic noise and the scroll offset wraps after one
// full period, so the terrain loops seamlessly.
// D.v[0]: size, cells, height, hills (period of the noise)
// D.v[1]: roughness, scroll offset (0..1, fraction of the terrain), valley, style
// D.v[2]: line colour * glow, seed
// D.v[3]: fill colour, has texture
// D.v[4]: texture tiles across the terrain, texture on lines
// D.v[8..11]: layer model matrix

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

// Terrain height at normalised coordinates (u across, w along, both 0..1
// over the whole terrain before scrolling).
fn t_height(u: f32, w: f32) -> f32 {
    let height = D.v[0].z;
    let hills = max(i32(D.v[0].w + 0.5), 1);
    let rough = clamp(D.v[1].x, 0.0, 1.0);
    let valley = D.v[1].z;
    let seed = u32(D.v[2].w);
    var sum = 0.0;
    var amp = 1.0;
    var norm = 0.0;
    var period = hills;
    for (var o = 0; o < 4; o = o + 1) {
        let p = vec2<f32>(u, w) * f32(period);
        sum = sum + t_noise(p, period, seed + u32(o) * 101u) * amp;
        norm = norm + amp;
        amp = amp * rough;
        period = period * 2;
    }
    var h = sum / max(norm, 1e-4);
    h = h * h * 1.6;
    // Flatten a valley down the middle.
    if (valley > 0.0) {
        let x = abs(u - 0.5) * 2.0;
        h = h * smoothstep(valley * 0.5, valley * 0.5 + 0.35, x);
    }
    return h * height;
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
    let g = vec2<f32>(f32(cell % cells + o.x), f32(cell / cells + o.y));
    let n = f32(cells);
    let u = g.x / n;
    // The landscape moves towards +z: sample further back as time passes.
    let w = g.y / n - scroll;
    let h = t_height(u, w);
    let e = 1.0 / n;
    let hx = t_height(u + e, w) - t_height(u - e, w);
    let hz = t_height(u, w + e) - t_height(u, w - e);
    let step = size / n;
    let local_n = normalize(vec3<f32>(-hx, 2.0 * step, -hz));
    // Sit a hair above the layer's base so a mirror floor at the same
    // height doesn't z-fight with flat valleys.
    let local = vec3<f32>((u - 0.5) * size, h + 0.03, (g.y / n - 0.5) * size);

    let model = mat4x4<f32>(D.v[8], D.v[9], D.v[10], D.v[11]);
    let world = model * vec4<f32>(local, 1.0);
    var out: TOut;
    out.pos = G.view_proj * world;
    out.world = world.xyz;
    out.normal = normalize((model * vec4<f32>(local_n, 0.0)).xyz);
    out.grid = vec2<f32>(g.x, g.y - scroll * n);
    let edge = min(min(g.x, n - g.x), min(g.y, n - g.y));
    out.border = clamp(edge / max(n * 0.12, 1.0), 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: TOut) -> @location(0) vec4<f32> {
    // Derivatives first (uniform control flow).
    let fw = max(fwidth(in.grid), vec2<f32>(1e-4));
    let d = abs(fract(in.grid - 0.5) - 0.5) / fw;
    let line = 1.0 - clamp(min(d.x, d.y) - 0.5, 0.0, 1.0);
    let style = i32(D.v[1].w + 0.5);
    // The texture moves with the landscape; whole tiles keep the loop seamless.
    let uv = in.grid / max(D.v[0].y, 1.0) * D.v[4].x;
    let texel = textureSample(t_tex, s_tex, uv).rgb;
    let has_tex = D.v[3].w > 0.5;

    if (!clip_visible(in.world)) {
        discard;
    }
    // Wireframe: only the lines are drawn.
    if (style == 0 && line < 0.05) {
        discard;
    }
    let dist = length(G.cam_pos.xyz - in.world);
    var glow = D.v[2].rgb * line * in.border;
    if (has_tex && D.v[4].y > 0.5) {
        glow = glow * texel * 1.5;
    }
    var col = vec3<f32>(0.0);
    if (style == 0) {
        col = glow;
    } else {
        var n = normalize(in.normal);
        let v = normalize(G.cam_pos.xyz - in.world);
        if (dot(n, v) < 0.0) {
            n = -n;
        }
        let l = normalize(G.light_dir.xyz);
        let diffuse = G.light_color.rgb * G.ground.w * max(dot(n, l), 0.0);
        let ambient = mix(G.ground.rgb, G.sky.rgb, n.y * 0.5 + 0.5) * G.sky.w;
        var ground = D.v[3].rgb;
        if (has_tex) {
            ground = ground * texel;
        }
        col = ground * (diffuse + ambient);
        if (style == 2) {
            col = col + glow;
        }
    }
    return vec4<f32>(apply_fog(col, dist), 1.0);
}
