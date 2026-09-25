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
    // x: audio level, y: bass
    audio: vec4<f32>,
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

fn fog_amount(dist: f32) -> f32 {
    return 1.0 - exp(-max(dist, 0.0) * G.fog.w);
}

fn apply_fog(c: vec3<f32>, dist: f32) -> vec3<f32> {
    return mix(c, G.fog.rgb, fog_amount(dist));
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
