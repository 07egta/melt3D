# MELTCAD performance notes

Goal: light on the machine. Dropping effects while moving is fine. Nothing here could be run on a GPU
by the author of the change (no display in the build box), so the effects below are reasoned
estimates, marked **(est.)**. Please check them once on a real machine (`MELTCAD_TIMING=1`).

## What was wrong

1. **The window never went idle.** `App::ui` sent `ViewportCommand::Title` on every frame, and
   `send_viewport_cmd` itself calls `request_repaint()`. So eframe drew a new frame as fast as vsync,
   forever, and each frame re-ran the full ray march (about 100 map() calls per pixel). This was
   the main reason the app felt heavy even when nothing moved.
2. Any repaint (mouse over a panel, a tooltip) re-marched the whole viewport.
3. Every map() call mixed 9 material values per shape, even when only the distance was wanted
   (marching, shadows, AO, normals).
4. The march hit threshold (0.004 mm) was about 30x finer than a pixel, so rays crawled along
   surfaces for many iterations.
5. The selection outline evaluated the selected shapes at every march step.
6. Start-up: the CJK font file (several MB) was read before the window, and the shader was compiled
   before the first frame.

## What was done

| # | Change | Where | Effect |
|---|---|---|---|
| 1 | Title is sent only when it changes. Status message wakes itself at expiry. A "stale picture" check asks for exactly one more frame (max 3 in a row) when something changed after the viewport drew. | `app.rs`, `perf::Drawn` | idle CPU/GPU about 0 % (was 100 % of vsync) |
| 2 | **Frame cache.** The scene is rendered into an offscreen texture only when the packed uniform (bitwise), the render size or the quality changed. Otherwise the callback only blits that texture (1 triangle). | `gpu.rs`, `perf::FrameCache`, `blit.wgsl` | UI-only repaints cost about 0.05 ms GPU instead of a full march |
| 3 | **Render scale / quality** (top bar, "がしつ"): じどう (default: 1x still, 0.5x moving), かるい 0.5x, ふつう 0.75x, きれい 1x (0.85x moving). Works because the picture is an offscreen texture of any size, scaled in the blit (linear filter; nearest when sizes match). | `perf::Quality`, `view.rs`, `gpu.rs` | moving: 4x fewer pixels in auto **(est.)** |
| 4 | **Fast mode** (flag bit 2 in `G.view.w`) while the camera glides/moves, a handle or a slider is dragged, and 150 ms after. Shader: no AO, 10-step shadow, no glass interior march, no halo, no translucency probe, 3-tap normals, 1 tap ground patch, over-relaxation 1.5, 48 iterations max, 0.5 px hit epsilon. A full frame follows 150 ms after the last change. | `shader.wgsl` | moving frame about 5-8x cheaper per pixel **(est.)** |
| 5 | `map_d()`: distance-only map, no material mixing. Marching, shadows, AO and normals use it. The full `map()` runs once at the hit. Lamps (flag bit 4) still march with `map()` for the halo. | `shader.wgsl` | about 1.5-2x on every march/shadow/AO step **(est.)** |
| 6 | **Bounding-sphere skip** per shape (`G.brad[]`, from `perf::shape_radius`): a shape farther than blend-reach beyond the current group value is not evaluated. Exact (see test `a_far_shape_does_not_change_a_blend`), per blend mode (smooth 1k, round max(1.3k, a+1.3k), gooey 6k). Stain shapes are skipped unless near. | `shader.wgsl`, `gpu.rs` | big for scenes with scattered shapes; none for one tight blob |
| 7 | Hit epsilon scales with pixel size (`max(0.004*(1+0.002t), 0.2 px)`), over-relaxed sphere tracing (1.1x, taken back when two steps would skip a surface), 128 iteration cap (200 with lamps). | `shader.wgsl` | about 1.5-2x fewer iterations **(est.)**; image differs by less than a quarter pixel |
| 8 | Selection outline evaluates the selected shapes only inside their bounding sphere (`G.selsph`); exact. | `shader.wgsl` | removes the extra shape evaluations per step far from the selection |
| 9 | Soft shadow: marches only inside the bounding sphere (inflated for the penumbra), 32 steps (was 40). Ground AO and shadow skip points far from the scene. | `shader.wgsl` | floor pixels away from the blob cost almost nothing |
| 10 | Hover pick: only when the mouse moved (or camera/scene changed), at most 30 times a second; the next look is scheduled with `request_repaint_after`. The CPU `Field` was already cached by scene equality (`App::settle`). | `view.rs`, `perf::HoverGate` | CPU raymarch per frame avoided |
| 11 | Start-up: CJK font search and read run on a thread started at the top of `main` (joined when the window exists); the shader/pipeline compile runs on its own thread, the window opens first and the viewport fills in when it is ready. `MELTCAD_TIMING=1` prints phase times to stderr. | `main.rs`, `fonts.rs`, `gpu::install`, `perf::mark` | window shows after the graphics device is up, not after the shader compiles **(est. 100-1000 ms earlier, driver dependent)** |
| 12 | `[profile.release.package.meltcad] codegen-units = 1` in the workspace `Cargo.toml` (only this crate). | `Cargo.toml` | small, CPU picking/export loops |

