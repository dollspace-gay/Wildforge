<!-- wildforge:guide -->

# Deterministic geological genesis

The parent defines the persisted model and chooses the first accepted bounded
attempt. It keeps rejected-attempt evidence and validates the chosen cell layers
before returning them. `attempt.rs` retains the original generation/constraint
sequence, including the existing compact-fixture acceptance policy.

`geometry.rs` owns spherical site math; `plates.rs` and `tectonics.rs` build plate,
craton, and boundary fields. `volcanism.rs` places volcanic/intrusive sites.
`relief.rs`, `strata.rs`, and `provinces.rs` shape terrain and rock history.
`deposits.rs` assigns finite resource sites and extraction envelopes. `kinds.rs`,
`minerals.rs`, and `records.rs` define codec-stable data. `validation.rs` checks
manifest consistency; `sampling.rs` exposes read-only queries to chunk generation.

Read [AGENTS.md](AGENTS.md). Final checks compare pinned atlas bytes/hashes,
rejection histories, stable site IDs/budgets, boundary rotation/seams, and
read/write compatibility. Chunk generation must not reroll these decisions.
