//! Built-in scene templates. The first three are modelled after the
//! reference moods: a red neon crystal arena, a golden mirrored kaleido room
//! and a blue solid orbited by debris.

use crate::color::hex;
use crate::palette::PaletteId;
use crate::param::{Param, Wave};
use crate::scene::*;

pub struct Preset {
    pub name: &'static str,
    pub description: &'static str,
    pub project: Project,
}

pub fn all() -> Vec<Preset> {
    vec![
        Preset {
            name: "Neon Arena",
            description: "Black glossy crystal arena, red neon strips, red nebula sky.",
            project: neon_arena(),
        },
        Preset {
            name: "Gold Kaleido Room",
            description: "Mirrored golden hall, walls of orbs, sparkling chandelier.",
            project: gold_room(),
        },
        Preset {
            name: "Orbiting Solid",
            description: "A glossy blue dodecahedron with a swarm of orbiting debris.",
            project: orbiting_solid(),
        },
        Preset {
            name: "Retro Tunnel",
            description: "90s raymarched tunnel, warp stars, EGA palette and CRT.",
            project: retro_tunnel(),
        },
        Preset {
            name: "Plasma Kaleidoscope",
            description: "Oldschool plasma, spinning tori, full-screen kaleidoscope.",
            project: plasma_kaleido(),
        },
        Preset {
            name: "Synth Sunset",
            description: "Synthwave sun, neon grid mirror floor, floating pyramids.",
            project: synth_sunset(),
        },
        Preset {
            name: "Vector Valley",
            description: "Wireframe landscape rushing past, laser fans and a neon ribbon.",
            project: vector_valley(),
        },
        Preset {
            name: "Glitch Shrine",
            description: "A glitching Menger sponge, flashing gems and a pulsing rose.",
            project: glitch_shrine(),
        },
        Preset {
            name: "Sponge Dive",
            description: "Flight through a Menger sponge, a breathing displaced chrome orb.",
            project: sponge_dive(),
        },
        Preset {
            name: "Stormy Lake",
            description: "Rain, lightning and storm clouds over a mountain lake.",
            project: stormy_lake(),
        },
        Preset {
            name: "Lava World",
            description: "Glowing lava rivers in volcanic canyons, embers and god rays.",
            project: lava_world(),
        },
        Preset {
            name: "Sunbeam Peaks",
            description: "Volumetric clouds, sunbeams and lens flare over alpine peaks.",
            project: sunbeam_peaks(),
        },
        Preset {
            name: "Aurora Tundra",
            description: "Northern lights over a frozen lake, gently falling snow.",
            project: aurora_tundra(),
        },
        Preset {
            name: "Dune Sea",
            description: "Sand dunes under a hazy sun, a sandstorm and a toxic oasis.",
            project: dune_sea(),
        },
        Preset {
            name: "Club Spotlights",
            description:
                "Sweeping spotlight cones in a hazy club, pools of light on a mirror floor.",
            project: club_spotlights(),
        },
        Preset {
            name: "Rainbow Falls",
            description:
                "A day passes over a waterfall valley: rainbow, valley mist, stars at night.",
            project: rainbow_falls(),
        },
        Preset {
            name: "Sunken Temple",
            description: "Underwater ruins with rippling caustics, sunbeams and rising bubbles.",
            project: sunken_temple(),
        },
        Preset {
            name: "Twister",
            description: "A tornado crossing wet, stormy plains with lightning and puddles.",
            project: twister(),
        },
        Preset {
            name: "Music Reactor",
            description:
                "Load a song: an equalizer wall, kick flashes, melody colours and time warp.",
            project: music_reactor(),
        },
        Preset {
            name: "Signal Flow",
            description:
                "Node graph: a sequence and a smoothed random walk drive the glow and size.",
            project: signal_flow(),
        },
        Preset {
            name: "Empty",
            description: "A blank stage with a floor and a sky.",
            project: empty(),
        },
    ]
}

/// Orbiting Solid rebuilt as a node graph whose signals drive the
/// centrepiece: a stepped glow sequence and a smoothed random scale.
pub fn signal_flow() -> Project {
    use crate::graph::{Graph, NodeKind};
    use crate::signal::{DriveMode, SignalNode};
    let mut p = orbiting_solid();
    p.name = "Signal Flow".into();
    let mut g = Graph::default();
    let out = g.add(NodeKind::Output, [980.0, 240.0]);
    let mut y = 20.0;
    // One output pin per layer keeps their order.
    for (pin, l) in p.layers.iter().enumerate() {
        let src = g.add(NodeKind::Source { layer: l.clone() }, [40.0, y]);
        y += 105.0;
        if l.name != "Dodecahedron" {
            g.connect(src, 0, out, pin);
            continue;
        }
        let glow = g.add(
            NodeKind::Drive {
                path: "kind.material.emissive".into(),
                mode: DriveMode::Replace,
            },
            [380.0, 640.0],
        );
        let size = g.add(
            NodeKind::Drive {
                path: "transform.scale".into(),
                mode: DriveMode::Multiply,
            },
            [680.0, 640.0],
        );
        let seq = g.add(
            NodeKind::Signal {
                sig: SignalNode::Sequence {
                    values: vec![0.1, 1.6, 0.4, 2.4],
                    beats: 2,
                    glide: false,
                },
            },
            [40.0, 700.0],
        );
        let walk = g.add(
            NodeKind::Signal {
                sig: SignalNode::Wave {
                    param: Param::new(0.0).osc(Wave::Random, 1.0, 8),
                },
            },
            [40.0, 980.0],
        );
        let smooth = g.add(
            NodeKind::Signal {
                sig: SignalNode::Smooth { beats: 2.0 },
            },
            [380.0, 980.0],
        );
        let remap = g.add(
            NodeKind::Signal {
                sig: SignalNode::Remap {
                    in_min: -1.0,
                    in_max: 1.0,
                    out_min: 0.7,
                    out_max: 1.35,
                    clamp: true,
                },
            },
            [680.0, 980.0],
        );
        g.connect(src, 0, glow, 0);
        g.connect(seq, 0, glow, 1);
        g.connect(glow, 0, size, 0);
        g.connect(walk, 0, smooth, 0);
        g.connect(smooth, 0, remap, 0);
        g.connect(remap, 0, size, 1);
        g.connect(size, 0, out, pin);
    }
    p.layers = g.compile();
    p.graph = Some(g);
    p.use_graph = true;
    p
}

pub fn by_name(name: &str) -> Option<Project> {
    all()
        .into_iter()
        .find(|p| p.name == name)
        .map(|p| p.project)
}

fn mesh(prim: Primitive, material: Material) -> MeshLayer {
    MeshLayer {
        source: MeshSource::Primitive(prim),
        material,
        ..Default::default()
    }
}

fn glossy_black() -> Material {
    Material {
        base_color: hex(0x0c0c0e),
        metallic: Param::new(0.9),
        roughness: Param::new(0.15),
        rim: Param::new(0.4),
        flat_shading: true,
        ..Default::default()
    }
}

fn neon(color: u32, strength: Param, mode: EmissiveMode) -> Material {
    Material {
        emissive_color: hex(color),
        emissive: strength,
        emissive_mode: mode,
        ..glossy_black()
    }
}

pub fn empty() -> Project {
    Project {
        name: "Empty".into(),
        camera: Camera {
            distance: Param::new(7.0),
            height: Param::new(2.5),
            target: [0.0, 1.0, 0.0],
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sky",
                LayerKind::Backdrop(Backdrop {
                    resolution: BgResolution::Full,
                    kind: BackdropKind::Gradient,
                    ..Default::default()
                }),
            ),
            Layer::new("Floor", LayerKind::Mirror(MirrorFloor::default())),
            Layer::new(
                "Cube",
                LayerKind::Mesh(mesh(Primitive::Cube, Material::default())),
            )
            .scaled(1.5)
            .at([0.0, 1.0, 0.0])
            .spin([0, 1, 0]),
        ],
        ..Default::default()
    }
}

