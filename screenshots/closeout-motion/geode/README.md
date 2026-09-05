<!-- wildforge:guide -->

# Geode motion sequence

phases.csv records the cracked-geode camera/motion phases for visual qualification.

Start with `phases.csv`. See the [repository overview](../../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep phases tied to the geode scenario and its capture provenance. A static image is not evidence for a motion requirement.

## Focused checks

```sh
python3 tools/verify_cracked_geode.py --help
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
