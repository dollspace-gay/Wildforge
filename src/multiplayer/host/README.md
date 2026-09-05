<!-- wildforge:guide -->

# Host session adapters

streaming.rs schedules guest interest and snapshot delivery. chunk_jobs.rs owns terrain preparation, wire encoding, revisions, and cache state behind bounded methods.

`streaming_errors.rs` translates retained read/worker failures into existing
server-error refusals. Terrain keeps its entry-priority queue; encoding uses
the shared FIFO snapshot owner with at most 32 queued/running/ready jobs.
Both queues stop before host shutdown joins either set of workers. Cancelled
encoding requests release their keys so later interest can retry them.

`HostChunkState` binds the working pool or startup failure to its requested
context. A context replacement joins the old terrain/encoding workers and drops
their cache and pending revisions together. Matching failures remain observable
without retrying each pump. This boundary protects prepared payloads; live guest
content negotiation remains the responsibility of session admission.

Start with `streaming.rs`. See the [repository overview](../../../README.md) for
build prerequisites and complete checks. Commands below run from the repository root.

## Maintenance boundaries

Maintain bounded work and entry priority. Only the simulation thread adopts terrain. Preserve revisions so stale encoded chunks cannot replace current edits.

## Focused checks

```sh
cargo test --locked tests::multiplayer::
```

Read [AGENTS.md](AGENTS.md) before changing this area. For a structural migration,
run the complete applicable gates and record evidence in the migration record.

`requests_*.rs` groups authenticated operations by domain. The parent on_msg
coordinator retains readiness, moderation, pre-entry request handling, observer
snapshot creation, and command-budget admission before its exhaustive protocol
dispatch. A domain adapter then borrows the admitted guest and calls the same
physical operations. Packet/variant definitions, reply order, counters, and
observer selection are unchanged. Shared gameplay rules live in player_ops;
these modules own protocol decoding and guest-specific responses.
