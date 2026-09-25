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
            name: "Empty",
            description: "A blank stage with a floor and a sky.",
            project: empty(),
        },
    ]
}

pub fn by_name(name: &str) -> Option<Project> {
    all().into_iter().find(|p| p.name == name).map(|p| p.project)
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
        metallic: 0.9,
        roughness: 0.15,
        rim: 0.4,
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
            beat_shake: 0.15,
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x120102),
            fog_density: Param::new(0.012),
            sky_color: hex(0x802020),
            ground_color: hex(0x050505),
            light_dir: [0.2, 1.0, -0.4],
            light_color: hex(0xffe0e0),
            light_intensity: 1.2,
            ambient: 0.15,
        },
        layers: vec![
            Layer::new(
                "Red nebula",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Nebula,
                    color_a: hex(0x020001),
                    color_b: hex(0x4a0404),
                    color_c: hex(0xe02010),
                    speed: 1,
                    intensity: Param::new(0.75),
                    detail: 1.2,
                    texture: None,
                }),
            ),
            Layer::new(
                "Glossy floor",
                LayerKind::Mirror(MirrorFloor {
                    size: 60.0,
                    base_color: hex(0x030303),
                    reflectivity: 0.35,
                    blur: 0.3,
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
                    neon(red, Param::new(4.0).osc(Wave::Sine, 1.0, 4), EmissiveMode::Full),
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
                            metallic: 1.0,
                            roughness: 0.2,
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
                    speed: 1.0,
                    radius: 7.0,
                    color_a: hex(0xffb080),
                    color_b: hex(0xff1000),
                    intensity: Param::new(3.0),
                    trail: 2,
                    trail_spacing: 0.006,
                    sprite: Sprite::Glow,
                    seed: 5,
                }),
            ),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(1.1),
                threshold: 0.9,
                radius: 0.75,
            },
            chroma: Chroma {
                enabled: true,
                amount: Param::new(0.002).osc(Wave::Pulse, 0.004, 16),
            },
            grade: Grade {
                vignette: 0.45,
                contrast: 1.12,
                saturation: 1.15,
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
        metallic: 1.0,
        roughness: 0.25,
        rim: 0.6,
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
            swing: 16.0,
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
            light_intensity: 1.8,
            ambient: 0.45,
        },
        layers: vec![
            Layer::new(
                "Warm glow",
                LayerKind::Backdrop(Backdrop {
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
                    reflectivity: 0.85,
                    blur: 0.1,
                    tint: hex(0xffd890),
                    texture: Some("led_grid".into()),
                    texture_scale: 0.35,
                    grid: Param::new(0.6).osc(Wave::Sine, 0.4, 4),
                    grid_color: hex(0xffd070),
                    grid_scale: 0.5,
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
                        texture_scale: 3.0,
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
                            metallic: 0.3,
                            roughness: 0.05,
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
                    speed: 1.0,
                    radius: 8.0,
                    color_a: hex(0xfff0c0),
                    color_b: hex(0xffa040),
                    intensity: Param::new(2.5),
                    trail: 0,
                    trail_spacing: 0.01,
                    sprite: Sprite::Star,
                    seed: 9,
                }),
            )
            .at([0.0, 4.0, -3.0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(1.0),
                threshold: 1.0,
                radius: 0.8,
            },
            mirror: MirrorSplit {
                enabled: true,
                mode: MirrorSplitMode::LeftToRight,
            },
            grade: Grade {
                exposure: Param::new(1.1),
                vignette: 0.3,
                saturation: 1.05,
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
            swing: 25.0,
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
            light_intensity: 2.2,
            ambient: 0.15,
        },
        layers: vec![
            Layer::new(
                "Deep space",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Starfield,
                    color_a: hex(0x000003),
                    color_b: hex(0x061530),
                    color_c: hex(0x80b0ff),
                    speed: 1,
                    intensity: Param::new(0.6),
                    detail: 1.0,
                    texture: None,
                }),
            ),
            Layer::new(
                "Dodecahedron",
                LayerKind::Mesh(mesh(
                    Primitive::Dodecahedron,
                    Material {
                        base_color: hex(0x1a70d8),
                        metallic: 0.5,
                        roughness: 0.18,
                        flat_shading: true,
                        rim: 0.9,
                        emissive_color: hex(0x40a0ff),
                        emissive: Param::new(0.15).osc(Wave::Pulse, 0.5, 8),
                        texture: Some("noise".into()),
                        texture_scale: 1.0,
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
                            metallic: 0.2,
                            roughness: 0.3,
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
                    speed: 1.0,
                    radius: 3.6,
                    color_a: hex(0xb0d8ff),
                    color_b: hex(0x2060ff),
                    intensity: Param::new(2.5),
                    trail: 3,
                    trail_spacing: 0.004,
                    sprite: Sprite::Glow,
                    seed: 3,
                }),
            )
            .rotated([15.0, 0.0, 0.0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.9),
                threshold: 0.9,
                radius: 0.7,
            },
            chroma: Chroma {
                enabled: true,
                amount: Param::new(0.003),
            },
            grade: Grade {
                vignette: 0.5,
                contrast: 1.1,
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
                    kind: BackdropKind::Tunnel,
                    color_a: hex(0x000000),
                    color_b: hex(0x2040ff),
                    color_c: hex(0xff40c0),
                    speed: 4,
                    intensity: Param::new(1.2),
                    detail: 1.0,
                    texture: Some("xor".into()),
                }),
            ),
            Layer::new(
                "Icosahedron",
                LayerKind::Mesh(mesh(
                    Primitive::Icosahedron,
                    Material {
                        base_color: hex(0xffffff),
                        texture: Some("checker".into()),
                        texture_scale: 2.0,
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
                    speed: 1.0,
                    radius: 6.0,
                    color_a: hex(0xffffff),
                    color_b: hex(0x55ffff),
                    intensity: Param::new(2.0),
                    trail: 4,
                    trail_spacing: 0.01,
                    sprite: Sprite::Square,
                    seed: 1,
                }),
            ),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.5),
                threshold: 0.9,
                radius: 0.5,
            },
            pixelate: Pixelate {
                enabled: true,
                size: 3.0,
            },
            palette: PaletteFx {
                enabled: true,
                palette: PaletteId::Ega,
                dither: 0.7,
            },
            crt: Crt {
                enabled: true,
                scanlines: 0.5,
                curvature: 0.12,
                noise: 0.08,
            },
            grade: Grade {
                vignette: 0.2,
                grain: 0.0,
                beat_flash: 0.15,
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
                    kind: BackdropKind::Plasma,
                    color_a: hex(0x10003a),
                    color_b: hex(0xff2090),
                    color_c: hex(0x20e0ff),
                    speed: 2,
                    intensity: Param::new(0.9),
                    detail: 1.0,
                    texture: None,
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
                            metallic: 0.6,
                            roughness: 0.2,
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
                        metallic: 1.0,
                        roughness: 0.1,
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
                angle: 0.0,
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
            light_intensity: 1.5,
            ambient: 0.3,
        },
        layers: vec![
            Layer::new(
                "Sunset",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::SynthGrid,
                    color_a: hex(0x0a0020),
                    color_b: hex(0xff3080),
                    color_c: hex(0xffd030),
                    speed: 1,
                    intensity: Param::new(1.0),
                    detail: 1.0,
                    texture: None,
                }),
            ),
            Layer::new(
                "Neon grid floor",
                LayerKind::Mirror(MirrorFloor {
                    size: 80.0,
                    base_color: hex(0x080010),
                    reflectivity: 0.5,
                    blur: 0.15,
                    tint: hex(0xffa0ff),
                    grid: Param::new(2.5),
                    grid_color: hex(0xff20c0),
                    grid_scale: 0.5,
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
                            metallic: 0.8,
                            roughness: 0.2,
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
                        metallic: 1.0,
                        roughness: 0.05,
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
                threshold: 0.8,
                radius: 0.8,
            },
            crt: Crt {
                enabled: true,
                scanlines: 0.25,
                curvature: 0.0,
                noise: 0.03,
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
