# Organic Tube (うねうねチューブ) — MeltCAD

Organic Tube is a **preset family using the existing editable Curve shape**.
There is no separate mesh to rebake and no duplicate spline or SDF engine.

## Create and edit

1. In the left shape strip, open **… → Organic Tube** and select **波型 / リング型 / S字型**.
   The **F** search palette also accepts `Organic Tube wave`, `ring` and `S`.
2. Select the curve. Drag the visible nodes to move them in 3D; drag the small
   radius handle or change the radius of any node in the Shape inspector.
3. Use the existing **なめらか** (Catmull–Rom smoothness), **輪にする** (open/closed),
   and **ぜんたいの太さ** controls. Midpoint + creates a node; Delete removes one.
4. The Shape inspector now also exposes **先端の丸み** (1 rounded; 0 flat for
   open curves) and **断面** (丸・楕円 or やわらか四角), plus **断面の幅** (0.45 to 1).
5. Save a native scene and later reselect/reedit the same control points.

## Geometry and performance

- `Shape::kind = Curve`; the user-editable `path` is still a sequence of
  `[x, y, z, radius]` nodes and is resampled with the existing centripetal
  Catmull–Rom spline. `params[0]` is loop, `params[1]` smoothness,
  `params[2]` cap flatness, `params[3]` overall radius scale.
- Curve-only `size[1]` is the width ratio (1: original circular section).
  Curve-only `size[2]` is the section contour (1: circle/oval; 2: rounded-square).
  Other shapes still interpret these fields as before.
- For circular sections, both the CPU picker/export and GPU preview still use
  the existing `round_cone` SDF branch. An elliptic or soft-square contour
  transforms only each tube span's **transverse** distance, keeps tangent/radius
  interpolation unchanged and uses the existing local joint smoothing.
- The section is orthogonal to each sampled span. Very sharp reversals or
  multiple nearly coincident nodes can make the orientation less visually
  regular; the conservative SDF and join ensure a filled union rather than
  placing an open triangle strip at the corner.
- GPU and CPU use the same math; conservative bounding spheres are enlarged for
  the rounded-square section so it cannot be clipped by accelerator skips.
- The curve stays fully editable and inexpensive to preview; no high-resolution
  triangle mesh is generated on each mouse movement. STL and 3MF use the
  existing `export_job` CPU mesher. For fine output choose a small **セル**
  / **細かさ** (e.g. 0.25 mm) and verify the print checker reports a closed mesh.

## Regression and release qualification

Run with Rust 1.95 or later on the configured workspace:

```sh
cargo test --profile ci --locked -p meltcad
cargo test --profile ci --locked -p meltcad organic_tube_high_precision_stl_is_watertight -- --ignored
cargo build --release --locked -p meltcad
```

Tests exercise exact closed-loop endpoints, curve SDF compatibility, oval/rounded-square
cross-sections, acute bends, end caps, native save/undo, egui drag events and
closed STL/3MF mesh output. Keep the PR unmerged until the Windows build and
test results, interactive rendering, and GPU/CPU agreement are verified.
