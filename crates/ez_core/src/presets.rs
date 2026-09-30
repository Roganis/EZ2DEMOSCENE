//! Built-in scene templates. The first three are modelled after the
//! reference moods: a red neon crystal arena, a golden mirrored kaleido room
//! and a blue solid orbited by debris.

use crate::color::hex;
use crate::palette::PaletteId;
use crate::param::{Param, Wave};
use crate::scene::*;

/// Preset groups in the gallery, in order.
pub const CATEGORY_CONSOLES: &str = "Consoles & arcade";
pub const CATEGORY_PC: &str = "PC era";
pub const CATEGORY_HOME: &str = "Home computers & handhelds";
pub const CATEGORY_DEMO: &str = "Demoscene";
pub const CATEGORY_ENGINE: &str = "Engine showcases";
pub const CATEGORIES: [&str; 5] = [
    CATEGORY_CONSOLES,
    CATEGORY_PC,
    CATEGORY_HOME,
    CATEGORY_DEMO,
    CATEGORY_ENGINE,
];

pub struct Preset {
    pub name: &'static str,
    /// Its group in the gallery (one of [`CATEGORIES`]).
    pub category: &'static str,
    pub description: &'static str,
    pub project: Project,
}

/// The built-in preset with this name (ignoring case).
pub fn find(name: &str) -> Option<Preset> {
    all()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

pub fn all() -> Vec<Preset> {
    vec![
        Preset {
            name: "PSX Crypt",
            category: CATEGORY_CONSOLES,
            description: "A PlayStation crypt: swimming brick textures, wobbling polygons and chunky 320 × 240 pixels.",
            project: psx_crypt(),
        },
        Preset {
            name: "Saturn Ghosts",
            category: CATEGORY_CONSOLES,
            description: "Sega Saturn: checkerboard see-through ghosts and wisps in a stone hall; pillars vanish as the camera brushes them.",
            project: saturn_ghosts(),
        },
        Preset {
            name: "Fog Island",
            category: CATEGORY_CONSOLES,
            description: "Nintendo 64: soft 3-point textures, fog rolling in close to the camera, dithered colour and the video blur.",
            project: fog_island(),
        },
        Preset {
            name: "Mode 7 Circuit",
            category: CATEGORY_CONSOLES,
            description: "A SNES-style Mode 7 race: an endless track turning and scrolling to a hard horizon, coins spinning on twos, 256 × 224.",
            project: mode7_circuit(),
        },
        Preset {
            name: "Super FX Starship",
            category: CATEGORY_CONSOLES,
            description: "SNES Super FX (Star Fox): a flat-shaded starfighter over flat ground, towers flashing past, everything animating at 15 frames a second in a small window.",
            project: crate::presets_hw::super_fx_starship(),
        },
        Preset {
            name: "Harrier Plains",
            category: CATEGORY_CONSOLES,
            description: "Space Harrier (1985 arcade): a chequered floor rushing to the horizon, columns and bushes flying past, pastel sky, 320 × 224.",
            project: crate::presets_hw::harrier_plains(),
        },
        Preset {
            name: "Polygon Arcade",
            category: CATEGORY_CONSOLES,
            description: "Sega Model 1 (Virtua Racing): untextured flat-shaded polygons at 496 × 384, a low-poly car racing past cone trees.",
            project: crate::presets_hw::polygon_arcade(),
        },
        Preset {
            name: "Battle Screen",
            category: CATEGORY_CONSOLES,
            description: "A retro RPG battle: two cycling patterns wobbling line by line behind a spinning crystal foe, on a CRT.",
            project: battle_screen(),
        },
        Preset {
            name: "Red Visor",
            category: CATEGORY_CONSOLES,
            description: "Virtual Boy: red wireframe crystals floating over a wire landscape, four shades of red on black, 384 × 224.",
            project: crate::presets_hw::red_visor(),
        },
        Preset {
            name: "Dreamcast Sunset",
            category: CATEGORY_CONSOLES,
            description: "Dreamcast (1999): clean 640 × 480, a glossy car by a sparkling sea under a low sun with lens flare.",
            project: crate::presets_hw::dreamcast_sunset(),
        },
        Preset {
            name: "Slipgate Courtyard",
            category: CATEGORY_PC,
            description: "Quake: light stepping through a 256-colour palette, flickering torches, a wobbling lava pool, square embers and a two-layer sky.",
            project: slipgate_courtyard(),
        },
        Preset {
            name: "Slime Falls",
            category: CATEGORY_PC,
            description: "Quake: a slime canyon and slime waterfall wobbling with the turbulent warp, a scrolling two-layer sky and a broken-tube strobe.",
            project: slime_falls(),
        },
        Preset {
            name: "Hangar Base",
            category: CATEGORY_PC,
            description: "Early-90s software 3D: 320 × 200, light stepping through a palette, flickering tubes, a slime pool, a camera that never looks up or down.",
            project: crate::presets_hw::hangar_base(),
        },
        Preset {
            name: "Accelerator Arena",
            category: CATEGORY_PC,
            description: "Late-90s 3D cards: 640 × 480, bilinear textures without mipmaps, dithered 16-bit colour, coloured lamps in a dark metal arena.",
            project: crate::presets_hw::accelerator_arena(),
        },
        Preset {
            name: "Wireframe Trader",
            category: CATEGORY_PC,
            description: "BBC Micro Elite (1984): white wireframe ships and a space station on a 1-bit screen, turning at 12 frames a second.",
            project: crate::presets_hw::wireframe_trader(),
        },
        Preset {
            name: "Screen Saver",
            category: CATEGORY_PC,
            description: "A 90s desktop screensaver: neon pipes winding through the dark round a morphing shape, 256 colours at 640 × 480.",
            project: crate::presets_hw::screen_saver(),
        },
        Preset {
            name: "Amiga Bounce",
            category: CATEGORY_HOME,
            description: "The Amiga's bouncing ball: red and white checks spinning and bouncing before a purple grid, with its shadow, 320 × 256.",
            project: crate::presets_hw::amiga_bounce(),
        },
        Preset {
            name: "C64 Intro",
            category: CATEGORY_HOME,
            description: "A C64 cracktro: a raster-barred logo, a sine scroller and sprite coins in double-wide 160 × 200 pixels, 16 colours, inside the border.",
            project: crate::presets_hw::c64_intro(),
        },
        Preset {
            name: "Spectrum Isometric",
            category: CATEGORY_HOME,
            description: "A ZX Spectrum isometric room: bright blocks, a chequered floor and a bouncing gem, 256 × 192 in a blue border.",
            project: crate::presets_hw::spectrum_isometric(),
        },
        Preset {
            name: "Handheld Quest",
            category: CATEGORY_HOME,
            description: "Game Boy: a little world of hills and trees with spinning coins, four greens and LCD ghosting, 160 × 144.",
            project: crate::presets_hw::handheld_quest(),
        },
        Preset {
            name: "Tape Loader",
            category: CATEGORY_HOME,
            description: "A ZX Spectrum loading from tape: pilot stripes, then the picture arriving line by line in black and white under data stripes, then its colours.",
            project: crate::presets_hw::tape_loader(),
        },
        Preset {
            name: "Neon Arena",
            category: CATEGORY_DEMO,
            description: "Black glossy crystal arena, red neon strips, red nebula sky.",
            project: neon_arena(),
        },
        Preset {
            name: "Gold Kaleido Room",
            category: CATEGORY_DEMO,
            description: "Mirrored golden hall, walls of orbs, sparkling chandelier.",
            project: gold_room(),
        },
        Preset {
            name: "Retro Tunnel",
            category: CATEGORY_DEMO,
            description: "90s raymarched tunnel, warp stars, EGA palette and CRT.",
            project: retro_tunnel(),
        },
        Preset {
            name: "Plasma Kaleidoscope",
            category: CATEGORY_DEMO,
            description: "Oldschool plasma, spinning tori, full-screen kaleidoscope.",
            project: plasma_kaleido(),
        },
        Preset {
            name: "Synth Sunset",
            category: CATEGORY_DEMO,
            description: "Synthwave sun, neon grid mirror floor, floating pyramids.",
            project: synth_sunset(),
        },
        Preset {
            name: "Vector Valley",
            category: CATEGORY_DEMO,
            description: "Wireframe landscape rushing past, laser fans and a neon ribbon.",
            project: vector_valley(),
        },
        Preset {
            name: "Glitch Shrine",
            category: CATEGORY_DEMO,
            description: "A glitching Menger sponge, flashing gems and a pulsing rose.",
            project: glitch_shrine(),
        },
        Preset {
            name: "Sponge Dive",
            category: CATEGORY_DEMO,
            description: "Flight through a Menger sponge, a breathing displaced chrome orb.",
            project: sponge_dive(),
        },
        Preset {
            name: "Oldschool Intro",
            category: CATEGORY_DEMO,
            description: "Chrome logo, sine scroller and greetings over the XOR tunnel.",
            project: oldschool_intro(),
        },
        Preset {
            name: "Copper Heaven",
            category: CATEGORY_DEMO,
            description: "An Amiga demo: a striped twister, a copper-barred logo and a sine scroller over rolling copper stripes, 320 × 256.",
            project: crate::presets_hw::copper_heaven(),
        },
        Preset {
            name: "Demo Party 93",
            category: CATEGORY_DEMO,
            description: "A 1993 PC demo: a tunnel, a plasma and a rotozoomer cut together on a timeline, 320 × 200 in VGA colours.",
            project: crate::presets_hw::demo_party_93(),
        },
        Preset {
            name: "Rotozoomer",
            category: CATEGORY_DEMO,
            description: "The rotozoomer: a picture turning and zooming under the camera (a Mode 7 floor seen from above), 320 × 200.",
            project: crate::presets_hw::rotozoomer(),
        },
        Preset {
            name: "Vector Balls",
            category: CATEGORY_DEMO,
            description: "Vector balls: a cube of shiny balls and a ring of balls turning over the stars, 320 × 200 in the Atari ST's 512 colours.",
            project: crate::presets_hw::vector_balls(),
        },
        Preset {
            name: "Glenz Vectors",
            category: CATEGORY_DEMO,
            description: "Glenz vectors: see-through faceted solids nested and turning against each other over a moving chequerboard.",
            project: crate::presets_hw::glenz_vectors(),
        },
        Preset {
            name: "Oldschool Fire",
            category: CATEGORY_DEMO,
            description: "The DOS fire effect: a wall of flame licking upwards behind flickering flames and a logo, 320 × 200.",
            project: crate::presets_hw::oldschool_fire(),
        },
        Preset {
            name: "Material Gallery",
            category: CATEGORY_ENGINE,
            description: "The nine physical material presets on spinning shapes in a softbox studio: metals, rubber, car paint, glass, velvet and ceramic.",
            project: material_gallery(),
        },
        Preset {
            name: "Chrome Studio",
            category: CATEGORY_ENGINE,
            description: "Chrome, gold and plastic lit by a sunset panorama that turns once per loop, with the sun and its shadows taken from the map.",
            project: chrome_studio(),
        },
        Preset {
            name: "Cathedral Light",
            category: CATEGORY_ENGINE,
            description: "Low sunlight streaming between stone columns through hazy air onto a polished marble floor, under a sunset sky; a golden orb turns in the light.",
            project: cathedral_light(),
        },
        Preset {
            name: "Liquid Metal",
            category: CATEGORY_ENGINE,
            description: "Chrome metaballs melting together, orbited by gyroid lattice balls, all raymarched.",
            project: liquid_metal(),
        },
        Preset {
            name: "Liquid Gold",
            category: CATEGORY_ENGINE,
            description: "A rocking bowl of molten gold sloshing from side to side under a studio sky, simulated ahead of time into a loop.",
            project: liquid_gold(),
        },
        Preset {
            name: "Banners",
            category: CATEGORY_ENGINE,
            description: "A row of flags on poles, flapping in a turning, gusting wind: simulated cloth that loops.",
            project: banners(),
        },
        Preset {
            name: "Beat Demolition",
            category: CATEGORY_ENGINE,
            description: "A wall of glowing blocks blown apart on the beat and rebuilt, with pearls raining behind: rigid bodies that loop.",
            project: beat_demolition(),
        },
        Preset {
            name: "Starling Dusk",
            category: CATEGORY_ENGINE,
            description: "A flock of 600 starlings wheeling through a sunset, scattering on every bar: simulated, and looping.",
            project: starling_dusk(),
        },
        Preset {
            name: "Tesla Swarm",
            category: CATEGORY_ENGINE,
            description: "Orbiting Solid wired up: lightning arcs jump from the core to the nearest debris and around a ring of coils.",
            project: tesla_swarm(),
        },
        Preset {
            name: "Glass Galaxy",
            category: CATEGORY_ENGINE,
            description: "Galaxy Swarm behind a glass logo that bends the stars, casting shadow rays through their light, with echoes as it sways.",
            project: glass_galaxy(),
        },
        Preset {
            name: "Crystal Garden",
            category: CATEGORY_ENGINE,
            description: "Twisted crystals standing on a scrolling tundra, beads riding a halo, colours travelling along.",
            project: crystal_garden(),
        },
        Preset {
            name: "Stormy Lake",
            category: CATEGORY_ENGINE,
            description: "Rain, lightning and storm clouds over a mountain lake.",
            project: stormy_lake(),
        },
        Preset {
            name: "Lava World",
            category: CATEGORY_ENGINE,
            description: "Glowing lava rivers in volcanic canyons, embers and god rays.",
            project: lava_world(),
        },
        Preset {
            name: "Sunbeam Peaks",
            category: CATEGORY_ENGINE,
            description: "Volumetric clouds, sunbeams and lens flare over alpine peaks.",
            project: sunbeam_peaks(),
        },
        Preset {
            name: "Aurora Tundra",
            category: CATEGORY_ENGINE,
            description: "Northern lights over a frozen lake, gently falling snow.",
            project: aurora_tundra(),
        },
        Preset {
            name: "Dune Sea",
            category: CATEGORY_ENGINE,
            description: "Sand dunes under a hazy sun, a sandstorm and a toxic oasis.",
            project: dune_sea(),
        },
        Preset {
            name: "Club Spotlights",
            category: CATEGORY_ENGINE,
            description:
                "Sweeping spotlight cones in a hazy club, pools of light on a mirror floor.",
            project: club_spotlights(),
        },
        Preset {
            name: "Rainbow Falls",
            category: CATEGORY_ENGINE,
            description:
                "A day passes over a waterfall valley: rainbow, valley mist, stars at night.",
            project: rainbow_falls(),
        },
        Preset {
            name: "Sunken Temple",
            category: CATEGORY_ENGINE,
            description: "Underwater ruins with rippling caustics, sunbeams and rising bubbles.",
            project: sunken_temple(),
        },
        Preset {
            name: "Campfire Sprites",
            category: CATEGORY_ENGINE,
            description: "Sprite-sheet flames round a fire, twinkling sparkles and a ring of spinning pixel coins under the aurora.",
            project: campfire_sprites(),
        },
        Preset {
            name: "Music Reactor",
            category: CATEGORY_ENGINE,
            description:
                "Load a song: an equalizer wall, kick flashes, melody colours and time warp.",
            project: music_reactor(),
        },
        Preset {
            name: "Signal Flow",
            category: CATEGORY_ENGINE,
            description:
                "Node graph: a sequence and a smoothed random walk drive the glow and size.",
            project: signal_flow(),
        },
        Preset {
            name: "Scene Tour",
            category: CATEGORY_ENGINE,
            description: "A timeline of three scenes: wipe, iris and glitch transitions, looping as one.",
            project: scene_tour(),
        },
        Preset {
            name: "Stop-Motion Shelf",
            category: CATEGORY_ENGINE,
            description: "Clay toys animating on 12, 8 and 6 frames a second while the camera glides smoothly.",
            project: stop_motion_shelf(),
        },
        Preset {
            name: "Empty",
            category: CATEGORY_ENGINE,
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

/// Aurora Tundra with the layout and shape tools: twisted crystals stand
/// on the terrain, beads ride a glowing halo, colours travel along both.
pub fn crystal_garden() -> Project {
    let mut p = aurora_tundra();
    p.name = "Crystal Garden".into();
    // Fly a loop around the halo, lingering a little at each view.
    let point = |eye: [f32; 3], target: [f32; 3], roll: f32, fov: f32| PathPoint {
        eye,
        target,
        roll,
        fov,
    };
    p.camera.mode = CameraMode::Path;
    p.camera.path = CameraPath {
        points: vec![
            point([0.0, 4.0, 2.0], [0.0, 4.5, -20.0], 0.0, 65.0),
            point([14.0, 6.5, -12.0], [0.0, 4.0, -22.0], -8.0, 60.0),
            point([6.0, 11.0, -36.0], [0.0, 4.0, -20.0], 0.0, 55.0),
            point([-13.0, 3.5, -24.0], [2.0, 5.0, -18.0], 10.0, 70.0),
        ],
        laps: 1,
        ease: 0.4,
        cut_on: None,
        drift: 0.2,
    };
    p.layers.retain(|l| l.name != "Rocks");
    p.layers.push(
        Layer::new(
            "Crystals",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Cone { segments: 6 }),
                subdivide: 2,
                instancer: Instancer::OnTerrain {
                    terrain: "Ice".into(),
                    count: 90,
                    seed: 5,
                    align: true,
                    lift: 1.2,
                    ground: None,
                },
                variation: Variation {
                    scale: 0.5,
                    seed: 3,
                    ..Default::default()
                },
                deform: Deform {
                    twist: Param::new(0.6).osc(Wave::Sine, 0.25, 1),
                    taper: Param::new(-0.3),
                    ..Default::default()
                },
                ramp: ColorRamp {
                    enabled: true,
                    colors: vec![hex(0x40ffd0), hex(0x7a5cff), hex(0xff4fd8)],
                    mode: RampMode::Gradient,
                    cycles: 1,
                    glow: true,
                },
                material: Material {
                    base_color: hex(0x102030),
                    metallic: Param::new(0.3),
                    roughness: Param::new(0.2),
                    flat_shading: true,
                    emissive: Param::new(0.8).osc(Wave::Pulse, 0.6, 16),
                    rim: Param::new(0.6),
                    ..Default::default()
                },
                ..Default::default()
            }),
        )
        .scaled(2.4)
        .stretched([0.5, 1.8, 0.5]),
    );
    p.layers.push(
        Layer::new(
            "Halo",
            LayerKind::Ribbon(Ribbon {
                curve: RibbonCurve::Wave,
                freq: [4, 1, 1],
                thickness: 0.02,
                color: hex(0x7a5cff),
                glow: Param::new(0.6),
                pulses: 2,
                pulse_speed: 1,
                pulse_length: Param::new(0.05),
                pulse_glow: Param::new(6.0),
            }),
        )
        .at([0.0, 5.0, -20.0])
        .scaled(7.0),
    );
    p.layers.push(
        Layer::new(
            "Beads",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Primitive(Primitive::Sphere { detail: 2 }),
                instancer: Instancer::Curve {
                    curve: RibbonCurve::Wave,
                    freq: [4, 1, 1],
                    size: 7.0,
                    count: 32,
                    laps: 1,
                    align: true,
                },
                ramp: ColorRamp {
                    enabled: true,
                    colors: vec![hex(0xffffff), hex(0x40ffd0), hex(0xff4fd8)],
                    mode: RampMode::Steps,
                    cycles: -2,
                    glow: true,
                },
                material: Material {
                    emissive: Param::new(2.5),
                    ..Default::default()
                },
                ..Default::default()
            }),
        )
        .at([0.0, 5.0, -20.0])
        .scaled(0.18),
    );
    p
}

