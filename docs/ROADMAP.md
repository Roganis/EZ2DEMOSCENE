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

### ☑ 3.3 Geometry and layout nodes
- **Deform:** Twist, Bend, Taper, Noise displacement, Explode (per-face
  push). Applied in the mesh vertex shader through a small deform list in
  the draw block, animatable.
- **Layout:** Instance on mesh surface (sample triangles by area, seeded),
  Scatter on terrain (copies follow the height field; terrain height
  function mirrored on the CPU), Follow curve (copies along a ribbon curve,
  moving a whole number of laps per loop).
- **Material:** Palette cycle and Gradient ramp across copies.

**Done:** everything is a layer setting (usable in Simple mode) *and* a
node. Deform lives in the mesh vertex shader (normals corrected, shadows
follow, culling widened). Colours across copies use a spare per-instance
slot (position among the copies) and 4 colours in the draw block; palette
cycling is its *Travel / loop*. Layout nodes gained a second, reference
input (a ribbon, shape or terrain they look at but don't pass on). Surface
points are sampled by area on the CPU from the shape's triangles and
cached by the renderer; the terrain height field is ported to Rust and a
render test checks copies against the GPU ground. Preset: *Crystal
Garden*.

---

## Phase 4 — Camera paths

### ☑ 4.1 Keyframed camera path
**How.** New camera mode *Path*: a closed Catmull-Rom spline through
points (position + look-at + roll + FOV), travelled a whole number of times
per loop with constant speed (arc-length table). An *easing* option
lingers at points. Viewport tools: "add point here" (from the current
view), drag points with the gizmo, show the path line.

### ☑ 4.2 Camera reacts to hits
Already possible through the 🎵 row on distance / FOV; add *punch-in* and
*cut to next point on kick* (a hit-driven jump along the path, loop-safe via
the loop-window hit list).

**Done (4.1 + 4.2):** closed uniform Catmull-Rom through eye, look-at,
roll and FOV; an arc-length table gives even speed and a smootherstep
blend gives *Linger*. Instead of dragging points with the gizmo (the
camera itself rides the path), shots are framed with the normal viewport
controls in Static mode and stored with *Add this view* / ⟳, and 👁 jumps
back to a shot; the flight line is drawn in the viewport. Cuts use the
per-frame hit count of the music frame (count mod points, plus a drift
during the beat); punch-in is a FOV kick on any camera mode. Crystal
Garden flies a path.

---

## Phase 5 — Text & logos

### ☑ 5.1 Text layer
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

### ☑ 5.2 3D logo text
Extrude glyph outlines (from the TTF outlines via `ab_glyph`/`ttf-parser`,
flattened and triangulated with ear clipping) into a mesh (`MeshSource::Text`),
so logos get every material, relief, glitch and copy option.

**Done (5.1 + 5.2):** no new dependencies: `skrifa` (already used by
egui) reads outlines and egui's bundled Hack and Ubuntu fonts are the
built-ins. Glyphs are flattened and turned into an exact signed distance
field (distance to the outline's segments, sign by winding) in a 16×12
cell atlas; the Pixel font is Hack snapped to an 11-per-em block grid
(blocks lit when inside or near the outline, so thin strokes survive).
Letters are one instanced quad each, laid out on the CPU per frame (loop
tests for every style); the shader does gradient, glow halo, outline,
drop shadow and a chrome bevel from the field's gradient. *Face the camera*
billboarding was added. 3D text triangulates contours by ear clipping with
hole bridging (holes found by containment depth, so TrueType and CFF
orientations both work) and extrudes walls smoothed across gentle corners;
the pixel font becomes voxel blocks. Presets: *Oldschool Intro*.

---

## Phase 6 — Scenes & sequencer

### ☑ 6.1 Several scenes, one timeline
**How.** `Project.scenes: Vec<Scene>` where a scene holds what a project
holds today (layers, camera, environment, post, graph), plus a sequence of
clips `(scene, start bar, length in bars, transition)`. Old projects are a
single scene. The loop length becomes the whole sequence; each scene still
sees its own phase, so scenes loop internally. The sequence itself loops.

### ☑ 6.2 Transitions
Crossfade, wipe (angle), iris, glitch cut, flash-to-white, and
"cut on kick" (music mode). Rendered by drawing both scenes into two HDR
targets during the transition and mixing in the post pass.

### ☑ 6.3 Sections from the music
Full-song mode proposes clip boundaries at detected section changes
(novelty in the smoothed spectrum), snapped to bars.

**Done (6.1–6.3):** the project's own fields stay "the scene being
edited" and other scenes wait in `Project.sequence` (switching swaps
contents, so every panel, the node editor and undo keep working). Clips
map the whole-sequence moment to each scene's own moment; a transition
covers the start of the incoming clip while the outgoing scene keeps
running, so the wrap is seamless by construction. Each scene is rendered
with its own post effects into its own target and a compositing pass mixes
the finished pictures (instead of mixing HDR before post, which would have
forced one post stack on both). Section detection: per-bar spectrum and
loudness, novelty between the 4 bars before and after each boundary,
peaks above mean + ½σ at least 4 bars apart; the button makes one clip
per section (tested on a synthetic three-part song). Preset: *Scene
Tour*.

---

## Phase 7 — Finishing effects

### ☑ 7.1 Motion blur (exports)
Render *k* sub-frames per exported frame at phases `(i + j/k)/N` and
average them in a float accumulation target (shutter setting 0–1). Exact
and loop-safe by construction. Optional in the preview at low *k*.

**Done:** sub-frames are averaged on the CPU after readback (linear light
via a lookup table), which keeps the three-frames-in-flight pipelining and
works identically in the web export job; desktop, web and `--motion-blur`
/ `--shutter` on the command line. Not in the preview (it would cost *k*
renders per preview frame).

### ☑ 7.2 Depth of field
Keep the scene depth (resolve it to a texture), compute circle of confusion
from a focus distance / aperture (animatable, music-linkable), blur with
a half-resolution gather (bokeh disc) and composite.

**Done, differently:** the main depth buffer is multisampled, which
WebGL2 can't sample, so a small half-resolution pass redraws solid meshes,
terrain and the mirror floor with a `fs_depth` entry point writing the
distance to the camera into an R16F texture (works on every backend). The
warp pass (which already samples the scene once) does a 24-tap
golden-angle gather with a thin-lens CoC, weighting each tap by whether
its own blur reaches the centre so sharp fronts don't smear. Auto focus
uses the camera's look-at point; focus and blur are animatable and
music-linkable.

### ☑ 7.3 Feedback trails (loop-exact)
A feedback post effect (zoom, rotate, fade, hue shift of the previous
frame). Made exact by rendering one full loop as warm-up before the frames
that are kept, from a fixed state, so the export and the scrubbed preview
match and the loop closes (the feedback decays below visibility within a
loop). The preview uses the running history.

**Done.** `post.feedback` (length as a Param, zoom/turn/hue per second)
runs after god rays: `fs_feedback` mixes the new frame with the zoomed,
turned, hue-shifted history in two ping-pong textures. Per-second rates
with fade = keep^(dt·30) keep it frame-rate independent; a jump (dt over
half a second) resets the history, and drawing the same moment again (a
paused preview while editing) redoes the last step from the same history
instead of feeding the picture back into itself. Looped exports render
the whole loop once as warm-up (not written) through both the CLI
pipeline and the web `ExportJob`, so frame 0 carries trails and loop
N+1 matches loop N exactly (tested: loop-to-loop difference 0).

---

## Phase 8 — More layers

### ☑ 8.1 Raymarched objects in the scene
SDF objects (metaballs, gyroid, fractal bulb, smooth unions of spheres and
boxes) drawn as a box proxy mesh; the fragment shader marches inside the
box and writes `frag_depth`, so they intersect meshes correctly and get
the usual lighting, fog and shadows.

**Done.** `MeshSource::Sdf { form, cycles }` (Metaballs, Gyroid, Fractal
bulb, Melting box) on the mesh layer, so copies, transforms, materials,
music links and ramps all apply. `sdf.wgsl` draws a ±1 box; only its far
faces march (from the near plane through the pixel, via
`inv_view_proj`, so perspective, the orthographic sun view and the
mirrored reflection view all work), in object space through the
inverse model matrix (flat varyings), and writes `frag_depth`. Separate
pipelines for the sun shadow map (depth only, with its own bias) and the
depth-of-field distance pass. The lighting moved from `mesh.wgsl` to a
shared `lit_surface()` (plus SDF ambient occlusion). Motion is whole
cycles per loop; copies vary by their random seed. Tested: every shape
shows, moves, loops exactly, cuts a bar through it and casts a shadow.
Preset: Liquid Metal.

### ☑ 8.2 Sprites & image planes
Billboards or fixed planes with an image or an image sequence (frames
chosen by loop phase), alpha or additive.

**Done.** A Sprite layer (`LayerKind::Sprite`): facing camera, upright
or fixed; alpha (copies sorted far to near), additive or cutout (writes
depth and draws with the solid pass); size, opacity and glow as Params.
Copies reuse the shape instancers through a shared `copies_with()` (graph
nodes and terrain linking now go through `LayerKind::instancer_mut()`).
Sheets play `cycles` whole passes per loop, optionally offset per copy;
the shader caps the mip level and insets samples so frames never bleed
into each other. Four built-in 4×4 sheets with alpha (explosion, flame,
coin, sparkle). Tested: every blend and facing shows, plays and loops.
Preset: Campfire Sprites.

### ☑ 8.3 Lightning arcs
A tesla-coil arc between two points (or from a point to the nearest copy),
reusing the lightning bolt code, re-striking a whole number of times per
loop.

**Done.** An Electric arcs layer with three paths: between two points,
from the layer to the N nearest copies of a shape or sprite layer
(re-picked every frame, so arcs jump as copies orbit), or copy to copy
round a ring. The weather bolt's noise, point and quad helpers moved to
`common.wgsl`; `arcs.wgsl` pins both ends, crawls the noise during a
strike and reseeds per strike (`strikes` whole per loop), with a
fade per strike and optional branches. One instance per arc, segments
from the vertex index. Tested: all three paths show, re-strike and loop.
Preset: Tesla Swarm.

### ☑ 8.4 GPU instancing (where compute exists)
Instance generation (orbit swarms, scatter, spectrum) moves to a compute
shader on WebGPU / native; WebGL2 keeps the CPU path. Allows 100k+ copies.

**Done.** A new copy layout, **Big swarm** (`Instancer::Swarm`: orbits,
cloud, shell, galaxy; up to 250,000), rather than changing Orbit and
Scatter: those draw from a sequential RNG stream, so a GPU port couldn't
reproduce them copy by copy and every saved project would change. Each
swarm copy is a pure function of its index (`swarm_local`, per-index
hashes), and `swarm.wgsl` ports it and the whole variation step (random
turn/size/spin, ripple, chase, hue, spectrum bars) line for line. The
renderer runs one compute dispatch per symmetry copy, writing straight
into a VERTEX|STORAGE buffer that the mesh, shadow and depth-of-field
passes draw from; WebGL2 (no compute) takes the CPU path. Tested: GPU and
CPU images match (mean difference ≤ 0.008/255) for every form, with
symmetry and variation on, and every form loops. Measured: on the CPU,
100k copies cost ~16 ms per frame to generate plus an 8 MB upload; the
compute path removes both. On llvmpipe (CPU-emulated GPU) the frame time
is the same either way (~0.4 s, dominated by rasterising 400k
triangles), so the gain shows on real GPUs only. Preset: Galaxy Swarm.

**Follow-up: every layout on the GPU.** Orbit, Scatter and On-a-terrain
now draw per-copy hashes instead of a sequential random stream (saved
projects place their copies differently; the look is the same). Orbit
is the swarm's orbit form with a lower cap. `copies.wgsl` (formerly
`swarm.wgsl`) ports every layout — grid, radial, wall, spiral, scatter,
curve, single, the swarm forms — and every mesh layer goes through the
compute pass when it exists. Surface and terrain copies need the mesh
sample / the landscape, so the CPU places them and uploads the matrices;
the GPU applies the variation. GPU layers get analytic bounds from their
layout (so off-screen culling still works) and contact shadows read the
compute buffer. Sprites stay on the CPU (alpha sprites are sorted far to
near there). Tested: GPU and CPU pictures match for every layout (mean
difference ≤ 0.003/255), with symmetry and variation, and all loop.
Benchmark: `render()` CPU time per frame down slightly (Gold Kaleido
Room 0.70 → 0.58 ms), frame totals unchanged on llvmpipe.

