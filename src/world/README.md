<!-- wildforge:guide -->

# Authoritative world domains

World owns spatial state, persistence, ecology, environment, structures, machines, and magic integration. The migration extracts domain state with its behavior.

Start with `alchemy.rs`, `belt.rs`, `calendar.rs`, `chunks.rs`, `discovery.rs`, `dross.rs`, `dungeon.rs`, `ecology.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Preserve block-edit fan-out, side-effect order, conservation ledgers, and save compatibility. Guests must never acquire authoritative generation/persistence capabilities.

## Focused checks

```sh
cargo test --locked tests::world::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
