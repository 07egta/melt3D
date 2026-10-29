# MELTCAD native UI baseline

This file records the existing app's native egui styling. It supersedes web templates from the general UI/UX references; do not apply CSS or browser-only interaction rules to Rust widgets.

Source of truth: `apps/meltcad/src/style.rs`.

## Current tokens

| Role | RGB | Hex |
|---|---:|---:|
| Deep panel / inset | 24, 24, 25 | `#181819` |
| Side panel | 31, 31, 32 | `#1F1F20` |
| Card / resting control | 44, 44, 46 | `#2C2C2E` |
| Hover | 62, 62, 65 | `#3E3E41` |
| Border | 72, 72, 72 | `#484848` |
| Main text | 230, 230, 230 | `#E6E6E6` |
| Secondary text | 158, 158, 158 | `#9E9E9E` |
| Selection accent | 52, 178, 120 | `#34B278` |
| Soft accent | 34, 118, 82 | `#227652` |
| Add | 120, 214, 150 | `#78D696` |
| Cut | 244, 124, 132 | `#F47C84` |
| Paint | 120, 190, 255 | `#78BEFF` |
| Intersect | 250, 200, 110 | `#FAC86E` |

## Interaction and layout

- Keep the dense desktop CAD workspace organized around the viewport, scene list, and property panels.
- Preserve mouse and keyboard access, visible selected/focused states, and the distinct Add/Cut/Paint/Intersect colors.
- Current UI animations use 100–120 ms timing. Prefer restrained movement and request repaints only while an animation is active.
- Match the existing icon system in `apps/meltcad/src/glyphs.rs`; do not introduce emoji or third-party CAD assets.
- Treat this as a record of current implementation, not approval for a redesign. Change tokens only alongside an intentional UI change and its visual review.

## Using the design skills

Ask UI UX Pro Max for relevant UX principles or desktop-app comparisons, and Impeccable for a human-readable critique. Their documented stacks and automated detector do not directly target egui; verify each suggestion against the Rust implementation and this baseline.
