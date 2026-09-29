//! Presets that recreate the look of particular machines and classic
//! demo effects (see `presets::all` for the gallery and its groups).

use crate::color::hex;
use crate::palette::PaletteId;
use crate::param::{Param, Wave};
use crate::retro::*;
use crate::scene::*;

fn mesh(prim: Primitive, material: Material) -> MeshLayer {
    MeshLayer {
        source: MeshSource::Primitive(prim),
        material,
        ..Default::default()
    }
}

/// A plain, matte, flat-shaded colour (untextured polygon hardware).
fn flat(color: u32) -> Material {
    Material {
        base_color: hex(color),
        metallic: Param::new(0.0),
        roughness: Param::new(0.85),
        flat_shading: true,
        rim: Param::new(0.0),
        ..Default::default()
    }
}

/// A matte textured surface.
fn textured(color: u32, tex: &str, scale: f32) -> Material {
    Material {
        base_color: hex(color),
        metallic: Param::new(0.0),
        roughness: Param::new(0.85),
        texture: Some(tex.into()),
        texture_scale: Param::new(scale),
        rim: Param::new(0.0),
        ..Default::default()
    }
}

/// Black with glowing edges: a wireframe on a vector or 1-bit display.
fn wire(color: u32, glow: f32) -> Material {
    Material {
        base_color: hex(0x000000),
        metallic: Param::new(0.0),
        roughness: Param::new(1.0),
        emissive_color: hex(color),
        emissive: Param::new(glow),
        emissive_mode: EmissiveMode::Edges,
        flat_shading: true,
        rim: Param::new(0.0),
        ..Default::default()
    }
}

fn sky(a: u32, b: u32, c: u32) -> Layer {
    Layer::new(
        "Sky",
        LayerKind::Backdrop(Backdrop {
            kind: BackdropKind::Gradient,
            color_a: hex(a),
            color_b: hex(b),
            color_c: hex(c),
            ..Default::default()
        }),
    )
}

fn backdrop(kind: BackdropKind, a: u32, b: u32, c: u32) -> Layer {
    Layer::new(
        "Background",
        LayerKind::Backdrop(Backdrop {
            kind,
            color_a: hex(a),
            color_b: hex(b),
            color_c: hex(c),
            speed: 1,
            ..Default::default()
        }),
    )
}

fn env(fog: u32, density: f32, sky: u32, ground: u32, light: u32, ambient: f32) -> Environment {
    Environment {
        fog_color: hex(fog),
        fog_density: Param::new(density),
        sky_color: hex(sky),
        ground_color: hex(ground),
        light_dir: [0.4, 0.9, 0.5],
        light_color: hex(light),
        light_intensity: Param::new(1.4),
        ambient: Param::new(ambient),
        ..Default::default()
    }
}

/// Clean post: no grain or vignette (machine palettes quantise anyway).
fn clean_post() -> PostStack {
    PostStack {
        grade: Grade {
            grain: Param::new(0.0),
            vignette: Param::new(0.0),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn crt(scanlines: f32, curvature: f32) -> Crt {
    Crt {
        enabled: true,
        scanlines: Param::new(scanlines),
        curvature: Param::new(curvature),
        noise: Param::new(0.0),
    }
}

fn timing(bpm: f32) -> crate::Timing {
    crate::Timing {
        bpm,
        loop_beats: 16,
    }
}

fn screen(p: &mut Project, preset: ScreenPreset) {
    preset.apply(&mut p.retro, &mut p.post.palette);
}

fn text(s: &str, style: TextStyle, size: f32, top: u32, bottom: u32) -> TextLayer {
    TextLayer {
        text: s.into(),
        style,
        size,
        width: 11.0,
        color_top: hex(top),
        color_bottom: hex(bottom),
        glow: Param::new(1.3),
        ..Default::default()
    }
}

/// A flat, hidden terrain that scrolls towards the camera: copies placed
/// on it rush past (things standing on a Mode 7 floor).
fn conveyor(size: f32, scroll: i32) -> Layer {
    let mut l = Layer::new(
        "Ground",
        LayerKind::Terrain(Terrain {
            size,
            cells: 8,
            height: Param::new(0.0),
            scroll,
            style: TerrainStyle::Solid,
            ..Default::default()
        }),
    )
    .at([0.0, 0.0, -size * 0.5 + 6.0]);
    l.enabled = false;
    l
}

fn on_ground(prim: Primitive, material: Material, count: u32, seed: u32, lift: f32) -> MeshLayer {
    MeshLayer {
        instancer: Instancer::OnTerrain {
            terrain: "Ground".into(),
            count,
            seed,
            align: false,
            lift,
            ground: None,
        },
        ..mesh(prim, material)
    }
}

// ---------------------------------------------------------------------------
// Consoles & arcade

/// Sega's Space Harrier (1985): a chequered floor rushing to the horizon,
/// columns flying past, pastel sky, 320 × 224.
pub fn harrier_plains() -> Project {
    let mut floor = Layer::new(
        "Chequered floor",
        LayerKind::Mode7(Mode7Floor {
            texture: Some("checker".into()),
            tile_size: 10.0,
            turns: 0,
            scroll: [0, -8],
            tint: hex(0x40e040),
            ..Default::default()
        }),
    );
    floor.transform.position[1] = 0.0;
    let mut p = Project {
        name: "Harrier Plains".into(),
        timing: timing(150.0),
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0, 1.8, -10.0],
            distance: Param::new(10.0),
            height: Param::new(0.6),
            fov: Param::new(62.0),
            ..Default::default()
        },
        environment: env(0xffc0e0, 0.0, 0xa0c0ff, 0x406030, 0xffffff, 0.8),
        layers: vec![
            sky(0x2060ff, 0xffb0d0, 0xffffff),
            floor,
            conveyor(80.0, 1),
            Layer::new(
                "Columns",
                LayerKind::Mesh(on_ground(
                    Primitive::Cylinder { segments: 8 },
                    textured(0xffd0a0, "brick", 1.0),
                    14,
                    7,
                    0.0,
                )),
            )
            .stretched([0.8, 4.0, 0.8])
            .scaled(1.0),
            Layer::new(
                "Bushes",
                LayerKind::Mesh(on_ground(
                    Primitive::Icosahedron,
                    flat(0x208020),
                    24,
                    3,
                    0.4,
                )),
            )
            .scaled(0.7),
        ],
        post: clean_post(),
        ..Default::default()
    };
    p.retro = Retro3d {
        enabled: true,
        resolution: RetroRes::Custom,
        custom: [320, 224],
        ..Default::default()
    };
    p
}

/// Sega Model 1 (Virtua Racing, 1992): untextured flat-shaded polygons at
/// 496 × 384, a low-poly track side rushing past.
pub fn polygon_arcade() -> Project {
    let mut p = Project {
        name: "Polygon Arcade".into(),
        timing: timing(140.0),
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(10.0),
            target: [0.0, 1.0, -12.0],
            distance: Param::new(12.0),
            height: Param::new(2.0),
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: env(0x80a8e0, 0.004, 0xb0d0ff, 0x406020, 0xffffff, 0.45),
        layers: vec![
            sky(0x1040c0, 0x80b0f0, 0xe0f0ff),
            Layer::new(
                "Grass",
                LayerKind::Terrain(Terrain {
                    size: 80.0,
                    cells: 24,
                    height: Param::new(1.5),
                    hills: 2,
                    roughness: Param::new(0.2),
                    scroll: 1,
                    valley: Param::new(1.0),
                    style: TerrainStyle::Solid,
                    fill_color: hex(0x3a8a28),
                    seed: 2,
                    ..Default::default()
                }),
            )
            .at([0.0, -0.5, -30.0]),
            Layer::new(
                "Trees",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::OnTerrain {
                        terrain: "Grass".into(),
                        count: 90,
                        seed: 5,
                        align: false,
                        lift: 0.0,
                        ground: None,
                    },
                    ..mesh(Primitive::Cone { segments: 5 }, flat(0x207a30))
                }),
            )
            .stretched([1.6, 4.0, 1.6])
            .scaled(1.0),
            Layer::new(
                "Road",
                LayerKind::Mesh(mesh(Primitive::Plane, flat(0x505058))),
            )
            .at([0.0, -0.08, -20.0])
            .stretched([3.5, 1.0, 60.0])
            .scaled(1.0),
            Layer::new(
                "Car",
                LayerKind::Mesh(mesh(Primitive::Cube, flat(0xd02020))),
            )
            .at([0.0, 0.2, -6.0])
            .stretched([1.2, 0.5, 2.2])
            .scaled(1.0),
            Layer::new(
                "Cockpit",
                LayerKind::Mesh(mesh(Primitive::Pyramid, flat(0x202838))),
            )
            .at([0.0, 0.62, -6.2])
            .stretched([0.8, 0.35, 0.9])
            .scaled(1.0),
        ],
        post: clean_post(),
        ..Default::default()
    };
    p.layers[4].transform.shake = Shake {
        amount: Param::new(0.03),
        per_loop: 64,
        ..Default::default()
    };
    p.retro = Retro3d {
        enabled: true,
        resolution: RetroRes::Custom,
        custom: [496, 384],
        filter: Some(TexFilter::Nearest),
        ..Default::default()
    };
    p
}

