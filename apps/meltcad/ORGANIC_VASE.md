# MeltCAD: Organic Vase (有機的な花瓶)

## Using the presets

Open **ライブラリ → 花瓶・Vase** and choose one of the three designs:

| Preset | Intent | Height | Nominal body diameter | Mouth | Base | Wall | Angular waves |
|---|---|---:|---:|---:|---:|---:|---:|
| Flow | balanced ceramic form | 132 mm | 92 mm | 49 mm | 54 mm | 2.4 mm | 5 |
| Ripple | tall, strongly undulating | 155 mm | 102 mm | 46 mm | 51 mm | 2.8 mm | 8 |
| Pebble | rounded, low form | 104 mm | 115 mm | 71 mm | 76 mm | 2.8 mm | 6 |

The newly inserted vase is selected automatically. In **形** use the Organic Vase controls for height,
body/mouth/base diameter, wall thickness, side undulation, waist, roundness, circumferential
wave count (0-16), and wave strength. The three quick-design buttons reset these parameters.
Values stay editable after saving/reopening; the existing project's `Revolve` remains fully
compatible with older shapes.

## Geometry and print design

- Uses `Kind::Revolve` and its existing CPU/WGSL profile distance field; no new mesher or Boolean
  engine. Shape outline has at most 63 vertices, regardless of preview fidelity.
- Closed 2D profile includes the exterior wall, an elliptical rounded rim, an interior wall,
  a **closed bottom floor**, and an axis segment which Revolve deliberately does not render as
  a cylinder. The mouth is open at the center.
- Angular waves remap the radius `r` before the existing profile SDF is evaluated; the
  same wave field displaces both interior and exterior walls. A conservative distance
  factor reduces sphere-tracing oversteps. At wave count 0, the original Revolve SDF is unchanged.
- `Shape::shell` stays **0**: generic shelling would hollow the base, so the thickness comes
  from the generated wall profile, not a hollow-sphere/subtract trick.
- Object bounding spheres include maximum wave amplitude, preventing missed geometry during
  culling and mesh generation. Position changes and height edits keep the vase's world-space
  base at the original height.
- Shapes are parametric `vase` metadata with Serde defaults. On project loading the
  sanitized parameters regenerate the exact profile; older files omit this optional field.

## Wall warning

The inspector estimates the minimum physical wall thickness and shows the approximate height
of the thinnest region. The calculation accounts for wall slope, circumferential wave slope,
and the smallest object scale. Below **0.8 mm** it warns rather than silently changing
the chosen geometry. This is a **sample-based estimate**, not a certified mesh thickness
measurement: inspect the final STL in a slicer, especially with thin or high-frequency waves.
For FDM, choose an appropriately thick wall for the actual nozzle and layer profile.

## Tests and validation

```bash
cargo test --locked -p meltcad organic_vase -- --nocapture
cargo test --locked -p meltcad the_shader_is_valid_wgsl
cargo test --locked -p meltcad organic_vase_stl_is_watertight -- --ignored --nocapture
cargo test --locked -p meltcad
```

The tests check profile simplicity, bottom closure, cavity opening, parameters,
searchable library insertion, save/load compatibility, finite SDF deformation, thin-wall
warnings, and (opt-in) watertight STL output/readback for all three styles.
The complete WGSL parser/validator runs as a separate existing test.

**Verification state (2026-10-17):** Source and tests are committed.
The feature-branch GitHub Actions run `39018171166` failed before its jobs reported
any steps; no compile, shader, Windows or STL pass result is available. Do not
mark build/export accuracy as confirmed until a runner executes these tests.
The PR is draft and does not alter the published Windows release.
