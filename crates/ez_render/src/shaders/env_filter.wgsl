// Blurs a captured sky (a cube map) into one face of one mip of the
// environment map: mip m reflects like a surface of roughness m / 5.
// Samples a cone around each texel's direction (a spiral, weighted
// towards the middle like a GGX lobe).

struct Params {
    // x: face (0..6: +x, -x, +y, -y, +z, -z), y: size of this mip,
    // z: roughness, w: _
    v: vec4<f32>,
};

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var t_src: texture_cube<f32>;
@group(0) @binding(2) var s_src: sampler;

struct VOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VOut {
    var o: VOut;
    let x = f32((vi << 1u) & 2u);
    let y = f32(vi & 2u);
    o.pos = vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
    return o;
}

// Direction of pixel `px` of `face` (rows from the top), as the cube map
// is read.
fn cube_dir(face: i32, px: vec2<f32>, size: f32) -> vec3<f32> {
    let s = 2.0 * px.x / size - 1.0;
    let t = 2.0 * px.y / size - 1.0;
    var d = vec3<f32>(0.0);
    switch face {
        case 0: { d = vec3<f32>(1.0, -t, -s); }
        case 1: { d = vec3<f32>(-1.0, -t, s); }
        case 2: { d = vec3<f32>(s, 1.0, t); }
        case 3: { d = vec3<f32>(s, -1.0, -t); }
        case 4: { d = vec3<f32>(s, -t, 1.0); }
        default: { d = vec3<f32>(-s, -t, -1.0); }
    }
    return normalize(d);
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let n = cube_dir(i32(P.v.x + 0.5), in.pos.xy, P.v.y);
    let rough = P.v.z;
    if (rough <= 0.0) {
        return vec4<f32>(textureSampleLevel(t_src, s_src, n, 0.0).rgb, 1.0);
    }
    let up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(n.z) < 0.999);
    let tx = normalize(cross(up, n));
    let ty = cross(n, tx);
    // The lobe's width: about the GGX spread for this roughness, up to a
    // hemisphere.
    let spread = min(rough * rough * 1.6, 1.5707);
    var sum = vec3<f32>(0.0);
    var weight = 0.0;
    let count = 64;
    for (var i = 0; i < count; i = i + 1) {
        let f = (f32(i) + 0.5) / f32(count);
        let a = spread * sqrt(f);
        let phi = f32(i) * 2.39996323;
        let l = n * cos(a) + (tx * cos(phi) + ty * sin(phi)) * sin(a);
        let w = cos(a) * exp(-2.0 * f);
        sum = sum + textureSampleLevel(t_src, s_src, l, 0.0).rgb * w;
        weight = weight + w;
    }
    return vec4<f32>(sum / weight, 1.0);
}
