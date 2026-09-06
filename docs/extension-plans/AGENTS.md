<!-- wildforge:guide -->

# Working in docs/extension-plans

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Historical phase and capability briefs describe structures, machines, NPCs, combat, and mod-facing contracts.

Resolve current behavior from code and verified tests. Maintain stable content identifiers and explicitly record any superseding design.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`git diff --check` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
