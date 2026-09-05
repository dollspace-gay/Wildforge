<!-- wildforge:guide -->

# Planetary atlas codecs

The binary modules encode immutable cells, atmosphere checkpoints, and versioned
containers. Primitives provide checked reads and explicit little-endian writes.
Sparse models retain their TOML representation; fingerprints retain the exact
field order used by stage evidence. Water-cycle records are encoded by `water_cycle/codec.rs`, which uses the same
checked binary primitives while retaining its domain schema.

Read [AGENTS.md](AGENTS.md). Final checks cover saved fixture compatibility,
corruption rejection, checksum and length validation, paired backup recovery,
and the applicable atlas, water-conservation, and full Rust/runtime gates.
