<!-- wildforge:guide -->

# Wire protocol and transport

protocol.rs defines wire values; handshake.rs authenticates admission;
transport.rs owns hosting, framing, and discovery; client.rs owns the guest
endpoint, runtime, and stream tasks; `../content_files.rs` owns mod hashes and transfer inventories.

Guest snapshot reconstruction belongs to `client_session/`, which owns stream
generations across Welcome and reconnect. This directory retains snapshot wire
values and datagram batching; it does not depend on the client-session owner.

Guest teardown queues Bye after outstanding reliable messages, waits for its
writer to finish, and keeps the endpoint/runtime alive while QUIC drains. Each
network wait has a two-second deadline, and all owned stream tasks are joined
before runtime destruction. Deadline and task failures are reported.

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
