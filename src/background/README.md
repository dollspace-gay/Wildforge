<!-- wildforge:guide -->

# Owned background execution

This module supplies native thread startup, panic-to-error reporting, and a
bounded FIFO owner for immutable snapshot work. Mesh construction and wire
encoding share the snapshot lifecycle; terrain retains its specialized
entry-priority queue and uses the same thread failure boundary.

The module depends only on the standard library. It never reads World, applies
simulation changes, draws, or sends network messages. Adapters own interest,
identity/revision validation, and delivery. Shutdown cancels queued requests,
joins running work, and discards results before returning.

Read [AGENTS.md](AGENTS.md). Run `cargo test --locked background::` and the
terrain, multiplayer, and agent scenarios when changing this boundary.
