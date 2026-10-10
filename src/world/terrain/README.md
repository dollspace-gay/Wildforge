<!-- wildforge:guide -->

# Resident terrain

TerrainStore owns loaded chunk planes, mesh dirtiness, and derived light. It
has no generator, save directory, palette writer, conservation ledger, or live
simulation. Authoritative adoption/persistence stays in the world coordinator;
replicas can reuse the resident storage and wire decoder without those powers.

lighting.rs preserves the sky/RGB propagation order and bounded seam cascade.
Each cascade builds a private optical palette from the current registry and
discards it afterward. `lighting_input.rs` prepares the optical cells, initial
sky grid, and sky frontier from block runs in physical index order. One cascade
reuses those immutable inputs within a 64 MiB allocated-capacity budget; inputs
that do not fit are recomputed normally. That bound covers cached vector/deque
storage, excluding map metadata, allocator overhead, solve buffers, and one
current uncached input. Every revisit still samples current neighbor light.
RGB work is skipped only when actual resident emission
is zero and every current border color is below the seed threshold. Chunk's
uniform-RGB replacement avoids an expanded black array while comparing the
complete sky field and clearing stale dense color; TerrainStore marks actual
changes dirty without changing the save flag. `lighting_tests.rs`
checks complete fields, level-one emitters, current border colors, mutable
definitions between calls, and invalid-ID panic behavior without resident writes.
It also checks complete fields through budget overflow and fresh AIR definitions.
Uniform-RGB scenarios cover final-cell stale color, unchanged dense storage and
flags, and full nonuniform sky replacement.
network.rs decodes existing WFC6-WFC9 payloads: settled wire light is retained,
legacy batches share one fallback cascade, and neighboring meshes become dirty.
The map is private; world-domain coordinators receive explicit chunk operations.

Read [AGENTS.md](AGENTS.md). Final checks cover light removal across seams, wire
fixtures/remapping/rejection, mesh dirtiness, and actual hardware captures with
`cargo test --locked tests::rendering::` and the full repository gates.

`edits.rs` owns resident plane writes, edge dirtiness, and the shared support
classification. Coordinators retain consequence order: authority accounts for
drops and domain changes, while replicas invalidate their received state and
settle derived lighting after the full ordered update group.
