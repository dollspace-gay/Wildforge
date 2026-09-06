<!-- wildforge:guide -->

# Climate generation and weather transactions

`normals.rs` generates immutable seasonal climate from terrain and geometry.
`solar.rs` owns astronomy, `circulation.rs` continental fetch/directions, and
`moisture.rs` bounded relaxation with convergence and moisture-budget results.
`transport.rs` shares the conservative advection stencil with dynamic weather.

The parent owns PlanetaryWeather, its old/scratch grids, cursor, checkpoint, and
failed-hour latch. A slice visits `hour_cell.rs` in the original index order;
only after all cells finish does `hour_commit.rs` apply inbound/surface transfers,
swap grids, settle terminal salts, advance daily groundwater, reconcile basin
levels, and check the conservation audit. A failed hour can restore its checkpoint.

`ecology.rs`, `custody.rs`, `industrial.rs`, and `basins.rs` own external water
exchange operations. `groundwater.rs` retains deterministic daily edge ordering.
`sampling.rs` and `weather_types.rs` expose read-only observations and reports.
The water-cycle state and ledger remain the owners of physical water/salt.

Read [AGENTS.md](AGENTS.md). Final checks compare pinned atlas bytes/hashes and
exercise slice budgets, abort/rollback, exact water/salt custody, and weather
sampling across seams. Structural extraction must not change physical constants.
