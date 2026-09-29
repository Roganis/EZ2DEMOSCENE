// Shared declarations, prepended to every scene shader.

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    view: mat4x4<f32>,
    cam_pos: vec4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    // x: loop phase, y: beat, z: beat fraction, w: beats per loop
    time: vec4<f32>,
    // x: width, y: height, z: 1/width, w: 1/height
    res: vec4<f32>,
    // rgb: fog colour, w: density
    fog: vec4<f32>,
    // rgb: sky colour, w: ambient
    sky: vec4<f32>,
    // rgb: ground colour, w: light intensity
    ground: vec4<f32>,
    light_dir: vec4<f32>,
    light_color: vec4<f32>,
    // Reflection clip plane (xyz normal, w distance); inactive when zero.
    clip: vec4<f32>,
    // x: audio level, y: bass, z: lightning flash
    audio: vec4<f32>,
    // Height fog: density, base height, falloff, _
    hfog: vec4<f32>,
    // Caustics: amount, scale, time angle, fade-out height
    caus: vec4<f32>,
    // rgb: caustics colour, w: ground wetness (rain)
    caus_col: vec4<f32>,
    // x: snow cover, y: night (0..1), z: dusk (0..1), w: rainbow
    extra: vec4<f32>,
    // xyz: direction towards the sun (also below the horizon)
    sun: vec4<f32>,
    // Sun shadow map projection (world -> light clip space).
    shadow_vp: mat4x4<f32>,
    // x: strength (0 = off), y: softness (texels), z: texel size (uv),
    // w: normal offset (world units)
    shadow: vec4<f32>,
    // Environment map: x: intensity (0 = off, the colours light the
    // scene), y: turn around +y (radians), z: roughest mip, w: 1 when the
    // diffuse light comes from the map's roughest mip (a captured sky)
    // instead of `sh`.
    ibl: vec4<f32>,
    // Its diffuse light: 9 spherical-harmonic coefficients (rgb).
    sh: array<vec4<f32>, 9>,
};

@group(0) @binding(0) var<uniform> G: Globals;

struct Draw {
    v: array<vec4<f32>, 16>,
};

@group(1) @binding(0) var<uniform> D: Draw;

const TAU: f32 = 6.28318530718;
const PI: f32 = 3.14159265359;

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

fn hash2u(a: u32, b: u32) -> f32 {
    return hash1((a * 0x9e3779b9u) ^ hash_u(b));
}

fn hash3f(p: vec3<f32>) -> f32 {
    let q = vec3<i32>(floor(p));
    return hash1(hash_u(u32(q.x) * 0x8da6b343u) ^ hash_u(u32(q.y) * 0xd8163841u) ^ hash_u(u32(q.z) * 0xcb1ab31fu));
}

fn vnoise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash3f(i);
    let b = hash3f(i + vec3<f32>(1.0, 0.0, 0.0));
    let c = hash3f(i + vec3<f32>(0.0, 1.0, 0.0));
    let d = hash3f(i + vec3<f32>(1.0, 1.0, 0.0));
    let e = hash3f(i + vec3<f32>(0.0, 0.0, 1.0));
    let g = hash3f(i + vec3<f32>(1.0, 0.0, 1.0));
    let h = hash3f(i + vec3<f32>(0.0, 1.0, 1.0));
    let k = hash3f(i + vec3<f32>(1.0, 1.0, 1.0));
    return mix(mix(mix(a, b, u.x), mix(c, d, u.x), u.y), mix(mix(e, g, u.x), mix(h, k, u.x), u.y), u.z);
}

fn fbm3(p_in: vec3<f32>, octaves: i32) -> f32 {
    var p = p_in;
    var sum = 0.0;
    var amp = 0.5;
    for (var i = 0; i < octaves; i = i + 1) {
        sum = sum + amp * vnoise3(p);
        p = p * 2.03 + vec3<f32>(1.7, 9.2, 3.1);
        amp = amp * 0.5;
    }
    return sum;
}

// Rotation around the grey axis (matches ez_core::color::hue_rotate).
fn hue_rotate(c: vec3<f32>, turns: f32) -> vec3<f32> {
    if (turns == 0.0) {
        return c;
    }
    let a = turns * TAU;
    let s = sin(a);
    let co = cos(a);
    let k = 1.0 / 3.0;
    let sq = sqrt(k);
    let m0 = co + (1.0 - co) * k;
    let m1 = k * (1.0 - co) - sq * s;
    let m2 = k * (1.0 - co) + sq * s;
    return max(vec3<f32>(
        c.r * m0 + c.g * m1 + c.b * m2,
        c.r * m2 + c.g * m0 + c.b * m1,
        c.r * m1 + c.g * m2 + c.b * m0,
    ), vec3<f32>(0.0));
}

