# EZ2DEMOSCENE

**Make loopable, stylised 3D scenes the demoscene way, without writing code.**

EZ2DEMOSCENE is a desktop app (Windows, Linux and macOS), a web app and an
Android app for building short,
seamless 3D loops: neon arenas, mirrored golden halls, glossy solids orbited by
debris, raymarched tunnels, plasma kaleidoscopes and synthwave sunsets. You
start from a preset, tweak layers with sliders, press **Export** and get an
MP4, WebM, GIF or PNG sequence that loops with no visible seam.

![Editor](docs/editor.jpg)

📖 **[User manual (PDF)](docs/manual/EZ2DEMOSCENE_User_Manual.pdf)**: a beginner's guide to every feature, with a quick start and recipes. The source is `docs/manual/USER_MANUAL.html`.

## Features

- **Seamless loops.** A loop is a whole number of beats at a chosen BPM.
  Every animation (spins, orbits, pulses, particles, scrolling textures,
  cloud drift, grain) runs an integer number of cycles per loop, so frame
  *N* equals frame 0. The test suite checks this for every preset.
- **Presets + layers for beginners.**
  - Backgrounds: gradient sky, nebula, starfield, oldschool plasma,
    synthwave sun & grid, and four raymarched ones with their own
    settings: tunnel (5 shapes, wall patterns, twist, light rings),
    fractal (3 formulas, fold, zoom), Menger sponge flight (sponge, beam
    lattice, cube field) and a ring corridor. Plus two skies:
    **volumetric clouds** (raymarched, sun-lit, fluffy / overcast / storm)
    and an **aurora** night sky.
  - Mirror floor: real planar reflections, blur, a glowing neon grid and
    LED textures.
  - Shapes: 25 built-ins (platonic solids, crystals, shards, beveled tech
    panels, neon ring bands, tori, torus knots, springs, gears, stars,
    gems, hearts, a Möbius strip, a Menger sponge…) and your own
    **glTF/GLB/OBJ** models.
  - Copies (instancing): grid, radial, scatter, orbit swarm, curved wall,
    spiral. Each has random tilt, size and hue, plus *size waves* and
    *light chases* that travel across the copies.
  - Symmetry: mirror X/Z/XZ, radial and kaleidoscopic duplication.
  - Particles: burst, drift, ring orbit, fountain, warp stars, vortex and
    glitter, with trails. They are fully deterministic on the GPU, so
    scrubbing and exports are exact.
  - Terrain: an endless wireframe or solid (optionally textured) landscape
    that scrolls past and repeats exactly every loop. Six shapes (hills,
    ridged mountains, mesas, dunes, canyons, craters), biomes that colour
    it by height and slope (alpine, desert, volcanic, arctic, alien), and
    **water, lava, toxic goo or ice** filling the low ground, with
    ripples, sun glints, foam, a churning crust or bubbles.
  - Weather: rain with splashes, snow, rising embers, a sandstorm or
    fireflies in a box that follows the camera, and **lightning** bolts
    whose flash lights up the scene. Rain wets every surface (gloss,
    puddles with ripples); snow settles on everything facing up.
  - Waterfalls of water, lava or goo, with foam, spray or smoke.
  - Laser beams or hazy **spotlight cones** with pools of light.
  - Particles can be smoke instead of glow, and there is a tornado
    emitter.
  - **Sun shadows** (soft shadow map) and contact shadows on floors.
  - Atmosphere: **mist** pooling in valleys, underwater **caustics**, a
    **rainbow**, and a **day & night cycle** with sunsets, stars and a
    moon.
  - Laser beams: fans, rotating cones or scattered beams that sweep and
    strobe on the beat.
  - Neon ribbons: glowing tubes along Lissajous, knot, figure-eight, wave
    or rose curves, with light pulses running along them.
  - Blink / strobe on any layer: rhythmic or random blinking and beat
    flashes.
- **Materials.** Glossy/metallic surfaces with fake studio reflections,
  flat-shaded facets, rim light, and neon glow on the whole surface, along
  polygon edges (Tron look), in stripes, or from a texture. **Relief**
  adds bump maps, normal maps and real displacement (with subdivision).
  **Glitch**
  corrupts a shape's geometry (jitter, VHS slices, shatter) in loop-safe
  bursts.
- **Retro PC textures.** 33 built-in tileable textures (XOR, plasma,
  checkerboard, circuit, tech panel, LED grid, speaker grille, Win9x, copper
  bars, Sierpinski, Tron grid, Truchet, the C64 10 PRINT maze, Matrix rain,
  CRT phosphors…). Imported images can be *retro-ized*: downscaled,
  palette-reduced and dithered.
- **Post FX.** Bloom, **god rays** (light shafts from the sun or the
  picture centre) with lens flare, **heat haze**, kaleidoscope, mirror split, chromatic aberration,
  pixelation, palette reduction with Bayer dithering (EGA, CGA, C64, Game
  Boy, PICO-8, Amiga copper, ZX Spectrum, VGA cube, phosphor), CRT
  scanlines/curvature/VHS wobble, grading, vignette, grain and beat flash.
