<!-- wildforge:guide -->

# Hydrology scenarios

Shared fixture helpers stay in the parent `hydrology.rs`; scenario modules group observable
contracts by operation. Run `cargo test --locked tests::hydrology::` from the repository root.

Keep physical identity, conservation, rejection, recovery, and compatibility
assertions intact when extending a scenario. Read [AGENTS.md](AGENTS.md).
