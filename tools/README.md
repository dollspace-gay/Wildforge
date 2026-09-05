<!-- wildforge:guide -->

# Development and qualification tools

Python tools generate authored assets, verify visual/gameplay evidence, install pinned test content, and report maintainability.

Start with `audit_tiles.py`, `check_maintainability.py`, `gen_base_tiles.py`, `gen_player_face.py`, `gen_texture_pack.py`, `heart_table.py`, `install_test_mods.py`, `magic_qualification_matrix.py`. See the [repository overview](../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Use explicit input/output paths and preserve user data. Keep shared verification rules in one owner and report incomplete scans/runs as failures.

`run_gameplay_proofs.py` runs real client interaction and guest-entry scenarios
on a hardware Vulkan adapter with disposable state. Its optional `--output`
argument retains the native screenshot, scenario report, exact test-binary hash,
and exit status in a new directory. Existing output directories are refused.

## Focused checks

```sh
python3 -m unittest discover -s tools/tests -v
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
