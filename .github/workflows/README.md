<!-- wildforge:guide -->

# CI workflows

ci.yml runs native Rust gates, dependency advisories, and the Python maintainability report.

Start with `ci.yml`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep cold-build time separate from scenario runtime. Findings in the maintainability report are advisory; analyzer failures must remain visible.

## Focused checks

```sh
actionlint .github/workflows/ci.yml
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
