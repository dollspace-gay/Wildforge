<!-- wildforge:guide -->

# Pinned planet manifest

manifest.toml records the identity and version of the atlas diagnostic dataset in the parent directory.

Start with `manifest.toml`. See the [repository overview](../../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Treat the manifest as historical provenance. Do not alter hashes or versions to make newer output look like the original dataset.

## Focused checks

```sh
git diff --check
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