/// The SNES's Super FX (Star Fox, 1993): a flat-shaded starfighter over a
/// flat green ground, towers flashing past, all animating at 15 frames a
/// second in a small window.
pub fn super_fx_starship() -> Project {
    let mut ship = Layer::new(
        "Starfighter",
        LayerKind::Mesh(mesh(Primitive::Pyramid, flat(0xd8d8e8))),
    )
    .at([0.0, 1.9, -4.0])
    .rotated([-80.0, 0.0, 0.0])
    .stretched([0.9, 2.2, 0.5])
    .scaled(1.0);
    ship.transform.tilt = Param::new(0.0).osc(Wave::Sine, 20.0, 2);
    ship.transform.bob = Param::new(0.0).osc(Wave::Sine, 0.3, 4);
    let mut wings = Layer::new(
        "Wings",
        LayerKind::Mesh(mesh(Primitive::Pyramid, flat(0x2050e0))),
    )
    .at([0.0, 1.85, -3.8])
    .stretched([3.0, 0.2, 1.2])
    .scaled(1.0);
    wings.transform.tilt = ship.transform.tilt;
    wings.transform.bob = ship.transform.bob;
    let mut p = Project {
        name: "Super FX Starship".into(),
        timing: timing(128.0),
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0, 1.6, -8.0],
            distance: Param::new(8.0),
            height: Param::new(1.0),
            fov: Param::new(58.0),
            ..Default::default()
        },
        environment: env(0x6090d0, 0.006, 0xa0c8ff, 0x305020, 0xffffff, 0.6),
        layers: vec![
            sky(0x0820a0, 0x6090d0, 0xa0c8ff),
            Layer::new(
                "Ground",
                LayerKind::Terrain(Terrain {
                    size: 90.0,
                    cells: 8,
                    height: Param::new(0.0),
                    scroll: 2,
                    style: TerrainStyle::Solid,
                    fill_color: hex(0x208a30),
                    ..Default::default()
                }),
            )
            .at([0.0, 0.0, -39.0]),
            Layer::new(
                "Towers",
                LayerKind::Mesh(on_ground(Primitive::Cube, flat(0xe0d8c0), 40, 9, 0.0)),
            )
            .stretched([2.0, 9.0, 2.0])
            .scaled(1.0),
            ship,
            wings,
        ],
        post: clean_post(),
        ..Default::default()
    };
    // The Super FX drew about 15 frames a second.
    for l in &mut p.layers {
        l.step_fps = 15.0;
    }
    p.retro = Retro3d {
        enabled: true,
        resolution: RetroRes::Custom,
        custom: [224, 192],
        color_15bit: true,
        dither: Param::new(0.0),
        // The Super FX drew into a smaller window of the 256 × 224
        // screen, black all round.
        screen: ConsoleScreen {
            enabled: true,
            size: [216, 176],
            frame: ScreenFrame::Tv,
            border: [0.0; 3],
            inset: [0.078, 0.107],
            ..Default::default()
        },
        ..Default::default()
    };
    p
}

