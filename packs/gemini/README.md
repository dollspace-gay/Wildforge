<!-- wildforge:guide -->

# Gemini built-in pack

Gemini is the built-in generated texture pack. pack.toml records its identity and tiles contains the images embedded by build.rs for startup without an external asset installation.

Keep the embedded pack usable from a clean checkout and preserve tile IDs. Coordinate generated artwork with the scripts in tools and record regeneration inputs instead of hand-editing generated aggregates.

Run `cargo test --locked tests::rendering::` from the repository root. See [AGENTS.md](AGENTS.md) for working instructions.
