<!-- wildforge:guide -->

# Versioned test inputs

Small authored fixtures make regression scenarios reproducible without depending on private player worlds.

Start with the documented child directories. See the [repository overview](../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep fixture intent documented. Do not weaken a fixture to hide a production defect or commit personal saves/identities.

## Focused checks

```sh
python3 tools/run_gameplay_proofs.py
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
