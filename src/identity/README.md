<!-- wildforge:guide -->

# Player and device identity

Local keys, ATProto account integration, and OAuth conformance support authenticated host/guest identity.

Start with `atproto.rs`, `local.rs`, `oauth_conformance.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Never print private keys or tokens. Preserve key pinning, trust policy, signed records, and atomic writes. Use disposable identities in tests.

`handle_dns.rs` owns native TXT resolution using the operating system's
nameservers. Configuration reads run on a blocking worker. `atproto.rs` retains
the two-second handle-verification budget and bounded public HTTPS fallback;
a valid differing DNS DID rejects the handle immediately.

The dependency migration preserves Jacquard's existing TXT selection: each
character string is inspected separately and the first valid DID wins. Segmented
values remain separate. Conflicting valid records therefore follow their
response order, which differs from the [ATProto requirement to reject conflicting
DIDs](https://atproto.com/specs/handle#dns-txt-method). Correcting those existing
protocol behaviors requires a separate change.

## Focused checks

```sh
cargo test --locked tests::identity::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.
