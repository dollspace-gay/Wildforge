<!-- wildforge:guide -->

# Built-in texture tiles

PNG tiles supply the base atlas. build.rs embeds these images and src/atlas resolves their stable tile names; texture-pack overrides live under packs instead.

Keep image dimensions, alpha behavior, and filename references consistent with the atlas. Update authored assets deliberately and verify them on the real renderer. Markdown guides are not image inputs.

Run `cargo test --locked tests::rendering::` from the repository root. See [AGENTS.md](AGENTS.md) for working instructions.
