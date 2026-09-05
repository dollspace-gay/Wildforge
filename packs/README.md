<!-- wildforge:guide -->

# Texture pack sources

Each child directory is a named texture pack with pack.toml and optional PNG overrides. src/atlas/packs.rs resolves inheritance and relief parameters; these assets change presentation without defining gameplay content.

Keep pack IDs, inheritance, and base tile names stable. Check missing overrides and cycles through the atlas tests, then compare rendered captures on the GPU. Do not modify registry content to compensate for artwork.

Run `cargo test --locked tests::rendering::` from the repository root. See [AGENTS.md](AGENTS.md) for working instructions.
