<!-- wildforge:guide -->

# Chunk adoption

`adoption.rs` chooses resident, stored, or prepared terrain and retains the
single ordered adoption coordinator. Stored bytes and matching registry/palette/
revision context precede speculative generation. Read failures remain failures.

Ecology/dross reconciliation, fresh water commit, material retrogen, physical
structure placement and loot each have a named responsibility. Their execution
order remains visible in adoption: generated structures precede final physical
material reservation; saved ecology and player modifications retain precedence.
These modules coordinate existing owners and do not create new conserved books.

See [world transaction order](../transaction-order.md) for cross-domain fan-out.
Run world/generation/streaming, save compatibility and conservation tests, then
the root Rust gates and final native/GPU qualification from the repository root.
