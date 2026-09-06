<!-- wildforge:guide -->

# Machines scenarios

Shared fixture helpers stay in the parent `machines.rs`; scenario modules group observable
contracts by operation. Run `cargo test --locked tests::machines::` from the repository root.

Keep physical identity, conservation, rejection, recovery, and compatibility
assertions intact when extending a scenario. Read [AGENTS.md](AGENTS.md).
