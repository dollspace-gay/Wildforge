<!-- wildforge:guide -->

# Dusk tile overrides

These PNG images override existing atlas slots for the dusk pack. The parent pack.toml owns metadata; images retain the same content meaning as the original tile.

Use existing atlas slot names and preserve intended alpha cutouts. Keep new artwork here only when it belongs to this palette; shared changes belong with the common source or generator.

Run `cargo test --locked tests::rendering::` from the repository root. See [AGENTS.md](AGENTS.md) for working instructions.
