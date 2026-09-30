//! Built-in presets. Each one is a project file in `assets/presets/`,
//! bundled into the program. To change a preset, open its file in the
//! editor, edit it and save it back over the file.

use crate::scene::Project;

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

/// A bundled preset, not parsed yet.
pub struct PresetInfo {
    pub name: &'static str,
    /// Its group in the gallery (one of [`CATEGORIES`]).
    pub category: &'static str,
    pub description: &'static str,
    /// The file name in `assets/presets/`, without the extension.
    pub file: &'static str,
    json: &'static str,
}

impl PresetInfo {
    pub fn project(&self) -> Project {
        let mut p = Project::from_json(self.json)
            .unwrap_or_else(|e| panic!("bundled preset {} is broken: {e}", self.file));
        p.migrate();
        p
    }

    pub fn load(&self) -> Preset {
        Preset {
            name: self.name,
            category: self.category,
            description: self.description,
            project: self.project(),
        }
    }
}

macro_rules! preset {
    ($name:literal, $category:expr, $file:literal, $description:literal) => {
        PresetInfo {
            name: $name,
            category: $category,
            description: $description,
            file: $file,
            json: include_str!(concat!("../../../assets/presets/", $file, ".ez2.json")),
        }
    };
}

/// The gallery presets, in gallery order.
pub static INDEX: &[PresetInfo] = &[
    preset!("PSX Crypt", CATEGORY_CONSOLES, "psx_crypt",
        "A PlayStation crypt: swimming brick textures, wobbling polygons and chunky 320 × 240 pixels."),
    preset!("Saturn Ghosts", CATEGORY_CONSOLES, "saturn_ghosts",
        "Sega Saturn: checkerboard see-through ghosts and wisps in a stone hall; pillars vanish as the camera brushes them."),
    preset!("Fog Island", CATEGORY_CONSOLES, "fog_island",
        "Nintendo 64: soft 3-point textures, fog rolling in close to the camera, dithered colour and the video blur."),
    preset!("Mode 7 Circuit", CATEGORY_CONSOLES, "mode_7_circuit",
        "A SNES-style Mode 7 race: an endless track turning and scrolling to a hard horizon, coins spinning on twos, 256 × 224."),
    preset!("Super FX Starship", CATEGORY_CONSOLES, "super_fx_starship",
        "SNES Super FX (Star Fox): a flat-shaded starfighter over flat ground, towers flashing past, everything animating at 15 frames a second in a small window."),
    preset!("Harrier Plains", CATEGORY_CONSOLES, "harrier_plains",
        "Space Harrier (1985 arcade): a chequered floor rushing to the horizon, columns and bushes flying past, pastel sky, 320 × 224."),
    preset!("Polygon Arcade", CATEGORY_CONSOLES, "polygon_arcade",
        "Sega Model 1 (Virtua Racing): untextured flat-shaded polygons at 496 × 384, a low-poly car racing past cone trees."),
    preset!("Battle Screen", CATEGORY_CONSOLES, "battle_screen",
        "A retro RPG battle: two cycling patterns wobbling line by line behind a spinning crystal foe, on a CRT."),
    preset!("Red Visor", CATEGORY_CONSOLES, "red_visor",
        "Virtual Boy: red wireframe crystals floating over a wire landscape, four shades of red on black, 384 × 224."),
    preset!("Dreamcast Sunset", CATEGORY_CONSOLES, "dreamcast_sunset",
        "Dreamcast (1999): clean 640 × 480, a glossy car by a sparkling sea under a low sun with lens flare."),
    preset!("Slipgate Courtyard", CATEGORY_PC, "slipgate_courtyard",
        "Quake: light stepping through a 256-colour palette, flickering torches, a wobbling lava pool, square embers and a two-layer sky."),
    preset!("Slime Falls", CATEGORY_PC, "slime_falls",
        "Quake: a slime canyon and slime waterfall wobbling with the turbulent warp, a scrolling two-layer sky and a broken-tube strobe."),
    preset!("Hangar Base", CATEGORY_PC, "hangar_base",
        "Early-90s software 3D: 320 × 200, light stepping through a palette, flickering tubes, a slime pool, a camera that never looks up or down."),
    preset!("Accelerator Arena", CATEGORY_PC, "accelerator_arena",
        "Late-90s 3D cards: 640 × 480, bilinear textures without mipmaps, dithered 16-bit colour, coloured lamps in a dark metal arena."),
    preset!("Wireframe Trader", CATEGORY_PC, "wireframe_trader",
        "BBC Micro Elite (1984): white wireframe ships and a space station on a 1-bit screen, turning at 12 frames a second."),
    preset!("Screen Saver", CATEGORY_PC, "screen_saver",
        "A 90s desktop screensaver: neon pipes winding through the dark round a morphing shape, 256 colours at 640 × 480."),
    preset!("Amiga Bounce", CATEGORY_HOME, "amiga_bounce",
        "The Amiga's bouncing ball: red and white checks spinning and bouncing before a purple grid, with its shadow, 320 × 256."),
    preset!("C64 Intro", CATEGORY_HOME, "c64_intro",
        "A C64 cracktro: a raster-barred logo, a sine scroller and sprite coins in double-wide 160 × 200 pixels, 16 colours, inside the border."),
    preset!("Spectrum Isometric", CATEGORY_HOME, "spectrum_isometric",
        "A ZX Spectrum isometric room: bright blocks, a chequered floor and a bouncing gem, 256 × 192 in a blue border."),
    preset!("Handheld Quest", CATEGORY_HOME, "handheld_quest",
        "Game Boy: a little world of hills and trees with spinning coins, four greens and LCD ghosting, 160 × 144."),
    preset!("Tape Loader", CATEGORY_HOME, "tape_loader",
        "A ZX Spectrum loading from tape: pilot stripes, then the picture arriving line by line in black and white under data stripes, then its colours."),
    preset!("Neon Arena", CATEGORY_DEMO, "neon_arena",
        "Black glossy crystal arena, red neon strips, red nebula sky."),
    preset!("Gold Kaleido Room", CATEGORY_DEMO, "gold_kaleido_room",
        "Mirrored golden hall, walls of orbs, sparkling chandelier."),
    preset!("Retro Tunnel", CATEGORY_DEMO, "retro_tunnel",
        "90s raymarched tunnel, warp stars, EGA palette and CRT."),
    preset!("Plasma Kaleidoscope", CATEGORY_DEMO, "plasma_kaleidoscope",
        "Oldschool plasma, spinning tori, full-screen kaleidoscope."),
    preset!("Synth Sunset", CATEGORY_DEMO, "synth_sunset",
        "Synthwave sun, neon grid mirror floor, floating pyramids."),
    preset!("Vector Valley", CATEGORY_DEMO, "vector_valley",
        "Wireframe landscape rushing past, laser fans and a neon ribbon."),
    preset!("Glitch Shrine", CATEGORY_DEMO, "glitch_shrine",
        "A glitching Menger sponge, flashing gems and a pulsing rose."),
    preset!("Sponge Dive", CATEGORY_DEMO, "sponge_dive",
        "Flight through a Menger sponge, a breathing displaced chrome orb."),
    preset!("Oldschool Intro", CATEGORY_DEMO, "oldschool_intro",
        "Chrome logo, sine scroller and greetings over the XOR tunnel."),
    preset!("Copper Heaven", CATEGORY_DEMO, "copper_heaven",
        "An Amiga demo: a striped twister, a copper-barred logo and a sine scroller over rolling copper stripes, 320 × 256."),
    preset!("Demo Party 93", CATEGORY_DEMO, "demo_party_93",
        "A 1993 PC demo: a tunnel, a plasma and a rotozoomer cut together on a timeline, 320 × 200 in VGA colours."),
    preset!("Rotozoomer", CATEGORY_DEMO, "rotozoomer",
        "The rotozoomer: a picture turning and zooming under the camera (a Mode 7 floor seen from above), 320 × 200."),
    preset!("Vector Balls", CATEGORY_DEMO, "vector_balls",
        "Vector balls: a cube of shiny balls and a ring of balls turning over the stars, 320 × 200 in the Atari ST's 512 colours."),
    preset!("Glenz Vectors", CATEGORY_DEMO, "glenz_vectors",
        "Glenz vectors: see-through faceted solids nested and turning against each other over a moving chequerboard."),
    preset!("Oldschool Fire", CATEGORY_DEMO, "oldschool_fire",
        "The DOS fire effect: a wall of flame licking upwards behind flickering flames and a logo, 320 × 200."),
    preset!("Material Gallery", CATEGORY_ENGINE, "material_gallery",
        "The nine physical material presets on spinning shapes in a softbox studio: metals, rubber, car paint, glass, velvet and ceramic."),
    preset!("Chrome Studio", CATEGORY_ENGINE, "chrome_studio",
        "Chrome, gold and plastic lit by a sunset panorama that turns once per loop, with the sun and its shadows taken from the map."),
    preset!("Cathedral Light", CATEGORY_ENGINE, "cathedral_light",
        "Low sunlight streaming between stone columns through hazy air onto a polished marble floor, under a sunset sky; a golden orb turns in the light."),
    preset!("Liquid Metal", CATEGORY_ENGINE, "liquid_metal",
        "Chrome metaballs melting together, orbited by gyroid lattice balls, all raymarched."),
    preset!("Liquid Gold", CATEGORY_ENGINE, "liquid_gold",
        "A rocking bowl of molten gold sloshing from side to side under a studio sky, simulated ahead of time into a loop."),
    preset!("Banners", CATEGORY_ENGINE, "banners",
        "A row of flags on poles, flapping in a turning, gusting wind: simulated cloth that loops."),
    preset!("Beat Demolition", CATEGORY_ENGINE, "beat_demolition",
        "A wall of glowing blocks blown apart on the beat and rebuilt, with pearls raining behind: rigid bodies that loop."),
    preset!("Starling Dusk", CATEGORY_ENGINE, "starling_dusk",
        "A flock of 600 starlings wheeling through a sunset, scattering on every bar: simulated, and looping."),
    preset!("Tesla Swarm", CATEGORY_ENGINE, "tesla_swarm",
        "Orbiting Solid wired up: lightning arcs jump from the core to the nearest debris and around a ring of coils."),
    preset!("Glass Galaxy", CATEGORY_ENGINE, "glass_galaxy",
        "Galaxy Swarm behind a glass logo that bends the stars, casting shadow rays through their light, with echoes as it sways."),
    preset!("Crystal Garden", CATEGORY_ENGINE, "crystal_garden",
        "Twisted crystals standing on a scrolling tundra, beads riding a halo, colours travelling along."),
    preset!("Stormy Lake", CATEGORY_ENGINE, "stormy_lake",
        "Rain, lightning and storm clouds over a mountain lake."),
    preset!("Lava World", CATEGORY_ENGINE, "lava_world",
        "Glowing lava rivers in volcanic canyons, embers and god rays."),
    preset!("Sunbeam Peaks", CATEGORY_ENGINE, "sunbeam_peaks",
        "Volumetric clouds, sunbeams and lens flare over alpine peaks."),
    preset!("Aurora Tundra", CATEGORY_ENGINE, "aurora_tundra",
        "Northern lights over a frozen lake, gently falling snow."),
    preset!("Dune Sea", CATEGORY_ENGINE, "dune_sea",
        "Sand dunes under a hazy sun, a sandstorm and a toxic oasis."),
    preset!("Club Spotlights", CATEGORY_ENGINE, "club_spotlights",
        "Sweeping spotlight cones in a hazy club, pools of light on a mirror floor."),
    preset!("Rainbow Falls", CATEGORY_ENGINE, "rainbow_falls",
        "A day passes over a waterfall valley: rainbow, valley mist, stars at night."),
    preset!("Sunken Temple", CATEGORY_ENGINE, "sunken_temple",
        "Underwater ruins with rippling caustics, sunbeams and rising bubbles."),
    preset!("Campfire Sprites", CATEGORY_ENGINE, "campfire_sprites",
        "Sprite-sheet flames round a fire, twinkling sparkles and a ring of spinning pixel coins under the aurora."),
    preset!("Music Reactor", CATEGORY_ENGINE, "music_reactor",
        "Load a song: an equalizer wall, kick flashes, melody colours and time warp."),
    preset!("Signal Flow", CATEGORY_ENGINE, "signal_flow",
        "Node graph: a sequence and a smoothed random walk drive the glow and size."),
    preset!("Scene Tour", CATEGORY_ENGINE, "scene_tour",
        "A timeline of three scenes: wipe, iris and glitch transitions, looping as one."),
    preset!("Stop-Motion Shelf", CATEGORY_ENGINE, "stop_motion_shelf",
        "Clay toys animating on 12, 8 and 6 frames a second while the camera glides smoothly."),
    preset!("Empty", CATEGORY_ENGINE, "empty",
        "A blank stage with a floor and a sky."),
];

