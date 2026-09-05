<!-- wildforge:guide -->

# Working in src/multiplayer/host

Read [README.md](README.md) and [the root instructions](../../../AGENTS.md).

streaming.rs schedules guest interest and snapshot delivery. chunk_jobs.rs owns terrain preparation, wire encoding, revisions, and cache state behind bounded methods.

Maintain bounded work and entry priority. Only the simulation thread adopts terrain. Preserve revisions so stale encoded chunks cannot replace current edits.

Keep read-failure and fatal-worker mapping in streaming_errors.rs. Retain fatal
state so later arrivals cannot wait forever on a failed pool. Stop both terrain
and encoding before joining; preserve encoder backpressure and release cancelled
deduplication keys. Exercise actual guest admission after injected worker failure.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::multiplayer::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
