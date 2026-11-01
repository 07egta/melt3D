# Arclet CAD - reproducible development checkpoint, 2026-11-01

## Do not lose or restart the project
The previous source ZIP and original Linux executable were recovered from Library. Their original files remain under `/mnt/data/arclet-recovered` in this session. This delivery embeds the previous source bundle/patches and 47 verified new baseline files. **Do not ask the owner to upload the source again.** This is not a full offline checkout: the pinned base objects and build dependencies are still required.

## Immutable source identities

- Source base: `a309790ac8e265b918da09474a02b069609ab3b5`.
- Original independent Arclet tip: `f97df2eee843baaac3e525ae70646c7827c74670`.
- Recovered bundle ref: `feat/arclet-workspace-20261101`.
- New continuation patches: `0005-stabilize-rational-curve-jets.patch`, `0006-guard-node-close-and-recovery.patch`.
- New commits / remote branches / PRs: **none created**. The patches are drafts, not CI-approved commits.

## Completed changes

1. Recovered the existing implementation rather than replacing it with another prototype. Verified all 47 complete new-file Git blob hashes from the old patch series.
2. Implemented rational-curve jet hardening: local coordinates, compensated summation, actual quotient denominator, exact stationary spans and finite-extreme recentering. Added seven Rust regression tests.
3. Implemented independent graph close protection, busy-operation handling, snapshot-specific discard and preservation of CAD save confirmation. Explicit dirty-state restoration/adoption handles equal-content graphs. Added eleven Rust state/UI tests. A later source review replaced repeated JSON serialization with borrowed graph equality against a copied snapshot.
4. Executed 7,492 independent numerical assertions against SciPy/90-digit references, with zero failures. Executed eight delivery safety tests. Both patches apply cleanly to hash-verified baseline files and reproduce the recorded source SHA-256 hashes.
5. Exercised original executable GUI workflows: Rebuild, Finish, Undo/Redo, curve and node-mesh project cold round-trips, exact 192-triangle STL record preservation, normal-exit graph recovery, four malformed-graph cases and a 2,000-node chain plus 2,001-node rejection.
6. Exercised one STEP torus round-trip with changed representation, mm units retained, finite closed STL meshes, and analytic torus vertex-distance checks. This is smoke coverage, not general interchange certification.
7. Measured five baseline sample/capture/shutdown runs: median 1.5847 seconds and median sampled peak RSS 305.34 MiB in software rendering. These are not first-frame or real-GPU measurements.
8. Repacked the original Linux executable without a stale 108,527,616-byte hidden file. Archive size decreased from 81,563,534 to 45,238,845 bytes (44.54%). Executable hash is unchanged. New patches are **not** in that executable.

## Actual limitations, not assumed completion

No cargo/rustc/rustfmt is installed and no complete base checkout could be downloaded directly. Therefore new Rust code, formatting, clippy and regression tests are **not run**. The independent numerical oracle is not a Rust test result. No Windows/macOS/browser build, native Intel GPU test, printer test, third-party Rhino round-trip, Class-A guarantee or general G3-surface validation occurred. No usable subagent mechanism or independent AI reviewer was available. GitHub connector creation/write actions were unavailable; no Melt3D remote files changed.

## Next blocking work

First restore the complete source with `restore_arclet.py`; obtain the pinned Rust toolchain; compile and run the 18 new regressions plus affected existing suites; fix any compile/lint issue; then drive the newly built close/save/cancel UI. Perform a Windows launch and file round-trip before shipping a Windows binary. See NEXT_STEPS.md for exact commands and edge cases. Do not start broad new compatibility claims before these P0 checks.

Keep the two new topics separate for review/PRs. The restoration branch combines them only for local integration testing. The old stable executable remains separately identifiable and is not overwritten.
