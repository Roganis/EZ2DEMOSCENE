// Logo: one quad flat on the screen, shaded from a baked texture with the
// colour in rgb and a signed distance field in alpha (0.5 on the outline,
// 0.5 more per spread inside; see logo.rs). Beyond the texture the field
// keeps falling with the distance, so effects can reach far out.
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
// D.v[14]: far field origin (texels of the logo texture), cell size, _
// D.v[15]: far field size (cells), _, _
// Effects, in the second block (distances in logo heights):
// E.v[0]: contour brightness, spacing, travel (0..1 of a spacing), reach
// E.v[1]: contour colour, line width (fraction of the spacing)
// E.v[2]: stacked outlines, their width, gap, contours inside too (0/1)
// E.v[3]: first stacked outline's colour, _
// E.v[4]: last stacked outline's colour, _
// E.v[5]: extrusion length, direction (radians), _, _
// E.v[6]: extrusion colour, _
// E.v[7]: dissolve, patches per logo height, from the edges, burn width
// E.v[8]: burn colour, seed
// E.v[9]: reveal amount, kind (0 grow, 1 edges, 2 wipe, 3 circle),
//         direction (radians), softness
// E.v[10]: morph, the other logo's width / height, its padding (x, y)
// E.v[11]: the other's spread (texels), texture width, height, deepest field
// E.v[12]: logo heights per field unit (this, other), margin around the
//          quad, extra margin across (logo heights)
// E.v[13]: the other's texels per this one's, _, _, _
// E.v[14], E.v[15]: the other's far field, as D.v[14], D.v[15]
// E.v[16]: copper bars (0..1), bars per logo height, scroll (bars, 0..2), _
// E.v[17]: first bar colour, _
// E.v[18]: second bar colour, _
// E.v[19]: rows sideways, columns up (logo heights), waves per logo
//          height, wave position (0..1)
// E.v[20]: glitch jump (logo heights), slices per logo height, share that
//          jumps, seed (changes a whole number of times per loop)
// E.v[21]: colour split (logo heights), direction (radians), a jumping
//          slice's split (logo heights), _
// E.v[22]: pixel blocks (logo heights, 0 = sharp), palette colours (0 = no
//          palette, 216 = the VGA cube), dither, palette rotation (steps)
// E.v[23]: by brightness (0/1), halftone, dot spacing (logo heights),
//          screen angle (radians)
// E.v[24]: scanlines, per logo height, phosphor stripes, line glow
// E.v[25]: moiré, lines per logo height, turn (radians), _
// E.v[26]: glass (0..1), bend (logo heights), dispersion (0..1), _
// E.v[27]: glass tint, _
// E.v[32..48]: palette colours (sRGB; dark to light when by brightness)

@group(2) @binding(0) var t_tex: texture_2d<f32>;
// The logo it morphs into (this one when none).
@group(2) @binding(1) var t_morph: texture_2d<f32>;
// Material sphere (white when none).
@group(2) @binding(2) var t_mat: texture_2d<f32>;
@group(2) @binding(3) var s_clamp: sampler;
// Coarse far fields of both (see logo.rs).
@group(2) @binding(4) var t_far: texture_2d<f32>;
@group(2) @binding(5) var t_morph_far: texture_2d<f32>;
// The picture behind the logos (for glass; may be bigger than the target).
@group(2) @binding(6) var t_behind: texture_2d<f32>;
@group(3) @binding(0) var<uniform> E: Effects;

// Three blocks of effect settings.
struct Effects {
    v: array<vec4<f32>, 48>,
};

// 4×4 Bayer threshold in (-0.5, 0.5), as the palette post effect.
fn bayer(p: vec2<u32>) -> f32 {
    let x = p.x & 3u;
    let y = p.y & 3u;
    let e = x ^ y;
    let v = ((e & 1u) << 3u) | ((y & 1u) << 2u) | (((e >> 1u) & 1u) << 1u) | ((y >> 1u) & 1u);
    return (f32(v) + 0.5) / 16.0 - 0.5;
}

