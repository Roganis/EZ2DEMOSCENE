// Planar mirror floor. The scene is rendered a second time from a camera
// mirrored below the plane; here we sample that image in screen space.
// D.v[0]: half size, height, reflectivity, has reflection
// D.v[1]: base rgb, has texture
// D.v[2]: tint rgb, texture scale
// D.v[3]: grid rgb (colour * strength), grid scale
// D.v[4]: grid scroll offset

@group(2) @binding(0) var t_tex: texture_2d<f32>;
@group(2) @binding(1) var s_tex: sampler;
@group(2) @binding(2) var t_refl: texture_2d<f32>;
@group(2) @binding(3) var s_refl: sampler;

struct FOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FOut {
    let c = quad_corner(vi) * D.v[0].x;
    let world = vec3<f32>(c.x, D.v[0].y, c.y);
    var out: FOut;
    out.world = world;
    out.pos = G.view_proj * vec4<f32>(world, 1.0);
    return out;
}

@fragment
fn fs_main(in: FOut) -> @location(0) vec4<f32> {
    let screen_uv = in.pos.xy * G.res.zw;
    let refl = textureSample(t_refl, s_refl, screen_uv).rgb;
    let tex_uv = in.world.xz * D.v[2].w * 0.25;
    let texel = textureSample(t_tex, s_tex, tex_uv).rgb;
    let g_scale = max(D.v[3].w, 0.001);
    let gp = in.world.xz * g_scale + vec2<f32>(0.0, D.v[4].x);
    let gw = fwidth(gp) * 1.5;

    let v = normalize(G.cam_pos.xyz - in.world);
    var base = D.v[1].rgb;
    if (D.v[1].w > 0.5) {
        base = base * texel;
    }
    let ambient = mix(G.ground.rgb, G.sky.rgb, 1.0) * G.sky.w;
    let sun_lit = sun_shadow(in.world, vec3<f32>(0.0, 1.0, 0.0));
    let diffuse = G.light_color.rgb * G.ground.w * max(normalize(G.light_dir.xyz).y, 0.0) * sun_lit;
    // Shadows also dim the ambient light a little, so they read on dark,
    // glossy floors too.
    var col = base * (ambient * mix(1.0, sun_lit, 0.5) + diffuse);

    let up = vec3<f32>(0.0, 1.0, 0.0);
    // Snow settles on the floor and hides the reflection.
    let snow = snow_cover(in.world, up);
    col = mix(col, vec3<f32>(0.88, 0.91, 0.96) * (ambient + diffuse), snow);
    col = col + (base + vec3<f32>(0.08)) * caustic_light(in.world, up);
    let fres = pow(1.0 - max(v.y, 0.0), 4.0);
    let k = D.v[0].z * mix(0.55, 1.0, fres) * D.v[0].w * (1.0 - snow * 0.85);
    col = col * (1.0 - k * 0.5) + refl * D.v[2].rgb * k * mix(1.0, sun_lit, 0.35);

    // Glowing grid lines
    let f = abs(fract(gp - 0.5) - 0.5);
    let line = max(1.0 - smoothstep(vec2<f32>(0.0), gw + vec2<f32>(0.02), f).x, 1.0 - smoothstep(0.0, gw.y + 0.02, f.y));
    let dist = length(G.cam_pos.xyz - in.world);
    col = col + D.v[3].rgb * line * exp(-dist * 0.015);

    // Emissive texture (LED floors)
    if (D.v[1].w > 0.5) {
        col = col + D.v[3].rgb * pow(dot(texel, vec3<f32>(0.333)), 3.0) * 0.5;
    }

    // Fade out towards the floor edge so it blends into the backdrop.
    let edge = max(abs(in.world.x), abs(in.world.z)) / D.v[0].x;
    col = col + refl * rain_rings(in.world) * G.caus_col.w * 0.3 * (1.0 - snow);
    let out_col = apply_fog_at(col, in.world);
    let fade = smoothstep(1.0, 0.85, edge);
    return vec4<f32>(mix(G.fog.rgb, out_col, fade), 1.0);
}

// Distance to the camera, for depth of field.
@fragment
fn fs_depth(in: FOut) -> @location(0) vec4<f32> {
    return vec4<f32>(length(in.world - G.cam_pos.xyz), 0.0, 0.0, 1.0);
}
