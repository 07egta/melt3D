# Organic Blob（ぷっくりブロブ）

## What this preset does

Open **ライブラリ → ＋ Organic Blob**, or **形の追加 → … → プリセット**, or search
`organic blob` / `ぷっくり` in the command palette. The card shows:

- **ふくらみ**: how far the lobes protrude from a sphere
- **凹凸・ばらつき**: how many lobes and how varied their radii and directions are
- **丸み・なめらかさ**: the existing soft-union SDF blend radius
- **縦横比（高さ／幅）**: vertical stretch, represented as regular sphere scale
- **シード**: deterministic generated variation (the next-seed button steps forward)

Default values are tuned for a lavender, rounded, asymmetric shape. The generator is
deterministic for a given settings tuple on a given implementation/runtime.

## Implementation and editability

`src/organic_blob.rs` generates a **single standard editable Group** containing
a base sphere and 9–19 smaller sphere lobes. Their directions follow a seeded
Fibonacci sphere with bounded jitter; every lobe overlaps the base. Each lobe
uses the existing `Shape::blend` and the existing `sdf::smin` in CPU and WGSL
shader paths. No new SDF primitive, custom mesh kind, shader branch, hidden
mesh, plugin dependency or new on-disk schema is required.

Press **ブロブを追加** to insert the group and select it. The user can move,
rotate, scale, hide, rename, duplicate or delete its native component shapes
and use the normal Melt/Boolean tools. Changes are ordinary scene edits
and participate in the existing Undo/Redo snapshots and native JSON save.
Adjust the parameters in the **preset card before adding**; after adding,
edit the resulting spheres in the normal inspector. This is not an
automatically regenerating parametric history feature.

The preset relies on the scene's **global melt amount and mode**; if the
amount is 0, soft joins are turned off for *all* shapes. The dialog warns
about this rather than silently changing existing scene settings.

## Lightweight preview

The settings card makes an actual CPU SDF thumbnail using the existing
`cpu_render::thumb` pipeline, not a fake flat sketch. It runs **off the UI
thread**, at just **96×96 pixels**. Work is started after **140 ms** without a
parameter change; at most **one preview worker** exists per dialog. Results
for obsolete settings are discarded and failed previews do not cause repeated
worker creation. Editing the sliders only changes dialog state; it does
**not** change the live model or trigger full-resolution SDF meshing or
3D printer export. Applying performs one ordinary scene edit and the normal
viewport cache handles refresh. This thumbnail has simple lighting and may
look different from the final GPU glass, glow or studio environment.

## Verification

Use the source branch `feature/organic-blob-preset-20261017` for checks.
Keep the base revision and known-good release untouched until verified.

```powershell
cargo fmt --all -- --check
cargo test --locked -p meltcad --release organic_blob -- --nocapture
# Full end-to-end export test (ignored in regular test runs to avoid a long mesh job).
cargo test --locked -p meltcad --release organic_blob_mesh_exports_roundtrip -- --ignored --nocapture
cargo test --locked -p meltcad --release
# Existing CPU/GPU agreement tests (adapter-dependent):
cargo test --locked -p meltcad --release gpu_headless_matches_cpu -- --ignored --nocapture
# For normal export timings and GPU frame times:
target\release\meltcad.exe --bench blob-gpu-report.txt --frames 12
```

Review the dialog in the Windows UI. Check 0/1 extremes of every slider,
repeated clicks of `別の形`, editing a resulting lobe with a gizmo,
Undo/Redo of adding and editing, saving/reloading `.meltcad` and `.yura`,
STL/3MF export and re-import, printing of holes/walls, and no detached
pieces. As an image check compare a CPU thumbnail and a normal GPU viewport
silhouette; these have intentionally different material shading.

### Measurement and validation status (2026-10-17)

**Unverified on hardware.** The source edits and tests were authored using
the GitHub connection. The environment used for this change cannot run Rust,
Windows GUI or Intel Arc 140V graphics measurements. No compilation pass,
benchmark, visual diff or measured performance gain is claimed. The change
should remain a draft pull request until CI, Windows screenshots and the
export reimport test pass. If a preview cannot be built it must leave the
editor and preset insertion usable.
