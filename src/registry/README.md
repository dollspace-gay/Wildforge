<!-- wildforge:guide -->

# Runtime content linkage

runtime.rs supplements registry.rs with runtime definition and lookup behavior. The migration will separate raw definitions, linking, validation, and publication.

Start with `runtime.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Preserve stable names and deterministic ID/remap behavior. Reject invalid linked content before publishing a new runtime registry.

## Focused checks

```sh
cargo test --locked tests::registry_tests::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
