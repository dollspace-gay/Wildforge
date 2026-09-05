<!-- wildforge:guide -->

# Terrain persistence primitives

`reader.rs` owns immutable WFC chunk decoding and palette remapping. The
parent storage adapter supplies save-directory, registry, and palette context;
workers can clone that context without borrowing or mutating the live world.
`decoder.rs` owns bounded reads/RLE; `region.rs` owns region I/O and
`region_tests.rs` covers its format and failure preservation. The existing
`world::region` namespace is retained as a compatibility facade.

Preserve WFC6-WFC8 compatibility, water units, stable IDs, and modified/dirty
flags. Keep read errors distinct from missing content. The authoritative world
alone adopts decoded terrain and commits lighting, ecology, and accounting.

Run `cargo test --locked tests::world::` and `cargo test --locked terrain_jobs::`
from the repository root. Read [AGENTS.md](AGENTS.md) before editing and record
incomplete compatibility/runtime gates in the migration progress document.
