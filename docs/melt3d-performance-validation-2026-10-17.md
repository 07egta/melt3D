# Melt3D exact reuse and usability validation — 2026-10-17

This change reduces repeated work while preserving the current modelling and
rendering behaviour. It does not change shader formulas, render-quality choices,
model limits, export formats, saved project data, or the startup font path.

## Baseline and scope

- Repository: `07egta/melt3D`, development branch `easy-layout-and-melt`.
- Baseline commit: `fff80f282891593ecb4523442e0a0aa31725eb29`.
- Baseline tree: `1a76c6202bdcbd10192fe24f0dd9cedc94ce6b8a`.
- All 1,449 baseline files were reconstructed against their Git blob hashes and
  modes. Performance comparisons use this baseline, not the stale `main` branch
  or a partially copied upstream tree.
- Test host: Linux x86_64, Rust/Cargo 1.99.1, existing locked dependencies,
  `profile.ci`, four build jobs. Software-Vulkan evidence is labelled separately
  from CPU measurements. No claim is made about Windows/Intel Arc frame rates.

## Changes and preserved behaviour

### Reuse exact geometry preparation and GPU packing

`Field::reusing` and `ScenePack::reusing` reuse inputs that are unchanged. Editing
one shape rebuilds its prepared data and packed record; unchanged groups share
their existing preparation. The cache keys include admitted shape counts and
prefixes, linked source resolution, transforms, group bounds, loft neighbours,
path/outline pool layout, mesh volume layout and melt settings. A bounded pool
or layout change triggers the necessary rebuild instead of reusing stale offsets.

Float keys compare bit patterns as well as normal equality, so a new revision
does not reuse a shape whose signed-zero representation changed. Whole-scene
revision equality retains its existing behaviour. Field access, rendering,
Undo/Redo and document replacement update or clear the relevant cache.

Prepared shapes remain contiguous inside each group. Only their large immutable
outline/path buffers are shared. An early per-shape `Arc<Prepared>` design was
rejected after paired measurements showed slower distance sampling. Cold export
and thumbnails use `Field::new` without retaining cache input snapshots.

The change keeps geometry, blending, materials, selection, glow and all quality
levels. Glow detection follows the visible resolved source of linked groups.
It does not introduce tiny-shape culling or lower quality during hover.

### Bound temporary uploads and discard unnecessary work

Mesh volume conversion streams the exact original little-endian binary16 bytes
through a reusable buffer of at most 1 MiB per volume upload. Small volumes use
only their own padded size. The tests include chunk boundaries, texel padding,
NaN, infinity and signed zero. GPU texture initialization is unchanged.

The scene texture pool shrinks with hysteresis after a substantial reduction,
discarding its obsolete mirror and binding state. Repeated small size changes do
not continuously reallocate it.

Gizmo construction obtains the existing rotation inverse directly, rather than
constructing all prepared geometry merely to read that inverse. Both **All** and
**Scale** retain local scale axes and face/corner handles; Move and Rotate keep
their world axes. Rotated nonuniform scaling still fixes the opposite face.

### Keep the complete library while making access lazy

The build script removes JSON whitespace outside strings. It preserves every
numeric token, escape, string and model value; it does not round-trip numbers
through a serializer. Readable source JSON remains unchanged in the repository.

Generated category names, model names and per-tag normalized search strings are
available without allocating any model geometry. A category is parsed at most
once when requested. Bulk callers share those same category allocations. A model
click resolves its category synchronously, preserving one-click insertion even
if a thumbnail worker cannot start.

Only visible library cards are laid out and queued for thumbnails. Wrapped names
remain complete, including after font/DPI changes. Each egui context owns a worker
with at most 48 pending jobs; scrolling prioritizes visible work, closing the tab
cancels unstarted jobs, and completed textures remain reusable. The existing
one-thumbnail-per-frame fallback is retained if worker creation fails.

EasyView compares borrowed input before copying it and shares the scene snapshot
across camera-only requests. Document replacement invalidates that snapshot.

### Repair input and export usability

- Esc restores an active gizmo gesture without adding an Undo entry. Ordinary
  Esc still clears selection and cancels sketch/curve operations.