/// Virtual Boy: red wireframes floating in black depth, four shades of red,
/// 384 × 224.
pub fn red_visor() -> Project {
    let mut p = Project {
        name: "Red Visor".into(),
        timing: timing(120.0),
        camera: Camera {
            target: [0.0, 0.5, 0.0],
            distance: Param::new(9.0),
            height: Param::new(1.5),
            swing: Param::new(25.0),
            mode: CameraMode::Pendulum,
            fov: Param::new(50.0),
            ..Default::default()
        },
        environment: env(0x000000, 0.06, 0x000000, 0x000000, 0xff0000, 0.1),
        layers: vec![
            Layer::new(
                "Grid",
                LayerKind::Terrain(Terrain {
                    size: 40.0,
                    cells: 24,
                    height: Param::new(1.0),
                    hills: 2,
                    scroll: 1,
                    style: TerrainStyle::Wireframe,
                    line_color: hex(0xff0000),
                    glow: Param::new(1.5),
                    ..Default::default()
                }),
            )
            .at([0.0, -2.0, -8.0]),
            Layer::new(
                "Crystals",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Orbit {
                        count: 6,
                        radius: 3.0,
                        spread: 1.0,
                        speed: 1,
                        seed: 4,
                    },
                    ..mesh(Primitive::Octahedron, wire(0xff0000, 2.0))
                }),
            )
            .at([0.0, 0.8, 0.0])
            .scaled(0.7)
            .spin([1, 2, 0]),
            Layer::new(
                "Core",
                LayerKind::Mesh(mesh(Primitive::Icosahedron, wire(0xff2020, 2.5))),
            )
            .at([0.0, 0.8, 0.0])
            .scaled(1.3)
            .spin([0, 1, 1]),
        ],
        post: clean_post(),
        ..Default::default()
    };
    screen(&mut p, ScreenPreset::VirtualBoy);
    p.post.palette.dither = Param::new(0.0);
    p
}

/// Dreamcast (1999): clean 640 × 480, smooth textures, a glossy car by a
/// bright sea under a sunset sun with lens flare.
pub fn dreamcast_sunset() -> Project {
    let mut car = mesh(Primitive::RoundedCube { radius: 0.35 }, Material::default());
    MaterialPreset::CarPaint.apply(&mut car.material);
    car.material.base_color = hex(0x1040d0);
    let mut cabin = mesh(Primitive::RoundedCube { radius: 0.4 }, Material::default());
    MaterialPreset::Glass.apply(&mut cabin.material);
    cabin.material.base_color = hex(0x203040);
    let mut p = Project {
        name: "Dreamcast Sunset".into(),
        timing: timing(118.0),
        camera: Camera {
            target: [0.0, 0.7, 0.0],
            distance: Param::new(5.0),
            height: Param::new(0.9),
            swing: Param::new(40.0),
            mode: CameraMode::Pendulum,
            fov: Param::new(50.0),
            ..Default::default()
        },
        environment: Environment {
            light_dir: [-0.6, 0.3, 0.8],
            light_color: hex(0xffc080),
            light_intensity: Param::new(2.0),
            ..env(0x90b8e8, 0.008, 0x90c0ff, 0x604030, 0xffc080, 0.6)
        },
        layers: vec![
            sky(0x1850c0, 0x70b0ff, 0xffd8a8),
            Layer::new(
                "Sea",
                LayerKind::Terrain(Terrain {
                    size: 120.0,
                    cells: 48,
                    height: Param::new(2.0),
                    hills: 2,
                    scroll: 0,
                    style: TerrainStyle::Solid,
                    shape: TerrainShape::Dunes,
                    biome: Biome::Desert,
                    liquid: Liquid {
                        kind: LiquidKind::Water,
                        level: Param::new(0.7),
                        color: hex(0x0060c0),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, -3.0, -40.0]),
            Layer::new(
                "Pier",
                LayerKind::Mesh(mesh(Primitive::Cube, textured(0xc09060, "wood", 2.0))),
            )
            .at([0.0, -0.2, 0.0])
            .stretched([5.0, 0.3, 4.0])
            .scaled(1.0),
            Layer::new("Car", LayerKind::Mesh(car))
                .at([0.0, 0.5, 0.0])
                .stretched([2.4, 0.7, 1.2])
                .scaled(1.0)
                .spin([0, 1, 0]),
            Layer::new("Cabin", LayerKind::Mesh(cabin))
                .at([0.0, 0.95, 0.0])
                .stretched([1.3, 0.45, 1.05])
                .scaled(1.0)
                .spin([0, 1, 0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                ..Default::default()
            },
            rays: GodRays {
                enabled: true,
                intensity: Param::new(0.6),
                flare: Param::new(0.6),
                tint: hex(0xffc080),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    p.environment.env_light = EnvLight {
        source: EnvSource::Studio(Studio::Sunset),
        ..Default::default()
    };
    p.retro = Retro3d {
        enabled: true,
        resolution: RetroRes::R640x480,
        ..Default::default()
    };
    p
}

// ---------------------------------------------------------------------------
// PC era

/// Doom-era software rendering (1993): 320 × 200, light stepping through a
/// palette, flickering ceiling lights, a pool of green slime, a level
/// camera that never looks up or down.
pub fn hangar_base() -> Project {
    let timing = timing(125.0);
    let secs = timing.loop_seconds();
    let mut p = Project {
        name: "Hangar Base".into(),
        timing,
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(50.0),
            target: [0.0, 1.4, 0.0],
            distance: Param::new(5.5),
            // Eye level with the target: no looking up or down.
            height: Param::new(0.0),
            fov: Param::new(75.0),
            ..Default::default()
        },
        environment: Environment {
            light_style: LightStylePreset::Fluorescent.style(secs),
            ..env(0x000000, 0.0, 0x9a8a78, 0x403020, 0xfff0d0, 0.8)
        },
        layers: vec![
            Layer::new(
                "Floor",
                LayerKind::Mesh(MeshLayer {
                    subdivide: 2,
                    ..mesh(Primitive::Plane, textured(0xa09080, "metal_plate", 5.0))
                }),
            )
            .scaled(20.0),
            Layer::new(
                "Ceiling",
                LayerKind::Mesh(MeshLayer {
                    subdivide: 2,
                    ..mesh(Primitive::Plane, textured(0x807060, "tech_panel", 4.0))
                }),
            )
            .at([0.0, 4.0, 0.0])
            .scaled(20.0),
            Layer::new(
                "Walls",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 8,
                        radius: 7.0,
                    },
                    ..mesh(Primitive::Cube, textured(0xc0b0a0, "tech_panel", 1.5))
                }),
            )
            .at([0.0, 2.0, 0.0])
            .stretched([5.8, 4.0, 0.5])
            .scaled(1.0),
            Layer::new(
                "Slime",
                LayerKind::Mesh(mesh(
                    Primitive::Plane,
                    Material {
                        base_color: hex(0x40ff40),
                        texture: Some("caustics".into()),
                        texture_scale: Param::new(2.0),
                        emissive_color: hex(0x40ff20),
                        emissive: Param::new(0.8),
                        emissive_mode: EmissiveMode::Texture,
                        turbulence: Turbulence {
                            amount: Param::new(0.12),
                            waves: 1.0,
                            cycles: 4,
                        },
                        rim: Param::new(0.0),
                        ..Default::default()
                    },
                )),
            )
            .at([0.0, 0.02, 0.0])
            .scaled(3.0),
            Layer::new(
                "Barrels",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 5,
                        radius: 4.2,
                    },
                    ..mesh(
                        Primitive::Cylinder { segments: 8 },
                        textured(0x70a060, "stripes", 1.0),
                    )
                }),
            )
            .at([0.0, 0.45, 0.0])
            .stretched([0.45, 0.9, 0.45])
            .scaled(1.0),
        ],
        post: clean_post(),
        ..Default::default()
    };
    p.retro.apply_style(RetroStyle::Quake);
    p
}

