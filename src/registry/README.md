<!-- wildforge:guide -->

# Runtime content linkage

Domain definition modules cover blocks, items, material/magic contracts, fauna,
narrative content, recipes, and structures. The parent re-exports their existing
names and owns raw loading/linking during the remaining migration.

`material_graph.rs` owns fixed-point material inference and balance validation;
`salvage.rs` derives physical recovery chains; arcane/ecology validators enforce
their conserved content contracts. Their call order remains explicit.
`runtime.rs` holds lookups, `policy.rs` resolves runtime content policy, and
`placeholders.rs` restores saved content names/payloads before remapping.
`schema/` owns raw documents, their defaults, and the parsed provider bundle.
`loading.rs` reads base/mod files and preserves provider ordering before calling
the linker. Embedded base paths still refer to the same authored files.
Linking stages and atomic publication remain in progress.

Start with `runtime.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Preserve stable names and deterministic ID/remap behavior. Reject invalid linked content before publishing a new runtime registry.

## Focused checks

```sh
cargo test --locked tests::registry_tests::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
