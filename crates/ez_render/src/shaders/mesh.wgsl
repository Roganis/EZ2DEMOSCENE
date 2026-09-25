// Lit, instanced meshes.
// D.v[0]: base rgb, metallic
// D.v[1]: emissive rgb (colour * strength), roughness
// D.v[2]: emissive mode, has texture, texture scale, flat shading
// D.v[3]: scroll u, scroll v, rim, hue shift

@group(2) @binding(0) var t_tex: texture_2d<f32>;
@group(2) @binding(1) var s_tex: sampler;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) edge: f32,
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    // hue, glow, rand, unused
    @location(8) inst: vec4<f32>,
};

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) edge: f32,
    @location(4) inst: vec4<f32>,
};

@vertex
fn vs_main(in: VIn) -> VOut {
    let model = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
    let world = model * vec4<f32>(in.pos, 1.0);
    var out: VOut;
    out.world = world.xyz;
    out.normal = normalize((model * vec4<f32>(in.normal, 0.0)).xyz);
    out.pos = G.view_proj * world;
    out.uv = in.uv;
    out.edge = in.edge;
    out.inst = in.inst;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let base_in = D.v[0].rgb;
    let metallic = D.v[0].w;
    let emissive_in = D.v[1].rgb;
    let rough = clamp(D.v[1].w, 0.02, 1.0);
    let mode = i32(D.v[2].x + 0.5);
    let has_tex = D.v[2].y > 0.5;
    let tex_scale = D.v[2].z;
    let flat_n = D.v[2].w > 0.5;
    let scroll = D.v[3].xy;
    let rim_k = D.v[3].z;
    let hue = D.v[3].w + in.inst.x;

    // Derivative-based values first (uniform control flow).
    let face_n = normalize(cross(dpdx(in.world), dpdy(in.world)));
    let edge_w = fwidth(in.edge) * 1.5 + 0.035;
    let uv = in.uv * tex_scale + scroll;
    let texel = textureSample(t_tex, s_tex, uv).rgb;

    if (!clip_visible(in.world)) {
        discard;
    }

    let v = normalize(G.cam_pos.xyz - in.world);
    var n = normalize(in.normal);
    if (flat_n) {
        n = face_n;
    }
    if (dot(n, v) < 0.0) {
        n = -n;
    }

    var tex = vec3<f32>(1.0);
    if (has_tex) {
        tex = texel;
    }
    let base = hue_rotate(base_in * tex, hue);
    let l = normalize(G.light_dir.xyz);
    let ndl = max(dot(n, l), 0.0);
    let diffuse = G.light_color.rgb * G.ground.w * ndl;
    let ambient = mix(G.ground.rgb, G.sky.rgb, n.y * 0.5 + 0.5) * G.sky.w;
    let h = normalize(l + v);
    let shin = mix(512.0, 8.0, rough);
    let spec = pow(max(dot(n, h), 0.0), shin) * (1.0 - rough) * G.ground.w;
    let ndv = max(dot(n, v), 0.0);
    let fres = pow(1.0 - ndv, 5.0);
    let f0 = mix(vec3<f32>(0.04), base, metallic);
    let fr = f0 + (vec3<f32>(1.0) - f0) * fres;
    let env = env_color(reflect(-v, n), rough);

    var col = base * (1.0 - metallic) * (diffuse + ambient);
    col = col + (spec * G.light_color.rgb + env * (1.0 - rough * 0.6)) * fr;
    col = col + G.sky.rgb * rim_k * pow(1.0 - ndv, 3.0) * 0.6;

    var mask = 1.0;
    switch mode {
        case 1: {
            mask = 1.0 - smoothstep(0.0, edge_w, in.edge);
        }
        case 2: {
            let s = fract(uv.y * 6.0);
            mask = smoothstep(0.35, 0.45, s) * (1.0 - smoothstep(0.55, 0.65, s));
        }
        case 3: {
            let lum = dot(texel, vec3<f32>(0.299, 0.587, 0.114));
            mask = lum * lum;
        }
        default: {}
    }
    let emissive = hue_rotate(emissive_in, hue) * mask * in.inst.y;
    col = col + emissive;

    let dist = length(G.cam_pos.xyz - in.world);
    return vec4<f32>(apply_fog(col, dist), 1.0);
}
