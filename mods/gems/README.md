<!-- wildforge:guide -->

# Gemstones example mod

mod.toml declares the gems namespace and dependency on base. Its definitions demonstrate gemstone world generation, items, recipes, and textures using the normal mod loader.

Keep content IDs, finite material vectors, and dependency order stable. New guide files begin with the wildforge:guide HTML comment so documentation cannot alter the mod or signed genesis identity. Test content changes through registry and world-generation scenarios.

Run `cargo test --locked tests::registry_tests::` and `cargo test --locked net::content::` from the repository root. Read [AGENTS.md](AGENTS.md) before editing.