- Losing the pointer during a drag cancels it; normal release commits once.
  Delete, Undo/Redo, Alt navigation and resize/DPI handling remain covered.
- Sketch input owns drawing clicks, while the XYZ orientation widget continues
  to work during sketching. Rapid endpoint clicks finish a sketch without adding
  duplicate points, including egui's triple-click classification.
- Scene changes from tools are tracked before hover picking. A throttled dirty
  hover request survives until it can be processed, and camera changes request a
  follow-up. This avoids a stale hover target after the picture has updated.
- The export dialog exposes purpose, quality and scope first. All six formats
  (binary STL, text STL, 3MF, OBJ, PLY, GLB), custom precision, units, scale, axis,
  smoothing, simplification, colour, paint, checks, progress and cancellation
  remain available. Nondefault saved options expand the advanced section.
- The outer export card scrolls at a small viewport while keeping its action
  buttons visible. Saved colour-off settings are described accurately.
- Estimate/check workers share a scene snapshot. Tolerance changes invalidate
  the matching print check; closing or reopening the dialog cancels superseded
  background estimates/checks without cancelling a separately running export.

## Measurements

Measurements below are for specific operations. Preparation timings are not
GPU frame rates, and embedded JSON bytes are not the whole executable size.

### Library and EasyView

Seven alternating fresh-process baseline/candidate pairs, same CI profile:

| Operation | Baseline | Candidate | Interpretation |
| --- | ---: | ---: | --- |
| Embedded library JSON, 15 categories / 205 models | 2,613,473 bytes | 1,129,297 bytes | 1,484,176 bytes / 56.8% less JSON; identical content tokens |
| First model access | 6.684 ms, parses all 205 models | 0.453 ms, parses first 12-model category | Remaining categories are deferred; these are different amounts of immediate work |
| Parse remaining categories after first access | Included above | 4.836 ms | Paid only when requested; not removed functionality |
| EasyView unchanged input, 300 shapes | 99.261 µs/update | 33.770 µs/update | Median, 2,000 updates/process |
| EasyView camera-only update, 300 shapes | 133.946 µs/update | 34.472 µs/update | Median, 2,000 updates/process |

The generated metadata catalog's Rust source is 42,674 bytes. This is not an
executable-size measurement or a promise of the same startup gain on every PC.

### Exact update preparation

Five runs, 30 single-shape edits per scene. The comparison is full reconstruction
versus incremental reuse in the final candidate, including actual App revision
tracking, retained input snapshots, CPU field preparation and GPU packing. This
isolates the work saved; it is not an application-frame or GPU-time comparison.

| Shape workload | Count | Full reconstruction (ms/edit) | Incremental (ms/edit) |
| --- | ---: | ---: | ---: |
| primitives | 10 | 0.008 | 0.013 |
| primitives | 100 | 0.067 | 0.073 |
| primitives | 300 | 0.203 | 0.213 |
| profiles48 | 10 | 0.057 | 0.022 |
| profiles48 | 100 | 0.610 | 0.105 |
| profiles48 | 300 | 1.859 | 0.341 |
| curves8 | 10 | 0.714 | 0.483 |
| curves8 | 100 | 7.143 | 0.572 |
| curves8 | 300 | 21.993 | 0.942 |

The 300-profile and 300-curve ratios of the displayed medians are 5.45× and
23.35×. The median of the five individual curve ratios is 23.67×; these are
different summary statistics. Primitive edits add approximately 5–10 µs of
cache bookkeeping in this benchmark. The benefit targets expensive preparation;
not every scene or operation becomes faster.

### Actual application CPU frames

Five alternating baseline/candidate pairs, the same 306-shape fixture and
2528 × 1440 egui input frames. Values are medians of each run's average.

| Operation | Baseline ms/frame | Candidate ms/frame | Baseline p95 ms | Candidate p95 ms |
| --- | ---: | ---: | ---: | ---: |
| idle | 2.08 | 2.06 | 2.44 | 2.47 |
| orbit | 2.10 | 2.11 | 2.49 | 2.48 |
| hover | 2.20 | 2.13 | 2.68 | 2.43 |
| node drag | 2.58 | 2.60 | 3.03 | 3.46 |
| gizmo drag | 2.26 | 2.11 | 2.93 | 2.40 |

