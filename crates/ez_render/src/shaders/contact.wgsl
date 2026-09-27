// Contact shadows: a soft dark patch on the mirror floor under every copy
// of a shape, fading as it rises. Blended multiplicatively.
// D.v[0]: floor height, strength

struct CIn {
    @location(4) m0: vec4<f32>,
    @location(5) m1: vec4<f32>,
    @location(6) m2: vec4<f32>,
    @location(7) m3: vec4<f32>,
    @location(8) inst: vec4<f32>,
};

struct COut {
    @builtin(position) pos: vec4<f32>,
    @location(0) quad: vec2<f32>,
    @location(1) k: f32,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, in: CIn) -> COut {
    let model = mat4x4<f32>(in.m0, in.m1, in.m2, in.m3);
    let centre = (model * vec4<f32>(0.0, 0.0, 0.0, 1.0)).xyz;
    let radius = max(max(length(in.m0.xyz), length(in.m2.xyz)), 0.05) * 0.75;
    let floor_y = D.v[0].x;
    let height = centre.y - floor_y - length(in.m1.xyz) * 0.5;
    let c = quad_corner(vi);
    let r = radius * (1.3 + max(height, 0.0) * 0.3);
    let world = vec3<f32>(centre.x + c.x * r, floor_y + 0.012, centre.z + c.y * r);
    var out: COut;
    out.pos = G.view_proj * vec4<f32>(world, 1.0);
    out.quad = c;
    let near = 1.0 - smoothstep(0.0, radius * 3.0, max(height, 0.0));
    out.k = D.v[0].y * near * (1.0 - fog_amount_at(world));
    if (height < -radius * 2.0) {
        out.k = 0.0;
    }
    return out;
}

@fragment
fn fs_main(in: COut) -> @location(0) vec4<f32> {
    let d2 = dot(in.quad, in.quad);
    let a = exp(-d2 * 3.5) * (1.0 - smoothstep(0.7, 1.0, d2)) * clamp(in.k, 0.0, 1.0);
    return vec4<f32>(vec3<f32>(1.0 - a), 1.0);
}
