<!-- wildforge:guide -->

# Owned background execution

This module supplies native thread startup, panic-to-error reporting, and a
bounded FIFO owner for immutable snapshot work. Mesh construction and wire
encoding share the snapshot lifecycle; terrain retains its specialized
entry-priority queue and uses the same thread failure boundary.

`Operation` owns a single cancellable thread, coalesces progress into one slot,
and joins before delivering its terminal result. The adapter decides how a
completed operation interacts with cancellation; successful durable publication
must remain visible. Dropping an unfinished operation cancels and joins it.
Snapshot initializers provide private per-worker context for homeland trials
without changing the shared queue, error, or shutdown policy.

The module depends only on the standard library. It never reads World, applies
simulation changes, draws, or sends network messages. Adapters own interest,
identity/revision validation, and delivery. Shutdown cancels queued requests,
joins running work, and discards results before returning.

Read [AGENTS.md](AGENTS.md). Run `cargo test --locked background::` and the
terrain, multiplayer, and agent scenarios when changing this boundary.
