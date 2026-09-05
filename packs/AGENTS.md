<!-- wildforge:guide -->

# Working in packs

Read [README.md](README.md) and [the root instructions](../AGENTS.md).

Keep pack IDs, inheritance, and base tile names stable. Check missing overrides and cycles through the atlas tests, then compare rendered captures on the GPU. Do not modify registry content to compensate for artwork.

Run `cargo test --locked tests::rendering::` from the repository root and the applicable full gates. Preserve unrelated assets. Add both local guides to any maintained subdirectory introduced here.