// Sun shadow map (bound for meshes, terrain and the mirror floor only).
@group(3) @binding(0) var t_shadow: texture_depth_2d;
@group(3) @binding(1) var s_shadow: sampler_comparison;
// The environment map (prefiltered cube: mip = roughness × roughest mip)
// and the split-sum BRDF table (n·v across, roughness down).
@group(3) @binding(2) var t_env: texture_cube<f32>;
@group(3) @binding(3) var s_env: sampler;
@group(3) @binding(4) var t_brdf: texture_2d<f32>;

// A world direction in the environment map's own frame (it turns around
// +y by G.ibl.y).
fn env_dir(d: vec3<f32>) -> vec3<f32> {
    let c = cos(G.ibl.y);
    let s = sin(G.ibl.y);
    return vec3<f32>(d.x * c - d.z * s, d.y, d.x * s + d.z * c);
}

// Light falling on a surface facing `n` from the environment map.
fn env_irradiance(n: vec3<f32>) -> vec3<f32> {
    let d = env_dir(n);
    if (G.ibl.w > 0.5) {
        return textureSampleLevel(t_env, s_env, d, G.ibl.z).rgb * PI;
    }
    var e = G.sh[0].rgb * 0.282095;
    e = e + G.sh[1].rgb * 0.488603 * d.y + G.sh[2].rgb * 0.488603 * d.z + G.sh[3].rgb * 0.488603 * d.x;
    e = e + G.sh[4].rgb * 1.092548 * d.x * d.y + G.sh[5].rgb * 1.092548 * d.y * d.z;
    e = e + G.sh[6].rgb * 0.315392 * (3.0 * d.z * d.z - 1.0) + G.sh[7].rgb * 1.092548 * d.x * d.z;
    e = e + G.sh[8].rgb * 0.546274 * (d.x * d.x - d.y * d.y);
    return max(e, vec3<f32>(0.0));
}

// The environment reflected in direction `r` by a surface of roughness
// `rough`.
fn env_specular(r: vec3<f32>, rough: f32) -> vec3<f32> {
    return textureSampleLevel(t_env, s_env, env_dir(r), clamp(rough, 0.0, 1.0) * G.ibl.z).rgb;
}

// How much sunlight reaches `world` (1 = lit, 0 = fully shadowed), with a
// 3x3 soft edge. No derivatives, so it may be called anywhere.
fn sun_shadow(world: vec3<f32>, n: vec3<f32>) -> f32 {
    if (G.shadow.x <= 0.0) {
        return 1.0;
    }
    let p = G.shadow_vp * vec4<f32>(world + n * G.shadow.w, 1.0);
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || p.z >= 1.0) {
        return 1.0;
    }
    let step = G.shadow.z * G.shadow.y;
    var lit = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let o = vec2<f32>(f32(x), f32(y)) * step;
            lit = lit + textureSampleCompareLevel(t_shadow, s_shadow, uv + o, p.z - 0.0015);
        }
    }
    lit = lit / 9.0;
    // Fade out towards the edge of the shadowed area.
    let edge = smoothstep(0.0, 0.1, min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y)));
    return mix(1.0, lit, G.shadow.x * edge);
}

fn fog_amount(dist: f32) -> f32 {
    return 1.0 - exp(-max(dist, 0.0) * G.fog.w);
}

fn apply_fog(c: vec3<f32>, dist: f32) -> vec3<f32> {
    return mix(c, G.fog.rgb, fog_amount(dist));
}

// Fog between the camera and `world`: distance fog plus height fog, whose
// density falls off exponentially above its base height (integrated
// exactly along the ray).
fn fog_amount_at(world: vec3<f32>) -> f32 {
    let cam = G.cam_pos.xyz;
    let d = length(world - cam);
    var od = d * G.fog.w;
    if (G.hfog.x > 0.0) {
        let f = max(G.hfog.z, 0.05);
        let base = G.hfog.x * exp(min(-(cam.y - G.hfog.y) / f, 20.0));
        let dy = world.y - cam.y;
        var k = d;
        if (abs(dy) > 1e-3) {
            k = d * (1.0 - exp(min(-dy / f, 40.0))) / (dy / f);
        }
        od = od + base * k;
    }
    return 1.0 - exp(-max(od, 0.0));
}

