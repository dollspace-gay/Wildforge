<!-- wildforge:guide -->

# Pinned planetary atlas diagnostics

Seed-1337 generation-v9 tables, maps, and planet metadata are historical diagnostic evidence for the finite planet.

Start with `biomes.toml`, `census.csv`, `census.toml`, `climate-transects.csv`, `country-adjacency.csv`, `geology.toml`, `hydrology.toml`, `manifest.toml`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Do not silently regenerate this pinned dataset with a new algorithm. Record a new versioned dataset when the generating contract changes.

## Focused checks

```sh
cargo test --locked tests::atlas::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
