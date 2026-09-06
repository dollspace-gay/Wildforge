<!-- wildforge:guide -->

# Working in frame preparation

Read [README.md](README.md) and [the parent guidance](../AGENTS.md).
Keep simulation timing separate from renderer-facing observations and cosmetic
emission. Do not change formulas, effect order, random streams, geometry winding,
or authored camera behavior while extracting a responsibility.

Preparation result types contain owned values or geometry, not a mutable World.
Preserve upload/first-frame/capture milestones and resize/error handling. CPU or
old-head screenshots do not qualify GPU changes. Run the repository Rust gates,
native gameplay proofs, and fresh hardware qualification after implementation.
Aim for 400 lines, review above 500, and retain useful authored geometry comments.
