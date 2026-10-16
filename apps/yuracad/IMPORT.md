# MELTCAD mesh import: design note

Status: **wired** (branch `yura-mesh`). The core below is used by `Kind::Mesh`, the CPU field
(`sdf.rs`), the GPU (`gpu.rs`, `shader_mesh.wgsl`), the import card (`mesh_dialog.rs`) and the scene
file (`mesh.rs`). See "How it is wired" at the end of this note.

Everything here is our own code (no new dependencies; `serde_json` is used for GLB).

## Files

| file | what |
|---|---|
| `src/import_inflate.rs` | DEFLATE decoder (stored, fixed and dynamic Huffman) with an output cap, the zlib wrapper, a ZIP reader (stored + deflated entries, CRC checked). `testutil::zip` builds ZIPs in tests |
| `src/import_mesh.rs` | `ImportedMesh`, the STL (binary and ASCII), OBJ and PLY (ascii, little and big endian) readers, `clean`, the helpers, `import_bytes` / `import_file` |
| `src/import_3mf.rs` | 3MF: XML tag reader, objects, meshes, components, build items, transforms, `unit`, models in other files of the package (`p:path`) |
| `src/import_glb.rs` | GLB (glTF 2.0 binary): POSITION and indices of all triangle primitives, scene nodes with matrix or TRS |
| `src/sdf_volume.rs` | `Volume`: bake a mesh to a signed distance grid, sample it, save and load it |

`main.rs` has five `mod` lines for these (`import_3mf`, `import_glb`, `import_inflate`,
`import_mesh`, `sdf_volume`).

## API

### Reading a file

```rust
use crate::import_mesh::{import_file, import_bytes, ImportOptions, Imported, MeshFormat};

let imp: Imported = import_file(path, &ImportOptions::default())?;   // Err(String) is already Japanese and friendly
// or: import_bytes(&bytes, "name.stl", &ImportOptions::default())   (the name is only a hint; the first bytes win)
```

* The format is found from the first bytes (ZIP = 3MF, `glTF` = GLB, `ply`), then the extension, then
  the text. A binary STL whose header starts with "solid" is recognised by its size.
* `ImportOptions { y_up: None }` decides by format: OBJ and GLB are Y up, the others Z up (the
  same defaults as the export dialog). `Some(true/false)` forces it. Y up is turned to Z up with
  `(x, y, z) -> (x, -z, y)`, the exact inverse of the Y-up export.
* `Imported { mesh, format, turned_y_up, dropped_triangles, health, notes }`. `notes` are short
  Japanese remarks to show the person ("穴があいている所があります", "平らな形です", ...). `health`
  counts open and non-manifold edges (`health.is_closed()`).
* `mesh: ImportedMesh { positions: Vec<[f32; 3]>, triangles: Vec<[u32; 3]> }`, millimetres
  (3MF `unit` is applied; STL/OBJ/PLY/GLB numbers are taken as they are).
* Checked: finite numbers, indices in range, no triangles without area (dropped and counted), at
  least one real triangle. Welded exactly (STL corners).
* Caps: file 256 MB, 5 million triangles, 15 million vertices, unpacked 3MF XML 400 MB, component
  depth 16, 200 000 component visits, GLB 100 000 node visits. Hostile files give `Err`, never a
  panic or an unbounded allocation (counts in headers are checked against the bytes that exist).

### Helpers on `ImportedMesh`

`bounds()`, `size()`, `weld(tol)` (0 = identical points only), `y_up_to_z_up()`,
`centre_and_ground()` (middle at x = y = 0, bottom at z = 0), `scale(f)`, `scale_to_size(mm)`
(longest side), `signed_volume()`, `surface_area()`, `health()`.

### Baking a distance volume

```rust
use crate::sdf_volume::{bake_with, BakeControl, BakeOptions, Volume};

let ctl = BakeControl::new();                   // share an Arc/&ref with the UI
// on a worker thread:
let (vol, stats) = bake_with(&mesh, &BakeOptions::default(), &ctl)?;
// UI: ctl.progress.load(Relaxed) is 0..=1000;  ctl.cancel.store(true, Relaxed) stops it (Err("中止しました"))
```