/// Late-90s 3D accelerator cards: 640 × 480, bilinear textures without
/// mipmaps, 16-bit colour dithered and smoothed by the card's output
/// filter, coloured lights in a dark metal arena.
pub fn accelerator_arena() -> Project {
    let timing = timing(135.0);
    let secs = timing.loop_seconds();
    let lamp = |color: u32| Material {
        base_color: hex(color),
        emissive_color: hex(color),
        emissive: Param::new(3.0),
        glow_style: LightStylePreset::Torch.style(secs),
        rim: Param::new(0.0),
        ..Default::default()
    };
    let mut p = Project {
        name: "Accelerator Arena".into(),
        timing,
        camera: Camera {
            target: [0.0, 1.6, 0.0],
            distance: Param::new(3.6),
            height: Param::new(0.6),
            swing: Param::new(0.0),
            orbit_turns: 1,
            fov: Param::new(75.0),
            ..Default::default()
        },
        // Warm light from above, cool ambient: coloured lighting.
        environment: env(0x100808, 0.02, 0x3060c0, 0x402010, 0xff9040, 0.7),
        layers: vec![
            Layer::new(
                "Floor",
                LayerKind::Mesh(MeshLayer {
                    subdivide: 2,
                    ..mesh(Primitive::Plane, textured(0x9090a0, "metal_plate", 6.0))
                }),
            )
            .scaled(26.0),
            Layer::new(
                "Pillars",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 6,
                        radius: 5.5,
                    },
                    ..mesh(Primitive::Cube, textured(0xe0e0f0, "tech_panel", 1.0))
                }),
            )
            .at([0.0, 2.0, 0.0])
            .stretched([0.9, 4.0, 0.9])
            .scaled(1.0),
            Layer::new(
                "Orange lamps",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 3,
                        radius: 3.0,
                    },
                    ..mesh(Primitive::Sphere { detail: 2 }, lamp(0xff8020))
                }),
            )
            .at([0.0, 2.3, 0.0])
            .rotated([0.0, 30.0, 0.0])
            .scaled(0.3),
            Layer::new(
                "Blue lamps",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 3,
                        radius: 3.0,
                    },
                    ..mesh(Primitive::Sphere { detail: 2 }, lamp(0x3080ff))
                }),
            )
            .at([0.0, 2.3, 0.0])
            .rotated([0.0, 90.0, 0.0])
            .scaled(0.3),
            Layer::new(
                "Pickup",
                LayerKind::Mesh(mesh(
                    Primitive::Octahedron,
                    Material {
                        base_color: hex(0x40ff80),
                        emissive_color: hex(0x40ff80),
                        emissive: Param::new(1.2).osc(Wave::Sine, 0.6, 8),
                        flat_shading: true,
                        ..Default::default()
                    },
                )),
            )
            .at([0.0, 1.2, 0.0])
            .scaled(0.5)
            .spin([0, 2, 0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    p.layers[4].transform.bob = Param::new(0.0).osc(Wave::Sine, 0.2, 4);
    p.retro = Retro3d {
        enabled: true,
        resolution: RetroRes::R640x480,
        filter: Some(TexFilter::Bilinear),
        color_15bit: true,
        dither: Param::new(1.0),
        vi_blur: Param::new(0.6),
        ..Default::default()
    };
    p
}

/// BBC Micro Elite (1984): white wireframe ships and a spinning space
/// station on a 1-bit screen, turning at 12 frames a second.
pub fn wireframe_trader() -> Project {
    let mut p = Project {
        name: "Wireframe Trader".into(),
        timing: timing(110.0),
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0, 0.0, 0.0],
            distance: Param::new(9.0),
            height: Param::new(0.5),
            fov: Param::new(45.0),
            ..Default::default()
        },
        environment: env(0x000000, 0.0, 0x000000, 0x000000, 0xffffff, 0.0),
        layers: vec![
            backdrop(BackdropKind::Starfield, 0x000000, 0x000000, 0xffffff),
            Layer::new(
                "Station",
                LayerKind::Mesh(mesh(Primitive::Dodecahedron, wire(0xffffff, 2.0))),
            )
            .at([1.5, 0.3, -2.0])
            .scaled(2.2)
            .spin([0, 1, 0]),
            Layer::new(
                "Ships",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Orbit {
                        count: 3,
                        radius: 4.0,
                        spread: 1.5,
                        speed: 1,
                        seed: 8,
                    },
                    ..mesh(Primitive::Tetrahedron, wire(0xffffff, 2.0))
                }),
            )
            .scaled(0.7)
            .spin([1, 1, 0]),
        ],
        post: clean_post(),
        ..Default::default()
    };
    for l in &mut p.layers[1..] {
        l.step_fps = 12.0;
    }
    p.retro = Retro3d {
        enabled: true,
        screen: ConsoleScreen {
            enabled: true,
            size: [320, 256],
            frame: ScreenFrame::Tv,
            border: [0.0; 3],
            ..Default::default()
        },
        ..Default::default()
    };
    p.post.palette = PaletteFx {
        enabled: true,
        palette: PaletteId::OneBit,
        dither: Param::new(0.0),
    };
    p
}

