<!-- wildforge:guide -->

# Rendering scenarios

Shared fixture helpers stay in the parent `rendering.rs`; scenario modules group observable
contracts by operation. Run `cargo test --locked tests::rendering::` from the repository root.

Keep physical identity, conservation, rejection, recovery, and compatibility
assertions intact when extending a scenario. Read [AGENTS.md](AGENTS.md).

The named `qualification::shadow_guard_hardware` test owns synchronous execution
of the renderer's asynchronous GPU fixture. Run it explicitly with `--ignored`
on a Vulkan DiscreteGpu; ordinary CPU CI does not count it as device validation.
