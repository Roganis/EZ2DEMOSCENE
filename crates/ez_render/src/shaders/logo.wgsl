// Logo: one quad flat on the screen, shaded from a baked texture with the
// colour in rgb and a signed distance field in alpha (0.5 on the outline,
// 0.5 more per spread inside; see logo.rs).
// D.v[0]: colour at the top, glow
// D.v[1]: colour at the bottom, outline width (0..1)
// D.v[2]: outline colour, drop shadow
// D.v[3]: tint (image colours), chrome
// D.v[4]: anchor position on the screen (0..1 from the left, from the
//         bottom), height (fraction of the screen height), rotation (radians)
// D.v[5]: anchor in the logo (0..1 from the left, from the bottom),
//         width / height, opacity
// D.v[6]: padding around the shape (fraction of its width, height),
//         spread (texels), colours (0 image, 1 gradient)
// D.v[7]: texture width, height (texels), _, _

@group(2) @binding(0) var t_tex: texture_2d<f32>;
@group(2) @binding(1) var s_tex: sampler;

struct LOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // Position in the shape's box (0..1 across the shape, y up; the
    // padding lies outside).
    @location(1) box: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> LOut {
    let c = quad_corner(vi) * 0.5 + 0.5;
    let res = G.res.xy;
    let h = D.v[4].z * res.y;
    let size = vec2<f32>(h * D.v[5].z, h);
    let pad = D.v[6].xy;
    let box = mix(-pad, vec2<f32>(1.0) + pad, c);
    // Pixels from the anchor, turned around it.
    let p = (box - D.v[5].xy) * size;
    let a = D.v[4].w;
    let q = vec2<f32>(p.x * cos(a) - p.y * sin(a), p.x * sin(a) + p.y * cos(a));
    let screen = D.v[4].xy * res + q;
    var out: LOut;
    out.pos = vec4<f32>(screen / res * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(c.x, 1.0 - c.y);
    out.box = box;
    return out;
}

@fragment
fn fs_main(in: LOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / max(D.v[7].xy, vec2<f32>(1.0));
    let spread = D.v[6].z;
    // Neighbours at the same distances (in spreads) as the text's.
    let g = texel * spread * 0.1875;
    let so = texel * spread * vec2<f32>(0.375, 0.5);
    let s = textureSample(t_tex, s_tex, in.uv);
    let d = s.a;
    let dx = textureSample(t_tex, s_tex, in.uv + vec2<f32>(g.x, 0.0)).a
        - textureSample(t_tex, s_tex, in.uv - vec2<f32>(g.x, 0.0)).a;
    let dy = textureSample(t_tex, s_tex, in.uv - vec2<f32>(0.0, g.y)).a
        - textureSample(t_tex, s_tex, in.uv + vec2<f32>(0.0, g.y)).a;
    let ds = textureSample(t_tex, s_tex, in.uv - so).a;
    let w = max(fwidth(d) * 0.75, 1e-4);

    var col = s.rgb * D.v[3].rgb;
    if (D.v[6].w > 0.5) {
        col = mix(D.v[1].rgb, D.v[0].rgb, clamp(in.box.y, 0.0, 1.0));
    }
    // The logo faces the camera; its own axes turn with it.
    let a = D.v[4].w;
    let right = G.cam_right.xyz;
    let up0 = G.cam_up.xyz;
    let side = right * cos(a) + up0 * sin(a);
    let up = up0 * cos(a) - right * sin(a);
    let normal = normalize(cross(right, up0));
    let look = sdf_look(
        d,
        w,
        vec2<f32>(dx, dy),
        ds,
        col,
        D.v[2].rgb,
        vec4<f32>(D.v[0].w, D.v[1].w, D.v[2].w, D.v[3].w),
        normal,
        side,
        up,
        normal,
    );
    return look * clamp(D.v[5].w, 0.0, 1.0);
}
