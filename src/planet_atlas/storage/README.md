<!-- wildforge:guide -->

# Planetary atlas persistence

The parent handles committed directory entry points, bounded file reads, and
manifest writes. `bundle.rs` writes new genesis bundles, `mutable.rs` checkpoints
atmosphere/water/history, and `load.rs` validates saved data and recovers paired
mutable backups. `arcane.rs` integrates ledger and geography checkpoint metadata.
The complete manifest remains the final commit marker.

Read [AGENTS.md](AGENTS.md). Final checks cover saved fixture compatibility,
corruption rejection, checksum and length validation, paired backup recovery,
and the applicable atlas, water-conservation, and full Rust/runtime gates.
