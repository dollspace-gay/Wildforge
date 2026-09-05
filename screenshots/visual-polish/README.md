<!-- wildforge:guide -->

# Visual-polish reports

Report TOML files capture measured visual and runtime outcomes for the staged polish scenarios.

Start with `closeout-geode-aperture-proof.report.toml`, `closeout-geode-composition.report.toml`, `closeout-geode-cracked-hero.report.toml`, `closeout-geode-performance-opened-a.report.toml`, `closeout-geode-performance-opened-b.report.toml`, `closeout-geode-performance-opened-c.report.toml`, `closeout-geode-performance-opened-d.report.toml`, `closeout-geode-performance-opened-e.report.toml`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Preserve capture identity, hardware, timings, and acceptance scope. Structural refactors require fresh evidence when source identity changes.

## Focused checks

```sh
python3 tools/verify_visual_polish.py --help
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
