<!-- wildforge:guide -->

# Finite planetary water ownership

The parent defines the water-cycle state. `mass.rs` owns exact integer water
and salt parcels, including remainder-preserving transfer, evaporation, and
freezing. `records.rs` defines cells, aquifers, basin identifiers, commitments,
and ledger fields. `custody.rs` coordinates detailed debits/credits with coarse
reservoir accounting. `audit.rs` computes conserved totals, validates ownership,
and renders read-only reports. `genesis.rs` establishes the initial finite
reservoirs; `codec.rs` preserves the versioned checkpoint representation.

Weather chooses requested fluxes. These accounting operations determine which
mass can actually move. Read [AGENTS.md](AGENTS.md). Final checks cover exact
parcels, salt remainders, failed transactions, commitment reconciliation,
checkpoint compatibility/corruption, and full atlas/water/runtime gates.
