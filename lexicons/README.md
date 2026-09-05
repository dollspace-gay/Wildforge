<!-- wildforge:guide -->

# Identity schemas

The ATProto device lexicon defines the external device-record shape used by identity integration.

Start with `gay.dollspace.wildforge.device.json`. See the [repository overview](../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Treat schema names and field meaning as external contracts. Validate identity and serialization behavior when changing a schema.

## Focused checks

```sh
cargo test --locked tests::identity::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
