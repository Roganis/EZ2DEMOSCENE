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
// D.v[8]: bevel (0 flat, 1 round, 2 chiselled, 3 stepped, 4 pillow),
//         width (field units), depth, steps
// D.v[9]: towards the light (screen x right, y up, z out), lighting amount
// D.v[10]: light colour, shine
// D.v[11]: highlight exponent, matcap amount, deepest field value, glint
// D.v[12]: glint position (0..1 across its sweep), width (logo heights),
//          direction (radians), _
// D.v[13]: glint colour, _

@group(2) @binding(0) var t_tex: texture_2d<f32>;
@group(2) @binding(1) var s_tex: sampler;
// Material sphere (white when none).
@group(3) @binding(0) var t_mat: texture_2d<f32>;
@group(3) @binding(1) var s_mat: sampler;

// Height of the bevel (0 at the outline, 1 on top) for a field value.
fn bevel_height(d: f32) -> f32 {
    let kind = i32(D.v[8].x + 0.5);
    var w = max(D.v[8].y, 1e-3);
    if (kind == 4) {
        w = max(D.v[11].z - 0.5, 1e-3);
    }
    let t = clamp((d - 0.5) / w, 0.0, 1.0);
    if (kind == 2) {
        return t;
    }
    if (kind == 3) {
        let n = max(D.v[8].w, 1.0);
        let x = t * n;
        return min((floor(x) + smoothstep(0.3, 0.7, fract(x))) / n, 1.0);
    }
    // Round and pillow: a quarter circle.
    return sqrt(max(1.0 - (1.0 - t) * (1.0 - t), 0.0));
}

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
    let d_r = textureSample(t_tex, s_tex, in.uv + vec2<f32>(g.x, 0.0)).a;
    let d_l = textureSample(t_tex, s_tex, in.uv - vec2<f32>(g.x, 0.0)).a;
    let d_u = textureSample(t_tex, s_tex, in.uv - vec2<f32>(0.0, g.y)).a;
    let d_d = textureSample(t_tex, s_tex, in.uv + vec2<f32>(0.0, g.y)).a;
    let dx = d_r - d_l;
    let dy = d_u - d_d;
    let ds = textureSample(t_tex, s_tex, in.uv - so).a;
    // The bevel's slope from close neighbours (a texel away), so thin
    // strokes keep their shape.
    let b_r = textureSample(t_tex, s_tex, in.uv + vec2<f32>(texel.x, 0.0)).a;
    let b_l = textureSample(t_tex, s_tex, in.uv - vec2<f32>(texel.x, 0.0)).a;
    let b_u = textureSample(t_tex, s_tex, in.uv - vec2<f32>(0.0, texel.y)).a;
    let b_d = textureSample(t_tex, s_tex, in.uv + vec2<f32>(0.0, texel.y)).a;
    let w = max(fwidth(d) * 0.75, 1e-4);

    var col = s.rgb * D.v[3].rgb;
    if (D.v[6].w > 0.5) {
        col = mix(D.v[1].rgb, D.v[0].rgb, clamp(in.box.y, 0.0, 1.0));
    }
    let a = D.v[4].w;
    // The bevel's surface normal on the screen (x right, y up, z out):
    // the height's slope per spread, turned with the logo.
    let kind = i32(D.v[8].x + 0.5);
    let step = 2.0 / max(spread, 1.0);
    let hx = (bevel_height(b_r) - bevel_height(b_l)) / step;
    let hy = (bevel_height(b_u) - bevel_height(b_d)) / step;
    var slope = vec2<f32>(0.0);
    if (kind > 0) {
        slope = vec2<f32>(hx, hy) * D.v[8].z;
    }
    let sl = vec2<f32>(slope.x * cos(a) - slope.y * sin(a), slope.x * sin(a) + slope.y * cos(a));
    let n = normalize(vec3<f32>(-sl, 1.0));
    // Material sphere looked up by the normal (sampled everywhere, used
    // where it is on).
    let mat = textureSample(t_mat, s_mat, vec2<f32>(n.x, -n.y) * 0.49 + 0.5).rgb;
    if (kind > 0) {
        // A flat top keeps its colour; slopes towards the light brighten,
        // away from it darken.
        let l = D.v[9].xyz;
        let ndl = max(dot(n, l), 0.0);
        let shade = clamp(0.3 + 0.7 * ndl / max(l.z, 0.2), 0.0, 1.8);
        col = col * mix(1.0, shade, clamp(D.v[9].w, 0.0, 2.0));
    }
    col = mix(col, mat, clamp(D.v[11].y, 0.0, 1.0));
    if (kind > 0) {
        let h = normalize(D.v[9].xyz + vec3<f32>(0.0, 0.0, 1.0));
        let spec = pow(max(dot(n, h), 0.0), D.v[11].x);
        col = col + D.v[10].rgb * spec * max(D.v[10].w, 0.0);
    }
    // The logo faces the camera; its own axes turn with it.
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
    // Glint: a soft band sweeping over the letters (logo heights across).
    let body = smoothstep(0.5 - w, 0.5 + w, d);
    let ga = D.v[12].z;
    let dir = vec2<f32>(cos(ga), sin(ga));
    let gw = max(D.v[12].y, 1e-3);
    let p = (in.box - 0.5) * vec2<f32>(D.v[5].z, 1.0);
    let reach = 0.5 * (D.v[5].z * abs(dir.x) + abs(dir.y)) + gw * 2.0;
    let centre = mix(-reach, reach, D.v[12].x);
    let off = (dot(p, dir) - centre) / gw;
    let glint = D.v[13].rgb * max(D.v[11].w, 0.0) * exp(-off * off) * body;
    return (look + vec4<f32>(glint, 0.0)) * clamp(D.v[5].w, 0.0, 1.0);
}
