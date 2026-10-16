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
