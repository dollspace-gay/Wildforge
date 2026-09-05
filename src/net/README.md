<!-- wildforge:guide -->

# Wire protocol and transport

protocol.rs defines wire values; handshake.rs authenticates admission; transport.rs owns QUIC channels; content.rs owns mod hashes and transfer inventories.

Guest snapshot reconstruction belongs to `client_session/`, which owns stream
generations across Welcome and reconnect. This directory retains snapshot wire
values and datagram batching; it does not depend on the client-session owner.

Start with `handshake.rs`, `protocol.rs`, `transport.rs`. See the [repository overview](../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Keep transport separate from gameplay. Preserve protocol compatibility, snapshot bounds, host-key checks, and data/script boundaries.

## Focused checks

```sh
cargo test --locked net::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.

New folder guides opt out of mod identity only when named README.md or
AGENTS.md and beginning with `<!-- wildforge:guide -->`. Historical unmarked
documents remain content. Never broaden this rule to runtime data or change
legacy path hashing as part of a structural refactor.