/// Scenes that are bundled but not shown in the gallery (the tests use
/// them); [`find`] still finds them.
static EXTRA: &[PresetInfo] = &[
    preset!(
        "Orbiting Solid",
        CATEGORY_ENGINE,
        "extra/orbiting_solid",
        "A dodecahedron in deep space, orbited by debris, rings and sparks."
    ),
    preset!(
        "Twister",
        CATEGORY_ENGINE,
        "extra/twister",
        "A tornado over the plains in the rain."
    ),
];

/// Every gallery preset, parsed.
pub fn all() -> Vec<Preset> {
    INDEX.iter().map(PresetInfo::load).collect()
}

/// The bundled preset with this name (ignoring case).
pub fn find(name: &str) -> Option<Preset> {
    INDEX
        .iter()
        .chain(EXTRA)
        .find(|p| p.name.eq_ignore_ascii_case(name))
        .map(PresetInfo::load)
}

/// The project of the bundled preset with this name (ignoring case).
pub fn by_name(name: &str) -> Option<Project> {
    find(name).map(|p| p.project)
}

/// The project of a bundled preset that is known to exist.
///
/// # Panics
/// When there is no preset with this name.
pub fn named(name: &str) -> Project {
    by_name(name).unwrap_or_else(|| panic!("no bundled preset named {name}"))
}

