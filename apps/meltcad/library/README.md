# MELTCAD model library

Each `*.json` here is one category of ready-made models, compiled into the app (`src/library.rs`).

**Designer Chair** is an intentional exception: its three parametric presets are built in `src/designer_chair.rs` and appended to the same `library::all()` item collection. This avoids duplicating generator math between JSON data and the on-screen controls. They use the same asynchronous CPU thumbnail pipeline and native scene insertion as the JSON items.
The files are **generated** by Python scripts in `gen/` (one script per file) so a model is a few
lines of readable code.

## File format

```json
{ "category": "どうぶつ",
  "items": [ { "name": "ねこ", "tags": ["cat", "ネコ", "pet"], "group": { ...a MELTCAD Group... } } ] }
```

A `group` is exactly what a `.yura` scene holds in `groups[]` (see `src/model.rs`: `Group`, `Shape`,
`Material`). `gen/lib.py` builds them: `shape(...)`, `group(...)`, `item(...)`.

## Rules for every model

- Units mm, **Z is up**. The model **stands on the ground plane z = 0**, centred on x = 0, y = 0,
  **facing -Y** (its front looks toward the viewer of the default front view). Typical height 40-120 mm.
- At most **24 shapes** per model (the whole scene holds 64). Fewer is better: 6-16 is typical.
- Names and tags: `name` is Japanese, hiragana/kana friendly ("ねこ"). `tags` add English words and
  kanji so search finds them ("cat", "猫"). Names are unique inside a file.
- Every number in the allowed range (`Scene::sanitize` must change nothing): size 0.01-2000,
  pos +-5000, scale 0.01-100, blend 0-200, outline at most 64 points, path at most 32 nodes, colours 0-1.
- The group's `op` is Add. The first shapes are `Add`; use `Subtract` for holes/eyes sockets/hollows
  and `Paint` for colour only (stain) after the shapes it should colour.
- **Soft modelling**: shapes in a group melt together by `blend` mm (6 default). Use 3-8 for soft
  joins (limbs, heads), 0-1.5 for hard details that must stay crisp (eyes, buttons, blades).
  Use `Capsule` for limbs and fingers, `Sphere`/`Hemi` for heads and bellies, `Box` with `round` for
  rounded boxes, `Revolve` (profile x = radius, y = height) for vases, cups, bottles, `Curve` for
  tubes/cables/tails, `Extrude` for flat shaped parts, `Loft` for tapered shapes.
- Give things sensible colours and materials (roughness, metallic, glass, coat).
  Look at `src/materials.rs` for good values.
- Make it recognisable first, cute second, detailed last. Look at the rendered picture and fix it.

## How to check a model (no GPU, no cargo build needed)

```
export MELTCAD_BIN=/path/to/meltcad          # given in your task
cd apps/meltcad/library/gen
python3 animals.py                            # writes ../animals.json and renders /tmp/lib_preview/*.png
```

then open the PNGs with the Read tool: four views (3/4, front, right, top). Iterate until each model
looks like what its name says. Do **not** run `cargo`.
Only edit your own `gen/<file>.py` and `<file>.json`.

## Things learnt the hard way

- Revolve profiles are centred on `pos` in Z; Box/Cylinder `pos` is the centre of the shape. A 4-sided
  Pyramid shows a corner toward -Y unless rotated 45 degrees about Z.
- A small part melted into a neighbour of a different colour takes a mixed colour: keep details
  (hair, eyes, buttons) at blend 0.5-1.5.
- The headless preview ignores `Paint`, the app does not. Colour with real shapes when in doubt.

## Designer Chair / three editable procedural variants

Search **Designer Chair** or switch to the **Designer Chair** library tab. Choose **Shell**, **Four-Leg**, or **Monocoque** to insert a fully editable group. The scene tree contains individual `座面` (Loft), `背もたれ` (Extrude), and `脚 1`…`脚 4` (Curve) parts. Side rails and a base are extra native SDF shapes on applicable styles. All geometry is in millimetres, Z-up, sitting at z≈0.

With the **group** selected, the right-hand Shape tab shows the parametric Designer Chair controls: seat height, width, depth, back tilt, leg count 1–4, leg radius, and overall rounding. The **individual shapes** can be independently selected, repositioned, recoloured, and edited using the ordinary inspector. Whole-chair sliders recalculate procedural geometry; any non-procedural shapes the user adds are retained. Numeric changes keep matching generated-part materials/visibility; switching variant selects its default palette. Undo/Redo is the usual native scene history.

The group's `designer_chair` JSON field stores the settings; the shapes themselves (including modified per-part geometry) are serialized normally. This means files open without needing a new mesh type. The existing SDF field and mesher handle the **3MF/OBJ** export. Existing library thumbnails remain 62×62 RGBA, generated once per item off the UI thread. Avoid asserting structural strength or physical safety from these decorative presets.
