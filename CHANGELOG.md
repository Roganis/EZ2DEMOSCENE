# Changelog

## [Unreleased]

### Added

- **Gaussian splat layers** (+ Add → ☁ Gaussian splats): captured objects
  and places from `.ply`, `.spz` (versions 1 to 3) or `.splat` files,
  drawn back to front as soft ellipses, on desktop and on the web. Up
  axis, fit to size, tint, brightness, opacity, splat size, scatter and a
  splat budget; a small built-in galaxy until a file is chosen.
- More model formats for shapes: **STL, PLY, OFF and 3MF**, besides
  glTF/GLB and OBJ. STL and 3MF are stood Y up; files without normals get
  smooth ones kept sharp across edges.
- "3D model or splat file…" and dropping a file on the window pick the
  layer from the file: a PLY file becomes a shape or splats depending on
  what it holds.

## [1.0.0] - 2026-10-01

The first public release of EZ2DEMOSCENE: a desktop (Windows, Linux, macOS),
web and Android app for building seamless, stylised 3D loops the demoscene
way, without writing code. Start from a preset, tweak layers with sliders,
press **Export** and get an MP4, WebM, GIF or PNG sequence that loops with no
visible seam.

### Downloads

- **Windows:** `EZ2DEMOSCENE-windows-x86_64.zip` (ffmpeg and the DirectX
  shader compiler included).
- **Linux:** `EZ2DEMOSCENE-linux-x86_64.tar.gz` (ffmpeg included).
- **macOS (Apple silicon):** `EZ2DEMOSCENE-macos-arm64.tar.gz`. For video
  export, install ffmpeg with `brew install ffmpeg`.
- **Android:** the APK attached to this release.
- **Web:** <https://roganis.github.io/ez2demoscene/> (installable as a PWA,
  works offline after the first visit).
- **Manual:** `EZ2DEMOSCENE_User_Manual.pdf`, included in every desktop
  archive.

### Highlights

- **Seamless loops, guaranteed.** A loop is a whole number of beats at a
  chosen BPM, and every animation runs a whole number of cycles per loop.
  The test suite checks frame *N* equals frame 0 for every preset.
- **Presets in five groups:** Consoles & arcade, PC era, Home computers &
  handhelds, Demoscene and Engine showcases, plus your own saved presets
  and layers.
- **Layers:** raymarched backgrounds (tunnels, fractals, Menger sponge,
  volumetric clouds, aurora), mirror floors, 25 built-in shapes, a library
  of about 1,000 CC0 models and your own glTF/GLB/OBJ, terrain with water
  and lava, particles, text and scrollers, logos, sprites, GIF and video
  textures, electric arcs and the Mode 7 floor.
- **Loop-closed simulations:** flocks, cloth, rigid bodies and liquid,
  baked ahead of time so they repeat exactly.
- **Rendering:** sun shadows, image-based lighting, physical materials
  (GGX, clearcoat, sheen, glass), screen-space reflections, light shafts,
  bloom, god rays, depth of field and motion blur.
- **Retro looks:** 5th-generation console 3D (low resolution, vertex
  snapping, affine warp), Quake light styles, whole-screen machine
  resolutions and hardware palettes, borders and tape-loading stripes.
- **Music:** drop in an MP3/WAV/OGG/FLAC or a MIDI file and animate
  anything to its beats, loudness and bands; live input and live channels
  fed by other programs.
- **Scenes & timeline**, camera paths, a node graph for advanced users,
  and readable `.ez2.json` project files and `.ez2pack` bundles.
- **Automation:** a command line for headless exports, a JSON Schema of
  the project format, and an MCP server (`ez2demoscene --mcp`) for AI
  assistants.

See the [README](https://github.com/roganis/ez2demoscene#readme) for the
full feature list.
