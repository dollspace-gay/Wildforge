<!-- wildforge:guide -->

# Working in docs/evidence/gameplay

Read [README.md](README.md) and [the root instructions](../../../AGENTS.md).

Before/after logs document client, simulation, transfer, and dungeon regressions from the prior gameplay repair.

Preserve the distinction between fixtures and production play. Add new evidence with its own scope and command rather than editing historical output.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::gameplay::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
