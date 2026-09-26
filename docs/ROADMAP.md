# EZ2DEMOSCENE roadmap

Where the tool goes next, in the order the work will happen, with the
chosen way to build each item. Every item must keep the project's promise:
**a loop is a pure function of the loop phase**, so every new animation
runs a whole number of cycles per loop (or is made continuous at the wrap
point) and a test proves it.

Legend: ☐ to do · ◐ in progress · ☑ done

---

## Phase 0 — Fix what's there

### ☑ 0.1 Preview colours match exports
**Problem.** egui 0.36 treats user textures as gamma-encoded, but the
viewport hands it an sRGB view of the output. The GPU decodes it to linear
and egui shows linear values as gamma, so the preview is visibly darker
than exported frames.

**How.** The final post pass writes two colour attachments (multiple render
targets, fine on WebGL2): the sRGB `output` used for exports (unchanged), and
a plain `Rgba8Unorm` `display` texture holding the same gamma-encoded values
(`fs_final` already computes them just before its `to_linear`). The
viewport and preset thumbnails register `display`. No extra pass.

**Test.** Render a frame, read back `output` and `display`, and check the
bytes match within ±1.

---

## Phase 1 — Big visual step: shadows

### ☑ 1.1 Sun shadow map
**How.**
- One 2048² `Depth32Float` shadow map rendered from the sun (the
  day-cycle light direction when enabled) with an orthographic projection
  fitted to the camera's view (the frustum corners' bounding sphere), and
  texel snapping so shadows don't shimmer when the camera moves.
- A depth-only pass draws meshes (instanced, the same vertex path with
  displacement and glitch), terrain and waterfalls. Particles, beams and
  weather don't cast shadows.
- Mesh, terrain and floor fragment shaders sample it with a
  comparison sampler and 3×3 PCF (softness setting). The shadow darkens
  the direct light only; ambient light and glow stay.
- Settings in *Light & fog → Shadows*: on/off, softness, strength,
  distance. Off by default for old projects (golden images unchanged);
  on in new presets where it helps.
- Globals get the light's view-projection; group 3 gets the shadow texture
  and sampler (the existing groups stay untouched).

**Loop safety.** Pure function of the frame: nothing to do.

**Cost.** One extra depth pass. Skipped when off, or when no layer can
cast (backgrounds only). WebGL2 supports depth textures with comparison
samplers.

### ☑ 1.2 Soft contact shadows (ambient occlusion)
**Done as:** contact shadows on the mirror floor (a multiplied soft disc
under every copy, fading with height and fog).

**How.** A cheap, stable approach instead of screen-space AO (which
flickers and needs normals): per-instance **blob shadows** on the floor and
terrain for mesh layers (dark discs under each copy, sized by its bounds
and height), plus a *Contact shadow* strength. Screen-space AO can come
later with the Phase 7 depth work.

---

## Phase 2 — Speed, especially on phones

### ☑ 2.1 Half-resolution raymarched backgrounds
**How.** Backgrounds of the heavy kinds (volumetric clouds, fractal, sponge,
tunnel) render into a half- or quarter-resolution texture (setting:
*Background resolution*: full / half / quarter, default half on phones
for clouds), then a full-resolution pass upsamples it with a 4-tap
bicubic-ish filter into the scene before the geometry. Backgrounds are
drawn first and behind everything, so no depth-aware upsampling is needed.

**Expected gain.** Clouds are ~1.2 of the layer "load" budget; a quarter of
the pixels cuts that to ~0.3.

**Done:** *Resolution* (full / half / quarter) per background; only the
last (visible) background is drawn at all. Measured on the software
rasterizer, Sunbeam Peaks: 252 → 200 (half) → 179 ms (quarter) per frame;
cloud presets now default to half, visually identical.

### ☑ 2.2 Specialised shaders
**How.** The background, terrain and mesh shaders branch on the kind,
style, liquid and biome. Use WGSL `override` constants (wgpu evaluates
them for every backend, WebGL2 included) and build pipelines lazily per
combination (cached in a `HashMap<PipelineKey, RenderPipeline>`). The GPU
then compiles only the code a layer uses. That lowers register pressure,
which matters most on mobile.

**Done (backgrounds):** `BG_KIND` override; one pipeline per background
kind, pass and sample count, built on first use and cached. The golden
images are unchanged; every specialisation is checked by the HLSL/MSL/GLSL
test, and the WebGL2 build was checked in a browser (Aurora, half-resolution
Clouds). The software rasterizer shows no speed difference (LLVM already
handles uniform branches), so the gain is only on real GPUs: a simple
gradient sky no longer reserves registers for the fractal raymarcher.
Terrain and meshes stay single shaders: their branches are small, and
splitting them multiplies pipelines (style × liquid × biome) for little
expected gain. Revisit with profiling on a real phone.

### ☑ 2.3 Skip invisible work
- Skip the mirror reflection pass when the floor quad is outside the view
  frustum (test its four corners).
- Skip layers whose bounds are outside the frustum (meshes: instance
  bounds; particles: emitter radius; terrain: its box).
