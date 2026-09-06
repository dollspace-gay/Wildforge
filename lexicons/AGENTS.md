<!-- wildforge:guide -->

# Working in lexicons

Read [README.md](README.md) and [the root instructions](../AGENTS.md).

The ATProto device lexicon defines the external device-record shape used by identity integration.

Treat schema names and field meaning as external contracts. Validate identity and serialization behavior when changing a schema.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::identity::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
