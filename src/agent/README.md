<!-- wildforge:guide -->

# Headless game agent

MCP exposes ordinary guest perception, movement, and work. motion.rs follows walkable terrain; work.rs performs host-authorized actions.

Start with `mcp.rs`, `mod.rs`, `motion.rs`, `perception.rs`, `work.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

The agent is an ordinary guest. Preserve entry readiness, terrain clearance, authoritative inventory, and honest timeout results. Do not add privileged game shortcuts.

`client_session/` owns admission, host content mapping, entity reconstruction, and
ordered terrain application for both guest adapters. The agent selects decoded-terrain readiness; it does not
require a rendered frame. Keep breadcrumb trails, perception, navigation,
movement cadence, and MCP responses here; discard shared receiver state and
queued terrain when Welcome starts a new session.

## Focused checks

```sh
cargo test --locked tests::agent:: -- --test-threads=1
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.

The agent now owns `ReplicaWorld`, containing streamed terrain, entity snapshots,
and bounded host observations. It has no world-cache path, generator, save API,
or authority ledgers. Perception uses host arcane cues directly; immutable
climate sampling preserves weather fallback before host cells arrive.
