<!-- wildforge:guide -->

# Verification evidence

Small checked-in records connect defects and fixes to reproducible observations. gameplay/ contains the earlier gameplay sweep evidence.

Start with the documented child directories. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Record commands, revisions, environment, and the observed result. Never replace failing evidence merely to make a gate appear green.

## Focused checks

```sh
git diff --check
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
