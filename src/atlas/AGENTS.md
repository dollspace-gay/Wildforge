<!-- wildforge:guide -->

# Working in src/atlas

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Atlas assembly, pack overrides, seasonal variants, and procedural fallback art live here. Planet geography is in planet_atlas/.

Keep texture indices and material channels consistent with registry and shader consumers. Presentation changes must not alter authoritative randomness.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::rendering::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
