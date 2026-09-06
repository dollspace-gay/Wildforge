<!-- wildforge:guide -->

# Tooling regression tests

Tests exercise source selection, scanner semantics, reports, and command exit behavior using disposable repositories.

Start with `test_maintainability.py`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep tests independent of personal files and installed game content. Test malformed input and incomplete scans as well as successful reports.

## Focused checks

```sh
python3 -m unittest discover -s tools/tests -v
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