/// A 90s desktop screensaver: glossy neon pipes winding through the
/// dark and a morphing shape, in 256 colours at 640 × 480.
pub fn screen_saver() -> Project {
    let pipe = |name: &str, curve: RibbonCurve, freq: [u32; 3], color: u32| {
        Layer::new(
            name,
            LayerKind::Ribbon(Ribbon {
                curve,
                freq,
                thickness: 0.05,
                color: hex(color),
                glow: Param::new(0.6),
                pulses: 0,
                ..Default::default()
            }),
        )
        .scaled(3.2)
    };
    let mut shape = mesh(
        Primitive::Cube,
        Material {
            base_color: hex(0xf0f0f0),
            metallic: Param::new(0.6),
            roughness: Param::new(0.2),
            flat_shading: true,
            ..Default::default()
        },
    );
    shape.morph = ShapeMorph {
        enabled: true,
        target: MeshSource::Primitive(Primitive::Sphere { detail: 3 }),
        amount: Param::new(0.5).osc(Wave::Sine, 0.5, 2),
    };
    shape.ramp = ColorRamp {
        enabled: true,
        colors: vec![hex(0xff4040), hex(0x40ff40), hex(0x4040ff)],
        cycles: 1,
        ..Default::default()
    };
    let mut p = Project {
        name: "Screen Saver".into(),
        timing: timing(100.0),
        camera: Camera {
            target: [0.0; 3],
            distance: Param::new(6.5),
            height: Param::new(1.0),
            swing: Param::new(0.0),
            orbit_turns: 1,
            fov: Param::new(50.0),
            ..Default::default()
        },
        environment: env(0x000000, 0.0, 0x8080a0, 0x202030, 0xffffff, 0.4),
        layers: vec![
            pipe("Red pipe", RibbonCurve::Knot, [2, 3, 5], 0xe02020),
            pipe("Green pipe", RibbonCurve::Lissajous, [3, 2, 1], 0x20c020),
            pipe("Blue pipe", RibbonCurve::Rose, [3, 1, 2], 0x2040e0),
            Layer::new("Flowerbox", LayerKind::Mesh(shape))
                .scaled(1.8)
                .spin([1, 1, 0]),
        ],
        post: clean_post(),
        ..Default::default()
    };
    p.retro = Retro3d {
        enabled: true,
        resolution: RetroRes::R640x480,
        ..Default::default()
    };
    p.post.palette = PaletteFx {
        enabled: true,
        palette: PaletteId::Vga,
        dither: Param::new(0.3),
    };
    p
}

// ---------------------------------------------------------------------------
// Home computers & handhelds

/// The Amiga's bouncing ball (1984): a red and white chequered ball
/// spinning and bouncing in front of a purple grid, with its shadow,
/// 320 × 256.
pub fn amiga_bounce() -> Project {
    let mut ball = Layer::new(
        "Ball",
        LayerKind::Mesh(mesh(
            Primitive::Sphere { detail: 1 },
            Material {
                base_color: hex(0xffffff),
                texture: Some("boing".into()),
                texture_scale: Param::new(1.0),
                pixelated: true,
                filter: TexFilter::Nearest,
                flat_shading: true,
                metallic: Param::new(0.0),
                roughness: Param::new(0.5),
                rim: Param::new(0.0),
                ..Default::default()
            },
        )),
    )
    .at([0.0, 1.0, 0.0])
    .rotated([0.0, 0.0, 18.0])
    .scaled(1.5)
    .spin([0, -4, 0]);
    // Bouncing: up and down four times per loop, quick at the floor.
    ball.transform.bob = Param::new(0.0).osc(Wave::Swell, 2.2, 4);
    let mut p = Project {
        name: "Amiga Bounce".into(),
        timing: timing(120.0),
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0, 1.8, 0.0],
            distance: Param::new(8.0),
            height: Param::new(0.6),
            fov: Param::new(50.0),
            ..Default::default()
        },
        environment: Environment {
            light_dir: [0.6, 0.8, 0.9],
            shadows: Shadows {
                enabled: true,
                ..Default::default()
            },
            ..env(0xa0a0a0, 0.0, 0xc0c0c0, 0x808080, 0xffffff, 0.7)
        },
        layers: vec![
            Layer::new(
                "Floor",
                LayerKind::Mirror(MirrorFloor {
                    base_color: hex(0xa8a8a8),
                    reflectivity: Param::new(0.0),
                    grid: Param::new(1.0),
                    grid_color: hex(0xa000a0),
                    grid_scale: Param::new(1.0),
                    ..Default::default()
                }),
            )
            .at([0.0, -0.4, 0.0]),
            Layer::new(
                "Wall",
                LayerKind::Mesh(mesh(
                    Primitive::Plane,
                    Material {
                        base_color: hex(0xb0a0b0),
                        texture: Some("tron_grid".into()),
                        texture_scale: Param::new(4.0),
                        hue_shift: Param::new(0.35),
                        rim: Param::new(0.0),
                        metallic: Param::new(0.0),
                        roughness: Param::new(1.0),
                        ..Default::default()
                    },
                )),
            )
            .at([0.0, 3.0, -3.0])
            .rotated([90.0, 0.0, 0.0])
            .scaled(10.0),
            ball,
        ],
        post: clean_post(),
        ..Default::default()
    };
    p.post.crt = crt(0.35, 0.1);
    screen(&mut p, ScreenPreset::AmigaLores);
    p.post.palette.dither = Param::new(0.0);
    p
}