fn apply_fog_at(c: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    return mix(c, G.fog.rgb, fog_amount_at(world));
}

// Height fog seen along a view ray that never hits anything (the sky).
fn sky_haze(rd: vec3<f32>) -> f32 {
    if (G.hfog.x <= 0.0) {
        return 0.0;
    }
    let f = max(G.hfog.z, 0.05);
    let base = G.hfog.x * exp(min(-(G.cam_pos.y - G.hfog.y) / f, 20.0));
    let od = base * f / max(rd.y, 0.004);
    return 1.0 - exp(-od);
}

// --- caustics ------------------------------------------------------------

// Rippling light web (after Dave Hoskins' "Tileable Water Caustic").
// Time only enters through whole multiples of the loop angle, so it loops.
fn caustic_pattern(p_in: vec2<f32>) -> f32 {
    let t = G.caus.z;
    let p = p_in;
    var i = p;
    var c = 1.0;
    let inten = 0.005;
    for (var n = 0; n < 4; n = n + 1) {
        let m = f32(select(n + 1, -(n + 1), n % 2 == 1));
        let tt = t * m;
        i = p + vec2<f32>(cos(tt - i.x) + sin(tt + i.y), sin(tt - i.y) + cos(tt + i.x));
        c = c + 1.0 / length(vec2<f32>(p.x / (sin(i.x + tt) / inten), p.y / (cos(i.y + tt) / inten)));
    }
    c = c / 4.0;
    c = 1.17 - pow(c, 1.4);
    return pow(abs(c), 8.0);
}

// Caustic light falling on a surface at `world` with normal `n`.
fn caustic_light(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    if (G.caus.x <= 0.0) {
        return vec3<f32>(0.0);
    }
    let below = smoothstep(G.caus.w + 0.5, G.caus.w - 0.5, world.y);
    if (below <= 0.0) {
        return vec3<f32>(0.0);
    }
    let q = (world.xz + vec2<f32>(world.y * 0.3, -world.y * 0.2)) * 0.9 / max(G.caus.y, 0.05) - vec2<f32>(250.0);
    let k = caustic_pattern(q);
    // Caustics are sunlight: shadows block them.
    return G.caus_col.rgb * G.caus.x * min(k * 4.0, 6.0) * below * (0.35 + 0.65 * max(n.y, 0.0)) * sun_shadow(world, n);
}

// --- wet ground & snow cover ----------------------------------------------

// Snow on upward-facing surfaces (0..1).
fn snow_cover(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let s = G.extra.x;
    if (s <= 0.0) {
        return 0.0;
    }
    let nz = vnoise3(world * 1.7) * 0.6 + vnoise3(world * 5.3) * 0.4;
    let t = 1.0 - s * 1.1;
    return smoothstep(t, t + 0.12, n.y + (nz - 0.5) * 0.35) * smoothstep(0.0, 0.1, s);
}

// Puddles on flat ground when it rains (0..1).
fn puddle(world: vec3<f32>, n: vec3<f32>) -> f32 {
    let w = G.caus_col.w;
    if (w <= 0.0) {
        return 0.0;
    }
    let flat_k = smoothstep(0.85, 0.97, n.y);
    let nz = vnoise3(vec3<f32>(world.x * 0.35, 0.0, world.z * 0.35)) * 0.7 + vnoise3(world * 1.3) * 0.3;
    return flat_k * smoothstep(0.62, 0.66, nz + w * 0.3);
}

// Rain drops hitting puddles: expanding rings, twice per beat (loop-safe).
fn rain_rings(world: vec3<f32>) -> f32 {
    let p = world.xz * 2.5;
    let cell = floor(p);
    let h = hash3f(vec3<f32>(cell.x, 7.0, cell.y));
    let life = fract(G.time.x * G.time.w * 2.0 + h * 17.0);
    let centre = cell + 0.5 + (vec2<f32>(hash3f(vec3<f32>(cell.x, 3.0, cell.y)), hash3f(vec3<f32>(cell.x, 5.0, cell.y))) - 0.5) * 0.5;
    let d = length(p - centre);
    return smoothstep(0.06, 0.0, abs(d - life * 0.45)) * (1.0 - life) * select(0.0, 1.0, h < 0.6);
}

