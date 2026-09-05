<!-- wildforge:guide -->

# Working in packs/hewn/tiles/base

Read [README.md](README.md) and [the root instructions](../../../../AGENTS.md).

Preserve map dimensions, channel encoding, and texture orientation. Validate pairing with the atlas tests and inspect lighting on the real GPU after changing relief data.

Run `cargo test --locked tests::rendering::` from the repository root and the applicable full gates. Preserve unrelated assets. Add both local guides to any maintained subdirectory introduced here.
