<!-- wildforge:guide -->

# Content file ownership

The parent module preserves historical mod inventory and content hashing.
`snapshot.rs` owns a private copy of transferred content files. Runtime registry
clones retain a shared owner so replacing the ordinary cache cannot alter a live
reader's assets. The last reader releases only its exclusively created directory.

Copies are prepared before cache publication. Colliding/stale directories are
never adopted or deleted. A failed copy removes its own partial directory;
process-crash leftovers remain available for deliberate recovery/cleanup.
The snapshot path does not enter legacy content identity.

Read [AGENTS.md](AGENTS.md). Final checks cover overlapping registry readers,
clone lifetimes, failed publication/copy, identity compatibility, and the root
Rust gates.