// Only palette colours: `c` linear, `cell` the dither position.
fn to_palette(c_in: vec3<f32>, cell: vec2<u32>) -> vec3<f32> {
    let n = i32(E.v[22].y + 0.5);
    let g = pow(clamp(c_in, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2));
    let d = bayer(cell) * clamp(E.v[22].z, 0.0, 1.0);
    if (n >= 216) {
        let q = clamp(round((g + d / 5.0) * 5.0), vec3<f32>(0.0), vec3<f32>(5.0)) / 5.0;
        return pow(q, vec3<f32>(2.2));
    }
    var best = 0;
    if (E.v[23].x > 0.5) {
        let luma = dot(g, vec3<f32>(0.2126, 0.7152, 0.0722));
        best = clamp(i32(floor((luma + d / f32(n)) * f32(n))), 0, n - 1);
    } else {
        let p = clamp(g + d / sqrt(f32(n)), vec3<f32>(0.0), vec3<f32>(1.0));
        var best_d = 1e9;
        for (var i = 0; i < n; i = i + 1) {
            let e = p - E.v[32 + i].rgb;
            let dist = 0.3 * e.r * e.r + 0.59 * e.g * e.g + 0.11 * e.b * e.b;
            if (dist < best_d) {
                best_d = dist;
                best = i;
            }
        }
    }
    let k = (best + i32(E.v[22].w + 0.5)) % n;
    return pow(E.v[32 + k].rgb, vec3<f32>(2.2));
}

