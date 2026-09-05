<!-- wildforge:guide -->

# Immutable geography

The parent Geography owner contains seeded climate fields, relief splines, an
optional immutable atlas, and a derived province-label cache. climate.rs supplies
climate/biome classification, provinces.rs owns canonical province queries and
cache population, and relief.rs computes terrain offsets from those inputs.

Generation and replica observations use the same algorithms. Geography has no
registry bindings, generated chunk output, save reader/writer, scheduling, or
simulation. Generator keeps its public query API through ../queries.rs.

Read [AGENTS.md](AGENTS.md). Final deterministic worldgen/atlas fixtures must
compare seeds, poles/seams, province ties, cache order, and worker counts using
`cargo test --locked tests::worldgen::` and the full repository gates.

`deposits.rs` and `prospecting.rs` compute seeded geological observations from
immutable fields. Generator forwards these queries and consumes the same fields
for materialization; replicas can inspect prospecting without acquiring chunk
generation. Legacy-only prospecting probes remain in the cfg(test) adapter.