/// Retro Tunnel dressed as a 90s intro: a chrome logo, a sine scroller
/// and a greetings list.
pub fn oldschool_intro() -> Project {
    let mut p = retro_tunnel();
    p.name = "Oldschool Intro".into();
    let mut logo = Layer::new(
        "Logo",
        LayerKind::Text(TextLayer {
            text: "EZ2DEMOSCENE".into(),
            font: TextFont::Sans,
            size: 1.1,
            color_top: hex(0xffffff),
            color_bottom: hex(0x55ffff),
            glow: Param::new(1.1).osc(Wave::Pulse, 0.6, 16),
            outline: 0.5,
            outline_color: hex(0x000040),
            shadow: 0.8,
            chrome: 0.9,
            ..Default::default()
        }),
    )
    .at([0.0, 2.5, 0.0]);
    logo.transform.bob = Param::new(0.0).osc(Wave::Sine, 0.15, 2);
    p.layers.push(logo);
    p.layers.push(
        Layer::new(
            "Scroller",
            LayerKind::Text(TextLayer {
                text: "WELCOME TO THE LOOP ... NOTHING HERE EVER ENDS ... PRESS EXPORT AND SHARE IT ... ".into(),
                font: TextFont::Pixel,
                style: TextStyle::SineScroller,
                size: 0.65,
                width: 11.0,
                speed: 1,
                wave: Param::new(0.45),
                wavelength: 9.0,
                wave_cycles: 2,
                color_top: hex(0xffff55),
                color_bottom: hex(0xff55ff),
                glow: Param::new(1.8),
                shadow: 0.7,
                ..Default::default()
            }),
        )
        .at([0.0, -2.6, 0.5]),
    );
    p.layers.push(
        Layer::new(
            "Greetings",
            LayerKind::Text(TextLayer {
                text: "GREETINGS TO\nALL DEMOSCENERS\nPIXEL PUSHERS\nAND LOOP LOVERS".into(),
                font: TextFont::Mono,
                style: TextStyle::Greetings,
                size: 0.38,
                beats_per_line: 4,
                color_top: hex(0x55ff55),
                color_bottom: hex(0x55ff55),
                glow: Param::new(1.5),
                ..Default::default()
            }),
        )
        .at([0.0, -0.95, 1.5]),
    );
    p
}

