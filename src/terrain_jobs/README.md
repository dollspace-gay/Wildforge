<!-- wildforge:guide -->

# Shared terrain preparation

`TerrainJobs` owns the preparation queue and ready results for one world
session. `TerrainContext` supplies immutable generation and save-reader inputs.
`queue.rs` preserves FIFO ordinary work and dedicated entry promotion/preemption;
`policy.rs` records the existing interactive/dedicated worker limits;
`workers.rs` loads saved chunks or runs the deterministic generator.

Callers retain their existing adoption time/count budgets. Workers do not
mutate `World`; the caller adopts `PreparedChunk` through the authoritative
`World::adopt_prepared` operation. Wire encoding and GPU meshing are separate
consumers. Loaded/generated provenance must travel with the prepared result.

Run `cargo test --locked terrain_jobs::` for queue/policy tests, then the
worldgen, multiplayer, and serial agent suites for caller integration. The
migration record tracks pending lifetime/error improvements and runtime proof.
