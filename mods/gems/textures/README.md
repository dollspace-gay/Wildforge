<!-- wildforge:guide -->

# Gemstone artwork

PNG textures here belong to the gems mod and are referenced by its content definitions. They participate in transferred mod data and content identity.

Preserve the filenames used by the parent TOML definitions and verify atlas loading after asset changes. New README.md and AGENTS.md files use the wildforge:guide marker; the exclusion never applies to PNG assets. Keep rendered evidence for intentional visual changes.

Run `cargo test --locked tests::registry_tests::` and `cargo test --locked net::content::` from the repository root. Read [AGENTS.md](AGENTS.md) before editing.
