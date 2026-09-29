// Liquid surface: a simulated liquid's droplets drawn as one smooth
// surface (screen-space fluid rendering). Before the scene, droplets are
// splatted as spheres into a half-resolution distance target (nearest
// wins) and a thickness target (summed), both blurred along the surface.
// In the scene's own pass the surface is then shaded at full resolution
// with the layer's material and the environment, writing its depth, so the
// scene's depth buffer hides it behind other things.
// D, D2: the droplet layer's material, as in mesh.wgsl, and:
// D.v[7].y: splat radius per unit of a copy's scale
// D.v[7].z: blur tolerance (world units): droplets further apart in
//           depth than this are different surfaces
// D.v[7].w: absorption (how fast see-through liquid darkens with depth)

@group(1) @binding(1) var<uniform> D2: Draw;

// Blur: the distances and the thickness (the second pass: both in a).
// Composite: the smoothed surface.
@group(2) @binding(0) var t_a: texture_2d<f32>;
@group(2) @binding(1) var t_b: texture_2d<f32>;

// Nothing there (the sky's distance in the distance pass).
const FAR: f32 = 60000.0;

struct SIn {
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(8) inst: vec4<f32>,
};

struct SOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) centre: vec3<f32>,
    @location(1) rad: f32,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_splat(@builtin(vertex_index) vi: u32, in: SIn) -> SOut {
    var out: SOut;
    out.centre = in.m3.xyz;
    out.rad = length(in.m0.xyz) * D.v[7].y;
    let c = quad_corner(vi);
    out.world = out.centre + (G.cam_right.xyz * c.x + G.cam_up.xyz * c.y) * out.rad;
    out.pos = G.view_proj * vec4<f32>(out.world, 1.0);
    if (out.rad <= 1e-5) {
        // A hidden droplet: off the picture.
        out.pos = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    }
    return out;
}

// Distance to the camera of the droplet's front at this pixel, and its
// thickness there; negative outside the droplet.
fn droplet(in: SOut) -> vec2<f32> {
    let q = in.world - in.centre;
    let x = dot(q, G.cam_right.xyz) / in.rad;
    let y = dot(q, G.cam_up.xyz) / in.rad;
    let r2 = x * x + y * y;
    if (r2 > 1.0) {
        return vec2<f32>(-1.0);
    }
    let z = sqrt(1.0 - r2);
    let d = length(in.centre - G.cam_pos.xyz) - z * in.rad;
    return vec2<f32>(d, 2.0 * z * in.rad);
}

@fragment
fn fs_splat_dist(in: SOut) -> @location(0) vec4<f32> {
    let h = droplet(in);
    if (h.x < 0.0) {
        discard;
    }
    return vec4<f32>(h.x, 0.0, 0.0, 1.0);
}

@fragment
fn fs_splat_thick(in: SOut) -> @location(0) vec4<f32> {
    let h = droplet(in);
    if (h.x < 0.0) {
        discard;
    }
    return vec4<f32>(h.y, 0.0, 0.0, 0.0);
}

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> FullscreenOut {
    return fullscreen(vi);
}

// One direction of the blur: distances only average with neighbours on
// the same surface (within the tolerance); thickness blurs freely, which
// softens the edges.
fn blur(p: vec2<i32>, step: vec2<i32>, first: bool) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(t_a));
    let d0 = textureLoad(t_a, p, 0).r;
    let tol = max(D.v[7].z, 1e-3);
    var sd = 0.0;
    var wd = 0.0;
    var st = 0.0;
    var wt = 0.0;
    for (var k = -8; k <= 8; k = k + 1) {
        let q = clamp(p + step * k, vec2<i32>(0), dims - 1);
        let g = exp(-f32(k * k) / 32.0);
        let a = textureLoad(t_a, q, 0);
        var d = a.r;
        var t = 0.0;
        if (first) {
            t = textureLoad(t_b, q, 0).r;
        } else {
            t = a.g;
        }
        st = st + t * g;
        wt = wt + g;
        if (d < FAR * 0.5) {
            var w = g;
            if (d0 < FAR * 0.5) {
                let e = (d - d0) / tol;
                w = w * exp(-e * e);
            }
            sd = sd + d * w;
            wd = wd + w;
        }
    }
    var d = FAR;
    if (wd > 1e-4 && d0 < FAR * 0.5) {
        d = sd / wd;
    }
    return vec4<f32>(d, st / wt, 0.0, 1.0);
}

@fragment
fn fs_blur_h(in: FullscreenOut) -> @location(0) vec4<f32> {
    return blur(vec2<i32>(in.pos.xy), vec2<i32>(1, 0), true);
}

@fragment
fn fs_blur_v(in: FullscreenOut) -> @location(0) vec4<f32> {
    return blur(vec2<i32>(in.pos.xy), vec2<i32>(0, 1), false);
}