// Cheap studio environment for glossy reflections.
fn env_color(dir: vec3<f32>, rough: f32) -> vec3<f32> {
    let up = smoothstep(-0.4, 0.6, dir.y);
    var c = mix(G.ground.rgb, G.sky.rgb, up);
    let l = normalize(G.light_dir.xyz);
    let sharp = mix(600.0, 12.0, rough);
    c = c + G.light_color.rgb * pow(max(dot(dir, l), 0.0), sharp) * (1.0 - rough * 0.8) * 6.0;
    // Softbox bands give black gloss its crisp highlights.
    let band = smoothstep(0.02, 0.0, abs(dir.y - 0.25)) + smoothstep(0.015, 0.0, abs(dir.y - 0.6)) * 0.6;
    c = c + G.sky.rgb * band * (1.0 - rough) * 1.5;
    return mix(c, G.fog.rgb + G.sky.rgb * 0.2, rough * 0.5);
}

// Lightning (weather bolts and electric arcs).
// Smooth 1D value noise for the bolt's zigzag.
fn bolt_noise(x: f32, seed: u32) -> f32 {
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
    let d1 = bolt_noise(t * 6.0, seed) * 0.5 + bolt_noise(t * 17.0, seed + 1u) * 0.3 + bolt_noise(t * 43.0, seed + 2u) * 0.15;
    let d2 = bolt_noise(t * 5.0, seed + 3u) * 0.5 + bolt_noise(t * 19.0, seed + 4u) * 0.3;
    let env = sqrt(clamp(t * 4.0, 0.0, 1.0));
    return top + axis * t + (side * d1 + side2 * d2) * len * jag * env;
}

// Camera-facing quad along the segment a → b, `width` wide.
fn beam_quad(a: vec3<f32>, b: vec3<f32>, width: f32, c: vec2<f32>) -> vec3<f32> {
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

// Sun, sky, reflections, rim light and the weather on a lit surface
// (meshes and raymarched objects). `v` points to the camera; `ao` darkens
// the sky light and reflections in creases.
fn lit_surface(
    base_in: vec3<f32>,
    metallic_in: f32,
    rough_in: f32,
    n_in: vec3<f32>,
    world: vec3<f32>,
    v: vec3<f32>,
    rim_k: f32,
    ao: f32,
) -> vec3<f32> {
    var base = base_in;
    var metallic = metallic_in;
    var rough = rough_in;
    var n = n_in;
    // Weather on the surface: wet and glossy in the rain (puddles on flat
    // tops), snow on everything facing up.
    let wet = G.caus_col.w * smoothstep(-0.3, 0.5, n.y);
    base = base * (1.0 - 0.45 * wet);
    rough = mix(rough, rough * 0.35, wet);
    let pud = puddle(world, n);
    if (pud > 0.0) {
        rough = mix(rough, 0.02, pud);
        n = normalize(mix(n, vec3<f32>(0.0, 1.0, 0.0), pud));
    }
    let snow = snow_cover(world, n);
    base = mix(base, vec3<f32>(0.88, 0.91, 0.96), snow);
    metallic = mix(metallic, 0.0, snow);
    rough = mix(rough, 0.85, snow);
    let l = normalize(G.light_dir.xyz);
    let sun_lit = sun_shadow(world, n);
    let ndl = max(dot(n, l), 0.0) * sun_lit;
    let diffuse = G.light_color.rgb * G.ground.w * ndl;
    var ambient = mix(G.ground.rgb, G.sky.rgb, n.y * 0.5 + 0.5) * G.sky.w * ao;
    let h = normalize(l + v);
    let shin = mix(512.0, 8.0, rough);
    let spec = pow(max(dot(n, h), 0.0), shin) * (1.0 - rough) * G.ground.w * sun_lit;
    let ndv = max(dot(n, v), 0.0);
    let fres = pow(1.0 - ndv, 5.0);
    let f0 = mix(vec3<f32>(0.04), base, metallic);
    let fr = f0 + (vec3<f32>(1.0) - f0) * fres;
    var env = env_color(reflect(-v, n), rough) * ao;
    var refl = env * (1.0 - rough * 0.6) * fr;
    if (G.ibl.x > 0.0) {
        // Image-based light: diffuse light from the map, and its
        // reflection prefiltered for the roughness, scaled by the
        // split-sum table.
        ambient = env_irradiance(n) * (G.ibl.x / PI) * ao;
        env = env_specular(reflect(-v, n), rough) * G.ibl.x * ao;
        let ab = textureSampleLevel(t_brdf, s_env, vec2<f32>(ndv, rough), 0.0).rg;
        refl = env * (f0 * ab.x + ab.y);
    }

    var col = base * (1.0 - metallic) * (diffuse + ambient);
    col = col + spec * G.light_color.rgb * fr + refl;
    col = col + G.sky.rgb * rim_k * pow(1.0 - ndv, 3.0) * 0.6;
    col = col + base * caustic_light(world, n);
    col = col + env * pud * rain_rings(world) * 0.6;
    return col;
}

fn clip_visible(world: vec3<f32>) -> bool {
    if (dot(G.clip.xyz, G.clip.xyz) == 0.0) {
        return true;
    }
    return dot(world, G.clip.xyz) + G.clip.w >= 0.0;
}

// World-space view ray through a clip-space position.
fn view_ray(ndc: vec2<f32>) -> vec3<f32> {
    let near = G.inv_view_proj * vec4<f32>(ndc, 0.0, 1.0);
    let far = G.inv_view_proj * vec4<f32>(ndc, 1.0, 1.0);
    return normalize(far.xyz / far.w - near.xyz / near.w);
}

struct FullscreenOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

fn fullscreen(vi: u32) -> FullscreenOut {
    var out: FullscreenOut;
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    let p = vec2<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0);
    out.pos = vec4<f32>(p, 0.0, 1.0);
    out.ndc = p;
    return out;
}