Also fixed on the way: a carving shape's id was inverted in `combine()` (the whole body lit up when
a cutter was hovered).

## Behaviour that changed on purpose

* While moving: no ambient occlusion, rougher shadow, no refraction in glass, no lamp halo. A full-quality frame appears 150 ms after the last move.
* Hit threshold and step length changed; full-quality images differ from before by well under a pixel.
* A scene with lamps still marches with materials (needed for the halo) in full quality.

## Checking it

* `MELTCAD_TIMING=1 cargo run --release -p meltcad` prints lines like `[meltcad]   412.3 ms  gpu ready`.
* Idle check: leave the window alone; the process should sit at about 0 % CPU/GPU. Move the mouse over the side panel: GPU only blits.
* Quality menu: "がしつ" next to "書き出し".
* Compare a still picture with the old build at 1x ("きれい"): edges, shadows and glass should match.

## Recommendations (not done)

* Release profile: for a shipping build, a separate profile with `lto = "fat"`, `codegen-units = 1`, `strip = true` would shrink and speed it slightly, but it changes every crate, so it was left alone. `panic = "abort"` must not be used: the main SolidCraft app relies on `catch_unwind`.
* wgpu backends: restricting to `Backends::PRIMARY` (Vulkan/DX12/Metal) skips the GL probe at start-up (tens of ms), but drops machines that only have GL.
* If the viewport is still too slow on weak GPUs: lower the default max iterations, or render the still frame in two passes (a low-res march giving a start distance per pixel). Not needed so far.
* Material preview swatches in the side panel are tessellated by egui every frame the panel repaints; caching them as one texture would save CPU on the Material tab.


## Big scenes (512 shapes, 96 groups, 256 instances)

The scene no longer lives in the 64 KB uniform buffer. `gpu.rs` packs it into **one long list of
vec4 in a float texture** (`Rgba32Float`, 1024 vec4 per row, read with `textureLoad`); only the
camera, light and the list's offsets (`Globals`, 23 vec4) are uniforms. The list is exactly as long as
the scene needs (a default scene is 14 vec4, a full scene about 50 000), and it is uploaded only when
the packed frame changed (the frame cache compares the whole frame). A texture was chosen over a
storage buffer because egui-wgpu asks for `downlevel_webgl2_defaults` limits on OpenGL (no storage
buffers, 16 KB uniforms), and the first thing a fragment shader can always read is a texture. The
list: groups (3 vec4), blocks (2), instances (4), shapes (16), outline points, path nodes.
`gpu::headless::open(true)` (`YURA_GL_LIMITS=1`) requests those OpenGL limits for the headless tests.

