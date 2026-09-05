<!-- wildforge:guide -->

# Working in src/registry

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

runtime.rs supplements registry.rs with runtime definition and lookup behavior. The migration will separate raw definitions, linking, validation, and publication.

Preserve stable names and deterministic ID/remap behavior. Reject invalid linked content before publishing a new runtime registry.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::registry_tests::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
