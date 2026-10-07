// Gaussian splats (3D Gaussian splatting), drawn back to front.
//
// Each splat is a soft ellipsoid; on screen it is an ellipse, found by
// projecting its 3D covariance (the EWA splatting of the 3DGS paper), and
// drawn as a quad covering three standard deviations, fading as a Gaussian.
//
// D.v[0]: brightness, opacity, splat size, scatter
// D.v[1]: tint
// D.v[8..11]: model matrix (splat space to world)
// The instance attribute is a splat's index, in back-to-front order (sorted
// on the CPU, see splats.rs). The splats are in a texture, two texels each,
// PER_ROW splats a row:
//   0: x, y, z (f32 bits), colour and opacity (rgba8, sRGB)
//   1: size along x, y, z (f32 bits), rotation (w, x, y, z as snorm8)

@group(2) @binding(0) var splat_data: texture_2d<u32>;

const PER_ROW: u32 = 1024u;
// Quads cover this many standard deviations.
const SIGMAS: f32 = 3.0;

struct SOut {
    @builtin(position) pos: vec4<f32>,
    // Position in the ellipse, in standard deviations.
    @location(0) offset: vec2<f32>,
    // Linear colour and opacity.
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
};

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// Rotation matrix of a unit quaternion (w, x, y, z).
fn quat_matrix(q: vec4<f32>) -> mat3x3<f32> {
    let w = q.x;
    let x = q.y;
    let y = q.z;
    let z = q.w;
    return mat3x3<f32>(
        vec3<f32>(1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y + w * z), 2.0 * (x * z - w * y)),
        vec3<f32>(2.0 * (x * y - w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z + w * x)),
        vec3<f32>(2.0 * (x * z + w * y), 2.0 * (y * z - w * x), 1.0 - 2.0 * (x * x + y * y)),
    );
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @location(0) index: u32) -> SOut {
    var out: SOut;
    // Outside the picture: nothing drawn.
    out.pos = vec4<f32>(0.0, 0.0, 2.0, 1.0);
    out.offset = vec2<f32>(0.0);
    out.color = vec4<f32>(0.0);
    out.world = vec3<f32>(0.0);
    let at = vec2<i32>(i32((index % PER_ROW) * 2u), i32(index / PER_ROW));
    let t0 = textureLoad(splat_data, at, 0);
    let t1 = textureLoad(splat_data, at + vec2<i32>(1, 0), 0);
    var centre = vec3<f32>(bitcast<f32>(t0.x), bitcast<f32>(t0.y), bitcast<f32>(t0.z));
    let scatter = D.v[0].w;
    if (scatter != 0.0) {
        let len = length(centre);
        let dir = select(vec3<f32>(0.0, 1.0, 0.0), centre / len, len > 1e-6);
        centre = centre + dir * scatter * (0.5 + hash1(index));
    }
    let model = mat4x4<f32>(D.v[8], D.v[9], D.v[10], D.v[11]);
    let world = (model * vec4<f32>(centre, 1.0)).xyz;
    let clip = G.view_proj * vec4<f32>(world, 1.0);
    if (clip.w <= 1e-4) {
        return out;
    }
    let ndc = clip.xy / clip.w;
    if (any(abs(ndc) > vec2<f32>(1.3))) {
        return out;
    }

    // The splat's covariance in the world: Σ = A Aᵀ, A = M R S.
    let s = vec3<f32>(bitcast<f32>(t1.x), bitcast<f32>(t1.y), bitcast<f32>(t1.z)) * D.v[0].z;
    let q = unpack4x8snorm(t1.w);
    let r = quat_matrix(q / max(length(q), 1e-6));
    let m3 = mat3x3<f32>(model[0].xyz, model[1].xyz, model[2].xyz) * r;
    let a = mat3x3<f32>(m3[0] * s.x, m3[1] * s.y, m3[2] * s.z);
    let sigma = a * transpose(a);
    // How the pixel position changes with the world position at the centre
    // (the derivative of the perspective divide), in pixels.
    let vp = G.view_proj;
    let rx = vec3<f32>(vp[0].x, vp[1].x, vp[2].x);
    let ry = vec3<f32>(vp[0].y, vp[1].y, vp[2].y);
    let rw = vec3<f32>(vp[0].w, vp[1].w, vp[2].w);
    let half_res = 0.5 * G.res.xy;
    let jx = (rx * clip.w - rw * clip.x) / (clip.w * clip.w) * half_res.x;
    let jy = (ry * clip.w - rw * clip.y) / (clip.w * clip.w) * half_res.y;
    // Its covariance on the screen, blurred by a third of a pixel so tiny
    // splats don't flicker.
    let sx = sigma * jx;
    let sy = sigma * jy;
    let cxx = dot(jx, sx) + 0.3;
    let cxy = dot(jx, sy);
    let cyy = dot(jy, sy) + 0.3;
    // The ellipse's axes: the covariance's eigenvectors and values.
    let mid = 0.5 * (cxx + cyy);
    let rad = length(vec2<f32>(0.5 * (cxx - cyy), cxy));
    let l1 = mid + rad;
    let l2 = max(mid - rad, 0.1);
    var v1 = vec2<f32>(cxy, l1 - cxx);
    if (dot(v1, v1) < 1e-12) {
        v1 = select(vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), cxx >= cyy);
    }
    v1 = normalize(v1);
    let v2 = vec2<f32>(-v1.y, v1.x);
    let r1 = min(sqrt(l1), 1024.0);
    let r2 = min(sqrt(l2), 1024.0);

    let corner = quad_corner(vi);
    let px = (corner.x * r1 * v1 + corner.y * r2 * v2) * SIGMAS;
    out.pos = vec4<f32>(clip.xy + px / half_res * clip.w, clip.z, clip.w);
    out.offset = corner * SIGMAS;
    let rgba = unpack4x8unorm(t0.w);
    let c = srgb_to_linear(rgba.rgb) * D.v[1].rgb * D.v[0].x;
    out.color = vec4<f32>(apply_fog_at(c, world), rgba.a * clamp(D.v[0].y, 0.0, 1.0));
    out.world = world;
    return out;
}

@fragment
fn fs_main(in: SOut) -> @location(0) vec4<f32> {
    if (!clip_visible(in.world)) {
        discard;
    }
    let d2 = dot(in.offset, in.offset);
    let a = in.color.a * exp(-0.5 * d2);
    if (d2 > SIGMAS * SIGMAS || a < 1.0 / 255.0) {
        discard;
    }
    // Premultiplied: blended back to front over what is behind.
    return vec4<f32>(in.color.rgb * a, a);
}
