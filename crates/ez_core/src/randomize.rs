//! "Randomize" / "Surprise me": seeded mutations of a project that keep it
//! looking coherent (shared hue rotation, bounded counts, loop-safe values).

use crate::color::hue_rotate;
use crate::graph::tint_layer;
use crate::rng::Rng;
use crate::scene::*;

/// What the randomizer may touch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RandomizeOptions {
    pub colors: bool,
    pub shapes: bool,
    pub motion: bool,
    pub post: bool,
    /// 0..1, how far values may drift.
    pub strength: f32,
}

impl Default for RandomizeOptions {
    fn default() -> Self {
        RandomizeOptions {
            colors: true,
            shapes: true,
            motion: true,
            post: true,
            strength: 0.5,
        }
    }
}

pub fn randomize(project: &mut Project, seed: u64, opt: RandomizeOptions) {
    let mut rng = Rng::new(seed);
    let k = opt.strength.clamp(0.0, 1.0);

    if opt.colors {
        // One global hue rotation keeps the palette harmonious.
        let hue = rng.signed() * 0.5 * k;
        for l in &mut project.layers {
            tint_layer(l, hue, 1.0);
        }
        let env = &mut project.environment;
        env.fog_color = hue_rotate(env.fog_color, hue);
        env.sky_color = hue_rotate(env.sky_color, hue);
        env.ground_color = hue_rotate(env.ground_color, hue);
        env.light_color = hue_rotate(env.light_color, hue * 0.5);
    }

    for l in &mut project.layers {
        match &mut l.kind {
            LayerKind::Mesh(m) => {
                if opt.shapes {
                    if rng.chance(0.3 * k) {
                        m.source =
                            MeshSource::Primitive(rng.pick(&Primitive::random_pool()).clone());
                    }
                    m.variation.seed = rng.next_u32() % 1000;
                    m.variation.scale =
                        (m.variation.scale + rng.signed() * 0.3 * k).clamp(0.0, 0.9);
                    m.variation.rotation =
                        (m.variation.rotation + rng.signed() * 40.0 * k).clamp(0.0, 180.0);
                    scale_counts(&mut m.instancer, &mut rng, k);
                }
                if opt.motion {
                    if rng.chance(0.4 * k) {
                        m.variation.chase = rng.range(0.0, 1.0);
                        m.variation.ripple_cycles = rng.range_u32(1, 8) as i32;
                        m.variation.ripple_spread = rng.range(0.0, 3.0);
                    }
                    let e = &mut m.material.emissive;
                    if e.base > 0.0 && rng.chance(0.3 * k) {
                        e.wave = *rng.pick(&[
                            crate::Wave::Pulse,
                            crate::Wave::Sine,
                            crate::Wave::Square,
                        ]);
                        e.cycles = *rng.pick(&[4, 8, 16]);
                        e.amp = e.base * rng.range(0.3, 1.0);
                    }
                }
            }
            LayerKind::Particles(p) => {
                if opt.shapes && rng.chance(0.4 * k) {
                    p.emitter = *rng.pick(&Emitter::ALL);
                }
                if opt.motion {
                    p.lifetimes = rng.range_u32(1, 4);
                    p.trail = rng.range_u32(0, 4);
                }
                p.seed = rng.next_u32() % 1000;
            }
            LayerKind::Backdrop(b) => {
                if opt.shapes && rng.chance(0.25 * k) {
                    b.kind = *rng.pick(&BackdropKind::ALL);
                }
                if opt.motion {
                    b.speed = rng.range_u32(1, 3) as i32;
                }
            }
            LayerKind::Mirror(_) => {}
            LayerKind::Terrain(t) => {
                if opt.shapes {
                    t.seed = rng.next_u32() % 1000;
                    if rng.chance(0.3 * k) {
                        t.hills = rng.range_u32(2, 8);
                    }
                }
                if opt.motion && rng.chance(0.3 * k) {
                    t.scroll = rng.range_u32(1, 3) as i32;
                }
                if opt.shapes && rng.chance(0.2 * k) {
                    t.shape = *rng.pick(&TerrainShape::ALL);
                }
            }
            LayerKind::Lasers(z) => {
                if opt.shapes && rng.chance(0.3 * k) {
                    z.pattern = *rng.pick(&LaserPattern::ALL);
                }
                if opt.motion {
                    z.sweep_cycles = rng.range_u32(1, 4) as i32;
                }
                z.seed = rng.next_u32() % 1000;
            }
            LayerKind::Falls(f) => {
                if opt.motion {
                    f.flow = rng.range_u32(2, 8);
                }
                f.seed = rng.next_u32() % 1000;
            }
            LayerKind::Weather(w) => {
                if opt.motion {
                    w.wind_dir = rng.range(0.0, 360.0);
                }
                w.seed = rng.next_u32() % 1000;
                w.lightning.seed = rng.next_u32() % 1000;
            }
            LayerKind::Ribbon(r) => {
                if opt.shapes && rng.chance(0.4 * k) {
                    r.curve = *rng.pick(&RibbonCurve::ALL);
                    r.freq = [
                        rng.range_u32(1, 5),
                        rng.range_u32(1, 5),
                        rng.range_u32(1, 6),
                    ];
                }
                if opt.motion && rng.chance(0.4 * k) {
                    r.pulse_speed = *rng.pick(&[-2, -1, 1, 2]);
                }
            }
            LayerKind::Text(t) => {
                // The words are the user's; only the motion changes.
                if opt.motion && matches!(t.style, TextStyle::SineScroller) {
                    t.wave_cycles = *rng.pick(&[1, 2, 3, 4]);
                }
            }
            LayerKind::Sprite(sp) => {
                if opt.shapes {
                    sp.variation.seed = rng.next_u32() % 1000;
                }
                if opt.motion && sp.frame_count() > 1 {
                    sp.cycles = *rng.pick(&[1, 2, 4]);
                }
            }
        }
        if opt.shapes
            && !matches!(
                l.kind,
                LayerKind::Backdrop(_)
                    | LayerKind::Mirror(_)
                    | LayerKind::Terrain(_)
                    | LayerKind::Weather(_)
            )
        {
            l.symmetry = match l.symmetry {
                Symmetry::Radial { .. } => Symmetry::Radial {
                    count: rng.range_u32(3, 12),
                },
                Symmetry::Kaleido { .. } => Symmetry::Kaleido {
                    count: 2 * rng.range_u32(1, 6),
                },
                other => other,
            };
        }
        if opt.motion && rng.chance(0.25 * k) {
            l.transform.spin[1] = rng.range_u32(0, 2) as i32 * if rng.chance(0.5) { 1 } else { -1 };
        }
    }

    if opt.motion {
        let c = &mut project.camera;
        if rng.chance(0.5 * k) {
            c.mode = *rng.pick(&CameraMode::ALL);
        }
        c.orbit_turns = if rng.chance(0.5) { 1 } else { -1 };
        c.swing.base = rng.range(10.0, 40.0);
        c.height.base = (c.height.base + rng.signed() * 2.0 * k).max(-1.0);
    }

    if opt.post {
        let p = &mut project.post;
        if rng.chance(0.3 * k) {
            p.kaleido.enabled = !p.kaleido.enabled;
        }
        p.kaleido.segments = 2 * rng.range_u32(2, 6);
        p.kaleido.turns = rng.range_u32(0, 1) as i32;
        if rng.chance(0.2 * k) {
            p.chroma.enabled = !p.chroma.enabled;
        }
        if rng.chance(0.15 * k) {
            p.mirror.enabled = !p.mirror.enabled;
            p.mirror.mode = *rng.pick(&MirrorSplitMode::ALL);
        }
        if p.palette.enabled && rng.chance(0.5) {
            p.palette.palette = *rng.pick(&crate::palette::PaletteId::ALL);
        }
    }
}

