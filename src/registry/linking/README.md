<!-- wildforge:guide -->

# Content interpretation and linking

Material, observation/discovery, and magic/ecology interpreters validate raw
fields and resolve qualified names before runtime publication. They return
existing definitions and errors without scheduling work or mutating game state.
`register.rs` owns per-provider block/item registration, texture slot
allocation, and deferred references. It calls block, item, and reference stages
in the original order. `pending.rs` names the deferred content and appearance
records used by later resolution. `mod.rs` coordinates these passes explicitly:

- `bootstrap` installs engine sentinels around provider registration.
- `shells` links resonance, working, site, and preparation contracts.
- `lookups` owns qualified item, block, ingredient, and piece lookup rules.
- `resources` resolves tags, rewards, aliases, placement, and held block forms.
- `structures` and `settlements` link templates and validate tier reachability.
- `stations` and `crafting` resolve transformation inputs and recipe gates.
- `fauna`, `npcs`, and `narrative` resolve species, companions, and story content.
- `features`, `modes`, and `extensions` link generation and optional capabilities.

The coordinator preserves ID assignment and resolution order. Error vectors
that must survive material-graph rebuilding join the registry afterward.

Read [AGENTS.md](AGENTS.md). Preserve provider order, name qualification, ID
assignment, diagnostics, and default values. Final checks include malformed and
valid content, stable registry IDs/hashes, and atomic reload/remap behavior.
