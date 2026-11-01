# Resume without another upload

## 1. Restore into a new independent directory

Use Python 3.10+ and Git. From this delivery directory:

```sh
python verify_delivery.py
python restore_arclet.py ../arclet-cad-hardening
```

If a local repository already contains the pinned base objects, avoid downloading them:

```sh
python restore_arclet.py ../arclet-cad-hardening --base-repository /path/to/base-repository
```

The destination **must not exist**. The script will not edit the original repository, force-reset, commit, push or publish. The upstream push URL is disabled. The old four commits are restored from the checked bundle; the two new patches remain uncommitted on `feat/arclet-hardening-20261101`. Network reconstruction has not been executed here. Verify the complete base's toolchain pin; the historical delivery specifies Rust 1.95 or newer.

## 2. Compile before calling these fixes shipped

From the restored checkout, read its AGENTS.md and available task instructions. Do not discard unrelated changes. Run targeted checks first, keeping logs:

```sh
cargo test -p solidcraft-geom nurbs_curve
cargo test -p solidcraft-ui-egui node_editor
cargo test -p solidcraft-ui-egui node_workspace
cargo test -p arclet
cargo clippy -p solidcraft-geom -p solidcraft-graph -p solidcraft-ui-egui -p arclet --all-targets -- -D warnings
cargo fmt --all -- --check
cargo xtask ci
```

The old delivery reports pre-existing unrelated format/list failures; reproduce and distinguish them rather than silently changing unrelated applications. No new commit should claim green CI until the actual result is known.

Run the numerical oracle independently:

```sh
python -m pip install numpy scipy mpmath
python qa/numerics/verify_jets.py
python qa/scripts/test_delivery_tools.py
```

The oracle is supplemental; it cannot replace executing the Rust regressions.

## 3. Verify the close guard on the newly built app

Test OS Close and any internal Quit path with: clean starter; edited graph; clean CAD plus dirty graph; dirty CAD plus dirty graph; cancelled Save dialog; denied/disk-full write; edit during asynchronous Save; pending mesh import; Cancel on the subsequent CAD save prompt; restored dirty graph equal to the starter; and changes made after a discard acknowledgement. No failed or cancelled save may count as saved. Confirm the graph's UI and saved file remain intact after Keep editing.

Add actual egui-frame interaction tests for the new modal, not only the included state tests. Check native and browser pending-download behavior separately. Verify that marking an untouched demo saved does not mark a recovered user document saved.

## 4. Build Windows first when a Windows toolchain is available

```powershell
cargo test -p arclet -p solidcraft-geom -p solidcraft-graph -p solidcraft-ui-egui
cargo build --release -p arclet
.\target\release\arclet.exe --curve-sample
```

Use the pinned workspace's Windows dependency/setup instructions. A successful build is not a successful desktop launch. Test actual file pickers, save/load, DX12/Vulkan fallback and Intel iGPU behavior, then package the executable with required runtime files and license notices. Do not label the Linux repack as a new Windows build.

## 5. Next geometry work after P0 verification

Retain exact editable NURBS separately from display meshes. Add persisted rational-curve controls/weights and direct 3D editing tests, degree/knot shape-preservation properties and rebuild deviation bounds. Stress B-rep operations and surface continuity before broad compatibility advertising. 3DM requires a distinct, tested adapter; GH/GHX and Rhino plugin runtimes are separate projects, not implied by a working node editor.

## Checkpoint locations

`modified-files/`: all new source versions. `patches/`: two topic-specific drafts. `recovered-baseline-files/`: 47 verified full new files from the old patch series, for offline review. `baseline-delivery/`: original four patches, thin bundle, licenses and historical logs. `qa/`: current-session reproducible numerical checks, test fixtures, screenshots and measurements. No need to ask the owner to upload the source again.
