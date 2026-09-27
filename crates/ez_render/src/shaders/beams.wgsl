// Beam directions shared by laser beams and spotlight cones: functions of
// (beam id, loop phase), so they loop exactly.
// D.v[0]: count, pattern (0 fan, 1 cone, 2 scatter), spread (radians), length
// D.v[3]: sweep (radians), sweep phase (0..1), seed, rotation phase (0..1)

fn beam_dir(i: u32) -> vec3<f32> {
    let count = max(D.v[0].x, 1.0);
    let pattern = i32(D.v[0].y + 0.5);
    let spread = D.v[0].z;
    let sweep = D.v[3].x;
    let sp = D.v[3].y;
    let seed = u32(D.v[3].z);
    let fi = f32(i);
    var t = 0.5;
    if (count > 1.5) {
        t = fi / (count - 1.0);
    }
    switch pattern {
        case 1: {
            // Rotating cone.
            let az = TAU * (fi / count + D.v[3].w);
            let polar = clamp(spread * 0.5 + sweep * 0.5 * sin(TAU * (sp + fi / count)), 0.0, PI);
            return vec3<f32>(sin(polar) * cos(az), cos(polar), sin(polar) * sin(az));
        }
        case 2: {
            // Random directions inside the cone that wobble.
            let r1 = hash2u(i, seed);
            let r2 = hash2u(i, seed ^ 0x5bd1e995u);
            let az = TAU * r2 + sweep * sin(TAU * (sp + r1));
            let polar = clamp(spread * 0.5 * sqrt(r1) + sweep * 0.3 * sin(TAU * (sp + r2)), 0.0, PI);
            return vec3<f32>(sin(polar) * cos(az), cos(polar), sin(polar) * sin(az));
        }
        default: {
            // Flat fan in the local XY plane, sweeping side to side.
            let a = (t - 0.5) * spread + sweep * sin(TAU * (sp + t * 0.5));
            return vec3<f32>(sin(a), cos(a), 0.0);
        }
    }
}