- **Animate anything.** Click `~` next to almost any value, post effects
  included, to pick a shape: LFOs (sine, triangle, saw up/down, square),
  beat fades (pulse, exponential and linear fades in/out, swell) or random
  (sample & hold, smooth random, drunken walk). The ♩ menu syncs it to
  every beat, bar or loop, a live graph previews the curve, and it can
  follow the **music**. Layers can also **shake** on the beat.
- **Music.** Drop in an MP3/WAV/OGG/FLAC. It is analysed once: loudness,
  kick, bass, mids, highs and brightness, hits (kicks, snares, hats,
  onsets, notes), a 16-band spectrum, the melody's note and the tempo.
  - Any value can **follow** a source or play a fade **on each hit** (the
    🎵 row of its `~` panel), e.g. glow on every kick, hue from the melody.
  - **Time warp** makes motion surge with the music while the loop still
    ends where it started.
  - **Equalizer** copies grow with the spectrum bands.
  - Loop a window of the song (seamless, snapped to the detected tempo) or
    run through the **whole song**; video exports mux the matching audio.
  - **MIDI files** give exact drum hits and melody; **live input**
    (microphone / line-in) drives the preview for VJ sets.
- **Randomize / Surprise me.** Seeded, harmonious mutations (one global hue
  rotation, bounded counts, loop-safe motion), and every change can be
  undone.
- **Node graph (advanced mode).** Source layers flow through modifiers
  (Symmetry, Array, Spin, Offset, Scale, Tint, Jitter, Mirror, Strobe,
  Colour/material, Merge) into Output. The
  graph compiles to the same layer list the simple mode uses, and *Bake to
  layers* brings it back into simple mode.
- **Project files.** Readable `.ez2.json` files. Static values are saved as
  plain numbers, and assets inside the project folder are stored with
  relative paths, so projects can be moved.
- **Packs.** A `.ez2pack` is one zip file holding a project plus every model,
  image and music file it uses, for sharing or archiving.
- **Autosave & crash recovery.** Unsaved work is autosaved every 30 s and
  offered back after a crash.
- **Your library.** "Save as my preset" (with a thumbnail) and "Save layer as
  template" keep your own building blocks across projects.
- **Viewport gizmos.** Click to select, then drag handles to move, rotate or
  scale (W/E/R). Hold Ctrl to snap, and press G for the ground grid.
- **Performance meter.** Shows fps, triangle and particle counts, the most
  expensive layers, and a ⚠ on heavy layers. Static copies are cached.

![Presets](docs/presets.jpg)

## Downloads

- **Web app:** <https://roganis.github.io/ez2demoscene/>. Runs in Chrome, Edge or
  Firefox on desktop and in Chrome on Android. It can be installed as a PWA and
  works offline after the first visit.