/// Raymarched objects: a chrome metaball blob over a mirror, orbited by
/// small gyroid balls, casting sun shadows.
pub fn liquid_metal() -> Project {
    let mut p = orbiting_solid();
    p.name = "Liquid Metal".into();
    p.camera.target = [0.0, 1.4, 0.0];
    p.environment.shadows.enabled = true;
    p.environment.light_dir = [-0.4, 1.0, 0.5];
    p.layers
        .retain(|l| matches!(l.kind, LayerKind::Backdrop(_)));
    p.layers.push(Layer::new(
        "Floor",
        LayerKind::Mirror(MirrorFloor {
            base_color: hex(0x10141c),
            ..Default::default()
        }),
    ));
    p.layers.push(
        Layer::new(
            "Blob",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Sdf {
                    form: SdfShape::Metaballs {
                        balls: 6,
                        blend: 0.4,
                    },
                    cycles: 1,
                },
                ..mesh(
                    Primitive::Cube,
                    Material {
                        base_color: hex(0xd8e4f0),
                        metallic: Param::new(1.0),
                        roughness: Param::new(0.12),
                        rim: Param::new(0.5),
                        ..Default::default()
                    },
                )
            }),
        )
        .scaled(1.9)
        .at([0.0, 1.5, 0.0])
        .spin([0, 1, 0]),
    );
    p.layers.push(
        Layer::new(
            "Lattice moons",
            LayerKind::Mesh(MeshLayer {
                source: MeshSource::Sdf {
                    form: SdfShape::Gyroid {
                        scale: 7.0,
                        thickness: 0.1,
                    },
                    cycles: 2,
                },
                instancer: Instancer::Orbit {
                    count: 6,
                    radius: 3.0,
                    spread: 0.4,
                    speed: 1,
                    seed: 5,
                },
                ..mesh(
                    Primitive::Cube,
                    Material {
                        base_color: hex(0xff9040),
                        metallic: Param::new(0.6),
                        roughness: Param::new(0.25),
                        emissive_color: hex(0xff6010),
                        emissive_mode: EmissiveMode::Edges,
                        emissive: Param::new(1.5).osc(Wave::ExpOut, 1.0, 8),
                        ..Default::default()
                    },
                )
            }),
        )
        .scaled(0.55)
        .at([0.0, 1.5, 0.0]),
    );
    p
}

/// Sprites: flames and sparkles playing built-in sprite sheets, and pixel
/// coins spinning in a ring.
pub fn campfire_sprites() -> Project {
    let mut p = empty();
    p.name = "Campfire Sprites".into();
    p.timing.bpm = 96.0;
    p.camera.distance = Param::new(7.5);
    p.camera.height = Param::new(2.2).osc(Wave::Sine, 0.6, 1);
    p.camera.target = [0.0, 1.2, 0.0];
    p.camera.mode = CameraMode::Orbit;
    p.environment.fog_color = hex(0x060a14);
    p.environment.ambient = Param::new(0.25);
    p.post.bloom.enabled = true;
    p.layers = vec![
        sky(
            BackdropKind::Aurora,
            0x040814,
            0x20ff80,
            0x8040ff,
            RaySettings::default(),
        ),
        Layer::new(
            "Floor",
            LayerKind::Mirror(MirrorFloor {
                base_color: hex(0x101418),
                reflectivity: Param::new(0.3),
                ..Default::default()
            }),
        ),
        Layer::new(
            "Fire",
            LayerKind::Sprite(SpriteLayer {
                image: Some("sheet_flame".into()),
                columns: 4,
                rows: 4,
                cycles: 8,
                facing: SpriteFacing::Upright,
                blend: SpriteBlend::Additive,
                size: Param::new(2.4).osc(Wave::ExpOut, 0.4, 16),
                glow: Param::new(1.6),
                ..Default::default()
            }),
        )
        .at([0.0, 1.15, 0.0]),
        Layer::new(
            "Torches",
            LayerKind::Sprite(SpriteLayer {
                image: Some("sheet_flame".into()),
                columns: 4,
                rows: 4,
                cycles: 8,
                random_start: true,
                facing: SpriteFacing::Upright,
                blend: SpriteBlend::Additive,
                size: Param::new(0.8),
                glow: Param::new(1.3),
                instancer: Instancer::Radial {
                    count: 8,
                    radius: 3.0,
                },
                ..Default::default()
            }),
        )
        .at([0.0, 0.4, 0.0]),
        Layer::new(
            "Sparkles",
            LayerKind::Sprite(SpriteLayer {
                image: Some("sheet_sparkle".into()),
                columns: 4,
                rows: 4,
                cycles: 4,
                random_start: true,
                blend: SpriteBlend::Additive,
                size: Param::new(0.35),
                tint: hex(0xffd080),
                glow: Param::new(2.0),
                instancer: Instancer::Orbit {
                    count: 50,
                    radius: 2.2,
                    spread: 1.2,
                    speed: 1,
                    seed: 4,
                },
                ..Default::default()
            }),
        )
        .at([0.0, 2.0, 0.0]),
        Layer::new(
            "Coins",
            LayerKind::Sprite(SpriteLayer {
                image: Some("sheet_coin".into()),
                columns: 4,
                rows: 4,
                cycles: 4,
                random_start: true,
                facing: SpriteFacing::Upright,
                blend: SpriteBlend::Cutout,
                pixelated: true,
                size: Param::new(0.5),
                glow: Param::new(1.2),
                instancer: Instancer::Radial {
                    count: 12,
                    radius: 4.2,
                },
                ..Default::default()
            }),
        )
        .at([0.0, 1.0, 0.0])
        .spin([0, 1, 0]),
    ];
    p
}

/// Electric arcs: from the centrepiece to the nearest orbiting debris
/// (they jump as the debris moves), and copy to copy around a ring.
pub fn tesla_swarm() -> Project {
    let mut p = orbiting_solid();
    p.name = "Tesla Swarm".into();
    p.post.bloom.enabled = true;
    p.layers.push(Layer::new(
        "Core arcs",
        LayerKind::Arcs(ArcLayer {
            path: ArcPath::Nearest {
                target: "Debris swarm".into(),
                count: 6,
            },
            strikes: 16,
            jag: Param::new(0.18),
            crawl: 1.5,
            width: Param::new(0.1),
            color: hex(0x80b0ff),
            glow: Param::new(2.5).osc(Wave::ExpOut, 1.5, 8),
            ..Default::default()
        }),
    ));
    p.layers.push(
        Layer::new(
            "Coils",
            LayerKind::Mesh(MeshLayer {
                instancer: Instancer::Radial {
                    count: 10,
                    radius: 4.5,
                },
                ..mesh(
                    Primitive::Sphere { detail: 2 },
                    Material {
                        base_color: hex(0x303848),
                        metallic: Param::new(1.0),
                        roughness: Param::new(0.2),
                        emissive_color: hex(0xa060ff),
                        emissive: Param::new(0.6),
                        ..Default::default()
                    },
                )
            }),
        )
        .scaled(0.18)
        .at([0.0, -1.2, 0.0])
        .spin([0, 1, 0]),
    );
    p.layers.push(Layer::new(
        "Ring arcs",
        LayerKind::Arcs(ArcLayer {
            path: ArcPath::Chain {
                target: "Coils".into(),
            },
            strikes: 32,
            jag: Param::new(0.12),
            branches: false,
            width: Param::new(0.05),
            color: hex(0xc080ff),
            glow: Param::new(1.8),
            seed: 7,
            ..Default::default()
        }),
    ));
    p
}

/// A big swarm: a spiral galaxy of shards (compute shader on desktop and
/// WebGPU, CPU fallback elsewhere).
pub fn galaxy_swarm() -> Project {
    let mut p = orbiting_solid();
    p.name = "Galaxy Swarm".into();
    p.post.bloom.enabled = true;
    p.camera.distance = Param::new(11.0).osc(Wave::Sine, 1.5, 1);
    p.camera.height = Param::new(4.0).osc(Wave::Sine, 1.5, 1);
    p.layers
        .retain(|l| matches!(l.kind, LayerKind::Backdrop(_)));
    p.layers.push(
        Layer::new(
            "Galaxy",
            LayerKind::Mesh(MeshLayer {
                instancer: Instancer::Swarm {
                    form: SwarmForm::Galaxy,
                    count: 40_000,
                    radius: 7.0,
                    spread: 1.5,
                    speed: 1,
                    seed: 2,
                },
                variation: Variation {
                    scale: 0.6,
                    rotation: 90.0,
                    ..Default::default()
                },
                ramp: ColorRamp {
                    enabled: true,
                    colors: vec![hex(0x80c0ff), hex(0xff60d0), hex(0xffe0a0)],
                    cycles: 1,
                    ..Default::default()
                },
                ..mesh(
                    Primitive::Tetrahedron,
                    Material {
                        base_color: hex(0x202030),
                        emissive_color: hex(0xffffff),
                        emissive: Param::new(0.5).osc(Wave::ExpOut, 0.4, 8),
                        ..Default::default()
                    },
                )
            }),
        )
        .scaled(0.03),
    );
    p.layers.push(
        Layer::new(
            "Core",
            LayerKind::Mesh(mesh(
                Primitive::Sphere { detail: 3 },
                Material {
                    base_color: hex(0xfff0d0),
                    emissive_color: hex(0xffe0b0),
                    emissive: Param::new(1.5),
                    ..Default::default()
                },
            )),
        )
        .scaled(0.5),
    );
    p
}

/// Logos on the screen over Synth Sunset: a chrome title that pulses on
/// every beat and sways, and a small pixel tag in the corner.
pub fn sunset_title() -> Project {
    let mut p = synth_sunset();
    p.name = "Sunset Title".into();
    p.layers.push(Layer::new(
        "Title",
        LayerKind::Logo(LogoLayer {
            text: "EZ2DEMOSCENE".into(),
            font: TextFont::Sans,
            y: Param::new(0.8).osc(Wave::Sine, 0.015, 2),
            size: Param::new(0.12),
            rotation: Param::new(0.0).osc(Wave::Sine, 2.5, 1),
            color_top: hex(0xffffff),
            color_bottom: hex(0x30d8ff),
            glow: Param::new(1.3).osc(Wave::ExpOut, 0.9, 16),
            outline: Param::new(0.6),
            outline_color: hex(0x10002a),
            shadow: Param::new(1.0),
            chrome: Param::new(0.45),
            bevel: LogoBevel::Round,
            bevel_width: Param::new(0.5),
            light_angle: Param::new(120.0),
            glint: Param::new(1.2),
            glint_cycles: 4,
            ..Default::default()
        }),
    ));
    p.layers.push(Layer::new(
        "Tag",
        LayerKind::Logo(LogoLayer {
            text: "LOOP 4EVER".into(),
            font: TextFont::Pixel,
            x: Param::new(0.97),
            y: Param::new(0.05),
            anchor: LogoAnchor::BottomRight,
            size: Param::new(0.055),
            color_top: hex(0xffff80),
            color_bottom: hex(0xff8040),
            glow: Param::new(1.4),
            shadow: Param::new(0.9),
            ..Default::default()
        }),
    ));
    p
}

