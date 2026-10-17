# Puffy Text / ぷっくり文字

## Usage

1. Click **Aa** in the left tool strip, or choose **File → Puffy Text（ぷっくり文字）**.
2. Type up to 24 Japanese/English characters, including ordinary spaces.
3. Choose an installed **TTF / OTF / TTC** file, or click the Japanese-font finder. For font collections, choose the appropriate face index.
4. Set em size, thickness, puff, rounding, tracking and left/center/right alignment, then create.
5. Each visible character is a separate **group**. Select the *group*, not just an inner counter's subtractive shape, and use the existing gizmo to move, rotate or scale the letter independently.
6. Select any Puffy Text character/group and choose **Puffy Text の文字・設定を再編集** in the inspector. Edit text or parameters, and **文字を更新** regenerates the run while preserving per-index manual translation, rotation and scale where possible.
7. Save normally. Letters are ordinary persistent **Extrude** SDF shapes with `Add` outlines and `Subtract` holes. There is no baked-mesh dependency and all existing exports run through the unchanged shared mesher.

## Implementation / safety

- Reuses the existing `ttf-parser` dependency and CJK installed-font discovery (`fonts.rs`).
- Emits only safe numeric glyph path commands, and feeds them to the existing SVG curve flattening, contour nesting and even-odd fill pipeline (`svg.rs`); **does not** introduce a new polygon library, shader kind, geometry kernel or exporter.
- Stores source text, font path and parameters as optional backward-compatible metadata on each character group. The saved scene already contains all outlines, so it still opens and exports when a font is removed or the file path moves.
- `Extrude.params[0]` (formerly unused, default zero) represents signed 3D SDF inflation. The same signed distance offset is applied in CPU and WGSL code; positive on outer outlines, negative on inner hole cutters. The existing rounded extrusion primitive provides edge softening.
- Preflights requested characters and glyph outlines, hole clearance, 512-shape / 96-group limits, finite ranges, and read size. If any piece is invalid, **the whole update is refused and the prior scene is retained**.
- Regeneration happens **only on the explicit Create/Update button**. No font parsing or expensive geometry rebuild happens on ordinary UI repaint or during a letter gizmo drag.
- Puffy Text metadata is not a license grant. **Never bundle or commit system/user font files**, and check the font's EULA/OFL for commercial use and redistribution of outline-derived models.

## Known limitations

- Requires the selected font to contain the characters. Full typographic shaping (ligatures, bidirectional shaping, combining sequences and advanced OpenType kerning) is not implemented; input is a single horizontal line.
- Oversized, overly intricate, tiny-counter or missing-glyph inputs return an error rather than silently corrupting letters. Narrow counters can require puff/rounding = 0.
- Character counts and scene memory are constrained by existing GPU/SDF scene limits. Multiple contours of a glyph consume individual shapes.
- Each character has its own editable group; editing one constituent outline manually and later regenerating text replaces that manual contour edit, although group-level moves/rotations/scaling are preserved.
- Moving characters into another order or changing the character count can map saved per-index transforms to the new character at that index.
- Mesh export makes a watertight model only when the shared SDF mesher succeeds at sufficient tolerance; model topology and minimum stroke thickness must be checked for the intended printer.

## Validation

Static code review alone is not a passing build. Run:

```sh
cargo fmt --all -- --check
cargo test -p meltcad --locked ring_counter_survives_puffing_sdf_save_and_reopen
cargo test -p meltcad --locked invalid_settings_and_too_much_puff_never_mutate_a_scene
cargo test -p meltcad --locked individual_letter_transform_survives_rebuild
cargo test -p meltcad --locked card_can_open_without_a_font_and_cancel_does_not_change_the_scene
cargo test -p meltcad --locked
```

On a machine with a licensed CJK font installed:

```sh
cargo test -p meltcad --locked if_an_installed_cjk_font_exists_japanese_english_can_be_regenerated -- --ignored --nocapture
cargo test -p meltcad --locked inflated_ring_is_watertight_after_mesh_export -- --ignored --nocapture
```

Then perform a Windows GUI roundtrip: generate **O / B / 田 / 日 / Puffy**, observe their counters, drag/rotate characters separately, save/reopen/re-edit, and export STL and 3MF with a suitable fine tolerance. Re-import the exported mesh and inspect closed edges, hole continuity and thin wall warnings. Compare exported and viewport silhouette. Mark anything not run as **unverified**.