pub fn neon_arena() -> Project {
    let pulse = Param::new(3.0).osc(Wave::Pulse, 4.0, 16);
    let red = 0xff1a10;
    Project {
        name: "Neon Arena".into(),
        timing: crate::Timing {
            bpm: 128.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Orbit,
            target: [0.0, 0.5, 0.0],
            distance: Param::new(15.0).osc(Wave::Sine, 2.0, 1),
            height: Param::new(6.5).osc(Wave::Sine, 1.0, 2),
            orbit_turns: 1,
            fov: Param::new(62.0),
            beat_shake: Param::new(0.15),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x120102),
            fog_density: Param::new(0.012),
            sky_color: hex(0x802020),
            ground_color: hex(0x050505),
            light_dir: [0.2, 1.0, -0.4],
            light_color: hex(0xffe0e0),
            light_intensity: Param::new(1.2),
            ambient: Param::new(0.15),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Red nebula",
                LayerKind::Backdrop(Backdrop {
                    resolution: BgResolution::Full,
                    kind: BackdropKind::Nebula,
                    color_a: hex(0x020001),
                    color_b: hex(0x4a0404),
                    color_c: hex(0xe02010),
                    speed: 1,
                    intensity: Param::new(0.75),
                    detail: Param::new(1.2),
                    texture: None,
                    ray: Default::default(),
                }),
            ),
            Layer::new(
                "Glossy floor",
                LayerKind::Mirror(MirrorFloor {
                    size: 60.0,
                    base_color: hex(0x030303),
                    reflectivity: Param::new(0.35),
                    blur: Param::new(0.3),
                    tint: hex(0xa08080),
                    grid: Param::new(0.0),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Crystal skyline",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 56,
                        radius: 34.0,
                    },
                    variation: Variation {
                        seed: 7,
                        rotation: 18.0,
                        scale: 0.55,
                        ..Default::default()
                    },
                    ..mesh(Primitive::Crystal { spikes: 6, seed: 3 }, glossy_black())
                }),
            )
            .scaled(7.0)
            .at([0.0, -1.0, 0.0]),
            Layer::new(
                "Outer neon ring",
                LayerKind::Mesh(mesh(
                    Primitive::Ring {
                        arc: 360.0,
                        width: 0.02,
                        height: 0.12,
                        segments: 160,
                    },
                    neon(
                        red,
                        Param::new(4.0).osc(Wave::Sine, 1.0, 4),
                        EmissiveMode::Full,
                    ),
                )),
            )
            .stretched([16.0, 1.0, 16.0])
            .at([0.0, 0.1, 0.0]),
            Layer::new(
                "Tech panels",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 20,
                        radius: 11.0,
                    },
                    variation: Variation {
                        chase: 1.0,
                        ripple_cycles: 4,
                        ripple_spread: 2.0,
                        ..Default::default()
                    },
                    ..mesh(
                        Primitive::Panel { bevel: 0.2 },
                        neon(red, Param::new(5.0), EmissiveMode::Edges),
                    )
                }),
            )
            .stretched([2.6, 0.35, 1.2])
            .at([0.0, 0.2, 0.0]),
            Layer::new(
                "Chevrons",
                LayerKind::Mesh(mesh(
                    Primitive::Ring {
                        arc: 40.0,
                        width: 0.12,
                        height: 0.08,
                        segments: 16,
                    },
                    neon(red, pulse, EmissiveMode::Full),
                )),
            )
            .scaled(6.5)
            .at([0.0, 0.06, 0.0])
            .sym(Symmetry::Kaleido { count: 6 }),
            Layer::new(
                "Debris blocks",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Scatter {
                        count: 70,
                        radius: 9.0,
                        shell: true,
                        seed: 4,
                    },
                    variation: Variation {
                        seed: 3,
                        rotation: 45.0,
                        scale: 0.5,
                        ..Default::default()
                    },
                    ..mesh(Primitive::Cube, glossy_black())
                }),
            )
            .stretched([1.0, 0.4, 1.0])
            .scaled(0.7)
            .at([0.0, -1.5, 0.0])
            .spin([0, 1, 0]),
            Layer::new(
                "Central platform",
                LayerKind::Mesh(mesh(
                    Primitive::Cylinder { segments: 8 },
                    neon(red, Param::new(2.5), EmissiveMode::Edges),
                )),
            )
            .stretched([3.4, 0.5, 3.4])
            .at([0.0, 0.25, 0.0]),
            Layer::new(
                "Chain ring",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 28,
                        radius: 2.2,
                    },
                    ..mesh(
                        Primitive::Torus {
                            thickness: 0.28,
                            segments: 16,
                        },
                        Material {
                            base_color: hex(0x303036),
                            metallic: Param::new(1.0),
                            roughness: Param::new(0.2),
                            ..glossy_black()
                        },
                    )
                }),
            )
            .scaled(0.4)
            .stretched([1.0, 1.0, 0.5])
            .rotated([0.0, 0.0, 0.0])
            .at([0.0, 1.35, 0.0])
            .spin([0, -1, 0]),
            Layer::new(
                "Rising sparks",
                LayerKind::Particles(ParticleLayer {
                    emitter: Emitter::Vortex,
                    count: 900,
                    lifetimes: 2,
                    size: Param::new(0.06),
                    speed: Param::new(1.0),
                    radius: Param::new(7.0),
                    color_a: hex(0xffb080),
                    color_b: hex(0xff1000),
                    intensity: Param::new(3.0),
                    trail: 2,
                    trail_spacing: Param::new(0.006),
                    sprite: Sprite::Glow,
                    seed: 5,
                    smoke: false,
                }),
            ),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(1.1),
                threshold: Param::new(0.9),
                radius: Param::new(0.75),
            },
            chroma: Chroma {
                enabled: true,
                amount: Param::new(0.002).osc(Wave::Pulse, 0.004, 16),
            },
            grade: Grade {
                vignette: Param::new(0.45),
                contrast: Param::new(1.12),
                saturation: Param::new(1.15),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn gold_room() -> Project {
    let gold = Material {
        base_color: hex(0xd4a02a),
        metallic: Param::new(1.0),
        roughness: Param::new(0.25),
        rim: Param::new(0.6),
        emissive_color: hex(0xffc060),
        emissive: Param::new(0.25),
        ..Default::default()
    };
    Project {
        name: "Gold Kaleido Room".into(),
        timing: crate::Timing {
            bpm: 110.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            target: [0.0, 3.0, -4.0],
            distance: Param::new(11.0).osc(Wave::Sine, 1.5, 1),
            height: Param::new(3.2).osc(Wave::Sine, 0.6, 2),
            swing: Param::new(16.0),
            fov: Param::new(75.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x3a2708),
            fog_density: Param::new(0.012),
            sky_color: hex(0xfff0c0),
            ground_color: hex(0x7a5010),
            light_dir: [0.0, 1.0, 0.5],
            light_color: hex(0xffe8b0),
            light_intensity: Param::new(1.8),
            ambient: Param::new(0.45),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Warm glow",
                LayerKind::Backdrop(Backdrop {
                    resolution: BgResolution::Full,
                    kind: BackdropKind::Gradient,
                    color_a: hex(0x3a2000),
                    color_b: hex(0xc08a30),
                    color_c: hex(0xfff0c0),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "LED mirror floor",
                LayerKind::Mirror(MirrorFloor {
                    size: 30.0,
                    base_color: hex(0x5a3a08),
                    reflectivity: Param::new(0.85),
                    blur: Param::new(0.1),
                    tint: hex(0xffd890),
                    texture: Some("led_grid".into()),
                    texture_scale: 0.35,
                    grid: Param::new(0.6).osc(Wave::Sine, 0.4, 4),
                    grid_color: hex(0xffd070),
                    grid_scale: Param::new(0.5),
                    grid_scroll: 2,
                }),
            ),
            Layer::new(
                "Orb wall",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Wall {
                        cols: 14,
                        rows: 5,
                        spacing: 0.75,
                        curve: 50.0,
                    },
                    variation: Variation {
                        hue: 0.06,
                        ripple: 0.12,
                        ripple_cycles: 4,
                        ripple_spread: 1.5,
                        chase: 0.6,
                        ..Default::default()
                    },
                    ..mesh(
                        Primitive::Sphere { detail: 2 },
                        Material {
                            base_color: hex(0x9a4a18),
                            emissive_color: hex(0xff9040),
                            emissive: Param::new(0.4),
                            ..gold.clone()
                        },
                    )
                }),
            )
            .scaled(0.33)
            .rotated([0.0, 90.0, 0.0])
            .at([6.5, 0.5, -3.0])
            .sym(Symmetry::MirrorX),
            Layer::new(
                "Speaker stacks",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Grid {
                        counts: [2, 3, 1],
                        spacing: [1.3, 1.3, 1.0],
                    },
                    variation: Variation {
                        ripple: 0.06,
                        ripple_cycles: 16,
                        ripple_spread: 0.0,
                        ..Default::default()
                    },
                    ..mesh(
                        Primitive::Cube,
                        Material {
                            base_color: hex(0x3a2a10),
                            texture: Some("speaker".into()),
                            emissive_color: hex(0xffc050),
                            emissive: Param::new(0.5).osc(Wave::Pulse, 1.0, 16),
                            emissive_mode: EmissiveMode::Edges,
                            ..gold.clone()
                        },
                    )
                }),
            )
            .scaled(1.1)
            .at([3.0, 1.5, -7.0])
            .sym(Symmetry::MirrorX),
            Layer::new(
                "Light panels",
                LayerKind::Mesh(mesh(
                    Primitive::Panel { bevel: 0.05 },
                    Material {
                        base_color: hex(0xfff0c0),
                        texture: Some("led_grid".into()),
                        texture_scale: Param::new(3.0),
                        emissive_color: hex(0xffe6a0),
                        emissive: Param::new(3.0).osc(Wave::Sine, 0.8, 8),
                        emissive_mode: EmissiveMode::Texture,
                        ..gold.clone()
                    },
                )),
            )
            .stretched([0.2, 5.0, 4.0])
            .at([9.0, 3.5, -2.0])
            .sym(Symmetry::MirrorX),
            Layer::new(
                "Back portal",
                LayerKind::Mesh(mesh(
                    Primitive::Panel { bevel: 0.05 },
                    Material {
                        emissive_color: hex(0xfff4d0),
                        emissive: Param::new(4.0),
                        emissive_mode: EmissiveMode::Stripes,
                        ..gold.clone()
                    },
                )),
            )
            .stretched([3.0, 4.0, 0.2])
            .at([0.0, 3.0, -9.0]),
            Layer::new(
                "Ceiling frame",
                LayerKind::Mesh(mesh(Primitive::Panel { bevel: 0.1 }, gold.clone())),
            )
            .stretched([9.0, 0.3, 12.0])
            .at([0.0, 8.5, -3.0]),
            Layer::new(
                "Chandelier",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Spiral {
                        count: 90,
                        radius: 1.6,
                        height: 3.0,
                        turns: 6.0,
                    },
                    variation: Variation {
                        chase: 1.0,
                        ripple_cycles: 2,
                        ripple_spread: 3.0,
                        ..Default::default()
                    },
                    ..mesh(
                        Primitive::Octahedron,
                        Material {
                            base_color: hex(0xfff8e0),
                            metallic: Param::new(0.3),
                            roughness: Param::new(0.05),
                            flat_shading: true,
                            emissive_color: hex(0xfff0c0),
                            emissive: Param::new(1.5),
                            ..gold.clone()
                        },
                    )
                }),
            )
            .scaled(0.18)
            .stretched([1.0, 1.8, 1.0])
            .at([0.0, 6.2, -3.0])
            .spin([0, 1, 0]),
            Layer::new(
                "Glitter",
                LayerKind::Particles(ParticleLayer {
                    emitter: Emitter::Snow,
                    count: 1200,
                    lifetimes: 1,
                    size: Param::new(0.05),
                    speed: Param::new(1.0),
                    radius: Param::new(8.0),
                    color_a: hex(0xfff0c0),
                    color_b: hex(0xffa040),
                    intensity: Param::new(2.5),
                    trail: 0,
                    trail_spacing: Param::new(0.01),
                    sprite: Sprite::Star,
                    seed: 9,
                    smoke: false,
                }),
            )
            .at([0.0, 4.0, -3.0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(1.0),
                threshold: Param::new(1.0),
                radius: Param::new(0.8),
            },
            mirror: MirrorSplit {
                enabled: true,
                mode: MirrorSplitMode::LeftToRight,
            },
            grade: Grade {
                exposure: Param::new(1.1),
                vignette: Param::new(0.3),
                saturation: Param::new(1.05),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn orbiting_solid() -> Project {
    Project {
        name: "Orbiting Solid".into(),
        timing: crate::Timing {
            bpm: 120.0,
            loop_beats: 8,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            target: [0.0, 0.0, 0.0],
            distance: Param::new(7.0).osc(Wave::Sine, 0.8, 1),
            height: Param::new(1.5).osc(Wave::Sine, 1.0, 1),
            swing: Param::new(25.0),
            fov: Param::new(50.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x000208),
            fog_density: Param::new(0.01),
            sky_color: hex(0x80c0ff),
            ground_color: hex(0x000000),
            light_dir: [-0.5, 0.8, 0.6],
            light_color: hex(0xe0f0ff),
            light_intensity: Param::new(2.2),
            ambient: Param::new(0.15),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Deep space",
                LayerKind::Backdrop(Backdrop {
                    resolution: BgResolution::Full,
                    kind: BackdropKind::Starfield,
                    color_a: hex(0x000003),
                    color_b: hex(0x061530),
                    color_c: hex(0x80b0ff),
                    speed: 1,
                    intensity: Param::new(0.6),
                    detail: Param::new(1.0),
                    texture: None,
                    ray: Default::default(),
                }),
            ),
            Layer::new(
                "Dodecahedron",
                LayerKind::Mesh(mesh(
                    Primitive::Dodecahedron,
                    Material {
                        base_color: hex(0x1a70d8),
                        metallic: Param::new(0.5),
                        roughness: Param::new(0.18),
                        flat_shading: true,
                        rim: Param::new(0.9),
                        emissive_color: hex(0x40a0ff),
                        emissive: Param::new(0.15).osc(Wave::Pulse, 0.5, 8),
                        texture: Some("noise".into()),
                        texture_scale: Param::new(1.0),
                        ..Default::default()
                    },
                )),
            )
            .scaled(1.6)
            .spin([1, 1, 0]),
            Layer::new(
                "Debris swarm",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Orbit {
                        count: 70,
                        radius: 3.2,
                        spread: 0.9,
                        speed: 1,
                        seed: 2,
                    },
                    variation: Variation {
                        scale: 0.6,
                        rotation: 90.0,
                        ..Default::default()
                    },
                    ..mesh(
                        Primitive::Cube,
                        Material {
                            base_color: hex(0xe8f0ff),
                            metallic: Param::new(0.2),
                            roughness: Param::new(0.3),
                            flat_shading: true,
                            ..Default::default()
                        },
                    )
                }),
            )
            .stretched([0.12, 0.12, 0.5]),
            Layer::new(
                "Blue orbit rings",
                LayerKind::Mesh(mesh(
                    Primitive::Ring {
                        arc: 250.0,
                        width: 0.06,
                        height: 0.03,
                        segments: 96,
                    },
                    Material {
                        emissive_color: hex(0x2080ff),
                        emissive: Param::new(5.0),
                        ..Default::default()
                    },
                )),
            )
            .scaled(2.9)
            .rotated([20.0, 0.0, 10.0])
            .spin([0, 2, 0])
            .sym(Symmetry::Kaleido { count: 2 }),
            Layer::new(
                "White arc",
                LayerKind::Mesh(mesh(
                    Primitive::Ring {
                        arc: 120.0,
                        width: 0.2,
                        height: 0.05,
                        segments: 48,
                    },
                    Material {
                        base_color: hex(0xffffff),
                        emissive_color: hex(0xd0e8ff),
                        emissive: Param::new(1.5),
                        ..Default::default()
                    },
                )),
            )
            .scaled(4.2)
            .rotated([-15.0, 0.0, -20.0])
            .spin([0, -1, 0]),
            Layer::new(
                "Sparks",
                LayerKind::Particles(ParticleLayer {
                    emitter: Emitter::Ring,
                    count: 600,
                    lifetimes: 1,
                    size: Param::new(0.04),
                    speed: Param::new(1.0),
                    radius: Param::new(3.6),
                    color_a: hex(0xb0d8ff),
                    color_b: hex(0x2060ff),
                    intensity: Param::new(2.5),
                    trail: 3,
                    trail_spacing: Param::new(0.004),
                    sprite: Sprite::Glow,
                    seed: 3,
                    smoke: false,
                }),
            )
            .rotated([15.0, 0.0, 0.0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.9),
                threshold: Param::new(0.9),
                radius: Param::new(0.7),
            },
            chroma: Chroma {
                enabled: true,
                amount: Param::new(0.003),
            },
            grade: Grade {
                vignette: Param::new(0.5),
                contrast: Param::new(1.1),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn retro_tunnel() -> Project {
    Project {
        name: "Retro Tunnel".into(),
        timing: crate::Timing {
            bpm: 140.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0, 0.0, 0.0],
            distance: Param::new(6.0),
            height: Param::new(0.0),
            fov: Param::new(70.0),
            roll: Param::new(0.0).osc(Wave::Sine, 12.0, 1),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x000000),
            fog_density: Param::new(0.03),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "XOR tunnel",
                LayerKind::Backdrop(Backdrop {
                    resolution: BgResolution::Full,
                    kind: BackdropKind::Tunnel,
                    color_a: hex(0x000000),
                    color_b: hex(0x2040ff),
                    color_c: hex(0xff40c0),
                    speed: 4,
                    intensity: Param::new(1.2),
                    detail: Param::new(1.0),
                    texture: Some("xor".into()),
                    ray: Default::default(),
                }),
            ),
            Layer::new(
                "Icosahedron",
                LayerKind::Mesh(mesh(
                    Primitive::Icosahedron,
                    Material {
                        base_color: hex(0xffffff),
                        texture: Some("checker".into()),
                        texture_scale: Param::new(2.0),
                        scroll: [1, 0],
                        flat_shading: true,
                        emissive_color: hex(0xffff55),
                        emissive: Param::new(0.6).osc(Wave::Pulse, 1.5, 16),
                        emissive_mode: EmissiveMode::Edges,
                        pixelated: true,
                        ..Default::default()
                    },
                )),
            )
            .scaled(1.3)
            .spin([2, 1, 0]),
            Layer::new(
                "Warp stars",
                LayerKind::Particles(ParticleLayer {
                    emitter: Emitter::Warp,
                    count: 700,
                    lifetimes: 4,
                    size: Param::new(0.05),
                    speed: Param::new(1.0),
                    radius: Param::new(6.0),
                    color_a: hex(0xffffff),
                    color_b: hex(0x55ffff),
                    intensity: Param::new(2.0),
                    trail: 4,
                    trail_spacing: Param::new(0.01),
                    sprite: Sprite::Square,
                    seed: 1,
                    smoke: false,
                }),
            ),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.5),
                threshold: Param::new(0.9),
                radius: Param::new(0.5),
            },
            pixelate: Pixelate {
                enabled: true,
                size: Param::new(3.0),
            },
            palette: PaletteFx {
                enabled: true,
                palette: PaletteId::Ega,
                dither: Param::new(0.7),
            },
            crt: Crt {
                enabled: true,
                scanlines: Param::new(0.5),
                curvature: Param::new(0.12),
                noise: Param::new(0.08),
            },
            grade: Grade {
                vignette: Param::new(0.2),
                grain: Param::new(0.0),
                beat_flash: Param::new(0.15),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn plasma_kaleido() -> Project {
    Project {
        name: "Plasma Kaleidoscope".into(),
        timing: crate::Timing {
            bpm: 125.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Orbit,
            distance: Param::new(8.0),
            height: Param::new(2.0).osc(Wave::Sine, 2.0, 1),
            orbit_turns: 1,
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x100020),
            fog_density: Param::new(0.01),
            sky_color: hex(0xff80ff),
            ground_color: hex(0x200040),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Plasma",
                LayerKind::Backdrop(Backdrop {
                    resolution: BgResolution::Full,
                    kind: BackdropKind::Plasma,
                    color_a: hex(0x10003a),
                    color_b: hex(0xff2090),
                    color_c: hex(0x20e0ff),
                    speed: 2,
                    intensity: Param::new(0.9),
                    detail: Param::new(1.0),
                    texture: None,
                    ray: Default::default(),
                }),
            ),
            Layer::new(
                "Tori",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 6,
                        radius: 2.5,
                    },
                    variation: Variation {
                        spin: 1,
                        hue: 0.4,
                        ..Default::default()
                    },
                    ..mesh(
                        Primitive::Torus {
                            thickness: 0.25,
                            segments: 32,
                        },
                        Material {
                            base_color: hex(0xffffff),
                            texture: Some("plasma".into()),
                            scroll: [1, 1],
                            metallic: Param::new(0.6),
                            roughness: Param::new(0.2),
                            emissive_color: hex(0xff60ff),
                            emissive: Param::new(0.6),
                            emissive_mode: EmissiveMode::Stripes,
                            hue_shift: Param::new(0.0).osc(Wave::Saw, 0.5, 1),
                            ..Default::default()
                        },
                    )
                }),
            )
            .spin([0, 1, 0]),
            Layer::new(
                "Core",
                LayerKind::Mesh(mesh(
                    Primitive::Icosahedron,
                    Material {
                        base_color: hex(0x202020),
                        metallic: Param::new(1.0),
                        roughness: Param::new(0.1),
                        flat_shading: true,
                        emissive_color: hex(0x40ffff),
                        emissive: Param::new(2.0).osc(Wave::Pulse, 3.0, 16),
                        emissive_mode: EmissiveMode::Edges,
                        ..Default::default()
                    },
                )),
            )
            .spin([1, 2, 0]),
        ],
        post: PostStack {
            kaleido: Kaleido {
                enabled: true,
                segments: 8,
                angle: Param::new(0.0),
                turns: 1,
                zoom: Param::new(1.0).osc(Wave::Sine, 0.2, 2),
                center: [0.5, 0.5],
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn synth_sunset() -> Project {
    Project {
        name: "Synth Sunset".into(),
        timing: crate::Timing {
            bpm: 100.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0, 2.0, -10.0],
            distance: Param::new(12.0),
            height: Param::new(1.5),
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x2a0830),
            fog_density: Param::new(0.02),
            sky_color: hex(0xff60a0),
            ground_color: hex(0x100020),
            light_dir: [0.0, 0.3, -1.0],
            light_color: hex(0xff80c0),
            light_intensity: Param::new(1.5),
            ambient: Param::new(0.3),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sunset",
                LayerKind::Backdrop(Backdrop {
                    resolution: BgResolution::Full,
                    kind: BackdropKind::SynthGrid,
                    color_a: hex(0x0a0020),
                    color_b: hex(0xff3080),
                    color_c: hex(0xffd030),
                    speed: 1,
                    intensity: Param::new(1.0),
                    detail: Param::new(1.0),
                    texture: None,
                    ray: Default::default(),
                }),
            ),
            Layer::new(
                "Neon grid floor",
                LayerKind::Mirror(MirrorFloor {
                    size: 80.0,
                    base_color: hex(0x080010),
                    reflectivity: Param::new(0.5),
                    blur: Param::new(0.15),
                    tint: hex(0xffa0ff),
                    grid: Param::new(2.5),
                    grid_color: hex(0xff20c0),
                    grid_scale: Param::new(0.5),
                    grid_scroll: 8,
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Pyramids",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Grid {
                        counts: [2, 1, 3],
                        spacing: [14.0, 1.0, 8.0],
                    },
                    variation: Variation {
                        scale: 0.3,
                        ..Default::default()
                    },
                    ..mesh(
                        Primitive::Pyramid,
                        Material {
                            base_color: hex(0x100018),
                            flat_shading: true,
                            emissive_color: hex(0x20e0ff),
                            emissive: Param::new(3.0).osc(Wave::Sine, 1.0, 4),
                            emissive_mode: EmissiveMode::Edges,
                            metallic: Param::new(0.8),
                            roughness: Param::new(0.2),
                            ..Default::default()
                        },
                    )
                }),
            )
            .scaled(2.5)
            .at([0.0, 1.25, -14.0]),
            Layer::new(
                "Floating octahedron",
                LayerKind::Mesh(mesh(
                    Primitive::Octahedron,
                    Material {
                        base_color: hex(0x101010),
                        metallic: Param::new(1.0),
                        roughness: Param::new(0.05),
                        flat_shading: true,
                        emissive_color: hex(0xff40d0),
                        emissive: Param::new(2.0),
                        emissive_mode: EmissiveMode::Edges,
                        ..Default::default()
                    },
                )),
            )
            .scaled(1.2)
            .at([0.0, 3.0, -8.0])
            .spin([0, 1, 0])
            .bobbing(Param::new(0.0).osc(Wave::Sine, 0.5, 2)),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(1.0),
                threshold: Param::new(0.8),
                radius: Param::new(0.8),
            },
            crt: Crt {
                enabled: true,
                scanlines: Param::new(0.25),
                curvature: Param::new(0.0),
                noise: Param::new(0.03),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn vector_valley() -> Project {
    Project {
        name: "Vector Valley".into(),
        timing: crate::Timing {
            bpm: 120.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(8.0),
            target: [0.0, 3.0, -12.0],
            distance: Param::new(16.0),
            height: Param::new(2.5),
            fov: Param::new(65.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x07021a),
            fog_density: Param::new(0.025),
            sky_color: hex(0x6040ff),
            ground_color: hex(0x080010),
            light_dir: [0.0, 0.4, -1.0],
            light_color: hex(0xc080ff),
            light_intensity: Param::new(1.2),
            ambient: Param::new(0.3),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Stars",
                LayerKind::Backdrop(Backdrop {
                    resolution: BgResolution::Full,
                    kind: BackdropKind::Starfield,
                    color_a: hex(0x02000a),
                    color_b: hex(0x301060),
                    color_c: hex(0xa0c0ff),
                    speed: 1,
                    intensity: Param::new(1.0),
                    detail: Param::new(1.0),
                    texture: None,
                    ray: Default::default(),
                }),
            ),
            Layer::new(
                "Valley",
                LayerKind::Terrain(Terrain {
                    size: 80.0,
                    cells: 80,
                    height: Param::new(7.0),
                    hills: 5,
                    roughness: Param::new(0.45),
                    scroll: 2,
                    valley: Param::new(0.35),
                    style: TerrainStyle::Both,
                    line_color: hex(0xff2bd6),
                    glow: Param::new(1.2).osc(Wave::Pulse, 0.6, 16),
                    fill_color: hex(0x0a0418),
                    seed: 7,
                    ..Default::default()
                }),
            )
            .at([0.0, -0.5, -20.0]),
            Layer::new(
                "Lasers",
                LayerKind::Lasers(Lasers {
                    count: 10,
                    spread: Param::new(80.0),
                    length: Param::new(60.0),
                    width: Param::new(0.1),
                    color_a: hex(0x20ffa0),
                    color_b: hex(0x20a0ff),
                    intensity: Param::new(3.0),
                    sweep: Param::new(20.0),
                    sweep_cycles: 2,
                    strobe: Param::new(0.6),
                    ..Default::default()
                }),
            )
            .at([0.0, 0.5, -45.0])
            .rotated([-20.0, 0.0, 0.0])
            .sym(Symmetry::MirrorX),
            Layer::new(
                "Ribbon",
                LayerKind::Ribbon(Ribbon {
                    curve: RibbonCurve::Wave,
                    freq: [5, 1, 1],
                    thickness: 0.03,
                    color: hex(0x00e5ff),
                    glow: Param::new(0.8),
                    pulses: 4,
                    pulse_speed: 2,
                    pulse_length: Param::new(0.06),
                    pulse_glow: Param::new(8.0),
                }),
            )
            .scaled(4.0)
            .at([0.0, 6.0, -14.0])
            .spin([0, 1, 0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(1.1),
                threshold: Param::new(0.8),
                radius: Param::new(0.8),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn glitch_shrine() -> Project {
    Project {
        name: "Glitch Shrine".into(),
        timing: crate::Timing {
            bpm: 128.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Orbit,
            target: [0.0, 2.0, 0.0],
            distance: Param::new(11.0),
            height: Param::new(3.5),
            orbit_turns: 1,
            fov: Param::new(55.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x06030c),
            fog_density: Param::new(0.03),
            sky_color: hex(0x40a0ff),
            ground_color: hex(0x100010),
            light_dir: [0.3, 1.0, 0.4],
            light_color: hex(0xffffff),
            light_intensity: Param::new(1.3),
            ambient: Param::new(0.25),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Nebula",
                LayerKind::Backdrop(Backdrop {
                    resolution: BgResolution::Full,
                    kind: BackdropKind::Nebula,
                    color_a: hex(0x020008),
                    color_b: hex(0x3010a0),
                    color_c: hex(0x00c0ff),
                    speed: 1,
                    intensity: Param::new(0.7),
                    detail: Param::new(1.0),
                    texture: None,
                    ray: Default::default(),
                }),
            ),
            Layer::new(
                "Floor",
                LayerKind::Mirror(MirrorFloor {
                    size: 60.0,
                    base_color: hex(0x040408),
                    reflectivity: Param::new(0.6),
                    blur: Param::new(0.2),
                    grid: Param::new(0.6),
                    grid_color: hex(0x00c0ff),
                    grid_scale: Param::new(1.0),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Sponge",
                LayerKind::Mesh(mesh(
                    Primitive::Menger { level: 2 },
                    Material {
                        base_color: hex(0x08080c),
                        metallic: Param::new(0.8),
                        roughness: Param::new(0.15),
                        flat_shading: true,
                        emissive_color: hex(0x00e5ff),
                        emissive: Param::new(2.0),
                        emissive_mode: EmissiveMode::Edges,
                        glitch: Glitch {
                            amount: Param::new(0.35),
                            style: GlitchStyle::Slices,
                            rate: 32,
                            chance: Param::new(0.3),
                            seed: 4,
                        },
                        ..Default::default()
                    },
                )),
            )
            .scaled(2.6)
            .at([0.0, 2.4, 0.0])
            .spin([0, 1, 0]),
            {
                let mut gems = Layer::new(
                    "Gems",
                    LayerKind::Mesh(MeshLayer {
                        instancer: Instancer::Radial {
                            count: 8,
                            radius: 5.0,
                        },
                        ..mesh(
                            Primitive::Gem { facets: 8 },
                            Material {
                                base_color: hex(0x200818),
                                metallic: Param::new(0.5),
                                roughness: Param::new(0.1),
                                flat_shading: true,
                                emissive_color: hex(0xff2090),
                                emissive: Param::new(1.5),
                                emissive_mode: EmissiveMode::Edges,
                                ..Default::default()
                            },
                        )
                    }),
                )
                .scaled(0.6)
                .at([0.0, 1.0, 0.0])
                .spin([0, -1, 0]);
                gems.blink = Blink {
                    mode: BlinkMode::Flash,
                    per_loop: 16,
                    duty: 0.25,
                    ..Default::default()
                };
                gems
            },
            Layer::new(
                "Rose",
                LayerKind::Ribbon(Ribbon {
                    curve: RibbonCurve::Rose,
                    freq: [4, 3, 1],
                    thickness: 0.02,
                    color: hex(0xffc040),
                    glow: Param::new(0.6),
                    pulses: 2,
                    pulse_speed: 1,
                    pulse_length: Param::new(0.1),
                    pulse_glow: Param::new(6.0),
                }),
            )
            .scaled(4.5)
            .at([0.0, 0.3, 0.0]),
            Layer::new(
                "Cone lasers",
                LayerKind::Lasers(Lasers {
                    count: 12,
                    pattern: LaserPattern::Cone,
                    spread: Param::new(40.0),
                    length: Param::new(30.0),
                    width: Param::new(0.05),
                    color_a: hex(0xff2090),
                    color_b: hex(0x00e5ff),
                    intensity: Param::new(2.0),
                    sweep: Param::new(20.0),
                    sweep_cycles: 1,
                    strobe: Param::new(0.3),
                    ..Default::default()
                }),
            )
            .at([0.0, 0.0, 0.0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(1.0),
                threshold: Param::new(0.8),
                radius: Param::new(0.7),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn sponge_dive() -> Project {
    Project {
        name: "Sponge Dive".into(),
        timing: crate::Timing {
            bpm: 124.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(20.0),
            target: [0.0, 2.0, 0.0],
            distance: Param::new(9.0),
            height: Param::new(1.5),
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x05030a),
            fog_density: Param::new(0.01),
            sky_color: hex(0xa0b0ff),
            ground_color: hex(0x201008),
            light_dir: [0.3, 1.0, 0.6],
            light_color: hex(0xffe0c0),
            light_intensity: Param::new(1.6),
            ambient: Param::new(0.35),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sponge",
                LayerKind::Backdrop(Backdrop {
                    resolution: BgResolution::Full,
                    kind: BackdropKind::Sponge,
                    color_a: hex(0x05030a),
                    color_b: hex(0x1c1030),
                    color_c: hex(0x9a4a1c),
                    speed: 1,
                    intensity: Param::new(0.8),
                    detail: Param::new(1.0),
                    texture: None,
                    ray: RaySettings {
                        glow: Param::new(0.4),
                        fog: Param::new(2.0),
                        spin: 1,
                        ..Default::default()
                    },
                }),
            ),
            Layer::new(
                "Orb",
                LayerKind::Mesh(MeshLayer {
                    subdivide: 1,
                    ..mesh(
                        Primitive::Sphere { detail: 5 },
                        Material {
                            base_color: hex(0xd0d4e0),
                            metallic: Param::new(1.0),
                            roughness: Param::new(0.12),
                            texture: Some("noise".into()),
                            texture_scale: Param::new(2.0),
                            relief: Relief {
                                bump: Param::new(1.5),
                                displace: Param::new(0.05).osc(Wave::Swell, 0.25, 16),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                    )
                }),
            )
            .scaled(1.6)
            .at([0.0, 2.0, 0.0])
            .spin([0, 1, 0]),
            Layer::new(
                "Plates",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 10,
                        radius: 4.2,
                    },
                    ..mesh(
                        Primitive::Panel { bevel: 0.1 },
                        Material {
                            base_color: hex(0x909098),
                            metallic: Param::new(0.8),
                            roughness: Param::new(0.3),
                            texture: Some("metal_plate".into()),
                            relief: Relief {
                                bump: Param::new(3.0),
                                ..Default::default()
                            },
                            emissive_color: hex(0xff9040),
                            emissive: Param::new(0.0).osc(Wave::ExpOut, 2.0, 16),
                            emissive_mode: EmissiveMode::Edges,
                            ..Default::default()
                        },
                    )
                }),
            )
            .scaled(1.0)
            .stretched([1.2, 0.15, 0.8])
            .at([0.0, 0.5, 0.0])
            .spin([0, -1, 0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.9),
                threshold: Param::new(0.9),
                radius: Param::new(0.7),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

impl Layer {
    pub fn bobbing(mut self, bob: Param) -> Self {
        self.transform.bob = bob;
        self
    }
}

fn sky(kind: BackdropKind, a: u32, b: u32, c: u32, ray: RaySettings) -> Layer {
    Layer::new(
        "Sky",
        LayerKind::Backdrop(Backdrop {
            // Soft clouds look the same at half resolution, 4× cheaper.
            resolution: if kind == BackdropKind::Clouds {
                BgResolution::Half
            } else {
                BgResolution::Full
            },
            kind,
            color_a: hex(a),
            color_b: hex(b),
            color_c: hex(c),
            speed: 1,
            intensity: Param::new(1.0),
            detail: Param::new(1.0),
            texture: None,
            ray,
        }),
    )
}

pub fn stormy_lake() -> Project {
    Project {
        name: "Stormy Lake".into(),
        timing: crate::Timing {
            bpm: 90.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(12.0),
            target: [0.0, 2.0, -20.0],
            distance: Param::new(22.0),
            height: Param::new(3.0),
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x1a2028),
            fog_density: Param::new(0.018),
            sky_color: hex(0x4a5868),
            ground_color: hex(0x101418),
            light_dir: [0.3, 0.5, -1.0],
            light_color: hex(0xb0c0d0),
            light_intensity: Param::new(0.7),
            ambient: Param::new(0.6),
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Clouds,
                0x2a3440,
                0x3c4855,
                0x303a48,
                RaySettings {
                    variant: 2,
                    size: Param::new(1.2),
                    warp: Param::new(1.1),
                    bend: Param::new(1.4),
                    glow: Param::new(0.3),
                    fog: Param::new(1.0),
                    ..Default::default()
                },
            ),
            Layer::new(
                "Mountains",
                LayerKind::Terrain(Terrain {
                    size: 120.0,
                    cells: 128,
                    lod: true,
                    height: Param::new(12.0),
                    hills: 4,
                    roughness: Param::new(0.5),
                    scroll: 1,
                    valley: Param::new(0.45),
                    style: TerrainStyle::Solid,
                    seed: 21,
                    shape: TerrainShape::Mountains,
                    biome: Biome::Alpine,
                    liquid: Liquid {
                        kind: LiquidKind::Water,
                        level: Param::new(0.12),
                        color: hex(0x0a2028),
                        glow: Param::new(0.5),
                        waves: Param::new(1.5),
                        flow: 1,
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, 0.0, -40.0]),
            Layer::new(
                "Rain",
                LayerKind::Weather(Weather {
                    count: 9000,
                    area: 20.0,
                    height: 16.0,
                    falls: 14,
                    wind: Param::new(12.0).osc(Wave::Sine, 6.0, 2),
                    intensity: Param::new(0.5),
                    lightning: Lightning {
                        enabled: true,
                        per_loop: 4,
                        chance: 0.6,
                        seed: 11,
                        distance: 45.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, 1.5, 0.0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.7),
                threshold: Param::new(1.0),
                radius: Param::new(0.7),
            },
            grade: Grade {
                saturation: Param::new(0.8),
                vignette: Param::new(0.5),
                grain: Param::new(0.04),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn lava_world() -> Project {
    Project {
        name: "Lava World".into(),
        timing: crate::Timing {
            bpm: 100.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(10.0),
            target: [0.0, 2.0, -18.0],
            distance: Param::new(20.0),
            height: Param::new(9.0),
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x2a0c06),
            fog_density: Param::new(0.02),
            sky_color: hex(0x803020),
            ground_color: hex(0x401008),
            light_dir: [0.0, 0.25, -1.0],
            light_color: hex(0xffa060),
            light_intensity: Param::new(1.1),
            ambient: Param::new(0.35),
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Clouds,
                0x301010,
                0x803818,
                0x401410,
                RaySettings {
                    variant: 1,
                    size: Param::new(1.0),
                    warp: Param::new(0.95),
                    bend: Param::new(1.0),
                    glow: Param::new(1.5),
                    fog: Param::new(1.2),
                    ..Default::default()
                },
            ),
            Layer::new(
                "Canyons",
                LayerKind::Terrain(Terrain {
                    size: 110.0,
                    cells: 128,
                    lod: true,
                    height: Param::new(7.0),
                    hills: 3,
                    roughness: Param::new(0.6),
                    scroll: 1,
                    valley: Param::new(0.0),
                    style: TerrainStyle::Solid,
                    seed: 5,
                    shape: TerrainShape::Canyons,
                    biome: Biome::Volcanic,
                    liquid: Liquid {
                        kind: LiquidKind::Lava,
                        level: Param::new(0.3),
                        color: hex(0xff4a08),
                        glow: Param::new(1.4).osc(Wave::Sine, 0.3, 4),
                        waves: Param::new(1.0),
                        flow: 2,
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, -2.0, -40.0]),
            Layer::new(
                "Embers",
                LayerKind::Weather(Weather {
                    kind: Precipitation::Embers,
                    count: 1500,
                    area: 16.0,
                    height: 10.0,
                    falls: 3,
                    wind: Param::new(15.0),
                    wind_dir: 90.0,
                    size: Param::new(0.07),
                    color: hex(0xff8030),
                    intensity: Param::new(3.0),
                    ..Default::default()
                }),
            )
            .at([0.0, -1.0, 0.0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(1.0),
                threshold: Param::new(1.0),
                radius: Param::new(0.8),
            },
            rays: GodRays {
                enabled: true,
                intensity: Param::new(1.2),
                length: Param::new(0.8),
                threshold: Param::new(0.6),
                tint: hex(0xffc080),
                ..Default::default()
            },
            haze: HeatHaze {
                enabled: true,
                amount: Param::new(1.5),
                ..Default::default()
            },
            grade: Grade {
                vignette: Param::new(0.6),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn sunbeam_peaks() -> Project {
    Project {
        name: "Sunbeam Peaks".into(),
        timing: crate::Timing {
            bpm: 110.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(15.0),
            target: [0.0, 5.0, -20.0],
            distance: Param::new(24.0),
            height: Param::new(4.0),
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x9fb6d0),
            fog_density: Param::new(0.012),
            sky_color: hex(0x7fa6d8),
            ground_color: hex(0x3a3020),
            light_dir: [0.35, 0.3, -1.0],
            light_color: hex(0xfff0d0),
            light_intensity: Param::new(1.6),
            ambient: Param::new(0.5),
            shadows: Shadows {
                enabled: true,
                distance: 60.0,
                ..Default::default()
            },
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Clouds,
                0x3a70c0,
                0xb8cce0,
                0x8090a8,
                RaySettings {
                    variant: 0,
                    size: Param::new(1.0),
                    warp: Param::new(1.0),
                    bend: Param::new(1.0),
                    glow: Param::new(1.2),
                    fog: Param::new(1.0),
                    ..Default::default()
                },
            ),
            Layer::new(
                "Peaks",
                LayerKind::Terrain(Terrain {
                    size: 140.0,
                    cells: 128,
                    lod: true,
                    height: Param::new(16.0),
                    hills: 4,
                    roughness: Param::new(0.55),
                    scroll: 1,
                    valley: Param::new(0.4),
                    style: TerrainStyle::Solid,
                    seed: 42,
                    shape: TerrainShape::Mountains,
                    biome: Biome::Alpine,
                    liquid: Liquid {
                        kind: LiquidKind::Water,
                        level: Param::new(0.1),
                        color: hex(0x0c3848),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, 0.0, -45.0]),
            Layer::new(
                "Crystal",
                LayerKind::Mesh(mesh(
                    Primitive::Octahedron,
                    Material {
                        base_color: hex(0x101820),
                        metallic: Param::new(0.9),
                        roughness: Param::new(0.1),
                        emissive: Param::new(0.6),
                        emissive_color: hex(0x80d0ff),
                        emissive_mode: EmissiveMode::Edges,
                        ..Default::default()
                    },
                )),
            )
            .scaled(1.6)
            .at([0.0, 5.0, -12.0])
            .spin([0, 1, 0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.6),
                threshold: Param::new(1.2),
                radius: Param::new(0.7),
            },
            rays: GodRays {
                enabled: true,
                intensity: Param::new(1.0),
                length: Param::new(0.7),
                threshold: Param::new(0.8),
                flare: Param::new(0.8),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn aurora_tundra() -> Project {
    Project {
        name: "Aurora Tundra".into(),
        timing: crate::Timing {
            bpm: 80.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(20.0),
            target: [0.0, 4.0, -20.0],
            distance: Param::new(20.0),
            height: Param::new(1.0),
            fov: Param::new(65.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x060c18),
            fog_density: Param::new(0.012),
            sky_color: hex(0x305870),
            ground_color: hex(0x081018),
            light_dir: [-0.3, 0.6, -1.0],
            light_color: hex(0x60ffb0),
            light_intensity: Param::new(0.5),
            ambient: Param::new(0.6),
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Aurora,
                0x040814,
                0x20ff80,
                0x8040ff,
                RaySettings {
                    size: Param::new(1.0),
                    twist: Param::new(1.2),
                    warp: Param::new(1.0),
                    glow: Param::new(1.0),
                    ..Default::default()
                },
            ),
            Layer::new(
                "Ice",
                LayerKind::Terrain(Terrain {
                    size: 120.0,
                    cells: 112,
                    lod: true,
                    height: Param::new(10.0),
                    hills: 4,
                    roughness: Param::new(0.5),
                    scroll: 1,
                    valley: Param::new(0.5),
                    style: TerrainStyle::Solid,
                    seed: 11,
                    shape: TerrainShape::Mountains,
                    biome: Biome::Arctic,
                    liquid: Liquid {
                        kind: LiquidKind::Ice,
                        level: Param::new(0.15),
                        color: hex(0x7ab0d0),
                        glow: Param::new(1.0),
                        waves: Param::new(1.0),
                        flow: 0,
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, -1.0, -40.0]),
            Layer::new(
                "Snow",
                LayerKind::Weather(Weather {
                    kind: Precipitation::Snow,
                    count: 5000,
                    area: 16.0,
                    height: 12.0,
                    falls: 2,
                    wind: Param::new(20.0),
                    size: Param::new(0.06),
                    color: hex(0xe8f0ff),
                    intensity: Param::new(0.9),
                    ground: Param::new(0.55),
                    ..Default::default()
                }),
            )
            .at([0.0, -1.0, 0.0]),
            Layer::new(
                "Rocks",
                LayerKind::Mesh(MeshLayer {
                    source: MeshSource::Primitive(Primitive::Dodecahedron),
                    material: Material {
                        base_color: hex(0x303640),
                        roughness: Param::new(0.6),
                        flat_shading: true,
                        ..Default::default()
                    },
                    instancer: Instancer::Scatter {
                        count: 7,
                        radius: 9.0,
                        shell: false,
                        seed: 4,
                    },
                    variation: Variation {
                        seed: 2,
                        rotation: 30.0,
                        scale: 0.5,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, -0.6, -9.0])
            .stretched([1.3, 0.6, 1.0])
            .scaled(2.2),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.9),
                threshold: Param::new(0.6),
                radius: Param::new(0.8),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn dune_sea() -> Project {
    Project {
        name: "Dune Sea".into(),
        timing: crate::Timing {
            bpm: 105.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(10.0),
            target: [0.0, 1.5, -20.0],
            distance: Param::new(20.0),
            height: Param::new(3.0),
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0xc09868),
            fog_density: Param::new(0.02),
            sky_color: hex(0xd0b090),
            ground_color: hex(0x6a4828),
            light_dir: [-0.2, 0.35, -1.0],
            light_color: hex(0xffe0b0),
            light_intensity: Param::new(1.5),
            ambient: Param::new(0.5),
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Clouds,
                0x5a88b8,
                0xe0c090,
                0xc09070,
                RaySettings {
                    variant: 1,
                    warp: Param::new(0.75),
                    bend: Param::new(0.6),
                    glow: Param::new(1.5),
                    fog: Param::new(1.5),
                    ..Default::default()
                },
            ),
            Layer::new(
                "Dunes",
                LayerKind::Terrain(Terrain {
                    size: 110.0,
                    cells: 128,
                    height: Param::new(6.0),
                    hills: 3,
                    roughness: Param::new(0.4),
                    scroll: 1,
                    valley: Param::new(0.0),
                    style: TerrainStyle::Solid,
                    seed: 9,
                    shape: TerrainShape::Dunes,
                    biome: Biome::Desert,
                    liquid: Liquid {
                        kind: LiquidKind::Toxic,
                        level: Param::new(0.08),
                        color: hex(0x30ff40),
                        glow: Param::new(1.2),
                        waves: Param::new(1.0),
                        flow: 1,
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, -0.5, -40.0]),
            Layer::new(
                "Sandstorm",
                LayerKind::Weather(Weather {
                    kind: Precipitation::Dust,
                    count: 2500,
                    area: 18.0,
                    height: 8.0,
                    falls: 3,
                    wind_dir: 180.0,
                    size: Param::new(0.6),
                    color: hex(0xd0a070),
                    intensity: Param::new(0.12),
                    ..Default::default()
                }),
            )
            .at([0.0, -0.5, 0.0]),
        ],
        post: PostStack {
            rays: GodRays {
                enabled: true,
                intensity: Param::new(0.8),
                length: Param::new(0.6),
                threshold: Param::new(0.9),
                tint: hex(0xffe0b0),
                flare: Param::new(0.4),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn club_spotlights() -> Project {
    let spot =
        |name: &str, x: f32, color_a: u32, color_b: u32, pattern: LaserPattern, seed: u32| {
            Layer::new(
                name,
                LayerKind::Lasers(Lasers {
                    count: 4,
                    pattern,
                    spread: Param::new(60.0),
                    length: Param::new(16.0),
                    color_a: hex(color_a),
                    color_b: hex(color_b),
                    intensity: Param::new(2.5),
                    sweep: Param::new(30.0),
                    sweep_cycles: 2,
                    strobe: Param::new(0.3),
                    seed,
                    style: BeamStyle::Spotlight,
                    cone: ConeAngle(Param::new(16.0)),
                    pools: true,
                    ..Default::default()
                }),
            )
            .at([x, 9.0, -2.0])
            .rotated([180.0, 0.0, 0.0])
        };
    Project {
        name: "Club Spotlights".into(),
        timing: crate::Timing {
            bpm: 126.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(25.0),
            target: [0.0, 3.0, 0.0],
            distance: Param::new(16.0),
            height: Param::new(3.5),
            fov: Param::new(60.0),
            beat_shake: Param::new(0.05),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x06040c),
            fog_density: Param::new(0.02),
            sky_color: hex(0x302050),
            ground_color: hex(0x050508),
            light_intensity: Param::new(0.4),
            ambient: Param::new(0.15),
            height_fog: HeightFog {
                density: Param::new(0.06),
                height: 0.0,
                falloff: 2.5,
            },
            shadows: Shadows {
                contact: 0.7,
                ..Default::default()
            },
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Gradient,
                0x020104,
                0x100818,
                0x201030,
                RaySettings::default(),
            ),
            Layer::new(
                "Floor",
                LayerKind::Mirror(MirrorFloor {
                    base_color: hex(0x050508),
                    reflectivity: Param::new(0.5),
                    blur: Param::new(0.3),
                    ..Default::default()
                }),
            ),
            spot(
                "Spots left",
                -6.0,
                0xff3080,
                0x8040ff,
                LaserPattern::Cone,
                1,
            ),
            spot(
                "Spots right",
                6.0,
                0x30c0ff,
                0x40ffb0,
                LaserPattern::Cone,
                2,
            ),
            spot(
                "Spots middle",
                0.0,
                0xffe0a0,
                0xffffff,
                LaserPattern::Fan,
                3,
            ),
            Layer::new(
                "Speakers",
                LayerKind::Mesh(MeshLayer {
                    source: MeshSource::Primitive(Primitive::RoundedCube { radius: 0.1 }),
                    material: Material {
                        base_color: hex(0x101014),
                        metallic: Param::new(0.6),
                        roughness: Param::new(0.25),
                        texture: Some("speaker".into()),
                        emissive: Param::new(0.6).osc(Wave::Pulse, 2.0, 16),
                        emissive_color: hex(0xff3080),
                        emissive_mode: EmissiveMode::Edges,
                        ..Default::default()
                    },
                    instancer: Instancer::Grid {
                        counts: [1, 2, 1],
                        spacing: [1.0, 1.7, 1.0],
                    },
                    ..Default::default()
                }),
            )
            .at([7.0, 0.8, -4.0])
            .scaled(1.6)
            .sym(Symmetry::MirrorX),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(1.0),
                threshold: Param::new(0.9),
                radius: Param::new(0.8),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn rainbow_falls() -> Project {
    Project {
        name: "Rainbow Falls".into(),
        timing: crate::Timing {
            bpm: 90.0,
            loop_beats: 32,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(12.0),
            target: [0.0, 4.0, -18.0],
            distance: Param::new(22.0),
            height: Param::new(3.0),
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0xa8c0d8),
            fog_density: Param::new(0.01),
            sky_color: hex(0x80a8e0),
            ground_color: hex(0x384028),
            light_dir: [0.0, 0.5, 1.0],
            light_color: hex(0xfff4e0),
            light_intensity: Param::new(1.5),
            ambient: Param::new(0.55),
            height_fog: HeightFog {
                density: Param::new(0.03),
                height: 0.0,
                falloff: 1.5,
            },
            rainbow: Param::new(1.0),
            day_cycle: DayCycle {
                enabled: true,
                cycles: 1,
                start: 0.35,
                noon_height: 45.0,
                ..Default::default()
            },
            shadows: Shadows {
                enabled: true,
                distance: 50.0,
                ..Default::default()
            },
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Clouds,
                0x3c78c8,
                0xb8d0e8,
                0x8898b0,
                RaySettings {
                    variant: 0,
                    warp: Param::new(0.85),
                    bend: Param::new(0.8),
                    glow: Param::new(1.0),
                    ..Default::default()
                },
            ),
            Layer::new(
                "Valley",
                LayerKind::Terrain(Terrain {
                    size: 120.0,
                    cells: 112,
                    lod: true,
                    height: Param::new(14.0),
                    hills: 3,
                    roughness: Param::new(0.5),
                    scroll: 0,
                    valley: Param::new(0.5),
                    style: TerrainStyle::Solid,
                    seed: 8,
                    shape: TerrainShape::Mesas,
                    biome: Biome::Alpine,
                    liquid: Liquid {
                        kind: LiquidKind::Water,
                        level: Param::new(0.08),
                        color: hex(0x0c3848),
                        waves: Param::new(0.8),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, -1.0, -40.0]),
            Layer::new(
                "Waterfall",
                LayerKind::Falls(Falls {
                    width: 4.0,
                    height: 8.0,
                    push: 1.2,
                    flow: 8,
                    ..Default::default()
                }),
            )
            .at([0.0, 7.4, -22.6]),
            Layer::new(
                "Cliff",
                LayerKind::Mesh(MeshLayer {
                    source: MeshSource::Primitive(Primitive::Dodecahedron),
                    material: Material {
                        base_color: hex(0x4a463c),
                        roughness: Param::new(0.9),
                        flat_shading: true,
                        texture: Some("noise".into()),
                        ..Default::default()
                    },
                    instancer: Instancer::Wall {
                        cols: 13,
                        rows: 3,
                        spacing: 2.6,
                        curve: 0.0,
                    },
                    variation: Variation {
                        seed: 5,
                        rotation: 60.0,
                        scale: 0.3,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, -0.2, -26.8])
            .scaled(3.6),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.5),
                threshold: Param::new(1.2),
                radius: Param::new(0.7),
            },
            rays: GodRays {
                enabled: true,
                intensity: Param::new(0.8),
                threshold: Param::new(0.9),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn sunken_temple() -> Project {
    Project {
        name: "Sunken Temple".into(),
        timing: crate::Timing {
            bpm: 84.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Orbit,
            target: [0.0, 2.0, 0.0],
            distance: Param::new(13.0),
            height: Param::new(2.0),
            orbit_turns: 1,
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x03263a),
            fog_density: Param::new(0.04),
            sky_color: hex(0x2080b0),
            ground_color: hex(0x06202a),
            light_dir: [0.5, 1.0, 0.3],
            light_color: hex(0xa0e0ff),
            light_intensity: Param::new(1.2),
            ambient: Param::new(0.5),
            caustics: Caustics {
                amount: Param::new(1.6),
                scale: 1.2,
                speed: 2,
                color: hex(0x90e0ff),
                below: 100.0,
            },
            shadows: Shadows {
                enabled: true,
                strength: 0.7,
                distance: 25.0,
                ..Default::default()
            },
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Gradient,
                0x2a90c0,
                0x021624,
                0x60c0e0,
                RaySettings::default(),
            ),
            Layer::new(
                "Sea floor",
                LayerKind::Mirror(MirrorFloor {
                    size: 60.0,
                    base_color: hex(0x405848),
                    reflectivity: Param::new(0.05),
                    texture: Some("tech_panel".into()),
                    texture_scale: 0.5,
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Pillars",
                LayerKind::Mesh(MeshLayer {
                    source: MeshSource::Primitive(Primitive::Cylinder { segments: 12 }),
                    material: Material {
                        base_color: hex(0x8a9a90),
                        roughness: Param::new(0.8),
                        flat_shading: true,
                        ..Default::default()
                    },
                    instancer: Instancer::Radial {
                        count: 8,
                        radius: 6.0,
                    },
                    variation: Variation {
                        seed: 3,
                        rotation: 6.0,
                        scale: 0.3,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, 2.5, 0.0])
            .stretched([0.6, 5.0, 0.6]),
            Layer::new(
                "Idol",
                LayerKind::Mesh(mesh(
                    Primitive::Gem { facets: 8 },
                    Material {
                        base_color: hex(0x302010),
                        metallic: Param::new(0.9),
                        roughness: Param::new(0.25),
                        emissive: Param::new(1.2).osc(Wave::Sine, 0.6, 4),
                        emissive_color: hex(0xffc040),
                        emissive_mode: EmissiveMode::Edges,
                        flat_shading: true,
                        ..Default::default()
                    },
                )),
            )
            .scaled(1.3)
            .at([0.0, 1.6, 0.0])
            .spin([0, 1, 0]),
            Layer::new(
                "Bubbles",
                LayerKind::Particles(ParticleLayer {
                    emitter: Emitter::Fountain,
                    count: 300,
                    lifetimes: 2,
                    size: Param::new(0.06),
                    speed: Param::new(1.0),
                    radius: Param::new(4.0),
                    color_a: hex(0xc0f0ff),
                    color_b: hex(0x60a0c0),
                    intensity: Param::new(0.8),
                    sprite: Sprite::Ring,
                    ..Default::default()
                }),
            ),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.8),
                threshold: Param::new(0.8),
                radius: Param::new(0.8),
            },
            rays: GodRays {
                enabled: true,
                intensity: Param::new(1.2),
                length: Param::new(0.9),
                threshold: Param::new(0.25),
                tint: hex(0xa0e8ff),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn twister() -> Project {
    Project {
        name: "Twister".into(),
        timing: crate::Timing {
            bpm: 100.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(10.0),
            target: [0.0, 5.0, -20.0],
            distance: Param::new(22.0),
            height: Param::new(-1.0),
            fov: Param::new(62.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x3a4038),
            fog_density: Param::new(0.015),
            sky_color: hex(0x6a7468),
            ground_color: hex(0x202418),
            light_dir: [0.3, 0.6, -1.0],
            light_color: hex(0xd0d8c0),
            light_intensity: Param::new(0.8),
            ambient: Param::new(0.6),
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Clouds,
                0x40483e,
                0x707a68,
                0x383e36,
                RaySettings {
                    variant: 2,
                    warp: Param::new(1.0),
                    bend: Param::new(1.2),
                    glow: Param::new(0.4),
                    ..Default::default()
                },
            ),
            Layer::new(
                "Plains",
                LayerKind::Terrain(Terrain {
                    size: 120.0,
                    cells: 96,
                    lod: true,
                    height: Param::new(3.0),
                    hills: 3,
                    roughness: Param::new(0.4),
                    scroll: 1,
                    valley: Param::new(0.0),
                    style: TerrainStyle::Solid,
                    fill_color: hex(0x3a4a20),
                    seed: 4,
                    ..Default::default()
                }),
            )
            .at([0.0, -1.0, -40.0]),
            Layer::new(
                "Tornado",
                LayerKind::Particles(ParticleLayer {
                    emitter: Emitter::Tornado,
                    count: 6000,
                    lifetimes: 2,
                    size: Param::new(0.9),
                    speed: Param::new(1.0),
                    radius: Param::new(4.5),
                    color_a: hex(0x2a2a26),
                    color_b: hex(0x4a4a44),
                    intensity: Param::new(0.35),
                    smoke: true,
                    ..Default::default()
                }),
            )
            .at([0.0, -0.5, -28.0]),
            Layer::new(
                "Rain",
                LayerKind::Weather(Weather {
                    count: 5000,
                    falls: 12,
                    wind: Param::new(25.0),
                    wind_dir: 0.0,
                    intensity: Param::new(0.35),
                    ground: Param::new(0.9),
                    lightning: Lightning {
                        enabled: true,
                        per_loop: 4,
                        chance: 0.6,
                        seed: 9,
                        distance: 50.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, -1.0, 0.0]),
        ],
        post: PostStack {
            grade: Grade {
                saturation: Param::new(0.7),
                vignette: Param::new(0.6),
                grain: Param::new(0.03),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn music_reactor() -> Project {
    use crate::music::{AudioSource, MusicSettings, TimeWarp};
    Project {
        name: "Music Reactor".into(),
        timing: crate::Timing {
            bpm: 124.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Orbit,
            target: [0.0, 2.5, 0.0],
            distance: Param::new(14.0).with_music(AudioSource::KickHit, -1.2),
            height: Param::new(3.0),
            orbit_turns: 1,
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x05030a),
            fog_density: Param::new(0.02),
            sky_color: hex(0x402060),
            ground_color: hex(0x050505),
            light_intensity: Param::new(1.0),
            ambient: Param::new(0.2),
            shadows: Shadows {
                contact: 0.6,
                ..Default::default()
            },
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Nebula,
                0x040208,
                0x201040,
                0xff3080,
                RaySettings::default(),
            ),
            Layer::new(
                "Floor",
                LayerKind::Mirror(MirrorFloor {
                    reflectivity: Param::new(0.5),
                    grid: Param::new(0.4).with_music(AudioSource::KickHit, 2.0),
                    grid_color: hex(0xff3080),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Equalizer",
                LayerKind::Mesh(MeshLayer {
                    source: MeshSource::Primitive(Primitive::Cube),
                    material: Material {
                        base_color: hex(0x08080c),
                        metallic: Param::new(0.8),
                        roughness: Param::new(0.2),
                        emissive: Param::new(0.8),
                        emissive_color: hex(0x30c0ff),
                        emissive_mode: EmissiveMode::Edges,
                        flat_shading: true,
                        hue_shift: Param::new(0.0).with_music(AudioSource::Pitch, 1.0),
                        ..Default::default()
                    },
                    instancer: Instancer::Wall {
                        cols: 16,
                        rows: 1,
                        spacing: 1.1,
                        curve: 120.0,
                    },
                    variation: Variation {
                        spectrum: 3.0,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, 0.5, -3.0]),
            Layer::new(
                "Core",
                LayerKind::Mesh(mesh(
                    Primitive::Icosahedron,
                    Material {
                        base_color: hex(0x101018),
                        metallic: Param::new(0.9),
                        roughness: Param::new(0.15),
                        emissive: Param::new(0.3).with_music(AudioSource::KickHit, 4.0),
                        emissive_color: hex(0xff3080),
                        emissive_mode: EmissiveMode::Edges,
                        flat_shading: true,
                        ..Default::default()
                    },
                )),
            )
            .scaled(1.4)
            .at([0.0, 2.5, 0.0])
            .spin([1, 2, 0]),
            Layer::new(
                "Sparks",
                LayerKind::Particles(ParticleLayer {
                    emitter: Emitter::Burst,
                    count: 600,
                    lifetimes: 4,
                    size: Param::new(0.05),
                    speed: Param::new(1.0),
                    radius: Param::new(4.0),
                    color_a: hex(0xffe0a0),
                    color_b: hex(0xff3080),
                    intensity: Param::new(0.5).with_music(AudioSource::SnareHit, 4.0),
                    ..Default::default()
                }),
            )
            .at([0.0, 2.5, 0.0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(1.0),
                threshold: Param::new(0.8),
                radius: Param::new(0.8),
            },
            chroma: Chroma {
                enabled: true,
                amount: Param::new(0.0).with_music(AudioSource::KickHit, 0.012),
            },
            ..Default::default()
        },
        music: MusicSettings {
            warp: TimeWarp {
                source: AudioSource::Kick,
                amount: 1.5,
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_roundtrip_json() {
        for p in all() {
            let json = p.project.to_json();
            let back = Project::from_json(&json).unwrap();
            assert_eq!(back, p.project, "{}", p.name);
        }
    }
}