The full-frame result is mixed: hover/gizmo averages improve, while orbit and
node-drag averages are essentially unchanged in this sample. Node-drag p95 is
higher (3.03 → 3.46 ms), with overlapping run ranges (2.98–4.46 versus 2.74–4.67).
Individual runs include scheduling outliers, including a candidate orbit frame
of 24.81 ms. These results do not establish a universal latency improvement or
presented-frame rate.

### Cold sampling, rendering and export

Three alternating baseline/candidate pairs using identical benchmark-only
instrumentation. Sampling uses 30 shapes and 3,840 points, repeated 20 times;
rendering uses a four-view 128-pixel sheet and a 128-pixel thumbnail; export uses
six shapes at 0.6 mm tolerance with simplification disabled. Values are medians.

| Workload | Distance ns, before → after | Sheet ms, before → after | Thumbnail ms, before → after | Export ms, before → after |
| --- | ---: | ---: | ---: | ---: |
| spheres | 642.589 → 611.181 | 121.086 → 123.976 | 117.041 → 120.655 | 16.709 → 16.633 |
| profiles24 | 16565.019 → 16253.726 | 3446.996 → 2972.170 | 2791.188 → 2766.272 | 59.639 → 52.069 |
| curves8 | 33625.996 → 33112.246 | 5978.403 → 6123.093 | 5512.451 → 5611.194 | 53.659 → 55.413 |

All recorded distance-sample, RGBA-sheet, RGBA-thumbnail and mesh
(vertex-coordinate/triangle-index) hashes match between versions. All exported
meshes are closed and keep the same vertex/triangle counts. Separate regression
tests compare reused and cold fields/packed data bit-for-bit and compare actual
rendered pixel arrays after edits and Undo.

Contiguous prepared data removes the first design's persistent sampling
slowdown. Final distance medians improve by 1.5–4.9%; several sphere/curve render
or export medians are 1.8–3.3% higher. The limited sample and mostly overlapping
ranges do not justify treating every small difference as either a guaranteed
gain or guaranteed noise. No Windows/Intel Arc speed claim is inferred.

### Normal release executable

Both versions were built as the actual Linux x86_64 application with
`cargo build --release --locked --offline -p meltcad --bin meltcad`, Rust 1.99.1,
the same locked dependencies, existing release settings and four build jobs.
Thin LTO and the existing line-table debug information were retained; neither
executable was stripped. These are normal release binaries, not test/CI binaries
or Windows distribution files.

| Executable | Size | SHA-256 |
| --- | ---: | --- |
| Baseline | 101,067,656 bytes | `5febf46ccad9e1da28caebdc488b3d5737d5be481814f83d7d78fa5c3ec1311a` |
| Candidate | 99,934,824 bytes | `8bf86cfdcc7357fc27b7bd5d6a02f81780c2d3577861d23afdee70d987b06c48` |

The executable is **1,132,832 bytes (1.120865%) smaller**. Its `.rodata` section
decreases from 5,333,848 to 3,858,776 bytes. Both builds succeed with the same seven
existing warnings. Build duration is not compared because dependency artifacts
were reused. All 1,449 baseline files remained unchanged before and after the
build; candidate production source files also remained unchanged during the build.

The saved release executables were then run directly with `--render`, the same
mesh/sphere/cutter fixture and size argument `96`. Both exit successfully with
empty stderr and produce identical 192 × 192 RGBA PNGs: **all 36,864 pixels and
the PNG files themselves match**. The common PNG SHA-256 is
`b1640eddb3ae75bdbca883c710ef0e3f4950c655fd54b038d41e825c5ac4f561`;
the decoded RGBA SHA-256 is
`72cfcd93bf53819a29ce29d89fe3c8f73245988a4d45b38fd64c38d261d47617`.
This also exercises production configuration after compiling out test counters.

### Memory tradeoffs

