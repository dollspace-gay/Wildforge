<!-- wildforge:guide -->

# Strata motion sequence

phases.csv records the strata camera/motion sequence used by the visual closeout.

Start with `phases.csv`. See the [repository overview](../../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Preserve the ordered route and compare on the actual GPU. Document lighting and camera differences in new evidence.

## Focused checks

```sh
python3 tools/verify_visual_closeout.py --help
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
