# MELTCAD promo video workspace

This folder is a private, separate Remotion project. It does not add JavaScript dependencies to the Rust app.

The npm override for Zod 4.6.7 addresses the high-severity advisory in Remotion's transitive Studio tree. Remotion emits a version-mismatch warning because it pins Zod 4.5.4; lint, audit and the blank-starter bundle pass with the override. Re-check it after a Remotion upgrade and remove it when the upstream dependency includes the fixed release.

## Preview

From the repository root, run:

```powershell
Set-Location apps/meltcad/promo
npm ci
npm run dev
```

The starter composition is a blank example. Use real MELTCAD captures for product footage and keep licensed media in the project. To create the promo, ask Codex to use the `video-shotcraft` skill for the storyboard and shot recipes, then Remotion for the composition, preview, and render.

See [the tooling guide](../DESIGN_AND_PROMO_TOOLS.md) for design context and license notes.
