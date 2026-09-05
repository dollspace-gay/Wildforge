<!-- wildforge:guide -->

# Hewn relief assets

This directory groups namespace-specific relief assets. The base child contains height and normal maps for built-in tiles; the parent pack metadata controls how the renderer uses them.

Group assets by the namespace they override. Keep relief maps aligned with their color tile and avoid introducing gameplay definitions into this presentation-only directory.

Run `cargo test --locked tests::rendering::` from the repository root. See [AGENTS.md](AGENTS.md) for working instructions.