---

## Phase 9 — 2D logo layer

### ☑ 9.1 Logo layer with a baked distance field
A screen-space layer for logos and title cards: an imported image (PNG with
alpha) or a line of text, placed by position, scale, rotation and anchor
on screen rather than in the world. It draws into the HDR scene target
after the 3D pass and before post, so bloom, feedback and transitions
apply to it.

**How.**
- On import, bake a signed distance field from the image's alpha (a
  one-off CPU distance transform, 8SSEDT or jump flooding, at a capped
  resolution) and keep it beside the colour image. Text logos reuse the
  glyph SDF atlas from 5.1.
- The SDF drives shape and every effect below; the colour texture keeps
  the artwork's own colours (or the layer's gradient replaces them).
- Reuse the text layer's looks (gradient, outline, glow halo, drop shadow,
  chrome bevel) by sharing the SDF shading from `text.wgsl` in
  `common.wgsl`.
- Every look value is a Param, so music and signal nodes can drive it
  (scale punch, glow flash or outline width on kicks).

**Done, differently in three places.** A Logo layer (`LayerKind::Logo`)
made of text (a built-in font or a TTF/OTF) or an image, whose shape comes
from its transparency, its bright parts or its dark parts (cropped to the
logo). Placed by anchor point, across/up position, height as a fraction of
the screen and a turn, all Params; the gizmo shows one handle at the
anchor (move, scale, rotate). The differences from the plan:
- **One baked texture per logo, not a colour image plus a field.**
  `logo.rs` bakes a half-float RGBA texture: linear colour spread outward
  from the shape (so edges never pick up the background) in rgb, the
  signed distance field in alpha. The field is unclamped, ready for the
  far-reaching effects of 9.3.
- **An exact distance transform, not 8SSEDT or jump flooding.**
  Felzenszwalb–Huttenlocher (lower envelopes of parabolas) on the inside
  and the outside, at 2–4× the output resolution, averaged down, so the
  outline falls between pixels. Rows run on several threads on desktop.
  Text logos are filled from the glyph outlines (non-zero winding) and
  go through the same bake instead of the glyph atlas, so a multi-line
  logo is one shape. A text bake takes about 0.1 s on the development
  container; bakes are cached by source and dropped once unused.
- **Drawn after depth of field, not in the scene pass.** A pass of its
  own on the picture right after the warp/depth-of-field pass, before
  god rays, feedback, bloom and the grade: blur never softens a logo,
  every other post effect applies. No MSAA or depth; the field
  antialiases.
The text layer's shading moved into `sdf_look()` in `common.wgsl`; the
text layer calls it unchanged (every golden image matches exactly).
Tested: the bake against a brute-force distance transform and an
analytic disc, masks, every font; on the GPU, text and image logos with
every look show only where they are anchored, move and loop, empty ones
draw nothing, and depth of field leaves a logo sharp. Preset: Sunset
Title.

**Follow-up: placement like a photo editor's reference points.** A logo
is placed against *the screen* or *another logo layer* (by name), from
one of nine points of it (`attach_point`), plus offsets in fractions of
the screen. A Place grid snaps: on the screen inside that point, a
margin in; against a logo outside that side (the logo's own anchor is
the opposite point), a small gap out, so a subtitle snaps under a title
in one click and follows it, turned or animated. The renderer resolves
the chain each frame from the baked widths (missing targets and loops
fall back to the screen) and hands the anchors to the viewport's
handles. Old projects keep their meaning: the default point is the
screen's bottom left, which makes the offsets the old absolute
position; new logos from + Add are measured from their own anchor.
Tested: a title in a corner, a subtitle under it (also after the title
moves and turns), a missing target, and two logos attached to each
other.