// Corner `vi` (0..6) of a two-triangle quad from -1 to 1:
// (-1,-1) (1,-1) (1,1) / (-1,-1) (1,1) (-1,1). No lookup table, because
// D3D's FXC rejects dynamically indexed local arrays.
fn quad_corner(vi: u32) -> vec2<f32> {
    let x = select(-1.0, 1.0, vi == 1u || vi == 2u || vi == 4u);
    let y = select(-1.0, 1.0, vi == 2u || vi == 4u || vi == 5u);
    return vec2<f32>(x, y);
}

// Looks of a shape drawn from a signed distance field (text, logos): a
// body with a glow halo, an outline ring, a drop shadow and a chrome
// bevel. `d` is the field (0.5 on the outline, 0.5 more per spread
// inside), `w` its antialiasing width, `grad` the field's change across
// the shape (x along `side`, y along `up`), `ds` the field at the shadow's
// offset. look: glow, outline width (0..1), shadow, chrome. `normal`,
// `side` and `up` place the shape in the world for the chrome's
// reflection; `view` points towards the eye. Returns premultiplied colour
// and coverage.
fn sdf_look(
    d: f32,
    w: f32,
    grad: vec2<f32>,
    ds: f32,
    base: vec3<f32>,
    outline_col: vec3<f32>,
    look: vec4<f32>,
    normal: vec3<f32>,
    side: vec3<f32>,
    up: vec3<f32>,
    view: vec3<f32>,
) -> vec4<f32> {
    let body = smoothstep(0.5 - w, 0.5 + w, d);
    let ow = clamp(look.y, 0.0, 1.0) * 0.22;
    var ring = 0.0;
    if (ow > 0.0) {
        ring = max(smoothstep(0.5 - ow - w, 0.5 - ow + w, d) - body, 0.0);
    }
    var col = base;
    // Chrome: a bevel from the distance field reflects the environment.
    let chrome = clamp(look.w, 0.0, 1.0);
    if (chrome > 0.0) {
        let slope = grad * 6.0 * smoothstep(0.75, 0.5, d);
        let n = normalize(normal - side * slope.x - up * slope.y);
        let nn = select(-n, n, dot(n, view) > 0.0);
        let r = reflect(-view, nn);
        let env = env_color(r, 0.08);
        col = mix(col, env * (0.35 + col * 0.9), chrome);
    }
    let glow = max(look.x, 0.0);
    var rgb = col * glow * body + outline_col * ring;
    var a = body + ring;
    // Soft halo around bright shapes (light only, no coverage).
    let halo = smoothstep(0.15, 0.5, d) * (1.0 - a);
    rgb = rgb + col * halo * max(glow - 1.0, 0.0) * 0.25;
    // Drop shadow under everything.
    let shadow = smoothstep(0.4, 0.55, ds) * clamp(look.z, 0.0, 1.0) * (1.0 - a);
    a = a + shadow * 0.8;
    return vec4<f32>(rgb, a);
}
