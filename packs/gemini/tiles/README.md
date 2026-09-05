<!-- wildforge:guide -->

# Gemini generated tiles

PNG files here provide the embedded Gemini atlas tiles. build.rs collects PNG inputs, while src/atlas assigns them to runtime atlas slots.

Preserve asset dimensions, alpha coverage, and slot names. Regenerate related tiles together from documented inputs when their generator changes, and verify the resulting atlas on the GPU.

Run `cargo test --locked tests::rendering::` from the repository root. See [AGENTS.md](AGENTS.md) for working instructions.
