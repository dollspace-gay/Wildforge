<!-- wildforge:guide -->

# Terrain persistence primitives

`reader.rs` owns immutable WFC chunk decoding and palette remapping. The
parent storage adapter supplies save-directory, registry, and palette context;
workers can clone that context without borrowing or mutating the live world.
`decoder.rs` owns bounded reads/RLE; `region.rs` owns region I/O and
`region_tests.rs` covers its format and failure preservation. The existing
`world::region` namespace is retained as a compatibility facade.

`region_store.rs` owns per-region read/write coordination and watched chunk
revisions for one World session. Its clones share locks through the immutable
loader. Prepared terrain retains an opaque revision; writes invalidate it before
touching disk, and authoritative adoption rejects stale results without disk I/O.
Production region reads, including spawn fingerprints, use this owner. Raw
region APIs remain for codec tests; they do not coordinate independent processes
or separately constructed World owners pointing at the same save directory.

The reader's prepared revision also retains its registry and immutable palette
snapshot. Authoritative adoption checks both that context and the region revision.
Refreshing a palette replaces its shared allocation; cloning a loader is cheap
and does not copy the remapping table or read the filesystem.

`palette.rs` parses bounded naming tables and appends new names without reusing
stored IDs. `palette_store.rs` owns publication and immutable registry mappings
for a World session. Full saves and direct chunk writes share that publication
boundary; failed writes retain the pending palette for retry. Failed initial
reads remain observable and require reopening the session after repair.

`encoder.rs` shares WFC plane encoding while keeping persistence's stored IDs
separate from WFC9 runtime IDs. A registry reorder leaves unedited disk chunks
clean. Missing/empty historical palettes retain the legacy identity fallback;
malformed and unreadable palettes are errors, never a regeneration request.

Preserve WFC6-WFC8 compatibility, water units, stable IDs, and modified/dirty
flags. Keep read errors distinct from missing content. The authoritative world
alone adopts decoded terrain and commits lighting, ecology, and accounting.

Run `cargo test --locked tests::world::` and `cargo test --locked terrain_jobs::`
from the repository root. Read [AGENTS.md](AGENTS.md) before editing and record
incomplete compatibility/runtime gates in the migration progress document.
