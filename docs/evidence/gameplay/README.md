<!-- wildforge:guide -->

# Gameplay regression evidence

Before/after logs document client, simulation, transfer, and dungeon regressions from the prior gameplay repair.

Start with `after-client.txt`, `after-sim.txt`, `before-belt.txt`, `before-client.txt`, `before-dungeon.txt`, `before-guest.txt`, `ci-format-refresh.json`, `validation.txt`. See the [repository overview](../../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Preserve the distinction between fixtures and production play. Add new evidence with its own scope and command rather than editing historical output.

## Focused checks

```sh
cargo test --locked tests::gameplay::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
