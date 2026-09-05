<!-- wildforge:guide -->

# Shared guest session

This module owns protocol interpretation shared by graphical guests and agents.
`palette.rs` resolves host block/item IDs against one immutable registry and
converts inventory, armor, cursor, and loose-item snapshots. Its callers retain
control of cameras, interpolation, HUD state, perception, and movement.

The map retains the host's names across local content reload. Rebinding resolves
those same names against the replacement registry, including removed and
reinstalled definitions; the wire numbers remain owned by the host.

`snapshots.rs` owns the five independent stream receivers for a connection.
`assembly.rs` keeps one bounded incomplete generation per stream, rejects old,
duplicate, or inconsistent fragments, and understands the wrapping host counter.
Welcome replaces all receivers together. Packet geometry is checked before a
new generation can discard older incomplete work. A single-packet snapshot uses
the same freshness rule as fragmented data.

`admission.rs` owns preparation, manifest validation, terrain readiness, the
one-time acknowledgement, host acceptance, idle timeout, and closure. The agent
requires decoded terrain; graphics additionally supplies its first-frame
milestone. Residency is acknowledged only after the real chunk decoder inserts
the chunk. Welcome starts a new admission epoch, and invalid transitions close it.

Replica ownership and content renegotiation are still being migrated from the
two adapters under the maintainability plan. Guests receive authoritative data;
this layer must not acquire generation or persistence capabilities.

Run `cargo test --locked client_session::` for focused checks and all serial
agent scenarios for protocol integration. Graphical session changes also require
native guest evidence. Read [AGENTS.md](AGENTS.md) before changing this module.