### ☑ 9.2 Lit logos
Make a flat logo read as a solid, shiny object.

**How.**
- Normal from the SDF gradient, height from a remap of distance: bevel
  profiles *round*, *chiselled*, *stepped* (quantised distance, terraced)
  and *pillow* (the whole logo curves).
- A 2D light (fixed, circling a whole number of times per loop, or
  following the beat) for diffuse and specular.
- **Glint sweep:** a bright diagonal band crosses the bevel a whole number
  of times per loop.
- **Matcap** option: look up a small sphere image by the normal (a few
  built-in matcaps: gold, chrome, plastic, candy), beside the existing
  environment reflection.

**Done.** A Lighting section on the logo layer. The height is a remap of
the field (round: a quarter circle; chiselled: linear; stepped: smoothed
terraces; pillow: a quarter circle spanning the deepest point, which the
bake now records), and the normal is its slope from neighbours one texel
apart (the text bevel's wider spacing flattened thin strokes), turned
with the logo. The light is not three modes but one direction on the
screen plus a height, both Params: *fixed* is a value, *circling* a Saw
with amplitude 180° (a "Circle the light" button sets it), *following
the beat* a music link. Flat tops keep their colour; slopes towards the
light brighten, away from it darken (Shading), with a Blinn highlight
(Shine, Gloss). Matcaps are generated built-in textures
(`matcap_gold`, `matcap_chrome`, `matcap_plastic`, `matcap_candy`) or
any image, bound as a second texture, and mix over the shaded colour;
the existing chrome reflection stays as it was. The glint is a Gaussian
band in logo heights at `fract(phase × sweeps)`, masked by the letters,
so it loops exactly. Tested: every bevel shades the letters and follows
the light, a matcap changes them, a glint with a circling light loops and
moves. Sunset Title's title got a round bevel and a glint on every bar;
+ Add → Logo has a Gold logo.

### ☑ 9.3 Distance-field effects
**How.**
- **Contour lines / neon tubes:** `fract(d * N - phase * cycles)`, rings
  that pulse outward from the edges.
- **Stacked outlines:** several coloured rings at set distances.
- **Fake extrusion:** sample the SDF a few times along one direction for
  a stepped isometric extrude without a mesh.
- **Dissolve / burn-in:** noise thresholded against distance, with a hot
  glowing rim at the front.
- **Reveal wipes:** by distance (grows from the skeleton outward or from
  the edge inward), linear or radial.
- **Morph** between two logos by mixing their fields (also as a scene
  transition).

**Done, but the morph is not a scene transition.** Distances are in logo
heights throughout. Effects reaching past the baked texture needed more
field than its padding: the bake now adds a coarse **far field** (the
same distance transform on cells of 2+ texels, about 300k at most)
reaching 2.2 logo heights around the shape; the shader uses the fine
field inside the texture and blends into the far one over the outer part
of the padding, and the quad grows by each effect's reach. (Extending
the fine field from its edge with its gradient left bands where the
gradient turns.) Logos now bind their texture, the morph target, the
matcap and both far fields in one group, which frees a second block of
settings.
- **Rings:** `fract(distance / spacing - fract(phase × rings))`,
  antialiased with the distance's screen derivative, fading with the
  reach and to nothing before the quad's edge; optionally inside too.
- **Stacked outlines:** up to 16 bands outward from the edge, colours
  from inner to outer.
- **Extrusion:** the field sampled 24 times back along a direction,
  nearest in front, darkening with depth.
- **Dissolve:** two octaves of value noise mixed with the depth inside
  the letters (0 patches, 1 from the edges), against the amount, with an
  HDR burn front.
- **Reveals:** Grow sinks the field by the unrevealed part of the
  deepest point (so outlines, glow and every effect grow with it);
  Edges first hides what is deeper than the revealed depth; Wipe and
  Circle are soft masks over the logo and its effects' margin.
- **Morph:** into another text (same font) or image, baked like the logo
  and placed on the same anchor and height; the two distances are mixed
  in logo heights, colours with them. A full morph matches the other
  logo drawn alone. As a scene transition it would need the other
  scene's logo; instead the morph is a Param, so a ramp over a song
  section does the same job.
Tested: the far field against the analytic distance of a disc; on the
GPU every effect shows, animates and loops, every reveal shows nothing
at 0 and the whole logo at 1, and a full morph is the other logo.
Preset: Logo Morph.

### ☑ 9.4 Rasters & distortion
**How.**
- **Copper bars:** horizontal colour bands scrolling through the logo only
  (masked by the SDF), a whole number of passes per loop.
- **Sine wobble / rubber logo:** per-row and per-column UV offsets, whole
  cycles per loop.
- **Raster glitch:** hashed horizontal slices shifted and colour-split,
  reseeded per beat.
- **Chromatic split:** R, G and B sampled at small offsets.

**Done.** The logo's fragment shader became `shade(uv, box, lod)`; the
entry point moves the picture first (wobble: a sine of the row for the
sideways sway and of the column for the bob; glitch: slices hashed with
a seed that steps a whole number of times per loop, so the jumps repeat
every loop), then shades once, or three times for a colour split,
keeping red from one, green from the middle and blue from the other
(with the average coverage). A jumping slice adds its own split. Mip
levels come from the unmoved coordinates, so slice edges don't blur.
Copper bars replace the colour in the letters before lighting (two
colours alternating, so the scroll is in pairs of bars per loop to stay
seamless). The settings outgrew the second block: logo effects are now
bound as 512 bytes (two slots) of the draw buffer. The quad grows by
the distortions' reach. Tested: every effect shows, moves and loops.
Preset: Copper Logo.

### ☑ 9.5 Retro looks
**How.**
- **Pixelate / mosaic** with an animatable block size (pixelate in and
  out as a reveal).
- **Palette + ordered dither:** quantise to a small palette using the
  existing `bayer4`, with **palette cycling** a whole number of turns per
  loop.
- **Halftone** dot screen, dot size following brightness.
- **Scanlines and CRT glow** limited to the logo.
- **Moiré:** two rotating line patterns multiplied inside the mask.

**Done.** In the logo shader's entry point: pixel blocks snap the
position to the middle of its block before anything else, so every
effect is worked out once per block; within blocks the antialiasing
widths are pinned small (a screen derivative across two blocks would
draw a line at every block edge). The finished colour then goes through
moiré (two line patterns turning opposite ways, whole turns per loop),
halftone (a dot per cell by brightness, transparent between dots), the
palette and the CRT look. The palette mirrors the post effect's
quantiser (same Bayer threshold, spread and weighted distance, the VGA
cube with 6 levels), in gamma, per block when pixelated; *by
brightness* sorts the palette dark to light and picks along it, and
cycling rotates the index by whole palette turns per loop. Scanlines
count per logo height; phosphor stripes are per screen pixel. The
palette (16 colours) needed a third block of logo settings. Tested:
every look shows, moves and loops. Preset: C64 Title.

### ☑ 9.6 Logo meets scene
**How.**
- **Glass logo / lens:** copy the scene target before the logo draws and
  sample it offset by the SDF normal (refraction, with optional chromatic
  dispersion and a tint).
- **Rays from the logo:** feed the logo as the source to the god-ray pass,
  with the ray centre at the logo's centre (or behind it, so the logo
  occludes a bright backdrop).
- **Echo trails:** a few past copies of the logo's transform drawn with
  fading alpha. Past states are the layer at earlier phases, so they
  are exact and loop without a history buffer.

**Done.**
- **Glass:** when a logo has glass (or shadow rays), the picture after
  depth of field is copied to a backdrop texture (kept by the renderer,
  as big as the largest target) just before the logo pass. The letters
  sample it at their pixel, moved by the surface's tilt (the bevel's
  normal, or a rounded edge's without a bevel) times the bend in logo
  heights; red and blue bend more and less for dispersion; a rim where
  the tilt is steepest keeps glass letters readable over dark scenes.
