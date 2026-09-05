<!-- wildforge:guide -->

# Base namespace relief

Height and normal PNG maps here augment the base namespace for the Hewn pack. Their names bind to existing built-in atlas tiles rather than creating new blocks or items.

Preserve map dimensions, channel encoding, and texture orientation. Validate pairing with the atlas tests and inspect lighting on the real GPU after changing relief data.

Run `cargo test --locked tests::rendering::` from the repository root. See [AGENTS.md](AGENTS.md) for working instructions.
