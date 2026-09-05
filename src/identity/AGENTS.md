<!-- wildforge:guide -->

# Working in src/identity

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Local keys, ATProto account integration, and OAuth conformance support authenticated host/guest identity.

Never print private keys or tokens. Preserve key pinning, trust policy, signed records, and atomic writes. Use disposable identities in tests.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::identity::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
