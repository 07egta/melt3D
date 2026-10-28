# MeltCAD design and promo tools

This guide connects the five requested projects to the existing native Rust/egui app without adding them to its Cargo dependency graph.

## What is installed

| Project | Use in MeltCAD | Setup |
|---|---|---|
| [Impeccable](https://github.com/pbakaus/impeccable) | Design critique prompts and visual quality guidance. Its mechanical detector targets HTML/CSS and web views; it does not scan Rust/egui widgets. | Project Codex skill v4.7.0 is in `.agents/skills/impeccable/`. No automatic hook is enabled. The Windows engine binary downloads on first use and is ignored by Git. |
| [UI UX Pro Max](https://github.com/nextlevelbuilder/ui-ux-pro-max-skill) | Searchable UX/design reference. Use its heuristics as review questions; its UI stack list has no egui target. | Project Codex skill is in `.agents/skills/ui-ux-pro-max/`. Its Python search utility uses the standard library. |
| [Remotion](https://github.com/remotion-dev/remotion) | Isolated React video authoring and rendering. | The starter project lives in `promo/`, pinned by `package-lock.json`; it is not imported by MeltCAD. The `remotion-best-practices` Codex skill is installed for this user at `~/.agents/skills/remotion-best-practices/`. |
| [video-shotcraft](https://github.com/Vincentwei1021/video-shotcraft) | Shot recipes and an agent workflow for product promos built with Remotion. | Installed as a user-level Codex skill at `~/.agents/skills/video-shotcraft/`. It is intentionally kept outside Git because the upstream package is about 63 MB. |
| [Penpot](https://github.com/penpot/penpot) | Optional external design board for flows, panel layouts and prototypes. | Use Penpot's hosted service or follow its official self-hosting guide. The application is not cloned or bundled with MeltCAD. |

## Video workspace

The Remotion starter is separate from the Rust application. From this directory:

```powershell
Set-Location apps/meltcad/promo
npm ci
npm run dev
```

Use the Codex skill `video-shotcraft` to plan a MELTCAD promo, then the Remotion skills to implement, preview and render it. Capture the real app for product footage; the initial project is a blank starter and contains no fabricated MELTCAD footage.

The isolated starter pins `zod` 4.6.7 with an npm override because Remotion 4.0.538 currently resolves an affected earlier Zod in its Studio dependency tree. The security fix clears `npm audit`, and the blank starter passes its lint and bundle checks; Remotion still prints a version-mismatch warning because it pins Zod 4.5.4 internally. Revalidate when creating or rendering a real video and remove the override once Remotion ships the patched pin. Do not run `npm audit fix --force`, which proposes downgrading Remotion.

Before commercial distribution, check Remotion's current [license terms](https://github.com/remotion-dev/remotion/blob/main/LICENSE.md) for the intended organization and use. Keep music, typefaces, logos and screenshots cleared for the intended release.

## Existing UI is the visual authority

MeltCAD currently defines its theme in `src/style.rs`: charcoal surfaces, muted text, a green selection accent, restrained borders and short transitions. Keep those native egui tokens and the existing icon system as the implementation authority. The design skills are references; web-only patterns (responsive breakpoints, CSS, browser focus rings, hover-only controls) need translation into native keyboard, pointer, scaling and viewport behavior before use.

Generated skill caches, Impeccable's local runtime data and video render output stay out of commits. To refresh the versioned Codex skills, review the upstream change and use the repo's `skills-lock.json` with the `skills` CLI.

To install the personal video skills on another machine, run `npx skills add Vincentwei1021/video-shotcraft --global --agent codex --skill video-shotcraft --yes --copy` and `npx skills add remotion-dev/skills --global --agent codex --skill remotion-best-practices --yes --copy`.