Exact incremental reuse retains bounded CPU/GPU input snapshots. Two independent
caches for a 300-shape test scene retain approximately 208,200 bytes for primitives,
424,800 bytes for 48-point profiles, or 285,000 bytes for eight-node curves,
excluding allocator rounding and container capacities. Mesh voxel volumes remain
shared. Maximum admitted outline/path payload plus fixed Shape/Arc storage is
about 2.06 MiB for both caches, before strings and allocator/container overhead.

This is a CPU-time/memory tradeoff, not a claim that every scene's total RSS falls.
The upload buffer bound, lazy library parsing and pool shrink address separate
sources of temporary or retained memory. No previous-field or previous-pack
chain is kept alive.

## Verification

| Check | Result |
| --- | --- |
| Exact original baseline, all Melt3D tests | 485 passed / 0 failed / 28 ignored |
| Exact original PR #22 alone on the baseline | 487 passed / 0 failed / 28 ignored |
| Final candidate, all Melt3D tests | **541 passed / 0 failed / 35 ignored**, 7.00 s after final lint edits |
| Normal candidate app build, CI profile | Passed; actual Japanese UI launched and inspected |
| Normal release application, both versions | Both builds and direct CLI renders pass; output PNGs and all decoded pixels are identical |
| Targeted real-input regressions | Esc, lost pointer, one-gesture Undo/Redo, Delete, XYZ navigation during sketch, rapid endpoints, same-frame/deferred hover and rotated scale handles passed in the full suite |
| Software-Vulkan mesh rendering | 1 passed, actual llvmpipe adapter; details below |
| Changed Rust files / whitespace | Formatted; `git diff --check` passes |
| Workflow static validation | YAML, permissions/triggers/dependencies and all three explicit Bash blocks pass |
| `cargo xtask ci` | Blocked at existing formatting failures in 34 untouched files; exact baseline has 46 |
| Melt3D clippy, all targets, `-D warnings` | Fails on existing diagnostics in both versions; **no new diagnostic source spans** after fixes |

The final clippy comparison records 53 distinct baseline diagnostic source spans
and 52 candidate spans (42 versus 41 distinct file/message categories). The
comparison includes the primary source text so line-number shifts cannot hide a
new instance of an existing warning. Examples of remaining baseline issues are
unused glyph imports, unused older geometry helpers and existing style lints.
The normal test/build jobs tolerate those warnings; `cargo xtask ci` requires
strict lint/format cleanliness. This report does not call that aggregate gate
passed. Unrelated files were not mass-reformatted across the existing PR stack.

Test-only preparation/packing counters are compiled out of the normal app,
including their storage and increments. The full-volume conversion helper is
retained only as a test reference for streamed-byte parity.

### Actual graphics-adapter check

`gpu::headless_tests::gpu_headless_mesh_matches_cpu` ran on **llvmpipe LLVM 20.1.2,
Mesa 25.2.8, Vulkan** with the lvp ICD. It did not return through the no-adapter
skip path. At 360 × 234 pixels, 2,586 silhouette pixels occurred in both CPU and
GPU images, 49 only in the GPU image and zero only in the CPU image: about 1.9%,
within the existing 6% CPU/GPU tolerance. Re-uploading the mesh volume produced
an exactly equal GPU pixel array on the repeated draw. The test passed in 85.55 s;
this software-driver duration is not a hardware performance estimate.

This fixture has 73 × 43 × 45 = 141,255 nodes and a 282,512-byte padded upload,
so it exercises one chunk. Multi-chunk boundaries and binary16 edge values are
covered by the separate streamed-byte identity tests, not by this adapter fixture.


### Reproducing the targeted checks

```sh
cargo test --profile ci --locked -p meltcad
cargo test --profile ci --locked -p meltcad exact_cache_update_bench -- --ignored --nocapture --test-threads=1
cargo test --profile ci --locked -p meltcad cold_sampling_render_and_export_runtime -- --ignored --nocapture --test-threads=1
cargo test --profile ci --locked -p meltcad frame_cpu_bench -- --ignored --nocapture --test-threads=1
cargo test --profile ci --locked -p meltcad benchmark_library_initial_access -- --ignored --nocapture --test-threads=1
cargo xtask ci
```

