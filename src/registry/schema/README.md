<!-- wildforge:guide -->

# Raw content schemas

These types deserialize mod documents before names and runtime IDs are linked.
Manifest, blocks, items, magic, fauna, recipes, structures, and narrative schemas
have separate modules. `RawMod` bundles parsed files for ordered linking.
Runtime definitions remain in the parent registry directory.

Preserve serde names, defaults, optional fields, and compatibility variants.
Schema fields/helpers are visible only inside the registry boundary. Read
[AGENTS.md](AGENTS.md). Verification includes valid and rejected pack fixtures,
mod lint, registry remapping, and unchanged linked content identities.
