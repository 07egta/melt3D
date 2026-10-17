# YuraCAD (apps/meltcad) — rules for AI agents

YuraCAD is the easy-to-use CAD app in `apps/meltcad`. The repo root `AGENTS.md` / `CLAUDE.md`
describe SolidCraft; where they conflict with this file, **this file wins for work in `apps/meltcad`**.

## Target branch
- The base of all YuraCAD work is **`easy-layout-and-melt`**, not `main` (`main` is behind it).
  Branch from it, open PRs against it.
- Branch names: `feat/yuracad-<topic>-YYYYMMDD`, `fix/yuracad-<topic>-YYYYMMDD`, `perf/...`, `audit/...`.
- Never push to `easy-layout-and-melt` directly and never force-push. One topic per branch.

## Where you may edit
- Free to edit: `apps/meltcad/**`, `docs/yuracad-*.md`, `.github/` templates for YuraCAD.
- Edit only when the task needs it, and say so in the PR: `crates/**`, `Cargo.toml`, `Cargo.lock`,
  `.github/workflows/**`.
- Do not touch: `vendor/**`, `ATTRIBUTION.md` rows you did not add, `packaging/**`, other apps.
- Hot files that many PRs touch: `sketch_ui.rs`, `app.rs`, `model.rs`, `sketch.rs`, `input_tests.rs`.
  Keep changes there small and local; do not reformat or move code.

## Before you start (every time)
1. `git fetch origin` and list open PRs against `easy-layout-and-melt` (`gh pr list --base easy-layout-and-melt`).
2. If an open PR already touches the same function or file region, **do not duplicate it**: build on that
   PR's branch, or pick another task and write the conflict in your PR ("Conflicts with #N").
3. Search whether the feature or fix already exists in another branch (`git log --all --oneline -S'<name>'`).
4. Check the Project board / issue for the task and move it to "In progress".

## Tests and integration
- Must pass before opening a PR: `cargo fmt --check`, `cargo clippy -p meltcad --all-targets -- -D warnings`,
  `cargo test -p meltcad`. Say exactly what you ran; if you could not run something, write "not run".
- Every bug fix needs a regression test. Input-derived numbers are hostile (no `unwrap`, no panics).
- egui is 0.36: use `ctx.content_rect()`, `ctx.egui_wants_keyboard_input()`, and
  `Event::ModifiersChanged` (RawInput has no `modifiers`). Do not write against older egui APIs.
- Merge `easy-layout-and-melt` into your branch before the PR and fix conflicts yourself; keep both sides'
  behavior (e.g. drag edits go through `DragPoint.candidate`, validated by `sketch::validate_edit` and
  `Shape::set_sketch_outline`, plus the hole-group check).
- Integration order is decided by the owner. A PR is ready only when CI is green and the PR checklist is filled.

## UX rules (what "easy" means here)
- Fewer clicks beats more options. Defaults must work without opening a panel.
- UI text is Japanese and plain (no jargon) unless the owner says otherwise.
- Esc always cancels only the current gesture; undo is always one step per gesture.
- Nothing may crash or lose work: saves are atomic, recovery must keep working.
