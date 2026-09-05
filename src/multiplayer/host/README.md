<!-- wildforge:guide -->

# Host streaming

streaming.rs schedules guest terrain delivery, wire encoding, snapshots, and residency around active and pending players.

Start with `streaming.rs`. See the [repository overview](../../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Maintain bounded work and entry priority. Only the simulation thread adopts terrain. Preserve revisions so stale encoded chunks cannot replace current edits.

## Focused checks

```sh
cargo test --locked tests::multiplayer::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
