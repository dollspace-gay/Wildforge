<!-- wildforge:guide -->

# Headless game agent

MCP exposes ordinary guest perception, movement, and work. motion.rs follows walkable terrain; work.rs performs host-authorized actions.

Start with `mcp.rs`, `mod.rs`, `motion.rs`, `perception.rs`, `work.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

The agent is an ordinary guest. Preserve entry readiness, terrain clearance, authoritative inventory, and honest timeout results. Do not add privileged game shortcuts.

## Focused checks

```sh
cargo test --locked tests::agent:: -- --test-threads=1
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
