<!-- wildforge:guide -->

# Resident terrain

TerrainStore owns loaded chunk planes, mesh dirtiness, and derived light. It
has no generator, save directory, palette writer, conservation ledger, or live
simulation. Authoritative adoption/persistence stays in the world coordinator;
replicas can reuse the resident storage and wire decoder without those powers.

lighting.rs preserves the sky/RGB propagation order and bounded seam cascade.
network.rs decodes existing WFC6-WFC9 payloads: settled wire light is retained,
legacy batches share one fallback cascade, and neighboring meshes become dirty.
The map is private; world-domain coordinators receive explicit chunk operations.

Read [AGENTS.md](AGENTS.md). Final checks cover light removal across seams, wire
fixtures/remapping/rejection, mesh dirtiness, and actual hardware captures with
`cargo test --locked tests::rendering::` and the full repository gates.
