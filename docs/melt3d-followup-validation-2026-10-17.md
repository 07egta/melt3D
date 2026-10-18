# Melt3D PR #52 follow-up validation

The follow-up reduces repeated curve preparation and document-save allocation,
repairs unsaved-state tracking, and incorporates the reviewed SVG import work.
The measured benefit depends on the operation: curve preparation improves, while
ordinary application-frame timings are mixed. The lint-corrected final candidate
reduces the 300-curve incremental preparation median from 1.139 to 0.532 ms/edit
(53.29%), while primitive-300 editing, 306-shape gizmo/hover and GPU packing
retain increases. All twelve same-renderer cross-version images from this
candidate match PR #52 byte for byte and pixel for pixel. Both earlier series
are preserved separately; local workspace CI4, the Windows MSVC cross-check and
both ordinary Linux release builds have passed. The unstripped release executable
increased by 89,640 bytes (0.08968996%). A separate production release CPU render
matched exactly, and the final release passed the two recorded native UI flows
with all six screenshots reviewed. Windows linking/execution/GUI remain untested.

**Measured candidate identity:** the primary performance results use preserved
test executable **`66cf7751…`**, including the equivalent iterator syntax
correction after CI3. Its standalone regressions and separate final render checks
passed; the complete local CI4 gate, Windows MSVC cross-check and ordinary Linux
release build have also passed on this source. Its separately identified release
executable is `1b5136bf…`; production rendering and the scoped final native UI
checks passed with unchanged inputs and closed evidence. Windows linking/test/
runtime/GUI remain untested. Previous `d93765d9…`
performance and render results remain historical records. No result
is transferred between identities.

