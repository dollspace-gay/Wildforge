<!-- wildforge:guide -->

# Static planetary hydrology

The parent owns the generation sequence and persisted hydrology model. It
coordinates bounded erosion, supported basin shaping, ocean labeling, drainage,
lake outlet resolution, watersheds, channel geometry, dense-cell accounting,
and final consistency checks before publishing a completed output.

`flood.rs` and `flow.rs` preserve seam-aware adjacency and ordered graph traversal.
`runoff.rs` partitions climate-normal water; `erosion.rs` and `basin_shape.rs`
update relief. `oceans.rs`, `lake_candidates.rs`, and `lakes.rs` produce finite
reservoirs. `watersheds.rs`, `rivers.rs`, and `channels.rs` establish names,
paths, sediment, salinity, and habitat constraints. `records.rs` defines stable
classifications and reservoir records. `validation.rs` compares sparse records
with dense routing and water budgets. `sampling.rs` serves read-only generation
queries. `placers.rs` transfers finite deposit quotas along established drainage.

`basin_query.rs` supplies transaction-scoped basin-only reads. It caches only
atlas-cell classifications proven to match full hydrology sampling everywhere
in the cell; mixed river/basin identities retain the original pointwise sampler.
The cache borrows immutable atlas state and adds no persisted data.

Read [AGENTS.md](AGENTS.md). Final validation covers pinned atlas output, seam
routing, acyclic lake outlets, lake/ocean budgets, placer conservation, and
save/codec compatibility, followed by the applicable full Rust/runtime gates.
