<!-- wildforge:guide -->

# Working in src/multiplayer/host

Read [README.md](README.md) and [the root instructions](../../../AGENTS.md).

streaming.rs schedules guest interest and snapshot delivery. chunk_jobs.rs owns terrain preparation, wire encoding, revisions, and cache state behind bounded methods.

Maintain bounded work and entry priority. Only the simulation thread adopts terrain. Preserve revisions so stale encoded chunks cannot replace current edits.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::multiplayer::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