fn scale_counts(inst: &mut Instancer, rng: &mut Rng, k: f32) {
    let f = |rng: &mut Rng, v: u32, lo: u32, hi: u32| -> u32 {
        ((v as f32 * (1.0 + rng.signed() * 0.6 * k)).round() as u32).clamp(lo, hi)
    };
    match inst {
        Instancer::Radial { count, .. } => *count = f(rng, *count, 3, 96),
        Instancer::Scatter { count, seed, .. } => {
            *count = f(rng, *count, 5, 400);
            *seed = rng.next_u32() % 1000;
        }
        Instancer::Orbit { count, seed, .. } => {
            *count = f(rng, *count, 5, 400);
            *seed = rng.next_u32() % 1000;
        }
        Instancer::Spiral { count, turns, .. } => {
            *count = f(rng, *count, 8, 300);
            *turns = (*turns * (1.0 + rng.signed() * 0.5 * k)).max(0.5);
        }
        Instancer::Wall { cols, rows, .. } => {
            *cols = f(rng, *cols, 2, 40);
            *rows = f(rng, *rows, 1, 12);
        }
        Instancer::Curve { count, .. } => *count = f(rng, *count, 6, 120),
        Instancer::Surface { count, seed, .. } | Instancer::OnTerrain { count, seed, .. } => {
            *count = f(rng, *count, 10, 400);
            *seed = rng.next_u32() % 1000;
        }
        Instancer::Grid { .. } | Instancer::Single => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;

    #[test]
    fn deterministic() {
        let mut a = presets::neon_arena();
        let mut b = presets::neon_arena();
        randomize(&mut a, 42, RandomizeOptions::default());
        randomize(&mut b, 42, RandomizeOptions::default());
        assert_eq!(a, b);
        let mut c = presets::neon_arena();
        randomize(&mut c, 43, RandomizeOptions::default());
        assert_ne!(a, c);
    }
}
