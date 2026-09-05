<!-- wildforge:guide -->

# Working in packs/gemini/tiles

Read [README.md](README.md) and [the root instructions](../../../AGENTS.md).

Preserve asset dimensions, alpha coverage, and slot names. Regenerate related tiles together from documented inputs when their generator changes, and verify the resulting atlas on the GPU.

Run `cargo test --locked tests::rendering::` from the repository root and the applicable full gates. Preserve unrelated assets. Add both local guides to any maintained subdirectory introduced here.