/// Distance-field logo effects over Neon Arena: a morph between two words
/// (held, then quick: the wave overshoots and is clamped), an extrusion,
/// stacked outlines and rings on every beat.
pub fn logo_morph() -> Project {
    let mut p = neon_arena();
    p.name = "Logo Morph".into();
    p.layers.push(Layer::new(
        "Logo",
        LayerKind::Logo(LogoLayer {
            text: "EZ2".into(),
            font: TextFont::Sans,
            size: Param::new(0.3),
            color_top: hex(0xffffff),
            color_bottom: hex(0xffd23c),
            glow: Param::new(1.2),
            bevel: LogoBevel::Round,
            bevel_width: Param::new(0.6),
            morph: Param::new(0.5).osc(Wave::Sine, 1.5, 1),
            morph_text: "LOOP".into(),
            extrude: Param::new(0.12),
            extrude_angle: -55.0,
            extrude_color: hex(0xc03000),
            stack: 2,
            stack_width: Param::new(0.025),
            stack_color_a: hex(0x101010),
            stack_color_b: hex(0xff2bd6),
            contours: Param::new(0.0).osc(Wave::ExpOut, 1.2, 16),
            contour_cycles: 16,
            contour_spacing: 0.1,
            contour_reach: 0.3,
            contour_color: hex(0x40e0ff),
            ..Default::default()
        }),
    ));
    p
}

/// Rasters on a logo over the Retro Tunnel: copper bars, a sine sway and
/// a raster glitch on every beat.
pub fn copper_logo() -> Project {
    let mut p = retro_tunnel();
    p.name = "Copper Logo".into();
    p.layers.push(Layer::new(
        "Logo",
        LayerKind::Logo(LogoLayer {
            text: "AMIGA".into(),
            font: TextFont::Pixel,
            y: Param::new(0.62),
            size: Param::new(0.28),
            outline: Param::new(0.3),
            outline_color: hex(0x000000),
            shadow: Param::new(1.0),
            copper: Param::new(1.0),
            copper_bars: 4.0,
            copper_cycles: 2,
            copper_a: hex(0xff4020),
            copper_b: hex(0x20c0ff),
            wobble_x: Param::new(0.05),
            wobble_waves: 1.0,
            wobble_cycles: 2,
            glitch: Param::new(0.0).osc(Wave::ExpOut, 0.12, 16),
            glitch_chance: 0.35,
            glitch_split: 0.03,
            ..Default::default()
        }),
    ));
    p
}

/// Retro looks on a logo over Vector Valley: a C64 palette ramp cycling,
/// a pixelate-in at the start of every loop and scanlines.
pub fn c64_title() -> Project {
    use crate::palette::PaletteId;
    let mut p = vector_valley();
    p.name = "C64 Title".into();
    p.layers.push(Layer::new(
        "Title",
        LayerKind::Logo(LogoLayer {
            text: "READY.".into(),
            font: TextFont::Pixel,
            attach_point: LogoAnchor::Top,
            anchor: LogoAnchor::Top,
            x: Param::new(0.0),
            y: Param::new(-0.1),
            size: Param::new(0.22),
            copper: Param::new(1.0),
            copper_bars: 2.0,
            copper_cycles: 1,
            copper_a: hex(0xffffff),
            copper_b: hex(0x202020),
            palette: Some(PaletteId::C64),
            palette_by_brightness: true,
            palette_cycles: 2,
            dither: 0.6,
            pixelate: Param::new(0.0).osc(Wave::ExpOut, 0.12, 1),
            scanlines: Param::new(0.5),
            scanline_count: 24.0,
            crt_glow: Param::new(0.4),
            shadow: Param::new(0.8),
            ..Default::default()
        }),
    ));
    p
}

/// A logo meeting the scene over Galaxy Swarm: glass letters bending the
/// swarm, shadow rays through its light and echoes of a gentle sway.
pub fn glass_galaxy() -> Project {
    let mut p = galaxy_swarm();
    p.name = "Glass Galaxy".into();
    p.layers.push(Layer::new(
        "Logo",
        LayerKind::Logo(LogoLayer {
            text: "GALAXY".into(),
            font: TextFont::Sans,
            attach_point: LogoAnchor::Centre,
            x: Param::new(0.0).osc(Wave::Sine, 0.04, 1),
            y: Param::new(0.0).osc(Wave::Sine, 0.03, 2),
            size: Param::new(0.24),
            bevel: LogoBevel::Round,
            bevel_width: Param::new(0.8),
            shine: Param::new(1.2),
            glass: Param::new(1.0),
            refraction: 0.12,
            dispersion: 0.35,
            glass_tint: hex(0xd8ecff),
            rays: Param::new(1.2),
            rays_shadow: true,
            rays_threshold: 0.3,
            rays_length: 0.7,
            rays_tint: hex(0xbfd8ff),
            echoes: 3,
            echo_spacing: 0.015,
            echo_fade: 0.5,
            ..Default::default()
        }),
    ));
    p
}

/// Neon Arena recoloured by a colour scheme: a triad of hues from one key
/// colour that turns once per loop, the lights and darks as they were.
pub fn colour_wheel_arena() -> Project {
    let mut p = neon_arena();
    p.name = "Colour Wheel Arena".into();
    p.color_scheme = ColorScheme {
        enabled: true,
        key: hex(0x20c0ff),
        key_turn: Param::new(0.0).osc(Wave::Saw, 180.0, 1),
        harmony: Harmony::Triadic,
        hue_pull: 1.0,
        chroma_match: 0.4,
        environment: true,
        // Greys stay grey: the lights and darks as they were.
        tint_greys: 0.0,
    };
    p
}

/// Three presets as scenes on one timeline, each coming in with its own
/// transition.
pub fn scene_tour() -> Project {
    use crate::sequence::{Clip, Scene, Transition, TransitionKind};
    let mut p = synth_sunset();
    p.name = "Scene Tour".into();
    p.timing.bpm = 120.0;
    p.sequence.scene_name = "Sunset".into();
    p.start_sequence();
    let mut clips = vec![Clip {
        scene: p.sequence.scene_id,
        beats: 8,
        transition: Transition {
            kind: TransitionKind::Glitch,
            beats: 2.0,
            angle: 0.0,
        },
    }];
    for (name, other, kind) in [
        ("Tunnel", retro_tunnel(), TransitionKind::Wipe),
        ("Kaleidoscope", plasma_kaleido(), TransitionKind::Iris),
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
                beats: 2.0,
                angle: 20.0,
            },
        });
    }
    p.sequence.scene_beats = 8;
    p.sequence.clips = clips;
    p.sync_sequence_length();
    p
}

/// A PlayStation crypt: big brick polygons whose textures swim (affine
/// warp), vertices that wobble on a 320 × 240 grid and chunky pixels.
pub fn psx_crypt() -> Project {
    let nearest = |color: u32, tex: &str, scale: f32| Material {
        base_color: hex(color),
        metallic: Param::new(0.0),
        roughness: Param::new(0.9),
        texture: Some(tex.into()),
        texture_scale: Param::new(scale),
        pixelated: true,
        filter: crate::TexFilter::Nearest,
        rim: Param::new(0.0),
        ..Default::default()
    };
    let mut p = Project {
        name: "PSX Crypt".into(),
        timing: crate::Timing {
            bpm: 96.0,
            loop_beats: 16,
        },
        camera: Camera {
            target: [0.0, 1.4, 0.0],
            distance: Param::new(4.6),
            height: Param::new(0.6).osc(Wave::Sine, 0.5, 2),
            swing: Param::new(0.0),
            orbit_turns: 1,
            fov: Param::new(62.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x1c1410),
            fog_density: Param::new(0.05),
            sky_color: hex(0xb08860),
            ground_color: hex(0x604030),
            light_dir: [0.5, 0.8, 0.3],
            light_color: hex(0xffd8a8),
            light_intensity: Param::new(1.8),
            ambient: Param::new(0.9),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sky",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Gradient,
                    color_a: hex(0x000000),
                    color_b: hex(0x120c0a),
                    color_c: hex(0x2a1810),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Floor",
                LayerKind::Mesh(mesh(Primitive::Plane, nearest(0xb09070, "wood", 5.0))),
            )
            .scaled(14.0),
            Layer::new(
                "Walls",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 8,
                        radius: 5.2,
                    },
                    ..mesh(Primitive::Cube, nearest(0xf0d8c8, "brick", 2.0))
                }),
            )
            .at([0.0, 2.2, 0.0])
            .stretched([2.2, 2.4, 0.3])
            .scaled(1.0),
            Layer::new(
                "Pillars",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 4,
                        radius: 2.8,
                    },
                    ..mesh(Primitive::Cube, nearest(0xe0e4f0, "metal_plate", 1.0))
                }),
            )
            .at([0.0, 1.4, 0.0])
            .stretched([0.45, 1.6, 0.45])
            .scaled(1.0),
            Layer::new(
                "Idol",
                LayerKind::Mesh(mesh(
                    Primitive::Gem { facets: 6 },
                    Material {
                        base_color: hex(0xffc040),
                        metallic: Param::new(0.9),
                        roughness: Param::new(0.3),
                        emissive_color: hex(0xff8020),
                        emissive: Param::new(0.6).osc(Wave::Pulse, 1.2, 16),
                        texture: Some("marble".into()),
                        pixelated: true,
                        filter: crate::TexFilter::Nearest,
                        flat_shading: true,
                        ..Default::default()
                    },
                )),
            )
            .at([0.0, 1.4, 0.0])
            .scaled(0.7)
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
    p.layers[4].transform.bob = Param::new(0.0).osc(Wave::Sine, 0.15, 4);
    if let LayerKind::Mesh(m) = &mut p.layers[4].kind {
        m.material.glow_style = crate::LightStylePreset::Candle.style(p.timing.loop_seconds());
    }
    p.retro.apply_style(crate::RetroStyle::Ps1);
    // Walls pop open when the camera brushes them, as on the console.
    p.retro.near_cull = Param::new(0.3);
    p
}

/// A 16:9 flight over a textured landscape at 256 × 224 (wider, keeping
/// the console's pixel shape), with wobbling hills and a stage title kept
/// sharp on top.
pub fn stage_select() -> Project {
    let mut p = Project {
        name: "Stage Select".into(),
        timing: crate::Timing {
            bpm: 128.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(12.0),
            target: [0.0, 1.0, -12.0],
            distance: Param::new(12.0),
            height: Param::new(2.6),
            fov: Param::new(60.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x80a8e8),
            fog_density: Param::new(0.025),
            sky_color: hex(0xa0c0ff),
            ground_color: hex(0x304020),
            light_dir: [0.5, 0.8, 0.3],
            light_color: hex(0xfff0d0),
            light_intensity: Param::new(1.4),
            ambient: Param::new(0.5),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sky",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Gradient,
                    color_a: hex(0x2040c0),
                    color_b: hex(0x80a8e8),
                    color_c: hex(0xe0e8ff),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Land",
                LayerKind::Terrain(Terrain {
                    size: 80.0,
                    cells: 48,
                    height: Param::new(3.5),
                    hills: 3,
                    roughness: Param::new(0.3),
                    scroll: 1,
                    valley: Param::new(0.4),
                    style: TerrainStyle::Solid,
                    fill_color: hex(0x70c050),
                    texture: Some("checker".into()),
                    tiles: 16,
                    pixelated: true,
                    seed: 4,
                    ..Default::default()
                }),
            )
            .at([0.0, -1.0, -30.0]),
            Layer::new(
                "Coins",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 6,
                        radius: 2.2,
                    },
                    ..mesh(
                        Primitive::Star {
                            points: 5,
                            inner: 0.5,
                            depth: 0.3,
                        },
                        Material {
                            base_color: hex(0xffd030),
                            metallic: Param::new(0.8),
                            roughness: Param::new(0.35),
                            emissive_color: hex(0xffa000),
                            emissive: Param::new(0.4),
                            flat_shading: true,
                            ..Default::default()
                        },
                    )
                }),
            )
            .at([0.0, 1.6, -8.0])
            .scaled(0.45)
            .spin([0, 1, 0]),
            Layer::new(
                "Title",
                LayerKind::Text(TextLayer {
                    text: "STAGE 1".into(),
                    size: 0.9,
                    color_top: hex(0xffffff),
                    color_bottom: hex(0xffd030),
                    outline: 0.6,
                    outline_color: hex(0x102060),
                    shadow: 0.8,
                    face_camera: true,
                    ..Default::default()
                }),
            )
            .at([0.0, 3.4, -8.0]),
        ],
        ..Default::default()
    };
    p.retro = crate::Retro3d {
        enabled: true,
        resolution: crate::RetroRes::R256x224,
        snap: true,
        snap_res: [256, 224],
        affine: Param::new(0.8),
        ..Default::default()
    };
    p
}

