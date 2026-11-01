# Arclet CAD — independent NURBS CAD development checkpoint

**This branch is a temporary, isolated source checkpoint hosted in an existing GitHub repository because the connected GitHub API cannot create a new repository. It does not change the Melt3D default branch.** Clone this checkpoint or migrate it to a dedicated `Arclet-CAD` repository before public releases.

This checkout stores six complete Git-format source patches, original license notices, numerical oracle, validation records, and a reproducible restoration tool. The patches are not proof of a fully working CAD release. The last two patches are **not compiled or CI-verified** as of 2026-11-01. The baseline Linux executable is not included in Git to avoid committing large binary artifacts.

## Restore full source (into a new directory)

```sh
python3 tools/restore_from_patches.py ../arclet-cad-source
cd ../arclet-cad-source
cargo test -p solidcraft-geom nurbs_curve
cargo test -p solidcraft-ui-egui node_editor
cargo test -p arclet
cargo build --release -p arclet
```

The restorer pins the original parent source commit `a309790ac8e265b918da09474a02b069609ab3b5` and applies patches 0001 through 0006 sequentially. It does **not** change the original repo or auto-push anything. Network access to download the pinned base commit is required unless `--existing-base` is used.

## Scope and provenance

Arclet is developed as a separate app (`apps/arclet`) and draws from the open licensed code in the pinned base. Avoid using proprietary CAD code or making unverified Class-A, G3-surface, 3DM, Grasshopper or plugin compatibility claims. See `COMPATIBILITY_MATRIX.md`, `DEVELOPMENT_STATUS.md`, `TEST_RESULTS.md`, and `LICENSE-*`.

This is a **source checkpoint**, not a validated Windows/macOS/Linux production release. Native testing and CI of new changes are the next required milestones.
