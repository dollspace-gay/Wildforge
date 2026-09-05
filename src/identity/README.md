<!-- wildforge:guide -->

# Player and device identity

Local keys, ATProto account integration, and OAuth conformance support authenticated host/guest identity.

Start with `atproto.rs`, `local.rs`, `oauth_conformance.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Never print private keys or tokens. Preserve key pinning, trust policy, signed records, and atomic writes. Use disposable identities in tests.

## Focused checks

```sh
cargo test --locked tests::identity::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
