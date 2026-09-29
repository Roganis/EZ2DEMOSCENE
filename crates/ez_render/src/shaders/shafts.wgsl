// Light shafts (half resolution): for each pixel, march the view ray from
// the camera to the scene (the distance pass's texture) through the fog,
// and gather the sunlight it scatters towards the eye where the sun
// reaches it (the sun shadow map), dimmed by the fog in front. Added onto
// the scene; the fog colour itself is already there, so this is the
// sun's extra light. A fixed Bayer jitter on the start: no history, so
// every frame stands alone and the loop closes.
// D.v[0]: strength, forward scattering g (-0.95..0.95), steps, reach

@group(2) @binding(3) var t_dist: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FullscreenOut {
    return fullscreen(vi);
}

fn shaft_bayer2(x: i32, y: i32) -> i32 {
    return (((x ^ y) & 1) << 1) | (y & 1);
}

fn shaft_bayer4(p: vec2<i32>) -> f32 {
    let v = shaft_bayer2(p.x & 1, p.y & 1) * 4 + shaft_bayer2((p.x >> 1) & 1, (p.y >> 1) & 1);
    return (f32(v) + 0.5) / 16.0;
}

// Fog density at a point: the distance fog plus the height fog, which
// thins out exponentially above its base (as `fog_amount_at` integrates).
fn fog_density(p: vec3<f32>) -> f32 {
    var k = G.fog.w;
    if (G.hfog.x > 0.0) {
        let f = max(G.hfog.z, 0.05);
        k = k + G.hfog.x * exp(min(-(p.y - G.hfog.y) / f, 20.0));
    }
    return k;
}

// Whether the sun reaches `p` (1 outside the shadow map's reach).
fn sun_reaches(p: vec3<f32>) -> f32 {
    if (G.shadow.x <= 0.0) {
        return 1.0;
    }
    let c = G.shadow_vp * vec4<f32>(p, 1.0);
    let uv = vec2<f32>(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || c.z >= 1.0) {
        return 1.0;
    }
    let lit = textureSampleCompareLevel(t_shadow, s_shadow, uv, c.z - 0.002);
    let edge = smoothstep(0.0, 0.1, min(min(uv.x, uv.y), min(1.0 - uv.x, 1.0 - uv.y)));
    return mix(1.0, lit, edge);
}

@fragment
fn fs_main(in: FullscreenOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.pos.xy);
    let rd = view_ray(in.ndc);
    let cam = G.cam_pos.xyz;
    let len = min(textureLoad(t_dist, px, 0).r, D.v[0].w);
    let steps = clamp(i32(D.v[0].z), 4, 96);
    let ds = len / f32(steps);
    // Henyey-Greenstein phase, scaled so that no preference gives 1.
    let g = clamp(D.v[0].y, -0.95, 0.95);
    let l = normalize(G.light_dir.xyz);
    let c = dot(rd, l);
    let phase = (1.0 - g * g) / pow(max(1.0 + g * g - 2.0 * g * c, 1e-4), 1.5);
    let jitter = shaft_bayer4(px);
    var od = 0.0;
    var light = 0.0;
    for (var i = 0; i < steps; i = i + 1) {
        let t = (f32(i) + jitter) * ds;
        let p = cam + rd * t;
        let k = fog_density(p);
        // Scattered here, dimmed by the fog between here and the eye.
        light = light + k * exp(-od) * sun_reaches(p) * ds;
        od = od + k * ds;
    }
    // The sun (or the moon at night); the fog scatters a fifth of it at
    // strength 1 (thick fog would otherwise glow white all over).
    let col = G.light_color.rgb * G.ground.w * light * phase * D.v[0].x * 0.2;
    return vec4<f32>(col, 0.0);
}
