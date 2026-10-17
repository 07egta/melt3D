# YuraCAD memory and Windows EXE size audit (candidate, not released)

Base: `easy-layout-and-melt` at `fff80f282891593ecb4523442e0a0aa31725eb29`.

## Evidence and scope

- The last visible `latest` Release asset `YURACAD.exe` was **21,927,424 bytes**
  (asset uploaded 2026-10-17 05:25 UTC). **This is not a confirmed EXE for the base SHA**:
  never substitute it for a same-commit Windows measurement.
- 15 embedded model-library JSON files are **2,613,473 bytes** in the repository.
  Whitespace only *outside quoted strings* accounts for **1,484,176 bytes**. A dry-run
  semantic JSON comparison found **15/15 equal**, giving **1,129,297 bytes** of literal
  text after minification (56.8% less). **Not yet a measured EXE size saving.**
- The 15 JSON files have different Git blob identifiers. No redundant entire model
  library file was found. Do not remove material models, icons, or rendering backends.
- Existing Release already uses `opt-level=3`, `lto="thin"`, and a one-codegen-unit
  override for `meltcad`; changing to `opt-level="z"`, fat LTO or UPX without
  side-by-side timing is unjustified. The `yuracad-small` **experimental** profile
  strips only debug info, and is not used by production builds.
- The old GPU mesh upload allocated both `Vec<u16>` and `Vec<u8>` for the
  full volume at once. At 16 million nodes this is approximately 64 MB of
  temporary allocation (in addition to the stored f32 SDF), before sending
  the data to the driver. New uploads visit the exact same binary16 values
  through a reused **1 MiB** chunk buffer. No quantization or node deletion.
- The CPU picking field, packed GPU scene representation and software frame can
  retain memory from an old scene until another operation replaces the cache.
  Now a scene-revision or document switch invalidates them immediately.
  An oversized GPU scene texture is rebuilt only after at least 4-fold shrink;
  the old CPU mirror is dropped instead of only `clear()`ing its vector.
- Existing `SharedVolume` already shares meshes through `Arc` for undo,
  copying and scene changes. It must **not** be replaced by deep copies.
- The thumbnail worker is lazy and thumbnail assets are small; aggressively
  evicting all thumbnails would slow repeat library openings. No unverified
  thumbnail eviction or runtime compressed libraries are introduced.

## Windows verification commands

On **the same Windows machine**, with the same Rust toolchain and `Cargo.lock`:

1. Build an unmodified baseline at the **base SHA**:
   `git worktree add ..\yuracad-base fff80f282891593ecb4523442e0a0aa31725eb29`,
   then in the worktree run `cargo build --release -p meltcad --locked`.
   Retain the baseline `target\release\meltcad.exe` without overwriting it.
2. On this candidate branch, run:
   `cargo fmt --all -- --check`;
   `cargo test --profile ci --locked -p meltcad`;
   `cargo build --release --locked -p meltcad`;
   `cargo build --profile yuracad-small --locked -p meltcad`.
3. Run the A/B script in PowerShell from the candidate worktree:
   `powershell -NoProfile -ExecutionPolicy Bypass -File scripts\yuracad-windows-footprint.ps1 -BeforeReleaseExe "C:\path\to\base\meltcad.exe" -AfterReleaseExe "C:\path\to\candidate\target\release\meltcad.exe" -RunGpuBench`
4. Compare the size of both **Release** builds (never compare debug with Release),
   time until the first window, idle Working Set and Private Bytes, RAM after loading
   a generated **exactly 300-shape** document, and the existing shapes300 GPU
   full/fast millisecond rows. Repeat 3 runs to see normal variability.
5. The candidate script also measures Working Set and Private Bytes on the
   **same PID after closing** the 300-shape document. The unmodified baseline
   lacks the `reset_scene` test hook, so its after-close RAM is **not measured**,
   unless an instrumented baseline is supplied with
   `-BaselineHasResetHook`. The script explicitly reports this limitation.
6. `WorkingSet` / `Private` excludes dedicated GPU VRAM; record GPU dedicated
   memory separately in Windows Task Manager or vendor tooling if needed.
   The startup clock stops at the first window handle, not the first finished
   GPU frame. The GPU benchmark includes an adapter/backend descriptor so runs
   on different GPUs must not be compared.

## Acceptance gate

- No changes to program behavior, export or SDF precision: all existing
  tests and the new exact-output, cache lifetime and 300-shape parity tests pass.
- Windows `cargo build --release --locked -p meltcad` and optional profile
  build succeed; a working EXE opens, imports, renders, saves and exports.
- Require demonstrated EXE size reduction and no repeatable >5% regression
  beyond run-to-run variance in startup or shapes300 GPU time. Reject profiles
  that trade noticeable speed or diagnostics for insignificant size gains.
- RAM must not regress at idle or with 300 shapes, and the candidate should
  release stale CPU caches when switching or closing the large document.
- Keep this as a **draft PR** until all gates pass; never publish an
  unverified Windows binary or overwrite the existing Release.

## Current status (2026-10-17)

Source inspection and static JSON-content comparison are complete. The source
changes and test cases are staged. Local Rust compilation, GitHub-hosted Windows
CI, live EXE-size A/B, cold start, idle RAM, 300-shape RAM and GPU timing are
**NOT VERIFIED**. The Actions runner has not reported a successful run.
