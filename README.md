# EZ2DEMOSCENE

**Make loopable, stylised 3D scenes the demoscene way, without writing code.**

EZ2DEMOSCENE is a native desktop app (Windows and Linux) for building short,
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
  - Backgrounds: gradient sky, nebula, starfield, raymarched tunnel,
    fractal (Kaliset), oldschool plasma, synthwave sun & grid.
  - Mirror floor: real planar reflections, blur, a glowing neon grid and
    LED textures.
  - Shapes: platonic solids, crystals, shards, beveled tech panels, neon
    ring bands, tori, pyramids, and your own **glTF/GLB/OBJ** models.
  - Copies (instancing): grid, radial, scatter, orbit swarm, curved wall,
    spiral. Each has random tilt, size and hue, plus *size waves* and
    *light chases* that travel across the copies.
  - Symmetry: mirror X/Z/XZ, radial and kaleidoscopic duplication.
  - Particles: burst, drift, ring orbit, fountain, warp stars, vortex and
    glitter, with trails. They are fully deterministic on the GPU, so
    scrubbing and exports are exact.
- **Materials.** Glossy/metallic surfaces with fake studio reflections,
  flat-shaded facets, rim light, and neon glow on the whole surface, along
  polygon edges (Tron look), in stripes, or from a texture.
- **Retro PC textures.** 20 built-in tileable textures (XOR, plasma,
  checkerboard, circuit, tech panel, LED grid, speaker grille, Win9x, copper
  bars, Sierpinski…). Imported images can be *retro-ized*: downscaled,
  palette-reduced and dithered.
- **Post FX.** Bloom, kaleidoscope, mirror split, chromatic aberration,
  pixelation, palette reduction with Bayer dithering (EGA, CGA, C64, Game
  Boy, PICO-8, Amiga copper, ZX Spectrum, VGA cube, phosphor), CRT
  scanlines/curvature/VHS wobble, grading, vignette, grain and beat flash.
- **Animate anything.** Click `~` next to a value to pick a wave (sine,
  triangle, saw, square, beat pulse, random), how many times per loop it
  repeats, and how much it follows the **music**.
- **Music.** Drop in an MP3/WAV/OGG/FLAC. It plays in sync with the loop,
  and its loudness and kick envelopes can drive any value. It is also muxed
  into video exports.
- **Randomize / Surprise me.** Seeded, harmonious mutations (one global hue
  rotation, bounded counts, loop-safe motion), and every change can be
  undone.
- **Node graph (advanced mode).** Source layers flow through modifiers
  (Symmetry, Array, Spin, Offset, Scale, Tint, Merge) into Output. The
  graph compiles to the same layer list the simple mode uses, and *Bake to
  layers* brings it back into simple mode.
- **Project files.** Readable `.ez2.json` files. Static values are saved as
  plain numbers.

![Presets](docs/presets.jpg)

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
| `crates/ez_app` | The egui editor (`ez2demoscene` binary) and the CLI. |
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
- Film grain and VHS noise are hashed from a frame id that wraps with the loop.
- The exporter renders frames at `i / N` for `i` in `0..N`, so the first
  frame is never duplicated at the end.

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
```

On a machine without a GPU, Mesa's `lavapipe` (package `mesa-vulkan-drivers`)
is enough to run the tests and exports.

## License

GPL-3.0-or-later. The generated texture pack in `assets/textures` is released
under CC0.