* `Volume { dims: [u32; 3], origin: [f32; 3], cell: f32, data: Vec<f32> }`. Node `(i, j, k)` is at
  `origin + (i, j, k) * cell`; `data[i + nx * (j + ny * k)]`; negative inside. Units: whatever
  the mesh was in (mm after `scale_to_size`). The model is padded by 4 empty cells on every side.
* `BakeOptions { res: 128, pad: 4, max_cells: 4_194_304, threads: 0, far_band: 10.0 }`. `res` is the
  number of cells along the longest side (lowered automatically if the grid would exceed
  `max_cells` or 1024 per axis). A 128 bake gives about 137^3 nodes for a ball, fewer for thin shapes.
* `Volume::distance(p)`: trilinear inside the grid; outside it is `distance to the grid box + the
  value at the closest point of the box`, so it is continuous and keeps growing. NaN gives 1e9.
  `Volume::gradient(p)` by central differences. `Volume::bounds()` is the box of the nodes.
* `Volume::to_f16_bits()` gives the data as half floats for an `R16Float` 3-D texture (filterable
  on every GPU); `f32_to_f16_bits(x)` is the converter.

### Saving into the scene file

```rust
let text: String = vol.to_base64();      // put it in the JSON
let vol = Volume::from_base64(&text)?;   // Err on anything hostile (dims caps, lengths, inflate cap, 32 MB text cap)
// or the raw bytes: to_bytes() / from_bytes(&[u8])
```

Format `MSDF` v1: 34-byte header (version, steps per cell = 64, dims, origin, cell) + our DEFLATE of
the residuals of a 3-D Lorenzo predictor over the distances quantised to 1/64 cell (so the saved
volume differs from the baked one by at most 1/128 cell), split into low and high byte planes.

## How `Kind::Mesh` should use it

1. **Import** (UI thread asks, worker thread does): `import_file` -> `centre_and_ground()` ->
   `scale_to_size(target)` (let the person choose the size; default something like 60 mm) ->
   `bake_with` with a progress bar and a cancel button. Show `imp.notes`. A 100k triangle model
   takes 1 to 4 seconds (see below), so a worker is a must.
2. **The shape** stores `Arc<Volume>` (share it between copies of the shape; compare by a hash of
   `to_bytes()` if shapes are deduplicated on load) plus the usual position / rotation / scale. The
   volume is in the model's own space (grounded, centred): the shape's centre is the middle of the
   floor of the model, so the primitive's origin is at the bottom. If you prefer the middle of the
   box, subtract `(0, 0, size_z / 2)` in `centre_and_ground` instead.
3. **CPU (`sdf.rs`)**: map the world point into the local frame exactly like the other primitives,
   then `d = volume.distance(p_local)`; multiply by the smallest scale component so that the
   result stays a lower bound of the distance when the scale is not uniform. The existing smooth
   union / subtraction code then treats it as a normal distance. Outside the grid the value
   grows with the distance to the box, so ray marching and the blends keep working.
4. **GPU**: upload `to_f16_bits()` once per volume as an `R16Float` 3-D texture (at most ~4M
   texels, 8 MB). Texel centres are the nodes, so in the shader: `node = clamp((p_local - origin) / cell,
   0, dims - 1)`, `uvw = (node + 0.5) / dims`, sample with `textureSampleLevel` and a linear
   sampler, then add the distance from `p_local` to the box of the nodes when `p_local` was clamped.
   For several mesh shapes use one texture each (bind group array or a texture atlas); a uniform
   of `(origin, cell, dims)` per shape is enough.
5. **Scene file**: `{"kind": "mesh", "name": "...", "volume": "<base64>"}`. Loading
   `from_base64` is about 0.1 s for 128^3, saving 0.2 s. Keep the original mesh out of the file.
   A 128-cell volume is 0.3 to 1.2 MB of text; the largest (4M nodes) about 2 MB.
