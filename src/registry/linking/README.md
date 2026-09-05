<!-- wildforge:guide -->

# Content interpretation and linking

Material, observation/discovery, and magic/ecology interpreters validate raw
fields and resolve qualified names before runtime publication. They return
existing definitions and errors without scheduling work or mutating game state.
`register.rs` owns per-provider block/item registration, texture slot
allocation, and deferred references. It calls block, item, and reference stages
in the original order. `pending.rs` names the deferred content and appearance
records used by later resolution. The remaining name-resolution passes are still
being migrated from registry.rs.

Read [AGENTS.md](AGENTS.md). Preserve provider order, name qualification, ID
assignment, diagnostics, and default values. Final checks include malformed and
valid content, stable registry IDs/hashes, and atomic reload/remap behavior.
