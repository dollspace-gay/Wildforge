<!-- wildforge:guide -->

# Built-in game content

TOML files define the base namespace: blocks, items, recipes, animals, structures, machines, and finite magic. The registry embeds these inputs and loads them before user mods. Textures live in the textures subdirectory.

Preserve stable names, material vectors, schema versions, and deterministic registration order. Content changes can alter signed genesis identity; architecture work must preserve the embedded bytes. Follow the format documented in ../mods/README.md.

Run `cargo test --locked tests::registry_tests::` from the repository root. See [AGENTS.md](AGENTS.md) for working instructions.
