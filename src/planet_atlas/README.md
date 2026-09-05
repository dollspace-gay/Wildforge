<!-- wildforge:guide -->

# Finite planetary systems

Climate, geology, hydrology, biomes, water cycles, and diagnostics operate on the immutable atlas and its explicit dynamic state.

`climate/` separates astronomy, circulation, moisture relaxation, and immutable
normals from weather cell preparation, completed-hour publication, and external
water exchanges. The climate parent owns the weather grids and rollback state;
water/salt accounting stays in the existing water-cycle ledger. Weather scratch
state uses complete validated grids, so publication and rollback swap grid
owners without exposing resizable storage.

`geology/` separates stable records and classifications, plate/tectonic fields,
volcanism, relief, strata/provinces, finite deposits, validation, and queries.
Its parent retains the bounded deterministic attempt-selection coordinator.

`hydrology/` separates ordered flooding/flow, erosion, ocean and lake reservoirs,
watersheds, river/channel geometry, validation, sampling, and finite placers.
Its parent keeps the generation sequence and final dense-cell accounting.

`grid.rs` owns canonical addressing and private dense storage; `layers.rs`, `dynamic.rs`,
and `history.rs` define the layer schemas. `generation.rs` coordinates stages,
`sampling.rs` exposes bounded queries, and `validation.rs` checks complete state.
`manifest.rs` owns compatibility; `codec/` and `storage/` own serialization and
committed persistence. The parent retains the public atlas facade.

`water_cycle/` separates exact mass arithmetic, custody records/operations,
audits, initial finite reservoirs, and versioned checkpoint encoding.

`biomes/` separates zonal classification, soils, habitats, country nuclei and
borders, heart selection, persisted records, validation, and local queries.

`diagnostics/` separates census collection, ordered map definitions, value
readers, image/CSV exports, resource estimates, and qualification-site selectors.

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
