<!-- wildforge:guide -->

# NPC runtime

NPC state links authored behavior to authoritative mobs and world progression.

Start with `mod.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Preserve stable NPC/mob identities and quest/reputation outcomes. Keep presentation and dialogue widgets outside the runtime owner.

## Focused checks

```sh
cargo test --locked tests::registry_tests::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