/// A Nintendo 64 island: soft 3-point filtered grass and stone, fog that
/// starts close to the camera, dithered colour smoothed by the video blur.
pub fn fog_island() -> Project {
    let soft = |color: u32, tex: &str, scale: f32| Material {
        base_color: hex(color),
        metallic: Param::new(0.0),
        roughness: Param::new(0.8),
        texture: Some(tex.into()),
        texture_scale: Param::new(scale),
        filter: crate::TexFilter::ThreePoint,
        rim: Param::new(0.0),
        ..Default::default()
    };
    let mut p = Project {
        name: "Fog Island".into(),
        timing: crate::Timing {
            bpm: 110.0,
            loop_beats: 16,
        },
        camera: Camera {
            target: [0.0, 2.0, 0.0],
            distance: Param::new(9.0),
            height: Param::new(2.5),
            swing: Param::new(0.0),
            orbit_turns: 1,
            fov: Param::new(58.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0xa8b8d0),
            fog_density: Param::new(0.0),
            sky_color: hex(0xc0d0f0),
            ground_color: hex(0x405030),
            light_dir: [0.4, 0.9, 0.3],
            light_color: hex(0xfff4e0),
            light_intensity: Param::new(1.5),
            ambient: Param::new(0.6),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sky",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Gradient,
                    color_a: hex(0x5078c0),
                    color_b: hex(0xa8b8d0),
                    color_c: hex(0xa8b8d0),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Hills",
                LayerKind::Terrain(Terrain {
                    size: 90.0,
                    cells: 64,
                    height: Param::new(4.0),
                    hills: 3,
                    roughness: Param::new(0.3),
                    scroll: 0,
                    valley: Param::new(0.8),
                    style: TerrainStyle::Solid,
                    fill_color: hex(0x68b040),
                    texture: Some("noise".into()),
                    tiles: 12,
                    seed: 21,
                    ..Default::default()
                }),
            )
            .at([0.0, -0.5, 0.0]),
            Layer::new(
                "Tower",
                LayerKind::Mesh(mesh(Primitive::Cube, soft(0xe8e0d0, "brick", 1.5))),
            )
            .at([0.0, 2.2, 0.0])
            .stretched([1.4, 3.6, 1.4])
            .scaled(1.0),
            Layer::new(
                "Roof",
                LayerKind::Mesh(mesh(
                    Primitive::Cone { segments: 8 },
                    soft(0xd03020, "metal_plate", 2.0),
                )),
            )
            .at([0.0, 4.55, 0.0])
            .stretched([1.3, 1.1, 1.3])
            .scaled(1.0),
            Layer::new(
                "Stars",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 8,
                        radius: 4.5,
                    },
                    ..mesh(
                        Primitive::Star {
                            points: 5,
                            inner: 0.5,
                            depth: 0.35,
                        },
                        Material {
                            base_color: hex(0xffd020),
                            metallic: Param::new(0.6),
                            roughness: Param::new(0.4),
                            emissive_color: hex(0xffb000),
                            emissive: Param::new(0.5),
                            flat_shading: true,
                            ..Default::default()
                        },
                    )
                }),
            )
            .at([0.0, 1.6, 0.0])
            .scaled(0.5)
            .spin([0, -1, 0]),
        ],
        ..Default::default()
    };
    p.layers[4].transform.bob = Param::new(0.0).osc(Wave::Sine, 0.25, 8);
    // Collectibles spin on twos.
    p.layers[4].step_fps = 12.0;
    p.retro.apply_style(crate::RetroStyle::N64);
    p.retro.fog.near = Param::new(3.0);
    p.retro.fog.far = Param::new(30.0);
    p
}

