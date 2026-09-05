<!-- wildforge:guide -->

# Runtime content linkage

Domain definition modules cover blocks, items, material/magic contracts, fauna,
narrative content, recipes, and structures. The parent re-exports their existing
names and holds the runtime content graph.

`material_graph.rs` owns fixed-point material inference and balance validation;
`salvage.rs` derives physical recovery chains; arcane/ecology validators enforce
their conserved content contracts. Their call order remains explicit.
`runtime.rs` holds lookups, `policy.rs` resolves runtime content policy, and
`placeholders.rs` restores saved content names/payloads before remapping.
`schema/` owns raw documents, their defaults, and the parsed provider bundle.
`loading.rs` reads base/mod files and preserves provider ordering before calling
the linker. Embedded base paths still refer to the same authored files.
`linking/` owns the ordered registration and resolution coordinator. Domain
passes consume their deferred declarations; graph validation follows linking.
Existing schema scenarios live in adjacent test modules with their original
test paths. `publication.rs` combines provider, material, and arcane diagnostics
in one runtime gate and checks live material/quest migration constraints.
`load` exposes a diagnostic candidate for inspectors; runtime entry uses
`load_validated` or validates a retained candidate before opening a world.
`reading.rs` distinguishes absent optional files from failed reads.

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

`assets.rs` ties transferred asset snapshots to registry lifetime. Registry
clones share the storage owner; embedded base paths and the existing content
hash convention remain unchanged. Runtime readers never follow a replaced cache.
