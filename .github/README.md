<!-- wildforge:guide -->

# Repository automation

GitHub automation is organized under workflows/. Each job should expose a concrete verification result.

Start with the documented child directories. See the [repository overview](../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Preserve read-only default permissions and the separation between subsystem, agent, MSRV, and release gates.

## Focused checks

```sh
actionlint .github/workflows/ci.yml
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