/// Saturn ghosts: see-through "mesh" shapes and sprites (a checkerboard of
/// left-out pixels instead of blending) drifting through a stone hall at
/// 320 × 224; pillars the camera brushes past vanish (near-plane culling).
pub fn saturn_ghosts() -> Project {
    let stone = |color: u32, tex: &str, scale: f32| Material {
        base_color: hex(color),
        metallic: Param::new(0.0),
        roughness: Param::new(0.9),
        texture: Some(tex.into()),
        texture_scale: Param::new(scale),
        rim: Param::new(0.0),
        ..Default::default()
    };
    let mut p = Project {
        name: "Saturn Ghosts".into(),
        timing: crate::Timing {
            bpm: 90.0,
            loop_beats: 16,
        },
        camera: Camera {
            target: [0.0, 1.5, 0.0],
            distance: Param::new(4.2),
            height: Param::new(0.4),
            swing: Param::new(0.0),
            orbit_turns: 1,
            fov: Param::new(66.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x101030),
            fog_density: Param::new(0.05),
            sky_color: hex(0x8090d0),
            ground_color: hex(0x403860),
            light_dir: [-0.3, 1.0, 0.4],
            light_color: hex(0xd0d8ff),
            light_intensity: Param::new(1.6),
            ambient: Param::new(1.1),
            ..Default::default()
        },
        layers: vec![
            // The Saturn drew big floors with its VDP2 background
            // processor: an endless flat plane, not polygons.
            Layer::new(
                "Floor",
                LayerKind::Mode7(Mode7Floor {
                    texture: Some("metal_plate".into()),
                    tile_size: 3.0,
                    turns: 0,
                    scroll: [0, 0],
                    tint: hex(0x9090c0),
                    fog: true,
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Pillars",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 6,
                        radius: 3.9,
                    },
                    ..mesh(Primitive::Cube, stone(0xd0c8e0, "sierpinski", 1.0))
                }),
            )
            .at([0.0, 2.0, 0.0])
            .stretched([0.6, 4.0, 0.6])
            .scaled(1.0),
            Layer::new(
                "Ghosts",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Orbit {
                        count: 5,
                        radius: 1.8,
                        spread: 0.6,
                        speed: 1,
                        seed: 3,
                    },
                    ..mesh(
                        Primitive::Capsule {
                            length: 0.6,
                            segments: 12,
                        },
                        Material {
                            base_color: hex(0xe0f0ff),
                            emissive_color: hex(0x80c0ff),
                            emissive: Param::new(0.8),
                            mesh: Param::new(0.5),
                            rim: Param::new(1.0),
                            ..Default::default()
                        },
                    )
                }),
            )
            .at([0.0, 1.6, 0.0])
            .scaled(0.35),
            Layer::new(
                "Wisps",
                LayerKind::Sprite(SpriteLayer {
                    blend: SpriteBlend::Mesh,
                    tint: hex(0x90e0ff),
                    glow: Param::new(1.5),
                    size: Param::new(0.5),
                    instancer: Instancer::Scatter {
                        count: 24,
                        radius: 3.0,
                        shell: false,
                        seed: 7,
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, 2.0, 0.0])
            .spin([0, 1, 0]),
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
    p.retro.apply_style(crate::RetroStyle::Saturn);
    p.retro.near_cull = Param::new(0.9);
    p
}

/// A Quake courtyard: light stepping through a 256-colour palette,
/// torches flickering by light styles, a turbulent lava pool, square
/// embers and a two-layer sky.
pub fn slipgate_courtyard() -> Project {
    let timing = crate::Timing {
        bpm: 120.0,
        loop_beats: 16,
    };
    let secs = timing.loop_seconds();
    let stone = |color: u32, tex: &str, scale: f32| Material {
        base_color: hex(color),
        metallic: Param::new(0.0),
        roughness: Param::new(0.9),
        texture: Some(tex.into()),
        texture_scale: Param::new(scale),
        rim: Param::new(0.0),
        ..Default::default()
    };
    let mut p = Project {
        name: "Slipgate Courtyard".into(),
        timing,
        camera: Camera {
            target: [0.0, 1.2, 0.0],
            distance: Param::new(6.5),
            height: Param::new(1.4),
            swing: Param::new(0.0),
            orbit_turns: 1,
            fov: Param::new(70.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x100808),
            fog_density: Param::new(0.0),
            sky_color: hex(0x806050),
            ground_color: hex(0x402820),
            light_dir: [0.3, 1.0, 0.5],
            light_color: hex(0xffe0c0),
            light_intensity: Param::new(1.3),
            ambient: Param::new(0.7),
            light_style: crate::LightStylePreset::Flicker.style(secs),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sky",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::LayeredSky,
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Floor",
                LayerKind::Mesh(MeshLayer {
                    subdivide: 2,
                    ..mesh(Primitive::Plane, stone(0x9a8a78, "metal_plate", 6.0))
                }),
            )
            .scaled(24.0),
            Layer::new(
                "Walls",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 10,
                        radius: 9.0,
                    },
                    ..mesh(Primitive::Cube, stone(0xb09880, "brick", 2.0))
                }),
            )
            .at([0.0, 1.6, 0.0])
            .stretched([5.6, 3.2, 0.6])
            .scaled(1.0),
            Layer::new(
                "Pillars",
                LayerKind::Mesh(MeshLayer {
                    instancer: Instancer::Radial {
                        count: 4,
                        radius: 3.6,
                    },
                    ..mesh(Primitive::Cube, stone(0xd8c8b0, "metal_plate", 1.0))
                }),
            )
            .at([0.0, 1.5, 0.0])
            .stretched([0.6, 3.0, 0.6])
            .scaled(1.0),
            Layer::new(
                "Lava pool",
                LayerKind::Mesh(mesh(
                    Primitive::Plane,
                    Material {
                        base_color: hex(0xffffff),
                        texture: Some("lava".into()),
                        texture_scale: Param::new(1.5),
                        emissive_color: hex(0xff6010),
                        emissive: Param::new(1.2),
                        emissive_mode: EmissiveMode::Texture,
                        turbulence: crate::Turbulence {
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
            .scaled(3.4),
            Layer::new(
                "Torches",
                LayerKind::Sprite(SpriteLayer {
                    image: Some("sheet_flame".into()),
                    columns: 4,
                    rows: 4,
                    cycles: 8,
                    random_start: true,
                    facing: SpriteFacing::Upright,
                    blend: SpriteBlend::Additive,
                    size: Param::new(0.9),
                    glow: Param::new(1.3),
                    glow_style: crate::LightStylePreset::Torch.style(secs),
                    instancer: Instancer::Radial {
                        count: 4,
                        radius: 3.6,
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, 3.4, 0.0]),
            Layer::new(
                "Embers",
                LayerKind::Particles(ParticleLayer {
                    emitter: Emitter::Fountain,
                    count: 400,
                    lifetimes: 4,
                    size: Param::new(0.06),
                    speed: Param::new(0.6),
                    radius: Param::new(2.0),
                    color_a: hex(0xffc040),
                    color_b: hex(0xc03000),
                    intensity: Param::new(1.5),
                    sprite: Sprite::SolidSquare,
                    seed: 5,
                    ..Default::default()
                }),
            ),
        ],
        ..Default::default()
    };
    p.retro.apply_style(crate::RetroStyle::Quake);
    p
}

/// Slime falls: a canyon of toxic slime and a slime waterfall, both
/// wobbling with Quake's turbulent warp, under a scrolling two-layer sky;
/// the light strobes like a broken fluorescent tube.
pub fn slime_falls() -> Project {
    let timing = crate::Timing {
        bpm: 100.0,
        loop_beats: 16,
    };
    let secs = timing.loop_seconds();
    let mut p = Project {
        name: "Slime Falls".into(),
        timing,
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(14.0),
            target: [0.0, 2.0, -6.0],
            distance: Param::new(12.0),
            height: Param::new(3.0),
            fov: Param::new(62.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x101810),
            fog_density: Param::new(0.02),
            sky_color: hex(0x708870),
            ground_color: hex(0x302820),
            light_dir: [0.2, 0.8, 0.6],
            light_color: hex(0xe0ffe0),
            light_intensity: Param::new(1.4),
            ambient: Param::new(0.6),
            light_style: crate::LightStylePreset::Fluorescent.style(secs),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sky",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::LayeredSky,
                    sky: LayeredSky {
                        far_scroll: [1, 0],
                        near_scroll: [3, 1],
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Canyon",
                LayerKind::Terrain(Terrain {
                    size: 70.0,
                    cells: 64,
                    height: Param::new(7.0),
                    hills: 3,
                    roughness: Param::new(0.4),
                    scroll: 1,
                    valley: Param::new(0.3),
                    style: TerrainStyle::Solid,
                    fill_color: hex(0x806a58),
                    texture: Some("brick".into()),
                    tiles: 20,
                    pixelated: true,
                    seed: 12,
                    shape: TerrainShape::Canyons,
                    liquid: Liquid {
                        kind: LiquidKind::Toxic,
                        level: Param::new(0.06),
                        color: hex(0x40ff30),
                        glow: Param::new(1.1),
                        turbulence: Param::new(0.25),
                        turb_cycles: 2,
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, -1.0, -25.0]),
            Layer::new(
                "Slime fall",
                LayerKind::Falls(Falls {
                    kind: FallKind::Toxic,
                    color: FallKind::Toxic.default_color(),
                    width: 3.0,
                    height: 7.0,
                    turbulence: Param::new(0.25),
                    ..Default::default()
                }),
            )
            .at([0.0, 6.0, -12.0]),
        ],
        ..Default::default()
    };
    p.retro.apply_style(crate::RetroStyle::Quake);
    p
}

/// A SNES-style Mode 7 race: an endless track floor turning and
/// scrolling under a low camera, a hard horizon, coins spinning on steps,
/// at 256 × 224 with a sharp lap counter.
pub fn mode7_circuit() -> Project {
    let mut floor = Layer::new(
        "Track",
        LayerKind::Mode7(Mode7Floor {
            texture: Some("track".into()),
            tile_size: 24.0,
            turns: 1,
            scroll: [0, 6],
            ..Default::default()
        }),
    );
    floor.transform.position = [0.0, 0.0, -30.0];
    let mut coins = Layer::new(
        "Coins",
        LayerKind::Mesh(MeshLayer {
            instancer: Instancer::Grid {
                counts: [3, 1, 1],
                spacing: [2.4, 1.0, 1.0],
            },
            ..mesh(
                Primitive::Ring {
                    arc: 360.0,
                    width: 0.35,
                    height: 0.3,
                    segments: 16,
                },
                Material {
                    base_color: hex(0xffd030),
                    metallic: Param::new(0.9),
                    roughness: Param::new(0.3),
                    emissive_color: hex(0xffa000),
                    emissive: Param::new(0.6),
                    flat_shading: true,
                    ..Default::default()
                },
            )
        }),
    )
    .at([0.0, 1.2, -5.0])
    .scaled(0.6)
    .spin([0, 8, 0]);
    // Coins spin on twos, like sprites in a 16-bit game.
    coins.step_fps = 12.0;
    let mut p = Project {
        name: "Mode 7 Circuit".into(),
        timing: crate::Timing {
            bpm: 140.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Static,
            target: [0.0, 0.6, -10.0],
            distance: Param::new(10.0),
            height: Param::new(2.2),
            fov: Param::new(62.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x6090e0),
            fog_density: Param::new(0.0),
            sky_color: hex(0xa0c0ff),
            ground_color: hex(0x305020),
            light_dir: [0.3, 1.0, 0.6],
            light_intensity: Param::new(1.4),
            ambient: Param::new(0.7),
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sky",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Gradient,
                    color_a: hex(0x2050d0),
                    color_b: hex(0xa0d0ff),
                    color_c: hex(0xffffff),
                    ..Default::default()
                }),
            ),
            floor,
            coins,
            Layer::new(
                "Lap",
                LayerKind::Text(TextLayer {
                    text: "LAP 1".into(),
                    size: 0.7,
                    color_top: hex(0xffffff),
                    color_bottom: hex(0xffe040),
                    outline: 0.6,
                    outline_color: hex(0x000040),
                    face_camera: true,
                    ..Default::default()
                }),
            )
            .at([0.0, 3.2, -6.0]),
        ],
        ..Default::default()
    };
    p.retro = crate::Retro3d {
        enabled: true,
        resolution: crate::RetroRes::R256x224,
        ..Default::default()
    };
    p
}

/// Stop-motion toys on a shelf: each toy animates on its own steps (12,
/// 8 and 6 frames a second) while the camera glides smoothly round.
pub fn stop_motion_shelf() -> Project {
    let toy = |name: &str, prim: Primitive, color: u32, x: f32, fps: f32, spin: [i32; 3]| {
        let mut l = Layer::new(
            name,
            LayerKind::Mesh(mesh(
                prim,
                Material {
                    base_color: hex(color),
                    metallic: Param::new(0.0),
                    roughness: Param::new(0.75),
                    rim: Param::new(0.2),
                    ..Default::default()
                },
            )),
        )
        .at([x, 0.9, 0.0])
        .scaled(0.8)
        .spin(spin);
        l.transform.bob = Param::new(0.0).osc(Wave::Sine, 0.35, 4);
        l.step_fps = fps;
        l
    };
    Project {
        name: "Stop-Motion Shelf".into(),
        timing: crate::Timing {
            bpm: 96.0,
            loop_beats: 16,
        },
        camera: Camera {
            target: [0.0, 0.9, 0.0],
            distance: Param::new(6.0),
            height: Param::new(1.2),
            swing: Param::new(35.0),
            mode: CameraMode::Pendulum,
            fov: Param::new(45.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x2a2018),
            fog_density: Param::new(0.01),
            sky_color: hex(0xf0d8b8),
            ground_color: hex(0x604830),
            light_dir: [0.4, 0.9, 0.5],
            light_color: hex(0xfff0d8),
            light_intensity: Param::new(1.6),
            ambient: Param::new(0.5),
            shadows: Shadows {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Wall",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Gradient,
                    color_a: hex(0x3a2c20),
                    color_b: hex(0x1a120c),
                    color_c: hex(0x604830),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Shelf",
                LayerKind::Mesh(mesh(
                    Primitive::Cube,
                    Material {
                        base_color: hex(0xc09060),
                        texture: Some("wood".into()),
                        roughness: Param::new(0.8),
                        metallic: Param::new(0.0),
                        rim: Param::new(0.0),
                        ..Default::default()
                    },
                )),
            )
            .stretched([7.0, 0.2, 2.0])
            .scaled(1.0),
            toy(
                "Clay star",
                Primitive::Star {
                    points: 5,
                    inner: 0.5,
                    depth: 0.4,
                },
                0xe04030,
                -2.2,
                12.0,
                [0, 1, 0],
            ),
            toy(
                "Clay ball",
                Primitive::Sphere { detail: 2 },
                0x3080e0,
                0.0,
                8.0,
                [1, 1, 0],
            ),
            toy(
                "Clay gear",
                Primitive::Gear {
                    teeth: 8,
                    depth: 0.4,
                },
                0x40b050,
                2.2,
                6.0,
                [0, 0, 2],
            ),
        ],
        ..Default::default()
    }
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

/// A 16-bit RPG battle: a two-layer battle background (rings cycling
/// through the colours, interlaced diamonds over them) behind a crystal
/// foe, on a CRT.
pub fn battle_screen() -> Project {
    let mut battle = Battle::default();
    battle.back.amount = Param::new(0.05).osc(Wave::Sine, 0.02, 1);
    battle.front.enabled = true;
    battle.front.tiles = Param::new(2.0);
    battle.front.amount = Param::new(0.015);
    battle.front.opacity = Param::new(0.3);
    battle.blend = BattleBlend::Add;
    Project {
        name: "Battle Screen".into(),
        timing: crate::Timing {
            bpm: 120.0,
            loop_beats: 16,
        },
        camera: Camera {
            distance: Param::new(6.0),
            height: Param::new(0.4),
            target: [0.0, 0.3, 0.0],
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Battle background",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Battle,
                    color_a: hex(0x0a0418),
                    color_b: hex(0xc01860),
                    color_c: hex(0x18b098),
                    battle,
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Foe",
                LayerKind::Mesh(mesh(
                    Primitive::Icosahedron,
                    Material {
                        base_color: hex(0x9070ff),
                        metallic: Param::new(0.6),
                        roughness: Param::new(0.25),
                        emissive_color: hex(0xff60c0),
                        emissive: Param::new(0.2).osc(Wave::Pulse, 1.5, 4),
                        rim: Param::new(0.8),
                        flat_shading: true,
                        ..Default::default()
                    },
                )),
            )
            .scaled(1.3)
            .at([0.0, 0.3, 0.0])
            .spin([1, 2, 0])
            .bobbing(Param::new(0.25)),
        ],
        post: PostStack {
            crt: Crt {
                enabled: true,
                scanlines: Param::new(0.2),
                curvature: Param::new(0.1),
                noise: Param::new(0.0),
            },
            ..Default::default()
        },
        ..Default::default()
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
                    battle: Battle::default(),
                    kind: BackdropKind::Nebula,
                    color_a: hex(0x020001),
                    color_b: hex(0x4a0404),
                    color_c: hex(0xe02010),
                    speed: 1,
                    intensity: Param::new(0.75),
                    detail: Param::new(1.2),
                    texture: None,
                    ray: Default::default(),
                    ..Default::default()
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
                    ..Default::default()
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
                    ..Default::default()
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
                    ..Default::default()
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
                    battle: Battle::default(),
                    kind: BackdropKind::Starfield,
                    color_a: hex(0x000003),
                    color_b: hex(0x061530),
                    color_c: hex(0x80b0ff),
                    speed: 1,
                    intensity: Param::new(0.6),
                    detail: Param::new(1.0),
                    texture: None,
                    ray: Default::default(),
                    ..Default::default()
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
                    ..Default::default()
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
                    battle: Battle::default(),
                    kind: BackdropKind::Tunnel,
                    color_a: hex(0x000000),
                    color_b: hex(0x2040ff),
                    color_c: hex(0xff40c0),
                    speed: 4,
                    intensity: Param::new(1.2),
                    detail: Param::new(1.0),
                    texture: Some("xor".into()),
                    ray: Default::default(),
                    ..Default::default()
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
                    ..Default::default()
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
                    battle: Battle::default(),
                    kind: BackdropKind::Plasma,
                    color_a: hex(0x10003a),
                    color_b: hex(0xff2090),
                    color_c: hex(0x20e0ff),
                    speed: 2,
                    intensity: Param::new(0.9),
                    detail: Param::new(1.0),
                    texture: None,
                    ray: Default::default(),
                    ..Default::default()
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
                    battle: Battle::default(),
                    kind: BackdropKind::SynthGrid,
                    color_a: hex(0x0a0020),
                    color_b: hex(0xff3080),
                    color_c: hex(0xffd030),
                    speed: 1,
                    intensity: Param::new(1.0),
                    detail: Param::new(1.0),
                    texture: None,
                    ray: Default::default(),
                    ..Default::default()
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
                    battle: Battle::default(),
                    kind: BackdropKind::Starfield,
                    color_a: hex(0x02000a),
                    color_b: hex(0x301060),
                    color_c: hex(0xa0c0ff),
                    speed: 1,
                    intensity: Param::new(1.0),
                    detail: Param::new(1.0),
                    texture: None,
                    ray: Default::default(),
                    ..Default::default()
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
                    battle: Battle::default(),
                    kind: BackdropKind::Nebula,
                    color_a: hex(0x020008),
                    color_b: hex(0x3010a0),
                    color_c: hex(0x00c0ff),
                    speed: 1,
                    intensity: Param::new(0.7),
                    detail: Param::new(1.0),
                    texture: None,
                    ray: Default::default(),
                    ..Default::default()
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
                    battle: Battle::default(),
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
                    ..Default::default()
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
            battle: Battle::default(),
            ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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

/// A murmuration at dusk: a simulated flock of dark birds following a
/// looping curve in front of a sunset, scattering on every bar.
pub fn starling_dusk() -> Project {
    use crate::sim::{Flock, FlockPath};
    let flock = Flock {
        count: 600,
        seed: 7,
        speed: Param::new(4.0),
        spacing: 0.5,
        sight: 1.4,
        radius: 6.0,
        path: Some(FlockPath {
            curve: RibbonCurve::Lissajous,
            freq: [1, 2, 1],
            size: 6.0,
            laps: 1,
        }),
        // A burst outward on every bar (4 per loop).
        scatter: Param::new(0.0).osc(Wave::Pulse, 0.7, 4),
        ..Default::default()
    };
    let mut birds = Layer::new(
        "Starlings",
        LayerKind::Mesh(MeshLayer {
            instancer: Instancer::Flock {
                flock: Box::new(flock),
                placed: None,
            },
            variation: Variation {
                scale: 0.3,
                ..Default::default()
            },
            ..mesh(
                Primitive::Pyramid,
                Material {
                    base_color: hex(0x050406),
                    metallic: Param::new(0.0),
                    roughness: Param::new(1.0),
                    rim: Param::new(0.0),
                    ..Default::default()
                },
            )
        }),
    )
    .scaled(0.17)
    .at([0.0, 6.0, 0.0]);
    // Flat and wide: wings, pointing where they fly.
    birds.transform.stretch = [2.2, 0.25, 1.0];
    Project {
        name: "Starling Dusk".into(),
        timing: crate::Timing {
            bpm: 100.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(12.0),
            target: [0.0, 6.0, 0.0],
            distance: Param::new(20.0),
            height: Param::new(1.0),
            fov: Param::new(55.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0xc07a6a),
            fog_density: Param::new(0.004),
            sky_color: hex(0xe09070),
            ground_color: hex(0x2a1830),
            light_dir: [-0.4, 0.12, -1.0],
            light_color: hex(0xffb070),
            light_intensity: Param::new(1.2),
            ambient: Param::new(0.15),
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Clouds,
                0x40305a,
                0xf0a070,
                0x906080,
                RaySettings {
                    variant: 0,
                    size: Param::new(1.2),
                    warp: Param::new(1.0),
                    bend: Param::new(1.0),
                    glow: Param::new(1.6),
                    fog: Param::new(1.0),
                    ..Default::default()
                },
            ),
            birds,
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
                length: Param::new(0.6),
                threshold: Param::new(0.8),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Shiny things lit by an environment map (image-based lighting): the
/// sunset panorama turns once per loop, its sun lights and shadows.
pub fn chrome_studio() -> Project {
    let thing = |name: &str, prim: Primitive, base: u32, metallic: f32, rough: f32, x: f32| {
        Layer::new(
            name,
            LayerKind::Mesh(mesh(
                prim,
                Material {
                    base_color: hex(base),
                    metallic: Param::new(metallic),
                    roughness: Param::new(rough),
                    rim: Param::new(0.0),
                    ..Default::default()
                },
            )),
        )
        .at([x, 1.1, 0.0])
        .spin([0, 1, 0])
    };
    Project {
        name: "Chrome Studio".into(),
        timing: crate::Timing {
            bpm: 100.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(25.0),
            target: [0.0, 1.0, 0.0],
            distance: Param::new(7.5),
            height: Param::new(1.5),
            fov: Param::new(45.0),
            ..Default::default()
        },
        environment: Environment {
            fog_density: Param::new(0.0),
            shadows: Shadows {
                enabled: true,
                distance: 20.0,
                ..Default::default()
            },
            reflections: Reflections {
                enabled: true,
                ..Default::default()
            },
            env_light: EnvLight {
                source: EnvSource::Studio(Studio::Sunset),
                // One whole turn per loop.
                rotation: Param::new(0.0).osc(Wave::Saw, 180.0, 1),
                intensity: Param::new(1.0),
                sun_from_map: true,
                sky_static: false,
            },
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sky",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Environment,
                    detail: Param::new(0.85),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Floor",
                LayerKind::Mirror(MirrorFloor {
                    base_color: hex(0x2a2826),
                    reflectivity: Param::new(0.15),
                    blur: Param::new(0.6),
                    ..Default::default()
                }),
            ),
            thing(
                "Chrome knot",
                Primitive::TorusKnot {
                    p: 2,
                    q: 3,
                    thickness: 0.12,
                },
                0xf0f0f4,
                1.0,
                0.04,
                -2.3,
            ),
            thing(
                "Gold ball",
                Primitive::Sphere { detail: 4 },
                0xffc860,
                1.0,
                0.3,
                0.0,
            ),
            thing("Plastic block", Primitive::Cube, 0xe8e8e0, 0.0, 0.7, 2.3),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.4),
                threshold: Param::new(1.5),
                radius: Param::new(0.6),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

/// A bowl of molten gold rocking back and forth: a simulated liquid in a
/// bowl shape, both tilted by the same swing.
pub fn liquid_gold() -> Project {
    // Both layers rock together: the liquid sloshes in the bowl's frame.
    let rock = Param::new(0.0).osc(Wave::Sine, 22.0, 2);
    let size = 1.5;
    let mut gold = Material::default();
    MaterialPreset::Gold.apply(&mut gold);
    gold.roughness = Param::new(0.12);
    let mut bowl_mat = Material::default();
    MaterialPreset::Ceramic.apply(&mut bowl_mat);
    bowl_mat.base_color = hex(0x1c1c22);
    let at = [0.0, 1.6, 0.0];
    let mut bowl = Layer::new(
        "Bowl",
        LayerKind::Mesh(mesh(Primitive::Bowl { thickness: 0.06 }, bowl_mat)),
    )
    .at(at)
    .scaled(size / 0.94);
    bowl.transform.tilt = rock;
    let mut liquid = Layer::new(
        "Molten gold",
        LayerKind::Mesh(MeshLayer {
            source: MeshSource::Primitive(Primitive::Sphere { detail: 1 }),
            material: gold,
            instancer: Instancer::Fluid {
                fluid: Box::new(crate::sim::Fluid {
                    count: 2500,
                    size,
                    spacing: 0.1,
                    viscosity: 0.15,
                    surface: true,
                    ..Default::default()
                }),
                placed: None,
            },
            ..Default::default()
        }),
    )
    .at(at)
    .scaled(0.075);
    liquid.transform.tilt = rock;
    Project {
        name: "Liquid Gold".into(),
        timing: crate::Timing {
            bpm: 90.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(20.0),
            target: [0.0, 1.2, 0.0],
            distance: Param::new(6.5),
            height: Param::new(2.6),
            fov: Param::new(45.0),
            ..Default::default()
        },
        environment: Environment {
            fog_density: Param::new(0.0),
            light_intensity: Param::new(0.8),
            light_dir: [0.4, 1.0, 0.3],
            shadows: Shadows {
                enabled: true,
                distance: 12.0,
                ..Default::default()
            },
            env_light: EnvLight {
                source: EnvSource::Studio(Studio::Softbox),
                intensity: Param::new(1.1),
                ..Default::default()
            },
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Studio",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Environment,
                    detail: Param::new(0.3),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Floor",
                LayerKind::Mirror(MirrorFloor {
                    base_color: hex(0x202024),
                    reflectivity: Param::new(0.3),
                    blur: Param::new(0.35),
                    ..Default::default()
                }),
            ),
            bowl,
            liquid,
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.35),
                threshold: Param::new(1.5),
                radius: Param::new(0.6),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Shafts of low sunlight through a colonnade: fog lit where the sun
/// reaches it, cut by the columns' shadows, over a marble mirror floor.
pub fn cathedral_light() -> Project {
    let mut stone = Material {
        base_color: hex(0xd8cfc0),
        ..Default::default()
    };
    MaterialPreset::Ceramic.apply(&mut stone);
    stone.base_color = hex(0xd8cfc0);
    stone.roughness = Param::new(0.55);
    stone.pbr.clearcoat = Param::new(0.0);
    let mut gold = Material::default();
    MaterialPreset::Gold.apply(&mut gold);
    Project {
        name: "Cathedral Light".into(),
        timing: crate::Timing {
            bpm: 80.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(8.0),
            target: [0.0, 2.2, -4.0],
            distance: Param::new(13.0),
            height: Param::new(0.6),
            fov: Param::new(50.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x3a3028),
            fog_density: Param::new(0.04),
            light_dir: [0.8, 0.3, -1.0],
            light_color: hex(0xffd9a0),
            light_intensity: Param::new(2.2),
            shadows: Shadows {
                enabled: true,
                distance: 28.0,
                softness: 1.0,
                ..Default::default()
            },
            env_light: EnvLight {
                source: EnvSource::Studio(Studio::Sunset),
                intensity: Param::new(0.7),
                ..Default::default()
            },
            shafts: LightShafts {
                enabled: true,
                strength: Param::new(1.1),
                scattering: 0.6,
                ..Default::default()
            },
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sky",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Environment,
                    detail: Param::new(0.6),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Marble floor",
                LayerKind::Mirror(MirrorFloor {
                    base_color: hex(0xcfc6b8),
                    texture: Some("marble".into()),
                    texture_scale: 6.0,
                    reflectivity: Param::new(0.35),
                    blur: Param::new(0.25),
                    ..Default::default()
                }),
            ),
            // Two rows of columns: the layer is stretched tall and thin,
            // so the spacing is divided by the same amounts.
            Layer::new(
                "Columns",
                LayerKind::Mesh(MeshLayer {
                    source: MeshSource::Primitive(Primitive::Cylinder { segments: 24 }),
                    material: stone,
                    instancer: Instancer::Grid {
                        counts: [2, 1, 7],
                        spacing: [8.0 / 0.7, 0.0, 3.6 / 0.7],
                    },
                    ..Default::default()
                }),
            )
            .at([0.0, 3.5, -6.0])
            .stretched([0.7, 7.0, 0.7]),
            Layer::new(
                "Orb",
                LayerKind::Mesh(mesh(Primitive::Sphere { detail: 4 }, gold)),
            )
            .at([0.0, 1.6, -3.0])
            .scaled(0.9)
            .spin([0, 1, 0]),
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.35),
                threshold: Param::new(1.4),
                radius: Param::new(0.6),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Every material preset (physical shading) on its own shape, lit by the
/// softbox studio, which turns once per loop.
pub fn material_gallery() -> Project {
    let shapes = [
        Primitive::Sphere { detail: 4 },
        Primitive::TorusKnot {
            p: 2,
            q: 3,
            thickness: 0.12,
        },
        Primitive::Sphere { detail: 4 },
    ];
    let mut layers = vec![
        Layer::new(
            "Studio",
            LayerKind::Backdrop(Backdrop {
                kind: BackdropKind::Environment,
                detail: Param::new(0.35),
                ..Default::default()
            }),
        ),
        Layer::new(
            "Floor",
            LayerKind::Mirror(MirrorFloor {
                base_color: hex(0x303236),
                reflectivity: Param::new(0.25),
                blur: Param::new(0.4),
                ..Default::default()
            }),
        ),
    ];
    for (i, preset) in MaterialPreset::ALL.iter().enumerate() {
        let (col, row) = ((i % 3) as f32, (i / 3) as f32);
        let mut material = Material::default();
        preset.apply(&mut material);
        layers.push(
            Layer::new(
                preset.label(),
                LayerKind::Mesh(mesh(shapes[(i + i / 3) % 3].clone(), material)),
            )
            .at([(col - 1.0) * 2.6, 0.7, (row - 1.0) * 2.6])
            .scaled(0.75)
            .spin([0, 1, 0]),
        );
    }
    Project {
        name: "Material Gallery".into(),
        timing: crate::Timing {
            bpm: 96.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Orbit,
            target: [0.0, 0.6, 0.0],
            distance: Param::new(9.0),
            height: Param::new(4.0),
            fov: Param::new(40.0),
            ..Default::default()
        },
        environment: Environment {
            fog_density: Param::new(0.0),
            light_intensity: Param::new(0.0),
            reflections: Reflections {
                enabled: true,
                ..Default::default()
            },
            env_light: EnvLight {
                source: EnvSource::Studio(Studio::Softbox),
                rotation: Param::new(0.0).osc(Wave::Saw, 180.0, 1),
                intensity: Param::new(1.2),
                sun_from_map: false,
                sky_static: false,
            },
            ..Default::default()
        },
        layers,
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.3),
                threshold: Param::new(1.6),
                radius: Param::new(0.5),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

/// A row of flags in a gusting wind that swings round: simulated cloth.
pub fn banners() -> Project {
    use crate::sim::{Cloth, ClothKind};
    let row = Instancer::Grid {
        counts: [5, 1, 1],
        spacing: [4.5, 1.0, 1.0],
    };
    let cloth = Cloth {
        kind: ClothKind::Flag,
        size: [3.0, 2.0],
        detail: 24,
        stiffness: 0.25,
        // A gust on every bar, the wind swinging ±35° over the loop.
        wind: Param::new(9.0).osc(Wave::Pulse, 5.0, 4),
        wind_direction: Param::new(-20.0).osc(Wave::Sine, 35.0, 1),
        gusts: 0.5,
        ..Default::default()
    };
    let mut flags = Layer::new(
        "Flags",
        LayerKind::Mesh(MeshLayer {
            source: MeshSource::Cloth {
                cloth: Box::new(cloth),
                mesh: None,
            },
            instancer: row.clone(),
            variation: Variation {
                hue: 0.35,
                ..Default::default()
            },
            material: Material {
                base_color: hex(0xe8e0d0),
                roughness: Param::new(0.85),
                texture: Some("rings".into()),
                ..Default::default()
            },
            ..Default::default()
        }),
    )
    .at([0.0, 4.2, 0.0]);
    flags.transform.position[0] = -1.5;
    let mut poles = Layer::new(
        "Poles",
        LayerKind::Mesh(MeshLayer {
            instancer: row,
            ..mesh(
                Primitive::Cylinder { segments: 12 },
                Material {
                    base_color: hex(0xb8bcc4),
                    metallic: Param::new(0.9),
                    roughness: Param::new(0.25),
                    ..Default::default()
                },
            )
        }),
    )
    .at([-1.5, 2.6, 0.0]);
    poles.transform.stretch = [0.06, 2.6, 0.06];
    Project {
        name: "Banners".into(),
        timing: crate::Timing {
            bpm: 110.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(25.0),
            target: [0.0, 3.2, 0.0],
            distance: Param::new(17.0),
            height: Param::new(2.5),
            fov: Param::new(55.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0xa8bcd4),
            fog_density: Param::new(0.01),
            sky_color: hex(0x88aee0),
            ground_color: hex(0x40464c),
            light_dir: [0.5, 0.8, 0.6],
            light_color: hex(0xfff4e0),
            light_intensity: Param::new(1.5),
            ambient: Param::new(0.5),
            shadows: Shadows {
                enabled: true,
                distance: 40.0,
                ..Default::default()
            },
            ..Default::default()
        },
        layers: vec![
            sky(
                BackdropKind::Clouds,
                0x3a70c0,
                0xc8d8ec,
                0x8898b0,
                RaySettings {
                    variant: 0,
                    size: Param::new(1.0),
                    warp: Param::new(1.0),
                    bend: Param::new(1.0),
                    glow: Param::new(1.0),
                    fog: Param::new(1.0),
                    ..Default::default()
                },
            ),
            Layer::new(
                "Ground",
                LayerKind::Mirror(MirrorFloor {
                    base_color: hex(0x3a4048),
                    reflectivity: Param::new(0.25),
                    blur: Param::new(0.5),
                    ..Default::default()
                }),
            ),
            poles,
            flags,
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.3),
                threshold: Param::new(1.4),
                radius: Param::new(0.6),
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

/// A wall of blocks knocked down by a blast and rebuilt (ping-pong), with
/// a rain of pearls behind it: simulated rigid bodies.
pub fn beat_demolition() -> Project {
    use crate::sim::{Collider, Physics};
    let wall = Layer::new(
        "Wall",
        LayerKind::Mesh(MeshLayer {
            instancer: Instancer::Physics {
                physics: Box::new(Physics {
                    counts: [8, 6, 1],
                    gap: 0.02,
                    blast_beat: 2.0,
                    blast: 12.0,
                    blast_at: [0.5, 1.2, 1.2],
                    ..Physics::stack()
                }),
                placed: None,
            },
            ramp: ColorRamp {
                enabled: true,
                colors: vec![hex(0xff3060), hex(0xffa020), hex(0x30c0ff)],
                cycles: 1,
                ..Default::default()
            },
            ..mesh(
                Primitive::Cube,
                Material {
                    base_color: hex(0x202028),
                    metallic: Param::new(0.3),
                    roughness: Param::new(0.35),
                    emissive: Param::new(0.9),
                    emissive_mode: EmissiveMode::Edges,
                    ..Default::default()
                },
            )
        }),
    );
    let rain = Layer::new(
        "Rain",
        LayerKind::Mesh(MeshLayer {
            instancer: Instancer::Physics {
                physics: Box::new(Physics {
                    collider: Collider::Ball,
                    extent: 1.0,
                    count: 40,
                    area: 3.0,
                    height: 5.0,
                    ..Physics::default()
                }),
                placed: None,
            },
            ..mesh(
                Primitive::Sphere { detail: 3 },
                Material {
                    base_color: hex(0xe8ecf4),
                    metallic: Param::new(0.1),
                    roughness: Param::new(0.25),
                    emissive: Param::new(0.35),
                    emissive_color: hex(0xc0d8ff),
                    ..Default::default()
                },
            )
        }),
    )
    .scaled(0.35)
    .at([0.0, 0.0, -6.0]);
    Project {
        name: "Beat Demolition".into(),
        timing: crate::Timing {
            bpm: 120.0,
            loop_beats: 16,
        },
        camera: Camera {
            mode: CameraMode::Pendulum,
            swing: Param::new(20.0),
            target: [0.0, 2.0, -1.0],
            distance: Param::new(15.0),
            height: Param::new(4.0),
            fov: Param::new(55.0),
            ..Default::default()
        },
        environment: Environment {
            fog_color: hex(0x080610),
            fog_density: Param::new(0.02),
            sky_color: hex(0x303050),
            ground_color: hex(0x101018),
            light_dir: [0.4, 0.9, 0.5],
            light_color: hex(0xfff0e0),
            light_intensity: Param::new(1.3),
            ambient: Param::new(0.35),
            shadows: Shadows {
                enabled: true,
                distance: 30.0,
                ..Default::default()
            },
            ..Default::default()
        },
        layers: vec![
            Layer::new(
                "Sky",
                LayerKind::Backdrop(Backdrop {
                    kind: BackdropKind::Nebula,
                    color_a: hex(0x401030),
                    color_b: hex(0x05040c),
                    ..Default::default()
                }),
            ),
            Layer::new(
                "Floor",
                LayerKind::Mirror(MirrorFloor {
                    base_color: hex(0x0c0c14),
                    reflectivity: Param::new(0.5),
                    blur: Param::new(0.3),
                    ..Default::default()
                }),
            ),
            wall,
            rain,
        ],
        post: PostStack {
            bloom: Bloom {
                enabled: true,
                intensity: Param::new(0.7),
                threshold: Param::new(1.0),
                radius: Param::new(0.7),
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
                        ..Default::default()
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
                        ..Default::default()
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
