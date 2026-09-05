<!-- wildforge:guide -->

# Working in src/planet_atlas

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Climate, geology, hydrology, biomes, water cycles, and diagnostics operate on the immutable atlas and its explicit dynamic state.

Preserve deterministic seed derivation, face topology, units, schema versions, and water/salt/material conservation. Diagnostics must read actual state.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::atlas::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