To keep a frame bounded by what is near the ray (`accel.rs`, same numbers on the CPU `Field`):
* shapes are cut into **blocks of 8** with a bounding sphere; a block that is farther than
  `rm * kmax + |g| + 0.02` from the point is skipped whole;
* every **group** has a sphere (its adds and overlaps plus the most their blends can pull the
  distance down: `pull(mode, k)`); a group (or one instance of it) farther than `far_enough(...)` is skipped;
* overlap shapes (`Op::Intersect`) are never skipped, and `map_d()` now treats them as overlaps (it used
  to measure them as unions, so marching and the picture disagreed).
`sdf::tests::skipping_far_shapes_blocks_and_groups_never_changes_the_answer` proves the skips are exact
(error 7e-5 at most, all three blend modes). The CPU `Field` uses the same skips (picking and the mesher got faster too).

Measured with the software Vulkan driver (llvmpipe, 4 cores) through `gpu_headless_bench`
(`cargo test -p meltcad --release gpu_headless_bench -- --ignored --nocapture`), 480 x 300 pixels, a
scene of library models on a grid, one still frame after the shader was compiled:

| shapes | groups | full quality | fast mode |
|---|---|---|---|
| 17 | 1 | 102 ms | 70 ms |
| 106 | 8 | 664 ms | 232 ms |
| 305 | 26 | 1556 ms | 673 ms |
| 502 | 44 | 1962 ms | 1012 ms |
| 17, as a grid of 250 live instances (a 4250-shape scene's worth) | 1 | 2979 ms | 1096 ms |

(a real GPU is far faster; the point is the growth: 18 times the shapes cost 15 times the time, and
the bigger scenes also fill more of the picture; the instanced scene costs about 1.5 times the 502-shape scene). Compiling the shader on llvmpipe takes minutes
(the driver inlines every call site); `GALLIVM_PERF=no_opt` makes it about 3 times faster for tests.

Instances (patterns and linked copies) are evaluated by carrying the point back into the group's own
space: three rows of a matrix per instance in the list, the group's sphere moved with it for the skip.
256 instances in the scene at most; each melts into the result in turn with the group's op and blend.

The CPU `Field` (picking, mesher, `--render`) uses the same skips: 20 000 distances in a 305-shape scene
take 37 ms instead of 245 ms (6.6 times faster); `meltcad --render big300.meltcad out.png 200` (four
views of 200 x 200 pixels, flat shaded, 4 threads) takes 0.7 s.

The real window was also run under xvfb on the OpenGL backend (`WGPU_BACKEND=gl`, llvmpipe through EGL,
with the `downlevel_webgl2_defaults` limits egui-wgpu asks for there): the old 40 KB uniform could not
have been bound under those limits, the scene texture can. On OpenGL the shader is now compiled on the
main thread (a compile on a second thread holds the GL context lock, and egui's texture upload on the
main thread then aborts with "Could not lock adapter context").


# Second round: "lags, make it silky" (adaptive resolution, banded sharp picture, shader work)

A screen recording from a Windows PC (Intel Core Ultra 258V, integrated Arc 140V, window about
2528 x 1440 physical pixels, 120 Hz display) showed 30 fps (median 33 ms between pictures, the slowest
tenth 67 ms or more, hitches of 200 to 730 ms) while a single pipe (a `Kind::Curve`, about 12 nodes) was
orbited and dragged. Nothing here could be run on that GPU. The numbers below come from the software
Vulkan driver (llvmpipe, 4 cores) and are **a proxy**: llvmpipe is bound by shader arithmetic and
divergence, an integrated GPU by arithmetic, divergence **and the latency of texture reads** (the scene is
read with `textureLoad`). So besides timings the change reports *reads of the scene list per pixel* (they do
not depend on the machine) and ships instruments that the person can run on their own PC.

## What was wrong (reasoned from the code and from the counters below)

1. **The pipe distance looped over all of the (up to 96) points** of the smoothed curve at every march
   step of every pixel whose ray was inside the pipe's box: 89 round cones per evaluation.
2. **The sharp picture was one single submission.** With "じどう", the frame drawn 150 ms after the last
   move was a full-quality picture of the whole window: floor shadow (32 steps) and occlusion for every
   floor pixel, 4-tap normals, glass... On a weak GPU that is hundreds of milliseconds in one piece, during
   which the window cannot present or take input. That is the 200 to 730 ms hitch.
3. **Moving frames had a fixed size** (half the window in each direction) whatever the GPU managed.
4. Every scene read was a texture read from the long list: for a library model in a 100-shape scene about
   1100 reads per pixel while moving, 250 reads per distance evaluation.
5. Rays that cannot hit anything marched anyway (every pixel inside the bounding sphere, floor pixels
   marched through the floor to the far side of the sphere).
6. CPU side: the whole scene was compared three times per frame and cloned once (`settle`, `Drawn`), packed
   completely for every frame (also when only the camera moved), and the picking field was rebuilt after every
   change. Together this is 1 to 3 ms in a 300-shape scene (measured, see below), so it was *not* the cause of the
   hitches, but it is gone. The library card drew one thumbnail per frame on the main thread (a few ms each);
   they are drawn on a thread now.

## What was done

| Part | What | Where |
|---|---|---|
| Frame-time HUD (F3, または「がしつ」→「フレーム時間を表示」) | CPU time of the window code and the interval between frames (rolling second: average and worst; worst of the last 10 s), GPU time of the scene pass (timestamp queries, resolved and read back asynchronously a few frames late, never waited for), render scale, shapes / groups / instances, adapter and backend. | `hud.rs`, `gpu_timing.rs`, `view.rs` |
| Speed test (`meltcad --bench [out.txt] [--quick] [--frames N]`, or 「がしつ」→「速さをはかる…」) | Draws five standard scenes (pipe like the report, library model of about 20 shapes, 100 shapes, 300 shapes, a gradient / rounded / glass scene) at 1280x720, 1920x1080, 2560x1440, full quality and in fast mode, in a texture of its own with the window's pipelines, on the backend the window uses. Time of each frame: GPU timestamps, else wall clock around submit + wait. Writes `meltcad-bench.txt` next to the program (or the temp folder), prints the table, shows it (window: copy button). Also prints the resolution the adaptive scale would settle at. | `bench.rs`, `gpu::offscreen` |
| Adaptive resolution (「じどう」) | While things move, `ScaleController` picks the render scale between 0.25 and 1 so that the scene pass costs about 10 ms of GPU time (from timestamps; without them: 60 fps from the frame interval, ignoring frames slow because of the processor). Down at once (at most a quarter per measurement), up by 3 % per measurement and only after a wait that triples when a drop follows a rise soon after, so it does not swing. The pictures live in textures as big as the window and only a corner of them is drawn, so changing the scale never makes a new texture. | `adapt.rs`, `gpu.rs` |
| Banded sharp picture | The sharp picture is drawn a band of rows at a time (`BandPacer`: as many rows as fit in about 6 ms, measured), into its own texture. The blit shows the cheap picture and, above the finished row, the sharp one: it sharpens from the top, no frame is longer than a band. A new move abandons the bands. | `gpu.rs`, `blit.wgsl`, `adapt.rs` |
| Upscale | Bilinear read plus a light contrast-adaptive sharpen (5 taps, only where the picture is smaller than the window; weaker for tiny pictures). | `blit.wgsl` |
| Pipe distance | The tube's segments are grouped by 8; each group has a bounding sphere (made on the CPU, `curve::group_spheres`). A point looks at the (up to 12) spheres first: far from all of them it answers with a lower bound, else only the groups that can hold something nearer than what is known are measured. Exact (test `the_grouped_search_equals_the_plain_loop`). | `shader_curve.wgsl`, `curve.rs`, `gpu.rs` |
| Candidates per ray | Once per pixel, before the primary march, the shader finds the groups / instances / shapes whose bounding sphere (grown by the reach of their blend) the ray comes near, in the order they melt, and the march (`map_d_cand`) reads only those (24 at most; more: bits per group instead). A pipe is also tested against the spheres of its segment groups. If nothing is near the ray, it is not marched at all. Shadow rays do the same (a shape farther than `tend / 10` from the ray cannot darken it). Switched off when a group or a shape keeps only an overlap (then what is far still decides where a ray stops) and for lamps. | `shader.wgsl` (`find_candidates`, `map_d_cand`), `gpu.rs` |
| Floor | The march ends at the floor (the last step is taken exactly to it, so that an over-relaxed step cannot jump over a surface in front of it); the floor tap in fast mode is skipped far from the scene. | `shader.wgsl` |
| Fast mode | 4 shadow steps (was 10); `relax` 1.3 instead of 1.5 when candidates are used (longer steps than the full scan, see below). | `shader.wgsl` |
| One call site | The key light's shadow and the occlusion are found once, in one place, for what is seen (floor or surface); `cargo test inlining_cost_report` still passes (shape_dist 15 copies, 141 000 expressions). | `shader.wgsl` |
| CPU | A scene revision number (`App::track_scene`: one comparison per call, a clone only on change) replaces whole-scene comparisons and clones; the scene is packed once per revision (`ScenePack`: the camera moves repack nothing), the list is uploaded row-wise only where it differs; the picking field is built when picking asks and the scene has changed. Library thumbnails come from a worker thread. | `app.rs`, `perf.rs`, `gpu.rs`, `library_ui.rs` |

## Measured (llvmpipe, 4 cores, 640 x 360, before and after; **proxy only**)

`cargo test -p meltcad --release bench_on_this_machine -- --ignored --nocapture` with
`MELTCAD_BENCH_SIZES=640x360 MELTCAD_BENCH_FRAMES=3` (before: `MELTCAD_SHADER_DIR=<the old shader files>`).
GPU timestamp of the scene pass, average of 3 frames; run to run the full numbers vary by about 15 %.

| scene (shapes) | full before | full after | fast (moving) before | fast after | fast speed-up |
|---|---|---|---|---|---|
| pipe, 12 nodes (1) | 93 - 95 ms | 36 - 42 ms | 47 - 49 ms | 18 - 22 ms | 2.4 x |
| library model (35) | 267 - 279 ms | 161 - 223 ms | 89 - 90 ms | 20 - 23 ms | 4.2 x |
| library models (106) | 642 - 694 ms | 355 - 407 ms | 236 - 261 ms | 40 ms | 6.2 x |
| library models (305) | 1420 - 1436 ms | 815 - 871 ms | 580 - 629 ms | 83 - 84 ms | 7.2 x |
| gradient / rounded / glass (4) | 60 - 66 ms | 47 ms | 40 ms | 18 - 22 ms | 2 x |

(Full quality is not what a moving window draws; it is what the sharp picture costs, now spread over bands.
The library scenes contain glowing parts, which keep the full-quality march on the slow path: see "Not done".)

Reads of the scene list per pixel (`count_work`, 320 x 180, average over the picture, fast mode; before -> after):
pipe 121 -> 45, library model 403 -> 86, 106 shapes 1090 -> 183, 305 shapes 2179 -> 387, gradient 182 -> 86.
Distance evaluations per pixel (fast): 2.4 -> 0.3 (pipe), 2.5 -> 0.3, 3.1 -> 0.4, 3.4 -> 0.4, 3.2 -> 0.6. (What is
left of the reads is mostly the once-per-pixel search for the shapes near the ray; rays that miss everything
pay only that.)

CPU per frame of the real window code (`frame_cpu_bench`, 2528 x 1440 pixels, pointer events through
`App::frame_body`): pipe alone 0.4 to 0.9 ms, with 100 shapes 1.0 to 1.3 ms, with 300 shapes 2.0 to 2.6 ms;
worst frame while dragging a node with 300 shapes 4.2 ms. No frame above 10 ms. The only large CPU item that
remained was the thumbnails (now off the main thread).

## What this should mean on the Arc 140V (estimates, **not verified**)

* Moving: 3 to 5 times fewer scene reads and evaluations, plus a cost-driven resolution: at 2560 x 1440 the
  controller takes whatever scale fits 10 ms; for the pipe that is expected to be near the full window or 0.75,
  for a 100-shape scene 0.5 to 0.75. The report's 33 ms median frame should become one frame per display
  refresh in the pipe case (60 to 120 fps).
* No stall when stopping: the sharp picture takes a few frames (1 to 12 depending on the scene) instead of one
  frame of hundreds of ms.
* What cannot be told without the GPU: whether texture reads or arithmetic dominate there (the counters say
  which got fewer), and how well the GPU compiler copes with the larger shader (about 7 % more expressions
  than before the work; llvmpipe compile of the whole pipeline about 90 s, as before).

## Checking it

* `meltcad --bench` (see README): the table is the thing to send. Rows with `wall ms` close to `avg ms` mean the
  timing is from the GPU (timestamps) or the card is the bottleneck.
* F3 while orbiting: `GPU(シーン)` should be near 10 ms or less, the scale (倍率) below 1 only while moving.
* `cargo test -p meltcad --release candidates_match_the_full_scan -- --ignored --nocapture` (also with
  `MELTCAD_BENCH_FAST=1`): every pixel's ray hits the same surface at the same distance with and without the
  per-ray candidates (to the march's own tolerance); `shadow_candidates_match_the_full_scan` the same for the
  picture with the shadows; `count_work` prints reads per pixel; `gpu_headless_golden` + `cmp` against an older
  build shows what changed in the picture (edges and thin paint patches differ by a fraction of a pixel,
  the selection outline is a little bolder while moving).
* Experiments: `MELTCAD_SHADER_SED="old=>new||old2=>new2"` (tests only) replaces text in the shader for one
  run; `MELTCAD_SHADER_DIR=dir` loads the six shader files from another folder (A/B against an old version).

## Things that changed on purpose

* Moving pictures are smaller when the GPU is slow (down to a quarter of the window in each direction); the
  upscale is sharpened.
* The selection outline is a little bolder while moving (the march takes shorter steps, so more samples fall
  next to the outline).
* The sharp picture appears from the top downwards instead of at once.
* Fast mode: shadows have 4 steps (long shadows of far objects are missing while moving).
* Where a march used to jump past an (invisible) part of a shape below the floor it now stops at the floor.

## Not done / ideas

* Lamps (a visible shape with glow) still march with full materials (`map`) in full quality, with 200 steps,
  and read the whole scene; scenes with lamps are the slowest to sharpen (library models with glowing
  parts). A glow-only distance for the halo would fix it but changes the halo slightly.
* A uniform-buffer window for the first 1000 vec4 of the list (small scenes would then read from uniform
  memory instead of texture memory) is possible within the 16 KB limit of GL; it needs an experiment on a
  real iGPU (`--bench` is the harness) before it is worth the extra code.
* The shape records could be packed more densely (5 instead of about 9 vec4 per distance evaluation).
* The hover highlight changes the picture, so a hover change redraws the scene; as an id buffer it would be free.
* On a fast GPU the moving picture could keep its full effects: when the measured cost of a whole sharp picture (the bands
  give ms per row) is below about 5 ms, draw it in one pass instead of the cheap picture. Not done: it needs a real GPU to tune.
* Testing the window: `MELTCAD_HUD=1` starts with the HUD on; with `MELTCAD_TEST_DIR=dir` the files `dir/hud` (toggle the HUD)
  and `dir/bench` (open the speed test window) are taken like `snap` and `png`; `dir/where` writes where the HUD is to `dir/where.txt`.

## 2026-10-17: hover/transient and emissive-scene CPU/GPU optimization

Base revision: `fff80f282891593ecb4523442e0a0aa31725eb29` on `easy-layout-and-melt`.
This patch preserves the offscreen frame cache, 30 Hz hover picking gate, adaptive
scale, 150 ms full-quality settle, bounding-sphere acceleration, 48/128/200 shader
march caps, and F3 timestamp HUD. No WGSL distance, light or shading math changed.

* `perf::scene_has_glow_resolved` checks only group sources actually returned by
  `pattern::resolve`. An unrelated hidden emissive source no longer activates
  the expensive full-material 200-iteration shader path merely because another
  visible group happens to be linked. Hidden sources of visible linked copies
  **do** activate it. `ScenePack::new` reuses its resolved list.
* Hover transitions temporarily invoke the already-existing fast/low-resolution
  frame. Moving the pointer within the same shape does not postpone refinement;
  after 150 ms of no changes the normal full-resolution picture is restored.
  The cache still prevents re-marching on egui-only repaints. This improves
  transition responsiveness; it does **not** eliminate re-marching for changed
  hover ID, which still participates in the scene shader's colour.
* `--bench` now includes paired `shapes100`/`shapes100_unlit` and
  `shapes300`/`shapes300_unlit` cases, using the same geometry with
  emission zeroed in the matching unlit scene. Existing pipe, 20-shape and
  complex material benchmarks are retained. Their reported actual shape counts
  may be greater than the nominal target since library groups are indivisible.

### Verification protocol

```powershell
git checkout easy-layout-and-melt
cargo build --release --locked -p meltcad
target\release\meltcad.exe --bench perf-before.txt --frames 12
# Switch to the performance branch (keep same driver, power mode, viewport, backend).
git checkout perf/hover-glow-budget-20261017
cargo build --release --locked -p meltcad
target\release\meltcad.exe --bench perf-after.txt --frames 12
cargo test --release -p meltcad
cargo test --release -p meltcad -- frame_cpu_bench --ignored --nocapture
# Full shader golden image comparison on a machine with a working wgpu adapter:
$env:MELTCAD_GOLDEN_DIR="golden-after"
cargo test --release -p meltcad -- gpu_headless_golden --ignored --nocapture
# Collect golden-before from the base commit with the same adapter before comparing.
# Run gpu_headless_matches_cpu and gpu_headless_mesh_matches_cpu on the same host.
```

The pre-change CLI does not have `shapes100_unlit` and
`shapes300_unlit`, so generate paired source cases with the new CLI when
comparing the two GPU configurations; do **not** treat unmatched rows as
before/after data. For like-for-like measurements, compare the five pre-existing
scene rows first, then separately compare emissive/unlit pairs on the new build.
F3 reports CPU frame time, GPU scene-pass timestamp when supported, and actual
moving render scale. `frame_cpu_bench` measures CPU app work, *not* GPU frames.
Use 100/300-shape scenes, with glow and without glow, at equal dimensions and
normal/full vs fast/moving. Also inspect 300+ shapes including linked copies and
intersections, check idle GPU/CPU activity and visually compare GPU goldens and
CPU --render output at fixed camera positions. The CPU simple view intentionally
omits advanced glass/glow appearance; compare shape silhouettes, not pixels.

### Measurement status (this change)

**Not measured:** This environment cannot run Rust/cargo, an interactive
Windows viewport, or the Intel Arc 140V GPU. No validated before/after timings,
F3 readings, GPU/CPU screenshots, power readings, or image-diff results are
claimed for this patch. Previous llvmpipe measurements elsewhere in this file
are historical baseline data only, not new results. In particular any Arc 140V
performance expectation remains a **hypothesis, not a measurement**.

Acceptance: no missing geometry/silhouette regressions in GPU/CPU comparisons;
equal idle render/cache behaviour; lower GPU scene-pass ms in the relevant
unlit/hover-interaction cases without worsening interactive p95 CPU frame
times. Revert hover-fast transitions if pointer motion visibly causes repeated
resolution changes or the elapsed hover-to-sharp time is unacceptable.