- **Rays:** after the logos, each logo with rays (up to four) is drawn
  on its own into a full-size source picture (made for a target the
  first time a logo with rays is drawn there, dropped when unused),
  black around it, or, for
  shadow rays, onto the backdrop with a blend that multiplies by one
  minus its coverage. The god-ray shader then runs from the logo's
  middle with no falloff (the sun's rays keep theirs, now a parameter)
  and is added to the picture.
- **Echoes:** the logo's blocks are built by one function at any phase;
  each echo is the layer at `phase − k × spacing` (placement and
  attachments included), drawn oldest first with its opacity times
  fade^k. Pure functions of the phase, so they loop.
Tested: glass differs from a plain logo and bends; both kinds of rays
show and loop; echoes trail a moving logo and loop. Preset: Glass Galaxy.

---

## Extras

### ☑ Colour scheme from one key colour
One key colour and a harmony rule (Mono, Analogous ±30°, Complementary,
Split 150°/210°, Triadic, Tetradic) that the whole project follows, live:
`Project::scene_layers` (and `scene_environment`, `scene_color` for the
sky, fog, sun and god rays) harmonise copies of the colours when drawing,
so every renderer, export, thumbnail and scene sees the same, and the
stored colours never change. Each colour goes to OKLCH: lightness kept,
hue pulled to the nearest scheme hue (picked before the key's animated
turn, so a turning scheme rotates every colour smoothly instead of
jumping between hues), chroma optionally matched to the key's, then back
into the displayable range by reducing chroma; near-greys stay grey and
glows keep their brightness. One visitor, `for_each_color_mut`, reaches
every colour of every layer kind; the Tint node and Randomize use it
too (they used to miss outlines, the mesh colour ramp and several logo
colours). Layers can keep their own colours;
Randomize picks a new key when a scheme is on. Tested: OKLab round trip,
lightness kept, greys untouched, hues on the scheme, full turns, smooth
turning, the visitor against the saved layers, and on the GPU: off is
byte-identical, on recolours, kept layers stay, a turning scheme loops.
Preset: Colour Wheel Arena.

---

## Phase 10 — Loop-closed simulations

Flocks, cloth, rigid bodies and fluid-like particles. Nothing in the app
simulates yet: every motion today is a formula of the phase. A simulation
depends on its own history, so the plan is to **simulate ahead of time
into a bake, and make the bake itself loop**. Drawing then reads the bake
at the phase, so the promise still holds: the picture at a phase depends
only on the settings, the loop length and the loop-window music, never on
what the preview happened to play before.

### ☑ 10.1 The bake and closing the loop (shared by every simulation)
**How.**
- `ez_core::sim`: a small `Sim` trait (`reset(settings, seed)`,
  `step(dt, &EvalCtx)`, `state()`) and a `Bake` that runs it at a fixed
  step (120 steps per second of loop time, whatever the export frame
  rate) and keeps **K keys per loop** (positions and velocities, plus
  rotations for rigid bodies; K = frames per loop at 60 fps, capped at
  480). Between keys, cubic Hermite from the stored velocities, so any
  phase (odd export rates, motion-blur sub-frames, scrubbing) reads
  smoothly.
- Forces are functions of the moment in the loop: settings are Params
  evaluated at the simulated time (so beat pulses, music links and signal
  nodes drive a simulation), wind turns a whole number of times per loop.
- Three ways to close the loop (`LoopClose`, picked per layer):
  - **Cross-fade halves.** The clouds' trick, unchanged: the bake is
    drawn twice, half a loop apart, with weights `1 − |2x − 1|` and the
    rest; each copy jumps back while its weight is 0. Exact by
    construction. Suits everything drawn with opacity or added light
    (glowing particles, smoke, fluid splats). Costs twice the draw, not
    twice the bake.
  - **Blend the tail.** For things that must not ghost (solid flocks,
    rigid bodies, cloth): simulate from −B to L (B = blend length,
    default a quarter loop). Over the last B of the loop, each body
    moves from its own state towards its state at the same time one loop
    earlier (the pre-roll): positions and velocities by smootherstep,
    rotations by normalised lerp on the short arc. At the end of the
    loop it *is* the start. Optionally **guided**: while baking the
    tail, a spring pulls each body towards its pre-roll state with a
    strength rising over B, so most of the closing is physical and the
    final blend only mops up.
  - **Ping-pong.** Forward for half the loop, backward for the other
    half: exact with no blend. The classic collapse-and-rebuild.
- **Warm-up.** Dissipative systems under loop-periodic forces (cloth in
  a turning wind, fluid in a rocking tank) settle into a cycle that
  nearly repeats. The bake runs up to 4 loops of warm-up until the
  loop-to-loop difference is under a threshold, so the tail blend has
  almost nothing left to hide. The closing error is shown next to the
  setting ("seam: 0.3 cm").
- **Determinism.** The same bake on desktop, web and Android: single
  thread or fixed partitions with fixed-order sums, no platform `sin` /
  `exp` inside the step (a small polynomial set in `sim/math.rs`), a
  stated seed. Chaotic systems amplify one-bit differences, so this is
  what keeps the web export equal to the desktop one.
- **Running it.** Like the music analysis (2.5): a bake is cached by a
  hash of its settings, loop length and music analysis. Desktop bakes on
  a thread, the web ~10 ms per frame, with "simulating… 40 %" in the
  viewport bar; exports (desktop, web, CLI) wait for it. While a new bake
  runs, the preview keeps the old one. `.ez2pack` does not store bakes
  (they are rebuilt, so they stay equal to the settings).

**Budgets** (bake time on the web ≲ 3 s, memory ≲ 32 MB per layer):
flock 2,000 (10,000 desktop), cloth 64 × 64, rigid bodies 300, fluid
8,000 particles. Keys are stored in f16 where precision allows.

**Test.** For each closing mode: the sampled state at phase 0 and 1 is
identical; the state is continuous across the blend window's start (no
jump in position, a bounded one in velocity); baking twice gives the same
bytes; a golden hash of a small bake catches accidental non-determinism.

