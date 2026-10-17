# YuraCAD: Incremental SDF/Boolean Geometry Performance Validation

This branch changes **preparation/packing caches**, not the analytical SDF formulas,
Boolean combine functions, export tolerance, meshing resolution, or the STL serializer.

## What is cached

- CPU `Field::reusing`: exact `Shape::PartialEq` checks reuse unchanged `Prepared` objects;
  entire unchanged source groups reuse acceleration metadata. Changes to melt
  mode/amount invalidate melt-dependent bounds. Source groups of linked copies
  follow source changes; transforms, Boolean order and visibility are re-resolved.
- GPU `ScenePack::reusing`: untouched shape payloads reuse packed values with
  updated polygon/path pool indices. Source-group bound/block metadata is shared
  only for unchanged shapes and compatible melt settings. Unchanged curves
  reuse their path payload only when the pool prefix (and therefore adaptive
  tessellation budget) is unchanged; otherwise they are repacked. Imported
  mesh payloads reuse only when volume texture layout is identical; otherwise
  they are repacked.
- Only the **last** CPU field scene and GPU pack scene are cached; no growing
  history/LRU. Pointer edits create a new revision, and Undo/Redo advances it.
- Dragging/geometry editing uses existing low-cost fast rendering and returns
  to the full-quality frame after settling. Export intentionally builds a
  fresh analytical `Field` and never consumes a preview cache.

## Commands (run on the same machine, in Release mode for timing)

```sh
cargo test -p meltcad
cargo test --release -p meltcad incremental_field_timing_10_100_300 -- --ignored --nocapture
cargo test --release -p meltcad incremental_gpu_pack_timing_10_100_300 -- --ignored --nocapture
```

The two timing tests print mean milliseconds per edit for the cold rebuild
and incremental rebuild over 30 edited scenes at **10, 100, 300 shapes**, plus
their ratio. GPU pack timing is **CPU-side list construction**, not shader time.
For actual interactive GPU FPS/frame latency and Intel Arc results run the
existing `meltcad --bench` and F3 measurements before and after this branch,
keeping screen size, adapter, scene and renderer identical.

## Geometry and accuracy gates

- `incremental_field_rebuilds_only_changed_geometry_and_never_stales_on_undo`
  compares every measured SDF bit, material and picked shape against cold field
  construction at 10/100/300 shapes, including 0.8-mm hollow walls.
- `melt_material_visibility_and_group_link_dependencies_invalidate_safely`
  checks melt, material, visibility and Boolean changes.
- `incremental_gpu_pack_is_byte_identical_for_10_100_300_boolean_thin_and_path_shapes`
  asserts exact equality of the packed shader input versus the old cold packer;
  it includes additions, deletions, Boolean edits and undo.
- Run existing `export_job` / `export_mesh` / `export_formats` tests,
  including STL binary readback, thin gaps, manifold geometry and tolerance
  tests, and GPU/CPU field parity. Any difference blocks merging.
- Open YuraCAD on Windows and drag a shape, then release it. Check that
  the live image uses fast mode and that the settled picture reflects the
  exact final edit. Verify undo/redo, save/reload, and STL on the same scene.

## Measurement record

**Not yet measured in this environment:** it does not have Rust/Cargo or
a Windows graphics driver. Do not replace the entries below with estimates.

| Object count | Cold SDF prepare (ms/edit) | Incremental SDF prepare (ms/edit) | Cold GPU pack (ms/edit) | Incremental GPU pack (ms/edit) |
|---|---:|---:|---:|---:|
| 10 | pending | pending | pending | pending |
| 100 | pending | pending | pending | pending |
| 300 | pending | pending | pending | pending |

If a benchmark is slower (for instance due to reindexing all shapes or to
copying a very small scene), investigate before promoting the change. This
optimization is expected to benefit repeated single-shape edits most.
