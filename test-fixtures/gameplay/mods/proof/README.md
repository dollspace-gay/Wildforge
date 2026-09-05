<!-- wildforge:guide -->

# Proof mod content

The proof mod supplies bounded animals, blocks, nests, and pieces for gameplay regression scenarios.

Start with `animals.toml`, `blocks.toml`, `mod.toml`, `nests.toml`, `pieces.toml`. See the [repository overview](../../../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep IDs and intended interactions aligned with the tests. Avoid introducing unrelated progression or random wildlife into the fixture.

## Focused checks

```sh
python3 tools/run_gameplay_proofs.py
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
