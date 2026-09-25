// Deterministic GPU particles: every position is an analytic function of
// (particle id, loop phase), so the simulation loops and scrubs exactly.
// D.v[0]: emitter, count, lifetimes, seed
// D.v[1]: colour a, size
// D.v[2]: colour b, intensity
// D.v[3]: speed, radius, trail count, trail spacing
// D.v[4]: sprite
// D.v[8..11]: layer model matrix

struct POut {
    @builtin(position) pos: vec4<f32>,
    @location(0) quad: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world: vec3<f32>,
};

fn rand_dir(a: f32, b: f32) -> vec3<f32> {
    let z = a * 2.0 - 1.0;
    let t = b * TAU;
    let r = sqrt(max(1.0 - z * z, 0.0));
    return vec3<f32>(r * cos(t), z, r * sin(t));
}

struct Particle {
    pos: vec3<f32>,
    life: f32,
    fade: f32,
    cycle: f32,
};

fn particle(id: u32, phase: f32) -> Particle {
    let emitter = i32(D.v[0].x + 0.5);
    let lifetimes = max(D.v[0].z, 1.0);
    let seed = u32(D.v[0].w);
    let speed = D.v[3].x;
    let radius = D.v[3].y;
    let h0 = hash2u(id, seed);
    let x = phase * lifetimes + h0;
    let life = fract(x);
    // Which rebirth this is (wraps with the loop).
    let cycle = floor(x) - floor(x / lifetimes) * lifetimes;
    let cid = u32(cycle);
    let h1 = hash2u(id * 7u + cid * 131u, seed + 1u);
    let h2 = hash2u(id * 13u + cid * 71u, seed + 2u);
    let h3 = hash2u(id * 17u + cid * 37u, seed + 3u);
    let h4 = hash2u(id, seed + 4u);
    let h5 = hash2u(id, seed + 5u);
    var p: Particle;
    p.life = life;
    p.cycle = cycle;
    p.fade = 1.0;
    switch emitter {
        case 0: {
            // Burst
            let d = rand_dir(h1, h2);
            let k = 1.0 - (1.0 - life) * (1.0 - life);
            p.pos = d * radius * k * (0.4 + 0.6 * h3) * speed;
            p.fade = 1.0 - life;
        }
        case 1: {
            // Sphere drift
            let d = rand_dir(h4, h5);
            let r = radius * pow(hash2u(id, seed + 6u), 0.333);
            let w = TAU * (phase + h0);
            p.pos = d * r + vec3<f32>(sin(w), cos(w * 2.0), sin(w + 1.0)) * 0.3 * speed;
            p.fade = 0.55 + 0.45 * sin(TAU * (phase * lifetimes + h0 * 5.0));
        }
        case 2: {
            // Ring orbit (whole revolutions per loop)
            let revs = max(round(speed), 1.0);
            let a = TAU * (h4 + phase * revs * select(1.0, 2.0, h5 > 0.7));
            let r = radius * (1.0 + (hash2u(id, seed + 7u) - 0.5) * 0.25);
            p.pos = vec3<f32>(cos(a) * r, (hash2u(id, seed + 8u) - 0.5) * 0.15 * radius, sin(a) * r);
            p.fade = 0.5 + 0.5 * sin(TAU * (phase * lifetimes + h0 * 3.0));
        }
        case 3: {
            // Fountain
            let a = h1 * TAU;
            let spread = 0.35 * h2;
            let v0 = vec3<f32>(cos(a) * spread, 1.0, sin(a) * spread) * radius * 1.6 * speed;
            let t = life;
            p.pos = v0 * t + vec3<f32>(0.0, -0.5 * 2.6 * radius * speed * t * t, 0.0) * 1.2;
            p.fade = 1.0 - life * life;
        }
        case 4: {
            // Warp: streams towards +Z of the layer.
            let a = h1 * TAU;
            let r = radius * (0.15 + 0.85 * sqrt(h2));
            let z = mix(-radius * 6.0, radius * 1.5, life);
            p.pos = vec3<f32>(cos(a) * r, sin(a) * r, z);
            p.fade = smoothstep(0.0, 0.3, life);
        }
        case 5: {
            // Vortex
            let a = TAU * (h1 + life * 1.5 * speed);
            let r = radius * (0.25 + 0.75 * h2) * (1.0 - life * 0.6);
            p.pos = vec3<f32>(cos(a) * r, life * radius * 1.4 - radius * 0.2, sin(a) * r);
            p.fade = sin(PI * life);
        }
        default: {
            // Snow / glitter
            let sway = TAU * (life * 2.0 + h3);
            p.pos = vec3<f32>(
                (h1 - 0.5) * 2.0 * radius + sin(sway) * 0.3 * speed,
                radius * (1.0 - 2.0 * life),
                (h2 - 0.5) * 2.0 * radius + cos(sway) * 0.3 * speed,
            );
            p.fade = sin(PI * life) * (0.5 + 0.5 * sin(TAU * (phase * lifetimes * 4.0 + h4)));
        }
    }
    return p;
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> POut {
    let trail = u32(D.v[3].z + 0.5);
    let spacing = D.v[3].w;
    let id = ii / (trail + 1u);
    let k = ii % (trail + 1u);
    let phase = G.time.x;
    var p = particle(id, phase);
    var alpha = p.fade;
    if (k > 0u) {
        let head = p;
        p = particle(id, fract(phase - f32(k) * spacing + 1.0));
        // Hide trail samples that belong to a previous life.
        if (p.cycle != head.cycle) {
            alpha = 0.0;
        } else {
            alpha = p.fade * (1.0 - f32(k) / f32(trail + 1u));
        }
    }
    let model = mat4x4<f32>(D.v[8], D.v[9], D.v[10], D.v[11]);
    let world = (model * vec4<f32>(p.pos, 1.0)).xyz;
    let scale = length(model[0].xyz);
    var size = D.v[1].w * scale;
    if (k > 0u) {
        size = size * (1.0 - 0.5 * f32(k) / f32(trail + 1u));
    }
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[vi];
    let wp = world + (G.cam_right.xyz * c.x + G.cam_up.xyz * c.y) * size;
    var out: POut;
    out.pos = G.view_proj * vec4<f32>(wp, 1.0);
    out.quad = c;
    let tint = mix(D.v[1].rgb, D.v[2].rgb, p.life);
    let fog = 1.0 - fog_amount(length(G.cam_pos.xyz - world));
    out.color = tint * D.v[2].w * max(alpha, 0.0) * fog;
    out.world = world;
    return out;
}

@fragment
fn fs_main(in: POut) -> @location(0) vec4<f32> {
    if (!clip_visible(in.world)) {
        discard;
    }
    let sprite = i32(D.v[4].x + 0.5);
    let q = in.quad;
    let r2 = dot(q, q);
    var a = 0.0;
    switch sprite {
        case 1: {
            a = select(0.0, 1.0, max(abs(q.x), abs(q.y)) < 0.6);
        }
        case 2: {
            let crs = max(exp(-abs(q.x) * 12.0) * exp(-abs(q.y) * 1.5), exp(-abs(q.y) * 12.0) * exp(-abs(q.x) * 1.5));
            a = crs + exp(-r2 * 10.0);
        }
        case 3: {
            let r = sqrt(r2);
            a = smoothstep(0.15, 0.0, abs(r - 0.6));
        }
        default: {
            a = exp(-r2 * 5.0) * (1.0 - smoothstep(0.8, 1.0, r2));
        }
    }
    return vec4<f32>(in.color * a, 0.0);
}
