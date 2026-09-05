<!-- wildforge:guide -->

# Engine extension specifications

Historical phase and capability briefs describe structures, machines, NPCs, combat, and mod-facing contracts.

Start with `wildforge-engine-extensions-spec.md`, `wildforge-phase5-amendment.md`, `wildforge-task-brief-capability-e0-mod-harness.md`, `wildforge-task-brief-capability-e1-ruleset.md`, `wildforge-task-brief-capability-e10-instanced-dungeon-zones.md`, `wildforge-task-brief-capability-e11-mod-extensible-screens.md`, `wildforge-task-brief-capability-e12-industrial-response-gradient.md`, `wildforge-task-brief-capability-e2-third-person-camera.md`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Resolve current behavior from code and verified tests. Maintain stable content identifiers and explicitly record any superseding design.

## Focused checks

```sh
git diff --check
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
