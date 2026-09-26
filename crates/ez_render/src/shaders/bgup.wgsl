// Upscales a background rendered at half / quarter resolution to the
// screen, behind everything else.

@group(2) @binding(0) var t_low: texture_2d<f32>;
@group(2) @binding(1) var s_low: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> FullscreenOut {
    var o = fullscreen(vi);
    o.pos.z = 1.0;
    return o;
}

@fragment
fn fs_main(in: FullscreenOut) -> @location(0) vec4<f32> {
    let uv = in.pos.xy * G.res.zw;
    return vec4<f32>(textureSampleLevel(t_low, s_low, uv, 0.0).rgb, 1.0);
}