/// A Commodore 64 cracktro: big logo with raster bars, a sine scroller,
/// spinning sprite coins over a starfield, 160 × 200 double-wide pixels
/// in the C64's 16 colours inside its border.
pub fn c64_intro() -> Project {
    let mut p = Project {
        name: "C64 Intro".into(),
        timing: timing(125.0),
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0; 3],
            distance: Param::new(10.0),
            height: Param::new(0.0),
            fov: Param::new(50.0),
            ..Default::default()
        },
        environment: env(0x000000, 0.0, 0x404040, 0x000000, 0xffffff, 0.5),
        layers: vec![
            backdrop(BackdropKind::Starfield, 0x000000, 0x352879, 0xffffff),
            Layer::new(
                "Logo",
                LayerKind::Logo(LogoLayer {
                    text: "EZ2".into(),
                    font: TextFont::Pixel,
                    y: Param::new(0.7).osc(Wave::Sine, 0.03, 2),
                    size: Param::new(0.3),
                    outline: Param::new(0.4),
                    outline_color: hex(0x000000),
                    copper: Param::new(1.0),
                    copper_bars: 5.0,
                    copper_cycles: 2,
                    copper_a: hex(0x9ad284),
                    copper_b: hex(0x6c5eb5),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Coins",
                LayerKind::Sprite(SpriteLayer {
                    image: Some("sheet_coin".into()),
                    columns: 4,
                    rows: 4,
                    cycles: 8,
                    blend: SpriteBlend::Cutout,
                    pixelated: true,
                    size: Param::new(0.9),
                    instancer: Instancer::Orbit {
                        count: 6,
                        radius: 3.2,
                        spread: 0.3,
                        speed: 1,
                        seed: 2,
                    },
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Scroller",
                LayerKind::Text(TextLayer {
                    wave: Param::new(0.5),
                    wavelength: 9.0,
                    wave_cycles: 2,
                    speed: 1,
                    ..text(
                        "HELLO SCENERS ... EZ2 PRESENTS A LOOP THAT NEVER ENDS ... ",
                        TextStyle::SineScroller,
                        0.9,
                        0xffffff,
                        0xb8c76f,
                    )
                }),
            )
            .at([0.0, -2.6, 0.0]),
        ],
        post: PostStack {
            grade: Grade {
                grain: Param::new(0.0),
                vignette: Param::new(0.0),
                beat_flash: Param::new(0.0),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    };
    p.post.crt = crt(0.5, 0.15);
    screen(&mut p, ScreenPreset::C64Multicolour);
    p.post.palette.dither = Param::new(0.4);
    p
}

/// A Game Boy adventure: a little hilly world of trees seen from above,
/// spinning coins on 8 frames a second, four greens with LCD ghosting,
/// 160 × 144.
pub fn handheld_quest() -> Project {
    let mut coins = Layer::new(
        "Coins",
        LayerKind::Mesh(MeshLayer {
            instancer: Instancer::Radial {
                count: 5,
                radius: 2.5,
            },
            ..mesh(Primitive::Cylinder { segments: 8 }, flat(0xd0d0a0))
        }),
    )
    .at([0.0, 1.0, 0.0])
    .rotated([90.0, 0.0, 0.0])
    .stretched([0.4, 0.08, 0.4])
    .scaled(1.0)
    .spin([0, 0, 2]);
    coins.step_fps = 8.0;
    let mut p = Project {
        name: "Handheld Quest".into(),
        timing: timing(120.0),
        camera: Camera {
            target: [0.0, 0.0, 0.0],
            distance: Param::new(10.0),
            height: Param::new(8.0),
            swing: Param::new(20.0),
            mode: CameraMode::Pendulum,
            fov: Param::new(45.0),
            ..Default::default()
        },
        environment: Environment {
            light_intensity: Param::new(1.0),
            ..env(0x606040, 0.0, 0x808060, 0x303020, 0xffffff, 0.3)
        },
        layers: vec![
            Layer::new(
                "Land",
                LayerKind::Terrain(Terrain {
                    size: 30.0,
                    cells: 32,
                    height: Param::new(2.0),
                    hills: 3,
                    valley: Param::new(0.0),
                    scroll: 0,
                    style: TerrainStyle::Solid,
                    fill_color: hex(0x909870),
                    liquid: Liquid {
                        kind: LiquidKind::Water,
                        level: Param::new(0.25),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, -1.0, 0.0]),
            Layer::new(
                "Trees",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::OnTerrain {
                        terrain: "Land".into(),
                        count: 45,
                        seed: 3,
                        align: false,
                        lift: 0.0,
                        ground: None,
                    },
                    ..mesh(Primitive::Cone { segments: 6 }, flat(0x305020))
                }),
            )
            .stretched([0.9, 1.8, 0.9])
            .scaled(1.0),
            coins,
        ],
        post: PostStack {
            feedback: Feedback {
                enabled: true,
                length: Param::new(0.35),
                zoom: 1.0,
                turn: 0.0,
                hue: 0.0,
            },
            ..clean_post()
        },
        ..Default::default()
    };
    screen(&mut p, ScreenPreset::GameBoy);
    p.post.palette.dither = Param::new(0.5);
    p
}

/// A ZX Spectrum isometric adventure: a chequered room with blocks and a
/// bouncing gem seen from above at an angle, 256 × 192 in the Spectrum's
/// bright colours inside a blue border.
pub fn spectrum_isometric() -> Project {
    let mut gem = Layer::new(
        "Gem",
        LayerKind::Mesh(mesh(Primitive::Octahedron, flat(0xffff00))),
    )
    .at([0.5, 1.4, 0.5])
    .scaled(0.5)
    .spin([0, 2, 0]);
    gem.transform.bob = Param::new(0.0).osc(Wave::Swell, 0.8, 4);
    gem.step_fps = 10.0;
    let wall = |name: &str, color: u32, at: [f32; 3], stretch: [f32; 3]| {
        Layer::new(
            name,
            LayerKind::Mesh(mesh(Primitive::Cube, textured(color, "brick", 1.0))),
        )
        .at(at)
        .stretched(stretch)
        .scaled(1.0)
    };
    let mut p = Project {
        name: "Spectrum Isometric".into(),
        timing: timing(120.0),
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0, 0.6, 0.0],
            distance: Param::new(22.0),
            height: Param::new(16.0),
            angle: Param::new(45.0),
            // A narrow view from far away: nearly isometric.
            fov: Param::new(22.0),
            ..Default::default()
        },
        environment: Environment {
            light_dir: [0.3, 1.0, 0.6],
            ..env(0x000000, 0.0, 0xffffff, 0x000000, 0xffffff, 0.6)
        },
        layers: vec![
            Layer::new(
                "Floor",
                LayerKind::Mesh(mesh(Primitive::Plane, textured(0x00ffff, "checker", 3.0))),
            )
            .scaled(6.0),
            wall("Back wall", 0xff00ff, [0.0, 1.0, -3.15], [6.3, 2.0, 0.3]),
            wall("Side wall", 0xff00ff, [-3.15, 1.0, 0.0], [0.3, 2.0, 6.3]),
            Layer::new(
                "Crates",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Grid {
                        counts: [2, 1, 2],
                        spacing: [2.4, 1.0, 2.4],
                    },
                    ..mesh(Primitive::Cube, textured(0xffff00, "wood", 1.0))
                }),
            )
            .at([0.3, 0.45, 0.3])
            .scaled(0.9),
            Layer::new(
                "Pillar",
                LayerKind::Mesh(mesh(Primitive::Cylinder { segments: 8 }, flat(0x00ff00))),
            )
            .at([-1.8, 1.0, -1.8])
            .stretched([0.6, 2.0, 0.6])
            .scaled(1.0),
            gem,
        ],
        post: clean_post(),
        ..Default::default()
    };
    screen(&mut p, ScreenPreset::ZxSpectrum);
    p.post.palette.dither = Param::new(0.0);
    p
}

/// A ZX Spectrum loading from tape: red and cyan pilot stripes, then the
/// picture arriving line by line in the Spectrum's memory order in black
/// and white under thin blue and yellow data stripes, then its colours,
/// all over one loop.
pub fn tape_loader() -> Project {
    let mut p = spectrum_isometric();
    p.name = "Tape Loader".into();
    // The gem holds still: a loading screen is a picture.
    p.layers[5].step_fps = 0.0;
    p.layers[5].transform.spin = [0, 0, 0];
    p.layers[5].transform.bob = Param::new(0.0);
    p.retro.screen.stripes = BorderStripes {
        mode: StripeMode::Loading,
        ..Default::default()
    };
    p
}

