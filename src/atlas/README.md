<!-- wildforge:guide -->

# Texture atlas and packs

Atlas assembly, pack overrides, seasonal variants, and procedural fallback art live here. Planet geography is in planet_atlas/.

Start with `mod.rs`, `packs.rs`, `procedural.rs`, `season.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep texture indices and material channels consistent with registry and shader consumers. Presentation changes must not alter authoritative randomness.

## Focused checks

```sh
cargo test --locked tests::rendering::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
