# MELTCAD model library

Most categories here are `*.json` ready-made models compiled into the app (`src/library.rs`).
These files are **generated** by Python scripts in `gen/` (one script per file).

## Parametric Designer Bag

The **バッグ** category is built in Rust (`src/designer_bag.rs`), not in a generated JSON file.
Choose **デザイナーバッグ・トート**, **デザイナーバッグ・半円** or **デザイナーバッグ・折り紙**.
The library inserts an editable group of five real SDF parts (hollow body, floor, Boolean
opening, front handle and rear handle). Select the group to edit width, height, depth,
wall thickness, handle rise/diameter/curvature and the variant. Use the left scene tree
to modify individual shapes and their materials; regenerating group dimensions updates
the parametric parts. The settings are saved in the scene's `Group.bag` field with the
ordinary shapes, so old scene files still load and editing survives save/reopen.

A rectangular Boolean opening removes the cap of the shell; two spline handles merge into
the rim with a small Smooth Union. The existing print-check dialog evaluates the generated
geometry, and the standard OBJ/3MF exporters operate on it without special-case writers.
**Physical load-bearing strength and minimum printable feature size depend on the printer
and material and are not guaranteed.**

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

## 家電10種類（2026-10-17）

`gen/appliances.json` is the authoritative compact catalogue for the ten common appliances.
`gen/tools_b.py` appends these originals to the existing 17 everyday items when regenerating
`tools_b.json`. Never edit only the generated JSON or regeneration will lose appliances.
They are fixed SDF groups (no special editor); the existing library code handles thumbnail,
insertion and save/OBJ/3MF export. The new names distinguish TV/fridge/lamp from older furniture.

Validation: `python3 tools_b.py` (rendering requires MELTCAD_BIN), then
`cargo test -p meltcad home_appliances_are_unique_searchable_and_save_round_trip`.
Export and visual rendering must also be tested with a runnable MeltCAD binary; static validation
alone does not establish actual OBJ/3MF or thumbnail success.
