<!-- wildforge:guide -->

# Hewn relief pack

Hewn inherits Gemini artwork and adds authored height and normal relief. pack.toml owns material parameters, including the layered ice treatment; tiles contains the authored overrides.

Keep inheritance and relief channel conventions consistent with src/atlas/packs.rs and the shader. Judge changes with grazing-light captures as well as ordinary daylight views.

Run `cargo test --locked tests::rendering::` from the repository root. See [AGENTS.md](AGENTS.md) for working instructions.