Run first-access measurements in a fresh process so static library caches have
not already been populated. Run before/after CPU benchmarks alternately on an
otherwise idle machine. For comparisons against the original commit, apply only
the same benchmark harness to a separate baseline checkout; do not copy changed
production code into that checkout. Compare samples, pixels, vertices and
triangle indices as well as elapsed time.

### Actual UI reproduction

Use a 960 × 600 client area with Japanese fonts, open Export, run the print check
and enable the triangle-reduction controls. The original card clips its title
and bottom action buttons once the extra content appears. The candidate keeps
the footer available and scrolls the body. Both states were captured from the
actual original/candidate apps; the candidate also passed English-tag search
(`dog` → `いぬ`) and one-click model insertion.
The comparison images are supplied with the task report. Direct GitHub image
attachment was unavailable because the connected-browser session was not
accessible; screenshots are not committed as repository evidence.

Linux EasyView screenshots validate this layout and text. They do not establish
Windows launch, native file-dialog behaviour or Intel Arc performance. The exact
Windows acceptance procedure remains in
[yuracad-windows-acceptance.md](yuracad-windows-acceptance.md).

## Existing PR review decisions

These are reviewed ideas and integration decisions, not a claim that the PRs
have been merged. The open-PR inventory was checked; only the targeted subset
below received detailed code/behaviour review.

| PR | Decision |
| --- | --- |
| #3 | Retain native checks; harden release handling to explicit, unique unpublished drafts after Windows, Tests and Web pass. A merge never replaces the download. |
| #10 | Adopt corrected drag cancellation. Original patch consumes ordinary Esc before checking for an active drag. |
| #11 | Adopt corrected sketch input isolation. Original patch also disables existing XYZ navigation. |
| #17 | Use exact glow/source detection without the proposed hover-quality reduction. |
| #22 | Preserve the simplified export choices; add responsive outer scrolling, accurate saved-state text and worker/snapshot lifecycle fixes. Exact original PR was also tested in isolation. |
| #24 | Rework incremental caches with complete layout/context guards and exact byte/sample tests. Keep contiguous prepared data after measuring the first design. |
| #30 | Adopt exact library JSON compaction, bounded exact volume conversion and pool shrink; do not introduce an unnecessary explicit zero-upload slab. |
| #32 | Index metadata, defer categories, virtualize visible cards and bound context-local thumbnail work. |
| #31 | Do not adopt unchanged: tiny-shape omission and removed quality choices conflict with feature/quality preservation. |
| #18 | Do not merge unchanged: recovery files from another live process can be treated as abandoned and deleted; sequential recovery prompts can replace an unsaved recovered scene. |
| #29 | Do not merge unchanged: queued Open records its edit baseline too late, so intervening edits can be overwritten; renaming the old save away creates a crash gap; the one-worker budget can drop an automatic print check. |

## CI and release status

The original PR #3 run
[`39009176319`](https://github.com/07egta/melt3D/actions/runs/39009176319)
failed before any retrievable runner steps. Empty job steps and unavailable logs
do not identify a compiler error or prove the exact account/billing cause. A
missing legacy commit status is not a green check.

The hardened PR #3 head `26f3fc0d327f5633228b278d3ea1aab0e4eda937`
triggered [run 39033169718](https://github.com/07egta/melt3D/actions/runs/39033169718).
Windows, Tests and Web again failed within two seconds with **empty raw job-step
arrays**; draft preparation was skipped. Check annotations exist, but their
contents are unavailable through the available repository API capability. The
specific account/runner cause is therefore still unverified. No workflow step
ran that could establish Windows compilation or test success.

The PR must retain its pending status until required checks succeed. The current
PR checks and discussion are authoritative for later runs; this report is not a
claim of remote CI success or a Windows GUI acceptance result.


The candidate workflow has read-only default permissions and grants write access
only to explicit draft preparation. It validates both Windows executables,
executes the native headless render, writes SHA-256 checksums, uses artifacts from
the same run and checks the branch head immediately before creating a unique
draft. Existing releases and published assets are never overwritten. Interactive
Windows verification and publication authorization remain separate requirements.
