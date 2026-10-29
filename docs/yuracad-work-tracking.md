# YuraCAD work tracking

Track every unit of work as a GitHub Issue (template: "YuraCAD work item") on one GitHub Project board.
Do not keep status in shared files that several chats edit: that is what causes merge conflicts.

Board columns, in order: **In progress → PR open → Waiting for tests → Waiting to integrate → Done**.
Fields per item: owning PR, planned files, priority (P0–P3), not-yet-verified items.
Rules for agents are in `apps/meltcad/AGENTS.md`.
