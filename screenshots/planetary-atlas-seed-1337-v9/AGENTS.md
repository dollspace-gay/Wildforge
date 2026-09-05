<!-- wildforge:guide -->

# Working in screenshots/planetary-atlas-seed-1337-v9

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Seed-1337 generation-v9 tables, maps, and planet metadata are historical diagnostic evidence for the finite planet.

Do not silently regenerate this pinned dataset with a new algorithm. Record a new versioned dataset when the generating contract changes.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::atlas::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