// ---------------------------------------------------------------------------
// Demoscene classics

/// An Amiga demo: a striped twister bar, a copper-barred logo and a sine
/// scroller over rolling copper stripes, 320 × 256.
pub fn copper_heaven() -> Project {
    let mut twister = mesh(
        Primitive::Cube,
        Material {
            base_color: hex(0xffffff),
            texture: Some("copper".into()),
            texture_scale: Param::new(1.0),
            scroll: [0, 2],
            metallic: Param::new(0.3),
            roughness: Param::new(0.4),
            ..Default::default()
        },
    );
    twister.subdivide = 3;
    twister.deform = Deform {
        twist: Param::new(0.0).osc(Wave::Sine, 1.2, 2),
        ..Default::default()
    };
    let mut p = Project {
        name: "Copper Heaven".into(),
        timing: timing(128.0),
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0; 3],
            distance: Param::new(9.0),
            height: Param::new(0.0),
            fov: Param::new(50.0),
            ..Default::default()
        },
        environment: env(0x000000, 0.0, 0xa0a0c0, 0x202030, 0xffffff, 0.6),
        layers: vec![
            Layer::new(
                "Copper stripes",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Battle,
                    color_a: hex(0x000040),
                    color_b: hex(0xff6000),
                    color_c: hex(0x2080ff),
                    battle: Battle {
                        back: BattleLayer {
                            pattern: BattlePattern::Stripes,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            ),
            Layer::new("Twister", LayerKind::Mesh(twister))
                .stretched([1.4, 5.0, 1.4])
                .scaled(1.0)
                .spin([0, 1, 0]),
            Layer::new(
                "Logo",
                LayerKind::Logo(LogoLayer {
                    text: "COPPER".into(),
                    font: TextFont::Pixel,
                    y: Param::new(0.8),
                    size: Param::new(0.18),
                    outline: Param::new(0.3),
                    outline_color: hex(0x000000),
                    shadow: Param::new(1.0),
                    copper: Param::new(1.0),
                    copper_bars: 4.0,
                    copper_cycles: 2,
                    copper_a: hex(0xffcc33),
                    copper_b: hex(0xcc3322),
                    wobble_x: Param::new(0.04),
                    wobble_waves: 1.0,
                    wobble_cycles: 2,
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Scroller",
                LayerKind::Text(TextLayer {
                    wave: Param::new(0.4),
                    wavelength: 8.0,
                    wave_cycles: 2,
                    ..text(
                        "AMIGA RULES ... GREETINGS TO ALL COPPER CHASERS ... ",
                        TextStyle::SineScroller,
                        0.7,
                        0xffffff,
                        0x88ccff,
                    )
                }),
            )
            .at([0.0, -2.8, 1.0]),
        ],
        post: clean_post(),
        ..Default::default()
    };
    p.post.crt = crt(0.35, 0.1);
    screen(&mut p, ScreenPreset::AmigaLores);
    p.post.palette.dither = Param::new(0.0);
    p
}

/// The 1993 PC demo party: a tunnel, a plasma and a rotozoomer, cut
/// together on a timeline at 320 × 200 in VGA's 256 colours.
pub fn demo_party_93() -> Project {
    use crate::sequence::{Clip, Scene, Transition, TransitionKind};
    let tunnel = || Project {
        camera: Camera {
            mode: CameraMode::Static,
            ..Default::default()
        },
        layers: vec![Layer::new(
            "Tunnel",
            LayerKind::Backdrop(Backdrop {
                kind: BackdropKind::Tunnel,
                color_a: hex(0x100020),
                color_b: hex(0x8040ff),
                color_c: hex(0xff60c0),
                speed: 2,
                texture: Some("xor".into()),
                ..Default::default()
            }),
        )],
        ..Default::default()
    };
    let plasma = || Project {
        layers: vec![backdrop(BackdropKind::Plasma, 0x200040, 0xff4080, 0x40c0ff)],
        ..Default::default()
    };
    let mut p = tunnel();
    p.name = "Demo Party 93".into();
    p.timing = timing(130.0);
    p.sequence.scene_name = "Tunnel".into();
    p.start_sequence();
    let mut clips = vec![Clip {
        scene: p.sequence.scene_id,
        beats: 8,
        transition: Transition {
            kind: TransitionKind::Wipe,
            beats: 1.0,
            angle: 0.0,
        },
    }];
    for (name, other, kind) in [
        ("Plasma", plasma(), TransitionKind::Iris),
        ("Rotozoomer", rotozoomer(), TransitionKind::Glitch),
    ] {
        let id = p.sequence.next_id();
        p.sequence.scenes.push(Scene {
            id,
            name: name.into(),
            loop_beats: 8,
            camera: other.camera,
            environment: other.environment,
            layers: other.layers,
            post: other.post,
            graph: None,
            use_graph: false,
        });
        clips.push(Clip {
            scene: id,
            beats: 8,
            transition: Transition {
                kind,
                beats: 1.0,
                angle: 0.0,
            },
        });
    }
    p.sequence.scene_beats = 8;
    p.sequence.clips = clips;
    p.sync_sequence_length();
    screen(&mut p, ScreenPreset::MsDosVga);
    p.post.palette.dither = Param::new(0.2);
    p
}

/// The rotozoomer: a picture turning and zooming under the camera (a
/// Mode 7 floor seen from straight above), 320 × 200.
pub fn rotozoomer() -> Project {
    let mut floor = Layer::new(
        "Picture",
        LayerKind::Mode7(Mode7Floor {
            texture: Some("xor".into()),
            tile_size: 3.0,
            turns: 1,
            scroll: [1, 1],
            ..Default::default()
        }),
    );
    floor.transform.scale = Param::new(1.0).osc(Wave::Sine, 0.6, 2);
    let mut p = Project {
        name: "Rotozoomer".into(),
        timing: timing(125.0),
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0, 0.0, 0.0],
            distance: Param::new(0.01),
            height: Param::new(6.0),
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: env(0x000000, 0.0, 0x808080, 0x000000, 0xffffff, 0.5),
        layers: vec![floor],
        post: clean_post(),
        ..Default::default()
    };
    screen(&mut p, ScreenPreset::MsDosVga);
    p.post.palette.dither = Param::new(0.0);
    p
}

/// Vector balls (Atari ST, Amiga): a cube of shiny balls turning and a
/// ring swinging round it, 320 × 200 in the ST's 512 colours.
pub fn vector_balls() -> Project {
    let ball = || Material {
        base_color: hex(0x3070ff),
        metallic: Param::new(0.2),
        roughness: Param::new(0.15),
        rim: Param::new(0.6),
        ..Default::default()
    };
    let mut p = Project {
        name: "Vector Balls".into(),
        timing: timing(128.0),
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0; 3],
            distance: Param::new(10.0),
            height: Param::new(1.0),
            fov: Param::new(50.0),
            ..Default::default()
        },
        environment: env(0x000000, 0.0, 0x8090ff, 0x100020, 0xffffff, 0.4),
        layers: vec![
            backdrop(BackdropKind::Starfield, 0x000000, 0x100020, 0xffffff),
            Layer::new(
                "Cube of balls",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Grid {
                        counts: [4, 4, 4],
                        spacing: [1.1, 1.1, 1.1],
                    },
                    ..mesh(Primitive::Sphere { detail: 2 }, ball())
                }),
            )
            .scaled(0.35)
            .spin([1, 2, 0]),
            Layer::new(
                "Ring of balls",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 16,
                        radius: 3.4,
                    },
                    ..mesh(
                        Primitive::Sphere { detail: 2 },
                        Material {
                            base_color: hex(0xff3060),
                            ..ball()
                        },
                    )
                }),
            )
            .rotated([60.0, 0.0, 0.0])
            .scaled(0.3)
            .spin([0, -1, 1]),
        ],
        post: clean_post(),
        ..Default::default()
    };
    p.retro = Retro3d {
        enabled: true,
        screen: ConsoleScreen {
            enabled: true,
            size: [320, 200],
            frame: ScreenFrame::Tv,
            border: [0.0; 3],
            ..Default::default()
        },
        ..Default::default()
    };
    // The Atari ST's 512 colours are the Mega Drive's cube.
    p.post.palette = PaletteFx {
        enabled: true,
        palette: PaletteId::MegaDrive,
        dither: Param::new(0.0),
    };
    p
}