**Done (the engine); wiring into the editor and exports comes with 10.2.**
`ez_core::sim`: a `Sim` moves a fixed set of `Body`s (position, velocity,
rotation, size; size 0 hides one, so emitters and respawns fit); a
`BakeJob` runs it a slice at a time into a `Bake`, and `Bake::sample`
fills a reusable `Frame` of one layer, or two weighted layers for a
cross-fade. 120 steps and up to 60 keys per second (16–480 keys per
loop), cubic Hermite between keys, positions, velocities and sizes
blended, rotations by normalised lerp. The differences from the plan:
- **The guided tail closes the gap by itself.** A pull of fixed strength
  only halved the gap of bouncing balls (the collisions push back faster
  than it pulls). Now each step closes `3 × step / time left` of the gap,
  eased in at the tail's start, so the gap shrinks like the time left
  cubed and is exactly zero a few steps before the end; the final blend
  then has nothing left to hide. The pull's own movement goes into the
  recorded velocities, so the interpolation between keys stays true.
  *Seam* reports what the final blend hides: 0 when guided, the natural
  gap otherwise (the test's chaotic balls: 2.1 units RMS unguided).
- **Warm-up can stop after any loop.** Warm-up loops cover the same
  phases as the recording (they start where the pre-roll starts), so
  stopping after any whole number of loops is seamless. The test springs
  settle loop by loop (differences 1.08, 0.044, 0.0018, 0.00007) and
  stop at the fourth.
- **Keys are f32**, not f16, for now: 2,000 bodies with rotations over a
  4-second loop take 26 MB, inside the budget. Halve it when a layer
  needs more.
Determinism: `sim::math` (sine, cosine, exp by range reduction and
polynomials in f64, quaternion helpers). Checked for real: the test's
chaotic bake built for wasm and run in node gives the native hash; the
same bake with the platform's `sin`/`cos` gives a different hash natively
(glibc) than in the browser. `same_bake_everywhere` pins the hash, and CI
runs it on Linux, Windows and macOS (ARM). `BakeCache` runs bakes per
slot and settings key (`KeyHasher`, a stable FNV-1a): on a thread on
desktop, in slices from `poll` in the browser, handing out the slot's
previous bake until the new one is ready; `finish_all` waits (exports),
`end_frame` forgets slots nobody asked for, and a bake no longer wanted
stops its thread. Tested: every closing mode is continuous over the whole
loop (6,000 samples; no layer moves further than its velocity allows,
and velocities are the positions' slope, through the blend too), the
tail arrives at the start, the cross-fade weights are 0 where each copy
jumps, ping-pong mirrors, Params drive the simulation at the simulated
moment, slicing doesn't change a bake, and the cache in both modes.
Wired in with 10.2: the renderer owns the cache (so the preview, both
exports, thumbnails and tests share one path) and the music envelope
(`set_audio`); the app polls it every frame (8 ms of baking per frame in
the browser) and shows *Simulating… n %* in the viewport bar. Desktop
exports and stills wait for bakes; the web export job, which shares the
editor's renderer, renders a frame again until no simulation in it was
drawn from an old bake. The cache keeps ready bakes by key (so two scenes
in a transition, or preset thumbnails, never restart each other's bakes)
and forgets them after 240 frames unused.
Also: `Param` waves (sine, pulse, exponential fades, swell) and the beat
pulse now use `sim::math`, since a simulation's settings are Params (every
golden image unchanged within 0.24/255). Music analysis still uses the
platform's maths, so a bake *driven by the music* is only as identical
across platforms as the analysis is.

### ☑ 10.2 Flocking
**How.** A new copy layout, **Flock** (`Instancer::Flock`), so any shape
or sprite layer can flock and keeps materials, shadows, variation and
colour ramps. Boids with separation, alignment and cohesion on a uniform
grid (neighbours in the 27 cells), a speed range, turning limits and
banking into turns (the copy's up leans towards the turn). Bounds: a
sphere, a box, or around a target point that follows a camera-path-like
closed curve, another layer, or a music-linked position. Extras:
avoid a shape layer's bounds, a predator (one lead copy with its own
path), scatter on a hit (a kick pushes every boid away from the centre).
The CPU places copies and uploads the matrices (the path surface and
terrain copies already use), the GPU applies variation.

Closing: blend the tail, guided, by default (cross-fade halves for
glowing sprites).

**Test.** Loops; boids keep their minimum distance; no copy leaves the
bounds; a scatter hit is visible in the frame after the kick. Preset:
*Starling Dusk* (thousands of dark birds against a sunset, a
music-linked attractor).

**Done, differently: a flock holds a loose formation.** Measured first:
a free flock of 300 ends its loop with each boid 8.5 units (RMS) from its
start, the whole flock's width, and closing that over a quarter loop
made boids race at 4–6× their cruising speed. Numbering is arbitrary, so
no blend can hide that, and matching boids up at the wrap is impossible
(a copy would have to become another). So each boid also has a place of
its own: a small tilted circle inside the flock, turned a whole number of
times per loop at about the cruising speed, all turning the same way (on
opposite-turning circles boids met head on, too fast to steer apart).
*Formation* (default 0.8, 0 = free) sets how firmly it keeps to it; the
flock nearly repeats (seam 1.5 → 0.2 units), and the rest is closed by:
- **A guided tail that steers.** `Sim::guide` (default: the kinematic
  pull of 10.1) lets a simulation steer to its start instead: the flock
  adds critically damped steering, capped at 6× its agility, while
  separation (inverse distance, capped at 8×) still keeps boids apart.
- **A short drawn blend when guided.** Blending the drawn positions over
  the whole tail averaged two arrangements of the flock and put a quarter
  of the boids on top of each other; guided bakes now blend only the last
  fifth of the tail (`GUIDED_BLEND`).
- **A half-loop tail by default.** Steering back over half the loop: the
  mean speed rises from 2.3 to 3.3 at worst, and about 8% of boids pass
  closer than a third of their spacing for a moment near the end (none
  the rest of the loop).
The flock flies in its target's frame (the target's motion is added on
top), so it follows a moving target exactly; before, the formation
spring lagged a quarter loop behind a target on a curve. Steering is
capped by *Agility*; keeping apart and scatter have their own caps.
Copies face along their velocity and bank (lift leans into the turn),
turning smoothed. Neighbours come from a hashed grid built by a counting
sort (the same order everywhere). Cross-faded flocks shrink each half away
instead of fading it (shapes have no opacity). Not done: a predator,
avoiding another layer and "around another layer" targets. Up to 2,000
boids. Starling Dusk's 600 boids over 9.6 s take 19 MB and about 3 s to
bake on the 4-core test container (one warm-up loop by default for
flocks; holding a formation, more changes the seam little: 0.03, 0.008
and 0.013 RMS for 0, 1 and 2). The browser bakes in 8 ms slices per
frame, so expect several times that there.
`ez_core` is now optimised in dev builds too (bakes were ~20× slower).
Tested: loops (also free), spacing (≤ 2% crowded outside the tail), stays
in its area at the cruising speed also while closing, faces where it
flies, follows a path (centre within 1.5 units), a scatter pulse turns
the flock outward at once and spreads it, the same bake on every platform
(the hash checked natively and in wasm), saving and loading; on the GPU,
nothing is drawn until baked (and the frame says so), a waiting renderer
and a polling preview draw the same birds, they move and the loop closes
(seam 0.0000); the web export job's frames equal a waiting renderer's.
Preset: *Starling Dusk* (600 birds on a Lissajous path, scattering on
every bar).

### ☑ 10.3 Cloth
**How.** A new shape source, `MeshSource::Cloth` (flag, curtain, cape,
banner, tablecloth draped over a sphere or box). Position-based
dynamics (XPBD): stretch and bend constraints, pinned edges or corners,
self-collision off by default, collision with the floor, the terrain
height field (already ported to Rust) and simple colliders (sphere, box,
capsule from other shape layers' bounds). Wind: a direction that turns
whole cycles per loop plus turbulence from looping noise (a closed
circle in noise space, as the signal Noise does). The shape's vertex
buffer is written per frame from the bake (positions interpolated,
normals recomputed on the CPU); everything else is the mesh pipeline, so
materials, textures (a picture on the flag), copies, shadows and
depth of field just work.

Closing: warm-up plus blend the tail; a flag in a periodic wind closes
with a seam under a millimetre.

**Test.** Loops; pinned vertices never move; edge lengths stay within
stretch; cloth never goes under the floor. Preset: *Banners* (a row of
flags in a gusting wind, a logo printed on each).

**Done, with measured seams.** `ez_core::sim::Cloth`: a Flag, Curtain,
Banner or *Drape over a ball*, as `MeshSource::Cloth` (the shape picker
has a Cloth tile). XPBD with 6 substeps of one constraint pass each (the
"small steps" way): stiff structural links, nearly stiff shear, bending
over every other particle with compliance 10⁻²…10⁻⁶ from *Stiffness*
(computed with `sim::math`, since `powf` is the platform's). Up to 48
particles across. What the first tries taught:
- **Wind along a flat flag does nothing.** The sheet started in the plane
  the wind blows along, so pressure across it was zero and the flag hung
  perfectly flat, perfectly symmetric. Gusts are now a turbulent vector
  (strength plus sideways swirl, from value noise travelling a circle in
  noise space, so it loops), which is what starts a flag flapping.
- **Air grows with the square of the speed**, across the sheet and
  (standing for the drag of the flapping, which is what streams a flag
  out) along it; with linear forces the flag drooped at any wind.
- **A drape needs grip.** Without friction the sheet slid off its ball
  and wandered over the floor; contacts now undo most of the sliding.
Colliders: the ball and the floor. Not done: the terrain and other layers'
shapes as colliders, self-collision.
Seams, measured without the guided tail (4 s loop): a flag in a steady
rhythm of wind is exactly periodic after warm-up (0); with the default
gusts it settles to 1 cm RMS after 3 warm-up loops, a curtain to 6 cm, a
fine (48-across) flag to 4–7 cm, and more warm-up hardly helps once the
flapping is turbulent. The guided tail (pinned particles held in place)
closes what is left. Bakes: about 1 s at the default detail, 5 s at 48
across (4-core test container), 3–13 MB.
Drawing: the renderer samples the bake (a cross-faded cloth mixes its two
halves by weight), builds the sheet (normals from neighbours, texture
coordinates over the whole sheet, the border as the outline for edge
glows) and rewrites one vertex buffer in place each frame; before the
first bake the cloth lies at rest. The mesh shader already draws both
sides and turns normals to the viewer, so the sheet is single. Copies,
materials, shadows, reflections and depth of field work unchanged;
subdivision is skipped (the sheet is as fine as its detail). `link_sims`
now shares one bake path between flocks and cloth.
Tested: loops with the pole fixed, links stretch under 15% for every
kind, a drape rests on its ball above the floor with the corners hanging,
the flag streams downwind and follows a turned wind, gusts loop, the same
bake on every platform (checked natively and in wasm), saving and
loading; on the GPU, the flags leave their rest pose, wave more than the
scene moves, a waiting renderer and a polling preview agree, and the loop
closes (seam 0.0000). Preset: *Banners* (five flags on poles, a gust on
every bar, the wind swinging ±35°).

### ☑ 10.4 Rigid bodies
**How.** A copy layout, **Physics** (`Instancer::Physics`): each copy is
a sphere, box or capsule fitted to the shape's bounds (convex hulls
later). Sequential-impulse solver with friction and restitution,
sleeping for resting bodies, a grid broad phase, the floor and terrain
as ground. Scenarios, all on a loop-periodic schedule:
- **Rain**: a whole number of drops per loop; each body lives a set
  number of beats and sinks into the ground or shrinks away.
- **Tower / wall**: copies start in a grid layout and a hit (a kick, or
  set beats) knocks them down; ping-pong closing rebuilds them.
- **Explode**: copies start in any layout and burst outward on a beat.

Closing: blend the tail (Rain), ping-pong (Tower, Explode).

**Test.** Loops; resting bodies don't jitter (sub-millimetre movement
over a second); no body falls through the ground; a wall is intact at
phase 0 in ping-pong. Preset: *Beat Demolition*.

**Done, differently in places.** `Instancer::Physics` over
`ez_core::sim::Physics`, extended position-based rigid bodies (Müller et
al. 2020): 8 substeps per step (960 Hz), positional contacts with static
friction, then a velocity pass for bounce and sliding friction.
Colliders are boxes (sized from the layer's scale and stretch: the
built-in cube is ±0.5) or balls; box contacts test each box's 8 corners
and 6 face centres against the other box (edge-on-edge hits are missed;
resting and stacking work), broad phase by sweep and prune (sorted by
edge then number, the same everywhere). Only the floor; not the terrain,
and no convex hulls. *Explode* is Stack and blast with gravity 0. What
the measurements changed:
- **Sleeping, from the start.** A single box on the floor crept 1.6 mm/s
  and a column of four tipped over: resting contacts solved one after
  another leave a twist. Bodies nearly still for 0.25 s now sleep (fixed
  for the others, woken by something moving or a blast), and a stack
  starts asleep, so it stands exactly until the blast. Only a body held
  up at three points or more may sleep: one landed balanced on an edge
  and slept there.
- **Overlaps push apart at most 4 units/s.** A deep overlap corrected
  in one 1/960 s substep flung a body across the floor at 19 units/s.
- **The guided tail acts inside the substeps**, before the contacts (a
  new `Sim::guide` override, like the flock's), and not on the dead: the
  default kinematic pull dragged boxes into the floor (dips of 0.15), and
  snapping hidden bodies smeared them across the screen. Now the floor
  and neighbours push back, dips stay under 1 cm and the final blend
  covers 0.008 units RMS.
- **Rain: short lives, long tail.** Closing a loop of piled, tumbling
  boxes had some box slide across the floor at 20–70 units/s to where it
  lay a loop earlier. A body dropped inside the tail starts exactly like
  one loop earlier; with lives shorter than half the tail (default: 4
  beats of 16, tail half the loop), bodies dropped earlier are gone by the
  end, and the fastest slide while closing is 5.9 units/s against 3.4 in
  the rest of the loop.
- **Friction 1.0 by default.** Friction on a contact point also turns the
  box, so boxes slowed at about 40% of the rate the setting suggests.
Bakes (4-core test container, 8 s loop): rain of 60 in 0.5 s (1.9 MB),
200 in 1.5 s, a wall of 48 in 0.3 s, a tower of 400 in 3.7 s. Up to 400
bodies.
Tested: a resting wall stays put, a blast knocks most of it down and
ping-pong rebuilds it (intact at 0, mirrored halves), nothing falls
through the floor (boxes and balls), rain drops, lands and piles, loops
and slides back no faster than about twice its own motion, sinking
bodies go through the floor, the same bake on every platform (checked
natively and in wasm), saving and loading; on the GPU nothing is drawn
until baked (and the frame says so), a waiting renderer and a polling
preview agree, the wall falls and the loop closes (seam 0.0000).
Preset: *Beat Demolition* (a wall of 48 glowing blocks blown apart on
beat 2 and rebuilt, pearls raining behind).

### ☐ 10.5 Fluid-like particles
**How.** A new motion for the particles layer, `Motion::Fluid` (the
formula particles stay the default): position-based fluids (density
constraint, XSPH viscosity, a little surface tension), in a container
(box, bowl, the floor) that rocks or spins whole cycles per loop, poured
from an emitter on a schedule, or stirred by the music. Positions go to
the particle pass through an instance buffer, so every particle look
(glow, sprites, smoke, colour by speed) applies. Trails reuse the logo
echoes' idea: ghost copies are the bake read at earlier phases, exact and
free of history.

Optional look, **Liquid surface**: particles splatted as spheres into a
half-resolution depth and thickness target, a depth-aware blur, normals
from the smoothed depth, shaded with Phase 11's environment light and a
refraction of the scene behind (the glass logo's backdrop copy). With
cross-fade halves, both copies splat into the same thickness weighted by
their fade, which is the clouds' field blend applied to a liquid.

Closing: cross-fade halves.

**Test.** Loops; the particle count is constant; no particle leaves the
container; density stays within 10 % of rest after settling. Preset:
*Liquid Gold* (a rocking bowl of molten metal under an HDRI).

---

## Phase 11 — Rendering step up

Physically based lighting from real environments, reflections of the
objects themselves, and shafts of sunlight through the fog. Every piece
is off in old projects (golden images unchanged), and on in new presets
where it helps. Nothing here depends on history, so loop safety is free.

### ☑ 11.1 Image-based lighting from HDRIs
**How.**
- *Light & fog → Environment light*: **Colours** (today's `env_color`,
  the default), **HDRI** (an imported `.hdr` Radiance file; `image`'s
  `hdr` feature, pure Rust, works on the web) or **Built-in studios**
  (a few environments generated at start-up like the texture pack:
  softbox studio, overcast, sunset, neon room; no downloaded files).
  Also **From the sky**: the scene's own background rendered into the
  cube, so clouds and sunsets light the objects.
- Prefiltering on import (and when a built-in or the sky changes):
  equirectangular → 256² RGBA16F cube map, specular mips prefiltered with
  GGX importance sampling (one render pass per face and mip: fragment
  shaders only, so WebGL2 works), diffuse light as 9 spherical-harmonic
  coefficients computed on the CPU into the globals, and a split-sum
  BRDF table (RG16F, 64², made once at start-up).
- Settings: rotation (animatable, whole turns per loop), intensity,
  exposure, show as background (a new backdrop kind that draws the HDRI,
  with blur), and **sun from the HDRI** (find the brightest spot, point
  the sun and its shadows there, remove it from the map so it isn't
  counted twice).
- Bindings: the cube, its sampler and the BRDF table join the shadow
  map in group 3; no new groups (WebGL2 has four).
- Storage: the `.hdr` is an asset like any image (relative path, copied
  into `.ez2pack`).

**Cost.** From the sky re-renders six 128² faces and their mips each
frame; a *static* option renders once (the sky kinds that don't move).

**Test.** A white rough sphere under a uniform HDRI matches the uniform
colour (energy check); rotating a whole turn is identical; a mirror
sphere shows the map; Colours mode is byte-identical to today.

**Done.** *Light & fog → Environment light* offers Colours (default,
unchanged), four built-in studios (softbox, overcast, sunset, neon room,
generated as equirect panoramas), a panorama photo (`.hdr`, picked or
dropped on the window, stored as an asset) and From the sky. Settings:
Strength, Turn (animatable degrees), Sun from the map, and Capture once
for the sky. A new **Environment map** backdrop draws the map, turned
with the light, with a Sharpness slider (panorama mip level).
- *Prefiltering is on the CPU*, not a render pass per face and mip: it
  is simple, identical on every backend and fast enough. Mip 0 is a
  direct 256² sample; mips 1–5 are GGX lobes (16–64 precomputed
  samples, Karis-style source LOD) read from a cube pyramid whose
  downsampling weights rows by sin θ (without that, rough mips lost up
  to 11% of their energy; now under 2.3%). Faces run on threads
  (except wasm): ~220 ms in release on the 4-core container, once per
  map change. Diffuse is 9 SH coefficients in the globals; the
  split-sum BRDF table is 32² RG in an RGBA16F texture, made at
  start-up. Cube, sampler and table joined the shadow map in group 3.
- *Sun from the map* finds the brightest region, turns it into the sun
  (direction turned with the map, colour and strength from its energy),
  and removes it from the map before prefiltering, so it isn't counted
  twice.
- *From the sky* renders the scene's background into six 64² faces each
  frame (once with Capture once), then a cone-blur shader
  (`env_filter.wgsl`, fullscreen fragment passes, WebGL2-safe) makes the
  six mips; its diffuse light is the roughest mip rather than SH.
- Measured (`environment_maps_light_the_scene`): a white rough ball
  under a uniform map reads 195 against the sky's 198; a mirror ball
  shows red right, green left, blue in the middle of an axes map, and
  red in the middle turned −90°; a sky-gradient mirror ball is red on
  top and blue below, and the captured-once version matches; a whole
  turn is identical; Colours differs from any map. Golden images of the
  old presets are unchanged. New preset **Chrome Studio**: a chrome
  knot, gold ball and plastic block under the turning sunset with its
  sun's shadows.
- Limits: the environment light goes through `lit_surface` (shapes,
  models, raymarched shapes); terrain, water, the mirror floor and text
  chrome keep the classic light. The sky capture is taken from the
  camera's eye. Specular stays classic Blinn plus split-sum reflection
  until 11.2.

### ☑ 11.2 PBR materials
**How.**
- *Shading*: **Classic** (today's Blinn + `env_color`, the default for
  old projects) or **Physical**. Physical replaces `lit_surface`'s
  specular with GGX, Smith height-correlated visibility and Schlick
  Fresnel for the sun, spot and laser lights, and split-sum IBL for the
  environment (multiple-scattering compensation so rough metals don't
  darken). Weather (wet, puddles, snow) keeps working by changing the
  inputs before shading, as today.
- Material maps: albedo (the existing texture), normal (the existing
  relief), plus **roughness / metallic / occlusion** (one packed ORM
  image or separate ones) and an emissive map, all on the material's
  tiling and scrolling. glTF import reads its PBR textures and factors
  instead of just the base colour.
- New settings: clearcoat (car paint, lacquer), sheen (cloth), and
  transmission with thickness (glass, liquids; sampling the backdrop
  copy the glass logo already makes).
- A **Material presets** row: gold, copper, chrome, brushed steel,
  rubber, car paint, glass, velvet, ceramic.
- Built-in matcaps become a Physical option too, so logos (9.2) can use
  the HDRI instead of a matcap.

**Test.** A furnace test (rough white under uniform light keeps ≥ 95 %
energy); the golden images of every existing preset unchanged in
Classic; new golden images for the material presets.

**Done.** *Material → Shading*: Classic (default, byte-identical: the
golden images of every existing preset match to the same tiny noise as
before the change) or Physical (`physical_surface` in common.wgsl, used
by shapes, models and raymarched shapes).
- *Shading*: GGX, Smith height-correlated visibility and Schlick Fresnel
  for the sun; split-sum environment light with Fdez-Agüera multiple
  scattering (the same compensation scales the sun's highlight); works
  with an environment map, the captured sky, or the sky and ground
  colours (then `env_color` stands in for the prefiltered map). The
  weather (wet, puddles, snow) moved into a shared `weathered()` step
  that both shadings call. Spot lights and lasers are drawn as beams and
  don't light surfaces in this renderer, so only the sun gets GGX.
- *Layers*: clearcoat (its own GGX lobe and split-sum term, f0 0.04,
  dimming what is under it), sheen (Charlie distribution with Neubelt
  visibility for the sun, a grazing-angle approximation for the
  environment) and transmission ("Glass": the view refracted in through
  the surface and out through the back as if the shape were a ball
  there, so a glass ball shows the world upside down and a slab passes
  it straight; it sees the environment, not the other shapes, instead of
  the backdrop copy the plan named, which doesn't exist yet while
  meshes are drawn).
- *Data*: the mesh draw block was full, and meshes already use all four
  bind groups, so meshes and raymarched shapes now take two draw slots:
  a second dynamic uniform binding in group 1 reads the next slot
  (`D2`: shading, map flags, clearcoat, coat roughness, sheen,
  transmission, sheen colour, IOR). Other pipelines are unchanged.
- *Maps*: an ORM picture (glTF packing, uploaded without sRGB decoding)
  multiplies roughness and metalness and gives occlusion; a glow map
  multiplies the glow colour. Both use the colour texture's tiling,
  scrolling and triplanar projection, and are skipped when absent.
- *glTF import*: adding or swapping in a `.gltf`/`.glb` model reads its
  first material (base colour, metal, roughness, emissive with
  `KHR_materials_emissive_strength`, `KHR_materials_transmission`,
  `KHR_materials_ior`) and its pictures; occlusion and
  metal/roughness pictures are merged into one ORM picture. They are
  saved as PNGs in the app's data folder (in memory on the web) and
  added to Your images. Pictures that can't be read are skipped; the
  same fix lets a model whose texture files are missing still load (it
  used to fail outright on disk).
- *Material presets* row: gold, copper, chrome, brushed steel (rough
  metal; no anisotropy), rubber, car paint (clearcoat), glass, velvet
  (sheen), ceramic (light clearcoat). New preset **Material Gallery**
  shows all nine under the softbox studio.
- *Logos*: a built-in material sphere `matcap_environment` is made on
  the CPU (64², from a 64×32 copy of the map) as a mirror ball reflecting
  the environment light, seen through the current camera and turned
  with the map, and remade only when those change; without a map it is
  the chrome sphere.
- Measured (`physical_materials_keep_energy`): the furnace test gives
  197.2 on the ball against 197.0 for the sky for rough and half-rough
  white plastic and metal (the ≥ 95 % target; multiple scattering
  makes it exact); a clearcoat raises black paint from 18.8 to 32.1; a
  glass ball shows the yellow behind it ([235, 237, 32]) where a mirror
  shows the blue in front; sheen lifts dark cloth's edge from 29 to 62;
  an ORM map with no roughness turns a rough metal ball's middle from
  the map's average (yellow) to the blue spot straight ahead; a black
  glow map keeps a glow off (0) and a white one lets it through (252);
  a logo's environment sphere is blue facing the map's blue side and
  yellow turned half a turn. glTF material reading has a unit test
  (factors, a merged ORM picture, a missing picture skipped).

### ☑ 11.3 Screen-space reflections on objects
The mirror floor keeps its planar reflection (exact, cheaper). Shiny
objects, terrain water and wet ground get screen-space reflections.

**How.**
- The half-resolution depth-of-field pass (7.2), which already redraws
  solid meshes, terrain and the floor, gains a second target: normal
  (octahedral, RG) and roughness plus reflect strength (BA), RGBA16F.
  It runs when depth of field *or* reflections need it.
- After the scene resolves, a reflection pass marches the reflected ray
  through the distance texture in screen space (hierarchical steps,
  32 at most, binary refinement at the hit, a fixed per-pixel Bayer
  jitter instead of temporal noise, since there is no history to
  accumulate into), and reads the resolved scene at the hit, blurred by
  roughness (a small mip chain of the scene).
- On a hit, the pass **replaces** the environment term the surface
  already has: it recomputes `env` for that pixel (a pure function of
  the direction, colours or HDRI) and adds `weight × (hit − env)`, so
  nothing is counted twice. Misses and hits near the screen edge fade
  back to the environment.
- Settings in *Light & fog → Reflections*: on/off, strength, maximum
  distance, roughness cut-off.

**Cost.** One half-resolution full-screen pass plus the (shared) G-buffer
pass. Skipped when no visible material reflects.

**Test.** A chrome sphere next to a red box shows red on its side, which
disappears when the box is hidden; the reflection pass off is
byte-identical; loops.

**Done.** *Light & fog → Reflections*: on/off, strength (animatable),
reach and a roughness cut-off. On in Material Gallery and Chrome Studio.
- *G-buffer*: the half-resolution distance pass now writes four targets:
  the distance (R16F, unchanged for depth of field), and RGBA16F normal
  (octahedral) + roughness, reflectance `k` and the reflected
  environment `e` the surface already shows (both dimmed by its fog).
  It runs for depth of field *or* reflections. Meshes share one
  `surface()` function between the colour and distance passes (the same
  maths, so Classic stays identical), and `classic_mirror` /
  `physical_mirror` reproduce the reflection terms of the two shadings;
  the mesh and SDF distance pipelines moved to the lit layout for the
  split-sum table.
- *Instead of recomputing the environment* in the reflection pass (as
  planned), each surface stores the `e` it added, so the pass adds
  `w × (k × hit − e)` and the same code serves shapes, raymarched shapes,
  terrain water, goo, ice and rain puddles, whose sky reflections are
  their own formulas. The floor writes nothing: its planar reflection is
  exact.
- *March*: 32 steps along the mirror ray in world space (spacing grows
  quadratically to the reach), a 4×4 Bayer jitter, a thickness of
  0.3 + 0.25 × distance, marching on behind thicker things, then 5
  bisection steps. Weight fades at the screen edge, towards the reach
  and the roughness cut-off, and on backfaces; rough hits average a
  small disc of taps instead of a mip chain.
- *Composite*: added onto the scene before depth of field with a 5×5
  blur weighted by distance difference, which removes the jitter
  pattern without bleeding across silhouettes (half-resolution edges
  can still step a little).
- Measured (`screen_space_reflections_show_neighbours`): the red
  measure on a chrome ball's side facing a glowing red box goes from 0.0
  (off) to 26.6 (on) and back to 0.0 with the box hidden; with
  reflections off (and depth of field sharing the pass) the picture is
  byte-identical to the default; the first and last frames of an
  orbiting camera match. Golden images of all presets without
  reflections are unchanged; Material Gallery and Chrome Studio were
  re-blessed with them on.
- Limits: only what is on the screen can be reflected, so reflections
  fade at the edges and cannot show the back of anything; the mirror
  floor's planar reflection doesn't contain reflections of reflections.

### ☑ 11.4 Light shafts through fog
Today's god rays are a screen-space blur from the sun's position, so they
vanish when the sun is off screen. Real shafts come from the fog being
lit where the sun reaches it.

**How.**
- A half-resolution pass after the scene: for each pixel, march from the
  camera to the scene's distance (the same distance texture) in 24–48
  steps, each sampling the **sun shadow map (1.1)** and the fog density
  (distance fog plus the height fog, as `fog_amount_at` integrates it),
  and accumulate in-scattered sunlight with a Henyey-Greenstein phase
  (forward scattering setting) and the transmittance so far. Beyond the
  shadow map's reach the fog counts as lit.
- Fixed per-pixel jitter on the start (Bayer, stable across frames) and
  a depth-aware 4 × 4 blur, then a depth-aware upsample added over the
  scene before bloom. Spot lights from *Club Spotlights* can join later
  (they have no shadow maps).
- Settings in *Light & fog → Light shafts*: strength, scattering
  (forward), steps (quality). Requires shadows; turning shafts on turns
  shadows on. Shafts follow the day cycle, shadow-casting copies and
  terrain, and add to the existing god rays rather than replacing them.

**Cost.** Half resolution, ~0.5 ms at 1080p on a desktop GPU; quarter
resolution on phones by default.

**Test.** Shafts appear only in lit fog (a wall's shadow cuts a dark
band through them); no fog means no shafts; loops; off is byte-identical.
Preset: *Cathedral Light* (shafts through a colonnade, a PBR marble floor,
an HDRI sky).

**Done.** *Light & fog → Light shafts*: strength (animatable),
scattering, reach and quality (steps); turning them on in the app
turns sun shadows on.
- *Pass* (`shafts.wgsl`, half resolution, after the scene and the
  reflections, before depth of field and bloom): each pixel marches
  from the camera to the distance pass's distance (capped by the reach)
  in 32 steps by default, each reading the fog density (distance fog
  plus the height fog's exponential falloff, the density
  `fog_amount_at` integrates) and one shadow-map comparison, and adds
  density × transmittance so far × sunlight. The sun colour and
  strength come from the globals, so shafts follow the day cycle (and
  the moon at night). Henyey-Greenstein phase normalised so that
  scattering 0 gives 1; the fog scatters a fifth of the sunlight at
  strength 1 (without that factor, typical fog glowed white all over).
  Beyond the shadow map the fog counts as lit.
- It shares the distance pass with depth of field and reflections, and
  the reflections' distance-aware 5×5 blur for the composite, which
  hides the Bayer jitter; the fog colour already in the picture stays,
  so the shafts are the sun's extra light on top. Skipped with no fog
  or mist.
- Measured (`light_shafts_follow_the_sun_shadows`): under a roof over
  the left half of the view with the sun overhead, a band of the
  picture brightens from 59.6 to 161.1 on the lit right and only to
  65.2 on the shadowed left; with no fog the picture is byte-identical
  to shafts off; the first and last frames of an orbiting camera
  match. Golden images of every other preset are unchanged.
- New preset **Cathedral Light**: two rows of physical stone columns on
  a marble mirror floor, the sunset studio as sky and light, a low sun
  ahead of the camera streaming between the columns, and a gold orb.
- Not done: spot lights joining the shafts (they have no shadow maps),
  and a quarter-resolution default on phones (the preview's resolution
  setting already halves everything).

---

## Order of work

0.1 → 1.1 → 1.2 → 2.1 → 2.3 → 2.4 → 2.5 → 2.2 → 2.6 → 3.1 → 3.2 → 3.3 →
4.1 → 4.2 → 5.1 → 5.2 → 6.1 → 6.2 → 6.3 → 7.1 → 7.2 → 7.3 → 8.1 → 8.2 →
8.3 → 8.4 → 9.1 → 9.2 → 9.3 → 9.4 → 9.5 → 9.6 → 10.1 → 10.2 → 10.3 →
11.1 → 11.2 → 10.4 → 10.5 → 11.3 → 11.4. (Environment light and PBR
come before rigid bodies and fluids, so the fluid's liquid surface and
the physics presets are shaded by them.)

Each item lands as its own commit with tests (loop seams, golden images
where the look is meant to stay, new goldens for new presets), README and
manual updates, and a check in the running editor. Every push builds the
Android preview APK.
