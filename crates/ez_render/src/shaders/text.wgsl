// Text: one quad per letter covering its atlas cell, shaded from the
// signed distance field (0.5 on the outline, 1 deep inside).
// D.v[0]: colour at the top of the letters, glow (brightness)
// D.v[1]: colour at the bottom, outline width (0..1)
// D.v[2]: outline colour, drop shadow strength
// D.v[3]: chrome, atlas columns, atlas rows, baseline height in the cell (0..1)

@group(2) @binding(0) var t_tex: texture_2d<f32>;
@group(2) @binding(1) var s_tex: sampler;

struct GIn {
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    // atlas cell, opacity, position along the text, _
    @location(8) inst: vec4<f32>,
};

struct GOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // Position inside the cell (0..1, y up).
    @location(1) local: vec2<f32>,
    @location(2) world: vec3<f32>,
    @location(3) alpha: f32,
    @location(4) normal: vec3<f32>,
    @location(5) side: vec3<f32>,
    @location(6) up: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, in: GIn) -> GOut {
    let model = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
    let c = quad_corner(vi) * 0.5 + 0.5;
    let world = model * vec4<f32>(c, 0.0, 1.0);
    let cols = max(D.v[3].y, 1.0);
    let rows = max(D.v[3].z, 1.0);
    let cell = u32(in.inst.x + 0.5);
    let col = f32(cell % u32(cols));
    let row = f32(cell / u32(cols));
    var out: GOut;
    out.pos = G.view_proj * world;
    out.uv = vec2<f32>((col + c.x) / cols, (row + 1.0 - c.y) / rows);
    out.local = c;
    out.world = world.xyz;
    out.alpha = in.inst.y;
    out.side = normalize(in.m0.xyz);
    out.up = normalize(in.m1.xyz);
    out.normal = normalize(cross(in.m0.xyz, in.m1.xyz));
    return out;
}

@fragment
fn fs_main(in: GOut) -> @location(0) vec4<f32> {
    let cols = max(D.v[3].y, 1.0);
    let rows = max(D.v[3].z, 1.0);
    let texel = vec2<f32>(1.0 / (cols * 64.0), 1.0 / (rows * 64.0));
    let d = textureSample(t_tex, s_tex, in.uv).r;
    // Neighbours for the chrome bevel and the shadow (uniform control flow).
    let dx = textureSample(t_tex, s_tex, in.uv + vec2<f32>(texel.x * 1.5, 0.0)).r
        - textureSample(t_tex, s_tex, in.uv - vec2<f32>(texel.x * 1.5, 0.0)).r;
    let dy = textureSample(t_tex, s_tex, in.uv - vec2<f32>(0.0, texel.y * 1.5)).r
        - textureSample(t_tex, s_tex, in.uv + vec2<f32>(0.0, texel.y * 1.5)).r;
    let ds = textureSample(t_tex, s_tex, in.uv - vec2<f32>(texel.x * 3.0, texel.y * 4.0)).r;
    let w = max(fwidth(d) * 0.75, 1e-4);
    if (!clip_visible(in.world)) {
        discard;
    }

    let body = smoothstep(0.5 - w, 0.5 + w, d);
    let ow = clamp(D.v[1].w, 0.0, 1.0) * 0.22;
    var ring = 0.0;
    if (ow > 0.0) {
        ring = max(smoothstep(0.5 - ow - w, 0.5 - ow + w, d) - body, 0.0);
    }
    // Top-to-bottom colour between the baseline and the capitals' top.
    let base = D.v[3].w;
    let t = clamp((in.local.y - base) / 0.42, 0.0, 1.0);
    var col = mix(D.v[1].rgb, D.v[0].rgb, t);
    // Chrome: a bevel from the distance field reflects the environment.
    let chrome = clamp(D.v[3].x, 0.0, 1.0);
    if (chrome > 0.0) {
        let slope = vec2<f32>(dx, dy) * 6.0 * smoothstep(0.75, 0.5, d);
        let n = normalize(in.normal - in.side * slope.x - in.up * slope.y);
        let v = normalize(G.cam_pos.xyz - in.world);
        let nn = select(-n, n, dot(n, v) > 0.0);
        let r = reflect(-v, nn);
        let env = env_color(r, 0.08);
        col = mix(col, env * (0.35 + col * 0.9), chrome);
    }
    let glow = max(D.v[0].w, 0.0);
    var rgb = col * glow * body + D.v[2].rgb * ring;
    var a = body + ring;
    // Soft halo around bright letters (light only, no coverage).
    let halo = smoothstep(0.15, 0.5, d) * (1.0 - a);
    rgb = rgb + col * halo * max(glow - 1.0, 0.0) * 0.25;
    // Drop shadow under everything.
    let shadow = smoothstep(0.4, 0.55, ds) * clamp(D.v[2].w, 0.0, 1.0) * (1.0 - a);
    a = a + shadow * 0.8;
    let fog = fog_amount_at(in.world);
    rgb = mix(rgb, G.fog.rgb * a, fog);
    return vec4<f32>(rgb, a) * in.alpha;
}
