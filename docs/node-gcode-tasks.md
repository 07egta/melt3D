# Node graph and G-code: work split and task list

Crates: `crates/slice` (triangles to layers, walls, infill, G-code) and `crates/graph` (node graph
engine and components). Read `AGENTS.md` first; its hard rules apply here too (no unwrap / expect /
panic in non-test code, clippy `-D warnings`, layering in `xtask/src/layers.rs`).

Quick check for these two crates:

```sh
cargo test -p solidcraft-slice -p solidcraft-graph
cargo clippy -p solidcraft-slice -p solidcraft-graph --all-targets -- -D warnings
```

Be honest in docs and PRs: nothing here has been run on a real Bambu printer yet.

## Who does what
- GPT / Codex: the printing side, `crates/slice/**` and a new `crates/bambu`.
- Claude: the node graph (`crates/graph/**`), the egui node editor and the Melt 3D integration.
- Need to touch the other side's files: open an issue or a small PR first.

## Printing side, in order (one branch and one PR per item)
1. Top and bottom skins (solid layers) and a per-layer infill density.
2. Seam placement and wipe / coasting at wall ends.
3. Print order: inner walls first, outer wall last; travel optimisation.
4. A Bambu A1 / A1 mini profile: start / end G-code, bed 256 x 256 (A1) or 180 x 180 (A1 mini),
   speeds, accelerations. Mark every value that could not be verified.
5. `crates/bambu`: pack G-code into `.gcode.3mf` (zip with `Metadata/Plate_1.gcode` and
   `Metadata/slice_info.config` listing every filament used). Test by unzipping and checking.
6. Ironing, bridges, supports (later).

## Known limits
Walls use a mitre offset (sharp inner corners are poor); overlapping solids cancel (even-odd);
no skins, supports, bridges or seam control yet.
