# Melt — gravity-drip editable preset

## User flow

Open the Melt tab and choose '＋ Melt（とろける形状）を追加', or use
the ... shape menu / command palette search for Melt, syrup, or とろける.
The preset opens without modifying the scene. Change its base between sphere
and box, dimensions, drip length, diameter, count, direction (tilt and
azimuth), viscosity, random seed and base-size preservation. A CPU SDF
thumbnail previews the generated shape off-thread; Apply creates the group.

The resulting group consists of one regular Sphere or rounded Box plus
between 1 and 12 regular Curve shapes. The outliner and inspectors can edit
them independently. Select the preset group (or one of its shapes), then
'Meltの形を再編集…' to reapply the stored parametric settings.
Reapply explicitly regenerates the group's shapes and discards subsequent
manual shape edits; the editor warns before that action. Cancel is non-mutating.

## Geometry and compatibility

- Existing Curve rounded-cone SDF and the existing smooth Boolean union
  between body and its Curve drips drive both GPU/CPU viewing and export.
- A uniform gravity tilt keeps neighbouring tubes parallel in plan view.
  Seeded drip-length variation shrinks automatically when tilted, tightly
  packed models would otherwise cross. Anchors have small bounded jitter.
- Requested diameter is capped at 40% of perimeter spacing. The minimum
  radius of every generated path node is 0.8 mm (1.6 mm diameter). This is
  a practical floor, not a guarantee for every printer or export tolerance.
- Each drip has a smoothly tapered viscous neck and a round terminal cap.
  Default material uses a glossy coat and slight translucency.
- The original primitive size is unchanged when '元形状の大きさを維持' is
  selected. Turning it off reduces body height to 78% to emphasize drips.
- The group's optional melt_preset metadata is serde-defaulted, so older
  .meltcad/.yura projects and bundled library JSON still load. This field
  does not replace baked shape geometry: old files and exporters only need
  the ordinary Sphere/Box and Curve shapes.
- The preset is a standalone new group placed beside the current design.
  The base is not silently replaced or overwritten.

## Tests and validation

    cargo fmt --all --check
    cargo test -p meltcad melt_preset::tests
    cargo test -p meltcad
    cargo clippy -p meltcad --all-targets -- -D warnings

Unit coverage: deterministic seed, distinct drips with new seeds, Sphere
and Box, parameter validation and persistence, fixed source size, minimum
thickness and round ends, dense tilted 3D spacing, smooth SDF body join,
one-step Undo/Redo and re-edit, preview/export Field equality, and mesh
vertices against the same preview Field at bounded meshing tolerance.

Windows manual smoke test: create each base type, sweep direction 0..360,
tilt up to 55, count to 12, choose several seeds, edit an individual Curve,
re-edit (after acknowledging regeneration), Undo/Redo, save/reopen, export
STL/3MF, reimport them, inspect minimum wall/tip and open/non-manifold
warnings, and compare screenshot/mesh bounds. Run 0.2-0.4 mm export
tolerance when tips are near the minimum printable diameter.

Do not claim visual GPU/mesh parity or successful cargo tests until those
checks have actually run. A preview only shows the generated preset group
and uses the current global scene melt parameters; interactions with other
nearby scene groups can change the final combined Boolean surface.