// The smoothed surface at `uv`: distance (FAR where there is none) and
// thickness. Bilinear among the texels that have a surface, so edges
// don't average with the far distance.
fn surface_at(uv: vec2<f32>) -> vec2<f32> {
    let dims = vec2<f32>(textureDimensions(t_a));
    let f = uv * dims - 0.5;
    let base = vec2<i32>(floor(f));
    let fr = f - floor(f);
    let hi = vec2<i32>(dims) - 1;
    var sd = 0.0;
    var wd = 0.0;
    var t = 0.0;
    for (var j = 0; j < 2; j = j + 1) {
        for (var i = 0; i < 2; i = i + 1) {
            let q = clamp(base + vec2<i32>(i, j), vec2<i32>(0), hi);
            let s = textureLoad(t_a, q, 0);
            let w = select(1.0 - fr.x, fr.x, i == 1) * select(1.0 - fr.y, fr.y, j == 1);
            t = t + s.g * w;
            if (s.r < FAR * 0.5) {
                sd = sd + s.r * w;
                wd = wd + w;
            }
        }
    }
    if (wd < 1e-4) {
        return vec2<f32>(FAR, t);
    }
    return vec2<f32>(sd / wd, t);
}

fn surface_point(ndc: vec2<f32>, d: f32) -> vec3<f32> {
    return G.cam_pos.xyz + view_ray(ndc) * d;
}

struct COut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_composite(in: FullscreenOut) -> COut {
    let uv = vec2<f32>(in.ndc.x * 0.5 + 0.5, 0.5 - in.ndc.y * 0.5);
    let s = surface_at(uv);
    if (s.x >= FAR * 0.5) {
        discard;
    }
    // Normal from the smoothed surface around (two half-resolution texels
    // each way).
    let texel = 2.0 / vec2<f32>(textureDimensions(t_a));
    let p = surface_point(in.ndc, s.x);
    let du = vec2<f32>(texel.x, 0.0);
    let dv = vec2<f32>(0.0, texel.y);
    let nd = vec2<f32>(2.0 * texel.x, 0.0);
    let ndv = vec2<f32>(0.0, -2.0 * texel.y);
    let sr = surface_at(uv + du);
    let sl = surface_at(uv - du);
    let sd = surface_at(uv + dv);
    let su = surface_at(uv - dv);
    // Take the nearer side at each step (an edge has only one).
    var dx = surface_point(in.ndc + nd, sr.x) - p;
    let dxl = p - surface_point(in.ndc - nd, sl.x);
    if (sr.x >= FAR * 0.5 || (sl.x < FAR * 0.5 && abs(sl.x - s.x) < abs(sr.x - s.x))) {
        dx = dxl;
    }
    var dy = surface_point(in.ndc + ndv, sd.x) - p;
    let dyu = p - surface_point(in.ndc - ndv, su.x);
    if (sd.x >= FAR * 0.5 || (su.x < FAR * 0.5 && abs(su.x - s.x) < abs(sd.x - s.x))) {
        dy = dyu;
    }
    let v = normalize(G.cam_pos.xyz - p);
    var n = normalize(cross(dy, dx));
    if (dot(n, v) < 0.0) {
        n = -n;
    }
    // At the edge (a side missing) the slope is a guess: lean towards the
    // viewer rather than flash at a grazing angle.
    let edge = select(0.0, 0.5, sr.x >= FAR * 0.5 || sl.x >= FAR * 0.5)
        + select(0.0, 0.5, sd.x >= FAR * 0.5 || su.x >= FAR * 0.5);
    n = normalize(mix(n, v, edge * 0.6));
    // Soft edges where the liquid thins out.
    let alpha = smoothstep(0.0, max(D.v[7].z, 1e-3) * 3.0, s.y);
    let base = D.v[0].rgb;
    let metallic = D.v[0].w;
    let rough = clamp(D.v[1].w, 0.02, 1.0);
    var col: vec3<f32>;
    var trans = 0.0;
    if (D2.v[0].x > 0.5) {
        var layers: PbrLayers;
        layers.k = D2.v[1];
        layers.sheen_ior = D2.v[2];
        col = physical_surface(base, metallic, rough, n, p, v, D.v[3].z, 1.0, layers);
        trans = clamp(D2.v[1].w, 0.0, 1.0) * (1.0 - metallic);
    } else {
        col = lit_surface(base, metallic, rough, n, p, v, D.v[3].z, 1.0);
    }
    if (trans > 0.0) {
        // See-through liquid darkens with depth (the environment behind
        // it comes from the physical shading's transmission).
        col = col * mix(1.0, exp(-s.y * max(D.v[7].w, 0.0)), trans);
    }
    // Glow (molten metal).
    col = col + D.v[1].rgb;
    col = apply_fog_at(col, p);
    if (alpha < 0.02) {
        discard;
    }
    var out: COut;
    out.color = vec4<f32>(col * alpha, alpha);
    let clip = G.view_proj * vec4<f32>(p, 1.0);
    out.depth = clamp(clip.z / clip.w, 0.0, 1.0);
    return out;
}