// Height of the bevel (0 at the outline, 1 on top) for a field value.
fn bevel_height(d: f32) -> f32 {
    let kind = i32(D.v[8].x + 0.5);
    var w = max(D.v[8].y, 1e-3);
    if (kind == 4) {
        w = max(deepest() - 0.5, 1e-3);
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

// A rounded edge `w` field units wide (0 at the outline, 1 on top).
fn round_height(d: f32, w: f32) -> f32 {
    let t = clamp((d - 0.5) / w, 0.0, 1.0);
    return sqrt(max(1.0 - (1.0 - t) * (1.0 - t), 0.0));
}

// The field at the thickest point (of the morph between the two logos).
fn deepest() -> f32 {
    let ka = max(E.v[12].x, 1e-6);
    let a = (D.v[11].z - 0.5) * ka;
    let b = (E.v[11].w - 0.5) * E.v[12].y;
    return 0.5 + mix(a, b, clamp(E.v[10].x, 0.0, 1.0)) / ka;
}

// A field sample near the edge of a fine texture blends into its coarse
// far field, which also carries on beyond it. `px`: texels of the fine
// texture.
fn with_far(
    fine: f32,
    px: vec2<f32>,
    size: vec2<f32>,
    spread: f32,
    far_field: f32,
) -> f32 {
    let edge = min(min(px.x, size.x - px.x), min(px.y, size.y - px.y));
    let blend = max(spread * 0.4, 1.0);
    return mix(far_field, fine, smoothstep(0.0, blend, edge));
}

// Colour and field of this logo's texture at `uv`.
fn own(uv: vec2<f32>, lod: f32) -> vec4<f32> {
    let c = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let s = textureSampleLevel(t_tex, s_clamp, c, lod);
    let size = D.v[7].xy;
    let px = uv * size;
    if (min(min(px.x, size.x - px.x), min(px.y, size.y - px.y)) >= max(D.v[6].z * 0.4, 1.0)) {
        return s;
    }
    let cell = max(D.v[14].z, 1.0);
    let fuv = (px - D.v[14].xy) / (cell * max(D.v[15].xy, vec2<f32>(1.0)));
    let far = textureSampleLevel(t_far, s_clamp, fuv, max(lod - log2(cell), 0.0)).a;
    return vec4<f32>(s.rgb, with_far(s.a, px, size, D.v[6].z, far));
}

// The same for the logo it morphs into.
fn other(uv: vec2<f32>, lod: f32) -> vec4<f32> {
    let c = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let s = textureSampleLevel(t_morph, s_clamp, c, lod);
    let size = E.v[11].yz;
    let px = uv * size;
    if (min(min(px.x, size.x - px.x), min(px.y, size.y - px.y)) >= max(E.v[11].x * 0.4, 1.0)) {
        return s;
    }
    let cell = max(E.v[14].z, 1.0);
    let fuv = (px - E.v[14].xy) / (cell * max(E.v[15].xy, vec2<f32>(1.0)));
    let far = textureSampleLevel(t_morph_far, s_clamp, fuv, max(lod - log2(cell), 0.0)).a;
    return vec4<f32>(s.rgb, with_far(s.a, px, size, E.v[11].x, far));
}

// Texture coordinates to the shape's box (0..1 across it, y up) and back.
fn to_box(uv: vec2<f32>, pad: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(uv.x, 1.0 - uv.y) * (vec2<f32>(1.0) + 2.0 * pad) - pad;
}

fn to_uv(box: vec2<f32>, pad: vec2<f32>) -> vec2<f32> {
    let t = (box + pad) / (vec2<f32>(1.0) + 2.0 * pad);
    return vec2<f32>(t.x, 1.0 - t.y);
}

// The logo at `uv` (this logo's texture coordinates): colour and field,
// blended towards the other logo and sunk for the Grow reveal.
fn shape(uv: vec2<f32>, lod: f32) -> vec4<f32> {
    var s = own(uv, lod);
    let t = clamp(E.v[10].x, 0.0, 1.0);
    if (t > 0.0) {
        // Both logos share the anchor and the height.
        let anchor = D.v[5].xy;
        let p = (to_box(uv, D.v[6].xy) - anchor) * vec2<f32>(D.v[5].z, 1.0);
        let box_b = p / vec2<f32>(max(E.v[10].y, 1e-3), 1.0) + anchor;
        let o = other(to_uv(box_b, E.v[10].zw), lod + log2(max(E.v[13].x, 1e-3)));
        // Mix the distances in logo heights, back in this logo's field.
        let ka = max(E.v[12].x, 1e-6);
        let da = (0.5 - s.a) * ka;
        let db = (0.5 - o.a) * E.v[12].y;
        s = vec4<f32>(mix(s.rgb, o.rgb, t), 0.5 - mix(da, db, t) / ka);
    }
    if (i32(E.v[9].y + 0.5) == 0) {
        // Grow: sink the field until only the middle lines are left.
        let r = clamp(E.v[9].x, 0.0, 1.0);
        s.a = s.a - (1.0 - r) * (deepest() - 0.5 + 0.02);
    }
    return s;
}

struct LOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    // Position in the shape's box (0..1 across the shape, y up; the
    // padding and the effects' margin lie outside).
    @location(1) box: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> LOut {
    let c = quad_corner(vi) * 0.5 + 0.5;
    let res = G.res.xy;
    let h = D.v[4].z * res.y;
    let aspect = max(D.v[5].z, 1e-3);
    let size = vec2<f32>(h * aspect, h);
    let pad = D.v[6].xy;
    // Room for effects reaching past the padding.
    let m = vec2<f32>((E.v[12].z + E.v[12].w) / aspect, E.v[12].z);
    let box = mix(-pad - m, vec2<f32>(1.0) + pad + m, c);
    // Pixels from the anchor, turned around it.
    let p = (box - D.v[5].xy) * size;
    let a = D.v[4].w;
    let q = vec2<f32>(p.x * cos(a) - p.y * sin(a), p.x * sin(a) + p.y * cos(a));
    let screen = D.v[4].xy * res + q;
    var out: LOut;
    out.pos = vec4<f32>(screen / res * 2.0 - 1.0, 0.0, 1.0);
    out.uv = to_uv(box, pad);
    out.box = box;
    return out;
}

// The whole logo at `uv` (texture coordinates) and `box` (the shape's box).
// `px`: the fragment's pixel (for glass).
fn shade(uv: vec2<f32>, box: vec2<f32>, lod: f32, px: vec2<f32>) -> vec4<f32> {
    let tsz = max(D.v[7].xy, vec2<f32>(1.0));
    let texel = 1.0 / tsz;
    let spread = D.v[6].z;
    // Neighbours at the same distances (in spreads) as the text's.
    let g = texel * spread * 0.1875;
    let so = texel * spread * vec2<f32>(0.375, 0.5);
    let s = shape(uv, lod);
    let d = s.a;
    let d_r = shape(uv + vec2<f32>(g.x, 0.0), lod).a;
    let d_l = shape(uv - vec2<f32>(g.x, 0.0), lod).a;
    let d_u = shape(uv - vec2<f32>(0.0, g.y), lod).a;
    let d_d = shape(uv + vec2<f32>(0.0, g.y), lod).a;
    let dx = d_r - d_l;
    let dy = d_u - d_d;
    let ds = shape(uv - so, lod).a;
    // The bevel's slope from close neighbours (a texel away), so thin
    // strokes keep their shape.
    let b_r = shape(uv + vec2<f32>(texel.x, 0.0), lod).a;
    let b_l = shape(uv - vec2<f32>(texel.x, 0.0), lod).a;
    let b_u = shape(uv - vec2<f32>(0.0, texel.y), lod).a;
    let b_d = shape(uv + vec2<f32>(0.0, texel.y), lod).a;
    // Pixel blocks are solid: no smoothing across them (a screen
    // derivative would see the jump between blocks).
    let blocky = E.v[22].x > 0.0;
    let w = select(max(fwidth(d) * 0.75, 1e-4), 1e-4, blocky);
    // Distance outside the outline in logo heights, and where we are
    // (logo heights from the middle).
    let ka = max(E.v[12].x, 1e-6);
    let dist = (0.5 - d) * ka;
    let fd = select(max(fwidth(dist), 1e-5), 1e-5, blocky);
    let aspect = D.v[5].z;
    let pc = (box - 0.5) * vec2<f32>(aspect, 1.0);
    let mh = max((deepest() - 0.5) * ka, 1e-3);

    var col = s.rgb * D.v[3].rgb;
    if (D.v[6].w > 0.5) {
        col = mix(D.v[1].rgb, D.v[0].rgb, clamp(box.y, 0.0, 1.0));
    }
    // Copper bars: alternating colours, bright in the middle of each bar.
    let cu = clamp(E.v[16].x, 0.0, 1.0);
    if (cu > 0.0) {
        let pos = box.y * E.v[16].y + E.v[16].z;
        let f = fract(pos);
        let even = fract(floor(pos) * 0.5) < 0.25;
        let prof = sin(PI * f);
        let bar = select(E.v[18].rgb, E.v[17].rgb, even) * (0.25 + 0.95 * prof)
            + vec3<f32>(pow(prof, 12.0) * 0.6);
        col = mix(col, bar, cu);
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
    let mat = textureSample(t_mat, s_clamp, vec2<f32>(n.x, -n.y) * 0.49 + 0.5).rgb;
    if (kind > 0) {
        // A flat top keeps its colour; slopes towards the light brighten,
        // away from it darken.
        let l = D.v[9].xyz;
        let ndl = max(dot(n, l), 0.0);
        let shade = clamp(0.3 + 0.7 * ndl / max(l.z, 0.2), 0.0, 1.8);
        col = col * mix(1.0, shade, clamp(D.v[9].w, 0.0, 2.0));
    }
    col = mix(col, mat, clamp(D.v[11].y, 0.0, 1.0));
    // Glass: the picture behind, bent by the slope of the letters (a
    // rounded edge when there is no bevel).
    let gl = clamp(E.v[26].x, 0.0, 1.0);
    if (gl > 0.0) {
        // The surface's tilt (at most 1 each way).
        var gn = n.xy;
        if (kind == 0) {
            let rw = 0.3;
            let rh = vec2<f32>(
                round_height(b_r, rw) - round_height(b_l, rw),
                round_height(b_u, rw) - round_height(b_d, rw),
            ) / step;
            let rs = vec2<f32>(rh.x * cos(a) - rh.y * sin(a), rh.x * sin(a) + rh.y * cos(a));
            gn = normalize(vec3<f32>(-rs, 1.0)).xy;
        }
        let dims = vec2<f32>(textureDimensions(t_behind));
        // Logo heights on the screen, as texels of the picture behind.
        let bend = vec2<f32>(gn.x, -gn.y) * E.v[26].y * D.v[4].z * G.res.y / dims;
        let base = px / dims;
        let dsp = clamp(E.v[26].z, 0.0, 1.0);
        let br = textureSampleLevel(t_behind, s_clamp, base + bend * (1.0 + dsp), 0.0).r;
        let bg = textureSampleLevel(t_behind, s_clamp, base + bend, 0.0).g;
        let bb = textureSampleLevel(t_behind, s_clamp, base + bend * (1.0 - dsp), 0.0).b;
        // Edges catch the light, as glass edges do.
        let rim = pow(clamp(length(gn), 0.0, 1.0), 3.0) * 0.6;
        col = mix(col, vec3<f32>(br, bg, bb) * E.v[27].rgb + E.v[27].rgb * rim, gl);
    }
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

    // Behind the logo, back to front: contour rings (light), stacked
    // outlines, the extrusion.
    var contour = vec3<f32>(0.0);
    let ci = E.v[0].x;
    if (ci > 0.0) {
        let spacing = max(E.v[0].y, 1e-3);
        let inside = E.v[2].w > 0.5;
        let x = select(max(dist, 0.0), abs(dist), inside);
        let f = x / spacing - E.v[0].z;
        let fw = fd / spacing;
        // Distance to the nearest ring, in spacings.
        let near = 0.5 - abs(fract(f) - 0.5);
        let line = 1.0 - smoothstep(E.v[1].w * 0.5 - fw, E.v[1].w * 0.5 + fw, near);
        // Faded out before the edge of the quad.
        let fade = exp(-x / max(E.v[0].w, 1e-3))
            * (1.0 - smoothstep(0.5 * E.v[12].z, E.v[12].z, x));
        let where_ = select(smoothstep(-fd, fd, dist), 1.0, inside);
        contour = E.v[1].rgb * ci * line * fade * where_;
    }
    var stack = vec4<f32>(0.0);
    let rings = i32(E.v[2].x + 0.5);
    if (rings > 0) {
        let sw = max(E.v[2].y, 1e-4);
        let gap = max(E.v[2].z, 0.0);
        for (var i = 0; i < rings; i = i + 1) {
            let x0 = f32(i) * (sw + gap);
            let band = smoothstep(x0 - fd, x0 + fd, dist)
                * (1.0 - smoothstep(x0 + sw - fd, x0 + sw + fd, dist));
            let c = mix(E.v[3].rgb, E.v[4].rgb, f32(i) / max(f32(rings - 1), 1.0));
            stack = stack + vec4<f32>(c, 1.0) * band;
        }
    }
    var ext = vec4<f32>(0.0);
    let len = E.v[5].x;
    if (len > 0.0) {
        // The logo repeated along the direction, nearest step in front.
        let ea = E.v[5].y;
        let ch = tsz.y / (1.0 + 2.0 * D.v[6].y);
        let step_uv = vec2<f32>(cos(ea) * texel.x, -sin(ea) * texel.y) * ch * len / 24.0;
        for (var i = 1; i <= 24; i = i + 1) {
            let t = f32(i) / 24.0;
            let e = shape(uv - step_uv * f32(i), lod).a;
            let c = smoothstep(0.5 - w, 0.5 + w, e) * (1.0 - ext.a);
            ext = ext + vec4<f32>(mix(E.v[6].rgb, E.v[6].rgb * 0.3, t), 1.0) * c;
        }
    }
    var behind = ext + stack * (1.0 - ext.a);
    behind = vec4<f32>(behind.rgb + contour * (1.0 - behind.a), behind.a);
    var outc = look + behind * (1.0 - look.a);

    // Glint: a soft band sweeping over the letters (logo heights across).
    let body = smoothstep(0.5 - w, 0.5 + w, d);
    let ga = D.v[12].z;
    let gdir = vec2<f32>(cos(ga), sin(ga));
    let gw = max(D.v[12].y, 1e-3);
    let reach = 0.5 * (aspect * abs(gdir.x) + abs(gdir.y)) + gw * 2.0;
    let centre = mix(-reach, reach, D.v[12].x);
    let off = (dot(pc, gdir) - centre) / gw;
    outc = outc + vec4<f32>(D.v[13].rgb * max(D.v[11].w, 0.0) * exp(-off * off) * body, 0.0);

    // Dissolve: noise (and the depth inside the letters) against the
    // amount; a hot rim at the front.
    let q = pc * E.v[7].y;
    let nz = vnoise3(vec3<f32>(q, E.v[8].w)) * 0.65
        + vnoise3(vec3<f32>(q * 2.3, E.v[8].w + 7.0)) * 0.35;
    let depth = clamp(-dist / mh, 0.0, 1.0);
    let v = mix(nz, depth, clamp(E.v[7].z, 0.0, 1.0));
    let fv = select(max(fwidth(v), 1e-4), 1e-4, blocky);
    let dis = E.v[7].x;
    if (dis > 0.0) {
        let front = dis * 1.02;
        let keep = smoothstep(front - fv, front + fv, v);
        let rim = (1.0 - smoothstep(0.0, max(E.v[7].w, 1e-3), v - front)) * keep;
        let cover = min(outc.a, 1.0);
        outc = outc * keep + vec4<f32>(E.v[8].rgb * rim * 1.5 * cover, 0.0);
    }

    // Reveal (Grow is done in shape()).
    let r = clamp(E.v[9].x, 0.0, 1.0);
    let rk = i32(E.v[9].y + 0.5);
    var shown = 1.0;
    if (rk == 1) {
        let inner = max(-dist, 0.0);
        shown = (1.0 - smoothstep(r * mh - fd, r * mh + fd, inner)) * smoothstep(0.0, 0.05, r);
    } else if (rk >= 2) {
        let soft = max(E.v[9].w, 1e-3);
        let margin = E.v[12].z;
        var t = 0.0;
        if (rk == 2) {
            let rdir = vec2<f32>(cos(E.v[9].z), sin(E.v[9].z));
            let extent = 0.5 * (aspect * abs(rdir.x) + abs(rdir.y)) + margin;
            t = (dot(pc, rdir) + extent) / (2.0 * extent);
        } else {
            t = length(pc) / (0.5 * sqrt(aspect * aspect + 1.0) + margin);
        }
        shown = clamp((r * (1.0 + soft) - t) / soft, 0.0, 1.0);
    }
    return outc * shown * clamp(D.v[5].w, 0.0, 1.0);
}

@fragment
fn fs_main(in: LOut) -> @location(0) vec4<f32> {
    let tsz = max(D.v[7].xy, vec2<f32>(1.0));
    let aspect = max(D.v[5].z, 1e-3);
    // Logo heights to texture coordinates and to the box.
    let ch = tsz.y / (1.0 + 2.0 * D.v[6].y);
    let h_uv = vec2<f32>(ch / tsz.x, -ch / tsz.y);
    let h_box = vec2<f32>(1.0 / aspect, 1.0);
    // Mip level from the screen footprint (every sample uses it).
    let fx = dpdx(in.uv * tsz);
    let fy = dpdy(in.uv * tsz);
    let lod = max(0.5 * log2(max(max(dot(fx, fx), dot(fy, fy)), 1e-8)), 0.0);
    // Pixel blocks: everything is worked out at the middle of the block.
    var pbox = in.box;
    var cell = vec2<u32>(in.pos.xy);
    let block = E.v[22].x;
    if (block > 0.0) {
        let hb = (in.box - 0.5) * vec2<f32>(aspect, 1.0);
        let k = floor(hb / block);
        cell = bitcast<vec2<u32>>(vec2<i32>(k));
        pbox = (k + 0.5) * block / vec2<f32>(aspect, 1.0) + 0.5;
    }
    let puv = to_uv(pbox, D.v[6].xy);
    // How far this pixel's picture is moved (logo heights).
    var off = vec2<f32>(0.0);
    let waves = E.v[19].z;
    let wpos = E.v[19].w;
    off.x = off.x + E.v[19].x * sin(TAU * (pbox.y * waves + wpos));
    off.y = off.y + E.v[19].y * sin(TAU * (pbox.x * aspect * waves + wpos));
    // Raster glitch: some slices jump sideways, split in colour.
    var split = 0.0;
    let jump = E.v[20].x;
    if (jump > 0.0) {
        let slice = bitcast<u32>(i32(floor(pbox.y * max(E.v[20].y, 0.1))));
        let seed = u32(E.v[20].w);
        if (hash2u(slice * 3u + 1u, seed) < E.v[20].z) {
            let k = hash2u(slice * 3u + 2u, seed) * 2.0 - 1.0;
            off.x = off.x + k * jump;
            split = E.v[21].z * sign(k);
        }
    }
    let uv = puv - off * h_uv;
    let box = pbox - off * h_box;
    var col: vec4<f32>;
    if (E.v[21].x > 0.0 || jump > 0.0) {
        // Chromatic split: red one way, blue the other.
        let ca = E.v[21].y;
        let c = vec2<f32>(cos(ca), sin(ca)) * (E.v[21].x + split);
        let r = shade(uv - c * h_uv, box - c * h_box, lod, in.pos.xy);
        let g = shade(uv, box, lod, in.pos.xy);
        let b = shade(uv + c * h_uv, box + c * h_box, lod, in.pos.xy);
        col = vec4<f32>(r.r, g.g, b.b, (r.a + g.a + b.a) / 3.0);
    } else {
        col = shade(uv, box, lod, in.pos.xy);
    }

    // Retro looks on the finished logo. Logo heights from the middle:
    let hp = (pbox - 0.5) * vec2<f32>(aspect, 1.0);
    // Moiré: two line patterns turning opposite ways.
    let mo = E.v[25].x;
    if (mo > 0.0) {
        let n = E.v[25].y;
        let a = E.v[25].z;
        let l1 = 0.5 + 0.5 * cos(TAU * n * dot(hp, vec2<f32>(cos(a), sin(a))));
        let l2 = 0.5 + 0.5 * cos(TAU * n * dot(hp, vec2<f32>(cos(0.1 - a), sin(0.1 - a))));
        col = vec4<f32>(col.rgb * mix(1.0, l1 * l2 * 2.0, clamp(mo, 0.0, 1.0)), col.a);
    }
    // Halftone: a dot per cell, as big as the colour is bright.
    let ha = E.v[23].w;
    let hr = vec2<f32>(hp.x * cos(ha) + hp.y * sin(ha), hp.y * cos(ha) - hp.x * sin(ha))
        / max(E.v[23].z, 1e-3);
    let hdist = length(fract(hr) - 0.5);
    let haa = max(fwidth(hdist), 1e-4);
    let ht = clamp(E.v[23].y, 0.0, 1.0);
    if (ht > 0.0) {
        let c = col.rgb / max(col.a, 1e-3);
        let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
        let radius = sqrt(clamp(luma, 0.0, 1.0)) * 0.62;
        let dot_in = 1.0 - smoothstep(radius - haa, radius + haa, hdist);
        let dot_col = c / max(max(c.r, max(c.g, c.b)), 1e-3);
        col = mix(col, vec4<f32>(dot_col, 1.0) * dot_in * col.a, ht);
    }
    // Palette with ordered dither (and colours turning).
    if (E.v[22].y > 0.5 && col.a > 0.002) {
        col = vec4<f32>(to_palette(col.rgb / col.a, cell) * col.a, col.a);
    }
    // Scanlines, phosphor stripes and glowing lines.
    let sl = clamp(E.v[24].x, 0.0, 1.0);
    let stripes = clamp(E.v[24].z, 0.0, 1.0);
    let glow = max(E.v[24].w, 0.0);
    if (sl > 0.0 || stripes > 0.0 || glow > 0.0) {
        let line = 0.5 + 0.5 * cos(TAU * pbox.y * E.v[24].y);
        var f = vec3<f32>(mix(1.0, line, sl) * (1.0 + glow * line));
        let k = u32(in.pos.x) % 3u;
        let rgb = vec3<f32>(f32(k == 0u), f32(k == 1u), f32(k == 2u)) * 2.4 + 0.2;
        f = f * mix(vec3<f32>(1.0), rgb, stripes);
        col = vec4<f32>(col.rgb * f, col.a);
    }
    return col;
}