- Skip the sky overlay when it would do nothing (already partly done).

### ☑ 2.4 Faster exports
**How.** Keep up to three frames in flight: frame *n + 2* renders while *n*
reads back (a ring of readback buffers), and desktop ffmpeg writing moves to
its own thread with a bounded channel. Same for the incremental web job.
Both expected to give 1.5–2.5× export speed on real GPUs.

**Done:** desktop export keeps three frames in flight and writes PNGs /
feeds ffmpeg on a writer thread; the web job keeps three frames in flight.
On the software rasterizer (where the "GPU" shares the CPU cores with the
encoder) PNG export got 9% faster and MP4 2%; real GPUs overlap far more.

### ☑ 2.5 Music analysis off the main thread
Desktop: a background thread with a "analysing…" status. Web: chunked
analysis spread over frames (keeps the page responsive without a worker
bundle).

**Done:** `ez_core::analysis::Analysis` and `ez_export::MusicJob` decode
and analyse a slice at a time (identical results, tested). Desktop runs the
job on a thread, the web spends ~10 ms per frame on it; a spinner with a
percentage shows in the viewport bar and exports wait for it. The audio
analysis is cached, so MIDI and MIDI-offset changes apply instantly.

### ☑ 2.6 Terrain level of detail
Grid cells get coarser with distance from the camera (a radial ring layout
generated from the vertex index instead of a uniform grid), with morphing
between levels to avoid popping. Allows 4× bigger landscapes for the same
cost.

**Done, differently:** instead of rings (which need morphing and crack
stitching), a *graded* grid. Grid lines are spread so the spacing is the
full resolution at the camera and grows linearly with distance (the
grading is solved on the CPU per axis, the vertex shader maps each index in
closed form), so there are no levels, no pops and no cracks. It draws half
the cells per side (a quarter of the triangles). Sunbeam Peaks: 204 → 103
ms/frame on the software rasterizer. On in six terrain presets (mean image
change ≤ 1.4/255); Vector Valley and Dune Sea keep the full grid (the
wireframe's lines are the art; the dunes' distant ridges simplify
visibly).

---

## Phase 3 — Signal nodes (the node graph becomes a modulation system)

### ☑ 3.1 Signal wires
**How.** Nodes today pass *layers*. Add a second pin type, **signal** (one
number per frame, evaluated from the `EvalCtx`). The compiled graph keeps
layers plus a list of *bindings* `(layer, setting path, signal expression)`.
At render time bindings are evaluated and written into the layer's
parameters, so the renderer stays unchanged.
- Settings are addressed by a stable path (e.g. `material.emissive`,
  `transform.scale`, `camera.distance`, `post.chroma.amount`) through a
  small `visit_params(&mut Layer, |path, &mut Param|)` walker.
- Binding modes: replace, add, multiply.

### ☑ 3.2 Signal nodes
- **Sources:** LFO (all wave shapes, whole cycles), Beat (fade per beat or
  bar), Random (per step, loop-safe), Noise (closed circle in noise space),
  Music (every follow source and every hit kind with shape and length),
  Time (loop phase, beat).
- **Math:** Add, Multiply, Remap (in range → out range), Clamp, Smooth /
  lag (computed statelessly as a windowed average over the loop, so it
  stays exact), Quantize / step, Mix, Compare, Choose A/B.
- **Logic:** Every N bars (cycles through 2–8 values), Counter on hits
  (loop-safe: counts hits within the loop window), Hold.
- **Targets:** *Drive* node: pick a setting of the layers flowing through
  it and a mode.

**UI.** Signal pins are a different colour; the node shows a live
sparkline of its value over the loop.

**Loop safety.** Every source is a function of the loop phase and the
loop-window music; math is pointwise; lag is a symmetric window over the
circular loop. A test evaluates random graphs at phase 0 and 1.

**Done (3.1 + 3.2):** pins are typed (layers / signal) and mismatched
wires are refused. Setting paths come from a generic serde walk of the
layer (every number, animatable setting and colour channel, so new
settings are drivable automatically); Drive applies replace/add/multiply
through one JSON round trip per layer (no measurable cost). Sources reuse
the animatable `Param` (waves, beat fades, random steps, music follow and
hits in one node); plus Math, Remap, Quantize, Smooth (16 midpoint samples
of a symmetric window), Mix, Sequence (with glide that wraps) and Hit
counter (a new per-frame hit count in the music frame). Hold and a
separate Noise source were dropped: Sequence and the Param's smooth-random
and drunk waves cover them. A test evaluates 200 random graphs at phase 0
and 1 (it caught a float edge in nested Smooth, fixed by wrapping phases).
Preset: *Signal Flow*.

### ☐ 3.3 Geometry and layout nodes
- **Deform:** Twist, Bend, Taper, Noise displacement, Explode (per-face
  push). Applied in the mesh vertex shader through a small deform list in
  the draw block, animatable.
