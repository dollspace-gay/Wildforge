<!-- wildforge:guide -->

# Cargo environment

Portable compiler configuration. The pinned toolchain and package MSRV live at the repository root.

Start with `config.toml`. See the [repository overview](../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep local cache paths and machine credentials out of tracked configuration. Check Linux and Windows assumptions before changing target settings.

## Focused checks

```sh
cargo +1.95.0 check --locked --all-targets
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
