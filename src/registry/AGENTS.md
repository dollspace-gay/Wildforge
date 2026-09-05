<!-- wildforge:guide -->

# Working in src/registry

Read [README.md](README.md) and [the root instructions](../../AGENTS.md).

Keep domain definitions independent of raw deserialization and linking. Use
explicit imports across definition, material graph, salvage, validation,
runtime policy, and saved-placeholder modules. The parent preserves existing
re-exported names; raw loading/linking is still being migrated. Preserve the
order of material fixed-point inference, derived salvage, and validation.

Preserve stable names and deterministic ID/remap behavior. Reject invalid linked content before publishing a new runtime registry.

Aim for cohesive source modules around 400 lines and review those above 500.
Add local README/AGENTS guidance when introducing a subdirectory. Run
`cargo test --locked tests::registry_tests::` from the repository root for focused feedback, then the applicable
full gates before accepting a slice. Record limitations and failures honestly.
