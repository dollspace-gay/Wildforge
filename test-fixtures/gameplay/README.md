<!-- wildforge:guide -->

# Real client gameplay fixtures

The mods/ tree supplies deterministic content for actual graphical interaction proofs.

Start with the documented child directories. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep the fixture small and explicit about its differences from production. Graphical proofs need a real display and disposable runtime directory.

## Focused checks

```sh
python3 tools/run_gameplay_proofs.py
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
