# YuraCAD scene-tree usability and performance checks

## Scope

The outliner keeps the current scene JSON format compatible. Existing `visible`, name, and group structure are retained; `locked` is an optional bool on shapes/groups, omitted when false. Secondary selections, folding, search and scroll position are window-only state.

The app currently limits a **saved/reopened scene** to 512 shapes and 96 groups. The 1000-shape case below is a synthetic **outliner-only** stress case, not a guarantee that 1000 objects can be saved, rendered or exported.

## Automated correctness

```sh
cargo fmt --all -- --check
cargo test -p meltcad --locked outliner_
cargo test -p meltcad --locked locking_prevents_edit_delete_reordering
cargo test -p meltcad --locked outliner_locks_default_to_unlocked
cargo test -p meltcad --locked
```

The tests cover nested group filtering while folded, Ctrl-like additive selection, Shift-like ranges, viewport-to-tree reveal, cancel/commit rename, grouping with undo/redo and JSON round-trip, inherited locks, hiding and lock undo.

## Reproducible benchmark

```sh
cargo test -p meltcad --release --locked outliner_headless_scroll_and_select_benchmark -- --ignored --nocapture
```

For each of 100, 300 and 1000 synthetic shapes, the ignored benchmark opens a fixed-height 272x460 logical-point tree viewport, repeatedly jumps to different rows, and measures:

- **scroll-jump + egui frame (p50, p95, milliseconds):** main-thread time to lay out and paint the virtualized tree, not a GPU frame/FPS measurement
- **selection-handler (p50, p95, microseconds):** model-side selection update, not input-to-pixel latency
- **materialized rows:** asserts that fewer than 80 rows are created even at 1000 entries

Do not describe those timings as physical mouse-wheel measurements. To measure actual wheel scrolling/FPS and input latency, use the real Windows build with 100 and 300 objects, record hardware, window size, DPI, renderer, GPU, CPU, and p95 frame time. For 1000, use a separate *UI-only synthetic* instrumentation path until the document cap is explicitly redesigned.

## Manual Windows scenarios

1. Expand/collapse groups and search Japanese and English names; clearing the filter restores folded groups
2. Click a tree item and then a 3D model; highlight and scroll target agree
3. Ctrl-click individual shapes, Shift-click a contiguous range; group them; Undo/Redo; save and reopen
4. Lock a shape and its parent group. Try gizmo, arrow-key nudge, Delete and inspector edits; they must be refused. Eye remains reversible
5. Double-click a tree row to frame it; use F2 or the context menu to rename, Enter to commit, Escape to cancel
6. Hide via eye; confirm the object remains in the tree and is not deleted. Test Undo
7. Drag a row to re-order and re-parent; confirm no accidental operation happens while a search is active

## Known limits

- This change does **not** raise scene serialization and renderer limits to 1000 objects.
- Legacy gizmos remain intentionally single-selection; multi-select is for organizing objects. The inspector explains this.
- Moving solids into a new CSG group can alter Boolean/melt results. Shape transforms and materials are preserved, and Undo is available; visual equivalence must be reviewed for subtractive assemblies.
- No measured benchmark results are included until the ignored test is actually executed on a known configuration.
