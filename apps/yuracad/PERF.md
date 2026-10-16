# MELTCAD performance notes

Goal: light on the machine. Dropping effects while moving is fine. Nothing here could be run on a GPU
by the author of the change (no display in the build box), so the effects below are reasoned
estimates, marked **(est.)**. Please check them once on a real machine (`YURACAD_TIMING=1`).

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
| 11 | Start-up: CJK font search and read run on a thread started at the top of `main` (joined when the window exists); the shader/pipeline compile runs on its own thread, the window opens first and the viewport fills in when it is ready. `YURACAD_TIMING=1` prints phase times to stderr. | `main.rs`, `fonts.rs`, `gpu::install`, `perf::mark` | window shows after the graphics device is up, not after the shader compiles **(est. 100-1000 ms earlier, driver dependent)** |
| 12 | `[profile.release.package.yuracad] codegen-units = 1` in the workspace `Cargo.toml` (only this crate). | `Cargo.toml` | small, CPU picking/export loops |

Also fixed on the way: a carving shape's id was inverted in `combine()` (the whole body lit up when
a cutter was hovered).

## Behaviour that changed on purpose

* While moving: no ambient occlusion, rougher shadow, no refraction in glass, no lamp halo. A full-quality frame appears 150 ms after the last move.
* Hit threshold and step length changed; full-quality images differ from before by well under a pixel.
* A scene with lamps still marches with materials (needed for the halo) in full quality.

## Checking it

* `YURACAD_TIMING=1 cargo run --release -p yuracad` prints lines like `[yuracad]   412.3 ms  gpu ready`.
* Idle check: leave the window alone; the process should sit at about 0 % CPU/GPU. Move the mouse over the side panel: GPU only blits.
* Quality menu: "がしつ" next to "書き出し".
* Compare a still picture with the old build at 1x ("きれい"): edges, shadows and glass should match.

## Recommendations (not done)

* Release profile: for a shipping build, a separate profile with `lto = "fat"`, `codegen-units = 1`, `strip = true` would shrink and speed it slightly, but it changes every crate, so it was left alone. `panic = "abort"` must not be used: the main SolidCraft app relies on `catch_unwind`.
* wgpu backends: restricting to `Backends::PRIMARY` (Vulkan/DX12/Metal) skips the GL probe at start-up (tens of ms), but drops machines that only have GL.
* If the viewport is still too slow on weak GPUs: lower the default max iterations, or render the still frame in two passes (a low-res march giving a start distance per pixel). Not needed so far.
* Material preview swatches in the side panel are tessellated by egui every frame the panel repaints; caching them as one texture would save CPU on the Material tab.