6. **Export (STL/3MF...)** needs nothing special: the volume is just another distance field for
   the existing mesher. It cannot be sharper than one cell (about 1/128 of the model's longest side).
   If sharp edges of the original matter, a later step could keep the original `ImportedMesh`
   (stored compressed) and export it directly when the shape is alone and unmodified.

## Measured (release, 4 cores shared with other builds, so rough)

Bake, 100k triangles, 128 cells on the longest side (`cargo test -p yuracad --release
report_timing -- --ignored --nocapture`):

| shape | nodes | bake | saved | base64 |
|---|---|---|---|---|
| ball | 137^3 = 2.57M | 2.3 to 3 s | 883 KB | 1.18 MB |
| torus | 137x137x41 | 1.3 to 1.8 s | 284 KB | 0.38 MB |
| lumpy blob | 100x137x125 | 1.7 to 2.3 s | 764 KB | 1.02 MB |
| 14k triangle blob | 100x137x126 | 0.8 to 1 s | 891 KB | 1.19 MB |
| blob, 160 cells (3.2M nodes) | 123x169x154 | 3.5 s | 1.38 MB | 1.84 MB |

The BVH build takes 0.03 s and the sign pass 0.05 s; nearly all the time is the distance pass.
`save` is 0.2 s and `load` 0.05 s. Before the far-field shortcut (`far_band`) the ball needed
10 to 15 s, which shows the cost of measuring the exact distance in the middle of a thick body.

Readers, 500k triangles (`report_read_times_for_a_big_mesh`): binary STL 0.12 s (25 MB), ASCII STL
0.47 s (100 MB), OBJ 0.16 s, PLY 0.11 s, GLB 0.09 s, 3MF about 0.5 s per 500k triangles.

## Rough edges and ideas

* **Accuracy**: nodes closer than 10 cells to the surface are exact to 0.4% of a cell (before saving;
  1/128 cell more after). Further away, blocks of 4x4x4 nodes are interpolated from exact corners:
  fine for blending, but inside thick bodies the value near the medial axis can be one to three cells
  too small. Set `far_band: 0.0` to measure every node (the ball took 10 to 15 s that way).
* **Signs**: three axis rays vote per node. A consistently oriented mesh is judged by the winding
  number (overlapping shells stay solid), any other mesh by parity (flipped triangles are fine). A
  mesh with a big hole still gets a sensible inside (an open box is inside in its middle) but the
  region right behind the opening is a guess. Very thin walls (thinner than one cell) can leak.
* A flat mesh (no thickness) bakes to an unsigned distance (everything "outside").
* Distances are in the mesh's units; `scale_to_size` before baking, so `cell` is in mm.
* OBJ: only `v` and `f` (n-gons are fanned, so a concave polygon can overlap itself); materials,
  groups, smoothing, `l`, `p` and curves are ignored. PLY: face `vertex_indices` lists only
  (triangle strips are ignored). GLB: no sparse accessors, no Draco/meshopt, only the BIN chunk
  of the file (no external `.bin`), no `.gltf` text files; metres are not converted (our own
  exporter writes the chosen unit). 3MF: beam lattice, slices, materials and colours are ignored;
  `type="support"` objects are skipped.
* Colours are not imported (the volume holds distances only). A later version could bake a coarse
  RGB volume or a single colour for the shape.
* The 3MF test build caps the unpacked XML at 8 MB (400 MB in the app) so that the zip-bomb test is light.
* `weld(tol)` with a tolerance is quantised to a grid (two points on opposite sides of a grid
  line can stay apart); STL corners are welded exactly while reading.

## How it is wired

| file | what |
|---|---|
| `src/mesh.rs` | `SharedVolume` (an `Arc<Volume>` that compares by pointer), the scene file form, the limits (`MAX_VOLUME_SHAPES` = 8 shapes, `MAX_TOTAL_NODES` = 16 M grid nodes in a scene), `can_add` |
| `src/mesh_import.rs` | read, turn upright (向き), centre, scale to the target size, bake, `place` (floor / view middle / file place), `estimate` (time, size), `shape_of` |
| `src/mesh_dialog.rs` | the import card (worker threads for reading and baking, progress, 中止), the dropped files, the inspector part of a mesh shape |
| `src/model.rs` | `Kind::Mesh` (id 20, not in `Kind::ALL` or the add menus), `Shape::volume`, `Shape::source`, `Shape::mesh`, `Scene::from_json_notes`, `sanitize_notes` |
| `src/sdf.rs` | `Prepared::volume`; `local_distance` samples `Volume::distance`, then the usual `* min_scale` and shell |
| `src/gpu.rs`, `src/shader_mesh.wgsl` | `VolumeSet` (the volumes' places), the volume texture, `mesh_dist` |

* **The shape.** The volume is baked around the model's middle (the box of the nodes is the model's box plus
  4 cells), so the shape's position is the middle of the model and the gizmo turns it about that point.
  `Shape::size` holds the model's size in mm (read only; the inspector shows it times the scale),
  `Shape::source` the file name. `local_half` is the box of the nodes seen from the origin, so the
  bounding spheres of `accel.rs` and the gizmo work unchanged. Outside the box the value is the
  distance to the box plus the value at the nearest edge point (continuous, never smaller than the true distance by
  more than the error inside; an upper bound of it, not a lower one).
* **Rounding** (`丸み`) is off for meshes (`can_round` is false): there is no corner information in a distance grid. Shell, blend, carving,
  overlap, paint, patterns, linked copies and export work.
* **Scene file.** `"volume": "<base64>"` in the shape (the first shape that uses a volume), later shapes with the same volume write its
  number (order of first appearance). Loading finds a volume that is alive in the program by the hash of its text and shares it (copy and paste
  within the program does not decode or duplicate). At most 8 new volumes are decoded per file, texts are capped at 32 MB, damaged
  data becomes "no volume" and `sanitize_notes` removes that shape and the shapes past the limits, with a note shown in the status bar.
  A 128-cell ball is a 1.25 MB scene file (save 0.2 s, load 0.06 s); a flat plate is tiny.
* **Undo and equality.** `SharedVolume` equality is pointer equality, so cloning and comparing scenes (every frame in `settle`) never touches
  the distances. Duplicate, mirror copy, array, paste, patterns and linked copies share the one `Arc`; `mesh::can_add` refuses the ninth mesh shape
  and a scene past the data limit (with a message through `App::notice`).
* **GPU.** Option (a) of the plan with a twist: the volumes do not go into the scene list texture (that is re-uploaded whenever the frame
  changes) but into their own texture, `Rgba16Float`, 2048 texels wide, four grid nodes per texel (r g b a), up to 2048 rows (= 16 M nodes,
  32 MB), bound as binding 2 and read with `textureLoad` (no filtering needed, so it works wherever the scene list does, OpenGL included;
  a 3-D texture would be capped at 256 per side on the OpenGL limits and there is one binding per volume). `mesh_dist` does the trilinear blend
  with eight loads. Volumes sit one after another (each starts on a multiple of 4 nodes, `VolumeSet::of`); per shape the record carries
  size = node counts, params = origin and cell, ext.x = first node. `ensure_volumes` uploads only volumes that are new or moved, then invalidates the
  frame cache; moving or rotating a mesh uploads nothing. Half floats have 11 bits of precision: 0.05% of the distance (0.1 mm at 200 mm), invisible next to the cell size.
* **Export.** Nothing special: the mesher samples the `Field`. A scene file saved with meshes exports like any other.

### Measured (this machine, release, software Vulkan llvmpipe)

* Ball, 128 cells: 2.57 M nodes, bake 0.7 s (4 cores), file 1.25 MB, save 0.2 s, load 0.06 s.
* `cargo test -p yuracad --release gpu_headless_mesh_matches_cpu -- --ignored --nocapture` (our own STL of a small model read back, baked,
  melted with a sphere, carved by a cylinder, a second copy at scale 0.7 sharing the volume): GPU and CPU silhouettes differ by about 2% of the covered pixels.
* End to end (`export_our_own_stl_import_it_and_measure_it_again`): the imported field is within about one cell of the original near the surface
  (worst 0.57 mm at a 0.49 mm cell; the export tolerance of 0.3 mm is part of it).

### Rough edges

* Colours are not imported; every mesh gets a clay grey and is painted like other shapes.
* The bake keeps the precision of IMPORT.md "Accuracy"; thin walls under one cell can leak and open meshes get a guessed inside.
* There is no re-bake of a placed mesh at another size or fineness (import again; the scale in the inspector only stretches the grid).
* A scale much above 1 makes the cells coarse (the cell grows with it); the inspector shows the base cell.
* Scenes with eight volume shapes cost eight times the per-step volume sampling in the shader (the bounding spheres skip far ones).