/// A blank stage with a floor and a sky: the start of a new project.
pub fn empty() -> Project {
    named("Empty")
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

    /// The files are saved by the program, so loading and saving one gives
    /// the same text: no field is misspelt (and silently ignored).
    #[test]
    fn preset_files_are_canonical() {
        for info in INDEX.iter().chain(EXTRA) {
            assert_eq!(info.project().to_json(), info.json, "{}", info.file);
        }
    }

    #[test]
    fn every_preset_file_is_listed_once() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/presets");
        let mut files = Vec::new();
        for sub in ["", "extra"] {
            for e in std::fs::read_dir(dir.join(sub)).unwrap() {
                let name = e.unwrap().file_name().to_string_lossy().to_string();
                if let Some(stem) = name.strip_suffix(".ez2.json") {
                    files.push(if sub.is_empty() {
                        stem.to_string()
                    } else {
                        format!("{sub}/{stem}")
                    });
                }
            }
        }
        files.sort();
        let mut listed: Vec<String> = INDEX.iter().chain(EXTRA).map(|p| p.file.into()).collect();
        listed.sort();
        assert_eq!(files, listed);
        let mut names: Vec<String> = INDEX
            .iter()
            .chain(EXTRA)
            .map(|p| p.name.to_lowercase())
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), INDEX.len() + EXTRA.len(), "duplicate names");
        assert!(INDEX.iter().all(|p| CATEGORIES.contains(&p.category)));
    }
}
