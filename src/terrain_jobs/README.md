<!-- wildforge:guide -->

# Shared terrain preparation

`TerrainJobs` owns the preparation queue and ready results for one world
session. `TerrainContext` supplies immutable generation and save-reader inputs.
`queue.rs` preserves FIFO ordinary work and dedicated entry promotion/preemption;
`policy.rs` records the existing interactive/dedicated worker limits;
`workers.rs` loads saved chunks or runs the deterministic generator;
`result.rs` carries saved/generated provenance and a unique session identity.

Callers retain their existing adoption time/count budgets. Workers do not
mutate `World`; the caller adopts `PreparedChunk` through the authoritative
`World::adopt_prepared` operation. Wire encoding and GPU meshing are separate
consumers. Loaded/generated provenance must travel with the prepared result.

Run `cargo test --locked terrain_jobs::` for queue/policy tests, then the
worldgen, multiplayer, and serial agent suites for caller integration. The
migration record tracks pending lifetime/error improvements and runtime proof.

Shutdown stops new requests, cancels queued work, joins running workers, and
discards completions. Drop follows the same path and reports worker failure.
Running preparation finishes before shutdown returns; it cannot mutate a world.
Old-session results never release current queue capacity. Save-read failure
classification and live startup/panic propagation remain tracked migration work.
