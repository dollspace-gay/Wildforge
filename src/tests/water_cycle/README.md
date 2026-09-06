<!-- wildforge:guide -->

# Water_cycle scenarios

Shared fixture helpers stay in the parent `water_cycle.rs`; scenario modules group observable
contracts by operation. Run `cargo test --locked tests::water_cycle::` from the repository root.

Keep physical identity, conservation, rejection, recovery, and compatibility
assertions intact when extending a scenario. Read [AGENTS.md](AGENTS.md).
