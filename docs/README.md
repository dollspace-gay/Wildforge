<!-- wildforge:guide -->

# Designs and operational documentation

Feature plans, operating instructions, qualification records, and reports explain the game and its history. The active maintainability plan is in .design/.

Start with `agent-mcp-operations.md`, `agent-mcp-plan.md`, `agent-playtest-fixes.md`, `animals-plan.md`, `atproto-oauth-client-metadata.json`, `bows-armor-plan.md`, `bronze-age-plan.md`, `browser-creative-plan.md`. See the [repository overview](../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Distinguish intended design from verified current behavior. Preserve dated evidence; record new outcomes instead of rewriting old results.

## Focused checks

```sh
git diff --check
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