- **Layout:** Instance on mesh surface (sample triangles by area, seeded),
  Scatter on terrain (copies follow the height field; terrain height
  function mirrored on the CPU), Follow curve (copies along a ribbon curve,
  moving a whole number of laps per loop).
- **Material:** Palette cycle and Gradient ramp across copies.

---

## Phase 4 — Camera paths

### ☐ 4.1 Keyframed camera path
**How.** New camera mode *Path*: a closed Catmull-Rom spline through
points (position + look-at + roll + FOV), travelled a whole number of times
per loop with constant speed (arc-length table). An *easing* option
lingers at points. Viewport tools: "add point here" (from the current
view), drag points with the gizmo, show the path line.

### ☐ 4.2 Camera reacts to hits
Already possible through the 🎵 row on distance / FOV; add *punch-in* and
*cut to next point on kick* (a hit-driven jump along the path, loop-safe via
the loop-window hit list).

---

## Phase 5 — Text & logos

### ☐ 5.1 Text layer
**How.**
- A built-in bitmap font atlas generated at start-up (a crisp 8×8 / 16×16
  pixel font in the classic demoscene style, drawn in code like the texture
  pack) plus *Import font* (TTF/OTF rasterised with `ab_glyph` into an atlas).
- Glyph quads generated in the vertex shader from a per-layer glyph buffer
  (instanced: one instance per character).
- Styles: **Scroller** (runs a whole number of text lengths per loop),
  **Sine scroller** (per-character wave, whole cycles), **Static**,
  **Typewriter** (reveal per beat), **Greetings list** (one line per bar).
- Look: colour gradient, glow, outline, drop shadow, chrome (env reflection
  mapped on the letters).

### ☐ 5.2 3D logo text
Extrude glyph outlines (from the TTF outlines via `ab_glyph`/`ttf-parser`,
flattened and triangulated with ear clipping) into a mesh (`MeshSource::Text`),
so logos get every material, relief, glitch and copy option.

---

## Phase 6 — Scenes & sequencer

### ☐ 6.1 Several scenes, one timeline
**How.** `Project.scenes: Vec<Scene>` where a scene holds what a project
holds today (layers, camera, environment, post, graph), plus a sequence of
clips `(scene, start bar, length in bars, transition)`. Old projects are a
single scene. The loop length becomes the whole sequence; each scene still
sees its own phase, so scenes loop internally. The sequence itself loops.

### ☐ 6.2 Transitions
Crossfade, wipe (angle), iris, glitch cut, flash-to-white, and
"cut on kick" (music mode). Rendered by drawing both scenes into two HDR
targets during the transition and mixing in the post pass.

### ☐ 6.3 Sections from the music
Full-song mode proposes clip boundaries at detected section changes
(novelty in the smoothed spectrum), snapped to bars.

---

## Phase 7 — Finishing effects

### ☐ 7.1 Motion blur (exports)
Render *k* sub-frames per exported frame at phases `(i + j/k)/N` and
average them in a float accumulation target (shutter setting 0–1). Exact
and loop-safe by construction. Optional in the preview at low *k*.

### ☐ 7.2 Depth of field
Keep the scene depth (resolve it to a texture), compute circle of confusion
from a focus distance / aperture (animatable, music-linkable), blur with
a half-resolution gather (bokeh disc) and composite.

### ☐ 7.3 Feedback trails (loop-exact)
A feedback post effect (zoom, rotate, fade, hue shift of the previous
frame). Made exact by rendering one full loop as warm-up before the frames
that are kept, from a fixed state, so the export and the scrubbed preview
match and the loop closes (the feedback decays below visibility within a
loop). The preview uses the running history.

---

## Phase 8 — More layers

### ☐ 8.1 Raymarched objects in the scene
SDF objects (metaballs, gyroid, fractal bulb, smooth unions of spheres and
boxes) drawn as a box proxy mesh; the fragment shader marches inside the
box and writes `frag_depth`, so they intersect meshes correctly and get
the usual lighting, fog and shadows.

### ☐ 8.2 Sprites & image planes
Billboards or fixed planes with an image or an image sequence (frames
chosen by loop phase), alpha or additive.

### ☐ 8.3 Lightning arcs
A tesla-coil arc between two points (or from a point to the nearest copy),
reusing the lightning bolt code, re-striking a whole number of times per
loop.

### ☐ 8.4 GPU instancing (where compute exists)
Instance generation (orbit swarms, scatter, spectrum) moves to a compute
shader on WebGPU / native; WebGL2 keeps the CPU path. Allows 100k+ copies.

---

## Order of work

0.1 → 1.1 → 1.2 → 2.1 → 2.3 → 2.4 → 2.5 → 2.2 → 2.6 → 3.1 → 3.2 → 3.3 →
4.1 → 4.2 → 5.1 → 5.2 → 6.1 → 6.2 → 6.3 → 7.1 → 7.2 → 7.3 → 8.1 → 8.2 →
8.3 → 8.4.

Each item lands as its own commit with tests (loop seams, golden images
where the look is meant to stay, new goldens for new presets), README and
manual updates, and a check in the running editor. Every push builds the
Android preview APK.
