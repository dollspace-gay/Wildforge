<!-- wildforge:guide -->

# Authoritative world domains

World owns spatial state, persistence, ecology, environment, structures, machines, and magic integration. The migration extracts domain state with its behavior.

Start with `alchemy.rs`, `belt.rs`, `calendar.rs`, `chunks.rs`, `discovery.rs`, `dross.rs`, `dungeon.rs`, `ecology.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Preserve block-edit fan-out, side-effect order, conservation ledgers, and save compatibility. Guests must never acquire authoritative generation/persistence capabilities.

`preparation.rs` runs immutable homeland trials through the shared bounded worker
owner. Worker count, per-worker generator context, and sorted adoption order are
preserved. Cancellable opening/spawn preparation checks between read/generation
steps; once durable homeland adoption starts, it completes before acknowledging
cancellation. `try_ensure_chunk` preserves read errors for entry callers while
ordinary simulation callers retain `ensure_chunk`'s existing boolean interface.

## Focused checks

```sh
cargo test --locked tests::world::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.

`TerrainRead` exposes only resident voxel/metadata/light queries and the immutable
registry; `SceneRead` adds local structure selection. Physics, item motion,
raycasts, camera collision, and mesh capture consume these contracts. World
retains its existing facade while forwarding common observations to that code.

`terrain.rs` owns the private resident map, dirty-mesh bookkeeping, light solve,
and WFC wire reconstruction. Adoption and persistence still coordinate through
World; the spatial owner has no generator, save writer, or conservation ledger.
This is the shared storage seam used by the separate replica migration.