- **Android APK:** built by `.github/workflows/android.yml` on every push.
  - Newest build of `main`: the [`android-latest`](https://github.com/roganis/ez2demoscene/releases/tag/android-latest) prerelease.
  - Newest build of a work branch: the [`android-preview`](https://github.com/roganis/ez2demoscene/releases/tag/android-preview) prerelease.
  - Every run also keeps the APK as a workflow artifact, and tagged releases attach it
    (plus a signed release APK when the signing secrets are set).

Tagged releases (`v*`) are built for Windows, Linux and macOS by
`.github/workflows/release.yml`. The Windows and Linux archives include
ffmpeg. On macOS, run `brew install ffmpeg`.

## Getting started

Requirements:

- Rust **1.95+** (`rustup update stable`)
- A GPU with Vulkan, DX12 or Metal support
- On Linux, the ALSA and X11/Wayland dev packages:
  `sudo apt install libasound2-dev libxkbcommon-dev libwayland-dev`
- For video and GIF export, [ffmpeg](https://ffmpeg.org) on your `PATH`
  (or point the export dialog at it). PNG sequences need nothing extra.

```sh
cargo run --release                 # opens the editor
cargo run --release -- my.ez2.json  # opens a project
```

### Using the editor

1. **Pick a preset** in the gallery (or click *Surprise me*).
2. **Layers** (left): toggle, reorder, duplicate or add backgrounds, shapes,
   particles, a mirror floor, or a 3D model.
3. **Inspector** (right): edit the selected layer, camera, light & fog,
   post effects, timing & music, or your images.
4. **Timeline** (bottom): play/pause (Space), scrub, set BPM and loop
   length. Beat ticks show where pulses land.
5. **Viewport**: drag to turn the camera, scroll to zoom.
6. **Export** (Ctrl+E): pick a format, size, frame rate and how many
   times to repeat the loop.

Drag & drop works for projects, models (`.gltf .glb .obj`), images
(which are applied to the selected shape) and music.

Shortcuts: `Space` play/pause · `Ctrl+S` save · `Ctrl+O` open · `Ctrl+E` export ·
`Ctrl+Z` / `Ctrl+Shift+Z` undo/redo.

### Command line (no window)

```sh
ez2demoscene --list-presets
ez2demoscene --render "Neon Arena" still.png --phase 0.25 --size 1920x1080
ez2demoscene --export "Gold Kaleido Room" loop.mp4 --size 1920x1080 --fps 60 --repeats 4
ez2demoscene --export my.ez2.json frames/        # PNG sequence
ez2demoscene --write-presets assets/presets
ez2demoscene --write-textures assets/textures
```

## Project layout

| Crate | What it does |
|---|---|
| `crates/ez_core` | Scene model (serde), loop clock, animatable `Param`s, camera/instancing/symmetry math, presets, randomizer, node graph compiler. No GPU dependencies. |
| `crates/ez_render` | wgpu renderer: procedural meshes, glTF/OBJ import, texture generator, WGSL shaders (SDF backdrops, lit instanced meshes, analytic particles, mirror floor, bloom, kaleido, retro post). |
| `crates/ez_export` | Offline loop rendering to PNG / ffmpeg (MP4, WebM, GIF), plus the audio envelope analysis. |
| `crates/ez_app` | The egui editor (`ez2demoscene` binary) and the CLI. On wasm it swaps in `library_web.rs` (IndexedDB), `audio_web.rs` (HTML audio) and `export_web.rs` (WebCodecs/GIF/PNG zip). |
| `web/` | Trunk entry point for the web build: `index.html`, PWA manifest, service worker, the WebCodecs bridge (`ez2_video.js`) and vendored MIT muxers. |
| `android-app/` | Capacitor wrapper that packages the web build as an Android app. |
| `assets/presets` | Built-in presets as project files (generated). |
| `assets/textures` | The built-in retro texture pack as PNGs (generated, CC0). |

### How the loop guarantee works

- `EvalCtx.phase` goes from 0 to 1 over the loop. `Param` oscillators use
  `cycles: i32`, and spins, orbits and texture scrolls are whole turns or
  tiles per loop.
- Each particle's age is `fract(phase * lives + hash(id))`, with an integer
  number of lives. Its position is an analytic function of `(id, age)`, so
  there is no simulation state.
- Noise-based backgrounds move on closed circles through noise space.
- The day cycle, caustics, heat haze, waterfall streaks and rain ripples
  all move a whole number of times per loop.
- Weather drops fall a whole number of times per loop, and lightning
  strikes are picked from time slots that wrap with the loop. Liquid
  currents drift a whole number of terrain lengths per loop.
- Music in a loop window is sampled as a circle (curves fade into the
  window's start, hits ring on past the loop point); time warp is
  rescaled to end each loop exactly where it began.
- Volumetric clouds can drift any distance: two copies of the cloud field,
  half a loop apart, cross-fade, so each one jumps back while invisible.
- Film grain and VHS noise are hashed from a frame id that wraps with the loop.
- The exporter renders frames at `i / N` for `i` in `0..N`, so the first
  frame is never duplicated at the end.

## Web and Android builds

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked          # or download a trunk release binary
cd web && trunk serve                 # http://127.0.0.1:8080, rebuilds on change
cd web && trunk build --release       # → dist/ (what GitHub Pages serves)

# Android (needs Node 22+, JDK 21 and the Android SDK, ANDROID_HOME set)
cd android-app && npm ci && npm run build:debug
# → android-app/android/app/build/outputs/apk/debug/app-debug.apk
```

`.github/workflows/pages.yml` deploys `dist/` to GitHub Pages on every push to
`main` (one-time setup: Settings → Pages → Source: *GitHub Actions*). The web
version renders with WebGPU where available, falling back to WebGL2. Imported
files, presets and the autosave live in IndexedDB. The phone layout kicks in
below 820 logical pixels. Automated browser tests can start an export with
`?ez2test=gif|zip|mp4|webm` (optionally `&preset=<name>`).

## Rebuilding the manual

```sh
chromium --headless --no-pdf-header-footer --allow-file-access-from-files \
  --print-to-pdf=docs/manual/EZ2DEMOSCENE_User_Manual.pdf docs/manual/USER_MANUAL.html
```

## Development

```sh
cargo test --workspace   # GPU tests skip themselves if no adapter is found
cargo clippy --workspace --all-targets
cargo run -p ez_render --example contact_sheet -- sheet.png 480   # render all presets
cargo run --release -p ez_render --example bench                 # renderer timing
EZ2_BLESS=1 cargo test -p ez_render --test golden                # update golden images after an intended visual change
```

On a machine without a GPU, Mesa's `lavapipe` (package `mesa-vulkan-drivers`)
is enough to run the tests and exports.

## License

GPL-3.0-or-later. The generated texture pack in `assets/textures` is released
under CC0.
