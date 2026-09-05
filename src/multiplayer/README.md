<!-- wildforge:guide -->

# Authoritative multiplayer adapter

Host requests, player profiles, roles, moderation, roster, and terrain delivery connect authenticated guests to shared simulation.

Start with `host.rs`, `moderation.rs`, `profiles.rs`, `roster.rs`, `settings.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Validate identity, permission, reach, and actor readiness before effects. Move duplicated gameplay rules into shared authoritative operations.

## Focused checks

```sh
cargo test --locked tests::multiplayer::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