/// Glenz vectors: see-through, faceted solids nested and turning against
/// each other over a moving chequerboard.
pub fn glenz_vectors() -> Project {
    // See-through faces (every other pixel left out) with bright edges:
    // the faces behind show through in the solid's colour.
    let glass = |color: u32| Material {
        base_color: hex(color),
        metallic: Param::new(0.1),
        roughness: Param::new(0.3),
        flat_shading: true,
        mesh: Param::new(0.5),
        emissive_color: hex(color),
        emissive: Param::new(1.5),
        emissive_mode: EmissiveMode::Edges,
        rim: Param::new(0.0),
        ..Default::default()
    };
    let mut floor = Layer::new(
        "Chequerboard",
        LayerKind::Mode7(Mode7Floor {
            texture: Some("checker".into()),
            tile_size: 4.0,
            turns: 0,
            scroll: [1, -2],
            tint: hex(0x8060c0),
            fog: true,
            ..Default::default()
        }),
    );
    floor.transform.position[1] = -2.5;
    Project {
        name: "Glenz Vectors".into(),
        timing: timing(125.0),
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0, 0.0, 0.0],
            distance: Param::new(8.0),
            height: Param::new(1.5),
            fov: Param::new(50.0),
            ..Default::default()
        },
        environment: env(0x100820, 0.02, 0xc0c0ff, 0x201030, 0xffffff, 0.8),
        layers: vec![
            sky(0x000010, 0x201040, 0x6040a0),
            floor,
            Layer::new(
                "Outer",
                LayerKind::Mesh(mesh(Primitive::Octahedron, glass(0xff4080))),
            )
            .scaled(2.2)
            .spin([1, 2, 0]),
            Layer::new(
                "Inner",
                // Solid: the same screen-door pattern would hide it
                // exactly behind the outer one's holes.
                LayerKind::Mesh(mesh(
                    Primitive::Octahedron,
                    Material {
                        mesh: Param::new(0.0),
                        ..glass(0x40c0ff)
                    },
                )),
            )
            .scaled(1.1)
            .spin([-2, -1, 1]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                ..Default::default()
            },
            ..clean_post()
        },
        ..Default::default()
    }
}

/// The DOS fire effect: a wall of flame licking upwards behind flickering
/// flame sprites and a logo, 320 × 200.
pub fn oldschool_fire() -> Project {
    let timing = timing(120.0);
    let secs = timing.loop_seconds();
    let mut p = Project {
        name: "Oldschool Fire".into(),
        timing,
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0, 0.0, 0.0],
            distance: Param::new(8.0),
            height: Param::new(0.0),
            fov: Param::new(50.0),
            ..Default::default()
        },
        environment: env(0x000000, 0.0, 0xffffff, 0xffffff, 0xffffff, 1.0),
        layers: vec![
            Layer::new(
                "Fire wall",
                LayerKind::Mesh(mesh(
                    Primitive::Plane,
                    Material {
                        base_color: hex(0xffffff),
                        texture: Some("fire".into()),
                        texture_scale: Param::new(1.0),
                        scroll: [0, 4],
                        emissive_color: hex(0xff7020),
                        emissive: Param::new(1.2),
                        emissive_mode: EmissiveMode::Texture,
                        turbulence: Turbulence {
                            amount: Param::new(0.08),
                            waves: 1.5,
                            cycles: 8,
                        },
                        rim: Param::new(0.0),
                        ..Default::default()
                    },
                )),
            )
            .at([0.0, 0.0, -1.0])
            .rotated([90.0, 0.0, 0.0])
            .stretched([1.8, 1.0, 1.0])
            .scaled(7.5),
            Layer::new(
                "Flames",
                LayerKind::Sprite(SpriteLayer {
                    image: Some("sheet_flame".into()),
                    columns: 4,
                    rows: 4,
                    cycles: 8,
                    random_start: true,
                    facing: SpriteFacing::Upright,
                    blend: SpriteBlend::Additive,
                    size: Param::new(2.2),
                    glow: Param::new(1.4),
                    glow_style: LightStylePreset::Candle.style(secs),
                    instancer: Instancer::Grid {
                        counts: [7, 1, 1],
                        spacing: [1.6, 1.0, 1.0],
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, -2.2, 0.5]),
            Layer::new(
                "Logo",
                LayerKind::Text(TextLayer {
                    shadow: 0.9,
                    outline: 0.5,
                    outline_color: hex(0x200000),
                    ..text("HOT LOOPS", TextStyle::Static, 1.2, 0xffff80, 0xff6000)
                }),
            )
            .at([0.0, 1.2, 1.0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                ..Default::default()
            },
            ..clean_post()
        },
        ..Default::default()
    };
    screen(&mut p, ScreenPreset::MsDosVga);
    p.post.palette.dither = Param::new(0.3);
    p
}