**Publication checkpoint:** the local results below are closed. At this checkpoint,
the follow-up had not been pushed, checked remotely or merged, its three selected
GitHub images had not been uploaded, and the evidence ZIP had not been packaged.
[PR #52](https://github.com/07egta/melt3D/pull/52) records later publication,
attachment and archive status; local validation does not establish those actions.

## Baseline and scope

- Repository: `07egta/melt3D`, development branch `easy-layout-and-melt`.
- Addendum baseline: PR #52 head `153904df047ce017c76acd86f1895a15b3e32ae1`.
  All 1,452 reconstructed file blobs matched that remote tree, with no missing
  files or mismatches in the verification record.
- This is an addendum to the earlier validation report. It does **not** remeasure
  that report's older `fff80f2` baseline or carry its startup, export, release-size
  or Windows conclusions forward as new results.
- Host: Linux x86-64, Rust/Cargo 1.99.1, locked dependencies and the existing `ci`
  profile. The standalone comparison builds used three build jobs; the later
  workspace CI4, Windows cross-check and ordinary release builds used two.
  The paired CPU and font
  measurements used CPU 8 affinity and one test thread. The save measurements
  used separate processes.
- The main performance and render results use `66cf7751…`, including the reviewed GPU
  allocation change and equivalent Clippy syntax correction. The earlier
  `d93765d9…` 18-invocation series/render captures and `130e94ed…` initial
  19-invocation series remain separate. Eleven native UI captures and the save,
  font and curve-uniform microbenchmarks belong to that initial functional stage.
- A complete compilation-input source manifest was not captured for that initial
  candidate. The recorded lint edits and allocation correction are known deltas,
  not proof that a later rebuild differs in only those inputs. The final results
  compare directly with the preserved PR #52 binary; an initial-to-final
  difference cannot be attributed solely to the allocation fix.

| Preserved artifact | Bytes | SHA-256 |
| --- | ---: | --- |
| PR #52 tests | 133,855,072 | `9e233b6c629c1ef232376e6fa3019c4c55e3c5b13f202c01aa861ed426868c95` |
| Initial candidate tests | 140,654,000 | `130e94edfdcee95a53e1fc30ebd9858618442dcb636233ba5e8098dab8800fa0` |
| Previous candidate tests, before CI3 lint correction | 140,670,304 | `d93765d9571bb9bd64c67a353c0749f27c61475bdf31b8acc555567a71cbe44a` |
| Final standalone candidate tests, after lint correction | 140,661,776 | `66cf77512ada45b7ffb1bcd7631ada570c2fb9e35c72d4bd1c59e894e5118e54` |
| PR #52 native, CI profile | 126,892,960 | `32577b719e9b889098eb4b81dd8fadf3ec306139cfee37ff38ee190e1df04600` |
| Initial candidate native, CI profile | 126,943,136 | `bbe7d35688a189cf4d7e4c8de11ffea4c5e4233f389e3078cba6ae367214205d` |

These sizes identify the artifacts; they are not normal release executable-size
measurements. The final standalone build records source-manifest digest
`01d0288faf1bc09f99f275c164dc4ade3e12860894c6cfd954f86250f15ad0a0`.
Its immutable metadata SHA-256 is
`d11e536d5dc675fd7f1661502d579d93a2f1492c4c148c3e4f35a6b10cb38bbc`.
All 215 recorded dependency artifact feature/profile entries match the PR #52
standalone build. The application uses the same `ci` profile and empty feature
set. All 1,453 source files stayed unchanged during the build and the binary stayed unchanged
during its passing **573 / 0 / 39** test run, including all ten required cache,
packed-data, capacity, fallback and degenerate-input regressions. The immutable
`lint-final-candidate-validation/metadata.json` and `lint-final-candidate-freeze.json`
record these checks; they do not turn the older initial measurements into final
source measurements.

**Source and publication identity:** the lint-corrected standalone source and
test executable are identified above; CI4, the Windows MSVC cross-check and the
ordinary release builds verify the recorded source content. The release binaries
are identified below. Use **the commit containing this report** as the repository
reference. [PR #52](https://github.com/07egta/melt3D/pull/52) and the saved external
evidence record the actual remote head, commit parents, checks and merge state
when available. This report does not embed its own commit SHA or the SHA of an
archive containing this report. Production rendering and final native UI have
separate closed results below; Windows linking/test/runtime/GUI remain unperformed.

## Reuse without changing geometry calculations

The candidate keeps prepared bounds with unchanged geometry, reuses prepared
curve data when packing, and retains curve radii needed for camera-only bounds
updates. Existing source resolution, visibility, pool admission and cache-key
checks remain relevant to reuse. Selection and view bounds retain their distinct
radius calculations; hidden or unpacked shapes follow the required fallback.

Targeted tests compare cold/reused SDF samples and packed float bits, exercise
bounded pools and linked sources, and check degenerate/nonfinite curve inputs.
These are exact checks for their fixtures, not a proof for every possible scene.
The change does not obtain its measured savings by reducing render quality.

## Final candidate performance

The principal comparison is preserved PR #52 binary `9e233b6c…` against
lint-corrected standalone binary **`66cf7751…`**. Cache preparation, actual
application CPU frames and CPU-side GPU-array packing each used three
alternating process pairs on CPU 8, one test thread, the existing `ci` profile
and unchanged benchmark fixtures. **All 18 invocations were accepted; none was
discarded or rerun.** One preflight sample before invocation sequence 10 caused
a quiet-host wait; this was not a discarded measurement or necessarily the start
of the entire series. The 0.10 load threshold was unchanged.

The tables show each version's **median of three run summaries**. Change is
`(final median / PR52 median - 1) * 100`; negative means less time. The evidence's
median of within-pair ratios is a different statistic and is not substituted
for these rates. Three repetitions and rounded summaries are descriptive
measurements, not confidence intervals or a guarantee of physical-host isolation.
Both earlier candidate series remain separate; no favorable values are pooled
across series and the changes between series are not assigned to the syntax fix.

### Application cache preparation

Each of the nine fixtures makes 30 single-shape position edits in groups of ten.
The benchmark checks sampled SDF bits and all packed-list float bits against the
same executable's cold path. Those within-binary assertions do not establish
cross-version image equality.

| Workload | Shapes | PR #52, ms/edit | Final, ms/edit | Change from run medians |
| --- | ---: | ---: | ---: | ---: |
| curves8 | 10 | 0.540 | 0.064 | -88.15% |
| curves8 | 100 | 0.629 | 0.178 | -71.70% |
| curves8 | 300 | 1.139 | 0.532 | -53.29% |
| primitives | 10 | 0.019 | 0.020 | +5.26% |
| primitives | 100 | 0.106 | 0.094 | -11.32% |
| primitives | 300 | 0.258 | 0.283 | +9.69% |
| profiles48 | 10 | 0.031 | 0.021 | -32.26% |
| profiles48 | 100 | 0.122 | 0.111 | -9.02% |
| profiles48 | 300 | 0.523 | 0.410 | -21.61% |

Incremental curves improved in all nine pairs across the three sizes. The
300-curve medians **1.139 → 0.532 ms/edit represent a 53.29% reduction**. This is
preparation time, not viewport frame time.

Primitive 300 increased by **25 µs/edit (+9.69%)** at the run medians; its paired
changes were **−7 / +11 / +43 µs**. Primitive 10's medians increased by 1 µs,
while its paired changes were −4 / −1 / +1 µs; differences of medians and paired
differences are not interchangeable. No-op results also remain visible in the
full table: primitive 10 increased 1.478 → 1.592 µs/update in all three pairs,
and primitive 300 changed 45.594 → 46.689 µs/update with mixed pairs. Cold
primitive-300 preparation increased 0.251 → 0.263 ms/edit.

### Actual application CPU frames

`App::frame_body` receives the real pointer-event sequences; the egui GPU paint
callbacks are not executed. Four library scenes have 1, 36, 107 and 306 actual
shapes and five operations each. The largest scene's medians of per-run average
CPU time are:

| Operation, 306 shapes | PR #52, ms/frame | Final, ms/frame | Change from run medians |
| --- | ---: | ---: | ---: |
| Idle | 2.69 | 2.63 | -2.23% |
| Orbit | 2.92 | 2.76 | -5.48% |
| Hover | 2.64 | 2.76 | +4.55% |
| Node drag | 3.34 | 3.16 | -5.39% |
| Gizmo drag | 2.76 | 2.84 | +2.90% |

At 306 shapes, node drag and orbit improved in all three pairs. **Gizmo drag
increased in all three pairs** by +0.12 / +0.14 / +0.06 ms/frame; its medians
increased 2.90%. Hover medians increased 4.55%, with mixed paired changes.
Node drag at 36 shapes changed **0.99 → 1.04 ms/frame (+5.05%)**, despite two
faster pairs and one slower pair. At 107 shapes, orbit changed
1.27 → 1.29 ms/frame with mixed pairs. There is no all-size or all-operation
node-drag improvement claim for this final series.

Accepted variation is retained, including PR #52's second-pair 107-shape idle
2.82, orbit 2.90, node drag 3.15 and gizmo drag 2.94 ms/frame, and 306-shape idle
5.61 ms/frame. Candidate variation includes 36-shape node drag 1.25 ms/frame in
the first pair and 107-shape orbit 1.88 ms/frame in the third. No value was removed
after viewing its magnitude.

**All 20 cases have identical frame, pack and field counts across all six
executions.** Per-run p95/max summaries and ranges remain in the evidence; their
medians are not pooled percentiles. These CPU results are not displayed FPS,
GPU execution time or end-to-end input latency.

### CPU preparation of GPU arrays

The supplementary incremental packing results are:

| Shapes | PR #52, ms/edit | Final, ms/edit | Change from run medians |
| --- | ---: | ---: | ---: |
| 10 | 0.009 | 0.008 | -11.11% |
| 100 | 0.050 | 0.050 | +0.00% |
| 300 | 0.130 | 0.150 | +15.38% |

The 300-shape medians increased **0.130 → 0.150 ms/edit (+0.020 ms, +15.38%)**;
one pair improved and two became slower. This CPU helper has no GPU execution,
runs briefly and prints to 0.001 ms resolution, so small differences are not a
standalone application-performance claim. Cold/no-op paths and every pair are
retained in the full tables.

`paired-bench-lint-final-results/results.json`, `summary.json`, `analysis.md`
and `all-paired-values.csv`, with `lint-final-paired-benchmark-completion.json`,
preserve 93 metrics, 279 paired values, 36 verified raw logs and 192 verified
emitted records. Source and immutable standalone metadata stayed unchanged.
Font, save and curve-uniform microbenchmarks below remain earlier comparisons
inside the initial candidate; they were not rerun in these 18 invocations.

## Previous candidate `d93765d9…` — historical summary

This distinct 18-invocation series had zero discarded measurements or measurement
reruns and two preflight wait samples. Its source digest was `31eb44d7…`, and its
standalone suite passed 573 / 0 / 39 before CI3 reported the iterator Clippy lint.
It is preserved in `paired-bench-final-results/` and
`final-paired-benchmark-completion.json`; it is not the final primary dataset.

Its curve-10/100/300 incremental medians were **0.523 → 0.077**, **0.673 → 0.194**
and **1.127 → 0.513 ms/edit (54.48% reduction)**. Cold curve-300 medians were
26.627 → 16.326 ms/edit. The following regression/noise observations remain
attached to that older series:

- Primitive 10 increased +2 / +1 / +8 µs/edit. Primitive 300 medians were
  0.290 → 0.272 ms/edit, with −18 / −377 / +27 µs paired changes and the accepted
  PR #52 sample **0.634 ms** retained.
- Profiles48 at 300 had 0.439 → 0.435 ms/edit medians but −49 / +2 / +182 µs
  paired changes. GPU incremental packing at 300 increased
  **0.131 → 0.138 ms/edit (+5.34%)**.
- The first pipe-only idle pair was **0.49 → 1.00 ms/frame**. Small-scene orbit
  medians were 0.49 → 0.52 and hover medians 0.51 → 0.54 ms/frame.
- Node drag improved across all four sizes and all pairs **only in that older
  series**. Its 306-shape medians were 3.26 → 3.22 ms/frame, orbit 2.79 → 2.79
  and gizmo 2.82 → 2.82; idle was 2.83 → 2.64 and hover 2.78 → 2.72. Its 20-case
  frame/pack/field counter equality and every p95/max/raw observation remain
  preserved separately. They do not extend the final candidate's node-drag claim.

The complete earlier metric tables remain in those raw/analysis artifacts,
without adding another duplicate primary table to this report.

## Initial candidate measurements — preserved history

This earlier series compared PR #52 with initial functional candidate
`130e94ed…`, before subsequent lint edits and the final group-bounds allocation
change. Its values remain evidence of that stage and are not the final headline
results. A complete compilation-input manifest was not captured for the initial
candidate. Known deltas do not establish that the final rebuild changed only one
variable, so an initial-to-final timing change is not attributed solely to the
allocation correction.

### Initial application measurements

Initial cache preparation, in milliseconds per edit:

| Workload | Shapes | Cold PR #52 → initial | Incremental PR #52 → initial |
| --- | ---: | ---: | ---: |
| curves8 | 10 | 0.830 → 0.478 | 0.585 → 0.064 |
| curves8 | 100 | 8.656 → 5.111 | 0.649 → 0.188 |
| curves8 | 300 | 29.574 → 17.808 | 1.323 → 0.661 |
| profiles48 | 300 | 1.922 → 1.763 | 0.440 → 0.484 |
| primitives, initial | 300 | 0.580 → 0.279 | **0.273 → 0.313** |


The initial primitive-300 incremental run medians differed by **+40 µs/edit**;
its paired increases were +9 / +36 / +40 µs. This result motivated the reviewed
allocation follow-up and remains preserved. The final direct PR #52 comparison
above is the evidence for the final candidate.

Initial CPU-frame medians for the 306-shape library scene, in milliseconds per
frame:

| Operation | PR #52 | Initial candidate |
| --- | ---: | ---: |
| Idle | 3.21 | 2.99 |
| Orbit | 2.65 | 2.67 |
| Hover | 2.70 | 2.62 |
| Node drag | 3.17 | 3.12 |
| Gizmo drag | 2.74 | 2.72 |


The initial 20-case frame/pack/field counter agreement and per-run summaries are
retained with that series. This table is not a final frame-performance result.

### Initial-candidate curve-uniform and font microbenchmarks

These comparisons use old/new paths inside the **same initial candidate executable
`130e94ed…`**. They were not rerun in the final 18-invocation series and are
separate from the final PR #52/candidate application measurements.

For curve bounds, clearing the retained radius cache restores the prior
recalculation path. Each case runs 128 frames in five alternating pairs. The
selected-group cases below are microseconds per uniform update:

| Curves | Recalculation | Cached | Radius-vector allocation |
| --- | ---: | ---: | ---: |
| 10 | 190.677 | 0.837 | 384 bytes |
| 100 | 1,973.954 | 7.539 | 3,072 bytes |
| 300 | 5,844.055 | 25.638 | 12,288 bytes |

All uniform float bits match in all nine tested count/selection cases and every
comparison frame. The retained vector is bounded at 512 entries/12,288 bytes on
this 64-bit host, excluding its header and allocator overhead. The roughly
194–262-fold range across the isolated cases applies only to uniform generation;
it is not a viewport speedup. The existing output reports internal medians, not
all five individual timings, so no internal-run range is reconstructed.

The font helper avoids repeatedly comparing 4,669,688 immutable font bytes when
their identity is already shared. Family mapping, font index, tweak settings and
the fallback byte comparison retain value-equality behavior. The equality
regression covers those settings. Five alternating rounds of 2,000 checks per
path measured **250,555.5 → 120.9 ns/check** at the median (legacy range
231,743.1–269,856.3; helper range 120.1–132.5). This is a font-comparison
microbenchmark, not a library-panel or whole-application speed claim.

### Initial measurement records and exclusions

The initial paired CPU runner accepted 19 test processes: six each for cache, frame and
CPU-side GPU packing, plus the internally paired curve-orbit test. It retained
raw logs, hashes, process usage and background-load records. A namespace-PID
instrumentation error was excluded and corrected; two short GPU-pack runs were
excluded under the existing load rule when `rsync` overlapped. Thresholds were
not relaxed and exclusions were not selected from the timing values. A copied
stderr file missing its last line was recovered to a separate file from the
recorded rows, with an exact match to its original recorded SHA-256; the
incomplete copy and recovery record were kept.


Those exclusions/recovery records belong only to the initial series. The final
series accepted all 18 invocations without a measurement discard or rerun.
`paired-bench-results/results.json` and the initial analysis keep the historical
raw values; font and save records retain their separate same-binary scope.

## Save state and document saving

Successful Undo/Redo now conservatively mark changed work unsaved, preserving
New/Open/Close save confirmation. Empty history remains a clean no-op. A revision
marker records a successful save during an unfinished edit without adding a
scene snapshot or moving the existing pointer-release Undo boundary:

- Save at the current drag position, then release: clean, with one Undo step.
- Move farther after that save: unsaved again.
- Return or cancel to the original position after saving an intermediate one:
  unsaved, even with no net Undo step, because the file contains the intermediate
  position.
- Cancel an ordinary drag after saving an already committed scene: still clean.

The pre-fix instrumented App suite reproduced these five affected scenarios with
`18 passed; 5 failed`. The corrected regressions pass. Save failure leaves the
scene, chosen path, history and unsaved state intact.

Saving now streams the existing pretty JSON through `serde_json::to_writer_pretty`
and a 64 KiB buffered writer under the existing mesh save guard. A unique file
in the destination directory receives the complete document, is flushed, synced
and closed, then replaces the destination with one rename. The old path stays
present until replacement. Errors and caught serialization panics trigger
best-effort cleanup of the staged file; Unix directory sync is also best effort
after replacement.

Tests compare complete bytes with `Scene::to_json()`, including shared mesh
volumes and non-ASCII names, and reopen the result. Injected write/replacement/
serialization failures check old-file preservation and staging cleanup. Unix
tests cover normal permissions and saving through an existing symlink.

Atomic replacement needs directory create/rename permission and changes inode
identity: other hard links retain the old inode. Ownership, ACLs and extended
attributes are not fully preserved. Read-only files are refused. Permission or
storage errors during cleanup, or forced process termination, can leave a staged
file; syncing is not a guarantee against every power-loss/storage failure.
Windows runtime behavior remains untested here.

### Save memory and exact output

Three alternating trials per mode used GNU `/usr/bin/time -v`, one process at a
time, with no concurrent build or benchmark. The same **initial candidate
executable `130e94ed…`** runs either the original `std::fs::write(path, scene.to_json())` or the new save helper.
This save experiment was not rerun on the final candidate. The valid stress
fixture has four distinct 128³ volumes (8,388,608 unique nodes)
and eight mesh shapes sharing those volumes, with finite, poorly compressible
values. This is not an average-model claim.

| Trial | Buffered RSS, KiB | Streaming RSS, KiB | Buffered save, ms | Streaming save, ms |
| --- | ---: | ---: | ---: | ---: |
| 1 | 75,372 | 58,452 | 1,084.667 | 1,173.979 |
| 2 | 75,412 | 58,412 | 1,110.927 | 1,166.379 |
| 3 | 75,604 | 58,256 | 1,361.552 | 999.180 |
| Median | **75,412** | **58,412** | **1,110.927** | **1,166.379** |

Median peak RSS decreased by **17,000 KiB/16.602 MiB (22.54%)**. RSS includes
fixture construction, saving, incremental hashing and output copying. The save
timer excludes hashing/copying. Median save time was **4.99% higher**; timings
vary and the new path includes sync. This supports a memory reduction, not a
save-speed improvement. The 64 KiB buffer does not include model storage or the
remaining per-volume compression/base64 temporaries.

All six tests exited successfully. All outputs were **23,606,322 bytes**, and
external `cmp` found complete byte equality across all six. Their common SHA-256
is `edd7b583e2df618bc82226f7c7cd9312a2e1b6a7f91c61f2c1bae292ece68df8`;
incremental FNV-1a 64 is `b55357021ef1557f`.

## SVG import from reviewed PR #12

The integration preserves local/forward `href="#id"` and `xlink:href` instances
from definitions, shape/group transforms, inherited fill, nested references and
holes. Plain symbols without viewport-dependent sizing work. External references
are not fetched.

Compiling the isolated original parser reproduced `E0515`: collected IDs borrowed
from a temporary tag rather than retaining the source-text lifetime. Correcting
that lifetime fixes compilation. Reference work also has shared per-pass limits:
depth 16, 48,000 referenced tag visits and 32,000,000 expanded fragment bytes.
Charging whole fragments bounds repeated comments and empty-reference fanout
that consume no geometry budget. Exhaustion is reported and later ordinary
inline geometry remains importable. Missing/cyclic references produce a bounded
partial-import warning; unused definitions and ordinary empty groups stay quiet.

The original code could ignore symbol `viewBox`/viewport sizing and import wrong
dimensions. References requiring those unimplemented semantics, and referenced
`<svg>` viewports, now produce an explicit omission warning or a clear error if
no supported shapes remain. Full viewport sizing is not implemented. Directly
instanced plain symbols work, while nested symbol definitions remain hidden.

An isolated 14-test comparison passed: existing inline geometry against PR #52,
references against manual expansion, the original sizing-bug reproduction and
work limits. Repository tests also cover extrusion and dialog interaction.
The card scrolls its preview/options/full warning details while keeping its title
and `読みこむ` (Import) / `やめる` (Cancel) actions outside the scrolling body.

## Actual native UI evidence

The preserved native applications ran on Linux 6.18.55 x86-64 with a Vulkan
software renderer on Xvfb. The initial **1380×860** windows were pixel-for-pixel
identical. In the actual Save → Undo → New interaction, PR #52 replaced the
undone scene without prompting; the candidate showed the save dialog, and
`キャンセル` (Cancel) retained the undone one-box scene.

Of the eleven screenshot files, three identify the PR #52 native binary
`32577b71…` and eight identify the initial functional candidate native binary
`bbe7d356…`. The candidate captures predate the last lint edits and the now-applied
GPU allocation change; they do not establish the appearance of a later final native
build. Two selected images from this completed evidence have already been saved
as task-report file artifacts. That completed file save is separate from the
planned three-image GitHub attachment set below, which remains pending.

At **960×600**, an actual X11 Xdnd local-file drop loaded an SVG with twelve
unsupported-feature categories plus a missing/cyclic-reference warning. Wheel
scrolling reached the last warning, and the fixed `読みこむ` (Import) button
remained usable.
Import followed by Save produced two groups/four shapes, including three imported
shapes and a hole. The manifest records both binary hashes and screenshot hashes.
These observations cover the recorded interactions on this renderer, not every
feature, GPU driver or Windows behavior.

### Final-release UI evidence and PR attachments

After the ordinary release comparison and production-render checks closed, the
final candidate release `1b5136bf…` ran once (PID 122267) on Linux/Xvfb with
Vulkan llvmpipe, LLVM 20.1.2. These captures identify that release separately
from the eleven initial native screenshots above.

Before modelling input, one **180-second deadline from child launch** covered
window acquisition and ROOT's inspection. The first startup PNG was incomplete
and was not approved. ROOT inspected `startup-0003.png`, showing a solid box,
grid and visible controls at **1380×860**, then atomically wrote a separate
`startup-review.json` bound to the launched PID and exact image/SHA. The generated
template was not edited. All **five immutable startup observations** were retained.
Approval was recorded at **21.418 seconds**, including reviewer waiting/writing;
this is **not a startup benchmark** or an explanation of the headless timing below.

The application saved two boxes, then Undo restored one box. New showed the
unsaved-work confirmation; `キャンセル` retained that one-box scene. The saved
two-box file stayed unchanged across Undo/New/Cancel until an explicit later
Save. The Undo and Cancel screenshots match in both bytes and decoded pixels.
At **960×600**, an actual X11 Xdnd drop loaded the unchanged SVG input. Scrolling
reached the final missing/cyclic-reference warning while `読みこむ` and `やめる`
remained visible and usable. Import and Save produced **two groups/four shapes**,
including two additive extrusions and a subtractive hole.

ROOT separately inspected **all six final screenshots** and recorded the bound
final review; `--review` exited 0 at **2026-10-18 04:29:27 UTC**. Startup approval
did not replace this final review.

| Final screenshot | Reviewed observation |
| --- | --- |
| `final-release-ui/undo-after-save.png` | One box after Undo |
| `final-release-ui/new-after-undo-save-confirmation.png` | New is guarded by the save confirmation |
| `final-release-ui/cancel-preserves-undone-scene.png` | Cancel retains the undone scene |
| `final-release-ui/svg-warnings-top-small-window.png` | Preview, options and fixed actions are visible |
| `final-release-ui/svg-warnings-bottom-small-window.png` | Last reference warning and both fixed actions remain readable |
| `final-release-ui/svg-warning-import-completed.png` | Imported scene contains the expected four shapes and hole |

The three complete saved scenes contain **2 → 1 → 4 shapes**. The final SVG result
is **12,522 bytes** and matches the reference in both complete bytes and parsed
JSON, SHA-256 `33502c77624c8cff494f237b2b3aa1e197b93d63ca99491706ef4cc4dbb342c6`.
All **7 post-checks and 33 closure checks passed**, including unchanged source,
modes, release/test executable identities, metadata and helper. All original
**31 files / 2,726,032 bytes** were copied exactly; the retained directory has
**34 files / 2,768,054 bytes** including the previous plan, copy proof and closure.
The application and owned Xvfb process were stopped; there were no reruns.

The final manifest is `final-release-ui/manifest.json`, SHA-256
`61ed04b8b18ded2cffe4d75aa03f464846c50b415c3f05326f2799ffe16dca9e`;
`final-release-ui/closure-results.json` has SHA-256
`ac8ea5d1c3fcb3aefa2f02a1d3ea6395b7303c5ec8b55c0632e27e86937ccdbb`.
These results cover the two recorded Linux flows. They do not establish all
features, Windows behavior, physical-GPU performance or a new cross-version
initial-window comparison.

Two unchanged copies of the final save-confirmation and SVG-warning screenshots
have also been saved as task-report image files, separately from the two earlier
historical image deliverables. Those copies are not additional captures. All
three selected GitHub attachments below exist and have been reviewed, but **none
has been uploaded to GitHub at this checkpoint**. Later URLs belong in
[PR #52](https://github.com/07egta/melt3D/pull/52).

| Scene | Evidence path in the bundle | Capture and attachment status |
| --- | --- | --- |
| PR #52: New after Undo without confirmation | `ui/before/new-after-undo-without-confirmation.png` | Historical baseline capture; not uploaded |
| Final release: New after Undo with save confirmation | `final-release-ui/new-after-undo-save-confirmation.png` | Captured and reviewed; not uploaded |
| Final release: last SVG warning and fixed actions | `final-release-ui/svg-warnings-bottom-small-window.png` | Captured and reviewed; not uploaded |

## Final candidate rendering and numerical checks

The final test executable **`66cf7751…`** completed five new explicit test invocations,
all with exit 0: four ignored GPU tests and the CPU cache-image test listed below.
The two earlier PR #52 golden/cache-image captures from `9e233b6c…` were reused
after checking their manifest and exact image bytes; the baseline tests were not
run a second time: these are **five new candidate executions and two reused
baseline capture records**, not seven new executions. Both executable hashes
remained unchanged. The final source digest is `01d0288f…` and immutable
standalone metadata is `d11e536d…`, matching the measured candidate above.

| Final-candidate test (`66cf7751…`) | Result and comparison |
| --- | --- |
| `gpu::headless_tests::curve_gpu_distance_matches_cpu` | PASS under the existing distance tolerances |
| `gpu::headless_tests::gpu_headless_matches_cpu` | PASS under the existing CPU/GPU silhouette checks |
| `gpu::headless_tests::gpu_headless_mesh_matches_cpu` | PASS under the existing mesh silhouette checks |
| `gpu::headless_tests::gpu_headless_golden` | PASS; six GPU images emitted |
| `cpu_render::tests::incremental_field_four_view_pixels_match_cold_geometry_materials_and_undo` | PASS; six CPU cache images emitted |

The GPU runs used an actual **Vulkan llvmpipe** adapter, LLVM 20.1.2 and Mesa
25.2.8; the unavailable-adapter early return was not treated as success. The
CPU/GPU distance and silhouette checks preserve their original tolerances and
do **not** establish bit-for-bit CPU/GPU equality. Their silhouette logs report
3,802 pixels in both/80 GPU-only/0 CPU-only, and for the mesh case
2,586 in both/49 GPU-only/0 CPU-only.

The separate **cross-version image comparison is exact**:

| Renderer/output | Images per version | Dimensions | PR #52 versus `66cf7751…` |
| --- | ---: | --- | --- |
| GPU golden PPM | 6 | 240×156 | All bytes and decoded pixels identical |
| CPU four-view cache PNG | 6 | 320×320 | All bytes and decoded pixels identical |

The images cover the existing default/default-fast, glass/glass-fast, lamp and
library GPU fixtures, and CPU geometry, material/blend and Undo cold/reused pairs.
Image sizes and hashes are recorded in `render-parity-lint-final/results.json`
(SHA-256 `e3cce15a2278a23ef328cd935cb4036efbb3962deb8fce204aed1bb7cb06f715`).
The copy-verification record checks **51 files / 6,704,443 bytes**, with the
originals before/after copying and the retained destination matching exactly.
Visual review inspected **two representative outputs from the actual final candidate**: the GPU
library image and CPU material/blend four-view image. This is not a claim that
all twelve images received independent manual inspection. The separate six-image
native UI review is recorded above.

These results apply to the unchanged fixtures on the reported software adapter.
They do not establish physical GPU/Windows behavior, display FPS or all-scene
correctness. Logged times are not performance measurements; the earlier
baseline capture overlapped workspace compilation. The separate production release
`--render` and two final native UI flows also passed within their recorded scope.

The final software-Vulkan logs also record a long first render: the golden
`default` row took 110.47 seconds and the mesh's timed row took 97.61 seconds.
The five later golden rows retained the same rounded times as the earlier
candidate (0.01/0.02/0.01/0.03/0.06 seconds). The original PR #52 capture also
had a long first row, but its concurrent compilation makes that duration
unsuitable for a performance comparison. These records suggest a first-use or
warm-state difference; they do not identify a JIT, driver-cache or permission
cause. `render-first-use-timing-notes.json` preserves the exact log references,
recorded environment and missing diagnostic information. The normal release UI
check subsequently established rendered output and completion of the two recorded
save/import flows. Its 21.418-second approval includes reviewer wait, is not a
startup benchmark, and does not establish the cause of the headless delay or a
startup-speed improvement.

The earlier `d93765d9…` render evidence remains unchanged in `render-parity/`:
five candidate invocations passed, two baseline captures were reused, twelve
cross-version images matched exactly and two representative images were visually
reviewed. That historical record is separate from these five new `66cf7751…`
invocations and their own copy/visual records.

## Existing PR history and integration decisions

The GitHub audit checked all four PR #12 commits, their parent links and file
lists. Their net scope is only `svg.rs` and `svg_dialog.rs`:

| Commit | Work reviewed |
| --- | --- |
| `db76734c6b30d89a1d992ec2fad21391997bb867` | Bounded local defs/use expansion |
| `eee598b903bd2f2836b2182617d7ba825081985f` | Transform, hole and unsafe-reference tests |
| `721f12a49b0c7cc54c4b919829b8e7ec475ad990` | Unsupported-feature reporting |
| `4a8997c719b94d2c7946dd4f72bcc590c2d5a1f9` | Import-dialog warning display |

The planned integration commit will have a first parent descended from PR #52
and PR #12's exact head as its second parent, with the reviewed SVG features and
corrections in its tree. The actual commit parents and resulting PR states will
be recorded in [PR #52](https://github.com/07egta/melt3D/pull/52) and external
integration evidence after creation. At the validation checkpoint, the follow-up
history commit had not been pushed and PR #12 and PR #52 were still unmerged;
the planned development-branch merge is not a completed action in this report.

PR #3 head `26f3fc0d327f5633228b278d3ea1aab0e4eda937` is already an ancestor of
PR #52 (the audited comparison is three commits ahead, zero behind). Its history
will be retained by a normal merge of a PR #52 descendant.

| PR | Decision and reason |
| --- | --- |
| [#51](https://github.com/07egta/melt3D/pull/51#pullrequestreview-5557864052) | Retain reviewed UI feedback already in PR #52 and selectively reviewed lint fixes. Do not adopt the whole branch: it includes hover-quality reduction, lost first model clicks, fallback removal and full-cache clearing that defeats reuse. Its Windows numbers belong to that other source. |
| [#18](https://github.com/07egta/melt3D/pull/18#pullrequestreview-5557761971) | Do not adopt the recovery rewrite wholesale: another live process's recovery can be treated as abandoned, and successive recovery prompts can replace unsaved recovered work. The contained atomic save helper is separately implemented and tested. |
| [#29](https://github.com/07egta/melt3D/pull/29#pullrequestreview-5557762270) | Do not adopt the background-job rewrite wholesale: queued Open can capture the edit baseline too late; moving the old save aside creates a crash gap; the worker budget can drop an automatic print check. |

### Additional bounded review of open drafts

The refreshed GitHub search returned **49 open-PR hits**, recorded in
`open-pr-triage-refresh.json`. This was an inventory, not a complete review of all
49 PRs. A further bounded read of actual diffs and relevant frozen source covered
these four draft heads; none was integrated. No build, test or UI execution was
performed for this additional review. #8 and #21 propose intentional behavior
changes whose validation remains outstanding; this review established no
concrete regression in either. The #27/#28 concerns below are derived from source,
not executed failures.

| PR / observed head | Reason not to integrate the whole draft |
|---|---|
| [#8](https://github.com/07egta/melt3D/pull/8), `7f6c6711…` | Changes thumbnail framing from `r * 0.62` to `r` and insertion centering from shape-origin averages to bounds of visible additive shapes. These are new framing/placement choices requiring actual-model visual and placement checks; the audit tooling does not prove a model defect or its repair. |
| [#21](https://github.com/07egta/melt3D/pull/21), `614c4e09…` | Adds persisted mesh-source metadata and cancellable asynchronous SDF rebaking, including new volume/size assignment and folding scale into the baked geometry. Project compatibility, transforms, cancellation and stale-result handling need validation of this separate feature; the frozen candidate's passing tests do not cover the proposed code. |
| [#27](https://github.com/07egta/melt3D/pull/27), `406417d1…` | Adds beginner mode with hidden docks, while the unchanged palette's library/outliner commands only toggle `lib_open`; they do not enable `show_scene_dock`. The source therefore exposes a hidden-dock command-path risk requiring reconciliation and interaction checks. The frozen candidate's narrower SVG-dialog fix does not constitute adoption of this responsive UI. |
| [#28](https://github.com/07egta/melt3D/pull/28), `bba0f6b1…` | The indexed palette's `action_key(Insert)` calls `library::all()`, conflicting with PR52's lazy category loading. Its n-gram fallback also omits some existing ordered-letter matches, such as `shr ba` against `sphere ball`. These source-derived allocation/search-parity risks need resolution; the existing F-key bilingual palette is already present, while Ctrl+K/favorites/indexing remain additional features. |

Exact heads, paths, feedback scope and evidence limits are retained in
`pr8-pr21-integration-audit.json` and `remaining-ux-pr-triage.json`. All four heads
were open drafts based on `fff80f2…` when read. Their titles, added tests and reported
mergeability do not establish readiness for the current PR52-derived candidate.

- Further unexecuted source review: [#23](https://github.com/07egta/melt3D/pull/23), `4a1957ac…`, base `793e4869…` (integration branch): duplicate inherent `App::run_action` definitions and inherited `holes`/`sketch_holes` API dependencies absent from the candidate require reconciliation. Not integrated; evidence: `contextual-ui-pr-triage.json`.
- Further unexecuted source review: [#25](https://github.com/07egta/melt3D/pull/25), `a758c9c6…`, base `fff80f2…`: successful sketch notifications can be classified as errors, and truncated status text is not restored by its generic hover text. These are source-derived concerns, not executed failures; not integrated.
- Further unexecuted source review: [#26](https://github.com/07egta/melt3D/pull/26), `d80989ff…`, base `fff80f2…`: inspector moves bypass the destination-lock check, and the changed Shift selection path conflicts with the retained group-selection help. These are source-derived concerns, not executed failures; not integrated.

## Validation gates and remaining evidence

The initial functional checkpoint completed the MeltCAD suite with **573
passed, zero failed, 39 ignored** in 24.58 seconds. The final standalone source
identified above has now independently passed the same **573 / 0 / 39** suite,
including all ten required regressions after the GPU allocation correction.
These remain package-level checks; the separate completed local workspace-CI
result is recorded below.

CI attempt 2 ended at its **110-minute watchdog with exit 124**. The 39 native
test executables that completed reported **2,924 passed / 0 failed / 85 ignored**.
That subtotal includes the kernel's **587 / 0 / 29** result; it excludes the
unfinished engine executable and doc tests. Engine had no final result, and the
Chrome offline/update suite was not reached. The complete CI gate did not pass.

The same attempt passed formatting, attribution for 87 assets, licences,
layering across 16 crates, workspace/all-target Clippy with `-D warnings`, native
test-binary compilation, the 11-crate wasm check and doc tests. Doc tests have a
successful gate result but no retained aggregate count. The action-matrix suite
reported **19 / 0 / 8**; individual action completion/timeout counts are unknown
because its optional diagnostic output was not retained. A suite PASS is not a
claim that every individual action completed. `full-xtask-ci-attempt2-results.json`
retains the gate statuses, source/input verification and matching raw-log hashes.
Keep that CI2 snapshot separate from the final standalone build above.

CI3 then **failed at all-target Clippy with command exit 1** (inner Clippy exit
101), reporting `clippy::chunks_exact_to_as_chunks` in `apps/meltcad/src/gpu.rs`
at the group-bounds iterator. Formatting/assets/licences/layers passed; native
test compilation/execution, wasm, doc tests and Chrome were not reached in this
attempt. The 170-minute watchdog did not trigger. Source remained unchanged,
and `full-xtask-ci-attempt3-results.json` preserves the exact input and log checks.

The equivalent iterator correction is included in `66cf7751…`; that new source
passed the standalone suite and has its own measured performance series above.
Prior images remain attached to their original binary.

**CI4 passed and its evidence is closed and verified.** `cargo xtask ci` completed
in approximately **152 minutes 26 seconds**, ending at **2026-10-18 04:03:19 UTC**,
within its 170-minute watchdog. Command, tee and tool exit codes were all zero;
the raw log ends with `ci: all gates passed`. All **40 native test executables**
completed, totaling **4,102 passed / 0 failed / 90 ignored**. Formatting,
attribution for **87 assets**, licences, layering across **16 crates**,
workspace/all-target Clippy with `-D warnings`, native test compilation, the
**11-crate wasm check**, workspace doc tests and Chrome offline/update checks
all passed. Doc tests have a successful command result but **no retained
workspace aggregate count**; the last crate's 0/0/0 is not a total. Existing
ignored tests remained ignored, without new filters or supplemental reruns.

Chrome executed the existing **stand-in app with actual registration and
service-worker code**: **19 scenarios** and **21 validated page launches along
the successful path**. This is not a production web-application UI test. The
existing `action_matrix_sample` retained **1,082 outcomes**: **109 successful
design loads** plus **973 action/edit rows (928 success, 45 false)**. Every false
row exactly matches a known-failure entry in the preserved baseline; new
prior-success failures, unregistered failures, timeouts, crashes and unknown
results were all zero. This is the default sample ratchet, not the full nightly
matrix or a claim that every action succeeded.

The before/after verification found no path, type, executable-mode, size or
hash changes in **1,453 source files / 79,953,096 bytes**. Their content matches
source digest `01d0288f…`; standalone binary `66cf7751…` and metadata `d11e536d…`
also remained unchanged. All closure verification checks passed. The historical
compiled-source snapshot records type/size/hash but no executable modes, so its
historical modes cannot be certified retroactively. Current modes were checked
against frozen CI3, the tracked Git index and the CI4 before/after manifests.

The terminal record is `full-xtask-ci-attempt4-results.json` (SHA-256
`aab8760cf4ccb4f64da50ca35c118165cef34b8844d6bd0143a0ccb76462922a`).
Its **9,947-byte** raw log, `full-xtask-ci-attempt4.log`, has SHA-256
`3aae796c0313c865c8d5c0cab684ecd5c4d2565f5128789c85e7c1ea9ba70698`.
These local CI results do not establish remote GitHub checks or Windows runtime
behavior. Ordinary release artifacts and final native UI have their own separate
closed results in this report.

The quality workflow requires all four jobs before manually requested draft
preparation; no release publication is recorded here.

**Windows MSVC cross-check passed; Windows execution remains unverified.** One
Linux-hosted MeltCAD check completed with exit 0 in **178.396 seconds**, using
the final source, all targets, the existing `ci` profile, locked offline
dependencies and two build jobs. No retry or additional installation was used.
The recorded invocation, with the host-specific Cargo path removed, is:

```sh
cargo check -p meltcad --all-targets --target x86_64-pc-windows-msvc --profile ci --locked --offline --jobs 2 --message-format=json
```

The compiler emitted **194 artifact messages** describing **212 verified files**,
including two MeltCAD `.rmeta` outputs for the ordinary and test configurations.
These are check outputs: **Windows application linking, test execution, runtime
and GUI behavior were not exercised**. The Windows feature/profile records are
retained separately; they are not asserted identical to the Linux test graph.

All close checks passed. The **1,453 source files / 79,953,096 bytes**, their
current modes, Cargo.lock, final metadata `d11e536d…`, test binary `66cf7751…` and
CI4 result remained unchanged. The historical compiled-mode limitation above
still applies. The **20-file evidence payload / 880,143 bytes** was copied and
hash-verified. `windows-validation/metadata.json` has SHA-256
`72ee35edcbde610109f13e37aa6595b652c48233b5fe8678a81ff6b3acf214df`;
`windows-validation-close-results.json` has SHA-256
`defd01982b55965f60bdee8d8280cbcddbdddf3bd7edd70ba4cc4613a952c77f`.

**Ordinary Linux release builds and executable-size comparison completed.** Both
variants used the same locked offline command, compiler and environment, with
two build jobs:

```sh
cargo build -p meltcad --bin meltcad --locked --offline --release --jobs 2 --message-format=json
```

The unchanged release profile uses optimization level 3, thin LTO, 16 codegen
units (1 for MeltCAD), line-table debug information and no stripping. All **218
normalized dependency feature/profile records** match between the two builds.
The candidate emitted **zero compiler diagnostics**; the baseline's **seven
warnings** remain recorded separately.

| Measured file data | Rebuilt PR #52 `153904d…` | Candidate | Change |
| --- | ---: | ---: | ---: |
| Whole unstripped ELF | 99,944,296 B | 100,033,936 B | **+89,640 B (+0.08968996%)** |
| `SHF_ALLOC` file sections | 21,219,815 B | 21,248,610 B | +28,795 B (+0.135699%) |
| Non-allocated debug sections | 73,315,232 B | 73,366,102 B | +50,870 B |

The baseline ELF SHA-256 is
`b365fc146172afbc2588a83c5beafc073776978fc7319247c360d94afb349983`;
the candidate ELF SHA-256 is
`1b5136bf91ebbcb49b0a38945c82cdfad4855cf0cddb484a8df54b482979f364`.
These are the newly built comparison files; older release measurements are not
substituted for this baseline. The executable became slightly larger.

The absolute checkout paths have different lengths, so debug/line-table strings
can affect the whole-ELF difference; the entire delta cannot be attributed solely
to implementation changes. These section sizes are file metadata, not process
RSS, packaged/download size or byte-reproducible-build evidence. The recorded
build durations, **405.47 s then 208.08 s**, came from baseline-first serial
builds sharing a Cargo cache; they are not a build-speed comparison.

All **17/17 close checks passed**. The candidate's 1,453 source files, modes,
lockfile, test binary `66cf7751…`, metadata `d11e536d…`, and closed CI4/Windows
records stayed unchanged; the baseline's blobs, modes and tree matched PR #52.
The **29-file / 203,342,635-byte payload** was copied exactly, including both
preserved ELFs. Executables are excluded from the final evidence ZIP.
`release-size-comparison/metadata.json` has SHA-256
`87fda208524ad0ea3128feb6da7892552f5dddf71f208995ba72c74dd3763636`;
`release-size-comparison/comparison.json` has SHA-256
`a76de821f4b2a2ea2be712dfa7738a0727a2c5f39caa928bee8906ee2760e852`;
`release-size-comparison-close-results.json` has SHA-256
`9aeac0f51f51cb3ec5f2e15b4f8f00688e425185cce8e1d14f2e2b859ea1209f`.
**Production release rendering passed with exact output.** Each preserved ordinary
Linux ELF ran once through the production CPU `--render` path, outside `cfg(test)`,
using the same saved material/blend scene and arguments. The portable command is:

```sh
/path/to/preserved-release --render /path/to/scene.yura sheet.png 160
```

The four-view sheet uses tile size 160 and is **320×320**. Both fresh PNGs are
**410,018 bytes**; all **102,400 decoded RGBA pixels and all PNG bytes match**,
with zero differing pixels and zero maximum channel difference. Their common
SHA-256 is `fe4c848a08b20ae6532aa6c2129681b6919aa1a49077f88b9f31fd49af03f8b8`.
The 8,370-byte input, `render-baseline/cpu/cache_material_blend.yura`, has SHA-256
`315b3c39dd2e2d82c69080a976206d663ae89a690d0658cd290412e1a1c3e5c2`.

Both invocations exited 0, but the check also required a newly generated,
decodable PNG: the existing CLI can return zero after some input/output errors.
All **19 closure checks passed**, including unchanged inputs, source/modes and
both release executable hashes. The **8-file / 836,670-byte** evidence payload
was copied exactly. `release-render-check/manifest.json` has SHA-256
`6201f76844662a77cbd3ea7c91c3e8f43a2ece42f46c09be09d853f23d88a00c`;
`release-render-check-close-results.json` has SHA-256
`7c46d3e57716a70dedff43d74bdf8aadd322db8df83bbdfe61300bb26a0c7c18`.
This is one production CPU scene comparison, not GPU performance, native UI,
Windows runtime or all-scene validation. Logged durations are not performance
comparisons.

| Gate | Status and scope |
| --- | --- |
| Final local workspace gate | **PASS — CI4:** `cargo xtask ci` exit 0; 40 native executables, **4,102 passed / 0 failed / 90 ignored**; wasm/doc/Chrome and all other gates passed within the scope above. Source content and preserved standalone identities were verified unchanged. CI2 exit 124 and CI3 Clippy failure remain separate historical records. |
| Final standalone regressions | **PASS — 573 passed / 0 failed / 39 ignored** on lint-corrected `66cf7751…` / source digest `01d0288f…`, including all ten required cache/packing regressions. Metadata `d11e536d…` is frozen. The separate final render checks and local CI4 passed as recorded above. |
| Remote GitHub checks | **Not run for the follow-up head at the publication checkpoint.** Use PR #52 for subsequent remote run URLs and conclusions. The audited older PR #52 run failed with empty job-step arrays/no runner; the service-side cause was unavailable, so no billing/account cause is inferred. |
| Windows | **MSVC cross-check PASS** on the final source: one `cargo check -p meltcad --all-targets --target x86_64-pc-windows-msvc --profile ci --locked --offline --jobs 2`, exit 0 in 178.396 seconds, with verified outputs and unchanged inputs. **Windows linking, test execution, runtime and GUI remain unverified and unexecuted.** |
| Normal release artifact | **BUILD/SIZE VERIFIED:** both ordinary release builds exited 0 with matching settings and 218 dependency records. Rebuilt PR #52 **99,944,296 B** → candidate **100,033,936 B**, **+89,640 B (+0.08968996%)**; 17/17 close checks passed. Production rendering and UI are separate checks below. |
| Production release rendering | **PASS:** one saved scene, one invocation per actual ordinary ELF, CPU four-view 320×320 output. All PNG bytes and 102,400 pixels match; 19/19 closure checks passed. |
| Final-release native UI | **PASS within the two recorded flows:** startup approval, six-image final review, 7 post-checks and 33 closure checks passed. Two final task-report image copies are saved separately from two earlier images. Three selected GitHub attachments remain unuploaded at this checkpoint. |
| Linux software-Vulkan rendering | **PASS within the recorded scope** on `66cf7751…`: five new candidate test invocations, two reused baseline captures, twelve exact cross-version images and two representative visual inspections. Original CPU/GPU tolerance checks remain distinct from byte/pixel identity. Native UI has separate evidence above; this is not a physical-GPU performance result. |
| Evidence archive | **Publication pending.** The final archive name, SHA-256 and download link belong in [PR #52](https://github.com/07egta/melt3D/pull/52) and an external post-packaging record, not in this versioned report or files contained in the ZIP. Preserve raw tables, source/build metadata, screenshots and recovery records. |

## Reproducing the scoped checks

Run from the repository root. The ordinary suites keep memory/performance
fixtures ignored; the exact small-window interaction test is listed explicitly.

```sh
cargo test --profile ci --locked -p meltcad -- --test-threads=3
cargo test --profile ci --locked -p meltcad app::tests:: -- --test-threads=1
cargo test --profile ci --locked -p meltcad app::project_save::tests:: -- --test-threads=1
cargo test --profile ci --locked -p meltcad svg::tests:: -- --test-threads=1
cargo test --profile ci --locked -p meltcad svg_dialog::tests::many_import_warnings_are_scrollable_and_actions_stay_visible -- --exact
cargo test --profile ci --locked -p meltcad gpu::tests:: -- --test-threads=1
cargo test --profile ci --locked -p meltcad sdf::tests:: -- --test-threads=1
cargo test --profile ci --locked -p meltcad library_ui::tests::font_cache_equality_matches_value_equality_for_every_font_setting -- --exact
```

The existing rendering tests use their unchanged fixtures and tolerances.
`MELTCAD_GOLDEN_DIR` selects the GPU output directory;
`MELT3D_CACHE_GOLDEN_DIR` selects the CPU output directory. For example, run these
commands from the PR #52 checkout, then run them from the candidate checkout with
the output path component `pr52` changed to `candidate`:

```sh
MELTCAD_GOLDEN_DIR=/path/to/evidence/pr52/gpu WGPU_BACKEND=vulkan cargo test --profile ci --locked -p meltcad gpu::headless_tests::gpu_headless_golden -- --exact --ignored --nocapture --test-threads=1
MELT3D_CACHE_GOLDEN_DIR=/path/to/evidence/pr52/cpu cargo test --profile ci --locked -p meltcad cpu_render::tests::incremental_field_four_view_pixels_match_cold_geometry_materials_and_undo -- --exact --nocapture --test-threads=1
```

Check the adapter line and actual output files as well as exit status. The
`run_render_parity.py` helper enforces those checks, verifies the captured
baseline hashes and compares the twelve image outputs. The final `66cf7751…`
render run and exact-copy checks passed as recorded above; the earlier
`d93765d9…` run remains separately preserved.

The other three GPU checks are reproducible without changing their
existing assertions or tolerances:

```sh
WGPU_BACKEND=vulkan cargo test --profile ci --locked -p meltcad gpu::headless_tests::curve_gpu_distance_matches_cpu -- --exact --ignored --nocapture --test-threads=1
WGPU_BACKEND=vulkan cargo test --profile ci --locked -p meltcad gpu::headless_tests::gpu_headless_matches_cpu -- --exact --ignored --nocapture --test-threads=1
WGPU_BACKEND=vulkan cargo test --profile ci --locked -p meltcad gpu::headless_tests::gpu_headless_mesh_matches_cpu -- --exact --ignored --nocapture --test-threads=1
```

The five original failing save-state tests are under `app::tests::`:

```text
undo_after_save_keeps_reverted_work_pending_before_new_scene
redo_after_save_keeps_reapplied_work_pending_before_close
saved_pending_edit_stays_clean_after_settle_and_retains_undo
saving_during_a_drag_keeps_one_undo_and_protects_later_movement
saving_mid_drag_then_returning_to_committed_keeps_work_unsaved
```

For measurements, first build once with
`cargo test --profile ci --locked -p meltcad --no-run --message-format=json`,
select the MeltCAD test executable from the compiler-artifact record, and hash
it. Invoke that executable directly with the exact filters below and
`--exact --ignored --nocapture --test-threads=1`; do not include Cargo/compilation
in `/usr/bin/time -v` RSS measurements.

```text
frame_bench::exact_cache_update_bench
frame_bench::frame_cpu_bench
gpu::tests::incremental_gpu_pack_timing_10_100_300
gpu::tests::curve_orbit_globals_benchmark
library_ui::tests::japanese_font_cache_bench
app::project_save::tests::buffered_save_peak_memory
app::project_save::tests::streaming_save_peak_memory
```

The evidence bundle's `paired_bench.py` records paired order and load checks.
The final series uses cache/frame/GPU-pack only; the curve-uniform, font and
save comparisons retain their initial-candidate scope. `run_save_memory.py`
runs three alternating pairs, uses
`MELTCAD_SAVE_BENCH_OUTPUT` to retain files, and checks complete output with
`cmp`/SHA-256 outside the measured process. The native interaction manifest and
X11 drop fixture/script record the UI reproduction. Preserve each result's
comparison scope when rerunning it.

<!-- Evidence index: paired-bench-analysis.md; paired-bench-results/results.json; paired-bench-summary.json; paired-bench-candidate-provenance.json; paired-bench-artifact-verification.json; font-cache-results.json; pr52-build-metadata.json; candidate-tests-metadata.json; candidate-native-metadata.json; save-memory-timed/measurements.json; undo-before-fix-tmp.log; meltcad-after-fix-tests.log; svg-pr12/pr12-original-compile.log; svg-pr12/parser-results-fixed.log; ui/native-ui-evidence.json; ci-remote-review.json; pr12-ancestry-review.json; ci-lint-semantic-review.patch; perf-lint-fixes.json; full-xtask-ci-attempt2.log; full-xtask-ci-attempt2-results.json; full-xtask-ci-attempt3-plan.json; full-xtask-ci-attempt3-results.json; final-candidate-validation/metadata.json; final-candidate-freeze.json; final-benchmark-main-findings.md; paired-bench-final-results/summary.json; paired-bench-final-results/analysis.md; paired-bench-final-results/results.json; final-paired-benchmark-completion.json; render-parity/results.json; render-parity/copy-verification.json; render-parity/visual-inspection.json; lint-final-benchmark-main-findings.md; paired-bench-lint-final-results/analysis.md; paired-bench-lint-final-results/summary.json; paired-bench-lint-final-results/results.json; paired-bench-lint-final-results/all-paired-values.csv; lint-final-paired-benchmark-completion.json; lint-final-candidate-validation/metadata.json; lint-final-candidate-freeze.json; render-parity-lint-final/results.json; render-parity-lint-final/copy-verification.json; render-parity-lint-final/visual-inspection.json; render-first-use-timing-notes.json; full-xtask-ci-attempt4-results.json; windows-validation/metadata.json; windows-validation-close-results.json; release-size-comparison/metadata.json; release-size-comparison/comparison.json; release-size-comparison-close-results.json; release-render-check/manifest.json; release-render-check-close-results.json; final-release-ui/manifest.json; final-release-ui/closure-results.json; final-release-ui-plan.json. Raw evidence is retained separately from the repository report. -->
