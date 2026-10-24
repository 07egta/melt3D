# YuraCAD launch and library performance benchmark

## Audited baseline

- The existing renderer already compiles shaders asynchronously and avoids unnecessary GPU redraws.
- Its font search was asynchronous but the UI-creation callback waited for the font thread to finish.
- The model library JSON is compiled into the binary and not read from disk at launch. On first tab entry, however, the existing panel parsed all 15 categories synchronously.
- The previous thumbnail worker was asynchronous but the panel queued even offscreen models and retained all texture handles.

## Candidate changes

- Font discovery is polled on the UI, never joined during window creation.
- Each category is parsed and sanitized on a background worker only when requested.
- Search parses just name/tag metadata on demand on the same worker, skipping full CAD group allocations until cards become visible.
- Virtualized grid uses egui ScrollArea::show_rows; thumbnail jobs are limited to visible rows.
- Thumbnail generation has a 48-job queue, 32 completed-image cap and an LRU of 96 egui TextureHandles per UI context.
- Missing/corrupt categories or thumbnail-worker failures are nonfatal.

## Reproducible Windows comparison

Create two executables using the same Windows toolchain, release/debug profile and build flags:

- Before: working branch easy-layout-and-melt at the recorded baseline SHA.
- After: this PR head.

Run on an interactive Windows desktop:

    powershell -ExecutionPolicy Bypass -File .\tools\bench-yuracad-startup.ps1 -Before C:\bench\before\meltcad.exe -After C:\bench\after\meltcad.exe -Runs 5 -Output C:\bench\comparison.csv

The script uses the preexisting MELTCAD_TEST_DIR screenshot/lib hooks for both builds, sends search text 'e' and scroll events, and records first screenshot latency, first library screenshot latency, a 100+ search-match browse trial and peak Windows resident working set in MiB. The audited assets have 205 models; 183 names/tags match 'e'. Search focus depends on DPI, so confirm the field actually contains 'e' and adjust SearchY/ScrollY if necessary. A failed search-input run is invalid, not an optimization result.

The first library screenshot measures visible frame responsiveness; it does not mean every thumbnail has finished.

Run five warm repeats in alternating baseline/candidate order, same monitor scale, power plan, SSD, GPU backend, virus-scanner state, and no overlapping background updates. For true cold starts, reboot between isolated runs: starting a new process does NOT flush OS file caches. Collect 3+ rebooted samples per build and report the median and range.

For additional instrumentation launch a console/debug build with MELTCAD_TIMING=1 to log first UI frame and optional performance marks. Record process peak working set separately from GPU VRAM. Do not report made-up timing or memory values.

## Quality gates

    cargo fmt --all --check
    cargo test -p meltcad
    cargo clippy -p meltcad --all-targets -- -D warnings

Check first launch, reopen, search across all categories, scrolling through 100+ matching entries, thumbnail cache eviction/revisit, failed asset handling and normal model insertion. Manual Windows desktop validation is required before merging. The automated script is a measurement aid, not proof of correctness.
