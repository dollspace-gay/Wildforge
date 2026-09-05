<!-- wildforge:guide -->

# Finite planetary systems

Climate, geology, hydrology, biomes, water cycles, and diagnostics operate on the immutable atlas and its explicit dynamic state.

Start with `biomes.rs`, `climate.rs`, `diagnostics.rs`, `geology.rs`, `hydrology.rs`, `water_cycle.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Preserve deterministic seed derivation, face topology, units, schema versions, and water/salt/material conservation. Diagnostics must read actual state.

## Focused checks

```sh
cargo test --locked tests::atlas::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
